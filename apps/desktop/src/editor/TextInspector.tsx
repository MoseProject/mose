// Inspector section for a selected text overlay -- the egui properties panel's text controls
// (`crates/ui/src/screens/editor/properties_panel/text_clip.rs`). Each change is one
// `update_text_clip` call, so one undo step; the preview engine reopens with the new overlay.

import { t, type TextKey } from "../i18n";
import { api } from "../ipc";
import { run } from "../store";
import type { FontCategory, TextAlign, TextClip, TextClipPatch, TextFontStyle } from "../types";
import { Slider } from "./Inspector";
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

export function TextInspector({ clip }: { clip: TextClip }) {
  const catalog = useOverlayCatalog();
  const commit = (patch: TextClipPatch) => void run(api.updateTextClip(clip.id, patch));
  const fonts = catalog?.fonts ?? [];
  const font = fonts.find((f) => f.id === clip.font_family);
  const categories = [...new Set(fonts.map((f) => f.category))];

  return (
    <>
      <div className="panel-title" style={{ border: 0, padding: 0 }}>
        {t("text")}
      </div>
      <TextField
        label={t("text")}
        value={clip.text}
        placeholder={t("textContentHint")}
        onCommit={(text) => commit({ text })}
      />
      <SelectField
        label={t("font")}
        value={clip.font_family}
        disabled={!catalog}
        onChange={(fontFamily) => commit({ fontFamily })}
      >
        {catalog && !font && <option value={clip.font_family}>{t("unknownFont")}</option>}
        {categories.map((category) => (
          <optgroup key={category} label={t(CATEGORY_LABELS[category])}>
            {fonts
              .filter((f) => f.category === category)
              .map((f) => (
                <option key={f.id} value={f.id}>
                  {f.displayName}
                </option>
              ))}
          </optgroup>
        ))}
      </SelectField>
      <SelectField
        label={t("fontStyle")}
        value={clip.font_style}
        disabled={!font?.supportsBold}
        onChange={(style) => commit({ fontStyle: style as TextFontStyle })}
      >
        <option value="Regular">{t("fontRegular")}</option>
        <option value="Bold">{t("fontBold")}</option>
      </SelectField>
      <Slider
        label={t("fontSize")}
        value={clip.font_size}
        min={10}
        max={120}
        step={1}
        format={(v) => `${v.toFixed(0)} pt`}
        onCommit={(fontSize) => commit({ fontSize })}
      />
      <ColorField
        label={t("textColor")}
        rgb={clip.color_rgba}
        onCommit={([r, g, b]) => commit({ colorRgba: [r, g, b, clip.color_rgba[3]] })}
      />
      <SelectField
        label={t("alignment")}
        value={clip.text_align}
        onChange={(align) => commit({ textAlign: align as TextAlign })}
      >
        {(Object.keys(ALIGN_LABELS) as TextAlign[]).map((align) => (
          <option key={align} value={align}>
            {t(ALIGN_LABELS[align])}
          </option>
        ))}
      </SelectField>
      <Slider
        label={t("positionX")}
        value={clip.pos_x}
        min={0}
        max={1}
        step={0.01}
        format={(v) => `${(v * 100).toFixed(0)}%`}
        onCommit={(posX) => commit({ posX })}
      />
      <Slider
        label={t("positionY")}
        value={clip.pos_y}
        min={0}
        max={1}
        step={0.01}
        format={(v) => `${(v * 100).toFixed(0)}%`}
        onCommit={(posY) => commit({ posY })}
      />
      {clip.words.length > 0 && (
        <>
          <label className="check">
            <input
              type="checkbox"
              checked={clip.highlight_enabled}
              onChange={(e) => commit({ highlightEnabled: e.target.checked })}
            />
            {t("wordHighlight")}
          </label>
          <ColorField
            label={t("highlightColor")}
            rgb={clip.highlight_color_rgba}
            onCommit={([r, g, b]) =>
              commit({ highlightColorRgba: [r, g, b, clip.highlight_color_rgba[3]] })
            }
          />
        </>
      )}
    </>
  );
}
