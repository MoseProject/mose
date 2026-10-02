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

mod clip;
mod marker;
mod shape_clip;
mod text_clip;
mod track;

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

pub use clip::{
    BlendMode, ClipFormatting, ClipInstance, ColorFilter, LayerTemplate, MaskShape, TransitionType,
};
pub use marker::{Marker, MarkerKind};
pub use shape_clip::{ShapeClip, ShapeKind, ShapePreset};
pub use text_clip::{
    TextAlign, TextClip, TextDirection, TextFontFamily, TextFontStyle, WordTiming,
};
pub use track::{AudioRole, Track, TrackKind};

/// A project's full set of tracks plus the current playhead position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Timeline {
    pub tracks: Vec<Track>,
    pub playhead_secs: f64,
    /// Review/comment markers — `#[serde(default)]` so a project saved before this field
    /// existed still loads (empty marker list), per this crate's struct-map `.ocproj` format
    /// (see `CLAUDE.md`).
    #[serde(default)]
    pub markers: Vec<Marker>,
    /// Multicam groups (P2 item 10, "Multicam editing") — `#[serde(default)]` so a project saved
    /// before this field existed still loads (empty group list), same convention as `markers`.
    #[serde(default)]
    pub multicam_groups: Vec<MulticamGroup>,
}

/// A set of `Video` tracks recorded simultaneously from different sources (e.g. game capture,
/// webcam, mic-facecam) that have been synced against each other by [`crate::multicam_sync`], so
/// switching which one plays at a given moment ("switch to angle 2") is a matter of retargeting
/// a clip on `program_track_id` to the right other member's asset at the right (offset-adjusted)
/// source time — see [`Timeline::switch_multicam_angle`]. Angles are ordinary [`Track`]s, not a
/// new [`TrackKind`]: this group is purely a sidecar grouping + offset record, same non-invasive
/// shape [`Marker`] uses, so no existing per-track/per-clip code needs to know about multicam at
/// all except `program_track_id`'s visibility (every non-program member is hidden — see
/// [`Timeline::add_multicam_group`] — so only the currently active angle actually renders/
/// exports; the others stay in the project purely as switchable source material).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MulticamGroup {
    pub id: u64,
    pub name: String,
    /// Every track in this group, `program_track_id` included, in a stable, user-meaningful
    /// order — index 0 is "angle 1", index 1 is "angle 2", etc., what number-key angle switching
    /// refers to.
    pub member_track_ids: Vec<u64>,
    /// Which member is currently the one visible/exported track. Always one of
    /// `member_track_ids`.
    pub program_track_id: u64,
    /// Each member track's sync offset in seconds, relative to every other member: for the same
    /// real-world moment, `member_track_time = other_member_track_time + (offset[member] -
    /// offset[other_member])`. The track chosen as the sync reference when the group was created
    /// has offset `0.0`; every other member's offset is `crate::multicam_sync::
    /// compute_sync_offset_secs`'s result against that reference. Absent an entry (shouldn't
    /// happen for a member_track_ids member, but keeps lookups total) is treated as `0.0`.
    pub sync_offsets_secs: HashMap<u64, f64>,
}

impl MulticamGroup {
    /// This member's sync offset, or `0.0` if `track_id` isn't in `sync_offsets_secs` (should
    /// only happen for a `track_id` that isn't actually a member of this group).
    pub fn offset_secs(&self, track_id: u64) -> f64 {
        self.sync_offsets_secs
            .get(&track_id)
            .copied()
            .unwrap_or(0.0)
    }
}

impl Timeline {
    /// The next free clip id across every track, including text and shape clips -- one past
    /// the highest id in use, `1` for an empty timeline. Clip ids are unique across all three
    /// clip kinds, so every frontend that inserts a clip must allocate through this.
    pub fn next_clip_id(&self) -> u64 {
        self.tracks
            .iter()
            .flat_map(|track| {
                track
                    .clips
                    .iter()
                    .map(|clip| clip.id)
                    .chain(track.text_clips.iter().map(|clip| clip.id))
                    .chain(track.shape_clips.iter().map(|clip| clip.id))
            })
            .max()
            .unwrap_or(0)
            + 1
    }

