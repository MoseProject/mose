// The only module that talks to Rust. Command names and argument keys must match the
// `#[tauri::command]` functions registered in src-tauri/src/main.rs (Tauri maps camelCase
// argument keys onto snake_case Rust parameters).

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ClipPatch,
  ExportAspectRatio,
  ExportJobView,
  MarkerKind,
  PreviewTick,
  PrefsView,
  RecentProject,
  SessionView,
  TrackKind,
} from "./types";

export const events = {
  session: "session-changed",
  preview: "preview-tick",
  exports: "export-jobs",
} as const;

export function onSession(handler: (view: SessionView) => void): Promise<UnlistenFn> {
  return listen<SessionView>(events.session, (e) => handler(e.payload));
}

export function onPreview(handler: (tick: PreviewTick) => void): Promise<UnlistenFn> {
  return listen<PreviewTick>(events.preview, (e) => handler(e.payload));
}

export function onExports(handler: (jobs: ExportJobView[]) => void): Promise<UnlistenFn> {
  return listen<ExportJobView[]>(events.exports, (e) => handler(e.payload));
}

// Custom URI schemes are served from http://<scheme>.localhost on Windows (WebView2) and
// <scheme>://localhost elsewhere.
const isWindows = navigator.userAgent.includes("Windows");
const schemeBase = isWindows ? "http://oca.localhost" : "oca://localhost";

export const frameUrl = (seq: number) => `${schemeBase}/frame/${seq}`;
export const thumbnailUrl = (assetId: number) => `${schemeBase}/thumb/${assetId}`;

export const api = {
  getSession: () => invoke<SessionView>("get_session"),
  newProject: (name: string, sequenceName: string) =>
    invoke<void>("new_project", { name, sequenceName }),
  openProject: () => invoke<boolean>("open_project"),
  openProjectPath: (path: string) => invoke<void>("open_project_path", { path }),
  saveProject: (saveAs: boolean) => invoke<boolean>("save_project", { saveAs }),
  getPrefs: () => invoke<PrefsView>("get_prefs"),
  setPrefs: (prefs: PrefsView) => invoke<void>("set_prefs", { prefs }),
  getRecentProjects: () => invoke<RecentProject[]>("get_recent_projects"),
  removeRecentProject: (path: string) => invoke<void>("remove_recent_project", { path }),
  restoreAutosave: (path: string) => invoke<void>("restore_autosave", { path }),
  discardAutosave: (path: string) => invoke<void>("discard_autosave", { path }),
  undo: () => invoke<void>("undo"),
  redo: () => invoke<void>("redo"),

  importMediaDialog: () => invoke<void>("import_media_dialog"),
  importMedia: (paths: string[]) => invoke<void>("import_media", { paths }),
  toggleFavorite: (assetId: number) => invoke<void>("toggle_favorite", { assetId }),

  addAssetToTimeline: (assetId: number, trackId?: number, startSecs?: number) =>
    invoke<void>("add_asset_to_timeline", {
      assetId,
      trackId: trackId ?? null,
      startSecs: startSecs ?? null,
    }),
  moveClip: (clipId: number, trackId: number, startSecs: number) =>
    invoke<void>("move_clip", { clipId, trackId, startSecs }),
  trimClip: (clipId: number, edge: "start" | "end", positionSecs: number) =>
    invoke<void>("trim_clip", { clipId, edge, positionSecs }),
  splitAtPlayhead: (clipIds: number[]) => invoke<void>("split_at_playhead", { clipIds }),
  deleteClips: (clipIds: number[], ripple: boolean) =>
    invoke<void>("delete_clips", { clipIds, ripple }),
  updateClip: (clipId: number, patch: ClipPatch) =>
    invoke<void>("update_clip", { clipId, patch }),
  addTrack: (kind: TrackKind) => invoke<void>("add_track", { kind }),
  setTrackFlags: (trackId: number, flags: { visible?: boolean; locked?: boolean }) =>
    invoke<void>("set_track_flags", {
      trackId,
      visible: flags.visible ?? null,
      locked: flags.locked ?? null,
    }),
  addSequence: (name: string) => invoke<void>("add_sequence", { name }),
  selectSequence: (index: number) => invoke<void>("select_sequence", { index }),
  duplicateSequence: (index: number, name: string) => invoke<void>("duplicate_sequence", { index, name }),
  renameSequence: (sequenceId: number, name: string) => invoke<void>("rename_sequence", { sequenceId, name }),
  removeSequence: (sequenceId: number) => invoke<void>("remove_sequence", { sequenceId }),
  moveSequence: (fromIndex: number, targetIndex: number) => invoke<void>("move_sequence", { fromIndex, targetIndex }),
  addMarker: (kind: MarkerKind = "Standard") => invoke<void>("add_marker", { kind }),
  removeMarker: (markerId: number) => invoke<void>("remove_marker", { markerId }),
  updateMarker: (markerId: number, patch: { label?: string; kind?: MarkerKind; completed?: boolean }) => invoke<void>("update_marker", {
    markerId, label: patch.label ?? null, kind: patch.kind ?? null, completed: patch.completed ?? null,
  }),

  seek: (positionSecs: number) => invoke<void>("preview_seek", { positionSecs }),
  step: (deltaFrames: number) => invoke<void>("preview_step", { deltaFrames }),
  setPlaying: (playing: boolean) => invoke<void>("preview_set_playing", { playing }),
  setHardwareDecode: (enabled: boolean) =>
    invoke<void>("preview_set_hardware_decode", { enabled }),

  setExportSettings: (targetLufs: number, aspectRatio: ExportAspectRatio) =>
    invoke<void>("set_export_settings", { targetLufs, aspectRatio }),
  pickExportPath: () =>
    invoke<{ path: string; exists: boolean } | null>("pick_export_path"),
  startExport: (path: string, rename: boolean) =>
    invoke<number>("start_export", { path, rename }),
  exportActiveSequenceSrt: () => invoke<boolean>("export_active_sequence_srt"),
  cancelExport: (jobId: number) => invoke<void>("cancel_export", { jobId }),
  pauseExport: (jobId: number) => invoke<void>("pause_export", { jobId }),
  resumeExport: (jobId: number) => invoke<void>("resume_export", { jobId }),
  removeExport: (jobId: number) => invoke<void>("remove_export", { jobId }),
  matchExportLoudness: (targetLufs: number) => invoke<void>("match_export_loudness", { targetLufs }),
  setGpuEncoderPreference: (preference: string) => invoke<void>("set_gpu_encoder_preference", { preference }),
  getGpuEncoderPreference: () => invoke<string>("get_gpu_encoder_preference"),
  listExports: () => invoke<ExportJobView[]>("list_exports"),
};

/** Asks for an output file and queues the active sequence. An existing file is overwritten
 *  only after the user confirms; otherwise the export goes to an auto-renamed sibling. */
export async function exportActiveSequence(confirmOverwrite: () => boolean): Promise<void> {
  const target = await api.pickExportPath();
  if (!target) return;
  await api.startExport(target.path, target.exists && !confirmOverwrite());
}
