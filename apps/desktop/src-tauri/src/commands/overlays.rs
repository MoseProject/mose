// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! Text and shape overlay clips: creation at the playhead, timeline move/trim, and inspector
//! edits. Same one-undo-step-per-commit contract as [`super::timeline`]; the preview engine
//! picks the change up from the published timeline (text/shape layers force a reopen).

use avcore::font_catalog::{self, FontCategory};
use avcore::timeline::{
    ShapeClip, ShapeKind, ShapePreset, TextAlign, TextClip, TextFontFamily, TextFontStyle,
    Timeline, Track, TrackKind,
};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use super::timeline::TrimEdge;
use super::undoable;

/// Shortest overlay a trim may leave behind (egui's duration field floor).
const MIN_OVERLAY_SECS: f64 = 0.1;
/// egui's font size slider range.
const FONT_SIZE_RANGE: (f32, f32) = (10.0, 120.0);
/// egui's shape width/height slider floor.
const MIN_SHAPE_SIZE: f32 = 0.01;

/// Text inspector changes; `None` fields are left untouched.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextClipPatch {
    pub text: Option<String>,
    pub font_family: Option<TextFontFamily>,
    pub font_style: Option<TextFontStyle>,
    pub font_size: Option<f32>,
    pub color_rgba: Option<[u8; 4]>,
    pub text_align: Option<TextAlign>,
    pub pos_x: Option<f32>,
    pub pos_y: Option<f32>,
    pub highlight_enabled: Option<bool>,
    pub highlight_color_rgba: Option<[u8; 4]>,
}

/// Shape inspector changes; `None` fields are left untouched. `opacity` is the color's alpha.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShapeClipPatch {
    pub shape_kind: Option<ShapePreset>,
    pub color_rgb: Option<[u8; 3]>,
    pub opacity: Option<u8>,
    pub center_x: Option<f32>,
    pub center_y: Option<f32>,
    pub width: Option<f32>,
    pub height: Option<f32>,
}

/// Static data the inspectors need from `core`: the bundled font catalog and each shape
/// preset's geometry, so the webview can classify a clip's `shape_kind` without its own copy
/// of the vertex lists.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayCatalog {
    pub fonts: Vec<FontOption>,
    pub shape_presets: Vec<ShapePresetOption>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FontOption {
    /// The persisted `TextFontFamily` value (a `font_catalog` slug).
    pub id: String,
    pub display_name: &'static str,
    pub category: &'static str,
    pub supports_bold: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShapePresetOption {
    pub preset: ShapePreset,
    pub kind: ShapeKind,
}

#[tauri::command]
pub fn overlay_catalog() -> OverlayCatalog {
    catalog()
}

/// Adds a text clip at the playhead on the first Text track (created when missing), like
/// egui's "+ Add text". `text` is the localized placeholder content. Returns the new clip's id
/// so the webview can select it, `None` when the target track is locked.
#[tauri::command]
pub fn add_text_clip(app: AppHandle, text: String) -> Result<Option<u64>, String> {
    let mut created = None;
    undoable(&app, |project| {
        created = insert_text(project.timeline_mut(), text);
        created.is_some()
    })?;
    Ok(created)
}

/// Adds the default rectangle at the playhead on the first Shape track (created when missing).
#[tauri::command]
pub fn add_shape_clip(app: AppHandle) -> Result<Option<u64>, String> {
    let mut created = None;
    undoable(&app, |project| {
        created = insert_shape(project.timeline_mut());
        created.is_some()
    })?;
    Ok(created)
}

#[tauri::command]
pub fn move_overlay_clip(
    app: AppHandle,
    clip_id: u64,
    track_id: u64,
    start_secs: f64,
) -> Result<(), String> {
    undoable(&app, |project| {
        move_overlay(project.timeline_mut(), clip_id, track_id, start_secs)
    })
}

#[tauri::command]
pub fn trim_overlay_clip(
    app: AppHandle,
    clip_id: u64,
    edge: TrimEdge,
    position_secs: f64,
) -> Result<(), String> {
    undoable(&app, |project| {
        trim_overlay(project.timeline_mut(), clip_id, edge, position_secs)
    })
}

#[tauri::command]
pub fn update_text_clip(app: AppHandle, clip_id: u64, patch: TextClipPatch) -> Result<(), String> {
    undoable(&app, |project| {
        patch_text(project.timeline_mut(), clip_id, patch)
    })
}

#[tauri::command]
pub fn update_shape_clip(
    app: AppHandle,
    clip_id: u64,
    patch: ShapeClipPatch,
) -> Result<(), String> {
    undoable(&app, |project| {
        patch_shape(project.timeline_mut(), clip_id, patch)
    })
}

pub(crate) fn catalog() -> OverlayCatalog {
    let fonts = TextFontFamily::ALL
        .into_iter()
        .filter_map(|family| {
            let entry = font_catalog::find_family(family.family_id())?;
            Some(FontOption {
                id: family.family_id().to_string(),
                display_name: entry.display_name,
                category: category_name(entry.category),
                supports_bold: family.supports_bold(),
            })
        })
        .collect();
    let shape_presets = ShapePreset::PICKABLE
        .into_iter()
        .filter_map(|preset| {
            preset
                .to_kind()
                .map(|kind| ShapePresetOption { preset, kind })
        })
        .collect();
    OverlayCatalog {
        fonts,
        shape_presets,
    }
}

fn category_name(category: FontCategory) -> &'static str {
    match category {
        FontCategory::Sans => "Sans",
        FontCategory::Display => "Display",
        FontCategory::Serif => "Serif",
        FontCategory::Handwritten => "Handwritten",
        FontCategory::Monospace => "Monospace",
        FontCategory::International => "International",
    }
}

