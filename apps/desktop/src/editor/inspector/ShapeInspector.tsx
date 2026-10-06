import { cx } from "../../cx";
import baseStyles from "../../styles/base.module.css";
import styles from "./inspector.module.css";
// Inspector section for a selected shape overlay -- the egui properties panel's shape controls
// (`crates/ui/src/screens/editor/properties_panel/shape_clip.rs`). Each change is one
// `update_shape_clip` call, so one undo step; the preview engine reopens with the new overlay.

import { t, type TextKey } from "../../i18n";
import { Icon } from "../chrome/Icon";
import { api } from "../../ipc";
import { shapePresetOf } from "../../overlays";
import { run } from "../../store";
import type { ShapeClip, ShapeClipPatch, ShapePreset } from "../../types";
import { F32KeyframeList } from "./KeyframeEditor";
import { Section, Slider } from "./controls";
import { ColorField, SelectField, useOverlayCatalog } from "./OverlayFields";

const PRESET_LABELS: Record<ShapePreset, TextKey> = {
  Ellipse: "shapeEllipse",
  Rectangle: "shapeRectangle",
  Triangle: "shapeTriangle",
  Trapezoid: "shapeTrapezoid",
  Arrow: "shapeArrow",
  Custom: "shapeCustom",
};

const percent = (v: number) => {
  return `${(v * 100).toFixed(0)}%`;
};

