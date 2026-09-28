// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! Timeline edits. Each command is one user-visible commit (one undo step): the webview
//! previews drags and slider moves locally and only calls in on release, the same
//! one-snapshot-per-gesture rule `ui::App::push_undo_snapshot_for_drag` enforces.

use avcore::timeline::{AudioRole, ClipInstance, Timeline, TrackKind};
use avcore::{MediaKind, Project};
use serde::Deserialize;
use tauri::AppHandle;

use super::undoable;

/// Shortest clip a trim may leave behind.
const MIN_CLIP_SECS: f64 = 0.1;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TrimEdge {
    Start,
    End,
}

/// Property changes from the inspector; `None` fields are left untouched.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipPatch {
    pub gain_db: Option<f32>,
    pub speed_factor: Option<f32>,
    pub brightness: Option<f32>,
    pub contrast: Option<f32>,
    pub saturation: Option<f32>,
    pub frozen: Option<bool>,
    pub flipped_h: Option<bool>,
    pub voice_cleanup_enabled: Option<bool>,
}

#[tauri::command]
pub fn add_asset_to_timeline(
    app: AppHandle,
    asset_id: u64,
    track_id: Option<u64>,
    start_secs: Option<f64>,
) -> Result<(), String> {
    undoable(&app, |project| {
        add_asset(project, asset_id, track_id, start_secs).is_some()
    })
}

#[tauri::command]
pub fn move_clip(
    app: AppHandle,
    clip_id: u64,
    track_id: u64,
    start_secs: f64,
) -> Result<(), String> {
    undoable(&app, |project| {
        move_clip_in(project.timeline_mut(), clip_id, track_id, start_secs)
    })
}

#[tauri::command]
pub fn trim_clip(
    app: AppHandle,
    clip_id: u64,
    edge: TrimEdge,
    position_secs: f64,
) -> Result<(), String> {
    undoable(&app, |project| trim(project, clip_id, edge, position_secs))
}

/// Splits `clip_ids` at the playhead, or every unlocked track's clip under the playhead when
/// `clip_ids` is empty.
#[tauri::command]
pub fn split_at_playhead(app: AppHandle, clip_ids: Vec<u64>) -> Result<(), String> {
    undoable(&app, |project| split(project.timeline_mut(), &clip_ids))
}

#[tauri::command]
pub fn delete_clips(app: AppHandle, clip_ids: Vec<u64>, ripple: bool) -> Result<(), String> {
    undoable(&app, |project| {
        delete(project.timeline_mut(), &clip_ids, ripple)
    })
}

#[tauri::command]
pub fn update_clip(app: AppHandle, clip_id: u64, patch: ClipPatch) -> Result<(), String> {
    undoable(&app, |project| {
        let timeline = project.timeline_mut();
        if track_of(timeline, clip_id).is_none_or(|index| timeline.tracks[index].locked) {
            return false;
        }
        timeline
            .clip_mut(clip_id)
            .is_some_and(|clip| apply_patch(clip, &patch))
    })
}

#[tauri::command]
pub fn add_track(app: AppHandle, kind: TrackKind) -> Result<(), String> {
    undoable(&app, |project| {
        project.timeline_mut().create_track(kind);
        true
    })
}

#[tauri::command]
pub fn set_track_flags(
    app: AppHandle,
    track_id: u64,
    visible: Option<bool>,
    locked: Option<bool>,
) -> Result<(), String> {
    undoable(&app, |project| {
        let Some(track) = project
            .timeline_mut()
            .tracks
            .iter_mut()
            .find(|t| t.id == track_id)
        else {
            return false;
        };
        let before = (track.visible, track.locked);
        track.visible = visible.unwrap_or(track.visible);
        track.locked = locked.unwrap_or(track.locked);
        before != (track.visible, track.locked)
    })
}

pub(crate) fn add_asset(
    project: &mut Project,
    asset_id: u64,
    track_id: Option<u64>,
    start_secs: Option<f64>,
) -> Option<u64> {
    let asset = project.media_library.iter().find(|a| a.id == asset_id)?;
    let kind = match asset.kind {
        MediaKind::Video => TrackKind::Video,
        MediaKind::Audio => TrackKind::Audio,
    };
    let duration = asset.duration_secs;
    let timeline = project.timeline_mut();
    let index = timeline.resolve_or_create_track(kind, track_id);
    if timeline.tracks[index].locked {
        return None;
    }
    let start = start_secs
        .map(|s| s.max(0.0))
        .unwrap_or_else(|| timeline.tracks[index].duration_secs());
    let id = timeline.next_clip_id();
    let clip = ClipInstance {
        // Mic tracks start with voice cleanup on -- CF-03, applied only at creation time.
        voice_cleanup_enabled: timeline.tracks[index].audio_role == AudioRole::Mic,
        ..ClipInstance::new(id, asset_id, start, 0.0, duration)
    };
    timeline.tracks[index].clips.push(clip);
    project.record_recent_asset(asset_id);
    Some(id)
}

