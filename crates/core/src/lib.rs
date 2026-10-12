// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! `core` — the UI-agnostic engine crate for oca.
//!
//! Holds the project/timeline/media data model (see [`project`], [`timeline`], [`media`],
//! [`export`]), the general keyframe-animation system ([`keyframe`]), the `ffprobe`/`ffmpeg`
//! wrappers that populate that model from real files ([`probe`], [`loudness`]), lightweight
//! editing proxies ([`proxy`]), the normalized-export renderer ([`render`]), a frame sampler
//! for the analysis features ([`frame_sampler`]), a local music/SFX catalog ([`sound_library`]), and
//! `.ocproj`/`.ocqueue` save/load — gzip-compressed MessagePack ([`persistence`]).
//!
//! Nothing in this crate depends on `egui` or any GUI toolkit — `ui` is the only
//! consumer, and it owns all presentation/formatting concerns (see its `i18n` module).

pub mod analysis;
pub mod av;
pub mod graphics;
pub mod model;
pub mod platform;
pub mod project_io;
pub mod render;

// The modules keep their flat paths (`avcore::probe`, `crate::timeline`) through these re-exports.
pub use analysis::{
    auto_reframe, background_removal, dynamic_reframe, gameplay_events, highlight_detection,
    mask_propagation, motion_tracking, privacy_blur, scene_detection, semantic_index,
    silence_detection, text_to_speech, transcribe, transcript, transcript_proposals,
    transcript_search,
};
use analysis::{onnx_models, whisper_model};
pub use av::{
    frame_sampler, loudness, probe, proxy, sound_library, subtitles, voice_cleanup_preview,
    waveform, youtube_download,
};
pub use graphics::{
    blend_mode, font_catalog, lut, motion_template, overlay_render, scopes, shape_render,
    text_layout, text_metrics,
};
pub use model::{
    frame_rate, keyframe, media, multicam_sync, nested_sequence, project, smart_bins, timeline,
    timeline_window, undo,
};
use platform::update_install;
pub use platform::{entitlement, error_reporting, telemetry, update_check};
pub use project_io::{
    app_paths, bundle, collab_bundle, export, export_queue, interchange, persistence, recovery,
    watched_folder,
};

