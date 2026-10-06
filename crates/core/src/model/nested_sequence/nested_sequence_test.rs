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

use std::path::{Path, PathBuf};

use super::*;
use crate::probe::probe_media;
use crate::project::{Recency, SequenceExportSettings};
use crate::timeline::{
    BlendMode, ClipInstance, ColorFilter, MaskShape, Track, TrackKind, TransitionType,
};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn clip(
    id: u64,
    asset_id: u64,
    start_secs: f64,
    source_in_secs: f64,
    source_out_secs: f64,
) -> ClipInstance {
    ClipInstance {
        id,
        asset_id,
        start_secs,
        source_in_secs,
        source_out_secs,
        composite_id: None,
        color_label: None,
        gain_db: 0.0,
        frozen: false,
        speed_factor: 1.0,
        speed_ramp_end_factor: None,
        nested_sequence_id: None,
        crop_x: 0.0,
        crop_y: 0.0,
        crop_w: 1.0,
        crop_h: 1.0,
        mask_shape: MaskShape::None,
        mask_corner_radius: 0.0,
        flipped_h: false,
        color_filter: ColorFilter::None,
        vignette_intensity: 0.0,
        brightness: 0.0,
        contrast: 1.0,
        saturation: 1.0,
        sharpen: 0.0,
        chroma_key_enabled: false,
        chroma_key_color: [0, 255, 0],
        chroma_key_tolerance: 0.4,
        blur_intensity: 0.0,
        shake_intensity: 0.0,
        glitch_intensity: 0.0,
        pixelize_intensity: 0.0,
        transition_in: TransitionType::None,
        transition_duration_secs: 0.5,
        fade_in_secs: 0.0,
        fade_out_secs: 0.0,
        position_keyframes: vec![],
        scale_keyframes: vec![],
        rotation_keyframes: vec![],
        opacity_keyframes: vec![],
        gain_keyframes: vec![],
        voice_cleanup_enabled: false,
        voice_cleanup_noise_floor_db: -30.0,
        voice_cleanup_compressor_threshold_db: -18.0,
        voice_cleanup_compressor_ratio: 3.0,
        voice_cleanup_ceiling_linear: 0.95,
        brightness_keyframes: vec![],
        contrast_keyframes: vec![],
        saturation_keyframes: vec![],
        crop_x_keyframes: vec![],
        crop_y_keyframes: vec![],
        crop_w_keyframes: vec![],
        crop_h_keyframes: vec![],
        deflicker_enabled: false,
        lut_path: String::new(),
        layer_scale_x: 1.0,
        layer_scale_y: 1.0,
        stabilization_intensity: 0.0,
        background_removal_enabled: false,
        background_removal_mask_path: String::new(),
        blend_mode: BlendMode::Normal,
        anchor_x: 0.5,
        anchor_y: 0.5,
        reframe_seed_point: None,
        privacy_blur_enabled: false,
        privacy_blur_mask_path: String::new(),
        privacy_blur_sigma: 15.0,
        privacy_blur_seed_vertices: Vec::new(),
        privacy_blur_seed_center_x_frac: 0.0,
        privacy_blur_seed_center_y_frac: 0.0,
    }
}

fn video_track(id: u64, clips: Vec<ClipInstance>) -> Track {
    Track {
        id,
        name: "V1".to_string(),
        kind: TrackKind::Video,
        clips,
        text_clips: vec![],
        shape_clips: vec![],
        visible: true,
        audio_role: crate::AudioRole::Unspecified,
        muted: false,
        solo: false,
        locked: false,
        color_label: None,
    }
}

fn timeline_with(tracks: Vec<Track>) -> Timeline {
    Timeline {
        tracks,
        playhead_secs: 0.0,
        markers: Vec::new(),
        multicam_groups: Vec::new(),
    }
}

fn sequence(id: u64, timeline: Timeline) -> Sequence {
    Sequence {
        id,
        name: format!("Sequence {id}"),
        timeline,
        export_settings: SequenceExportSettings::default(),
    }
}

fn project(sequences: Vec<Sequence>, media_library: Vec<MediaAsset>) -> Project {
    Project {
        id: 1,
        name: "Test Project".to_string(),
        last_edited: Recency::HoursAgo(0),
        summary: String::new(),
        media_library,
        sequences,
        active_sequence: 0,
        file_path: None,
        panel_layout: None,
        smart_bins: Vec::new(),
        recent_asset_ids: Vec::new(),
    }
}

