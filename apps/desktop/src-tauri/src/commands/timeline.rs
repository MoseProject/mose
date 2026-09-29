// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! Timeline edits. Each command is one user-visible commit (one undo step): the webview
//! previews drags and slider moves locally and only calls in on release, the same
//! one-snapshot-per-gesture rule `ui::App::push_undo_snapshot_for_drag` enforces.

use std::collections::HashSet;

use avcore::timeline::{
    AudioRole, BlendMode, ClipInstance, MaskShape, Timeline, TrackKind, TransitionType,
};
use avcore::{Keyframe, MediaKind, Position, Project};
use serde::Deserialize;
use tauri::{AppHandle, Manager};

use crate::state::AppState;

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
    pub crop_x: Option<f32>,
    pub crop_y: Option<f32>,
    pub crop_w: Option<f32>,
    pub crop_h: Option<f32>,
    pub mask_shape: Option<MaskShape>,
    pub mask_corner_radius: Option<f32>,
    pub transition_in: Option<TransitionType>,
    pub transition_duration_secs: Option<f32>,
    pub blend_mode: Option<BlendMode>,
    pub position_keyframes: Option<Vec<Keyframe<Position>>>,
    pub layer_scale_x: Option<f32>,
    pub layer_scale_y: Option<f32>,
    pub scale_keyframes: Option<Vec<Keyframe<f32>>>,
    pub rotation_keyframes: Option<Vec<Keyframe<f32>>>,
    pub opacity_keyframes: Option<Vec<Keyframe<f32>>>,
    pub gain_keyframes: Option<Vec<Keyframe<f32>>>,
    pub brightness_keyframes: Option<Vec<Keyframe<f32>>>,
    pub contrast_keyframes: Option<Vec<Keyframe<f32>>>,
    pub saturation_keyframes: Option<Vec<Keyframe<f32>>>,
    pub crop_x_keyframes: Option<Vec<Keyframe<f32>>>,
    pub crop_y_keyframes: Option<Vec<Keyframe<f32>>>,
    pub crop_w_keyframes: Option<Vec<Keyframe<f32>>>,
    pub crop_h_keyframes: Option<Vec<Keyframe<f32>>>,
}

/// Shortest crop rectangle side a crop edit may shrink to -- mirrors
/// `crates/ui/src/app/mod.rs`'s `CROP_MIN_SIZE`.
const CROP_MIN_SIZE: f32 = 0.05;
/// Mirrors `crates/ui/src/app/mod.rs`'s `MASK_CORNER_RADIUS_RANGE`.
const MASK_CORNER_RADIUS_RANGE: std::ops::RangeInclusive<f32> = 0.0..=1.0;
/// Mirrors `crates/ui/src/app/mod.rs`'s `TRANSITION_DURATION_RANGE`.
const TRANSITION_DURATION_RANGE: std::ops::RangeInclusive<f32> = 0.1..=3.0;

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

/// Rolls the shared boundary between `clip_id` (must be the *earlier* clip of the pair) and its
/// next neighbor to `boundary_secs` -- the "Roll" tool.
#[tauri::command]
pub fn roll_edit_clip(app: AppHandle, clip_id: u64, boundary_secs: f64) -> Result<(), String> {
    undoable(&app, |project| roll_edit(project, clip_id, boundary_secs))
}

/// [`roll_edit_clip`], but for a drag that grabbed `edited_clip_id`'s *start* edge instead of
/// its end -- resolves the earlier clip in the pair (`edited_clip_id`'s previous neighbor) and
/// delegates. A no-op if `edited_clip_id` has no previous neighbor.
#[tauri::command]
pub fn roll_edit_from_start_edge(
    app: AppHandle,
    edited_clip_id: u64,
    boundary_secs: f64,
) -> Result<(), String> {
    undoable(&app, |project| {
        let Some(index) = track_of(project.timeline(), edited_clip_id) else {
            return false;
        };
        let Some(previous_id) = project.timeline().tracks[index].previous_clip_id(edited_clip_id)
        else {
            return false;
        };
        roll_edit(project, previous_id, boundary_secs)
    })
}

/// Slips `clip_id`'s source in/out points by `delta_secs` without moving it on the timeline or
/// changing its duration -- the "Slip" tool.
#[tauri::command]
pub fn slip_clip(app: AppHandle, clip_id: u64, delta_secs: f64) -> Result<(), String> {
    undoable(&app, |project| {
        let max_source_out_secs = clip_asset_max_source_out_secs(project, clip_id);
        let Some(index) = track_of(project.timeline(), clip_id) else {
            return false;
        };
        let Some(clip) = project.timeline_mut().tracks[index].clip_mut(clip_id) else {
            return false;
        };
        clip.slip(delta_secs, max_source_out_secs)
    })
}