fn track_of(timeline: &Timeline, clip_id: u64) -> Option<usize> {
    timeline
        .tracks
        .iter()
        .position(|t| t.clips.iter().any(|c| c.id == clip_id))
}

pub(crate) fn move_clip_in(
    timeline: &mut Timeline,
    clip_id: u64,
    track_id: u64,
    start_secs: f64,
) -> bool {
    let Some(source) = track_of(timeline, clip_id) else {
        return false;
    };
    let Some(target) = timeline.tracks.iter().position(|t| t.id == track_id) else {
        return false;
    };
    if timeline.tracks[source].locked || timeline.tracks[target].locked {
        return false;
    }
    let start_secs = start_secs.max(0.0);
    if source == target {
        timeline.tracks[source].move_clip(clip_id, start_secs)
    } else {
        timeline.move_clip_to_track(clip_id, track_id, start_secs)
    }
}

fn trim(project: &mut Project, clip_id: u64, edge: TrimEdge, position_secs: f64) -> bool {
    let timeline = project.timeline();
    let Some(index) = track_of(timeline, clip_id) else {
        return false;
    };
    if timeline.tracks[index].locked {
        return false;
    }
    let max_source_out = timeline.tracks[index]
        .clips
        .iter()
        .find(|c| c.id == clip_id)
        .and_then(|clip| project.media_library.iter().find(|a| a.id == clip.asset_id))
        .map(|asset| asset.duration_secs);
    let Some(clip) = project.timeline_mut().tracks[index].clip_mut(clip_id) else {
        return false;
    };
    match edge {
        TrimEdge::Start => clip.trim_start(position_secs, MIN_CLIP_SECS),
        TrimEdge::End => clip.trim_end(position_secs, MIN_CLIP_SECS, max_source_out),
    }
}

pub(crate) fn split(timeline: &mut Timeline, clip_ids: &[u64]) -> bool {
    let playhead = timeline.playhead_secs;
    let targets: Vec<usize> = timeline
        .tracks
        .iter()
        .enumerate()
        .filter(|(_, track)| !track.locked)
        .filter(|(_, track)| {
            track
                .clip_at(playhead)
                .is_some_and(|clip| clip_ids.is_empty() || clip_ids.contains(&clip.id))
        })
        .map(|(index, _)| index)
        .collect();
    let mut changed = false;
    for index in targets {
        let new_id = timeline.next_clip_id();
        changed |= timeline.tracks[index].split_clip_at(playhead, new_id);
    }
    changed
}

pub(crate) fn delete(timeline: &mut Timeline, clip_ids: &[u64], ripple: bool) -> bool {
    let mut next_id = timeline.next_clip_id();
    let mut changed = false;
    for &clip_id in clip_ids {
        let Some(index) = track_of(timeline, clip_id) else {
            continue;
        };
        let track = &mut timeline.tracks[index];
        if track.locked {
            continue;
        }
        if ripple {
            let Some((start, end)) = track
                .clips
                .iter()
                .find(|c| c.id == clip_id)
                .map(|c| (c.start_secs, c.start_secs + c.duration_secs()))
            else {
                continue;
            };
            changed |= track.ripple_delete_range(start, end, &mut next_id);
        } else {
            let before = track.clips.len();
            track.clips.retain(|c| c.id != clip_id);
            changed |= track.clips.len() != before;
        }
    }
    changed
}

fn apply_patch(clip: &mut ClipInstance, patch: &ClipPatch) -> bool {
    let before = clip.clone();
    if let Some(gain_db) = patch.gain_db {
        clip.gain_db = gain_db.clamp(-24.0, 24.0);
    }
    if let Some(speed) = patch.speed_factor {
        clip.speed_factor = speed.clamp(0.25, 4.0);
    }
    if let Some(brightness) = patch.brightness {
        clip.brightness = brightness.clamp(-1.0, 1.0);
    }
    if let Some(contrast) = patch.contrast {
        clip.contrast = contrast.clamp(0.0, 2.0);
    }
    if let Some(saturation) = patch.saturation {
        clip.saturation = saturation.clamp(0.0, 2.0);
    }
    if let Some(frozen) = patch.frozen {
        clip.frozen = frozen;
    }
    if let Some(flipped) = patch.flipped_h {
        clip.flipped_h = flipped;
    }
    if let Some(enabled) = patch.voice_cleanup_enabled {
        clip.voice_cleanup_enabled = enabled;
    }
    *clip != before
}

#[cfg(test)]
#[path = "timeline_test.rs"]
mod tests;