pub use auto_reframe::{
    compute_reframe_crop, detect_faces, main_subject_center, CropRect, FaceBox, ReframeError,
};
pub use avbridge::{
    extract_pcm_16k_mono, last_video_encoder, AudioSegment, Canvas, ClipSegment,
    GpuEncoderPreference, PcmError, ShapeSegment,
};
pub use background_removal::{encode_matte_video, segment_person, MatteEncodeError, SegmentError};
pub use bundle::{
    bundled_resource_path, bundled_resources_dir, configure_bundled_runtime, resource_path_in,
    validate_bundled_resources, BundledResource, MissingBundleResources,
};
pub use collab_bundle::{export_collab_bundle, import_collab_bundle, CollabBundleError};
pub use dynamic_reframe::{
    augment_sample_times_for_cuts, fill_reframe_gaps, select_subject_center,
    smooth_subject_centers, sparse_crop_keyframes, CropKeyframeAxes, ReframeSample,
    DEFAULT_CONTINUITY_MAX_DISTANCE, DEFAULT_EXTRA_SAMPLES_PER_CUT, DEFAULT_SPARSIFY_EPSILON,
    MAX_HOLD_GAP_SAMPLES,
};
pub use error_reporting::{
    contains_forbidden_content, sanitize_stack_trace, sanitize_text, validate_envelope,
    validate_report, Breadcrumb, ClipCountBucket, ErrorCode, ErrorReport, ErrorReportBuilder,
    ErrorReporter, ErrorSeverity, ExportStage, MediaContext, NullReporter, Operation,
    QueueEnvelope, RecoveryOutcome, ReleaseMetadata, ReportValidationError, ResolutionBucket,
    RuntimeEnvironment, ERROR_REPORT_SCHEMA_VERSION, MAX_BREADCRUMBS, MAX_REPORT_BYTES,
    MAX_STACK_TRACE_BYTES, QUEUE_MAX_BYTES, QUEUE_MAX_RECORDS, QUEUE_RETENTION_SECS,
    STATE_TRANSITIONS,
};
pub use export::{ExportAspectRatio, ExportJob, ExportJobStatus, PlatformExportPreset};
pub use export_queue::{
    export_queue_path, load_export_queue_from_path, next_available_path,
    normalize_loaded_export_queue, persisted_export_queue_snapshot, save_export_queue_to_path,
    RenderControl,
};
pub use frame_rate::FrameRate;
pub use frame_sampler::{FrameSampler, SampledFrame, SamplerError};
pub use highlight_detection::{
    clip_amplitude_samples, detect_highlight_candidates, HighlightCandidate,
    TimelineAmplitudeSample, DEFAULT_HIGHLIGHT_GRID_SECS, DEFAULT_HIGHLIGHT_MIN_DURATION_SECS,
    DEFAULT_HIGHLIGHT_THRESHOLD_LINEAR,
};
pub use keyframe::{Keyframe, Position};
pub use loudness::{measure_loudness, LoudnessError};
pub use lut::{Lut3D, LutParseError};
pub use media::{LoudnessMetrics, MediaAsset, MediaKind};
pub use motion_tracking::{
    rgba_to_gray, track_region, track_region_with_scores, tracked_positions_to_keyframes,
    GrayFrame, TrackResult, TrackedPosition,
};
pub use multicam_sync::{
    amplitude_envelope, best_lag_windows, compute_sync_offset_secs, DEFAULT_ENVELOPE_WINDOW_SECS,
    DEFAULT_MAX_SYNC_OFFSET_SECS,
};
pub use persistence::{
    from_ocproj_bytes, from_ocqueue_bytes, from_octr_bytes, from_project_bytes,
    load_project_from_file, save_project_to_file, to_ocproj_bytes, to_ocqueue_bytes, to_octr_bytes,
    to_project_bytes, write_atomic, PersistError,
};
pub use probe::{probe_media, ProbeError, ProbedMedia};
pub use project::{PanelLayout, Project, Recency, Sequence, SequenceExportSettings};
pub use proxy::{ensure_proxy, PreviewQuality, ProxyError};
pub use render::{
    apply_export_aspect_ratio, render_export, render_export_job, render_export_job_multi,
    render_export_job_multi_with_audio, render_export_job_multi_with_options,
    render_timeline_export, resolve_audio_segments, resolve_privacy_blur_segments,
    resolve_shape_segments, resolve_text_segments, resolve_timeline_segments,
    resolve_timeline_segments_multi, PrivacyBlurSegment, RenderError, RenderOutcome, TextSegment,
    NO_NORMALIZATION,
};
pub use scene_detection::{detect_scene_cuts, SceneCut, DEFAULT_SCENE_CUT_THRESHOLD};
pub use scopes::{luma_waveform_rgba, render_scopes_rgba, vectorscope_rgba};
pub use shape_render::{build_shape_filter_desc, point_in_polygon, ShapeRenderInput};
pub use silence_detection::{
    clip_silence_gaps, detect_silence_gaps, SilenceGap, DEFAULT_MIN_SILENCE_SECS,
    DEFAULT_SILENCE_THRESHOLD_LINEAR,
};
pub use smart_bins::SmartBin;
pub use sound_library::{scan_library_dir, LibraryTrack, SoundCategory};
pub use subtitles::export_srt;
pub use telemetry::{record_event, GpuSampler, ResourceSampler, TelemetryError, TelemetryEvent};
pub use text_to_speech::{
    load_voice_config, phonemes_to_ids, synthesize, synthesize_with_phonemizer, write_wav,
    PhonemizationError, Phonemizer, PiperVoiceConfig, PortuguesePhonemizer, TtsError,
};
pub use timeline::{
    AudioRole, ClipFormatting, ClipInstance, LayerTemplate, Marker, MarkerKind, ShapeClip,
    ShapeKind, TextAlign, TextClip, TextDirection, TextFontFamily, TextFontStyle, Timeline, Track,
    TrackKind,
};
pub use timeline_window::extract_timeline_window;
pub use transcribe::{transcribe, TranscribeError, TranscribeOutcome, TranscribeSegment};
pub use transcript::{
    cache_dir_for_project as transcript_cache_dir_for_project, load_transcript_document,
    save_transcript_document, transcript_path, TranscriptDocument, TranscriptStorageError,
    TranscriptValidationError, TranscriptWord, TRANSCRIPT_SCHEMA_VERSION,
};
pub use transcript_proposals::{
    detect_proposals, filler_word_set, map_source_range_to_timeline, merge_source_ranges,
    SourceRange, TranscriptEditKind, TranscriptProposal, DEAD_AIR_THRESHOLD_SECS, MAX_REPEAT_NGRAM,
    REPEAT_WINDOW_SECS,
};
pub use transcript_search::{search_transcripts_in_project, ProjectTranscriptHit};
pub use update_check::{
    apply_update, auto_update_supported, expected_update_asset_name, fetch_latest_release,
    is_newer, package_supports_atomic_update, release_supports_auto_update, ApplyUpdateError,
    ApplyUpdateOutcome, LatestRelease, UpdateCheckError, UpdatePackage,
};
pub use update_install::restart_application;
pub use watched_folder::{
    is_video_file, output_path_for, process_watched_file, ProcessError, StabilityTracker,
    DEFAULT_OUTPUT_SUBFOLDER, DEFAULT_STABLE_SECS, DEFAULT_TARGET_LUFS, VIDEO_EXTENSIONS,
};
pub use waveform::{
    detailed_bucket_count, generate_detailed_waveform, generate_waveform, WaveformError,
    WAVEFORM_BUCKET_COUNT, WAVEFORM_DETAIL_BUCKETS_PER_SEC,
};
pub use youtube_download::{
    download_youtube, is_yt_dlp_available, Mp3Bitrate, Mp4Quality, YoutubeDownloadError,
    YoutubeDownloadTarget,
};