pub(crate) fn insert_text(timeline: &mut Timeline, text: String) -> Option<u64> {
    let index = timeline.resolve_or_create_track(TrackKind::Text, None);
    if timeline.tracks[index].locked {
        return None;
    }
    let id = timeline.next_clip_id();
    let start = timeline.playhead_secs;
    timeline.tracks[index]
        .text_clips
        .push(TextClip::new(id, start, text));
    Some(id)
}

pub(crate) fn insert_shape(timeline: &mut Timeline) -> Option<u64> {
    let index = timeline.resolve_or_create_track(TrackKind::Shape, None);
    if timeline.tracks[index].locked {
        return None;
    }
    let id = timeline.next_clip_id();
    let start = timeline.playhead_secs;
    timeline.tracks[index]
        .shape_clips
        .push(ShapeClip::new(id, start));
    Some(id)
}

/// Index of the track holding overlay `clip_id`.
fn overlay_track(timeline: &Timeline, clip_id: u64) -> Option<usize> {
    timeline
        .tracks
        .iter()
        .position(|track| has_overlay(track, clip_id))
}

fn has_overlay(track: &Track, clip_id: u64) -> bool {
    track.text_clips.iter().any(|c| c.id == clip_id)
        || track.shape_clips.iter().any(|c| c.id == clip_id)
}

/// Mutable `(start_secs, duration_secs)` of overlay `clip_id` on `track`.
fn overlay_span(track: &mut Track, clip_id: u64) -> Option<(&mut f64, &mut f64)> {
    if let Some(clip) = track.text_clips.iter_mut().find(|c| c.id == clip_id) {
        return Some((&mut clip.start_secs, &mut clip.duration_secs));
    }
    track
        .shape_clips
        .iter_mut()
        .find(|c| c.id == clip_id)
        .map(|clip| (&mut clip.start_secs, &mut clip.duration_secs))
}

/// Moves an overlay to `start_secs`, optionally onto another unlocked track of the same kind.
pub(crate) fn move_overlay(
    timeline: &mut Timeline,
    clip_id: u64,
    track_id: u64,
    start_secs: f64,
) -> bool {
    let Some(source) = overlay_track(timeline, clip_id) else {
        return false;
    };
    let Some(target) = timeline.tracks.iter().position(|t| t.id == track_id) else {
        return false;
    };
    if timeline.tracks[source].locked
        || timeline.tracks[target].locked
        || timeline.tracks[target].kind != timeline.tracks[source].kind
    {
        return false;
    }
    let start_secs = start_secs.max(0.0);
    if source == target {
        let Some((start, _)) = overlay_span(&mut timeline.tracks[source], clip_id) else {
            return false;
        };
        let changed = *start != start_secs;
        *start = start_secs;
        return changed;
    }
    let source_track = &mut timeline.tracks[source];
    if let Some(position) = source_track.text_clips.iter().position(|c| c.id == clip_id) {
        let mut clip = source_track.text_clips.remove(position);
        clip.start_secs = start_secs;
        timeline.tracks[target].text_clips.push(clip);
    } else if let Some(position) = source_track
        .shape_clips
        .iter()
        .position(|c| c.id == clip_id)
    {
        let mut clip = source_track.shape_clips.remove(position);
        clip.start_secs = start_secs;
        timeline.tracks[target].shape_clips.push(clip);
    } else {
        return false;
    }
    true
}

