// Inspector section for a selected text overlay -- the egui properties panel's text controls
// (`crates/ui/src/screens/editor/properties_panel/text_clip.rs`). Each change is one
// `update_text_clip` call, so one undo step; the preview engine reopens with the new overlay.

import { t, type TextKey } from "../i18n";
import { api } from "../ipc";
import { run } from "../store";
import type {
  FontCategory,
  TextAlign,
  TextClip,
  TextClipPatch,
  TextDirection,
  TextFontStyle,
} from "../types";
import { F32KeyframeList } from "./KeyframeEditor";
import { Section, Slider } from "./Inspector";
import { ColorField, SelectField, TextField, useOverlayCatalog } from "./OverlayFields";

const CATEGORY_LABELS: Record<FontCategory, TextKey> = {
  Sans: "fontCategorySans",
  Display: "fontCategoryDisplay",
  Serif: "fontCategorySerif",
  Handwritten: "fontCategoryHandwritten",
  Monospace: "fontCategoryMonospace",
  International: "fontCategoryInternational",
};

const ALIGN_LABELS: Record<TextAlign, TextKey> = {
  Auto: "alignAuto",
  Left: "alignLeft",
  Center: "alignCenter",
  Right: "alignRight",
  Start: "alignStart",
  End: "alignEnd",
};

const DIRECTION_LABELS: Record<TextDirection, TextKey> = {
  Auto: "directionAuto",
  Ltr: "directionLtr",
  Rtl: "directionRtl",
};

