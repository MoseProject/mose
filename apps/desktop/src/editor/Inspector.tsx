import { useState } from "react";
import { formatTime, t } from "../i18n";
import { api } from "../ipc";
import { previewStore, run, uiStore, useStore } from "../store";
import { playheadFraction } from "../keyframeAt";
import { ShapeInspector } from "./ShapeInspector";
import { TextInspector } from "./TextInspector";
import {
  activeTimeline,
  aspectRatioDims,
  type ClipInstance,
  type ClipPatch,
  type SessionView,
} from "../types";
import { Section, Slider, type InspectorGroup } from "./inspector/controls";

// Kept here for the importers that read these from the inspector module.
export { Section, Slider };
import { AiActionsSection } from "./inspector/AiActionsSection";
import { ColorSection } from "./inspector/ColorSection";
import { CropSection } from "./inspector/CropSection";
import { GainSection } from "./inspector/GainSection";
import { KeyframesSection } from "./inspector/KeyframesSection";
import { MaskSection } from "./inspector/MaskSection";
import { SpeedSection } from "./inspector/SpeedSection";
import { TransformSection } from "./inspector/TransformSection";
import { TransitionSection } from "./inspector/TransitionSection";

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
  const { playheadSecs } = useStore(previewStore);
  const fraction = playheadFraction(clip, playheadSecs);
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
      />
      <MaskSection
        clip={clip}
        commit={commit}
      />
      <TransitionSection
        clip={clip}
        commit={commit}
      />
      <KeyframesSection
        clip={clip}
        commit={commit}
      />
      <AiActionsSection clip={clip} />
    </>
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