    /// Appends a brand-new, empty track of `kind` -- never reuses an existing one. Named
    /// `V<n>`/`A<n>`/`T<n>`/`S<n>`, `n` one past the count of existing tracks of that kind.
    /// Returns the new track's index.
    pub fn create_track(&mut self, kind: TrackKind) -> usize {
        let count = self.tracks.iter().filter(|t| t.kind == kind).count();
        self.push_empty_track(kind, format!("{}{}", kind.name_prefix(), count + 1))
    }

    /// The track a new clip of `kind` should land on: `preferred_track_id` if it exists and
    /// matches `kind`, else the first existing track of that kind, else a newly appended
    /// `V1`/`A1`/`T1`/`S1` track. Returns the resolved track's index.
    pub fn resolve_or_create_track(
        &mut self,
        kind: TrackKind,
        preferred_track_id: Option<u64>,
    ) -> usize {
        if let Some(id) = preferred_track_id {
            if let Some(index) = self
                .tracks
                .iter()
                .position(|t| t.id == id && t.kind == kind)
            {
                return index;
            }
        }
        if let Some(index) = self.tracks.iter().position(|t| t.kind == kind) {
            return index;
        }
        self.push_empty_track(kind, format!("{}1", kind.name_prefix()))
    }

    fn push_empty_track(&mut self, kind: TrackKind, name: String) -> usize {
        let track_id = self.tracks.iter().map(|t| t.id).max().unwrap_or(0) + 1;
        self.tracks.push(Track {
            id: track_id,
            name,
            kind,
            clips: Vec::new(),
            text_clips: Vec::new(),
            shape_clips: Vec::new(),
            visible: true,
            audio_role: AudioRole::Unspecified,
            locked: false,
            color_label: None,
        });
        self.tracks.len() - 1
    }

    /// Mutable access to the clip with `clip_id` across all tracks, if it exists. Searches
    /// tracks in order and returns the first match — clip ids are unique within a timeline.
    pub fn clip_mut(&mut self, clip_id: u64) -> Option<&mut ClipInstance> {
        self.tracks.iter_mut().find_map(|t| t.clip_mut(clip_id))
    }

    /// The position, in seconds, where the last clip on any track ends — i.e. how long the
    /// edited sequence runs for. `0.0` for an empty timeline.
    pub fn duration_secs(&self) -> f64 {
        self.tracks
            .iter()
            .map(Track::duration_secs)
            .fold(0.0, f64::max)
    }

    /// Adds a new [`Marker`] at `position_secs` (clamped to `0.0`) with `kind`, `label` empty
    /// and `completed: false`, and returns its freshly assigned id — one past the highest
    /// existing marker id, `1` if there are none yet, same "max + 1" convention every other
    /// timeline entity's id assignment already uses (see `next_clip_id` in `ui`).
    pub fn add_marker(&mut self, position_secs: f64, kind: MarkerKind) -> u64 {
        let id = self.markers.iter().map(|m| m.id).max().unwrap_or(0) + 1;
        self.markers.push(Marker {
            id,
            position_secs: position_secs.max(0.0),
            label: String::new(),
            kind,
            completed: false,
        });
        id
    }

    /// Removes the marker with `marker_id`, if any. `true` if a marker was actually removed.
    pub fn remove_marker(&mut self, marker_id: u64) -> bool {
        let before = self.markers.len();
        self.markers.retain(|m| m.id != marker_id);
        self.markers.len() != before
    }

    /// Mutable access to the marker with `marker_id`, if it exists.
    pub fn marker_mut(&mut self, marker_id: u64) -> Option<&mut Marker> {
        self.markers.iter_mut().find(|m| m.id == marker_id)
    }

    /// Every marker sorted by `position_secs` ascending — what a searchable Timeline Index
    /// panel (and the ruler's own left-to-right tick rendering) both want, rather than
    /// insertion order.
    pub fn markers_sorted(&self) -> Vec<&Marker> {
        let mut markers: Vec<&Marker> = self.markers.iter().collect();
        markers.sort_by(|a, b| a.position_secs.total_cmp(&b.position_secs));
        markers
    }

