// TypeScript mirror of the JSON the Rust side emits. `Project` and friends serialize with
// avcore's own serde derives (snake_case field names, externally tagged enums); the shell's
// own view types (`SessionView`, `PreviewTick`, `ExportJobView`) are camelCase. Only the fields
// the webview reads are declared -- extra fields are ignored at runtime.

import type { PatchOp } from "./sessionPatch";

export type MediaKind = "Video" | "Audio";
export type TrackKind = "Video" | "Audio" | "Text" | "Shape";
export interface Keyframe<T> {
  time_fraction: number;
  value: T;
}
export interface Position {
  x: number;
  y: number;
}
export type MaskShape = "None" | "Circle" | "RoundedRect";
export type TransitionType = "None" | "Fade" | "HardCut" | "Slide" | "Zoom" | "Dissolve";
/** Industry-standard compositing-mode names, shown untranslated regardless of locale -- mirrors
 *  `crates/ui/src/screens/editor/properties_panel/keyframe_editors.rs`'s `blend_mode_label`. */
export const BLEND_MODES = [
  "Normal",
  "Addition",
  "And",
  "Average",
  "Burn",
  "Darken",
  "Difference",
  "GrainExtract",
  "Divide",
  "Dodge",
  "Exclusion",
  "HardLight",
  "Lighten",
  "Multiply",
  "Negation",
  "Or",
  "Overlay",
  "Phoenix",
  "PinLight",
  "Reflect",
  "Screen",
  "SoftLight",
  "Subtract",
  "VividLight",
  "Xor",
  "HardMix",
  "LinearLight",
  "Glow",
  "GrainMerge",
  "Multiply128",
  "Heat",
  "Freeze",
  "Extremity",
  "SoftDifference",
  "Geometric",
  "Harmonic",
  "Bleach",
  "Stain",
  "Interpolate",
  "HardOverlay",
] as const;
export type BlendMode = (typeof BLEND_MODES)[number];
export type AudioRole = "Unspecified" | "GameAudio" | "Mic" | "Music";
export type ExportAspectRatio = "Original" | "Landscape" | "Portrait" | "Square";
/** What the export dialog asks beyond the sequence's settings; absent fields keep the
 *  source-matched defaults. */
export interface ExportOptions {
  bitrateMbps?: number;
  maxHeight?: number;
  /** `false` exports without normalizing the loudness; absent follows the app setting. */
  normalize?: boolean;
}
/** Size, source-matched bitrate and length an export would have before any option. */
export interface ExportCanvas {
  width: number;
  height: number;
  bitrateMbps: number;
  durationSecs: number;
}
export type MarkerKind = "Standard" | "ToDo" | "Chapter" | "Highlight";

export interface LoudnessMetrics {
  integrated_lufs: number;
  true_peak_dbtp: number;
  loudness_range_lu: number;
}

export interface MediaAsset {
  id: number;
  file_name: string;
  source_path: string;
  kind: MediaKind;
  has_audio: boolean;
  duration_secs: number;
  codec: string;
  source_bitrate_mbps: number;
  resolution: [number, number] | null;
  fps: number | null;
  sample_rate_khz: number | null;
  loudness: LoudnessMetrics | null;
  /** The asset has a coarse waveform; its peaks are fetched from `coarseWaveformUrl`. */
  has_waveform: boolean;
  favorited: boolean;
  /** What the file says about its pixels and audio streams, read on import. */
  streams: StreamInfo;
  hdr_handling: HdrHandling;
  /** For an audio asset made from one track of a multi-track recording: where it came from. Null
   *  for every other asset. */
  audio_stream: { origin: string; stream_index: number } | null;
}

export type ColorTransfer = "Unspecified" | "Bt709" | "Srgb" | "Pq" | "Hlg" | "Other";
export type HdrHandling = "Auto" | "ToneMap" | "TreatAsSdr";

