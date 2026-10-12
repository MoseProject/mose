//! How the editor's per-edit work grows with a big project: dozens of tracks, thousands of clips,
//! effects on all of them. Ignored by default (it prints timings):
//! `cargo test --release -p desktop stress -- --ignored --nocapture`

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use avcore::timeline::{
    AudioRole, ClipInstance, MaskShape as CoreMask, Timeline, Track, TrackKind,
};
use avcore::{MediaAsset, MediaKind, Project};
use bush::{Lut3D, SceneVersions};

use super::mapper::{map_timeline, OverlayCache, SourceResolver};

struct Resolver;

impl SourceResolver for Resolver {
    fn source_path(&mut self, asset: &MediaAsset) -> Option<PathBuf> {
        Some(PathBuf::from(format!("asset-{}.mp4", asset.id)))
    }
    fn lut(&mut self, _path: &str) -> Option<Arc<Lut3D>> {
        None
    }
}

fn asset(id: u64) -> MediaAsset {
    MediaAsset {
        id,
        file_name: format!("asset-{id}"),
        source_path: PathBuf::from(format!("asset-{id}.mp4")),
        kind: MediaKind::Video,
        has_audio: true,
        duration_secs: 600.0,
        codec: "h264".into(),
        source_bitrate_mbps: 8.0,
        resolution: Some((1920, 1080)),
        fps: Some(60.0),
        sample_rate_khz: None,
        loudness: None,
        proxy_path: None,
        waveform_peaks: Some(vec![(-0.5, 0.5); 2000]),
        favorited: false,
    }
}

fn big_project(tracks: usize, clips_per_track: usize) -> Project {
    let mut project = crate::state::blank_project("stress".into(), "seq".into());
    project.media_library = (1..=40).map(asset).collect();
    let mut next_id = 1;
    let tracks_vec = (0..tracks)
        .map(|t| {
            let clips = (0..clips_per_track)
                .map(|c| {
                    let mut clip =
                        ClipInstance::new(next_id, 1 + (next_id % 40), c as f64 * 5.0, 0.0, 5.0);
                    next_id += 1;
                    clip.brightness = 0.1;
                    clip.blur_intensity = 0.3;
                    clip.mask_shape = CoreMask::Circle;
                    clip
                })
                .collect();
            Track {
                id: t as u64 + 1,
                name: format!("V{}", t + 1),
                kind: TrackKind::Video,
                clips,
                text_clips: Vec::new(),
                shape_clips: Vec::new(),
                visible: true,
                audio_role: AudioRole::Unspecified,
                muted: false,
                solo: false,
                color_label: None,
                locked: false,
            }
        })
        .collect();
    let mut tracks_vec: Vec<Track> = tracks_vec;
    // A caption track and a shape track, so the inspector and timeline have both to show.
    let overlay = |id: u64, kind: TrackKind, name: &str| Track {
        id,
        name: name.into(),
        kind,
        clips: Vec::new(),
        text_clips: Vec::new(),
        shape_clips: Vec::new(),
        visible: true,
        audio_role: AudioRole::Unspecified,
        muted: false,
        solo: false,
        color_label: None,
        locked: false,
    };
    let mut text = overlay(900, TrackKind::Text, "T1");
    text.text_clips = (0..6)
        .map(|i| {
            avcore::timeline::TextClip::new(10_000 + i, i as f64 * 7.0, format!("Legenda {i}"))
        })
        .collect();
    let mut shapes = overlay(901, TrackKind::Shape, "S1");
    shapes.shape_clips = (0..4)
        .map(|i| avcore::timeline::ShapeClip::new(11_000 + i, i as f64 * 9.0))
        .collect();
    tracks_vec.push(text);
    tracks_vec.push(shapes);
    project.sequences[0].timeline = Timeline {
        tracks: tracks_vec,
        playhead_secs: 0.0,
        markers: Vec::new(),
        multicam_groups: Vec::new(),
    };
    project
}

fn ms(label: &str, runs: u32, mut work: impl FnMut()) {
    work();
    let started = Instant::now();
    for _ in 0..runs {
        work();
    }
    eprintln!(
        "  {label:<34} {:>8.2} ms",
        started.elapsed().as_secs_f64() * 1e3 / f64::from(runs)
    );
}

#[test]
#[ignore]
fn stress() {
    for (tracks, clips) in [(10, 100), (30, 100), (60, 100)] {
        let project = big_project(tracks, clips);
        eprintln!("{tracks} tracks x {clips} clips = {} clips", tracks * clips);
        ms("clone the project", 5, || {
            std::hint::black_box(project.clone());
        });
        ms("clone the sequence (undo step)", 5, || {
            std::hint::black_box(project.active_sequence().clone());
        });
        let mut edited = project.clone();
        edited.timeline_mut().tracks[tracks / 2].clips[10].start_secs += 1.0;
        ms("diff after moving one clip", 5, || {
            std::hint::black_box(crate::session_sync::diff(&project, &edited));
        });
        ms("full snapshot as JSON", 3, || {
            let value = serde_json::to_value(&project).unwrap();
            std::hint::black_box(serde_json::to_string(&value).unwrap());
        });
        ms("save to bytes", 3, || {
            std::hint::black_box(avcore::to_project_bytes(&project).unwrap());
        });
        let mut cache = OverlayCache::default();
        ms("map the timeline to a scene", 3, || {
            std::hint::black_box(
                map_timeline(
                    project.timeline(),
                    &project.media_library,
                    (1280, 720),
                    &mut Resolver,
                    &mut cache,
                    SceneVersions::default(),
                )
                .unwrap(),
            );
        });
    }
}

/// Writes a big project as the JSON the webview receives, for driving the real frontend against
/// it (`OCA_DUMP_SESSION=path OCA_DUMP_TRACKS=30 OCA_DUMP_CLIPS=100`).
#[test]
#[ignore]
fn dump_session_json() {
    let Some(path) = std::env::var_os("OCA_DUMP_SESSION") else {
        return;
    };
    let number = |name: &str, default: usize| {
        std::env::var(name)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };
    let project = big_project(number("OCA_DUMP_TRACKS", 30), number("OCA_DUMP_CLIPS", 100));
    let view = serde_json::json!({
        "rev": 1,
        "project": project,
        "filePath": null,
        "canUndo": false,
        "canRedo": false,
        "dirty": false,
    });
    std::fs::write(
        path,
        serde_json::to_vec(&crate::session_sync::snapshot(&view)).unwrap(),
    )
    .unwrap();
}
