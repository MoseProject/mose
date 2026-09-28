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

use avcore::timeline::{AudioRole, ShapeClip, ShapeKind, TextClip, TrackKind};

use super::timeline_ops::{create_new_track, next_clip_id, resolve_or_create_track};
use super::App;

impl App {
    /// Appends a new empty video track to the active sequence's timeline. The track's name is
    /// generated as `V<n>` where `n` is one past the count of existing video tracks — so a
    /// timeline with one video track "V1" gets a second track "V2". The new track is always
    /// visible and starts empty; the user drags clips onto it from the media library.
    pub fn add_video_track(&mut self) {
        self.push_undo_snapshot();
        let timeline = self.active_project_mut().timeline_mut();
        create_new_track(timeline, TrackKind::Video);
    }

    /// Toggles the `visible` flag on the track with `track_id` in the active sequence. A no-op
    /// if the track isn't found (shouldn't happen from the timeline UI, but safe regardless).
    pub fn toggle_track_visibility(&mut self, track_id: u64) {
        let timeline = self.active_project_mut().timeline_mut();
        if let Some(track) = timeline.tracks.iter_mut().find(|t| t.id == track_id) {
            track.visible = !track.visible;
        }
    }

    /// Toggles the `locked` flag on the track with `track_id` in the active sequence — the
    /// timeline track header's lock icon (`oca-editor-mock.html`'s `.tl-track-tool`). Same
    /// "metadata about a track" reasoning as [`Self::toggle_track_visibility`]: not undo-tracked.
    pub fn toggle_track_locked(&mut self, track_id: u64) {
        let timeline = self.active_project_mut().timeline_mut();
        if let Some(track) = timeline.tracks.iter_mut().find(|t| t.id == track_id) {
            track.locked = !track.locked;
        }
    }

    /// Sets the `AudioRole` on the track with `track_id` — what the timeline track header's
    /// role picker does (D2, `spec/architecture/differentiators.md`: highlight detection needs
    /// to know which track is the mic vs. game audio). Not undo-tracked, same as
    /// [`Self::toggle_track_visibility`] — metadata about a track, not an edit to its content.
    /// A no-op if the track isn't found.
    pub fn set_track_audio_role(&mut self, track_id: u64, role: avcore::AudioRole) {
        let timeline = self.active_project_mut().timeline_mut();
        if let Some(track) = timeline.tracks.iter_mut().find(|t| t.id == track_id) {
            track.audio_role = role;
        }
    }

    /// Sets `track_id`'s color label ([`avcore::timeline::Track::color_label`]) — what picking
    /// a swatch (or "Limpar") in the timeline track header's context menu does. A no-op if
    /// `track_id` doesn't exist.
    pub fn set_track_color_label(&mut self, track_id: u64, color_label: Option<[u8; 3]>) {
        self.push_undo_snapshot_for_drag();
        let timeline = self.active_project_mut().timeline_mut();
        if let Some(track) = timeline.tracks.iter_mut().find(|t| t.id == track_id) {
            track.color_label = color_label;
        }
    }

    /// Renames `track_id` — Section 49's Track More Menu "Rename Track". A no-op if the track
    /// isn't found or `name` is blank (mirrors `App::renaming_sequence`'s own confirm handling).
    pub fn rename_track(&mut self, track_id: u64, name: String) {
        let name = name.trim().to_string();
        if name.is_empty() {
            return;
        }
        self.push_undo_snapshot();
        let timeline = self.active_project_mut().timeline_mut();
        if let Some(track) = timeline.tracks.iter_mut().find(|t| t.id == track_id) {
            track.name = name;
        }
    }

