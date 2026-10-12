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

//! Tracks performance of the engine's actual hot paths: parsing `loudnorm` filter stderr and
//! (de)serializing a project. Run with `cargo bench -p core` (or `make bench`) and
//! compare against a previous run's `target/criterion/` report to catch regressions as the
//! engine grows — this is the "acompanhar o desempenho da aplicação" half of the ask; the
//! test suite (`cargo test`) is the correctness half.

use std::hint::black_box;
use std::path::PathBuf;

use criterion::{criterion_group, criterion_main, Criterion};

use avcore::loudness::parse_loudnorm_stderr;
use avcore::persistence::{
    from_ocproj_bytes, from_project_bytes, to_ocproj_bytes, to_project_bytes,
};
use avcore::timeline::{
    AudioRole, BlendMode, ClipInstance, ColorFilter, MaskShape, Timeline, Track, TrackKind,
    TransitionType,
};
use avcore::{LoudnessMetrics, MediaAsset, MediaKind, Project, Recency, Sequence};

const LOUDNORM_STDERR_FIXTURE: &str = r#"
ffmpeg version 6.0 Copyright (c) 2000-2023 the FFmpeg developers
  built with gcc 12.2.0
Input #0, mov,mp4,m4a,3gp,3g2,mj2, from 'boss03_ribby_croaks.mp4':
  Duration: 00:02:14.23, start: 0.000000, bitrate: 42021 kb/s
[Parsed_loudnorm_0 @ 0000020a1b2c3d40]
{
	"input_i" : "-19.40",
	"input_tp" : "-3.20",
	"input_lra" : "8.10",
	"input_thresh" : "-29.80",
	"output_i" : "-16.02",
	"output_tp" : "-1.50",
	"output_lra" : "7.00",
	"output_thresh" : "-26.40",
	"normalization_type" : "dynamic",
	"target_offset" : "0.00"
}
frame=  8043 fps=812 q=-1.0 Lsize=N/A time=00:02:14.20 bitrate=N/A speed=101x
video:0kB audio:0kB subtitle:0kB other streams:0kB global headers:0kB muxing overhead: unknown
"#;

/// A project sized like a long-running edit session (Minecraft-longplay-into-many-shorts
/// territory) — big enough for serialization cost and timeline math to actually show up in a
/// profile.
fn large_project(asset_count: usize, clips_per_track: usize) -> Project {
    let media_library: Vec<MediaAsset> = (0..asset_count)
        .map(|i| MediaAsset {
            id: i as u64,
            file_name: format!("clip_{i:04}.mp4"),
            source_path: PathBuf::from(format!("/media/clip_{i:04}.mp4")),
            kind: MediaKind::Video,
            has_audio: true,
            duration_secs: 30.0,
            codec: "h264".to_string(),
            source_bitrate_mbps: 42.0,
            resolution: Some((1920, 1080)),
            fps: Some(60.0),
            sample_rate_khz: None,
            loudness: Some(LoudnessMetrics {
                integrated_lufs: -16.0,
                true_peak_dbtp: -1.5,
                loudness_range_lu: 7.0,
            }),
            proxy_path: None,
            waveform_peaks: None,
            favorited: false,
            streams: Default::default(),
            hdr_handling: Default::default(),
            audio_stream: None,
        })
        .collect();

    let make_track = |id: u64, name: &str, kind: TrackKind| Track {
        id,
        name: name.to_string(),
        kind,
        clips: (0..clips_per_track)
            .map(|i| ClipInstance {
                id: i as u64,
                asset_id: (i % asset_count.max(1)) as u64,
                start_secs: i as f64 * 30.0,
                source_in_secs: 0.0,
                source_out_secs: 30.0,
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
            })
            .collect(),
        text_clips: vec![],
        shape_clips: vec![],
        visible: true,
        audio_role: AudioRole::Unspecified,
        muted: false,
        solo: false,
        locked: false,
        mix: Default::default(),
        color_label: None,
    };

    Project {
        id: 1,
        name: "Bench Project".to_string(),
        last_edited: Recency::HoursAgo(1),
        summary: "Synthetic project for benchmarking.".to_string(),
        media_library,
        sequences: vec![Sequence {
            id: 1,
            name: "Sequência principal".to_string(),
            timeline: Timeline {
                tracks: vec![
                    make_track(1, "V1", TrackKind::Video),
                    make_track(2, "A1", TrackKind::Audio),
                    make_track(3, "A2", TrackKind::Audio),
                ],
                playhead_secs: 0.0,
                markers: vec![],
                multicam_groups: Vec::new(),
            },
            export_settings: Default::default(),
        }],
        active_sequence: 0,
        file_path: None,
        panel_layout: None,
        smart_bins: Vec::new(),
        recent_asset_ids: Vec::new(),
        frame_rate: None,
    }
}

