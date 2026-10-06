// Webview-side state. Authoritative project data arrives from Rust (`session-changed`); this
// module only caches it and adds transient UI state (screen, selection, zoom) that is never
// serialized -- the same persistent/transient split the egui shell keeps between `avcore`
// types and `ui::App`. Preview ticks live in their own store so 60 Hz playhead updates only
// re-render the components that read them.

import { applyOps } from "./sessionPatch";
import { useSyncExternalStore } from "react";
import {
  api,
  onAutoReframe,
  onExports,
  onAutosaveFailed,
  onImportFailed,
  onPreviewRecovery,
  onMatteGeneration,
  onMotionTrack,
  onPreview,
  onPrivacyBlur,
  onSceneCut,
  onSession,
  onSessionPatch,
  onTranscribe,
  onTts,
} from "./ipc";
import { localeFromPrefs, setLocale, t, type TextKey } from "./i18n";
import type { ExportJobView, LayerTemplate, PreviewTick, SessionView } from "./types";

export interface Store<T> {
  get: () => T;
  set: (next: T | ((prev: T) => T)) => void;
  subscribe: (listener: () => void) => () => void;
}

export function createStore<T>(initial: T): Store<T> {
  let value = initial;
  const listeners = new Set<() => void>();
  return {
    get: () => {
      return value;
    },
    set: (next) => {
      const resolved = typeof next === "function" ? (next as (prev: T) => T)(value) : next;
      if (Object.is(resolved, value)) {
        return;
      }
      value = resolved;
      listeners.forEach((l) => {
        return l();
      });
    },
    subscribe: (listener) => {
      listeners.add(listener);
      return () => {
        return listeners.delete(listener);
      };
    },
  };
}

export function useStore<T>(store: Store<T>): T {
  return useSyncExternalStore(store.subscribe, store.get);
}

export type Screen = "home" | "editor";

/** Timeline edit tool -- egui's `EditorTool`. "select" is the default drag-to-move/trim mode;
 *  the others repurpose a body/edge drag into the matching Premiere/DaVinci/FCP-style edit. */
export type EditorTool = "select" | "roll" | "slip" | "slide" | "text";

export interface UiState {
  screen: Screen;
  selectedClipIds: number[];
  tool: EditorTool;
  /** Timeline zoom, in pixels per second. */
  pxPerSec: number;
  error: string | null;
  /** Click-to-place-vertex custom shape drawing (canvas-fraction coordinates, same space
   *  ShapeClip::center_x/_y use) -- `null` when not drawing. Mirrors `ui::App::drawing_shape_
   *  points`; kept here rather than as backend state since nothing is committed until the
   *  drawing finishes (Enter, >=3 points) or is cancelled (Escape). */
  drawingShapePoints: [number, number][] | null;
  /** The export queue popover; closed until the user asks for it or starts an export. */
  exportQueueOpen: boolean;
  /** The export options dialog (aspect ratio, loudness target, encoder). */
  exportDialogOpen: boolean;
  /** How many modal dialogs are open; global shortcuts stand down while it is above zero. */
  openDialogs: number;
  /** Timeline in/out marks (seconds) set with I and O; `null` when unset. */
  markIn: number | null;
  markOut: number | null;
  /** Whether dragging clips snaps to edges and markers (toggled with N). */
  snapEnabled: boolean;
  shortcutsOpen: boolean;
  paletteOpen: boolean;
  settingsOpen: boolean;
  watchOpen: boolean;
  /** Program-monitor zoom: fit the window, or a fixed percentage of the canvas size. */
  monitorZoom: "fit" | "50" | "100";
  /** Title/action safe-area and center guides over the monitor. */
  monitorGuides: boolean;
}

export const sessionStore = createStore<SessionView | null>(null);
export const previewStore = createStore<PreviewTick>({
  frameSeq: 0,
  playheadSecs: 0,
  playing: false,
  hasClip: false,
  available: false,
  error: null,
  peakL: 0,
  peakR: 0,
  momentaryLufs: null,
});
export const exportsStore = createStore<ExportJobView[]>([]);
export const layerTemplatesStore = createStore<LayerTemplate[]>([]);
export const transcribingAssetIdStore = createStore<number | null>(null);
export const uiStore = createStore<UiState>({
  screen: "home",
  selectedClipIds: [],
  tool: "select",
  pxPerSec: 40,
  error: null,
  drawingShapePoints: null,
  exportQueueOpen: false,
  exportDialogOpen: false,
  openDialogs: 0,
  markIn: null,
  markOut: null,
  snapEnabled: true,
  shortcutsOpen: false,
  paletteOpen: false,
  settingsOpen: false,
  watchOpen: false,
  monitorZoom: "fit",
  monitorGuides: false,
});

/** An asset being dragged from the media library toward the timeline. */
export interface AssetDrag {
  assetId: number;
  clientX: number;
  clientY: number;
}

export const assetDragStore = createStore<AssetDrag | null>(null);

/** Low enough to see a whole ten-hour timeline in an ordinary window. */
export const MIN_PX_PER_SEC = 0.01;
export const MAX_PX_PER_SEC = 600;

export function patchUi(patch: Partial<UiState>) {
  uiStore.set((prev) => {
    return { ...prev, ...patch };
  });
}