    /// Duplicates `track_id` — Section 49's "Duplicate Track". The copy gets a fresh id (and
    /// fresh ids for every clip/text-clip/shape-clip it carries, so dragging one copy never
    /// mutates the other) and is inserted directly after the original. A no-op if the track
    /// isn't found.
    pub fn duplicate_track(&mut self, track_id: u64) {
        self.push_undo_snapshot();
        let timeline = self.active_project_mut().timeline_mut();
        let Some(index) = timeline.tracks.iter().position(|t| t.id == track_id) else {
            return;
        };
        let mut next_id = timeline
            .tracks
            .iter()
            .flat_map(|t| {
                std::iter::once(t.id)
                    .chain(t.clips.iter().map(|c| c.id))
                    .chain(t.text_clips.iter().map(|c| c.id))
                    .chain(t.shape_clips.iter().map(|c| c.id))
            })
            .max()
            .unwrap_or(0)
            + 1;
        let mut copy = timeline.tracks[index].clone();
        copy.id = next_id;
        next_id += 1;
        for clip in &mut copy.clips {
            clip.id = next_id;
            next_id += 1;
        }
        for clip in &mut copy.text_clips {
            clip.id = next_id;
            next_id += 1;
        }
        for clip in &mut copy.shape_clips {
            clip.id = next_id;
            next_id += 1;
        }
        timeline.tracks.insert(index + 1, copy);
    }

    /// Moves `track_id` one position earlier in the track list — Section 49's "Move Track Up".
    /// A no-op if the track isn't found or already first.
    pub fn move_track_up(&mut self, track_id: u64) {
        self.push_undo_snapshot();
        let timeline = self.active_project_mut().timeline_mut();
        if let Some(index) = timeline.tracks.iter().position(|t| t.id == track_id) {
            if index > 0 {
                timeline.tracks.swap(index, index - 1);
            }
        }
    }

    /// Moves `track_id` one position later in the track list — Section 49's "Move Track Down".
    /// A no-op if the track isn't found or already last.
    pub fn move_track_down(&mut self, track_id: u64) {
        self.push_undo_snapshot();
        let timeline = self.active_project_mut().timeline_mut();
        if let Some(index) = timeline.tracks.iter().position(|t| t.id == track_id) {
            if index + 1 < timeline.tracks.len() {
                timeline.tracks.swap(index, index + 1);
            }
        }
    }

    /// Deletes `track_id` outright — Section 49's "Delete Track". Confirmation for a track that
    /// actually carries content is the caller's job (`App::deleting_track` + its modal), same
    /// split `App::delete_sequence` already uses; this itself never asks. A no-op if the track
    /// isn't found or it's the timeline's only remaining track (an editor with zero tracks has
    /// nowhere for "+ Adicionar faixa" to make sense of).
    pub fn delete_track(&mut self, track_id: u64) {
        let timeline = self.active_project_mut().timeline_mut();
        if timeline.tracks.len() <= 1 {
            return;
        }
        let Some(index) = timeline.tracks.iter().position(|t| t.id == track_id) else {
            return;
        };
        self.push_undo_snapshot();
        let timeline = self.active_project_mut().timeline_mut();
        timeline.tracks.remove(index);
        if self.selected_clip_id.is_some()
            && !self
                .active_project()
                .timeline()
                .tracks
                .iter()
                .flat_map(|t| &t.clips)
                .any(|c| Some(c.id) == self.selected_clip_id)
        {
            self.selected_clip_id = None;
        }
    }
}

impl App {
    /// Appends a new text track (`TrackKind::Text`) to the active sequence's timeline. The
    /// track is named using [`crate::i18n::Text::DefaultTextTrackName`]. A no-op if the
    /// project has no sequences.
    pub fn add_text_track(&mut self) {
        use crate::i18n::Text;
        self.push_undo_snapshot();
        let locale = self.locale;
        let track_id = {
            let timeline = self.active_project().timeline();
            timeline.tracks.iter().map(|t| t.id).max().unwrap_or(0) + 1
        };
        let name = Text::DefaultTextTrackName.tr(locale).to_string();
        self.active_project_mut()
            .timeline_mut()
            .tracks
            .push(avcore::timeline::Track {
                id: track_id,
                name,
                kind: TrackKind::Text,
                clips: Vec::new(),
                text_clips: Vec::new(),
                shape_clips: Vec::new(),
                visible: true,
                audio_role: AudioRole::Unspecified,
                locked: false,
                color_label: None,
            });
    }