    /// Moves the clip with `clip_id` onto `target_track_id` at `new_start_secs`, removing it
    /// from wherever it currently lives (which may itself be `target_track_id`, for a same-
    /// track reposition — [`Track::move_clip`] is the cheaper path for that specific case, but
    /// this stays correct for it too). No-op (`false`) if the clip or target track don't
    /// exist, `new_start_secs` is negative, or the target track's `kind` doesn't match the
    /// clip's current track's — a video clip can't land on an audio track and vice versa.
    pub fn move_clip_to_track(
        &mut self,
        clip_id: u64,
        target_track_id: u64,
        new_start_secs: f64,
    ) -> bool {
        if new_start_secs < 0.0 {
            return false;
        }
        let Some(source_index) = self
            .tracks
            .iter()
            .position(|t| t.clips.iter().any(|c| c.id == clip_id))
        else {
            return false;
        };
        let Some(target_index) = self.tracks.iter().position(|t| t.id == target_track_id) else {
            return false;
        };
        if self.tracks[source_index].kind != self.tracks[target_index].kind {
            return false;
        }

        let clip_index = self.tracks[source_index]
            .clips
            .iter()
            .position(|c| c.id == clip_id)
            .expect("source_index was found by locating a track containing this clip_id");
        let mut clip = self.tracks[source_index].clips.remove(clip_index);
        clip.start_secs = new_start_secs;
        self.tracks[target_index].clips.push(clip);
        true
    }

    /// [`Track::move_clip`], but if `clip_id` is a composite block member every other clip
    /// sharing its `composite_id` moves by the same delta, on the same track — what dragging a
    /// composite block's body does, so the whole block reads as one clip per `request.md`'s
    /// Fase 3 "blocos compostos" spec. Falls back to a plain [`Track::move_clip`] if `clip_id`
    /// isn't a composite member. No-op (`false`) if the clip isn't found.
    pub fn move_clip_with_group(&mut self, clip_id: u64, new_start_secs: f64) -> bool {
        let Some((old_start_secs, composite_id, track_index)) =
            self.tracks.iter().enumerate().find_map(|(index, t)| {
                let dragged = t.clips.iter().find(|c| c.id == clip_id)?;
                Some((dragged.start_secs, dragged.composite_id, index))
            })
        else {
            return false;
        };
        let Some(group) = composite_id else {
            return self.tracks[track_index].move_clip(clip_id, new_start_secs);
        };
        let delta = new_start_secs - old_start_secs;
        let updates: Vec<(u64, f64)> = self.tracks[track_index]
            .clips
            .iter()
            .filter(|c| c.composite_id == Some(group))
            .map(|c| (c.id, c.start_secs + delta))
            .collect();
        let mut changed = false;
        for (id, start_secs) in updates {
            changed |= self.tracks[track_index].move_clip(id, start_secs);
        }
        changed
    }

    /// Captures `clip_id`'s clip data for [`Timeline::paste_clip_group`], along with the track
    /// kind it lives on. If `clip_id` is a composite block member, every clip sharing its
    /// `composite_id` is captured too (not just the one asked for), so a copy/cut round-trips
    /// the whole block as one unit through [`Timeline::paste_clip_group`]. `None` if `clip_id`
    /// isn't found.
    pub fn copy_clip_group(&self, clip_id: u64) -> Option<(Vec<ClipInstance>, TrackKind)> {
        self.tracks.iter().find_map(|t| {
            let selected = t.clips.iter().find(|c| c.id == clip_id)?;
            let group: Vec<ClipInstance> = match selected.composite_id {
                Some(composite_id) => t
                    .clips
                    .iter()
                    .filter(|c| c.composite_id == Some(composite_id))
                    .cloned()
                    .collect(),
                None => vec![selected.clone()],
            };
            Some((group, t.kind))
        })
    }

