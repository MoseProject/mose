import { useState, type ReactNode } from "react";
import { useDraft } from "../useDraft";
import { formatTime, t } from "../i18n";
import { api } from "../ipc";
import { run, uiStore, useStore } from "../store";
import { F32KeyframeList, PositionKeyframeList } from "./KeyframeEditor";
import { ShapeInspector } from "./ShapeInspector";
import { TextInspector } from "./TextInspector";
import {
  activeTimeline,
  BLEND_MODES,
  type BlendMode,
  type ClipInstance,
  type ClipPatch,
  type MaskShape,
  type SessionView,
  type TransitionType,
} from "../types";

const MASK_SHAPES: { value: MaskShape; label: () => string }[] = [
  {
    value: "None",
    label: () => {
      return t("maskNone");
    },
  },
  {
    value: "Circle",
    label: () => {
      return t("maskCircle");
    },
  },
  {
    value: "RoundedRect",
    label: () => {
      return t("maskRoundedRect");
    },
  },
];

const TRANSITION_TYPES: { value: TransitionType; label: () => string }[] = [
  {
    value: "None",
    label: () => {
      return t("transitionNone");
    },
  },
  {
    value: "Fade",
    label: () => {
      return t("transitionFade");
    },
  },
  {
    value: "HardCut",
    label: () => {
      return t("transitionHardCut");
    },
  },
  {
    value: "Slide",
    label: () => {
      return t("transitionSlide");
    },
  },
  {
    value: "Zoom",
    label: () => {
      return t("transitionZoom");
    },
  },
];

export function Inspector({ session }: { session: SessionView }) {
  const { selectedClipIds } = useStore(uiStore);
  const timeline = activeTimeline(session.project);
  const clip =
    selectedClipIds.length === 1
      ? timeline.tracks
          .flatMap((track) => {
            return track.clips;
          })
          .find((c) => {
            return c.id === selectedClipIds[0];
          })
      : undefined;
  const asset =
    clip &&
    session.project.media_library.find((a) => {
      return a.id === clip.asset_id;
    });
  const textClip =
    selectedClipIds.length === 1
      ? timeline.tracks
          .flatMap((track) => {
            return track.text_clips;
          })
          .find((item) => {
            return item.id === selectedClipIds[0];
          })
      : undefined;
  const shapeClip =
    selectedClipIds.length === 1
      ? timeline.tracks
          .flatMap((track) => {
            return track.shape_clips;
          })
          .find((item) => {
            return item.id === selectedClipIds[0];
          })
      : undefined;

  const [tab, setTab] = useState<InspectorGroup>("inspector");
  return (
    <section className="panel inspector">
      <div
        className="panel-tabs"
        role="tablist"
      >
        {(clip ? (["inspector", "effects", "audio"] as const) : (["inspector"] as const)).map(
          (id) => {
            return (
              <button
                key={id}
                role="tab"
                aria-selected={tab === id}
                className={tab === id ? "active" : ""}
                onClick={() => {
                  return setTab(id);
                }}
              >
                {t(
                  id === "inspector"
                    ? "inspector"
                    : id === "effects"
                      ? "inspectorEffects"
                      : "inspectorAudio",
                )}
              </button>
            );
          },
        )}
      </div>
      <div
        className="inspector-body"
        data-tab={clip ? tab : "inspector"}
      >
        {clip ? (
          <div data-testid="inspector-clip">
            <div
              className="clip-head"
              title={asset?.source_path}
            >
              <span className="clip-name">{asset?.file_name ?? "—"}</span>
              <span className="muted">
                {[
                  t("source"),
                  asset?.resolution ? `${asset.resolution[0]}×${asset.resolution[1]}` : null,
                  asset?.fps ? `${asset.fps.toFixed(asset.fps % 1 === 0 ? 0 : 3)} fps` : null,
                  asset ? formatTime(asset.duration_secs) : null,
                ]
                  .filter(Boolean)
                  .join(" · ")}
              </span>
            </div>
            <ClipControls
              key={clip.id}
              clip={clip}
            />
          </div>
        ) : textClip ? (
          <TextInspector
            key={textClip.id}
            clip={textClip}
          />
        ) : shapeClip ? (
          <ShapeInspector
            key={shapeClip.id}
            clip={shapeClip}
          />
        ) : (
          <div className="empty-state">{t("noClipSelected")}</div>
        )}
      </div>
    </section>
  );
}
/** Collapsible property group, closed by default unless `defaultOpen` -- mirrors
 *  `crates/ui/src/components/property.rs`'s `property_section`. Uncontrolled (`<details
 *  open>` sets only the *initial* state): the parent `ClipControls` remounts on every clip
 *  selection change (`key={clip.id}` above), so each newly selected clip's sections
 *  recompute their own default-open state fresh, same per-clip behavior egui's
 *  `default_open` argument has. */
