import libraryStyles from "../library/library.module.css";
import editorStyles from "../../screens/editor.module.css";
import { cx } from "../../cx";
import baseStyles from "../../styles/base.module.css";
import styles from "./inspector.module.css";
import { Suspense, useContext, useState } from "react";
import { formatTime, t } from "../../i18n";
import { api } from "../../ipc";
import { previewStore, run, uiStore, useStore } from "../../store";
import { playheadFraction } from "../../keyframeAt";
import { lazyNamed } from "../../lazy";
import {
  activeTimeline,
  aspectRatioDims,
  type ClipInstance,
  type ClipPatch,
  type SessionView,
} from "../../types";
import { InspectorTabContext, Section, Slider, type InspectorGroup } from "./controls";
import { loadTab, saveTab } from "./memory";

// Kept here for the importers that read these from the inspector module.
export { Section, Slider };
import { ColorSection } from "./ColorSection";
import { CropSection } from "./CropSection";
import { GainSection } from "./GainSection";
import { SpeedSection } from "./SpeedSection";
import { TransformSection } from "./TransformSection";

// Code that only a text or shape clip, or the Effects tab, needs: fetched when first shown.
const TextInspector = lazyNamed(() => {
  return import("./TextInspector");
}, "TextInspector");
const ShapeInspector = lazyNamed(() => {
  return import("./ShapeInspector");
}, "ShapeInspector");
const EffectsSections = lazyNamed(() => {
  return import("./EffectsSections");
}, "EffectsSections");

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
  const measuredLufs = asset?.loudness?.integrated_lufs ?? null;
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

  const outerTab = useContext(InspectorTabContext);
  const [tab, setTabState] = useState<InspectorGroup>(loadTab);
  const setTab = (next: InspectorGroup) => {
    setTabState(next);
    saveTab(next);
  };
  return (
    <section className={cx(styles, baseStyles, libraryStyles, editorStyles, "panel inspector")}>
      <div
        className={cx(styles, baseStyles, libraryStyles, "panel-tabs")}
        role="tablist"
      >
        {(clip ? (["inspector", "effects", "audio"] as const) : (["inspector"] as const)).map(
          (id) => {
            return (
              <button
                key={id}
                role="tab"
                aria-selected={tab === id}
                className={cx(styles, baseStyles, tab === id ? "active" : "")}
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
        className={cx(styles, baseStyles, "inspector-body")}
        data-tab={clip ? tab : "inspector"}
      >
        {clip ? (
          <div data-testid="inspector-clip">
            <div
              className={cx(styles, baseStyles, "clip-head")}
              title={asset?.source_path}
            >
              <span className={cx(styles, baseStyles, "clip-name")}>{asset?.file_name ?? "—"}</span>
              <span className={cx(styles, baseStyles, "muted")}>
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
            <InspectorTabContext.Provider value={outerTab ?? tab}>
              <ClipControls
                key={clip.id}
                clip={clip}
                measuredLufs={measuredLufs}
                targetLufs={timelineSequence.export_settings.target_lufs}
                canvas={aspectRatioDims(
                  timelineSequence.export_settings.aspect_ratio,
                  asset?.resolution ?? [1920, 1080],
                )}
              />
            </InspectorTabContext.Provider>
          </div>
        ) : textClip ? (
          <Suspense fallback={null}>
            <TextInspector
              key={textClip.id}
              clip={textClip}
            />
          </Suspense>
        ) : shapeClip ? (
          <Suspense fallback={null}>
            <ShapeInspector
              key={shapeClip.id}
              clip={shapeClip}
            />
          </Suspense>
        ) : selectedClipIds.length > 1 ? (
          <MultiSelection ids={selectedClipIds} />
        ) : (
          <div className={cx(styles, baseStyles, libraryStyles, "empty-state")}>
            {t("noClipSelected")}
          </div>
        )}
      </div>
    </section>
  );
}

function ClipControls({
  clip,
  canvas,
  measuredLufs,
  targetLufs,
}: {
  clip: ClipInstance;
  measuredLufs: number | null;
  targetLufs: number;
  /** Output canvas size in pixels, for the transform's pixel fields. */
  canvas: [number, number];
}) {
  const commit = (patch: ClipPatch) => {
    return run(api.updateClip(clip.id, patch));
  };
  const { playheadSecs } = useStore(previewStore);
  const fraction = playheadFraction(clip, playheadSecs);
  const tab = useContext(InspectorTabContext);
  return (
    <>
      <TransformSection
        clip={clip}
        commit={commit}
        canvas={canvas}
        fraction={fraction}
      />
      <GainSection
        clip={clip}
        commit={commit}
        fraction={fraction}
        measuredLufs={measuredLufs}
        targetLufs={targetLufs}
      />
      <SpeedSection
        clip={clip}
        commit={commit}
      />
      <ColorSection
        clip={clip}
        commit={commit}
        fraction={fraction}
      />
      <CropSection
        clip={clip}
        commit={commit}
        fraction={fraction}
      />
      {(tab === "effects" || tab === "all") && (
        <Suspense fallback={null}>
          <EffectsSections
            clip={clip}
            commit={commit}
          />
        </Suspense>
      )}
    </>
  );
}

/** What the inspector shows for several selected clips: how many, and the actions that apply to
 *  a group. */
function MultiSelection({ ids }: { ids: number[] }) {
  return (
    <div
      className={cx(styles, baseStyles, "multi-selection")}
      data-testid="inspector-multi"
    >
      <p className={cx(styles, baseStyles, "clip-name")}>
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