export interface ColorInfo {
  bit_depth: number;
  matrix: "Unspecified" | "Bt601" | "Bt709" | "Bt2020" | "Other";
  primaries: "Unspecified" | "Bt601" | "Bt709" | "Bt2020" | "Other";
  transfer: ColorTransfer;
  full_range: boolean;
}

export interface AudioStreamInfo {
  stream_index: number;
  channels: number;
  sample_rate_hz: number;
  codec: string;
  title: string;
  language: string;
}

export interface StreamInfo {
  color: ColorInfo | null;
  audio: AudioStreamInfo[];
}

export interface ClipInstance {
  id: number;
  asset_id: number;
  start_secs: number;
  source_in_secs: number;
  source_out_secs: number;
  nested_sequence_id: number | null;
  color_label: [number, number, number] | null;
  gain_db: number;
  frozen: boolean;
  speed_factor: number;
  speed_ramp_end_factor: number | null;
  flipped_h: boolean;
  brightness: number;
  contrast: number;
  saturation: number;
  voice_cleanup_enabled: boolean;
  crop_x: number;
  crop_y: number;
  crop_w: number;
  crop_h: number;
  mask_shape: MaskShape;
  mask_corner_radius: number;
  transition_in: TransitionType;
  transition_duration_secs: number;
  fade_in_secs: number;
  fade_out_secs: number;
  blend_mode: BlendMode;
  position_keyframes: Keyframe<Position>[];
  layer_scale_x: number;
  layer_scale_y: number;
  scale_keyframes: Keyframe<number>[];
  rotation_keyframes: Keyframe<number>[];
  opacity_keyframes: Keyframe<number>[];
  gain_keyframes: Keyframe<number>[];
  brightness_keyframes: Keyframe<number>[];
  contrast_keyframes: Keyframe<number>[];
  saturation_keyframes: Keyframe<number>[];
  crop_x_keyframes: Keyframe<number>[];
  crop_y_keyframes: Keyframe<number>[];
  crop_w_keyframes: Keyframe<number>[];
  crop_h_keyframes: Keyframe<number>[];
}

export interface TextClip {
  id: number;
  start_secs: number;
  duration_secs: number;
  text: string;
  font_size: number;
  font_family: string;
  font_style: TextFontStyle;
  /** `wght` axis value (100..=900); only meaningful for a variable-source family. */
  font_weight: number | null;
  color_rgba: Rgba;
  background_rgba: Rgba;
  background_padding: number;
  background_corner_radius: number;
  direction: TextDirection;
  pos_x: number;
  pos_y: number;
  pos_x_keyframes: Keyframe<number>[];
  pos_y_keyframes: Keyframe<number>[];
  scale_keyframes: Keyframe<number>[];
  rotation_keyframes: Keyframe<number>[];
  opacity_keyframes: Keyframe<number>[];
  /** Per-word timing from transcription; word highlight only applies when non-empty. */
  words: unknown[];
  highlight_enabled: boolean;
  highlight_color_rgba: Rgba;
  text_align: TextAlign;
}

export interface ShapeClip {
  id: number;
  start_secs: number;
  duration_secs: number;
  shape_kind: ShapeKind;
  center_x: number;
  center_y: number;
  center_x_keyframes: Keyframe<number>[];
  center_y_keyframes: Keyframe<number>[];
  width: number;
  height: number;
  width_keyframes: Keyframe<number>[];
  height_keyframes: Keyframe<number>[];
  rotation_deg: number;
  rotation_keyframes: Keyframe<number>[];
  stroke_thickness_px: number;
  color_rgba: Rgba;
}

/** `avcore::timeline::ShapeKind` as serde_json emits it. */
export type ShapeKind = "Ellipse" | { Polygon: [number, number][] };

