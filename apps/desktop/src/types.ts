// TypeScript mirror of the JSON the Rust side emits. `Project` and friends serialize with
// avcore's own serde derives (snake_case field names, externally tagged enums); the shell's
// own view types (`SessionView`, `PreviewTick`, `ExportJobView`) are camelCase. Only the fields
// the webview reads are declared -- extra fields are ignored at runtime.

export type MediaKind = "Video" | "Audio";
export type TrackKind = "Video" | "Audio" | "Text" | "Shape";
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
}

export interface TextClip {
  id: number;
  start_secs: number;
  duration_secs: number;
  text: string;
}

export interface ShapeClip {
  id: number;
  start_secs: number;
  duration_secs: number;
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

export type ExportStatus = "rendering" | "done" | "failed" | "cancelled";

export interface ExportJobView {
  id: number;
  title: string;
  outputPath: string;
  status: ExportStatus;
  percent: number;
  message: string | null;
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
}

/** Timeline length of a clip, matching `ClipInstance::duration_secs` for constant speed. */
export function clipDuration(clip: ClipInstance): number {
  return (clip.source_out_secs - clip.source_in_secs) / Math.max(clip.speed_factor, 0.0001);
}

export function activeTimeline(project: Project): Timeline {
  return project.sequences[project.active_sequence].timeline;
}