    /// Appends a new [`TextClip`] to the first text track in the active sequence, starting at
    /// the current playhead position and lasting 3 seconds. Auto-creates the track when this is
    /// the first manually inserted text overlay and selects the new clip immediately.
    pub fn add_text_clip(&mut self) {
        use crate::i18n::Text;

        self.push_undo_snapshot();
        let playhead_secs = self.active_project().timeline().playhead_secs;
        let default_text = Text::DefaultTextContent.tr(self.locale).to_string();
        let timeline = self.active_project_mut().timeline_mut();
        let track_index = resolve_or_create_track(timeline, TrackKind::Text, None);
        let clip_id = next_clip_id(timeline);
        timeline.tracks[track_index].text_clips.push(TextClip::new(
            clip_id,
            playhead_secs,
            default_text,
        ));
        self.selected_clip_id = None;
        self.selected_shape_clip_id = None;
        self.selected_text_clip_id = Some(clip_id);
        self.invalidate_preview_rendering();
    }

    /// Creates a new Text Graphic at `(pos_x, pos_y)` (canvas-fraction coordinates, the same
    /// space [`TextClip::pos_x`]/`pos_y` already use) — the Text Tool's own click-to-create
    /// (`CINECUT_PRODUCT_DECISIONS_v1.0.md`, Section 6), distinct from [`Self::add_text_clip`]:
    /// starts with genuinely empty `text` (not the placeholder default) and a fixed position
    /// rather than the toolbar button's own `(0.1, 0.85)` default, since this one is placed by a
    /// real click. Returns the new clip's id, recorded by the caller in
    /// `App::text_tool_pending_empty_clip_id` so an Escape press with nothing typed yet can
    /// discard it (Section 6's own "empty text" rule).
    pub fn add_text_clip_at(&mut self, pos_x: f32, pos_y: f32) -> u64 {
        self.push_undo_snapshot();
        let playhead_secs = self.active_project().timeline().playhead_secs;
        let timeline = self.active_project_mut().timeline_mut();
        let track_index = resolve_or_create_track(timeline, TrackKind::Text, None);
        let clip_id = next_clip_id(timeline);
        let mut clip = TextClip::new(clip_id, playhead_secs, String::new());
        clip.pos_x = pos_x;
        clip.pos_y = pos_y;
        timeline.tracks[track_index].text_clips.push(clip);
        self.selected_clip_id = None;
        self.selected_shape_clip_id = None;
        self.selected_text_clip_id = Some(clip_id);
        self.invalidate_preview_rendering();
        clip_id
    }

    /// Appends a new shape track (`TrackKind::Shape`) to the active sequence's timeline. The
    /// track is named using [`crate::i18n::Text::DefaultShapeTrackName`]. Mirrors
    /// [`App::add_text_track`].
    pub fn add_shape_track(&mut self) {
        use crate::i18n::Text;
        self.push_undo_snapshot();
        let locale = self.locale;
        let track_id = {
            let timeline = self.active_project().timeline();
            timeline.tracks.iter().map(|t| t.id).max().unwrap_or(0) + 1
        };
        let name = Text::DefaultShapeTrackName.tr(locale).to_string();
        self.active_project_mut()
            .timeline_mut()
            .tracks
            .push(avcore::timeline::Track {
                id: track_id,
                name,
                kind: TrackKind::Shape,
                clips: Vec::new(),
                text_clips: Vec::new(),
                shape_clips: Vec::new(),
                visible: true,
                audio_role: AudioRole::Unspecified,
                locked: false,
                color_label: None,
            });
    }

    /// Appends a new [`ShapeClip`] (a default rectangle, centered, half the canvas size) to the
    /// first shape track in the active sequence, starting at the current playhead position and
    /// lasting 3 seconds. Auto-creates a shape track if none exists yet, matching manual text
    /// insertion. Selects the new clip immediately so the properties panel shows its controls.
    pub fn add_shape_clip(&mut self) {
        self.push_undo_snapshot();
        let playhead_secs = self.active_project().timeline().playhead_secs;
        let timeline = self.active_project_mut().timeline_mut();
        let track_index = resolve_or_create_track(timeline, TrackKind::Shape, None);
        let clip_id = next_clip_id(timeline);
        timeline.tracks[track_index]
            .shape_clips
            .push(ShapeClip::new(clip_id, playhead_secs));
        self.selected_clip_id = None;
        self.selected_text_clip_id = None;
        self.selected_shape_clip_id = Some(clip_id);
    }

