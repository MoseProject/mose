// The only module that talks to Rust. Command names and argument keys must match the
// `#[tauri::command]` functions registered in src-tauri/src/main.rs (Tauri maps camelCase
// argument keys onto snake_case Rust parameters).

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AudioRole,
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
  PathStatus,
  PrefsView,
  PrivacyBlurEvent,
  RecentProject,
  SessionPatch,
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
  sessionPatch: "session-patch",
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
  importFailed: "import-failed",
  closeRequested: "close-requested",
  autosaveFailed: "autosave-failed",
  previewRecovery: "preview-recovery",
  youtubeDownload: "youtube-download-event",
  updateCheck: "update-check-event",
} as const;

export function onSession(handler: (view: SessionView) => void): Promise<UnlistenFn> {
  return listen<SessionView>(events.session, (e) => {
    return handler(e.payload);
  });
}

export function onSessionPatch(handler: (patch: SessionPatch) => void): Promise<UnlistenFn> {
  return listen<SessionPatch>(events.sessionPatch, (e) => {
    return handler(e.payload);
  });
}

export function onPreview(handler: (tick: PreviewTick) => void): Promise<UnlistenFn> {
  return listen<PreviewTick>(events.preview, (e) => {
    return handler(e.payload);
  });
}

export function onExports(handler: (jobs: ExportJobView[]) => void): Promise<UnlistenFn> {
  return listen<ExportJobView[]>(events.exports, (e) => {
    return handler(e.payload);
  });
}

export function onTranscribe(handler: (event: TranscribeEvent) => void): Promise<UnlistenFn> {
  return listen<TranscribeEvent>(events.transcribe, (e) => {
    return handler(e.payload);
  });
}

export function onSceneCut(handler: (cutCount: number) => void): Promise<UnlistenFn> {
  return listen<number>(events.sceneCut, (e) => {
    return handler(e.payload);
  });
}

export function onMotionTrack(handler: (keyframeCount: number) => void): Promise<UnlistenFn> {
  return listen<number>(events.motionTrack, (e) => {
    return handler(e.payload);
  });
}

export function onAutoReframe(handler: (event: AutoReframeEvent) => void): Promise<UnlistenFn> {
  return listen<AutoReframeEvent>(events.autoReframe, (e) => {
    return handler(e.payload);
  });
}

export function onMatteGeneration(
  handler: (event: MatteGenerationEvent) => void,
): Promise<UnlistenFn> {
  return listen<MatteGenerationEvent>(events.matteGeneration, (e) => {
    return handler(e.payload);
  });
}

export function onPrivacyBlur(handler: (event: PrivacyBlurEvent) => void): Promise<UnlistenFn> {
  return listen<PrivacyBlurEvent>(events.privacyBlur, (e) => {
    return handler(e.payload);
  });
}

export function onTts(handler: (event: TtsEvent) => void): Promise<UnlistenFn> {
  return listen<TtsEvent>(events.tts, (e) => {
    return handler(e.payload);
  });
}

export function onImportFailed(handler: (fileName: string) => void): Promise<UnlistenFn> {
  return listen<{ fileName: string }>(events.importFailed, (e) => {
    return handler(e.payload.fileName);
  });
}

/** Saving the project in the background failed; `message` is the reason. */
export function onAutosaveFailed(handler: (message: string) => void): Promise<UnlistenFn> {
  return listen<string>(events.autosaveFailed, (e) => {
    return handler(e.payload);
  });
}

/** The playback thread crashed; `restarted` is false once it was given up on. */
export function onPreviewRecovery(handler: (restarted: boolean) => void): Promise<UnlistenFn> {
  return listen<{ restarted: boolean }>(events.previewRecovery, (e) => {
    return handler(e.payload.restarted);
  });
}

/** The window was asked to close while work would be lost: unsaved changes and/or exports. */
export function onCloseRequested(
  handler: (request: { dirty: boolean; exporting: boolean }) => void,
): Promise<UnlistenFn> {
  return listen<{ dirty: boolean; exporting: boolean }>(events.closeRequested, (e) => {
    return handler(e.payload);
  });
}

export function onWatchFolder(handler: (event: WatchFolderEvent) => void): Promise<UnlistenFn> {
  return listen<WatchFolderEvent>(events.watchFolder, (e) => {
    return handler(e.payload);
  });
}