/** Runs a backend call, surfacing a rejection in the error banner instead of dropping it. */
/** Error codes the backend returns instead of a sentence, with the text that explains them. */
const ERROR_CODES: Record<string, TextKey> = {
  assetInUse: "assetInUse",
  pathMissing: "pathMissing",
  outputIsSource: "exportOutputIsSource",
  outputInQueue: "exportOutputInQueue",
  exportPreparationBusy: "exportPreparationBusy",
  exportPreparationCancelled: "exportPreparationCancelled",
  analysisBusy: "analysisBusy",
  analysisStale: "analysisStale",
  projectMissing: "projectMissing",
  projectNameEmpty: "projectNameEmpty",
};

/** What to show for a failed call: the translated text for a known backend code, otherwise the
 *  error as the backend wrote it. */
export function describeError(error: unknown): string {
  const text = String(error);
  const key = ERROR_CODES[text];
  return key ? t(key) : text;
}

export async function run<T>(call: Promise<T>): Promise<T | undefined> {
  try {
    return await call;
  } catch (error) {
    patchUi({ error: describeError(error) });
    return undefined;
  }
}

/** Makes `view` the current session and drops what it no longer has: a changed sequence resets
 *  the selection, and selected clips that were deleted are unselected. */
function adoptSession(view: SessionView) {
  const previous = sessionStore.get();
  if (previous?.rev !== undefined && view.rev !== undefined && view.rev < previous.rev) {
    return;
  }
  sessionStore.set(view);
  const previousId = previous?.project.sequences[previous.project.active_sequence]?.id;
  const activeId = view.project.sequences[view.project.active_sequence]?.id;
  if (
    previousId !== undefined &&
    (previousId !== activeId || previous?.projectEpoch !== view.projectEpoch)
  ) {
    patchUi({ selectedClipIds: [] });
  }
  // Drop selections that no longer exist (undo, delete, project switch).
  const selected = uiStore.get().selectedClipIds;
  if (selected.length === 0) {
    return;
  }
  const ids = new Set(
    view.project.sequences[view.project.active_sequence].timeline.tracks.flatMap((t) => {
      return t.clips.map((c) => {
        return c.id;
      });
    }),
  );
  if (
    selected.some((id) => {
      return !ids.has(id);
    })
  ) {
    patchUi({
      selectedClipIds: selected.filter((id) => {
        return ids.has(id);
      }),
    });
  }
}

let connected = false;

/** Subscribes to backend events once and loads the initial session. */
export async function connect() {
  if (connected) {
    return;
  }
  connected = true;
  await onSession(adoptSession);
  await onSessionPatch((patch) => {
    const current = sessionStore.get();
    if (!current || current.rev !== patch.base) {
      // An update was missed (or none arrived yet): start over from a whole snapshot.
      void run(api.getSession()).then((view) => {
        if (view) {
          adoptSession(view);
        }
      });
      return;
    }
    adoptSession({
      ...current,
      rev: patch.rev,
      projectEpoch: patch.projectEpoch,
      project: applyOps(current.project, patch.ops),
      filePath: patch.filePath,
      canUndo: patch.canUndo,
      canRedo: patch.canRedo,
      dirty: patch.dirty,
    });
  });
  await onPreview((tick) => {
    return previewStore.set(tick);
  });
  await onExports((jobs) => {
    return exportsStore.set(jobs);
  });
  await onImportFailed((fileName) => {
    patchUi({ error: t("importFailed") + ": " + fileName });
  });
  await onAutosaveFailed((message) => {
    patchUi({ error: t("autosaveFailed") + ": " + message });
  });
  await onPreviewRecovery((restarted) => {
    patchUi({ error: t(restarted ? "previewRestarted" : "previewGaveUp") });
  });
  await onSceneCut((count) => {
    if (count === 0) {
      patchUi({ error: t("sceneCutDetectionNone") });
    }
  });
  await onMotionTrack((count) => {
    if (count === 0) {
      patchUi({ error: t("motionTrackNoFramesDecoded") });
    }
  });
  await onAutoReframe((event) => {
    if (!event.subjectFound) {
      patchUi({ error: t("autoReframeNoSubjectFound") });
    }
  });
  await onMatteGeneration((event) => {
    if (event.type === "failed") {
      patchUi({ error: describeError(event.message) });
    }
  });
  await onPrivacyBlur((event) => {
    if (event.type === "failed") {
      patchUi({ error: describeError(event.message) });
    }
  });
  await onTts((event) => {
    if (event.type === "failed") {
      patchUi({ error: describeError(event.message) });
    }
  });
  await onTranscribe((event) => {
    if (event.type === "started") {
      transcribingAssetIdStore.set(event.assetId);
      return;
    }
    transcribingAssetIdStore.set(null);
    if (event.type === "failed") {
      patchUi({ error: describeError(event.message) });
    } else if (event.type === "noSpeech") {
      patchUi({ error: t("transcribeNoSpeechFound") });
    }
  });
  sessionStore.set(await api.getSession());
  const prefs = await api.getPrefs();
  setLocale(localeFromPrefs(prefs.locale));
  layerTemplatesStore.set(prefs.savedLayerTemplates);
  exportsStore.set(await api.listExports());
}

export const exportPreparationStore = createStore(false);
