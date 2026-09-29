// The only module that talks to Rust. Command names and argument keys must match the
// `#[tauri::command]` functions registered in src-tauri/src/main.rs (Tauri maps camelCase
// argument keys onto snake_case Rust parameters).

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AutoReframeEvent,
  ClipPatch,
  ExportAspectRatio,
  ExportJobView,
  LayerTemplate,
  LibraryTrack,
  MarkerKind,
  MatteGenerationEvent,
  OverlayCatalog,
  PreviewTick,
  PrefsView,
  PrivacyBlurEvent,
  RecentProject,
  SessionView,
  ShapeClipPatch,
  TextClipPatch,
  TrackKind,
  SilenceGap,
  SilenceReview,
  TranscribeEvent,
  TranscriptDocument,
  TranscriptHit,
  TtsEvent,
  WatchFolderEvent,
  YoutubeDownloadEvent,
  CrashReview,
  ErrorReportingConsent,
  UpdateCheckEvent,
} from "./types";

export const events = {
  session: "session-changed",
  preview: "preview-tick",
  exports: "export-jobs",
  transcribe: "transcribe-event",
  sceneCut: "scene-cut-event",
  motionTrack: "motion-track-event",
  autoReframe: "auto-reframe-event",
  matteGeneration: "matte-generation-event",
  privacyBlur: "privacy-blur-event",
  tts: "tts-event",
  watchFolder: "watch-folder-event",
  youtubeDownload: "youtube-download-event",
  updateCheck: "update-check-event",
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

export function onTranscribe(handler: (event: TranscribeEvent) => void): Promise<UnlistenFn> {
  return listen<TranscribeEvent>(events.transcribe, (e) => handler(e.payload));
}

export function onSceneCut(handler: (cutCount: number) => void): Promise<UnlistenFn> {
  return listen<number>(events.sceneCut, (e) => handler(e.payload));
}

export function onMotionTrack(handler: (keyframeCount: number) => void): Promise<UnlistenFn> {
  return listen<number>(events.motionTrack, (e) => handler(e.payload));
}

export function onAutoReframe(handler: (event: AutoReframeEvent) => void): Promise<UnlistenFn> {
  return listen<AutoReframeEvent>(events.autoReframe, (e) => handler(e.payload));
}

export function onMatteGeneration(
  handler: (event: MatteGenerationEvent) => void,
): Promise<UnlistenFn> {
  return listen<MatteGenerationEvent>(events.matteGeneration, (e) => handler(e.payload));
}

export function onPrivacyBlur(handler: (event: PrivacyBlurEvent) => void): Promise<UnlistenFn> {
  return listen<PrivacyBlurEvent>(events.privacyBlur, (e) => handler(e.payload));
}

export function onTts(handler: (event: TtsEvent) => void): Promise<UnlistenFn> {
  return listen<TtsEvent>(events.tts, (e) => handler(e.payload));
}

export function onWatchFolder(handler: (event: WatchFolderEvent) => void): Promise<UnlistenFn> {
  return listen<WatchFolderEvent>(events.watchFolder, (e) => handler(e.payload));
}

export function onUpdateCheck(handler: (event: UpdateCheckEvent) => void): Promise<UnlistenFn> {
  return listen<UpdateCheckEvent>(events.updateCheck, (e) => handler(e.payload));
}

export function onYoutubeDownload(
  handler: (event: YoutubeDownloadEvent) => void,
): Promise<UnlistenFn> {
  return listen<YoutubeDownloadEvent>(events.youtubeDownload, (e) => handler(e.payload));
}

// Custom URI schemes are served from http://<scheme>.localhost on Windows (WebView2) and
// <scheme>://localhost elsewhere.
const isWindows = navigator.userAgent.includes("Windows");
const schemeBase = isWindows ? "http://oca.localhost" : "oca://localhost";

export const frameUrl = (seq: number) => `${schemeBase}/frame/${seq}`;
export const thumbnailUrl = (assetId: number) => `${schemeBase}/thumb/${assetId}`;

export const api = {
  setPanelLayout: (libraryWidth: number, inspectorWidth: number, timelineHeight: number) =>
    invoke<void>("set_panel_layout", { libraryWidth, inspectorWidth, timelineHeight }),
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
  saveLayerTemplate: (name: string, clipIds: number[]) =>
    invoke<LayerTemplate[]>("save_layer_template", { name, clipIds }),
  deleteLayerTemplate: (index: number) =>
    invoke<LayerTemplate[]>("delete_layer_template", { index }),
  applyLayerTemplate: (templateIndex: number, assetIds: (number | null)[]) =>
    invoke<void>("apply_layer_template", { templateIndex, assetIds }),
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
  moveClipGroup: (clipId: number, startSecs: number) =>
    invoke<void>("move_clip_group", { clipId, startSecs }),
  trimClip: (clipId: number, edge: "start" | "end", positionSecs: number) =>
    invoke<void>("trim_clip", { clipId, edge, positionSecs }),
  rollEditClip: (clipId: number, boundarySecs: number) =>
    invoke<void>("roll_edit_clip", { clipId, boundarySecs }),
  rollEditFromStartEdge: (editedClipId: number, boundarySecs: number) =>
    invoke<void>("roll_edit_from_start_edge", { editedClipId, boundarySecs }),
  slipClip: (clipId: number, deltaSecs: number) => invoke<void>("slip_clip", { clipId, deltaSecs }),
  slideClip: (clipId: number, startSecs: number) => invoke<void>("slide_clip", { clipId, startSecs }),
  splitAtPlayhead: (clipIds: number[]) => invoke<void>("split_at_playhead", { clipIds }),
  deleteClips: (clipIds: number[], ripple: boolean) =>
    invoke<void>("delete_clips", { clipIds, ripple }),
  updateClip: (clipId: number, patch: ClipPatch) =>
    invoke<void>("update_clip", { clipId, patch }),
  mergeClipsIntoComposite: (clipIds: number[]) =>
    invoke<void>("merge_clips_into_composite", { clipIds }),
  copyClip: (clipId: number) => invoke<void>("copy_clip", { clipId }),
  cutClip: (clipId: number) => invoke<void>("cut_clip", { clipId }),
  pasteClip: () => invoke<number[]>("paste_clip"),
  copyClipFormatting: (clipId: number) => invoke<void>("copy_clip_formatting", { clipId }),
  pasteClipFormatting: (clipId: number) => invoke<void>("paste_clip_formatting", { clipId }),
  overlayCatalog: () => invoke<OverlayCatalog>("overlay_catalog"),
  /** Resolves to the new clip's id, or null when the target track is locked. */
  addTextClip: (text: string) => invoke<number | null>("add_text_clip", { text }),
  addShapeClip: () => invoke<number | null>("add_shape_clip"),
  moveOverlayClip: (clipId: number, trackId: number, startSecs: number) =>
    invoke<void>("move_overlay_clip", { clipId, trackId, startSecs }),
  trimOverlayClip: (clipId: number, edge: "start" | "end", positionSecs: number) =>
    invoke<void>("trim_overlay_clip", { clipId, edge, positionSecs }),
  updateTextClip: (clipId: number, patch: TextClipPatch) =>
    invoke<void>("update_text_clip", { clipId, patch }),
  updateShapeClip: (clipId: number, patch: ShapeClipPatch) =>
    invoke<void>("update_shape_clip", { clipId, patch }),
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
  transcribeAsset: (assetId: number) => invoke<void>("transcribe_asset", { assetId }),
  getTranscript: (assetId: number) =>
    invoke<TranscriptDocument | null>("get_transcript", { assetId }),
  searchTranscripts: (query: string) =>
    invoke<TranscriptHit[]>("search_transcripts", { query }),
  seekToTranscriptWord: (assetId: number, sourceSecs: number) =>
    invoke<boolean>("seek_to_transcript_word", { assetId, sourceSecs }),

  detectSilenceGapsForClip: (clipId: number) =>
    invoke<SilenceReview | null>("detect_silence_gaps_for_clip", { clipId }),
  applySilenceGaps: (trackId: number, gaps: SilenceGap[]) =>
    invoke<void>("apply_silence_gaps", { trackId, gaps }),

  detectSceneCutsForClip: (clipId: number) =>
    invoke<void>("detect_scene_cuts_for_clip", { clipId }),
  detectHighlights: () => invoke<void>("detect_highlights"),
  motionTrackClip: (
    clipId: number,
    centerX: number,
    centerY: number,
    templateWidth: number,
    templateHeight: number,
    searchRadius: number,
  ) =>
    invoke<void>("motion_track_clip", {
      clipId,
      centerX,
      centerY,
      templateWidth,
      templateHeight,
      searchRadius,
    }),
  autoReframeClip: (clipId: number) => invoke<void>("auto_reframe_clip", { clipId }),
  dynamicAutoReframeClip: (clipId: number) => invoke<void>("dynamic_auto_reframe_clip", { clipId }),
  generateMatteForClip: (clipId: number) => invoke<void>("generate_matte_for_clip", { clipId }),
  applyPrivacyBlurToClip: (
    clipId: number,
    centerX: number,
    centerY: number,
    widthFrac: number,
    heightFrac: number,
  ) =>
    invoke<void>("apply_privacy_blur_to_clip", {
      clipId,
      centerX,
      centerY,
      widthFrac,
      heightFrac,
    }),
  generateTts: (text: string) => invoke<void>("generate_tts", { text }),
  listSoundLibrary: () => invoke<LibraryTrack[]>("list_sound_library"),

  pickWatchFolder: () => invoke<string | null>("pick_watch_folder"),
  startWatchingFolder: (path: string) => invoke<void>("start_watching_folder", { path }),
  stopWatchingFolder: () => invoke<void>("stop_watching_folder"),
  addWatchedFileToProject: (watchPath: string, sourcePath: string) =>
    invoke<void>("add_watched_file_to_project", { watchPath, sourcePath }),

  isYtDlpAvailable: () => invoke<boolean>("is_yt_dlp_available"),
  downloadYoutube: (url: string, format: "mp4" | "mp3", quality: string) =>
    invoke<void>("download_youtube", { url, format, quality }),
  cancelYoutubeDownload: () => invoke<void>("cancel_youtube_download"),

  getPendingCrashReview: () => invoke<CrashReview | null>("get_pending_crash_review"),
  sendCrashOnce: () => invoke<void>("send_crash_once"),
  alwaysSendCrash: () => invoke<void>("always_send_crash"),
  dismissCrashReview: () => invoke<void>("dismiss_crash_review"),
  setErrorReportingConsent: (consent: ErrorReportingConsent) =>
    invoke<void>("set_error_reporting_consent", { consent }),
  getQueuedReportCount: () => invoke<number>("get_queued_report_count"),
  deleteQueuedReports: () => invoke<void>("delete_queued_reports"),

  checkForUpdate: () => invoke<void>("check_for_update"),
  installAvailableUpdate: (version: string) =>
    invoke<void>("install_available_update", { version }),
  restartAfterUpdate: () => invoke<void>("restart_after_update"),

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