    /// Enters "draw a custom shape" mode — `request.md`'s Fase 4 "forma personalizada" ask,
    /// the one gap the fixed-preset shapes above don't cover. What the toolbar's "Desenhar
    /// forma" button does: clears any in-progress drawing and starts a fresh, empty point
    /// list. The preview panel (`screens::editor::layer_transform_preview`) reads
    /// [`App::drawing_shape_points`] each frame while it's `Some` and switches into a
    /// click-to-place-vertex mode instead of its usual layer drag/resize handling; see that
    /// function's doc comment for why a loaded preview frame is a precondition. Overwrites
    /// (does not append to) any drawing already in progress.
    pub fn start_drawing_custom_shape(&mut self) {
        self.drawing_shape_points = Some(Vec::new());
    }

    /// Discards the in-progress custom-shape drawing without creating a clip — what pressing
    /// Escape while drawing does.
    pub fn cancel_drawing_custom_shape(&mut self) {
        self.drawing_shape_points = None;
    }

    /// Appends one clicked point (canvas-fraction coordinates — same space as
    /// [`ShapeClip::center_x`]/`_y`, clamped to `0.0..=1.0`) to the in-progress custom shape.
    /// A no-op if [`App::start_drawing_custom_shape`] hasn't been called (or the drawing was
    /// already finished/cancelled) — lets the preview panel call this unconditionally on every
    /// click without checking the mode itself first.
    pub fn push_drawing_shape_point(&mut self, x: f32, y: f32) {
        if let Some(points) = self.drawing_shape_points.as_mut() {
            points.push((x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)));
        }
    }

    /// Finishes the in-progress custom-shape drawing — what pressing Enter with at least 3
    /// points placed does. A no-op (drawing mode stays active) if fewer than 3 points have
    /// been placed yet, since a polygon needs at least a triangle.
    ///
    /// The clicked points are absolute canvas-fraction coordinates; [`ShapeKind::Polygon`]
    /// stores vertices relative to the shape's own local unit square instead (see that
    /// variant's doc comment), so this derives a bounding box across all clicked points,
    /// centers/sizes the new [`ShapeClip`] on it, and re-expresses each point as an offset
    /// from that box's center divided by its width/height. `rotation_deg` starts at `0.0` (the
    /// shape is drawn axis-aligned to how it was clicked) — the properties panel's existing
    /// rotation control still applies afterward, same as any other shape.
    pub fn finish_drawing_custom_shape(&mut self) {
        let Some(points) = self.drawing_shape_points.as_ref() else {
            return;
        };
        if points.len() < 3 {
            return;
        }
        let points = self.drawing_shape_points.take().unwrap();
        self.push_undo_snapshot();

        let min_x = points.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
        let max_x = points.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max);
        let min_y = points.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
        let max_y = points.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
        let center_x = (min_x + max_x) / 2.0;
        let center_y = (min_y + max_y) / 2.0;
        // Floors the bounding box away from zero so near-collinear clicks (e.g. three points
        // almost in a vertical line) can't produce a divide-by-zero below.
        let width = (max_x - min_x).max(0.02);
        let height = (max_y - min_y).max(0.02);
        let local_vertices: Vec<(f32, f32)> = points
            .iter()
            .map(|&(x, y)| ((x - center_x) / width, (y - center_y) / height))
            .collect();

        let playhead_secs = self.active_project().timeline().playhead_secs;
        let timeline = self.active_project_mut().timeline_mut();
        let track_index = resolve_or_create_track(timeline, TrackKind::Shape, None);
        let clip_id = next_clip_id(timeline);
        timeline.tracks[track_index].shape_clips.push(ShapeClip {
            id: clip_id,
            start_secs: playhead_secs,
            duration_secs: 3.0,
            shape_kind: ShapeKind::Polygon(local_vertices),
            center_x,
            center_y,
            center_x_keyframes: vec![],
            center_y_keyframes: vec![],
            width_keyframes: vec![],
            height_keyframes: vec![],
            rotation_keyframes: vec![],
            width,
            height,
            rotation_deg: 0.0,
            color_rgba: [255, 255, 255, 255],
            stroke_thickness_px: 0.0,
        });
        self.selected_clip_id = None;
        self.selected_text_clip_id = None;
        self.selected_shape_clip_id = Some(clip_id);
    }
}