    /// Pastes `copied` (from [`Timeline::copy_clip_group`]) as fresh, newly-id'd clips at
    /// `playhead_secs`, on a matching-`kind` track ([`Timeline::resolve_or_create_track`]). The
    /// earliest copied clip lands exactly on the playhead; every other copied clip keeps its
    /// original offset relative to it, and its `color_label` is dropped. More than one copied
    /// clip is assigned one fresh shared `composite_id` so the block pastes back as one unit; a
    /// single copied clip pastes standalone. Returns the pasted clips' fresh ids (empty if
    /// `copied` is empty).
    pub fn paste_clip_group(
        &mut self,
        copied: Vec<ClipInstance>,
        kind: TrackKind,
        playhead_secs: f64,
    ) -> Vec<u64> {
        if copied.is_empty() {
            return Vec::new();
        }
        let track_index = self.resolve_or_create_track(kind, None);
        let group_min_start = copied
            .iter()
            .map(|c| c.start_secs)
            .fold(f64::INFINITY, f64::min);
        let new_composite_id = (copied.len() > 1).then(|| {
            self.tracks[track_index]
                .clips
                .iter()
                .filter_map(|c| c.composite_id)
                .max()
                .unwrap_or(0)
                + 1
        });
        let mut new_ids = Vec::with_capacity(copied.len());
        for mut clip in copied {
            let clip_id = self.next_clip_id();
            clip.id = clip_id;
            clip.start_secs = playhead_secs + (clip.start_secs - group_min_start);
            clip.composite_id = new_composite_id;
            clip.color_label = None;
            self.tracks[track_index].clips.push(clip);
            new_ids.push(clip_id);
        }
        new_ids
    }

    /// Merges every clip in `ids` into one composite block, assigning them all a fresh
    /// `composite_id` — what "Mesclar em bloco composto" does. No-op (`false`), leaving every
    /// clip's `composite_id` untouched, unless `ids` has at least two entries and they're all
    /// on the same track — composite blocks don't span tracks yet.
    pub fn merge_into_composite(&mut self, ids: &HashSet<u64>) -> bool {
        if ids.len() < 2 {
            return false;
        }
        let Some(index) = self
            .tracks
            .iter()
            .position(|t| ids.iter().all(|id| t.clips.iter().any(|c| c.id == *id)))
        else {
            return false;
        };
        let group_id = self.tracks[index]
            .clips
            .iter()
            .filter_map(|c| c.composite_id)
            .max()
            .unwrap_or(0)
            + 1;
        for clip in &mut self.tracks[index].clips {
            if ids.contains(&clip.id) {
                clip.composite_id = Some(group_id);
            }
        }
        true
    }
}

impl Timeline {
    /// Adds a new [`MulticamGroup`] over `member_track_ids` (must have at least 2 members, all
    /// existing `Video` tracks, or this is a no-op returning `None`) with `program_track_id`'s
    /// offset implicitly `0.0` unless overridden in `sync_offsets_secs`, and hides every other
    /// member (`Track::visible = false`) so only the program track actually renders/exports —
    /// the rest stay in the project purely as switchable source material for
    /// [`Timeline::switch_multicam_angle`]. Returns the freshly assigned group id ("max + 1",
    /// same convention every other timeline entity's id assignment uses), or `None` if
    /// `program_track_id` isn't one of `member_track_ids`, fewer than 2 members are given, or
    /// any member id doesn't name an existing `Video` track.
    pub fn add_multicam_group(
        &mut self,
        name: String,
        member_track_ids: Vec<u64>,
        program_track_id: u64,
        sync_offsets_secs: HashMap<u64, f64>,
    ) -> Option<u64> {
        if member_track_ids.len() < 2 || !member_track_ids.contains(&program_track_id) {
            return None;
        }
        if !member_track_ids.iter().all(|id| {
            self.tracks
                .iter()
                .any(|t| t.id == *id && t.kind == TrackKind::Video)
        }) {
            return None;
        }

        for track_id in &member_track_ids {
            if *track_id == program_track_id {
                continue;
            }
            if let Some(track) = self.tracks.iter_mut().find(|t| t.id == *track_id) {
                track.visible = false;
            }
        }

        let id = self.multicam_groups.iter().map(|g| g.id).max().unwrap_or(0) + 1;
        self.multicam_groups.push(MulticamGroup {
            id,
            name,
            member_track_ids,
            program_track_id,
            sync_offsets_secs,
        });
        Some(id)
    }

    /// Mutable access to the multicam group with `group_id`, if it exists.
    pub fn multicam_group_mut(&mut self, group_id: u64) -> Option<&mut MulticamGroup> {
        self.multicam_groups.iter_mut().find(|g| g.id == group_id)
    }

