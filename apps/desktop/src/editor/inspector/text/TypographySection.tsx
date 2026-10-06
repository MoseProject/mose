import { t, type TextKey } from "../../../i18n";
import type {
  FontCategory,
  TextAlign,
  TextClip,
  TextClipPatch,
  TextDirection,
  TextFontStyle,
} from "../../../types";
import { Section, Slider } from "../controls";
import { ColorField, SelectField, useOverlayCatalog } from "../OverlayFields";

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

type Commit = (patch: TextClipPatch) => void;

export function TypographySection({ clip, commit }: { clip: TextClip; commit: Commit }) {
  const catalog = useOverlayCatalog();
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
  return (
    <>
      <Section
        label={t("textTypography")}
        defaultOpen
        testId="section-text-typography"
      >
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
      </Section>
    </>
  );
}