export type InspectorGroup = "inspector" | "effects" | "audio";

export function Section({
  label,
  defaultOpen,
  testId,
  group = "inspector",
  children,
}: {
  label: string;
  defaultOpen: boolean;
  testId?: string;
  group?: InspectorGroup;
  children: ReactNode;
}) {
  return (
    <details
      className="property-section"
      open={defaultOpen}
      data-testid={testId}
      data-group={group}
    >
      <summary>{label}</summary>
      <div className="property-section-body">{children}</div>
    </details>
  );
}

function ClipControls({ clip }: { clip: ClipInstance }) {
  const commit = (patch: ClipPatch) => {
    return run(api.updateClip(clip.id, patch));
  };
  const [region, setRegion] = useState({ centerX: 0.5, centerY: 0.5, width: 0.3, height: 0.3 });
  return (
    <>
      <Section
        label={t("gain")}
        defaultOpen={clip.gain_db !== 0}
        testId="section-gain"
        group="audio"
      >
        <Slider
          label={t("gain")}
          value={clip.gain_db}
          min={-24}
          max={24}
          step={0.5}
          format={(v) => {
            return `${v > 0 ? "+" : ""}${v.toFixed(1)} dB`;
          }}
          onCommit={(v) => {
            return commit({ gainDb: v });
          }}
        />
      </Section>
      <Section
        label={t("speed")}
        defaultOpen={clip.speed_factor !== 1}
      >
        <Slider
          label={t("speed")}
          value={clip.speed_factor}
          min={0.25}
          max={4}
          step={0.05}
          format={(v) => {
            return `${v.toFixed(2)}×`;
          }}
          onCommit={(v) => {
            return commit({ speedFactor: v });
          }}
        />
        <div className="field">
          <span>{t("speedRamp")}</span>
          <div className="export-actions">
            <button
              className="icon"
              onClick={() => {
                return run(api.applySpeedRampToClip(clip.id, 0.5, 2.0, 4));
              }}
            >
              {t("speedRampSlowToFast")}
            </button>
            <button
              className="icon"
              onClick={() => {
                return run(api.applySpeedRampToClip(clip.id, 2.0, 0.5, 4));
              }}
            >
              {t("speedRampFastToSlow")}
            </button>
          </div>
        </div>
      </Section>
      <Section
        group="effects"
        label={t("colorAdjust")}
        defaultOpen={clip.brightness !== 0 || clip.contrast !== 1 || clip.saturation !== 1}
      >
        <Slider
          label={t("brightness")}
          value={clip.brightness}
          min={-1}
          max={1}
          step={0.01}
          format={(v) => {
            return v.toFixed(2);
          }}
          onCommit={(v) => {
            return commit({ brightness: v });
          }}
        />
        <Slider
          label={t("contrast")}
          value={clip.contrast}
          min={0}
          max={2}
          step={0.01}
          format={(v) => {
            return v.toFixed(2);
          }}
          onCommit={(v) => {
            return commit({ contrast: v });
          }}
        />
        <Slider
          label={t("saturation")}
          value={clip.saturation}
          min={0}
          max={2}
          step={0.01}
          format={(v) => {
            return v.toFixed(2);
          }}
          onCommit={(v) => {
            return commit({ saturation: v });
          }}
        />
      </Section>
      <label className="check">
        <input
          type="checkbox"
          checked={clip.frozen}
          onChange={(e) => {
            return commit({ frozen: e.target.checked });
          }}
        />
        {t("freeze")}
      </label>
      <label className="check">
        <input
          type="checkbox"
          checked={clip.flipped_h}
          onChange={(e) => {
            return commit({ flippedH: e.target.checked });
          }}
        />
        {t("flip")}
      </label>
      <label className="check">
        <input
          type="checkbox"
          checked={clip.voice_cleanup_enabled}
          onChange={(e) => {
            return commit({ voiceCleanupEnabled: e.target.checked });
          }}
        />
        {t("voiceCleanup")}
      </label>

      <Section
        label={t("crop")}
        defaultOpen={
          clip.crop_x !== 0 || clip.crop_y !== 0 || clip.crop_w !== 1 || clip.crop_h !== 1
        }
      >
        <Slider
          label="x"
          value={clip.crop_x}
          min={0}
          max={1}
          step={0.01}
          format={(v) => {
            return v.toFixed(2);
          }}
          onCommit={(v) => {
            return commit({ cropX: v });
          }}
        />
        <Slider
          label="y"
          value={clip.crop_y}
          min={0}
          max={1}
          step={0.01}
          format={(v) => {
            return v.toFixed(2);
          }}
          onCommit={(v) => {
            return commit({ cropY: v });
          }}
        />
        <Slider
          label="w"
          value={clip.crop_w}
          min={0.05}
          max={1}
          step={0.01}
          format={(v) => {
            return v.toFixed(2);
          }}
          onCommit={(v) => {
            return commit({ cropW: v });
          }}
        />
        <Slider
          label="h"
          value={clip.crop_h}
          min={0.05}
          max={1}
          step={0.01}
          format={(v) => {
            return v.toFixed(2);
          }}
          onCommit={(v) => {
            return commit({ cropH: v });
          }}
        />
        <button
          className="icon"
          onClick={() => {
            return commit({ cropX: 0, cropY: 0, cropW: 1, cropH: 1 });
          }}
        >
          {t("cropReset")}
        </button>
      </Section>

      <Section
        label={t("mask")}
        defaultOpen={clip.mask_shape !== "None"}
        group="effects"
      >
        <div className="field">
          <label>{t("mask")}</label>
          <select
            value={clip.mask_shape}
            onChange={(e) => {
              return commit({ maskShape: e.target.value as MaskShape });
            }}
          >
            {MASK_SHAPES.map(({ value, label }) => {
              return (
                <option
                  key={value}
                  value={value}
                >
                  {label()}
                </option>
              );
            })}
          </select>
        </div>
        {clip.mask_shape === "RoundedRect" && (
          <Slider
            label={t("maskCornerRadius")}
            value={clip.mask_corner_radius}
            min={0}
            max={1}
            step={0.01}
            format={(v) => {
              return v.toFixed(2);
            }}
            onCommit={(v) => {
              return commit({ maskCornerRadius: v });
            }}
          />
        )}
      </Section>

      <Section
        label={t("transition")}
        defaultOpen={clip.transition_in !== "None"}
        group="effects"
      >
        <div className="field">
          <label>{t("transition")}</label>
          <select
            value={clip.transition_in}
            onChange={(e) => {
              return commit({ transitionIn: e.target.value as TransitionType });
            }}
          >
            {TRANSITION_TYPES.map(({ value, label }) => {
              return (
                <option
                  key={value}
                  value={value}
                >
                  {label()}
                </option>
              );
            })}
          </select>
        </div>
        {clip.transition_in !== "None" && (
          <Slider
            label={t("transitionDuration")}
            value={clip.transition_duration_secs}
            min={0.1}
            max={3}
            step={0.1}
            format={(v) => {
              return `${v.toFixed(1)}s`;
            }}
            onCommit={(v) => {
              return commit({ transitionDurationSecs: v });
            }}
          />
        )}
      </Section>

      <div
        className="field"
        data-group="inspector"
      >
        <label>{t("blendMode")}</label>
        <select
          value={clip.blend_mode}
          onChange={(e) => {
            return commit({ blendMode: e.target.value as BlendMode });
          }}
        >
          {BLEND_MODES.map((mode) => {
            return (
              <option
                key={mode}
                value={mode}
              >
                {mode}
              </option>
            );
          })}
        </select>
      </div>

      <Section
        label={t("keyframes")}
        defaultOpen={
          clip.position_keyframes.length > 0 ||
          clip.scale_keyframes.length > 0 ||
          clip.rotation_keyframes.length > 0 ||
          clip.opacity_keyframes.length > 0 ||
          clip.gain_keyframes.length > 0 ||
          clip.brightness_keyframes.length > 0 ||
          clip.contrast_keyframes.length > 0 ||
          clip.saturation_keyframes.length > 0 ||
          clip.crop_x_keyframes.length > 0 ||
          clip.crop_y_keyframes.length > 0 ||
          clip.crop_w_keyframes.length > 0 ||
          clip.crop_h_keyframes.length > 0
        }
      >
        <PositionKeyframeList
          label={t("positionKeyframes")}
          keyframes={clip.position_keyframes}
          onCommit={(next) => {
            return commit({ positionKeyframes: next });
          }}
        />
        <F32KeyframeList
          label={t("scaleKeyframes")}
          keyframes={clip.scale_keyframes}
          min={1}
          max={3}
          step={0.01}
          defaultValue={1}
          onCommit={(next) => {
            return commit({ scaleKeyframes: next });
          }}
        />
        <F32KeyframeList
          label={t("rotationKeyframes")}
          keyframes={clip.rotation_keyframes}
          min={-180}
          max={180}
          step={1}
          defaultValue={0}
          onCommit={(next) => {
            return commit({ rotationKeyframes: next });
          }}
        />
        <F32KeyframeList
          label={t("opacityKeyframes")}
          keyframes={clip.opacity_keyframes}
          min={0}
          max={1}
          step={0.01}
          defaultValue={1}
          onCommit={(next) => {
            return commit({ opacityKeyframes: next });
          }}
        />
        <F32KeyframeList
          label={t("gainKeyframes")}
          keyframes={clip.gain_keyframes}
          min={-24}
          max={24}
          step={0.5}
          defaultValue={0}
          onCommit={(next) => {
            return commit({ gainKeyframes: next });
          }}
        />
        <F32KeyframeList
          label={t("brightnessKeyframes")}
          keyframes={clip.brightness_keyframes}
          min={-1}
          max={1}
          step={0.01}
          defaultValue={0}
          onCommit={(next) => {
            return commit({ brightnessKeyframes: next });
          }}
        />
        <F32KeyframeList
          label={t("contrastKeyframes")}
          keyframes={clip.contrast_keyframes}
          min={0}
          max={2}
          step={0.01}
          defaultValue={1}
          onCommit={(next) => {
            return commit({ contrastKeyframes: next });
          }}
        />
        <F32KeyframeList
          label={t("saturationKeyframes")}
          keyframes={clip.saturation_keyframes}
          min={0}
          max={2}
          step={0.01}
          defaultValue={1}
          onCommit={(next) => {
            return commit({ saturationKeyframes: next });
          }}
        />
        <F32KeyframeList
          label={t("cropXKeyframes")}
          keyframes={clip.crop_x_keyframes}
          min={0}
          max={1}
          step={0.01}
          defaultValue={0}
          onCommit={(next) => {
            return commit({ cropXKeyframes: next });
          }}
        />
        <F32KeyframeList
          label={t("cropYKeyframes")}
          keyframes={clip.crop_y_keyframes}
          min={0}
          max={1}
          step={0.01}
          defaultValue={0}
          onCommit={(next) => {
            return commit({ cropYKeyframes: next });
          }}
        />
        <F32KeyframeList
          label={t("cropWKeyframes")}
          keyframes={clip.crop_w_keyframes}
          min={0.05}
          max={1}
          step={0.01}
          defaultValue={1}
          onCommit={(next) => {
            return commit({ cropWKeyframes: next });
          }}
        />
        <F32KeyframeList
          label={t("cropHKeyframes")}
          keyframes={clip.crop_h_keyframes}
          min={0.05}
          max={1}
          step={0.01}
          defaultValue={1}
          onCommit={(next) => {
            return commit({ cropHKeyframes: next });
          }}
        />
      </Section>

      <Section
        label={t("aiActions")}
        defaultOpen={false}
        group="effects"
      >
        <div className="region-row">
          <input
            type="number"
            step={0.05}
            min={0}
            max={1}
            value={region.centerX}
            title={t("centerX")}
            onChange={(e) => {
              return setRegion({ ...region, centerX: Number(e.target.value) });
            }}
          />
          <input
            type="number"
            step={0.05}
            min={0}
            max={1}
            value={region.centerY}
            title={t("centerY")}
            onChange={(e) => {
              return setRegion({ ...region, centerY: Number(e.target.value) });
            }}
          />
          <input
            type="number"
            step={0.05}
            min={0.05}
            max={0.6}
            value={region.width}
            title={t("regionWidth")}
            onChange={(e) => {
              return setRegion({ ...region, width: Number(e.target.value) });
            }}
          />
          <input
            type="number"
            step={0.05}
            min={0.05}
            max={0.6}
            value={region.height}
            title={t("regionHeight")}
            onChange={(e) => {
              return setRegion({ ...region, height: Number(e.target.value) });
            }}
          />
        </div>
        <button
          onClick={() => {
            return run(
              api.motionTrackClip(
                clip.id,
                region.centerX,
                region.centerY,
                region.width,
                region.height,
                0.08,
              ),
            );
          }}
        >
          {t("trackMotion")}
        </button>
        <button
          onClick={() => {
            return run(api.autoReframeClip(clip.id));
          }}
        >
          {t("autoReframe")}
        </button>
        <button
          onClick={() => {
            return run(api.dynamicAutoReframeClip(clip.id));
          }}
        >
          {t("dynamicAutoReframe")}
        </button>
        <button
          onClick={() => {
            return run(api.generateMatteForClip(clip.id));
          }}
        >
          {t("generateMatte")}
        </button>
        <button
          onClick={() => {
            return run(
              api.applyPrivacyBlurToClip(
                clip.id,
                region.centerX,
                region.centerY,
                region.width,
                region.height,
              ),
            );
          }}
        >
          {t("applyPrivacyBlur")}
        </button>
      </Section>
    </>
  );
}

/** A range input that previews locally while dragging and commits once on release. */
export function Slider(props: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  format: (value: number) => string;
  onCommit: (value: number) => void;
}) {
  const [draft, setDraft] = useDraft(props.value);
  const commit = () => {
    if (draft !== props.value) {
      props.onCommit(draft);
    }
  };
  return (
    <div className="field slider-field">
      <label>{props.label}</label>
      <input
        type="range"
        min={props.min}
        max={props.max}
        step={props.step}
        value={draft}
        onChange={(e) => {
          return setDraft(Number(e.target.value));
        }}
        onPointerUp={commit}
        onKeyUp={commit}
        onBlur={commit}
      />
      <span className="mono">{props.format(draft)}</span>
    </div>
  );
}