export type Rgba = [number, number, number, number];
export type TextFontStyle = "Regular" | "Bold";
export type TextAlign = "Auto" | "Left" | "Center" | "Right" | "Start" | "End";
export type TextDirection = "Auto" | "Ltr" | "Rtl";
export type ShapePreset = "Ellipse" | "Rectangle" | "Triangle" | "Trapezoid" | "Arrow" | "Custom";

export interface TextClipPatch {
  text?: string;
  fontFamily?: string;
  fontStyle?: TextFontStyle;
  fontSize?: number;
  fontWeight?: number;
  colorRgba?: Rgba;
  backgroundRgba?: Rgba;
  backgroundPadding?: number;
  backgroundCornerRadius?: number;
  direction?: TextDirection;
  textAlign?: TextAlign;
  posX?: number;
  posY?: number;
  posXKeyframes?: Keyframe<number>[];
  posYKeyframes?: Keyframe<number>[];
  scaleKeyframes?: Keyframe<number>[];
  rotationKeyframes?: Keyframe<number>[];
  opacityKeyframes?: Keyframe<number>[];
  highlightEnabled?: boolean;
  highlightColorRgba?: Rgba;
  startSecs?: number;
  durationSecs?: number;
}

export interface ShapeClipPatch {
  shapeKind?: ShapePreset;
  /** Shape-local unit-square vertices -- see `ShapeClip.shape_kind`'s `Polygon` variant. */
  polygonVertices?: [number, number][];
  colorRgb?: [number, number, number];
  opacity?: number;
  centerX?: number;
  centerY?: number;
  centerXKeyframes?: Keyframe<number>[];
  centerYKeyframes?: Keyframe<number>[];
  width?: number;
  height?: number;
  widthKeyframes?: Keyframe<number>[];
  heightKeyframes?: Keyframe<number>[];
  rotationDeg?: number;
  rotationKeyframes?: Keyframe<number>[];
  strokeThicknessPx?: number;
  startSecs?: number;
  durationSecs?: number;
}

export type FontCategory =
  "Sans" | "Display" | "Serif" | "Handwritten" | "Monospace" | "International";

/** `commands::overlays::OverlayCatalog`: static font/shape data owned by `core`. */
export interface OverlayCatalog {
  fonts: {
    id: string;
    displayName: string;
    category: FontCategory;
    supportsBold: boolean;
    isVariable: boolean;
  }[];
  shapePresets: { preset: ShapePreset; kind: ShapeKind }[];
}

export interface Track {
  id: number;
  name: string;
  kind: TrackKind;
  clips: ClipInstance[];
  text_clips: TextClip[];
  shape_clips: ShapeClip[];
  visible: boolean;
  audio_role: AudioRole;
  color_label: [number, number, number] | null;
  locked: boolean;
  muted: boolean;
  solo: boolean;
  /** The track's fader (dB) and stereo balance (-1 left to 1 right). */
  mix: TrackMix;
}

export interface TrackMix {
  volume_db: number;
  pan: number;
}

export const MIN_TRACK_VOLUME_DB = -60;
export const MAX_TRACK_VOLUME_DB = 12;

export interface Marker {
  id: number;
  position_secs: number;
  label: string;
  kind: MarkerKind;
  completed: boolean;
}

export interface Timeline {
  tracks: Track[];
  playhead_secs: number;
  markers: Marker[];
}

export interface SequenceExportSettings {
  aspect_ratio: ExportAspectRatio;
  target_lufs: number;
}

export interface Sequence {
  id: number;
  name: string;
  timeline: Timeline;
  export_settings: SequenceExportSettings;
}

export interface Project {
  id: number;
  name: string;
  media_library: MediaAsset[];
  sequences: Sequence[];
  active_sequence: number;
  panel_layout: PanelLayout | null;
  recent_asset_ids: number[];
  frame_rate: FrameRate | null;
}

/** Frames per second as `num / den`, so 29.97 is 30000/1001. */
export interface FrameRate {
  num: number;
  den: number;
}

export interface PanelLayout {
  lib_panel_width: number;
  props_panel_width: number;
  timeline_height: number;
}