#[test]
fn is_a_noop_when_no_clip_is_nested() {
    let timeline = timeline_with(vec![video_track(1, vec![clip(1, 1, 0.0, 0.0, 1.0)])]);
    let proj = project(vec![sequence(1, timeline.clone())], vec![]);
    let mut cache = HashMap::new();

    let assets =
        materialize_nested_sequences(&proj, &timeline, Path::new("."), &mut cache).unwrap();
    assert!(assets.is_empty());
    assert!(cache.is_empty());
}

#[test]
fn errors_on_a_missing_nested_sequence() {
    let mut c = clip(1, 0, 0.0, 0.0, 1.0);
    c.nested_sequence_id = Some(999);
    let timeline = timeline_with(vec![video_track(1, vec![c])]);
    let proj = project(vec![sequence(1, timeline.clone())], vec![]);
    let mut cache = HashMap::new();

    let err =
        materialize_nested_sequences(&proj, &timeline, Path::new("."), &mut cache).unwrap_err();
    assert!(matches!(err, RenderError::MissingNestedSequence));
}

#[test]
fn detects_a_direct_self_nesting_cycle() {
    // Sequence 1's own clip references sequence 1 itself.
    let mut c = clip(1, 0, 0.0, 0.0, 1.0);
    c.nested_sequence_id = Some(1);
    let timeline = timeline_with(vec![video_track(1, vec![c])]);
    let proj = project(vec![sequence(1, timeline.clone())], vec![]);
    let mut cache = HashMap::new();

    let err =
        materialize_nested_sequences(&proj, &timeline, Path::new("."), &mut cache).unwrap_err();
    assert!(matches!(err, RenderError::CyclicNestedSequence));
}

#[test]
fn detects_an_indirect_nesting_cycle() {
    // Sequence 1 nests sequence 2, which nests sequence 1 back.
    let mut c1 = clip(1, 0, 0.0, 0.0, 1.0);
    c1.nested_sequence_id = Some(2);
    let seq1_timeline = timeline_with(vec![video_track(1, vec![c1])]);

    let mut c2 = clip(2, 0, 0.0, 0.0, 1.0);
    c2.nested_sequence_id = Some(1);
    let seq2_timeline = timeline_with(vec![video_track(1, vec![c2])]);

    let proj = project(
        vec![
            sequence(1, seq1_timeline.clone()),
            sequence(2, seq2_timeline),
        ],
        vec![],
    );
    let mut cache = HashMap::new();

    let err = materialize_nested_sequences(&proj, &seq1_timeline, Path::new("."), &mut cache)
        .unwrap_err();
    assert!(matches!(err, RenderError::CyclicNestedSequence));
}

#[test]
fn renders_a_real_nested_sequence_to_a_probeable_synthetic_asset() {
    let path = fixture("video.mp4");
    let asset = probe_media(&path)
        .unwrap()
        .into_media_asset(1, "video.mp4".to_string(), path);

    // Child sequence: a plain 0.3s clip of the real fixture.
    let child_timeline = timeline_with(vec![video_track(1, vec![clip(1, 1, 0.0, 0.0, 0.3)])]);
    let child = sequence(2, child_timeline);

    // Parent sequence: one clip nesting the child.
    let mut nested_clip = clip(10, 0, 0.0, 0.0, 0.3);
    nested_clip.nested_sequence_id = Some(2);
    let parent_timeline = timeline_with(vec![video_track(1, vec![nested_clip])]);

    let proj = project(
        vec![sequence(1, parent_timeline.clone()), child],
        vec![asset],
    );
    let cache_dir =
        std::env::temp_dir().join(format!("oca_nested_sequence_test_{}", std::process::id()));
    let mut cache = HashMap::new();

    let assets =
        materialize_nested_sequences(&proj, &parent_timeline, &cache_dir, &mut cache).unwrap();
    assert_eq!(assets.len(), 1);
    let synthetic = assets.get(&2).expect("child sequence id 2 should resolve");
    assert!(synthetic.source_path.exists());
    assert!(synthetic.resolution.is_some());
    assert!(synthetic.duration_secs > 0.0);
    assert_eq!(cache.len(), 1);

    let _ = std::fs::remove_dir_all(&cache_dir);
}

