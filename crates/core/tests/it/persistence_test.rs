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

use std::fs;
use std::path::PathBuf;

use avcore::persistence::{
    from_ocproj_bytes, from_ocqueue_bytes, load_project_from_file, save_project_to_file,
    to_ocproj_bytes, to_ocqueue_bytes, PersistError,
};
use avcore::timeline::{
    AudioRole, BlendMode, ClipInstance, ColorFilter, MaskShape, TextClip, TextFontFamily,
    TextFontStyle, Timeline, Track, TrackKind, TransitionType,
};
use avcore::{LoudnessMetrics, MediaAsset, MediaKind, Project, Recency, Sequence, TextSegment};

/// Looks up `key` in a MessagePack struct-map value, panicking if `value` isn't a map or
/// doesn't have that key. `rmpv::Value` only exposes read-only indexing/`as_map`, so mutating
/// a nested field means walking the map ourselves.
fn as_map_field_mut<'a>(value: &'a mut rmpv::Value, key: &str) -> &'a mut rmpv::Value {
    match value {
        rmpv::Value::Map(pairs) => {
            &mut pairs
                .iter_mut()
                .find(|(k, _)| k.as_str() == Some(key))
                .unwrap_or_else(|| panic!("missing key {key:?}"))
                .1
        }
        _ => panic!("expected a map looking up {key:?}"),
    }
}

/// Same as [`as_map_field_mut`], but expects the field's value to be an array.
fn as_array_field_mut<'a>(value: &'a mut rmpv::Value, key: &str) -> &'a mut Vec<rmpv::Value> {
    match as_map_field_mut(value, key) {
        rmpv::Value::Array(items) => items,
        _ => panic!("expected {key:?} to be an array"),
    }
}

