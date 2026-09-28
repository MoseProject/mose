// Inspector section for a selected shape overlay -- the egui properties panel's shape controls
// (`crates/ui/src/screens/editor/properties_panel/shape_clip.rs`). Each change is one
// `update_shape_clip` call, so one undo step; the preview engine reopens with the new overlay.

import { t, type TextKey } from "../i18n";
import { api } from "../ipc";
import { shapePresetOf } from "../overlays";
import { run } from "../store";
import type { ShapeClip, ShapeClipPatch, ShapePreset } from "../types";
import { Slider } from "./Inspector";
import { ColorField, SelectField, useOverlayCatalog } from "./OverlayFields";

const PRESET_LABELS: Record<ShapePreset, TextKey> = {
  Ellipse: "shapeEllipse",
  Rectangle: "shapeRectangle",
  Triangle: "shapeTriangle",
  Trapezoid: "shapeTrapezoid",
  Arrow: "shapeArrow",
  Custom: "shapeCustom",
};

const percent = (v: number) => `${(v * 100).toFixed(0)}%`;

export function ShapeInspector({ clip }: { clip: ShapeClip }) {
  const catalog = useOverlayCatalog();
  const commit = (patch: ShapeClipPatch) => void run(api.updateShapeClip(clip.id, patch));
  const presets = catalog?.shapePresets ?? [];
  const current = catalog ? shapePresetOf(clip.shape_kind, presets) : null;

  return (
    <>
      <div className="panel-title" style={{ border: 0, padding: 0 }}>
        {t("shape")}
      </div>
      <SelectField
        label={t("shapeKind")}
        value={current ?? ""}
        disabled={!catalog}
        onChange={(preset) => commit({ shapeKind: preset as ShapePreset })}
      >
        {presets.map(({ preset }) => (
          <option key={preset} value={preset}>
            {t(PRESET_LABELS[preset])}
          </option>
        ))}
        {/* Custom is only ever the current value, never something to pick. */}
        {current === "Custom" && (
          <option value="Custom" disabled>
            {t(PRESET_LABELS.Custom)}
          </option>
        )}
      </SelectField>
      <ColorField
        label={t("shapeColor")}
        rgb={clip.color_rgba}
        onCommit={(colorRgb) => commit({ colorRgb })}
      />
      <Slider
        label={t("opacity")}
        value={clip.color_rgba[3] / 255}
        min={0}
        max={1}
        step={0.01}
        format={percent}
        onCommit={(v) => commit({ opacity: Math.round(v * 255) })}
      />
      <Slider
        label={t("positionX")}
        value={clip.center_x}
        min={0}
        max={1}
        step={0.01}
        format={percent}
        onCommit={(centerX) => commit({ centerX })}
      />
      <Slider
        label={t("positionY")}
        value={clip.center_y}
        min={0}
        max={1}
        step={0.01}
        format={percent}
        onCommit={(centerY) => commit({ centerY })}
      />
      <Slider
        label={t("width")}
        value={clip.width}
        min={0.01}
        max={1}
        step={0.01}
        format={percent}
        onCommit={(width) => commit({ width })}
      />
      <Slider
        label={t("height")}
        value={clip.height}
        min={0.01}
        max={1}
        step={0.01}
        format={percent}
        onCommit={(height) => commit({ height })}
      />
    </>
  );
}