/// Moves one edge of an overlay to `position_secs`, keeping at least [`MIN_OVERLAY_SECS`].
/// Overlays have no source media, so neither edge is bounded by a source duration.
pub(crate) fn trim_overlay(
    timeline: &mut Timeline,
    clip_id: u64,
    edge: TrimEdge,
    position_secs: f64,
) -> bool {
    let Some(index) = overlay_track(timeline, clip_id) else {
        return false;
    };
    if timeline.tracks[index].locked {
        return false;
    }
    let Some((start, duration)) = overlay_span(&mut timeline.tracks[index], clip_id) else {
        return false;
    };
    let end = *start + *duration;
    let (new_start, new_end) = match edge {
        TrimEdge::Start => (position_secs.max(0.0).min(end - MIN_OVERLAY_SECS), end),
        TrimEdge::End => (*start, position_secs.max(*start + MIN_OVERLAY_SECS)),
    };
    if new_start < 0.0 || (new_start == *start && new_end == end) {
        return false;
    }
    *start = new_start;
    *duration = new_end - new_start;
    true
}

/// Removes overlay `clip_id` (no ripple: overlays sit above the edit, like egui's overlay
/// delete). Returns whether it was found on an unlocked track.
pub(crate) fn remove_overlay(timeline: &mut Timeline, clip_id: u64) -> bool {
    let Some(index) = overlay_track(timeline, clip_id) else {
        return false;
    };
    let track = &mut timeline.tracks[index];
    if track.locked {
        return false;
    }
    track.text_clips.retain(|c| c.id != clip_id);
    track.shape_clips.retain(|c| c.id != clip_id);
    true
}

pub(crate) fn patch_text(timeline: &mut Timeline, clip_id: u64, patch: TextClipPatch) -> bool {
    let Some(index) = overlay_track(timeline, clip_id) else {
        return false;
    };
    let track = &mut timeline.tracks[index];
    if track.locked {
        return false;
    }
    let Some(clip) = track.text_clip_mut(clip_id) else {
        return false;
    };
    let before = clip.clone();
    if let Some(text) = patch.text {
        clip.text = text;
    }
    if let Some(family) = patch.font_family {
        clip.set_font_family(family);
    }
    if let Some(style) = patch.font_style {
        // Bold only exists for families that ship a bold face (egui disables the combo).
        if style == TextFontStyle::Regular || clip.font_family.supports_bold() {
            clip.font_style = style;
        }
    }
    if let Some(size) = patch.font_size.filter(|v| v.is_finite()) {
        clip.font_size = size.clamp(FONT_SIZE_RANGE.0, FONT_SIZE_RANGE.1);
    }
    if let Some(color) = patch.color_rgba {
        clip.color_rgba = color;
    }
    if let Some(align) = patch.text_align {
        clip.text_align = align;
    }
    if let Some(x) = patch.pos_x.filter(|v| v.is_finite()) {
        clip.pos_x = x.clamp(0.0, 1.0);
    }
    if let Some(y) = patch.pos_y.filter(|v| v.is_finite()) {
        clip.pos_y = y.clamp(0.0, 1.0);
    }
    if let Some(enabled) = patch.highlight_enabled {
        clip.highlight_enabled = enabled;
    }
    if let Some(color) = patch.highlight_color_rgba {
        clip.highlight_color_rgba = color;
    }
    *clip != before
}

pub(crate) fn patch_shape(timeline: &mut Timeline, clip_id: u64, patch: ShapeClipPatch) -> bool {
    let Some(index) = overlay_track(timeline, clip_id) else {
        return false;
    };
    let track = &mut timeline.tracks[index];
    if track.locked {
        return false;
    }
    let Some(clip) = track.shape_clip_mut(clip_id) else {
        return false;
    };
    let before = clip.clone();
    if let Some(kind) = patch.shape_kind.and_then(ShapePreset::to_kind) {
        clip.shape_kind = kind;
    }
    if let Some([r, g, b]) = patch.color_rgb {
        clip.color_rgba = [r, g, b, clip.color_rgba[3]];
    }
    if let Some(alpha) = patch.opacity {
        clip.color_rgba[3] = alpha;
    }
    if let Some(x) = patch.center_x.filter(|v| v.is_finite()) {
        clip.center_x = x.clamp(0.0, 1.0);
    }
    if let Some(y) = patch.center_y.filter(|v| v.is_finite()) {
        clip.center_y = y.clamp(0.0, 1.0);
    }
    if let Some(width) = patch.width.filter(|v| v.is_finite()) {
        clip.width = width.clamp(MIN_SHAPE_SIZE, 1.0);
    }
    if let Some(height) = patch.height.filter(|v| v.is_finite()) {
        clip.height = height.clamp(MIN_SHAPE_SIZE, 1.0);
    }
    *clip != before
}

#[cfg(test)]
#[path = "overlays_test.rs"]
mod tests;
