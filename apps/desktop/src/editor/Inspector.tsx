import { useState, type ReactNode } from "react";
import { Icon } from "./Icon";
import { useDraft } from "../useDraft";
import { formatTime, t } from "../i18n";
import { api } from "../ipc";
import { previewStore, run, uiStore, useStore } from "../store";
import {
  hasKeyframeAt,
  playheadFraction,
  toggleKeyframe,
  valueAt,
  withKeyframe,
  withoutKeyframe,
} from "../keyframeAt";
import { F32KeyframeList, PositionKeyframeList } from "./KeyframeEditor";
import { ShapeInspector } from "./ShapeInspector";
import { TextInspector } from "./TextInspector";
import {
  activeTimeline,
  aspectRatioDims,
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
  const timelineSequence = session.project.sequences[session.project.active_sequence];
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
              canvas={aspectRatioDims(
                timelineSequence.export_settings.aspect_ratio,
                asset?.resolution ?? [1920, 1080],
              )}
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
        ) : selectedClipIds.length > 1 ? (
          <MultiSelection ids={selectedClipIds} />
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

/** State of a section's keyframe diamond. */
export interface SectionKeyframe {
  /** A keyframe already sits at the playhead, so clicking removes it. */
  active: boolean;
  /** The playhead is outside the clip. */
  disabled: boolean;
  onToggle: () => void;
}

export function Section({
  label,
  defaultOpen,
  testId,
  group = "inspector",
  onReset,
  keyframe,
  children,
}: {
  label: string;
  defaultOpen: boolean;
  testId?: string;
  group?: InspectorGroup;
  /** Shows a reset button in the header that puts this section back to its defaults. */
  onReset?: () => void;
  /** Shows a diamond in the header that adds or removes a keyframe at the playhead. */
  keyframe?: SectionKeyframe;
  children: ReactNode;
}) {
  return (
    <details
      className="property-section"
      open={defaultOpen}
      data-testid={testId}
      data-group={group}
    >
      <summary>
        <span>{label}</span>
        {keyframe && (
          <button
            className={"icon section-keyframe" + (keyframe.active ? " active" : "")}
            aria-label={t("keyframeToggle") + ": " + label}
            aria-pressed={keyframe.active}
            title={keyframe.disabled ? t("keyframeOutside") : t("keyframeToggle")}
            disabled={keyframe.disabled}
            onClick={(e) => {
              e.preventDefault();
              e.stopPropagation();
              keyframe.onToggle();
            }}
          >
            <Icon name="keyframe" />
          </button>
        )}
        {onReset && (
          <button
            className="icon section-reset"
            aria-label={t("resetSection") + ": " + label}
            title={t("resetSection")}
            onClick={(e) => {
              e.preventDefault();
              e.stopPropagation();
              onReset();
            }}
          >
            <Icon name="reset" />
          </button>
        )}
      </summary>
      <div className="property-section-body">{children}</div>
    </details>
  );
}

function ClipControls({
  clip,
  canvas,
}: {
  clip: ClipInstance;
  /** Output canvas size in pixels, for the transform's pixel fields. */
  canvas: [number, number];
}) {
  const commit = (patch: ClipPatch) => {
    return run(api.updateClip(clip.id, patch));
  };
  const [region, setRegion] = useState({ centerX: 0.5, centerY: 0.5, width: 0.3, height: 0.3 });
  const { playheadSecs } = useStore(previewStore);
  const fraction = playheadFraction(clip, playheadSecs);
  const gainDiamond: SectionKeyframe = {
    active: fraction !== null && hasKeyframeAt(clip.gain_keyframes, fraction),
    disabled: fraction === null,
    onToggle: () => {
      if (fraction !== null) {
        void commit({
          gainKeyframes: toggleKeyframe(
            clip.gain_keyframes,
            fraction,
            valueAt(clip.gain_keyframes, fraction, clip.gain_db),
          ),
        });
      }
    },
  };
  const colorDiamond: SectionKeyframe = {
    active:
      fraction !== null &&
      (hasKeyframeAt(clip.brightness_keyframes, fraction) ||
        hasKeyframeAt(clip.contrast_keyframes, fraction) ||
        hasKeyframeAt(clip.saturation_keyframes, fraction)),
    disabled: fraction === null,
    onToggle: () => {
      if (fraction === null) {
        return;
      }
      const lists = [
        ["brightnessKeyframes", clip.brightness_keyframes, clip.brightness],
        ["contrastKeyframes", clip.contrast_keyframes, clip.contrast],
        ["saturationKeyframes", clip.saturation_keyframes, clip.saturation],
      ] as const;
      const remove = lists.some(([, list]) => {
        return hasKeyframeAt(list, fraction);
      });
      const patch: ClipPatch = {};
      for (const [key, list, base] of lists) {
        patch[key] = remove
          ? withoutKeyframe(list, fraction)
          : withKeyframe(list, fraction, valueAt(list, fraction, base));
      }
      void commit(patch);
    },
  };
  return (
    <>
      <TransformSection
        clip={clip}
        commit={commit}
        canvas={canvas}
        fraction={fraction}
      />
      <Section
        label={t("gain")}
        defaultOpen={clip.gain_db !== 0}
        testId="section-gain"
        group="audio"
        keyframe={gainDiamond}
        onReset={() => {
          void commit({ gainDb: 0 });
        }}
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
        onReset={() => {
          void commit({ speedFactor: 1 });
        }}
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
        keyframe={colorDiamond}
        defaultOpen={clip.brightness !== 0 || clip.contrast !== 1 || clip.saturation !== 1}
        onReset={() => {
          void commit({ brightness: 0, contrast: 1, saturation: 1 });
        }}
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
      <div
        className="clip-checks"
        data-group="inspector"
      >
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
      </div>
      <div
        className="clip-checks"
        data-group="audio"
      >
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
      </div>

      <Section
        label={t("crop")}
        defaultOpen={
          clip.crop_x !== 0 || clip.crop_y !== 0 || clip.crop_w !== 1 || clip.crop_h !== 1
        }
        onReset={() => {
          void commit({ cropX: 0, cropY: 0, cropW: 1, cropH: 1 });
        }}
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

/** One numeric inspector field, committed on blur or Enter rather than per keystroke. */
function NumberField({
  label,
  value,
  step,
  disabled,
  suffix,
  onCommit,
}: {
  label: string;
  value: number;
  step: number;
  disabled?: boolean;
  suffix?: string;
  onCommit: (value: number) => void;
}) {
  const shown = String(Number(value.toFixed(2)));
  const [draft, setDraft] = useDraft(shown);
  const commit = () => {
    const parsed = Number(draft);
    if (draft.trim() !== "" && Number.isFinite(parsed) && String(parsed) !== shown) {
      onCommit(parsed);
    } else {
      setDraft(shown);
    }
  };
  return (
    <label className="number-field">
      <span>{label}</span>
      <input
        type="number"
        aria-label={label}
        step={step}
        value={draft}
        disabled={disabled}
        onChange={(e) => {
          setDraft(e.target.value);
        }}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.currentTarget.blur();
          }
        }}
      />
      {suffix && <span className="muted">{suffix}</span>}
    </label>
  );
}

