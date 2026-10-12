use avcore::timeline::{AudioRole, ClipInstance, Track, TrackKind};
use serde_json::{json, Value};

use super::*;
use crate::state::blank_project;

fn track(id: u64, clips: usize) -> Track {
    Track {
        id,
        name: format!("T{id}"),
        kind: TrackKind::Video,
        clips: (0..clips)
            .map(|i| ClipInstance::new(i as u64 + 1, 1, i as f64 * 10.0, 0.0, 10.0))
            .collect(),
        text_clips: Vec::new(),
        shape_clips: Vec::new(),
        visible: true,
        audio_role: AudioRole::Unspecified,
        muted: false,
        solo: false,
        locked: false,
        mix: Default::default(),
        color_label: None,
    }
}

fn project() -> Project {
    let mut project = blank_project("P".into(), "S".into());
    project.timeline_mut().tracks = vec![track(1, 4), track(2, 3)];
    project
}

/// The webview's `applyOps`, in Rust, for checking that a diff reproduces the next project.
fn apply(root: &mut Value, ops: &[Op]) {
    for op in ops {
        let path = op["p"].as_array().unwrap();
        let (last, parents) = path
            .split_last()
            .map_or((None, &[][..]), |(l, p)| (Some(l), p));
        let mut node = &mut *root;
        for segment in parents {
            node = match segment {
                Value::String(key) => &mut node[key.as_str()],
                other => &mut node[other.as_u64().unwrap() as usize],
            };
        }
        if let Some(len) = op.get("len") {
            let target = if let Some(last) = last {
                match last {
                    Value::String(key) => &mut node[key.as_str()],
                    other => &mut node[other.as_u64().unwrap() as usize],
                }
            } else {
                node
            };
            target
                .as_array_mut()
                .unwrap()
                .truncate(len.as_u64().unwrap() as usize);
            continue;
        }
        let value = op["v"].clone();
        match last.unwrap() {
            Value::String(key) => node[key.as_str()] = value,
            index => {
                let array = node.as_array_mut().unwrap();
                let index = index.as_u64().unwrap() as usize;
                if index == array.len() {
                    array.push(value);
                } else {
                    array[index] = value;
                }
            }
        }
    }
}

/// The project as the webview holds it after a snapshot.
fn project_json(project: &Project) -> Value {
    snapshot(&json!({ "project": project }))["project"].clone()
}

fn assert_round_trip(prev: &Project, next: &Project) -> usize {
    let ops = diff(prev, next);
    let mut json = project_json(prev);
    apply(&mut json, &ops);
    assert_eq!(json, project_json(next));
    ops.len()
}

#[test]
fn nothing_changed_sends_nothing() {
    let p = project();
    assert_eq!(assert_round_trip(&p, &p.clone()), 0);
}

#[test]
fn moving_one_clip_sends_one_clip() {
    let prev = project();
    let mut next = prev.clone();
    next.timeline_mut().tracks[0].clips[2].start_secs = 55.0;
    assert_eq!(assert_round_trip(&prev, &next), 1);
    let ops = diff(&prev, &next);
    assert_eq!(
        ops[0]["p"],
        json!(["sequences", 0, "timeline", "tracks", 0, "clips", 2])
    );
}

#[test]
fn adding_and_removing_clips_and_tracks_round_trips() {
    let prev = project();
    let mut added = prev.clone();
    added.timeline_mut().tracks[1]
        .clips
        .push(ClipInstance::new(9, 1, 99.0, 0.0, 1.0));
    added.timeline_mut().tracks.push(track(3, 2));
    assert_round_trip(&prev, &added);
    // And back: surplus elements are cut.
    assert_round_trip(&added, &prev);
    let mut middle = prev.clone();
    middle.timeline_mut().tracks[0].clips.remove(1);
    assert_round_trip(&prev, &middle);
    let mut gone = prev.clone();
    gone.timeline_mut().tracks.remove(0);
    assert_round_trip(&prev, &gone);
}

#[test]
fn track_flags_sequence_and_project_fields_round_trip() {
    let prev = project();
    let mut next = prev.clone();
    next.name = "Renamed".into();
    next.timeline_mut().tracks[0].muted = true;
    next.timeline_mut().tracks[0].mix = avcore::timeline::TrackMix {
        volume_db: -9.0,
        pan: 0.4,
    };
    next.timeline_mut().tracks[1].name = "Voice".into();
    next.timeline_mut().playhead_secs = 12.5;
    next.active_sequence_mut().name = "Cut".into();
    next.recent_asset_ids = vec![3, 1];
    next.summary = "text".into();
    assert_round_trip(&prev, &next);
}

#[test]
fn a_new_sequence_and_media_round_trip() {
    let prev = project();
    let mut next = prev.clone();
    let mut second = next.sequences[0].clone();
    second.id = 2;
    second.name = "Second".into();
    next.sequences.push(second);
    next.active_sequence = 1;
    assert_round_trip(&prev, &next);
    assert_round_trip(&next, &prev);
}

#[test]
fn a_one_clip_edit_in_a_big_project_costs_far_less_than_the_whole_session() {
    let mut prev = project();
    prev.timeline_mut().tracks = (1..=3).map(|id| track(id, 1000)).collect();
    let mut next = prev.clone();
    next.timeline_mut().tracks[1].clips[500].gain_db = 3.0;
    let patch = std::time::Instant::now();
    let ops = diff(&prev, &next);
    let bytes = serde_json::to_vec(&ops).unwrap().len();
    let patch = patch.elapsed();
    let whole = std::time::Instant::now();
    let full = serde_json::to_vec(&next).unwrap().len();
    let whole = whole.elapsed();
    eprintln!("patch {patch:?} ({bytes} bytes) vs whole {whole:?} ({full} bytes)");
    assert!(bytes * 100 < full);
}

pub(super) fn audio_asset(id: u64, peaks: Option<Vec<(f32, f32)>>) -> avcore::MediaAsset {
    let mut asset = avcore::MediaAsset {
        id,
        file_name: format!("a{id}.wav"),
        source_path: format!("a{id}.wav").into(),
        kind: avcore::MediaKind::Audio,
        has_audio: true,
        duration_secs: 4.0,
        codec: "pcm".into(),
        source_bitrate_mbps: 1.0,
        resolution: None,
        fps: None,
        sample_rate_khz: Some(48.0),
        loudness: None,
        proxy_path: None,
        waveform_peaks: None,
        favorited: false,
        streams: Default::default(),
        hdr_handling: Default::default(),
        audio_stream: None,
    };
    asset.waveform_peaks = peaks;
    asset
}

#[test]
fn the_coarse_waveform_stays_out_of_what_the_webview_receives() {
    let mut prev = project();
    prev.media_library.push(audio_asset(1, None));
    let mut next = prev.clone();
    next.media_library[0].waveform_peaks = Some(vec![(-0.5, 0.5); 2000]);
    next.media_library
        .push(audio_asset(2, Some(vec![(-1.0, 1.0); 2000])));

    let snapshot = project_json(&next);
    assert_eq!(snapshot["media_library"][0]["waveform_peaks"], Value::Null);
    assert_eq!(snapshot["media_library"][0]["has_waveform"], true);
    assert_eq!(
        project_json(&prev)["media_library"][0]["has_waveform"],
        false
    );

    // The patch that announces new peaks carries the flag, not the numbers.
    let ops = diff(&prev, &next);
    assert!(serde_json::to_vec(&ops).unwrap().len() < 2_000);
    assert_round_trip(&prev, &next);
}