/// Slides `clip_id` to `start_secs`, keeping its own duration/source range unchanged -- its
/// immediate neighbors' in/out points adjust to absorb the move. The "Slide" tool.
#[tauri::command]
pub fn slide_clip(app: AppHandle, clip_id: u64, start_secs: f64) -> Result<(), String> {
    undoable(&app, |project| {
        let Some(index) = track_of(project.timeline(), clip_id) else {
            return false;
        };
        let previous_id = project.timeline().tracks[index].previous_clip_id(clip_id);
        let prev_max_source_out_secs =
            previous_id.and_then(|id| clip_asset_max_source_out_secs(project, id));
        project.timeline_mut().tracks[index].slide_clip(
            clip_id,
            start_secs.max(0.0),
            MIN_CLIP_SECS,
            prev_max_source_out_secs,
        )
    })
}

/// Repositions `clip_id` on its own track, dragging every other clip sharing its
/// `composite_id` along with it -- what dragging a composite block's body does. Falls back to a
/// plain same-track move if `clip_id` isn't a composite member.
#[tauri::command]
pub fn move_clip_group(app: AppHandle, clip_id: u64, start_secs: f64) -> Result<(), String> {
    undoable(&app, |project| {
        project
            .timeline_mut()
            .move_clip_with_group(clip_id, start_secs.max(0.0))
    })
}

/// Merges every clip in `clip_ids` into one composite block. A no-op unless there are at least
/// two and they're all on the same track.
#[tauri::command]
pub fn merge_clips_into_composite(app: AppHandle, clip_ids: Vec<u64>) -> Result<(), String> {
    undoable(&app, |project| {
        project
            .timeline_mut()
            .merge_into_composite(&clip_ids.into_iter().collect::<HashSet<u64>>())
    })
}

/// Captures `clip_id` (and its composite group, if any) into the clip clipboard -- `Ctrl+C`.
/// Not undoable: it doesn't touch the project.
#[tauri::command]
pub fn copy_clip(app: AppHandle, clip_id: u64) -> Result<(), String> {
    let state = app.state::<AppState>();
    let copied = state.session().project.timeline().copy_clip_group(clip_id);
    if copied.is_some() {
        *state
            .clip_clipboard
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = copied;
    }
    Ok(())
}

/// [`copy_clip`] followed by deleting every clip it captured -- `Ctrl+X`.
#[tauri::command]
pub fn cut_clip(app: AppHandle, clip_id: u64) -> Result<(), String> {
    copy_clip(app.clone(), clip_id)?;
    let state = app.state::<AppState>();
    let ids: Vec<u64> = state
        .clip_clipboard
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .as_ref()
        .map(|(clips, _)| clips.iter().map(|c| c.id).collect())
        .unwrap_or_default();
    undoable(&app, |project| delete(project.timeline_mut(), &ids, false))
}

/// Pastes the clip clipboard as fresh clip(s) at the playhead -- `Ctrl+V`. A no-op if the
/// clipboard is empty.
#[tauri::command]
pub fn paste_clip(app: AppHandle) -> Result<Vec<u64>, String> {
    let state = app.state::<AppState>();
    let Some((copied, kind)) = state
        .clip_clipboard
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone()
    else {
        return Ok(Vec::new());
    };
    drop(state);
    let playhead = app.state::<AppState>().preview.playhead();
    let mut new_ids = Vec::new();
    undoable(&app, |project| {
        project.timeline_mut().playhead_secs = playhead;
        new_ids = project
            .timeline_mut()
            .paste_clip_group(copied, kind, playhead);
        !new_ids.is_empty()
    })?;
    Ok(new_ids)
}

/// Captures `clip_id`'s gain/freeze/speed/crop/mask/flip/color-adjustment settings into the
/// formatting clipboard -- `Ctrl+Shift+C`.
#[tauri::command]
pub fn copy_clip_formatting(app: AppHandle, clip_id: u64) -> Result<(), String> {
    let state = app.state::<AppState>();
    let formatting = state
        .session()
        .project
        .timeline()
        .tracks
        .iter()
        .flat_map(|t| &t.clips)
        .find(|c| c.id == clip_id)
        .map(|c| c.formatting());
    if formatting.is_some() {
        *state
            .formatting_clipboard
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = formatting;
    }
    Ok(())
}