/** Position, scale and rotation as plain numbers. Position is the layer's top-left corner in
 *  pixels of the output canvas, stored as a fraction of it as the on-canvas handle moves it; an animated value (several keyframes)
 *  is edited in the Keyframes section instead. */
function TransformSection({
  clip,
  commit,
  canvas,
  fraction,
}: {
  clip: ClipInstance;
  commit: (patch: ClipPatch) => Promise<unknown>;
  /** Output canvas size in pixels; positions are stored as fractions of it. */
  canvas: [number, number];
  /** The playhead inside the clip (0..1), null outside it. */
  fraction: number | null;
}) {
  const canvasW = Math.max(1, canvas[0]);
  const canvasH = Math.max(1, canvas[1]);
  const position = clip.position_keyframes[0]?.value ?? { x: 0, y: 0 };
  const rotation = clip.rotation_keyframes[0]?.value ?? 0;
  const positionAnimated = clip.position_keyframes.length > 1;
  const rotationAnimated = clip.rotation_keyframes.length > 1;
  const diamond: SectionKeyframe = {
    active:
      fraction !== null &&
      (hasKeyframeAt(clip.position_keyframes, fraction) ||
        hasKeyframeAt(clip.rotation_keyframes, fraction)),
    disabled: fraction === null,
    onToggle: () => {
      if (fraction === null) {
        return;
      }
      const remove =
        hasKeyframeAt(clip.position_keyframes, fraction) ||
        hasKeyframeAt(clip.rotation_keyframes, fraction);
      const xs = clip.position_keyframes.map((keyframe) => {
        return { time_fraction: keyframe.time_fraction, value: keyframe.value.x };
      });
      const ys = clip.position_keyframes.map((keyframe) => {
        return { time_fraction: keyframe.time_fraction, value: keyframe.value.y };
      });
      const here = {
        x: valueAt(xs, fraction, 0),
        y: valueAt(ys, fraction, 0),
      };
      const angle = valueAt(clip.rotation_keyframes, fraction, 0);
      void commit({
        positionKeyframes: remove
          ? withoutKeyframe(clip.position_keyframes, fraction)
          : withKeyframe(clip.position_keyframes, fraction, here),
        rotationKeyframes: remove
          ? withoutKeyframe(clip.rotation_keyframes, fraction)
          : withKeyframe(clip.rotation_keyframes, fraction, angle),
      });
    },
  };
  const setPosition = (x: number, y: number) => {
    // The monitor drag keeps a layer within one canvas of the frame; typed values get the same
    // bound so a stray digit cannot throw the layer out of reach.
    const bound = (value: number) => {
      return Math.min(2, Math.max(-1, value));
    };
    void commit({ positionKeyframes: [{ time_fraction: 0, value: { x: bound(x), y: bound(y) } }] });
  };
  return (
    <Section
      label={t("transform")}
      defaultOpen
      testId="section-transform"
      keyframe={diamond}
      onReset={() => {
        void commit({
          positionKeyframes: [],
          layerScaleX: 1,
          layerScaleY: 1,
          rotationKeyframes: [],
        });
      }}
    >
      <div className="field-row">
        <NumberField
          label={t("positionX")}
          value={Math.round(position.x * canvasW)}
          step={1}
          suffix="px"
          disabled={positionAnimated}
          onCommit={(value) => {
            setPosition(value / canvasW, position.y);
          }}
        />
        <NumberField
          label={t("positionY")}
          value={Math.round(position.y * canvasH)}
          step={1}
          suffix="px"
          disabled={positionAnimated}
          onCommit={(value) => {
            setPosition(position.x, value / canvasH);
          }}
        />
      </div>
      <NumberField
        label={t("scale")}
        value={clip.layer_scale_x * 100}
        step={1}
        suffix="%"
        onCommit={(value) => {
          const scale = Math.max(1, value) / 100;
          void commit({ layerScaleX: scale, layerScaleY: scale });
        }}
      />
      <NumberField
        label={t("rotation")}
        value={rotation}
        step={1}
        suffix="°"
        disabled={rotationAnimated}
        onCommit={(value) => {
          void commit({ rotationKeyframes: [{ time_fraction: 0, value }] });
        }}
      />
      <p className="muted">
        {positionAnimated || rotationAnimated ? t("transformAnimated") : t("transformHint")}
      </p>
    </Section>
  );
}

/** What the inspector shows for several selected clips: how many, and the actions that apply to
 *  a group. */
function MultiSelection({ ids }: { ids: number[] }) {
  return (
    <div
      className="multi-selection"
      data-testid="inspector-multi"
    >
      <p className="clip-name">
        {ids.length} {t("clipsSelected")}
      </p>
      <button
        onClick={() => {
          return run(api.mergeClipsIntoComposite(ids));
        }}
      >
        {t("mergeComposite")}
      </button>
      <button
        onClick={() => {
          return run(api.createCompoundClip(ids[0], ids));
        }}
      >
        {t("compoundClip")}
      </button>
      <button
        onClick={() => {
          return run(api.deleteClips(ids, false));
        }}
      >
        {t("delete")}
      </button>
      <button
        onClick={() => {
          return run(api.deleteClips(ids, true));
        }}
      >
        {t("rippleDelete")}
      </button>
    </div>
  );
}
