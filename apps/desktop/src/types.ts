// TypeScript mirror of the JSON the Rust side emits. `Project` and friends serialize with
// avcore's own serde derives (snake_case field names, externally tagged enums); the shell's
// own view types (`SessionView`, `PreviewTick`, `ExportJobView`) are camelCase. Only the fields
// the webview reads are declared -- extra fields are ignored at runtime.

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
export type TransitionType = "None" | "Fade" | "HardCut" | "Slide" | "Zoom";
/** Industry-standard compositing-mode names, shown untranslated regardless of locale -- mirrors
 *  `crates/ui/src/screens/editor/properties_panel/keyframe_editors.rs`'s `blend_mode_label`. */
export const BLEND_MODES = [
  "Normal", "Addition", "And", "Average", "Burn", "Darken", "Difference", "GrainExtract",
  "Divide", "Dodge", "Exclusion", "HardLight", "Lighten", "Multiply", "Negation", "Or",
  "Overlay", "Phoenix", "PinLight", "Reflect", "Screen", "SoftLight", "Subtract", "VividLight",
  "Xor", "HardMix", "LinearLight", "Glow", "GrainMerge", "Multiply128", "Heat", "Freeze",
  "Extremity", "SoftDifference", "Geometric", "Harmonic", "Bleach", "Stain", "Interpolate",
  "HardOverlay",
] as const;
export type BlendMode = (typeof BLEND_MODES)[number];
export type AudioRole = "Unspecified" | "GameAudio" | "Mic" | "Music";
export type ExportAspectRatio = "Original" | "Landscape" | "Portrait" | "Square";
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
  waveform_peaks: [number, number][] | null;
  favorited: boolean;
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
  blend_mode: BlendMode;
  position_keyframes: Keyframe<Position>[];
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
  color_rgba: Rgba;
  pos_x: number;
  pos_y: number;
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
  width: number;
  height: number;
  color_rgba: Rgba;
}

/** `avcore::timeline::ShapeKind` as serde_json emits it. */
export type ShapeKind = "Ellipse" | { Polygon: [number, number][] };

export type Rgba = [number, number, number, number];
export type TextFontStyle = "Regular" | "Bold";
export type TextAlign = "Auto" | "Left" | "Center" | "Right" | "Start" | "End";
export type ShapePreset = "Ellipse" | "Rectangle" | "Triangle" | "Trapezoid" | "Arrow" | "Custom";

export interface TextClipPatch {
  text?: string;
  fontFamily?: string;
  fontStyle?: TextFontStyle;
  fontSize?: number;
  colorRgba?: Rgba;
  textAlign?: TextAlign;
  posX?: number;
  posY?: number;
  highlightEnabled?: boolean;
  highlightColorRgba?: Rgba;
}

export interface ShapeClipPatch {
  shapeKind?: ShapePreset;
  colorRgb?: [number, number, number];
  opacity?: number;
  centerX?: number;
  centerY?: number;
  width?: number;
  height?: number;
}

export type FontCategory = "Sans" | "Display" | "Serif" | "Handwritten" | "Monospace" | "International";

/** `commands::overlays::OverlayCatalog`: static font/shape data owned by `core`. */
export interface OverlayCatalog {
  fonts: { id: string; displayName: string; category: FontCategory; supportsBold: boolean }[];
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
}

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
  recent_asset_ids: number[];
}

export interface SessionView {
  project: Project;
  filePath: string | null;
  canUndo: boolean;
  canRedo: boolean;
  dirty: boolean;
}

export interface PrefsView {
  locale: string;
  lufsProfile: number;
  outputFolder: string;
  autosaveMinutes: number;
  previewHardwareDecode: boolean;
  recentProjectPaths: string[];
  savedLayerTemplates: LayerTemplate[];
  whisperModelPath: string;
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

export interface RecentProject {
  path: string;
  name: string;
  modifiedUnixSecs: number;
  autosavePath: string | null;
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
  blendMode?: BlendMode;
  positionKeyframes?: Keyframe<Position>[];
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