export function onUpdateCheck(handler: (event: UpdateCheckEvent) => void): Promise<UnlistenFn> {
  return listen<UpdateCheckEvent>(events.updateCheck, (e) => {
    return handler(e.payload);
  });
}

export function onYoutubeDownload(
  handler: (event: YoutubeDownloadEvent) => void,
): Promise<UnlistenFn> {
  return listen<YoutubeDownloadEvent>(events.youtubeDownload, (e) => {
    return handler(e.payload);
  });
}

// Custom URI schemes are served from http://<scheme>.localhost on Windows (WebView2) and
// <scheme>://localhost elsewhere.
const isWindows = navigator.userAgent.includes("Windows");
const schemeBase = isWindows ? "http://oca.localhost" : "oca://localhost";

export const frameUrl = (seq: number) => {
  return `${schemeBase}/frame/${seq}`;
};
export const thumbnailUrl = (assetId: number) => {
  return `${schemeBase}/thumb/${assetId}`;
};

export const api = {
  setPanelLayout: (libraryWidth: number, inspectorWidth: number, timelineHeight: number) => {
    return invoke<void>("set_panel_layout", { libraryWidth, inspectorWidth, timelineHeight });
  },
  getSession: () => {
    return invoke<SessionView>("get_session");
  },
  newProject: (name: string, sequenceName: string) => {
    return invoke<void>("new_project", { name, sequenceName });
  },
  createProject: (name: string, sequenceName: string) => {
    return invoke<boolean>("create_project", { name, sequenceName });
  },
  openProject: () => {
    return invoke<boolean>("open_project");
  },
  openProjectPath: (path: string) => {
    return invoke<void>("open_project_path", { path });
  },
  saveProject: (saveAs: boolean) => {
    return invoke<boolean>("save_project", { saveAs });
  },
  getPrefs: () => {
    return invoke<PrefsView>("get_prefs");
  },
  setPrefs: (prefs: PrefsView) => {
    return invoke<void>("set_prefs", { prefs });
  },
  getRecentProjects: () => {
    return invoke<RecentProject[]>("get_recent_projects");
  },
  removeRecentProject: (path: string) => {
    return invoke<void>("remove_recent_project", { path });
  },
  restoreAutosave: (path: string) => {
    return invoke<void>("restore_autosave", { path });
  },
  discardAutosave: (path: string) => {
    return invoke<void>("discard_autosave", { path });
  },
  saveLayerTemplate: (name: string, clipIds: number[]) => {
    return invoke<LayerTemplate[]>("save_layer_template", { name, clipIds });
  },
  deleteLayerTemplate: (index: number) => {
    return invoke<LayerTemplate[]>("delete_layer_template", { index });
  },
  applyLayerTemplate: (templateIndex: number, assetIds: (number | null)[]) => {
    return invoke<void>("apply_layer_template", { templateIndex, assetIds });
  },
  undo: () => {
    return invoke<void>("undo");
  },
  redo: () => {
    return invoke<void>("redo");
  },

  importMediaDialog: () => {
    return invoke<void>("import_media_dialog");
  },
  importMedia: (paths: string[]) => {
    return invoke<void>("import_media", { paths });
  },
  toggleFavorite: (assetId: number) => {
    return invoke<void>("toggle_favorite", { assetId });
  },

  quitApp: () => {
    return invoke<void>("quit_app");
  },
  removeAsset: (assetId: number) => {
    return invoke<void>("remove_asset", { assetId });
  },

  addAssetToTimeline: (assetId: number, trackId?: number, startSecs?: number) => {
    return invoke<void>("add_asset_to_timeline", {
      assetId,
      trackId: trackId ?? null,
      startSecs: startSecs ?? null,
    });
  },
  moveClip: (clipId: number, trackId: number, startSecs: number) => {
    return invoke<void>("move_clip", { clipId, trackId, startSecs });
  },
  moveClipGroup: (clipId: number, startSecs: number) => {
    return invoke<void>("move_clip_group", { clipId, startSecs });
  },
  trimClip: (clipId: number, edge: "start" | "end", positionSecs: number) => {
    return invoke<void>("trim_clip", { clipId, edge, positionSecs });
  },
  rollEditClip: (clipId: number, boundarySecs: number) => {
    return invoke<void>("roll_edit_clip", { clipId, boundarySecs });
  },
  rollEditFromStartEdge: (editedClipId: number, boundarySecs: number) => {
    return invoke<void>("roll_edit_from_start_edge", { editedClipId, boundarySecs });
  },
  slipClip: (clipId: number, deltaSecs: number) => {
    return invoke<void>("slip_clip", { clipId, deltaSecs });
  },
  slideClip: (clipId: number, startSecs: number) => {
    return invoke<void>("slide_clip", { clipId, startSecs });
  },
  splitAtPlayhead: (clipIds: number[]) => {
    return invoke<void>("split_at_playhead", { clipIds });
  },
  deleteClips: (clipIds: number[], ripple: boolean) => {
    return invoke<void>("delete_clips", { clipIds, ripple });
  },
  updateClip: (clipId: number, patch: ClipPatch) => {
    return invoke<void>("update_clip", { clipId, patch });
  },
  mergeClipsIntoComposite: (clipIds: number[]) => {
    return invoke<void>("merge_clips_into_composite", { clipIds });
  },
  /** Resolves to the new nested sequence's id, or null if clipId isn't on a Video track. */
  createCompoundClip: (clipId: number, selectedClipIds: number[]) => {
    return invoke<number | null>("create_compound_clip", { clipId, selectedClipIds });
  },
  insertSequenceAsCompoundClip: (sequenceId: number) => {
    return invoke<boolean>("insert_sequence_as_compound_clip", { sequenceId });
  },
  copyClip: (clipId: number) => {
    return invoke<void>("copy_clip", { clipId });
  },
  cutClip: (clipId: number) => {
    return invoke<void>("cut_clip", { clipId });
  },
  pasteClip: () => {
    return invoke<number[]>("paste_clip");
  },
  copyClipFormatting: (clipId: number) => {
    return invoke<void>("copy_clip_formatting", { clipId });
  },
  pasteClipFormatting: (clipId: number) => {
    return invoke<void>("paste_clip_formatting", { clipId });
  },
  applySpeedRampToClip: (clipId: number, startSpeed: number, endSpeed: number, steps: number) => {
    return invoke<void>("apply_speed_ramp_to_clip", { clipId, startSpeed, endSpeed, steps });
  },
  overlayCatalog: () => {
    return invoke<OverlayCatalog>("overlay_catalog");
  },
  /** Resolves to the new clip's id, or null when the target track is locked. */
  addTextClip: (text: string, posX?: number, posY?: number) => {
    return invoke<number | null>("add_text_clip", { text, posX, posY });
  },
  addShapeClip: () => {
    return invoke<number | null>("add_shape_clip");
  },
  addCustomShapeClip: (points: [number, number][]) => {
    return invoke<number | null>("add_custom_shape_clip", { points });
  },
  moveOverlayClip: (clipId: number, trackId: number, startSecs: number) => {
    return invoke<void>("move_overlay_clip", { clipId, trackId, startSecs });
  },
  trimOverlayClip: (clipId: number, edge: "start" | "end", positionSecs: number) => {
    return invoke<void>("trim_overlay_clip", { clipId, edge, positionSecs });
  },
  updateTextClip: (clipId: number, patch: TextClipPatch) => {
    return invoke<void>("update_text_clip", { clipId, patch });
  },
  updateShapeClip: (clipId: number, patch: ShapeClipPatch) => {
    return invoke<void>("update_shape_clip", { clipId, patch });
  },
  addTrack: (kind: TrackKind) => {
    return invoke<void>("add_track", { kind });
  },
  setTrackFlags: (
    trackId: number,
    flags: {
      visible?: boolean;
      locked?: boolean;
      muted?: boolean;
      solo?: boolean;
      audioRole?: AudioRole;
    },
  ) => {
    return invoke<void>("set_track_flags", {
      trackId,
      visible: flags.visible ?? null,
      locked: flags.locked ?? null,
      muted: flags.muted ?? null,
      solo: flags.solo ?? null,
      audioRole: flags.audioRole ?? null,
    });
  },
  getKeybindings: () => {
    return invoke<Record<string, string[]>>("get_keybindings");
  },
  setKeybindings: (keybindings: Record<string, string[]>) => {
    return invoke<Record<string, string[]>>("set_keybindings", { keybindings });
  },
  rippleDeleteRange: (startSecs: number, endSecs: number) => {
    return invoke<void>("ripple_delete_range", { startSecs, endSecs });
  },
  liftRange: (startSecs: number, endSecs: number) => {
    return invoke<void>("lift_range", { startSecs, endSecs });
  },
  pickPath: (kind: "file" | "folder", extensions: string[] = []) => {
    return invoke<string | null>("pick_path", { kind, extensions });
  },
  checkPath: (path: string) => {
    return invoke<PathStatus>("check_path", { path });
  },
  revealInFolder: (path: string) => {
    return invoke<void>("reveal_in_folder", { path });
  },
  addSequence: (name: string) => {
    return invoke<void>("add_sequence", { name });
  },
  selectSequence: (index: number) => {
    return invoke<void>("select_sequence", { index });
  },
  duplicateSequence: (index: number, name: string) => {
    return invoke<void>("duplicate_sequence", { index, name });
  },
  renameSequence: (sequenceId: number, name: string) => {
    return invoke<void>("rename_sequence", { sequenceId, name });
  },
  removeSequence: (sequenceId: number) => {
    return invoke<void>("remove_sequence", { sequenceId });
  },
  moveSequence: (fromIndex: number, targetIndex: number) => {
    return invoke<void>("move_sequence", { fromIndex, targetIndex });
  },
  transcribeAsset: (assetId: number) => {
    return invoke<void>("transcribe_asset", { assetId });
  },
  getTranscript: (assetId: number) => {
    return invoke<TranscriptDocument | null>("get_transcript", { assetId });
  },
  searchTranscripts: (query: string) => {
    return invoke<TranscriptHit[]>("search_transcripts", { query });
  },
  seekToTranscriptWord: (assetId: number, sourceSecs: number) => {
    return invoke<boolean>("seek_to_transcript_word", { assetId, sourceSecs });
  },

  detectSilenceGapsForClip: (clipId: number) => {
    return invoke<SilenceReview | null>("detect_silence_gaps_for_clip", { clipId });
  },
  applySilenceGaps: (trackId: number, gaps: SilenceGap[]) => {
    return invoke<void>("apply_silence_gaps", { trackId, gaps });
  },

  detectSceneCutsForClip: (clipId: number) => {
    return invoke<void>("detect_scene_cuts_for_clip", { clipId });
  },
  detectHighlights: () => {
    return invoke<void>("detect_highlights");
  },
  motionTrackClip: (
    clipId: number,
    centerX: number,
    centerY: number,
    templateWidth: number,
    templateHeight: number,
    searchRadius: number,
  ) => {
    return invoke<void>("motion_track_clip", {
      clipId,
      centerX,
      centerY,
      templateWidth,
      templateHeight,
      searchRadius,
    });
  },
  autoReframeClip: (clipId: number) => {
    return invoke<void>("auto_reframe_clip", { clipId });
  },
  dynamicAutoReframeClip: (clipId: number) => {
    return invoke<void>("dynamic_auto_reframe_clip", { clipId });
  },
  generateMatteForClip: (clipId: number) => {
    return invoke<void>("generate_matte_for_clip", { clipId });
  },
  applyPrivacyBlurToClip: (
    clipId: number,
    centerX: number,
    centerY: number,
    widthFrac: number,
    heightFrac: number,
  ) => {
    return invoke<void>("apply_privacy_blur_to_clip", {
      clipId,
      centerX,
      centerY,
      widthFrac,
      heightFrac,
    });
  },
  generateTts: (text: string) => {
    return invoke<void>("generate_tts", { text });
  },
  listSoundLibrary: () => {
    return invoke<LibraryTrack[]>("list_sound_library");
  },

  pickWatchFolder: () => {
    return invoke<string | null>("pick_watch_folder");
  },
  startWatchingFolder: (path: string) => {
    return invoke<void>("start_watching_folder", { path });
  },
  stopWatchingFolder: () => {
    return invoke<void>("stop_watching_folder");
  },
  addWatchedFileToProject: (watchPath: string, sourcePath: string) => {
    return invoke<void>("add_watched_file_to_project", { watchPath, sourcePath });
  },

  isYtDlpAvailable: () => {
    return invoke<boolean>("is_yt_dlp_available");
  },
  downloadYoutube: (url: string, format: "mp4" | "mp3", quality: string) => {
    return invoke<void>("download_youtube", { url, format, quality });
  },
  cancelYoutubeDownload: () => {
    return invoke<void>("cancel_youtube_download");
  },

  getPendingCrashReview: () => {
    return invoke<CrashReview | null>("get_pending_crash_review");
  },
  sendCrashOnce: () => {
    return invoke<void>("send_crash_once");
  },
  alwaysSendCrash: () => {
    return invoke<void>("always_send_crash");
  },
  dismissCrashReview: () => {
    return invoke<void>("dismiss_crash_review");
  },
  setErrorReportingConsent: (consent: ErrorReportingConsent) => {
    return invoke<void>("set_error_reporting_consent", { consent });
  },
  getQueuedReportCount: () => {
    return invoke<number>("get_queued_report_count");
  },
  deleteQueuedReports: () => {
    return invoke<void>("delete_queued_reports");
  },

  checkForUpdate: () => {
    return invoke<void>("check_for_update");
  },
  installAvailableUpdate: (version: string) => {
    return invoke<void>("install_available_update", { version });
  },
  restartAfterUpdate: () => {
    return invoke<void>("restart_after_update");
  },

  addMarker: (kind: MarkerKind = "Standard") => {
    return invoke<void>("add_marker", { kind });
  },
  removeMarker: (markerId: number) => {
    return invoke<void>("remove_marker", { markerId });
  },
  updateMarker: (
    markerId: number,
    patch: { label?: string; kind?: MarkerKind; completed?: boolean },
  ) => {
    return invoke<void>("update_marker", {
      markerId,
      label: patch.label ?? null,
      kind: patch.kind ?? null,
      completed: patch.completed ?? null,
    });
  },

  seek: (positionSecs: number) => {
    return invoke<void>("preview_seek", { positionSecs });
  },
  step: (deltaFrames: number) => {
    return invoke<void>("preview_step", { deltaFrames });
  },
  setPlaying: (playing: boolean) => {
    return invoke<void>("preview_set_playing", { playing });
  },

  setExportSettings: (targetLufs: number, aspectRatio: ExportAspectRatio) => {
    return invoke<void>("set_export_settings", { targetLufs, aspectRatio });
  },
  pickExportPath: () => {
    return invoke<{ path: string; exists: boolean } | null>("pick_export_path");
  },
  startExport: (path: string, rename: boolean) => {
    return invoke<number>("start_export", { path, rename });
  },
  exportActiveSequenceSrt: () => {
    return invoke<boolean>("export_active_sequence_srt");
  },
  cancelExport: (jobId: number) => {
    return invoke<void>("cancel_export", { jobId });
  },
  pauseExport: (jobId: number) => {
    return invoke<void>("pause_export", { jobId });
  },
  resumeExport: (jobId: number) => {
    return invoke<void>("resume_export", { jobId });
  },
  removeExport: (jobId: number) => {
    return invoke<void>("remove_export", { jobId });
  },
  matchExportLoudness: (targetLufs: number) => {
    return invoke<void>("match_export_loudness", { targetLufs });
  },
  setGpuEncoderPreference: (preference: string) => {
    return invoke<void>("set_gpu_encoder_preference", { preference });
  },
  getGpuEncoderPreference: () => {
    return invoke<string>("get_gpu_encoder_preference");
  },
  listExports: () => {
    return invoke<ExportJobView[]>("list_exports");
  },
};

/** Asks for an output file and queues the active sequence. An existing file is overwritten
 *  only after the user confirms; otherwise the export goes to an auto-renamed sibling. */
export async function exportActiveSequence(
  confirmOverwrite: () => boolean | Promise<boolean>,
): Promise<void> {
  const target = await api.pickExportPath();
  if (!target) {
    return;
  }
  const replace = target.exists ? await confirmOverwrite() : true;
  await api.startExport(target.path, target.exists && !replace);
}
/** One video frame of an asset at a source time, for the timeline's clip strips. */
export const frameAtUrl = (assetId: number, millis: number) => {
  return `${schemeBase}/thumb/${assetId}/${millis}`;
};
/** The overview waveform stored with the project, same layout as `waveformUrl`; instant, since
 *  nothing is decoded. */
export const coarseWaveformUrl = (assetId: number) => {
  return `${schemeBase}/wave/${assetId}/coarse`;
};
/** The asset's detailed waveform: little-endian f32 (min, max) pairs across its duration. */
export const waveformUrl = (assetId: number) => {
  return `${schemeBase}/wave/${assetId}`;
};