    /// Removes the multicam group with `group_id` and re-shows every one of its member tracks
    /// (undoing the hide [`Timeline::add_multicam_group`] applied) — the tracks and their clips
    /// themselves are left alone, only the grouping/offset record and the non-program members'
    /// visibility are undone. `true` if a group was actually removed.
    pub fn remove_multicam_group(&mut self, group_id: u64) -> bool {
        let Some(index) = self.multicam_groups.iter().position(|g| g.id == group_id) else {
            return false;
        };
        let group = self.multicam_groups.remove(index);
        for track_id in &group.member_track_ids {
            if let Some(track) = self.tracks.iter_mut().find(|t| t.id == *track_id) {
                track.visible = true;
            }
        }
        true
    }

    /// Switches `group_id`'s active angle, at `at_secs` (timeline-relative), to
    /// `member_track_ids[angle_index]`: splits the program track's clip covering `at_secs` (via
    /// [`Track::split_clip_at`], `new_clip_id`) so the switch takes effect exactly at that point,
    /// then retargets the resulting piece's `asset_id`/`source_in_secs`/`source_out_secs` to
    /// whatever the target angle's own track was showing at the sync-offset-adjusted equivalent
    /// time — same source-window duration, different source. Resets the retargeted piece's
    /// `speed_factor` to `1.0` (a speed-ramped multicam switch is out of scope for this pass) and
    /// clears its stale `background_removal_mask_path`/keyframes the same way an ordinary split
    /// already does for its second half, since they're generated for/apply to the pre-switch
    /// source.
    ///
    /// No-op (`false`) if: the group or `angle_index` don't exist; the target angle is already
    /// the program track; the program track has no clip covering `at_secs`; or the target angle's
    /// track has no clip covering the offset-adjusted time (e.g. that source hadn't started
    /// recording yet at this moment).
    pub fn switch_multicam_angle(
        &mut self,
        group_id: u64,
        angle_index: usize,
        at_secs: f64,
        new_clip_id: u64,
    ) -> bool {
        let Some(group) = self.multicam_groups.iter().find(|g| g.id == group_id) else {
            return false;
        };
        let Some(&target_track_id) = group.member_track_ids.get(angle_index) else {
            return false;
        };
        if target_track_id == group.program_track_id {
            return false;
        }
        let program_track_id = group.program_track_id;
        let target_time =
            at_secs - group.offset_secs(program_track_id) + group.offset_secs(target_track_id);

        let Some(target_track) = self.tracks.iter().find(|t| t.id == target_track_id) else {
            return false;
        };
        let Some(target_clip) = target_track.clip_at(target_time) else {
            return false;
        };
        let new_asset_id = target_clip.asset_id;
        let new_source_in_secs = target_clip.source_in_secs
            + (target_time - target_clip.start_secs) * target_clip.speed_factor as f64;

        let Some(program_track) = self.tracks.iter_mut().find(|t| t.id == program_track_id) else {
            return false;
        };
        if program_track.clip_at(at_secs).is_none() {
            return false;
        }
        let already_at_boundary = program_track.clips.iter().any(|c| c.start_secs == at_secs);
        if !already_at_boundary && !program_track.split_clip_at(at_secs, new_clip_id) {
            return false;
        }
        // Whether pre-existing or freshly created by the split above, the piece this switch
        // targets is the one starting exactly at `at_secs`.
        let Some(switched_clip) = program_track
            .clips
            .iter_mut()
            .find(|c| c.start_secs == at_secs)
        else {
            return false;
        };
        let switched_duration_secs = (switched_clip.source_out_secs - switched_clip.source_in_secs)
            / switched_clip.speed_factor as f64;
        switched_clip.asset_id = new_asset_id;
        switched_clip.speed_factor = 1.0;
        switched_clip.source_in_secs = new_source_in_secs;
        switched_clip.source_out_secs = new_source_in_secs + switched_duration_secs;
        switched_clip.background_removal_enabled = false;
        switched_clip.background_removal_mask_path = String::new();
        switched_clip.position_keyframes.clear();
        switched_clip.scale_keyframes.clear();
        switched_clip.rotation_keyframes.clear();
        switched_clip.opacity_keyframes.clear();
        true
    }
}