export interface SessionView {
  /** Revision of the update stream this snapshot is current as of. */
  rev?: number;
  project: Project;
  filePath: string | null;
  canUndo: boolean;
  canRedo: boolean;
  dirty: boolean;
}

/** A partial session update: `ops` turn the project at revision `base` into revision `rev`. */
export interface SessionPatch {
  base: number;
  rev: number;
  filePath: string | null;
  canUndo: boolean;
  canRedo: boolean;
  dirty: boolean;
  ops: PatchOp[];
}

export interface PrefsView {
  locale: string;
  lufsProfile: number;
  outputFolder: string;
  autosaveMinutes: number;
  /** Kept for older prefs files; the Bush preview ignores it. */
  previewHardwareDecode: boolean;
  recentProjectPaths: string[];
  savedLayerTemplates: LayerTemplate[];
  whisperModelPath: string;
  reframeModelPath: string;
  backgroundRemovalModelPath: string;
  ttsModelPath: string;
  soundLibraryPath: string;
  errorReportingConsent: ErrorReportingConsent;
  lastReviewedCrashUnix: number;
  telemetryEnabled: boolean;
  /** Changed keyboard shortcuts, command id to stored bindings (see keymap.ts). */
  keybindings: Record<string, string[]>;
}

export type ErrorReportingConsent = "disabled" | "always_send";

export interface CrashReview {
  timestamp: number;
  appVersion: string;
  location: string;
  message: string;
  payloadPreview: string;
}

export type UpdateCheckEvent =
  | {
      type: "newerVersionAvailable";
      version: string;
      htmlUrl: string;
      autoUpdateAvailable: boolean;
    }
  | { type: "upToDate" }
  | { type: "failed" }
  | { type: "installed"; version: string }
  | { type: "installFailed" };

export interface LibraryTrack {
  name: string;
  path: string;
  durationSecs: number;
  category: "music" | "sfx";
}

export interface TranscriptWord {
  id: number;
  text: string;
  start_secs: number;
  end_secs: number;
  confidence: number;
  speaker: string | null;
}

export interface TranscriptDocument {
  schema_version: number;
  asset_id: number;
  language: string | null;
  words: TranscriptWord[];
}

export interface TranscriptHit {
  assetId: number;
  assetName: string;
  word: TranscriptWord;
}

export interface SilenceGap {
  startSecs: number;
  endSecs: number;
}

export interface SilenceReview {
  trackId: number;
  gaps: SilenceGap[];
}

export interface AutoReframeEvent {
  clipId: number;
  subjectFound: boolean;
}

export type MatteGenerationEvent =
  { type: "done"; clipId: number } | { type: "failed"; clipId: number; message: string };

export type PrivacyBlurEvent =
  { type: "done"; clipId: number } | { type: "failed"; clipId: number; message: string };

export type TtsEvent = { type: "done" } | { type: "failed"; message: string };

export type WatchFolderStatus = "stabilizing" | "processing" | "done" | "error";

export type YoutubeDownloadEvent =
  { type: "progress"; fraction: number } | { type: "done" } | { type: "failed"; message: string };

export type WatchFolderEvent =
  | { type: "detected"; path: string }
  | { type: "stabilizing"; path: string }
  | { type: "processing"; path: string }
  | { type: "progress"; path: string; percent: number }
  | { type: "done"; path: string; before: LoudnessMetrics; after: LoudnessMetrics }
  | { type: "failed"; path: string; message: string };

export type TranscribeEvent =
  | { type: "started"; assetId: number }
  | { type: "done"; assetId: number }
  | { type: "noSpeech"; assetId: number }
  | { type: "failed"; assetId: number; message: string };

/** A saved layer-group template -- `layers` is opaque `ClipFormatting` per layer (position/
 *  scale/crop/effects), never edited field-by-field in the webview, only captured and reapplied
 *  as a whole through the backend commands. */