export function TextInspector({ clip }: { clip: TextClip }) {
  const catalog = useOverlayCatalog();
  const commit = (patch: TextClipPatch) => {
    return void run(api.updateTextClip(clip.id, patch));
  };
  const fonts = catalog?.fonts ?? [];
  const font = fonts.find((f) => {
    return f.id === clip.font_family;
  });
  const categories = [
    ...new Set(
      fonts.map((f) => {
        return f.category;
      }),
    ),
  ];
  const backgroundEnabled = clip.background_rgba[3] > 0;

  return (
    <>
      <div
        className="panel-title"
        style={{ border: 0, padding: 0 }}
      >
        {t("text")}
      </div>
      <TextField
        label={t("text")}
        value={clip.text}
        placeholder={t("textContentHint")}
        onCommit={(text) => {
          return commit({ text });
        }}
      />
      <SelectField
        label={t("font")}
        value={clip.font_family}
        disabled={!catalog}
        onChange={(fontFamily) => {
          return commit({ fontFamily });
        }}
      >
        {catalog && !font && <option value={clip.font_family}>{t("unknownFont")}</option>}
        {categories.map((category) => {
          return (
            <optgroup
              key={category}
              label={t(CATEGORY_LABELS[category])}
            >
              {fonts
                .filter((f) => {
                  return f.category === category;
                })
                .map((f) => {
                  return (
                    <option
                      key={f.id}
                      value={f.id}
                    >
                      {f.displayName}
                    </option>
                  );
                })}
            </optgroup>
          );
        })}
      </SelectField>
      <SelectField
        label={t("fontStyle")}
        value={clip.font_style}
        disabled={!font?.supportsBold}
        onChange={(style) => {
          return commit({ fontStyle: style as TextFontStyle });
        }}
      >
        <option value="Regular">{t("fontRegular")}</option>
        <option value="Bold">{t("fontBold")}</option>
      </SelectField>
      {font?.isVariable && (
        <Slider
          label={t("fontWeight")}
          value={clip.font_weight ?? 400}
          min={100}
          max={900}
          step={10}
          format={(v) => {
            return v.toFixed(0);
          }}
          onCommit={(fontWeight) => {
            return commit({ fontWeight });
          }}
        />
      )}
      <SelectField
        label={t("direction")}
        value={clip.direction}
        onChange={(direction) => {
          return commit({ direction: direction as TextDirection });
        }}
      >
        {(Object.keys(DIRECTION_LABELS) as TextDirection[]).map((direction) => {
          return (
            <option
              key={direction}
              value={direction}
            >
              {t(DIRECTION_LABELS[direction])}
            </option>
          );
        })}
      </SelectField>
      <Slider
        label={t("fontSize")}
        value={clip.font_size}
        min={10}
        max={120}
        step={1}
        format={(v) => {
          return `${v.toFixed(0)} pt`;
        }}
        onCommit={(fontSize) => {
          return commit({ fontSize });
        }}
      />
      <ColorField
        label={t("textColor")}
        rgb={clip.color_rgba}
        onCommit={([r, g, b]) => {
          return commit({ colorRgba: [r, g, b, clip.color_rgba[3]] });
        }}
      />
      <SelectField
        label={t("alignment")}
        value={clip.text_align}
        onChange={(align) => {
          return commit({ textAlign: align as TextAlign });
        }}
      >
        {(Object.keys(ALIGN_LABELS) as TextAlign[]).map((align) => {
          return (
            <option
              key={align}
              value={align}
            >
              {t(ALIGN_LABELS[align])}
            </option>
          );
        })}
      </SelectField>

      <label className="check">
        <input
          type="checkbox"
          checked={backgroundEnabled}
          onChange={(e) => {
            return commit({
              backgroundRgba: [
                clip.background_rgba[0],
                clip.background_rgba[1],
                clip.background_rgba[2],
                e.target.checked ? 192 : 0,
              ],
            });
          }}
        />
        {t("backgroundEnabled")}
      </label>
      {backgroundEnabled && (
        <>
          <ColorField
            label={t("backgroundColor")}
            rgb={clip.background_rgba}
            onCommit={([r, g, b]) => {
              return commit({ backgroundRgba: [r, g, b, clip.background_rgba[3]] });
            }}
          />
          <Slider
            label={t("backgroundPadding")}
            value={clip.background_padding}
            min={0}
            max={64}
            step={1}
            format={(v) => {
              return `${v.toFixed(0)} px`;
            }}
            onCommit={(backgroundPadding) => {
              return commit({ backgroundPadding });
            }}
          />
          <Slider
            label={t("backgroundRadius")}
            value={clip.background_corner_radius}
            min={0}
            max={64}
            step={1}
            format={(v) => {
              return `${v.toFixed(0)} px`;
            }}
            onCommit={(backgroundCornerRadius) => {
              return commit({ backgroundCornerRadius });
            }}
          />
        </>
      )}

      <Slider
        label={t("positionX")}
        value={clip.pos_x}
        min={0}
        max={1}
        step={0.01}
        format={(v) => {
          return `${(v * 100).toFixed(0)}%`;
        }}
        onCommit={(posX) => {
          return commit({ posX });
        }}
      />
      <Slider
        label={t("positionY")}
        value={clip.pos_y}
        min={0}
        max={1}
        step={0.01}
        format={(v) => {
          return `${(v * 100).toFixed(0)}%`;
        }}
        onCommit={(posY) => {
          return commit({ posY });
        }}
      />

      <Section
        label={t("posXKeyframes")}
        defaultOpen={clip.pos_x_keyframes.length > 0}
      >
        <F32KeyframeList
          label={t("posXKeyframes")}
          keyframes={clip.pos_x_keyframes}
          min={0}
          max={1}
          step={0.01}
          defaultValue={clip.pos_x}
          onCommit={(posXKeyframes) => {
            return commit({ posXKeyframes });
          }}
        />
      </Section>
      <Section
        label={t("posYKeyframes")}
        defaultOpen={clip.pos_y_keyframes.length > 0}
      >
        <F32KeyframeList
          label={t("posYKeyframes")}
          keyframes={clip.pos_y_keyframes}
          min={0}
          max={1}
          step={0.01}
          defaultValue={clip.pos_y}
          onCommit={(posYKeyframes) => {
            return commit({ posYKeyframes });
          }}
        />
      </Section>
      <Section
        label={t("scaleKeyframes")}
        defaultOpen={clip.scale_keyframes.length > 0}
      >
        <F32KeyframeList
          label={t("scaleKeyframes")}
          keyframes={clip.scale_keyframes}
          min={1}
          max={3}
          step={0.01}
          defaultValue={1}
          onCommit={(scaleKeyframes) => {
            return commit({ scaleKeyframes });
          }}
        />
      </Section>
      <Section
        label={t("rotationKeyframes")}
        defaultOpen={clip.rotation_keyframes.length > 0}
      >
        <F32KeyframeList
          label={t("rotationKeyframes")}
          keyframes={clip.rotation_keyframes}
          min={-180}
          max={180}
          step={1}
          defaultValue={0}
          onCommit={(rotationKeyframes) => {
            return commit({ rotationKeyframes });
          }}
        />
      </Section>
      <Section
        label={t("opacityKeyframes")}
        defaultOpen={clip.opacity_keyframes.length > 0}
      >
        <F32KeyframeList
          label={t("opacityKeyframes")}
          keyframes={clip.opacity_keyframes}
          min={0}
          max={1}
          step={0.01}
          defaultValue={1}
          onCommit={(opacityKeyframes) => {
            return commit({ opacityKeyframes });
          }}
        />
      </Section>

      {clip.words.length > 0 && (
        <>
          <label className="check">
            <input
              type="checkbox"
              checked={clip.highlight_enabled}
              onChange={(e) => {
                return commit({ highlightEnabled: e.target.checked });
              }}
            />
            {t("wordHighlight")}
          </label>
          <ColorField
            label={t("highlightColor")}
            rgb={clip.highlight_color_rgba}
            onCommit={([r, g, b]) => {
              return commit({
                highlightColorRgba: [r, g, b, clip.highlight_color_rgba[3]],
              });
            }}
          />
        </>
      )}

      <div className="field">
        <label>{t("overlayStart")}</label>
        <input
          type="number"
          min={0}
          step={0.1}
          defaultValue={clip.start_secs}
          onBlur={(e) => {
            return commit({ startSecs: Number(e.target.value) });
          }}
        />
      </div>
      <div className="field">
        <label>{t("overlayDuration")}</label>
        <input
          type="number"
          min={0.1}
          step={0.1}
          defaultValue={clip.duration_secs}
          onBlur={(e) => {
            return commit({ durationSecs: Number(e.target.value) });
          }}
        />
      </div>
    </>
  );
}
