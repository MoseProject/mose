// Webview-side state. Authoritative project data arrives from Rust (`session-changed`); this
// module only caches it and adds transient UI state (screen, selection, zoom) that is never
// serialized -- the same persistent/transient split the egui shell keeps between `avcore`
// types and `ui::App`. Preview ticks live in their own store so 60 Hz playhead updates only
// re-render the components that read them.

import { useSyncExternalStore } from "react";
import {
  api,
  onAutoReframe,
  onExports,
  onMatteGeneration,
  onMotionTrack,
  onPreview,
  onPrivacyBlur,
  onSceneCut,
  onSession,
  onTranscribe,
  onTts,
} from "./ipc";
import { localeFromPrefs, setLocale, t } from "./i18n";
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
    get: () => value,
    set: (next) => {
      const resolved = typeof next === "function" ? (next as (prev: T) => T)(value) : next;
      if (Object.is(resolved, value)) return;
      value = resolved;
      listeners.forEach((l) => l());
    },
    subscribe: (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}

export function useStore<T>(store: Store<T>): T {
  return useSyncExternalStore(store.subscribe, store.get);
}

export type Screen = "home" | "editor" | "prefs" | "watch";

/** Timeline edit tool -- egui's `EditorTool`. "select" is the default drag-to-move/trim mode;
 *  the others repurpose a body/edge drag into the matching Premiere/DaVinci/FCP-style edit. */
export type EditorTool = "select" | "roll" | "slip" | "slide";

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
});

/** An asset being dragged from the media library toward the timeline. */
export interface AssetDrag {
  assetId: number;
  clientX: number;
  clientY: number;
}

export const assetDragStore = createStore<AssetDrag | null>(null);

export const MIN_PX_PER_SEC = 2;
export const MAX_PX_PER_SEC = 600;

export function patchUi(patch: Partial<UiState>) {
  uiStore.set((prev) => ({ ...prev, ...patch }));
}

/** Runs a backend call, surfacing a rejection in the error banner instead of dropping it. */
export async function run<T>(call: Promise<T>): Promise<T | undefined> {
  try {
    return await call;
  } catch (error) {
    patchUi({ error: String(error) });
    return undefined;
  }
}

let connected = false;

/** Subscribes to backend events once and loads the initial session. */
export async function connect() {
  if (connected) return;
  connected = true;
  await onSession((view) => {
    const previous = sessionStore.get();
    sessionStore.set(view);
    const previousId = previous?.project.sequences[previous.project.active_sequence]?.id;
    const activeId = view.project.sequences[view.project.active_sequence]?.id;
    if (previousId !== undefined && previousId !== activeId) patchUi({ selectedClipIds: [] });
    // Drop selections that no longer exist (undo, delete, project switch).
    const ids = new Set(
      view.project.sequences[view.project.active_sequence].timeline.tracks.flatMap((t) =>
        t.clips.map((c) => c.id),
      ),
    );
    const selected = uiStore.get().selectedClipIds;
    if (selected.some((id) => !ids.has(id))) {
      patchUi({ selectedClipIds: selected.filter((id) => ids.has(id)) });
    }
  });
  await onPreview((tick) => previewStore.set(tick));
  await onExports((jobs) => exportsStore.set(jobs));
  await onSceneCut((count) => {
    if (count === 0) patchUi({ error: t("sceneCutDetectionNone") });
  });
  await onMotionTrack((count) => {
    if (count === 0) patchUi({ error: t("motionTrackNoFramesDecoded") });
  });
  await onAutoReframe((event) => {
    if (!event.subjectFound) patchUi({ error: t("autoReframeNoSubjectFound") });
  });
  await onMatteGeneration((event) => {
    if (event.type === "failed") patchUi({ error: event.message });
  });
  await onPrivacyBlur((event) => {
    if (event.type === "failed") patchUi({ error: event.message });
  });
  await onTts((event) => {
    if (event.type === "failed") patchUi({ error: event.message });
  });
  await onTranscribe((event) => {
    if (event.type === "started") {
      transcribingAssetIdStore.set(event.assetId);
      return;
    }
    transcribingAssetIdStore.set(null);
    if (event.type === "failed") {
      patchUi({ error: event.message });
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