fn bench_parse_loudnorm_stderr(c: &mut Criterion) {
    c.bench_function("loudness::parse_loudnorm_stderr", |b| {
        b.iter(|| parse_loudnorm_stderr(black_box(LOUDNORM_STDERR_FIXTURE)).unwrap())
    });
}

fn bench_project_ocproj_round_trip(c: &mut Criterion) {
    let mut group = c.benchmark_group("persistence");

    for clip_count in [10usize, 200, 1000] {
        let project = large_project(50, clip_count);
        let bytes = to_ocproj_bytes(&project).unwrap();

        let project_bytes = to_project_bytes(&project).unwrap();
        group.bench_function(
            format!("to_project_bytes/{clip_count}_clips_per_track"),
            |b| b.iter(|| to_project_bytes(black_box(&project)).unwrap()),
        );
        group.bench_function(
            format!("from_project_bytes/{clip_count}_clips_per_track"),
            |b| b.iter(|| from_project_bytes(black_box(&project_bytes)).unwrap()),
        );
        group.bench_function(
            format!("to_ocproj_bytes/{clip_count}_clips_per_track"),
            |b| b.iter(|| to_ocproj_bytes(black_box(&project)).unwrap()),
        );
        group.bench_function(format!("msgpack_only/{clip_count}_clips_per_track"), |b| {
            b.iter(|| {
                let mut out = Vec::new();
                serde::Serialize::serialize(
                    black_box(&project),
                    &mut rmp_serde::Serializer::new(&mut out).with_struct_map(),
                )
                .unwrap();
                out
            })
        });
        group.bench_function(
            format!("from_ocproj_bytes/{clip_count}_clips_per_track"),
            |b| b.iter(|| from_ocproj_bytes::<Project>(black_box(&bytes)).unwrap()),
        );
    }

    group.finish();
}

fn bench_timeline_duration(c: &mut Criterion) {
    let mut group = c.benchmark_group("timeline_duration_secs");

    for clip_count in [10usize, 200, 1000] {
        let project = large_project(50, clip_count);
        group.bench_function(format!("{clip_count}_clips_per_track"), |b| {
            b.iter(|| black_box(project.timeline()).duration_secs())
        });
    }

    group.finish();
}

/// What every edit pays in the desktop shell: the undo snapshot (`Sequence` clone), the copy
/// handed to the preview thread (`Project` clone) and the JSON the webview receives on
/// `session-changed`. Scales with project size, so a drag that publishes per pointer move is
/// bounded by these numbers.
fn bench_per_edit_cost(c: &mut Criterion) {
    let mut group = c.benchmark_group("per_edit");

    for clip_count in [10usize, 200, 1000] {
        let project = large_project(50, clip_count);
        group.bench_function(format!("project_clone/{clip_count}_clips_per_track"), |b| {
            b.iter(|| black_box(&project).clone())
        });
        group.bench_function(
            format!("sequence_clone/{clip_count}_clips_per_track"),
            |b| b.iter(|| black_box(&project).active_sequence().clone()),
        );
        group.bench_function(format!("session_json/{clip_count}_clips_per_track"), |b| {
            b.iter(|| serde_json::to_vec(black_box(&project)).unwrap())
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_parse_loudnorm_stderr,
    bench_project_ocproj_round_trip,
    bench_timeline_duration,
    bench_per_edit_cost,
);
criterion_main!(benches);