/// Applies the formatting clipboard onto `clip_id`, without touching its position/trim/
/// composite membership -- `Ctrl+Shift+V`.
#[tauri::command]
pub fn paste_clip_formatting(app: AppHandle, clip_id: u64) -> Result<(), String> {
    let state = app.state::<AppState>();
    let formatting = state
        .formatting_clipboard
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    drop(state);
    let Some(formatting) = formatting else {
        return Ok(());
    };
    undoable(&app, |project| {
        project
            .timeline_mut()
            .clip_mut(clip_id)
            .is_some_and(|clip| {
                clip.apply_formatting(&formatting);
                true
            })
    })
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

/// Splits `clip_id` into `steps` equal-duration pieces (at its current speed) and assigns each
/// piece a linearly interpolated `speed_factor` from `start_speed` to `end_speed` -- mirrors
/// `ui::App::apply_speed_ramp_to_selected_clip` exactly (the Clip menu's "Rampa de velocidade"
/// presets: Slow -> Fast 0.5x->2x, Fast -> Slow 2x->0.5x, both 4 steps).
#[tauri::command]
pub fn apply_speed_ramp_to_clip(
    app: AppHandle,
    clip_id: u64,
    start_speed: f32,
    end_speed: f32,
    steps: usize,
) -> Result<(), String> {
    undoable(&app, |project| {
        apply_speed_ramp(
            project.timeline_mut(),
            clip_id,
            start_speed,
            end_speed,
            steps,
        )
    })
}

/// Mirrors `crates/ui/src/app/timeline_ops.rs`'s `apply_speed_ramp_to_selected_clip` -- same
/// split-then-reflow algorithm, `SPEED_FACTOR_RANGE` clamp (0.25..=4.0, matching
/// `apply_patch`'s `speed_factor` clamp already above), and `speed_ramp_end_factor` reset (a
/// prior smooth-ramp application may have left it set, which would make `duration_secs()`
/// ignore the plain `speed_factor` this stepped mode sets).
fn apply_speed_ramp(
    timeline: &mut Timeline,
    clip_id: u64,
    start_speed: f32,
    end_speed: f32,
    steps: usize,
) -> bool {
    if steps < 2 {
        return false;
    }
    let Some(index) = track_of(timeline, clip_id) else {
        return false;
    };
    if timeline.tracks[index].locked {
        return false;
    }
    let Some(clip) = timeline.clip_mut(clip_id) else {
        return false;
    };
    let (start_secs, original_duration_secs) = (clip.start_secs, clip.duration_secs());
    if original_duration_secs <= 0.0 {
        return false;
    }

    let mut next_id = timeline
        .tracks
        .iter()
        .flat_map(|t| &t.clips)
        .map(|c| c.id)
        .max()
        .unwrap_or(0)
        + 1;

    // Split into `steps` equal-duration pieces (at the clip's still-unramped speed) by cutting
    // at each internal boundary left to right, so an earlier split never shifts a later
    // boundary's position.
    let mut clip_ids = vec![clip_id];
    for i in 1..steps {
        let boundary = start_secs + original_duration_secs * (i as f64 / steps as f64);
        if timeline.tracks[index].split_clip_at(boundary, next_id) {
            clip_ids.push(next_id);
            next_id += 1;
        }
    }

    // Assign each piece's ramped speed, then reflow start_secs left to right so the pieces stay
    // contiguous despite each one's own duration now changing.
    let ramped_steps = clip_ids.len();
    let mut cursor_secs = start_secs;
    for (i, &id) in clip_ids.iter().enumerate() {
        let t = if ramped_steps > 1 {
            i as f32 / (ramped_steps - 1) as f32
        } else {
            0.0
        };
        let speed = (start_speed + (end_speed - start_speed) * t).clamp(0.25, 4.0);
        if let Some(c) = timeline.clip_mut(id) {
            c.speed_factor = speed;
            c.speed_ramp_end_factor = None;
            c.start_secs = cursor_secs;
            cursor_secs += c.duration_secs();
        }
    }
    true
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

/// The source asset's own full duration for `clip_id`'s footage, if both the clip and its asset
/// can be found -- the upper bound roll/slip/slide edits pass so they can't ask for footage past
/// the actual source media's end. Mirrors `trim`'s own inline lookup below.
fn clip_asset_max_source_out_secs(project: &Project, clip_id: u64) -> Option<f64> {
    let asset_id = project
        .timeline()
        .tracks
        .iter()
        .flat_map(|t| &t.clips)
        .find(|c| c.id == clip_id)
        .map(|c| c.asset_id)?;
    project
        .media_library
        .iter()
        .find(|a| a.id == asset_id)
        .map(|a| a.duration_secs)
}

fn roll_edit(project: &mut Project, clip_id: u64, boundary_secs: f64) -> bool {
    let max_source_out_secs = clip_asset_max_source_out_secs(project, clip_id);
    let Some(index) = track_of(project.timeline(), clip_id) else {
        return false;
    };
    project.timeline_mut().tracks[index].roll_edit(
        clip_id,
        boundary_secs,
        MIN_CLIP_SECS,
        max_source_out_secs,
    )
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
            // Text/shape overlays: removed without ripple (they don't carry the edit).
            changed |= super::overlays::remove_overlay(timeline, clip_id);
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
    if let Some(crop_x) = patch.crop_x {
        clip.crop_x = crop_x.clamp(0.0, 1.0);
    }
    if let Some(crop_y) = patch.crop_y {
        clip.crop_y = crop_y.clamp(0.0, 1.0);
    }
    if let Some(crop_w) = patch.crop_w {
        clip.crop_w = crop_w.clamp(CROP_MIN_SIZE, 1.0);
    }
    if let Some(crop_h) = patch.crop_h {
        clip.crop_h = crop_h.clamp(CROP_MIN_SIZE, 1.0);
    }
    if let Some(mask_shape) = patch.mask_shape {
        clip.mask_shape = mask_shape;
    }
    if let Some(mask_corner_radius) = patch.mask_corner_radius {
        clip.mask_corner_radius = mask_corner_radius.clamp(
            *MASK_CORNER_RADIUS_RANGE.start(),
            *MASK_CORNER_RADIUS_RANGE.end(),
        );
    }
    if let Some(transition_in) = patch.transition_in {
        clip.transition_in = transition_in;
    }
    if let Some(transition_duration_secs) = patch.transition_duration_secs {
        clip.transition_duration_secs = transition_duration_secs.clamp(
            *TRANSITION_DURATION_RANGE.start(),
            *TRANSITION_DURATION_RANGE.end(),
        );
    }
    if let Some(blend_mode) = patch.blend_mode {
        clip.blend_mode = blend_mode;
    }
    if let Some(keyframes) = patch.position_keyframes.clone() {
        clip.position_keyframes = keyframes;
    }
    if let Some(scale_x) = patch.layer_scale_x {
        clip.layer_scale_x = scale_x.clamp(CROP_MIN_SIZE, 8.0);
    }
    if let Some(scale_y) = patch.layer_scale_y {
        clip.layer_scale_y = scale_y.clamp(CROP_MIN_SIZE, 8.0);
    }
    if let Some(keyframes) = patch.scale_keyframes.clone() {
        clip.scale_keyframes = keyframes;
    }
    if let Some(keyframes) = patch.rotation_keyframes.clone() {
        clip.rotation_keyframes = keyframes;
    }
    if let Some(keyframes) = patch.opacity_keyframes.clone() {
        clip.opacity_keyframes = keyframes;
    }
    if let Some(keyframes) = patch.gain_keyframes.clone() {
        clip.gain_keyframes = keyframes;
    }
    if let Some(keyframes) = patch.brightness_keyframes.clone() {
        clip.brightness_keyframes = keyframes;
    }
    if let Some(keyframes) = patch.contrast_keyframes.clone() {
        clip.contrast_keyframes = keyframes;
    }
    if let Some(keyframes) = patch.saturation_keyframes.clone() {
        clip.saturation_keyframes = keyframes;
    }
    if let Some(keyframes) = patch.crop_x_keyframes.clone() {
        clip.crop_x_keyframes = keyframes;
    }
    if let Some(keyframes) = patch.crop_y_keyframes.clone() {
        clip.crop_y_keyframes = keyframes;
    }
    if let Some(keyframes) = patch.crop_w_keyframes.clone() {
        clip.crop_w_keyframes = keyframes;
    }
    if let Some(keyframes) = patch.crop_h_keyframes.clone() {
        clip.crop_h_keyframes = keyframes;
    }
    *clip != before
}

#[cfg(test)]
#[path = "timeline_test.rs"]
mod tests;
