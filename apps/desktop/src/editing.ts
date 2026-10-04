// Small editing actions shared by the timeline toolbar and the top-bar menus.

import { t } from "./i18n";
import { api } from "./ipc";
import { patchUi, previewStore, run } from "./store";
import { clipDuration, type Marker, type SessionView } from "./types";

/** Moves the playhead to the nearest marker before (`-1`) or after (`1`) it, if any. */
export function seekToMarker(markers: Marker[], direction: -1 | 1) {
  const playhead = previewStore.get().playheadSecs;
  const candidates =
    direction < 0
      ? markers
          .filter((m) => {
            return m.position_secs < playhead;
          })
          .sort((a, b) => {
            return b.position_secs - a.position_secs;
          })
      : markers
          .filter((m) => {
            return m.position_secs > playhead;
          })
          .sort((a, b) => {
            return a.position_secs - b.position_secs;
          });
  if (candidates[0]) {
    void run(api.seek(candidates[0].position_secs));
  }
}

/** Creates a text/shape clip at the playhead and selects it, as egui's "+ Add" buttons do. */
export async function addOverlay(call: Promise<number | null>) {
  const id = await run(call);
  if (id != null) {
    patchUi({ selectedClipIds: [id] });
  }
}

export const addTextOverlay = () => {
  return addOverlay(api.addTextClip(t("defaultTextContent")));
};
export const addShapeOverlay = () => {
  return addOverlay(api.addShapeClip());
};

const activeSequence = (session: SessionView) => {
  return session.project.sequences[session.project.active_sequence];
};

/** End of the last clip (media or overlay) of the active sequence, in seconds. */
export function timelineDuration(session: SessionView): number {
  let end = 0;
  for (const track of activeSequence(session).timeline.tracks) {
    for (const clip of track.clips) {
      end = Math.max(end, clip.start_secs + clipDuration(clip));
    }
    for (const clip of [...track.text_clips, ...track.shape_clips]) {
      end = Math.max(end, clip.start_secs + clip.duration_secs);
    }
  }
  return end;
}

/** Every clip start and end, sorted and de-duplicated: the cut points the playhead can jump to. */
export function editPoints(session: SessionView): number[] {
  const points = new Set<number>([0]);
  for (const track of activeSequence(session).timeline.tracks) {
    for (const clip of track.clips) {
      points.add(Number(clip.start_secs.toFixed(3)));
      points.add(Number((clip.start_secs + clipDuration(clip)).toFixed(3)));
    }
    for (const clip of [...track.text_clips, ...track.shape_clips]) {
      points.add(Number(clip.start_secs.toFixed(3)));
      points.add(Number((clip.start_secs + clip.duration_secs).toFixed(3)));
    }
  }
  return [...points].sort((a, b) => {
    return a - b;
  });
}

/** The nearest cut point strictly before (`-1`) or after (`1`) `from`, or `null`. */
export function neighbourEditPoint(
  points: number[],
  from: number,
  direction: -1 | 1,
): number | null {
  const epsilon = 0.005;
  if (direction < 0) {
    const before = points.filter((point) => {
      return point < from - epsilon;
    });
    return before.length > 0 ? before[before.length - 1] : null;
  }
  return (
    points.find((point) => {
      return point > from + epsilon;
    }) ?? null
  );
}

/** Ids of every clip, text overlay and shape overlay of the active sequence. */
export function allClipIds(session: SessionView): number[] {
  const ids: number[] = [];
  for (const track of activeSequence(session).timeline.tracks) {
    ids.push(
      ...track.clips.map((clip) => {
        return clip.id;
      }),
    );
    ids.push(
      ...track.text_clips.map((clip) => {
        return clip.id;
      }),
    );
    ids.push(
      ...track.shape_clips.map((clip) => {
        return clip.id;
      }),
    );
  }
  return ids;
}

export type TimelineAction =
  "zoomIn" | "zoomOut" | "zoomFit" | "scrollToPlayhead" | "renameSequence";
export const TIMELINE_ACTION_EVENT = "oca-timeline-action";

/** Asks the mounted timeline (which owns its scroll and zoom state) to do something. */
export function timelineAction(action: TimelineAction) {
  window.dispatchEvent(new CustomEvent<TimelineAction>(TIMELINE_ACTION_EVENT, { detail: action }));
}