#[test]
fn a_materialized_compound_clip_resolves_through_resolve_timeline_segments_multi() {
    // Regression test for the bug this session found: `ClipInstance::asset_id` is left
    // untouched (0) when a clip is wrapped as a compound clip -- only `resolved_asset_id()`
    // (which substitutes `synthetic_asset_id(nested_sequence_id)`) matches the id
    // `materialize_nested_sequences` gives the rendered nested sequence's synthetic
    // `MediaAsset`. Before `resolved_asset_id()` existed, every resolve function's plain
    // `clip.asset_id` lookup would miss this asset and fail with `RenderError::MissingAsset`,
    // silently breaking compound clips in both the render/export path and (via the same
    // pattern) preview, in every shell.
    use crate::render::resolve_timeline_segments_multi;

    let path = fixture("video.mp4");
    let asset = probe_media(&path)
        .unwrap()
        .into_media_asset(1, "video.mp4".to_string(), path);

    let child_timeline = timeline_with(vec![video_track(1, vec![clip(1, 1, 0.0, 0.0, 0.3)])]);
    let child = sequence(2, child_timeline);

    let mut nested_clip = clip(10, 0, 0.0, 0.0, 0.3);
    nested_clip.nested_sequence_id = Some(2);
    assert_eq!(nested_clip.resolved_asset_id(), synthetic_asset_id(2));
    let parent_timeline = timeline_with(vec![video_track(1, vec![nested_clip])]);
    let parent = sequence(1, parent_timeline.clone());

    let proj = project(vec![parent.clone(), child], vec![asset.clone()]);
    let cache_dir = std::env::temp_dir().join(format!(
        "oca_nested_sequence_test_resolve_{}",
        std::process::id()
    ));
    let mut cache = HashMap::new();

    let nested_assets =
        materialize_nested_sequences(&proj, &parent_timeline, &cache_dir, &mut cache).unwrap();
    let mut media_library = vec![asset];
    media_library.extend(nested_assets.into_values());

    let result = resolve_timeline_segments_multi(&parent, &media_library);
    assert!(
        result.is_ok(),
        "compound clip should resolve via resolved_asset_id(), got {result:?}"
    );

    let _ = std::fs::remove_dir_all(&cache_dir);
}

#[test]
fn reuses_the_cached_render_when_the_nested_timeline_is_unchanged() {
    let path = fixture("video.mp4");
    let asset = probe_media(&path)
        .unwrap()
        .into_media_asset(1, "video.mp4".to_string(), path);

    let child_timeline = timeline_with(vec![video_track(1, vec![clip(1, 1, 0.0, 0.0, 0.3)])]);
    let child = sequence(2, child_timeline);

    let mut nested_clip = clip(10, 0, 0.0, 0.0, 0.3);
    nested_clip.nested_sequence_id = Some(2);
    let parent_timeline = timeline_with(vec![video_track(1, vec![nested_clip])]);

    let mut proj = project(
        vec![sequence(1, parent_timeline.clone()), child],
        vec![asset],
    );
    let cache_dir = std::env::temp_dir().join(format!(
        "oca_nested_sequence_test_cache_reuse_{}",
        std::process::id()
    ));
    let mut cache = HashMap::new();

    let first =
        materialize_nested_sequences(&proj, &parent_timeline, &cache_dir, &mut cache).unwrap();
    let first_path = first.get(&2).unwrap().source_path.clone();

    let second =
        materialize_nested_sequences(&proj, &parent_timeline, &cache_dir, &mut cache).unwrap();
    let second_path = second.get(&2).unwrap().source_path.clone();

    assert_eq!(first_path, second_path, "same cached render path reused");
    let lease = cache.get(&2).unwrap().artifact();
    proj.sequences[1].timeline.clip_mut(1).unwrap().gain_db = 3.0;
    let changed =
        materialize_nested_sequences(&proj, &parent_timeline, &cache_dir, &mut cache).unwrap();
    let changed_path = changed.get(&2).unwrap().source_path.clone();
    assert_ne!(
        changed_path, first_path,
        "an edited child needs a new immutable render"
    );
    assert!(
        first_path.exists(),
        "the old consumer still owns its render"
    );
    let changed_lease = cache.get(&2).unwrap().artifact();
    cache.clear();
    assert!(changed_path.exists());
    drop(changed_lease);
    assert!(!changed_path.exists());
    assert!(
        first_path.exists(),
        "a consumer lease must keep the render alive"
    );
    drop(lease);
    assert!(
        !first_path.exists(),
        "the last lease must release the render"
    );

    let _ = std::fs::remove_dir_all(&cache_dir);
}

#[test]
fn cancelled_materialization_creates_no_render_or_cache_entry() {
    let mut nested = clip(10, 0, 0.0, 0.0, 0.3);
    nested.nested_sequence_id = Some(2);
    let parent = timeline_with(vec![video_track(1, vec![nested])]);
    let child = timeline_with(vec![]);
    let proj = project(
        vec![sequence(1, parent.clone()), sequence(2, child)],
        vec![],
    );
    let dir = tempfile::tempdir().unwrap();
    let mut cache = HashMap::new();
    let cancel = AtomicBool::new(true);
    assert!(matches!(
        materialize_nested_sequences_with_cancel(&proj, &parent, dir.path(), &mut cache, &cancel),
        Err(RenderError::Cancelled)
    ));
    assert!(cache.is_empty());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}