export interface LayerTemplate {
  name: string;
  layers: [TrackKind, Record<string, unknown>][];
}

/** Unsaved work an earlier launch left behind: a project that was never saved when it ended. */
export interface RecoverySession {
  id: string;
  projectName: string;
  savedUnixSecs: number;
}

export interface RecentProject {
  path: string;
  name: string;
  modifiedUnixSecs: number;
  autosavePath: string | null;
  folder: string;
  sequenceCount: number;
  mediaCount: number;
  durationSecs: number;
}

export interface PreviewTick {
  frameSeq: number;
  playheadSecs: number;
  playing: boolean;
  hasClip: boolean;
  available: boolean;
  error: string | null;
  peakL: number;
  peakR: number;
  /** Momentary loudness in LUFS while playing; null when silent or stopped. */
  momentaryLufs: number | null;
  /** A compound clip is rendering in the background. */
  renderingCompound: boolean;
}

export type ExportStatus =
  | "Queued"
  | { Rendering: { percent: number } }
  | { Paused: { percent: number } }
  | "Done"
  | { Failed: { message: string } };

export interface ExportJobView {
  id: number;
  title: string;
  outputPath: string;
  status: ExportStatus;
  percent: number;
  elapsedSecs: number | null;
  /** Once the job has run: where it was encoded and how long it took. */
  report?: ExportReport | null;
}

export interface ExportReport {
  /** `h264_nvenc`, `libopenh264`, ... or null when none was opened. */
  encoder: string | null;
  tookSecs: number;
  /** Length of the exported video. */
  videoSecs: number;
}

export interface ClipPatch {
  gainDb?: number;
  speedFactor?: number;
  brightness?: number;
  contrast?: number;
  saturation?: number;
  frozen?: boolean;
  flippedH?: boolean;
  voiceCleanupEnabled?: boolean;
  cropX?: number;
  cropY?: number;
  cropW?: number;
  cropH?: number;
  maskShape?: MaskShape;
  maskCornerRadius?: number;
  transitionIn?: TransitionType;
  transitionDurationSecs?: number;
  fadeInSecs?: number;
  fadeOutSecs?: number;
  blendMode?: BlendMode;
  positionKeyframes?: Keyframe<Position>[];
  layerScaleX?: number;
  layerScaleY?: number;
  scaleKeyframes?: Keyframe<number>[];
  rotationKeyframes?: Keyframe<number>[];
  opacityKeyframes?: Keyframe<number>[];
  gainKeyframes?: Keyframe<number>[];
  brightnessKeyframes?: Keyframe<number>[];
  contrastKeyframes?: Keyframe<number>[];
  saturationKeyframes?: Keyframe<number>[];
  cropXKeyframes?: Keyframe<number>[];
  cropYKeyframes?: Keyframe<number>[];
  cropWKeyframes?: Keyframe<number>[];
  cropHKeyframes?: Keyframe<number>[];
}

/** Timeline length of a clip, matching `ClipInstance::duration_secs` for constant speed. */
export function clipDuration(clip: ClipInstance): number {
  return (clip.source_out_secs - clip.source_in_secs) / Math.max(clip.speed_factor, 0.0001);
}

export function activeTimeline(project: Project): Timeline {
  return project.sequences[project.active_sequence].timeline;
}

/** Mirrors `avcore::export::ExportAspectRatio::dims_or` -- the canvas dimensions the real
 *  export/compositing pipeline frames everything against. */
export function aspectRatioDims(
  ratio: ExportAspectRatio,
  fallback: [number, number],
): [number, number] {
  switch (ratio) {
    case "Landscape":
      return [1920, 1080];
    case "Portrait":
      return [1080, 1920];
    case "Square":
      return [1080, 1080];
    default:
      return fallback;
  }
}

/** What a path currently is on disk, as `check_path` reports it. */
export type PathStatus = "empty" | "file" | "folder" | "missing";
