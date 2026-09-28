// Webview-side state. Authoritative project data arrives from Rust (`session-changed`); this
// module only caches it and adds transient UI state (screen, selection, zoom) that is never
// serialized -- the same persistent/transient split the egui shell keeps between `avcore`
// types and `ui::App`. Preview ticks live in their own store so 60 Hz playhead updates only
// re-render the components that read them.

import { useSyncExternalStore } from "react";
import { api, onExports, onPreview, onSession } from "./ipc";
import type { ExportJobView, PreviewTick, SessionView } from "./types";

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

export type Screen = "home" | "editor";

export interface UiState {
  screen: Screen;
  selectedClipIds: number[];
  /** Timeline zoom, in pixels per second. */
  pxPerSec: number;
  error: string | null;
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
export const uiStore = createStore<UiState>({
  screen: "home",
  selectedClipIds: [],
  pxPerSec: 40,
  error: null,
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
    sessionStore.set(view);
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
  sessionStore.set(await api.getSession());
  exportsStore.set(await api.listExports());
}