export function ShapeInspector({ clip }: { clip: ShapeClip }) {
  const catalog = useOverlayCatalog();
  const commit = (patch: ShapeClipPatch) => {
    return void run(api.updateShapeClip(clip.id, patch));
  };
  const presets = catalog?.shapePresets ?? [];
  const current = catalog ? shapePresetOf(clip.shape_kind, presets) : null;
  const vertices = typeof clip.shape_kind === "object" ? clip.shape_kind.Polygon : null;

  return (
    <>
      <div
        className={cx(styles, baseStyles, "panel-title")}
        style={{ border: 0, padding: 0 }}
      >
        {t("shape")}
      </div>
      <SelectField
        label={t("shapeKind")}
        value={current ?? ""}
        disabled={!catalog}
        onChange={(preset) => {
          return commit({ shapeKind: preset as ShapePreset });
        }}
      >
        {presets.map(({ preset }) => {
          return (
            <option
              key={preset}
              value={preset}
            >
              {t(PRESET_LABELS[preset])}
            </option>
          );
        })}
        {/* Custom is only ever the current value, never something to pick. */}
        {current === "Custom" && (
          <option
            value="Custom"
            disabled
          >
            {t(PRESET_LABELS.Custom)}
          </option>
        )}
      </SelectField>

      {/* Vertex editor -- every preset except Ellipse is already a Polygon under the hood, so
          this lets the user hand-edit any of them into a custom shape too. */}
      {vertices && (
        <Section
          label={t("shapeVertices")}
          defaultOpen={true}
        >
          <p
            className={cx(styles, baseStyles, "muted")}
            style={{ fontSize: 11, margin: "0 0 4px" }}
          >
            {t("shapeVerticesHint")}
          </p>
          {vertices.map(([x, y], i) => {
            return (
              <div
                className={cx(styles, baseStyles, "field")}
                key={i}
              >
                <label>#{i + 1}</label>
                <div style={{ display: "flex", gap: 4 }}>
                  <input
                    type="number"
                    step={0.01}
                    value={x}
                    onChange={(e) => {
                      const next = vertices.map((v, j) => {
                        return j === i ? [Number(e.target.value), v[1]] : v;
                      }) as [number, number][];
                      commit({ polygonVertices: next });
                    }}
                  />
                  <input
                    type="number"
                    step={0.01}
                    value={y}
                    onChange={(e) => {
                      const next = vertices.map((v, j) => {
                        return j === i ? [v[0], Number(e.target.value)] : v;
                      }) as [number, number][];
                      commit({ polygonVertices: next });
                    }}
                  />
                  <button
                    className={cx(styles, baseStyles, "icon")}
                    aria-label={t("removeVertex")}
                    title={t("removeVertex")}
                    disabled={vertices.length <= 3}
                    onClick={() => {
                      return commit({
                        polygonVertices: vertices.filter((_, j) => {
                          return j !== i;
                        }),
                      });
                    }}
                  >
                    <Icon name="close" />
                  </button>
                </div>
              </div>
            );
          })}
        </Section>
      )}

      <ColorField
        label={t("shapeColor")}
        rgb={clip.color_rgba}
        onCommit={(colorRgb) => {
          return commit({ colorRgb });
        }}
      />
      <Slider
        label={t("opacity")}
        value={clip.color_rgba[3] / 255}
        min={0}
        max={1}
        step={0.01}
        format={percent}
        onCommit={(v) => {
          return commit({ opacity: Math.round(v * 255) });
        }}
      />
      <Slider
        label={t("positionX")}
        value={clip.center_x}
        min={0}
        max={1}
        step={0.01}
        format={percent}
        onCommit={(centerX) => {
          return commit({ centerX });
        }}
      />
      <Slider
        label={t("positionY")}
        value={clip.center_y}
        min={0}
        max={1}
        step={0.01}
        format={percent}
        onCommit={(centerY) => {
          return commit({ centerY });
        }}
      />
      <Section
        label={t("posXKeyframes")}
        defaultOpen={clip.center_x_keyframes.length > 0}
      >
        <F32KeyframeList
          label={t("posXKeyframes")}
          keyframes={clip.center_x_keyframes}
          min={0}
          max={1}
          step={0.01}
          defaultValue={clip.center_x}
          onCommit={(centerXKeyframes) => {
            return commit({ centerXKeyframes });
          }}
        />
      </Section>
      <Section
        label={t("posYKeyframes")}
        defaultOpen={clip.center_y_keyframes.length > 0}
      >
        <F32KeyframeList
          label={t("posYKeyframes")}
          keyframes={clip.center_y_keyframes}
          min={0}
          max={1}
          step={0.01}
          defaultValue={clip.center_y}
          onCommit={(centerYKeyframes) => {
            return commit({ centerYKeyframes });
          }}
        />
      </Section>
      <Slider
        label={t("width")}
        value={clip.width}
        min={0.01}
        max={1}
        step={0.01}
        format={percent}
        onCommit={(width) => {
          return commit({ width });
        }}
      />
      <Slider
        label={t("height")}
        value={clip.height}
        min={0.01}
        max={1}
        step={0.01}
        format={percent}
        onCommit={(height) => {
          return commit({ height });
        }}
      />
      <Section
        label={t("widthKeyframes")}
        defaultOpen={clip.width_keyframes.length > 0}
      >
        <F32KeyframeList
          label={t("widthKeyframes")}
          keyframes={clip.width_keyframes}
          min={0.01}
          max={1}
          step={0.01}
          defaultValue={clip.width}
          onCommit={(widthKeyframes) => {
            return commit({ widthKeyframes });
          }}
        />
      </Section>
      <Section
        label={t("heightKeyframes")}
        defaultOpen={clip.height_keyframes.length > 0}
      >
        <F32KeyframeList
          label={t("heightKeyframes")}
          keyframes={clip.height_keyframes}
          min={0.01}
          max={1}
          step={0.01}
          defaultValue={clip.height}
          onCommit={(heightKeyframes) => {
            return commit({ heightKeyframes });
          }}
        />
      </Section>
      <Slider
        label={t("shapeRotation")}
        value={clip.rotation_deg}
        min={0}
        max={360}
        step={1}
        format={(v) => {
          return `${v.toFixed(0)}°`;
        }}
        onCommit={(rotationDeg) => {
          return commit({ rotationDeg });
        }}
      />
      <Section
        label={t("rotationKeyframes")}
        defaultOpen={clip.rotation_keyframes.length > 0}
      >
        <F32KeyframeList
          label={t("rotationKeyframes")}
          keyframes={clip.rotation_keyframes}
          min={0}
          max={360}
          step={1}
          defaultValue={clip.rotation_deg}
          onCommit={(rotationKeyframes) => {
            return commit({ rotationKeyframes });
          }}
        />
      </Section>
      <div className={cx(styles, baseStyles, "field")}>
        <label>{t("shapeStroke")}</label>
        <input
          type="number"
          min={0}
          step={0.5}
          defaultValue={clip.stroke_thickness_px}
          onBlur={(e) => {
            return commit({ strokeThicknessPx: Number(e.target.value) });
          }}
        />
      </div>
      <p
        className={cx(styles, baseStyles, "muted")}
        style={{ fontSize: 10, margin: 0 }}
      >
        {t("shapeStrokeHint")}
      </p>
      <div className={cx(styles, baseStyles, "field")}>
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
      <div className={cx(styles, baseStyles, "field")}>
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