fn clip(id: u64, asset_id: u64) -> ClipInstance {
    ClipInstance {
        id,
        asset_id,
        start_secs: 0.0,
        source_in_secs: 0.0,
        source_out_secs: 10.0,
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

fn fixture_project() -> Project {
    Project {
        id: 1,
        name: "Fixture project".to_string(),
        last_edited: Recency::HoursAgo(2),
        summary: "Persistence test fixture.".to_string(),
        media_library: vec![MediaAsset {
            id: 1,
            file_name: "clip.mp4".to_string(),
            source_path: PathBuf::from("/media/clip.mp4"),
            kind: MediaKind::Video,
            has_audio: true,
            duration_secs: 10.0,
            codec: "H.264".to_string(),
            source_bitrate_mbps: 40.0,
            resolution: Some((1920, 1080)),
            fps: Some(60.0),
            sample_rate_khz: None,
            loudness: Some(LoudnessMetrics {
                integrated_lufs: -18.0,
                true_peak_dbtp: -3.0,
                loudness_range_lu: 8.0,
            }),
            proxy_path: None,
            waveform_peaks: None,
            favorited: false,
        }],
        sequences: vec![Sequence {
            id: 1,
            name: "Sequência principal".to_string(),
            timeline: Timeline {
                playhead_secs: 0.0,
                tracks: vec![Track {
                    id: 1,
                    name: "V1".to_string(),
                    kind: TrackKind::Video,
                    clips: vec![clip(1, 1)],

                    text_clips: vec![],
                    shape_clips: vec![],

                    visible: true,
                    audio_role: AudioRole::Unspecified,
                    locked: false,
                    color_label: None,
                }],
                markers: Vec::new(),
                multicam_groups: Vec::new(),
            },
            export_settings: Default::default(),
        }],
        active_sequence: 0,
        file_path: None,
        panel_layout: None,
        smart_bins: Vec::new(),
        recent_asset_ids: Vec::new(),
    }
}

fn empty_project() -> Project {
    Project {
        id: 2,
        name: "Empty project".to_string(),
        last_edited: Recency::DaysAgo(3),
        summary: String::new(),
        media_library: vec![],
        sequences: vec![Sequence {
            id: 1,
            name: "Sequência principal".to_string(),
            timeline: Timeline {
                tracks: vec![],
                playhead_secs: 0.0,
                markers: Vec::new(),
                multicam_groups: Vec::new(),
            },
            export_settings: Default::default(),
        }],
        active_sequence: 0,
        file_path: None,
        panel_layout: None,
        smart_bins: Vec::new(),
        recent_asset_ids: Vec::new(),
    }
}

fn project_with_styled_text() -> Project {
    let mut project = fixture_project();
    project.sequences[0].timeline.tracks.push(Track {
        id: 2,
        name: "Text".to_string(),
        kind: TrackKind::Text,
        clips: vec![],
        text_clips: vec![TextClip {
            id: 2,
            start_secs: 0.0,
            duration_secs: 2.0,
            text: "Paco Paçoca".to_string(),
            font_size: 48.0,
            font_family: TextFontFamily::PlayfairDisplay,
            font_style: TextFontStyle::Bold,
            font_weight: None,
            color_rgba: [255, 255, 255, 255],
            background_rgba: [10, 20, 30, 180],
            background_padding: 14.0,
            background_corner_radius: 9.0,
            pos_x: 0.1,
            pos_y: 0.8,
            words: vec![],
            highlight_enabled: false,
            highlight_color_rgba: [255, 220, 0, 255],
            opacity_keyframes: vec![],
            pos_x_keyframes: vec![],
            pos_y_keyframes: vec![],
            scale_keyframes: vec![],
            rotation_keyframes: vec![],
            direction: Default::default(),
            language: None,
            text_align: Default::default(),
        }],
        shape_clips: vec![],
        visible: true,
        audio_role: AudioRole::Unspecified,
        locked: false,
        color_label: None,
    });
    project
}

#[test]
fn round_trips_a_project_through_ocproj() {
    let original = fixture_project();
    let bytes = to_ocproj_bytes(&original).unwrap();
    let restored = from_ocproj_bytes(&bytes).unwrap();
    assert_eq!(original, restored);
}

#[test]
fn round_trips_bundled_font_and_text_background_style() {
    let original = project_with_styled_text();
    let restored: Project = from_ocproj_bytes(&to_ocproj_bytes(&original).unwrap()).unwrap();
    let text = &restored.sequences[0].timeline.tracks[1].text_clips[0];

    assert_eq!(text.font_family, TextFontFamily::PlayfairDisplay);
    assert_eq!(text.font_style, TextFontStyle::Bold);
    assert_eq!(text.background_rgba, [10, 20, 30, 180]);
    assert_eq!(text.background_padding, 14.0);
    assert_eq!(text.background_corner_radius, 9.0);
}

#[test]
fn projects_saved_before_text_styles_load_with_safe_defaults() {
    let bytes = to_ocproj_bytes(&project_with_styled_text()).unwrap();
    let header_len = 5;
    let mut msgpack = Vec::new();
    std::io::Read::read_to_end(
        &mut flate2::read::GzDecoder::new(&bytes[header_len..]),
        &mut msgpack,
    )
    .unwrap();

    let mut value = rmpv::decode::read_value(&mut &msgpack[..]).unwrap();
    let sequences = as_array_field_mut(&mut value, "sequences");
    let timeline = as_map_field_mut(&mut sequences[0], "timeline");
    let tracks = as_array_field_mut(timeline, "tracks");
    let text_clips = as_array_field_mut(&mut tracks[1], "text_clips");
    if let rmpv::Value::Map(pairs) = &mut text_clips[0] {
        pairs.retain(|(key, _)| {
            !matches!(
                key.as_str(),
                Some(
                    "font_family"
                        | "font_style"
                        | "background_rgba"
                        | "background_padding"
                        | "background_corner_radius"
                )
            )
        });
    }

    let mut edited_msgpack = Vec::new();
    rmpv::encode::write_value(&mut edited_msgpack, &value).unwrap();
    let mut edited_bytes = Vec::new();
    edited_bytes.extend_from_slice(&bytes[..header_len]);
    let mut encoder = flate2::write::GzEncoder::new(&mut edited_bytes, flate2::Compression::fast());
    std::io::Write::write_all(&mut encoder, &edited_msgpack).unwrap();
    encoder.finish().unwrap();

    let restored: Project = from_ocproj_bytes(&edited_bytes).unwrap();
    let text = &restored.sequences[0].timeline.tracks[1].text_clips[0];
    assert_eq!(text.font_family, TextFontFamily::Lato);
    assert_eq!(text.font_style, TextFontStyle::Regular);
    assert_eq!(text.background_rgba, [0, 0, 0, 0]);
    assert_eq!(text.background_padding, 8.0);
    assert_eq!(text.background_corner_radius, 8.0);
}

#[test]
fn projects_with_an_unrecognized_font_family_name_preserve_it_as_unknown() {
    // FONT-01's forwards-compatibility rule (spec/architecture/built-in-font-catalog.md), now in
    // full: a project saved by a future build with a font family this build doesn't know about
    // must still load, not fail outright -- and, since FONT-01A's persisted-identity swap, the
    // unrecognized name is preserved (TextFontFamily::Unknown) rather than silently normalized
    // away, so a resave from this build doesn't lose it for whichever future build *does*
    // recognize "InterVariable". `family_id()`/`supports_bold()` still make it *behave* as Lato
    // everywhere it's rendered -- covered by timeline_test.rs's own
    // `text_font_family_unknown_renders_and_behaves_as_lato`.
    let bytes = to_ocproj_bytes(&project_with_styled_text()).unwrap();
    let header_len = 5;
    let mut msgpack = Vec::new();
    std::io::Read::read_to_end(
        &mut flate2::read::GzDecoder::new(&bytes[header_len..]),
        &mut msgpack,
    )
    .unwrap();

    let mut value = rmpv::decode::read_value(&mut &msgpack[..]).unwrap();
    let sequences = as_array_field_mut(&mut value, "sequences");
    let timeline = as_map_field_mut(&mut sequences[0], "timeline");
    let tracks = as_array_field_mut(timeline, "tracks");
    let text_clips = as_array_field_mut(&mut tracks[1], "text_clips");
    if let rmpv::Value::Map(pairs) = &mut text_clips[0] {
        for (key, val) in pairs.iter_mut() {
            if key.as_str() == Some("font_family") {
                *val = rmpv::Value::String("InterVariable".into());
            }
        }
    }

    let mut edited_msgpack = Vec::new();
    rmpv::encode::write_value(&mut edited_msgpack, &value).unwrap();
    let mut edited_bytes = Vec::new();
    edited_bytes.extend_from_slice(&bytes[..header_len]);
    let mut encoder = flate2::write::GzEncoder::new(&mut edited_bytes, flate2::Compression::fast());
    std::io::Write::write_all(&mut encoder, &edited_msgpack).unwrap();
    encoder.finish().unwrap();

    let restored: Project = from_ocproj_bytes(&edited_bytes).unwrap();
    let text = &restored.sequences[0].timeline.tracks[1].text_clips[0];
    assert_eq!(
        text.font_family,
        TextFontFamily::Unknown("InterVariable".to_string())
    );
    // The originally-saved family (PlayfairDisplay in project_with_styled_text()) was a
    // recognized name and must still round-trip untouched by an unrelated edit.
    assert_eq!(text.font_style, TextFontStyle::Bold);

    // Resaving must keep the exact same unrecognized string, not silently drop it -- the whole
    // point of preserving it in the first place.
    let resaved = to_ocproj_bytes(&restored).unwrap();
    let reloaded: Project = from_ocproj_bytes(&resaved).unwrap();
    assert_eq!(
        reloaded.sequences[0].timeline.tracks[1].text_clips[0].font_family,
        TextFontFamily::Unknown("InterVariable".to_string())
    );
}

#[test]
fn round_trips_a_project_with_a_saved_panel_layout() {
    let mut original = fixture_project();
    original.panel_layout = Some(avcore::PanelLayout {
        lib_panel_width: 250.0,
        props_panel_width: 300.0,
        timeline_height: 210.0,
    });
    let bytes = to_ocproj_bytes(&original).unwrap();
    let restored = from_ocproj_bytes(&bytes).unwrap();
    assert_eq!(original, restored);
}

#[test]
fn round_trips_per_sequence_export_settings() {
    let mut original = fixture_project();
    original.sequences[0].export_settings.aspect_ratio = avcore::ExportAspectRatio::Portrait;
    original.sequences[0].export_settings.target_lufs = -23.0;

    let bytes = to_ocproj_bytes(&original).unwrap();
    let restored: Project = from_ocproj_bytes(&bytes).unwrap();

    assert_eq!(
        restored.sequences[0].export_settings.aspect_ratio,
        avcore::ExportAspectRatio::Portrait
    );
    assert_eq!(restored.sequences[0].export_settings.target_lufs, -23.0);
}

/// Proves struct-map mode's field-level defaulting actually works end to end — not just that
/// `gain_db` has a default value in isolation. Decodes the real `.ocproj` MessagePack payload
/// as a generic value, removes the `gain_db` key the same way the old JSON test removed it from
/// a `serde_json::Value`, re-encodes, and confirms `from_ocproj_bytes` still loads it (at unity
/// gain) instead of erroring out on the "missing" field.
#[test]
fn projects_saved_before_per_block_gain_load_at_unity_gain() {
    let original = fixture_project();
    let bytes = to_ocproj_bytes(&original).unwrap();

    // Peel off the [MAGIC][version] header and gunzip to get the raw MessagePack bytes.
    let header_len = 5;
    let mut msgpack = Vec::new();
    std::io::Read::read_to_end(
        &mut flate2::read::GzDecoder::new(&bytes[header_len..]),
        &mut msgpack,
    )
    .unwrap();

    let mut value = rmpv::decode::read_value(&mut &msgpack[..]).unwrap();
    let sequences = as_array_field_mut(&mut value, "sequences");
    for sequence in sequences {
        let timeline = as_map_field_mut(sequence, "timeline");
        let tracks = as_array_field_mut(timeline, "tracks");
        for track in tracks {
            let clips = as_array_field_mut(track, "clips");
            for clip in clips {
                if let rmpv::Value::Map(pairs) = clip {
                    pairs.retain(|(key, _)| key.as_str() != Some("gain_db"));
                }
            }
        }
    }

    let mut edited_msgpack = Vec::new();
    rmpv::encode::write_value(&mut edited_msgpack, &value).unwrap();

    let mut edited_bytes = Vec::new();
    edited_bytes.extend_from_slice(&bytes[..header_len]);
    let mut encoder = flate2::write::GzEncoder::new(&mut edited_bytes, flate2::Compression::fast());
    std::io::Write::write_all(&mut encoder, &edited_msgpack).unwrap();
    encoder.finish().unwrap();

    let restored: Project = from_ocproj_bytes(&edited_bytes).unwrap();

    assert!(restored
        .sequences
        .iter()
        .flat_map(|sequence| &sequence.timeline.tracks)
        .flat_map(|track| &track.clips)
        .all(|clip| clip.gain_db == 0.0));
}

/// Same defaulting guarantee as the `gain_db` test above, for `Project::panel_layout` (added
/// for `ui`'s per-project layout scope) — a project saved before this field existed at all
/// should still load, with `panel_layout: None` rather than a deserialization error.
#[test]
fn projects_saved_before_panel_layout_load_with_no_layout() {
    let original = fixture_project();
    let bytes = to_ocproj_bytes(&original).unwrap();

    let header_len = 5;
    let mut msgpack = Vec::new();
    std::io::Read::read_to_end(
        &mut flate2::read::GzDecoder::new(&bytes[header_len..]),
        &mut msgpack,
    )
    .unwrap();

    let mut value = rmpv::decode::read_value(&mut &msgpack[..]).unwrap();
    if let rmpv::Value::Map(pairs) = &mut value {
        pairs.retain(|(key, _)| key.as_str() != Some("panel_layout"));
    }

    let mut edited_msgpack = Vec::new();
    rmpv::encode::write_value(&mut edited_msgpack, &value).unwrap();

    let mut edited_bytes = Vec::new();
    edited_bytes.extend_from_slice(&bytes[..header_len]);
    let mut encoder = flate2::write::GzEncoder::new(&mut edited_bytes, flate2::Compression::fast());
    std::io::Write::write_all(&mut encoder, &edited_msgpack).unwrap();
    encoder.finish().unwrap();

    let restored: Project = from_ocproj_bytes(&edited_bytes).unwrap();

    assert_eq!(restored.panel_layout, None);
}

#[test]
fn projects_saved_before_sequence_export_settings_load_with_defaults() {
    let original = fixture_project();
    let bytes = to_ocproj_bytes(&original).unwrap();

    let header_len = 5;
    let mut msgpack = Vec::new();
    std::io::Read::read_to_end(
        &mut flate2::read::GzDecoder::new(&bytes[header_len..]),
        &mut msgpack,
    )
    .unwrap();

    let mut value = rmpv::decode::read_value(&mut &msgpack[..]).unwrap();
    for sequence in as_array_field_mut(&mut value, "sequences") {
        if let rmpv::Value::Map(pairs) = sequence {
            pairs.retain(|(key, _)| key.as_str() != Some("export_settings"));
        }
    }

    let mut edited_msgpack = Vec::new();
    rmpv::encode::write_value(&mut edited_msgpack, &value).unwrap();
    let mut edited_bytes = Vec::new();
    edited_bytes.extend_from_slice(&bytes[..header_len]);
    let mut encoder = flate2::write::GzEncoder::new(&mut edited_bytes, flate2::Compression::fast());
    std::io::Write::write_all(&mut encoder, &edited_msgpack).unwrap();
    encoder.finish().unwrap();

    let restored: Project = from_ocproj_bytes(&edited_bytes).unwrap();

    assert_eq!(
        restored.sequences[0].export_settings,
        avcore::SequenceExportSettings::default()
    );
}

#[test]
fn round_trips_a_project_with_an_empty_timeline_and_library() {
    let original = empty_project();
    assert!(original.media_library.is_empty());
    assert!(original.timeline().tracks.is_empty());
    let bytes = to_ocproj_bytes(&original).unwrap();
    let restored = from_ocproj_bytes(&bytes).unwrap();
    assert_eq!(original, restored);
}

#[test]
fn from_ocproj_bytes_rejects_malformed_input() {
    assert!(matches!(
        from_ocproj_bytes::<Project>(b"not an ocproj file"),
        Err(PersistError::Corrupt(_))
    ));
}

#[test]
fn from_ocproj_bytes_rejects_an_unsupported_version() {
    let original = fixture_project();
    let mut bytes = to_ocproj_bytes(&original).unwrap();
    bytes[4] = 99;
    assert!(matches!(
        from_ocproj_bytes::<Project>(&bytes),
        Err(PersistError::Corrupt(_))
    ));
}

#[test]
fn from_ocproj_bytes_rejects_a_decompression_bomb() {
    // Bypasses to_ocproj_bytes to build the raw gzip stream by hand: a run of zero bytes well
    // past MAX_DECOMPRESSED_BYTES (256 MiB), which flate2 compresses down to a few KB -- the
    // exact "tiny file, huge decompressed size" shape a real gzip-bomb DoS attempt would have.
    use std::io::Write;
    let mut bytes = b"OCPJ".to_vec();
    bytes.push(1); // FORMAT_VERSION
    let mut encoder = flate2::write::GzEncoder::new(&mut bytes, flate2::Compression::fast());
    let chunk = vec![0u8; 1024 * 1024];
    for _ in 0..257 {
        encoder.write_all(&chunk).unwrap();
    }
    encoder.finish().unwrap();

    assert!(matches!(
        from_ocproj_bytes::<Project>(&bytes),
        Err(PersistError::Corrupt(_))
    ));
}

#[test]
fn ocqueue_round_trip_uses_its_own_magic_bytes() {
    let original = vec![1_u64, 5, 9];

    let bytes = to_ocqueue_bytes(&original).unwrap();
    let restored: Vec<u64> = from_ocqueue_bytes(&bytes).unwrap();

    assert_eq!(&bytes[..4], b"OCQU");
    assert_eq!(restored, original);
    assert!(matches!(
        from_ocproj_bytes::<Vec<u64>>(&bytes),
        Err(PersistError::Corrupt(_))
    ));
}

#[test]
fn queued_text_segments_without_a_glyph_range_load_as_whole_text() {
    let original = vec![TextSegment {
        start_secs: 1.0,
        duration_secs: 0.5,
        text: "old queued word".to_string(),
        font_size: 32.0,
        font_family: TextFontFamily::Lato,
        font_style: TextFontStyle::Regular,
        font_weight: None,
        color_rgba: [255, 255, 255, 255],
        background_rgba: [0, 0, 0, 0],
        background_padding: 0.0,
        background_corner_radius: 0.0,
        glyph_byte_range: Some([4, 10]),
        pos_x: 0.1,
        pos_y: 0.8,
        opacity_keyframe_expr: String::new(),
        position_keyframe_expr_x: String::new(),
        position_keyframe_expr_y: String::new(),
        scale_keyframe_expr_x: String::new(),
        scale_keyframe_expr_y: String::new(),
        rotation_keyframe_expr_x: String::new(),
        rotation_keyframe_expr_y: String::new(),
        direction: Default::default(),
        text_align: Default::default(),
    }];
    let bytes = to_ocqueue_bytes(&original).unwrap();
    let mut msgpack = Vec::new();
    std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(&bytes[5..]), &mut msgpack)
        .unwrap();
    let mut value = rmpv::decode::read_value(&mut &msgpack[..]).unwrap();
    let rmpv::Value::Array(segments) = &mut value else {
        panic!("expected segment array");
    };
    let rmpv::Value::Map(fields) = &mut segments[0] else {
        panic!("expected segment map");
    };
    fields.retain(|(key, _)| key.as_str() != Some("glyph_byte_range"));

    let mut legacy_msgpack = Vec::new();
    rmpv::encode::write_value(&mut legacy_msgpack, &value).unwrap();
    let mut legacy_bytes = bytes[..5].to_vec();
    let mut encoder = flate2::write::GzEncoder::new(&mut legacy_bytes, flate2::Compression::fast());
    std::io::Write::write_all(&mut encoder, &legacy_msgpack).unwrap();
    encoder.finish().unwrap();

    let restored: Vec<TextSegment> = from_ocqueue_bytes(&legacy_bytes).unwrap();
    assert_eq!(restored[0].glyph_byte_range, None);
    assert_eq!(restored[0].text, "old queued word");
}

#[test]
fn from_ocqueue_bytes_rejects_malformed_and_unsupported_data() {
    assert!(matches!(
        from_ocqueue_bytes::<Vec<u64>>(b"not an ocqueue file"),
        Err(PersistError::Corrupt(_))
    ));

    let mut bytes = to_ocqueue_bytes(&vec![1_u64]).unwrap();
    bytes[4] = 99;
    assert!(matches!(
        from_ocqueue_bytes::<Vec<u64>>(&bytes),
        Err(PersistError::Corrupt(_))
    ));
}

#[test]
fn file_path_is_not_part_of_the_serialized_bytes() {
    let mut project = fixture_project();
    project.file_path = Some("/tmp/whatever.ocproj".into());
    let bytes = to_ocproj_bytes(&project).unwrap();
    assert!(!bytes.windows(b"whatever".len()).any(|w| w == b"whatever"));
}

#[test]
fn save_then_load_round_trips_through_a_real_file() {
    let original = fixture_project();
    let path = std::env::temp_dir().join(format!("oca_persist_test_{}.ocproj", original.id));

    save_project_to_file(&original, &path).unwrap();
    let loaded = load_project_from_file(&path).unwrap();
    let _ = fs::remove_file(&path);

    assert_eq!(original, loaded);
}

#[test]
fn load_project_from_file_errors_on_a_missing_file() {
    let path = std::env::temp_dir().join("oca_persist_test_does_not_exist.ocproj");
    assert!(matches!(
        load_project_from_file(&path),
        Err(PersistError::Io(_))
    ));
}
