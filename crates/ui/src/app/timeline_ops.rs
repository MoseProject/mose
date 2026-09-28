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

use avcore::timeline::{AudioRole, ClipInstance, TrackKind};

use super::{App, GAIN_DB_RANGE, SPEED_FACTOR_RANGE};

impl App {
    /// Appends `asset_id` to the timeline as a new, untrimmed clip — what double-clicking an
    /// asset in the media library panel does. Lands on the first track whose kind matches the
    /// asset (video onto video, audio onto audio), auto-creating one (`"V1"`/`"A1"`) if none
    /// exists yet, right after whatever's already there
    /// ([`avcore::timeline::Track::duration_secs`] — `0.0` for an empty/new track). A no-op if
    /// `asset_id` isn't in the active project's media library.
    ///
    /// The new clip's [`ClipInstance::voice_cleanup_enabled`] defaults to the landing track's own
    /// `audio_role == Mic` — CF-03's own "Mic by default, explicit override elsewhere" acceptance
    /// criterion, applied only at this creation moment. Reassigning a track's role later never
    /// retroactively flips already-placed clips; the toggle stays a plain, always-overridable
    /// per-clip field either way.
    pub fn add_asset_to_timeline(&mut self, asset_id: u64) {
        let Some((kind, duration_secs)) = self.asset_kind_and_duration(asset_id) else {
            return;
        };
        self.push_undo_snapshot();
        let was_empty = self
            .active_project()
            .timeline()
            .tracks
            .iter()
            .all(|track| track.clips.is_empty());
        let timeline = self.active_project_mut().timeline_mut();
        let track_index = resolve_or_create_track(timeline, kind, None);
        let start_secs = timeline.tracks[track_index].duration_secs();
        let clip_id = next_clip_id(timeline);
        let voice_cleanup_enabled = timeline.tracks[track_index].audio_role == AudioRole::Mic;
        timeline.tracks[track_index]
            .clips
            .push(default_clip_instance(
                clip_id,
                asset_id,
                start_secs,
                0.0,
                duration_secs,
                voice_cleanup_enabled,
            ));
        self.active_project_mut().record_recent_asset(asset_id);
        self.fit_timeline_to_initial_content(was_empty);
    }

    /// Appends `asset_id` onto a brand-new, never-reused track — what a multi-file drop/drag
    /// onto the Editor uses ([`App::handle_dropped_files`]) instead of
    /// [`App::add_asset_to_timeline`] for each file in the batch, so simultaneous source files
    /// land on parallel tracks (each new track is empty, so its clip starts at `0.0`) rather
    /// than being silently concatenated one after another onto a single shared track. A no-op
    /// if `asset_id` isn't in the active project's media library.
    pub fn add_asset_to_new_track(&mut self, asset_id: u64) {
        let Some((kind, duration_secs)) = self.asset_kind_and_duration(asset_id) else {
            return;
        };
        self.push_undo_snapshot();
        let was_empty = self
            .active_project()
            .timeline()
            .tracks
            .iter()
            .all(|track| track.clips.is_empty());
        let timeline = self.active_project_mut().timeline_mut();
        let track_index = create_new_track(timeline, kind);
        let start_secs = timeline.tracks[track_index].duration_secs();
        let clip_id = next_clip_id(timeline);
        let voice_cleanup_enabled = timeline.tracks[track_index].audio_role == AudioRole::Mic;
        timeline.tracks[track_index]
            .clips
            .push(default_clip_instance(
                clip_id,
                asset_id,
                start_secs,
                0.0,
                duration_secs,
                voice_cleanup_enabled,
            ));
        self.active_project_mut().record_recent_asset(asset_id);
        self.fit_timeline_to_initial_content(was_empty);
    }

    /// Inserts `asset_id` onto the timeline at `start_secs` — what dropping an asset dragged
    /// out of the media library onto the timeline strip does. Prefers `preferred_track_id` if
    /// it exists and matches the asset's kind (the track row the drop landed on); otherwise
    /// falls back to the same track-resolution as [`App::add_asset_to_timeline`] (first
    /// existing track of matching kind, auto-created if none exists). A no-op if `asset_id`
    /// isn't in the active project's media library or `start_secs` is negative.
    pub fn add_asset_to_timeline_at(
        &mut self,
        asset_id: u64,
        preferred_track_id: Option<u64>,
        start_secs: f64,
    ) {
        if start_secs < 0.0 {
            return;
        }
        let Some((kind, duration_secs)) = self.asset_kind_and_duration(asset_id) else {
            return;
        };
        self.push_undo_snapshot();
        let was_empty = self
            .active_project()
            .timeline()
            .tracks
            .iter()
            .all(|track| track.clips.is_empty());
        let timeline = self.active_project_mut().timeline_mut();
        let track_index = resolve_or_create_track(timeline, kind, preferred_track_id);
        let clip_id = next_clip_id(timeline);
        let voice_cleanup_enabled = timeline.tracks[track_index].audio_role == AudioRole::Mic;
        timeline.tracks[track_index]
            .clips
            .push(default_clip_instance(
                clip_id,
                asset_id,
                start_secs,
                0.0,
                duration_secs,
                voice_cleanup_enabled,
            ));
        self.active_project_mut().record_recent_asset(asset_id);
        self.fit_timeline_to_initial_content(was_empty);
    }

    /// Gives the first timeline clip enough horizontal room to be edited immediately.
    ///
    /// A fixed four-pixels-per-second initial zoom makes a typical five-second imported clip
    /// only twenty pixels wide. This runs exactly once, when the first clip is placed, and
    /// therefore never overrides a deliberate user zoom adjustment. Long timelines retain the
    /// former overview scale while short footage is capped at the normal maximum zoom.
    fn fit_timeline_to_initial_content(&mut self, was_empty: bool) {
        if !was_empty {
            return;
        }
        let duration_secs = self.active_project().timeline().duration_secs() as f32;
        if duration_secs <= 0.0 {
            return;
        }
        const EDITABLE_CLIP_WIDTH_PX: f32 = 480.0;
        const OVERVIEW_PX_PER_SEC: f32 = 4.0;
        const MAX_PX_PER_SEC: f32 = 60.0;
        self.timeline_px_per_sec =
            (EDITABLE_CLIP_WIDTH_PX / duration_secs).clamp(OVERVIEW_PX_PER_SEC, MAX_PX_PER_SEC);
    }

    /// Detaches this block's embedded audio onto a synced clip on its own Audio track — the
    /// mechanical precondition for J-cuts/L-cuts (audio and video changing at different points),
    /// per `spec/ROADMAP.md` P4 item 28. Mutes the video clip's own audio ([`ClipInstance::
    /// gain_db`] set to [`GAIN_DB_RANGE`]'s floor — this codebase has no separate "muted" flag,
    /// so muting reuses the existing gain primitive, the same reuse the roadmap item's own
    /// scoping note calls for) and places a new clip on an Audio track pointing at the same
    /// asset, with the same trim range and timeline placement, at unity gain. Both clips are
    /// then independently trimmable — no render/preview pipeline change needed, since per-track
    /// independent clips already mix correctly ([`avcore::render::resolve_audio_segments`]). A
    /// no-op if nothing is selected, the selected clip isn't on a Video track, or its asset has
    /// no audio.
    pub fn detach_audio_from_selected_clip(&mut self) {
        let Some(clip_id) = self.selected_clip_id else {
            return;
        };
        if self.selected_clip_track_kind() != Some(TrackKind::Video) {
            return;
        }
        let Some(clip) = self.selected_clip() else {
            return;
        };
        let (asset_id, start_secs, source_in_secs, source_out_secs) = (
            clip.asset_id,
            clip.start_secs,
            clip.source_in_secs,
            clip.source_out_secs,
        );
        let has_audio = self
            .active_project()
            .media_library
            .iter()
            .any(|a| a.id == asset_id && a.has_audio);
        if !has_audio {
            return;
        }

        self.push_undo_snapshot();
        let timeline = self.active_project_mut().timeline_mut();
        if let Some(video_clip) = timeline.clip_mut(clip_id) {
            video_clip.gain_db = *GAIN_DB_RANGE.start();
        }
        let track_index = resolve_or_create_track(timeline, TrackKind::Audio, None);
        let new_clip_id = next_clip_id(timeline);
        let voice_cleanup_enabled = timeline.tracks[track_index].audio_role == AudioRole::Mic;
        timeline.tracks[track_index]
            .clips
            .push(default_clip_instance(
                new_clip_id,
                asset_id,
                start_secs,
                source_in_secs,
                source_out_secs,
                voice_cleanup_enabled,
            ));
    }

    /// Approximates a speed ramp on the selected clip as `steps` discrete segments, each a
    /// constant [`avcore::timeline::ClipInstance::speed_factor`] linearly interpolated between
    /// `start_speed` and `end_speed` — a stepped "staircase" ramp rather than a smooth curve,
    /// per `spec/ROADMAP.md` P4 item 29. A true continuous speed curve needs the export-side
    /// `setpts` filter's output PTS to be the *integral* of `1/speed` over time, which for a
    /// piecewise-linear speed curve has no simple closed form (needs a `log()` term per
    /// segment) — a real, easy-to-get-subtly-wrong derivation with no way to render/verify it
    /// in this sandbox (no decode capability). This instead reuses two already-correct,
    /// already-tested primitives unchanged: [`avcore::timeline::Track::split_clip_at`] (to
    /// carve the clip into `steps` equal-timeline-duration pieces at its current, unramped
    /// speed) and the existing `speed_factor` field (set per piece afterward). Since
    /// [`avcore::timeline::ClipInstance::duration_secs`] depends on `speed_factor`, each
    /// piece's new duration shifts where the next one needs to start to stay contiguous — this
    /// reflows every piece's `start_secs` left to right after the speed changes, the same "no
    /// auto-ripple, caller repositions" contract this codebase's other editing operations
    /// already have (see [`App::delete_selected_clip`]). A no-op if nothing is selected,
    /// `steps` is less than 2, or the clip has zero duration.
    pub fn apply_speed_ramp_to_selected_clip(
        &mut self,
        start_speed: f32,
        end_speed: f32,
        steps: usize,
    ) {
        if steps < 2 {
            return;
        }
        let Some(clip_id) = self.selected_clip_id else {
            return;
        };
        let Some(clip) = self.selected_clip() else {
            return;
        };
        let (start_secs, original_duration_secs) = (clip.start_secs, clip.duration_secs());
        if original_duration_secs <= 0.0 {
            return;
        }

        self.push_undo_snapshot();
        let timeline = self.active_project_mut().timeline_mut();
        let mut next_id = timeline
            .tracks
            .iter()
            .flat_map(|t| &t.clips)
            .map(|c| c.id)
            .max()
            .unwrap_or(0)
            + 1;

        // Split into `steps` equal-duration pieces (at the clip's still-unramped speed) by
        // cutting at each internal boundary left to right, so an earlier split never shifts a
        // later boundary's position.
        let mut clip_ids = vec![clip_id];
        for i in 1..steps {
            let boundary = start_secs + original_duration_secs * (i as f64 / steps as f64);
            for track in &mut timeline.tracks {
                if track.split_clip_at(boundary, next_id) {
                    clip_ids.push(next_id);
                    next_id += 1;
                    break;
                }
            }
        }

        // Assign each piece's ramped speed, then reflow start_secs left to right so the pieces
        // stay contiguous despite each one's own duration now changing. Uses clip_ids.len()
        // (not `steps`) as the interpolation denominator, in case a split above didn't take
        // (e.g. a boundary landing exactly on an existing edge) and fewer pieces resulted.
        let ramped_steps = clip_ids.len();
        let mut cursor_secs = start_secs;
        for (i, &id) in clip_ids.iter().enumerate() {
            let t = if ramped_steps > 1 {
                i as f32 / (ramped_steps - 1) as f32
            } else {
                0.0
            };
            let speed = (start_speed + (end_speed - start_speed) * t)
                .clamp(*SPEED_FACTOR_RANGE.start(), *SPEED_FACTOR_RANGE.end());
            if let Some(c) = timeline.clip_mut(id) {
                c.speed_factor = speed;
                // A prior smooth-ramp application (App::apply_smooth_speed_ramp_to_selected_clip)
                // may have left this set — clear it so duration_secs() uses the plain
                // speed_factor this stepped mode actually sets, not a stale ramp endpoint.
                c.speed_ramp_end_factor = None;
                c.start_secs = cursor_secs;
                cursor_secs += c.duration_secs();
            }
        }
    }

    /// Wraps the selected clip in a new nested sequence ("compound clip" — Premiere/DaVinci/FCP
    /// all have this). oca's existing per-tab `Sequence` already *is* an independently-editable
    /// sub-timeline, so a compound clip just points a `ClipInstance` at another `Sequence`
    /// instead of an asset (`ClipInstance::nested_sequence_id`) rather than needing a second
    /// sub-timeline concept — see `avcore::nested_sequence`'s own module doc comment for the
    /// full design and its "renders synchronously, blocks briefly" known cost.
    ///
    /// Moves the selected clip's own data (trim range, every effect/keyframe) into a fresh
    /// `Sequence`'s own new V1 track, rebased to start at `0.0`, then replaces it in place on
    /// the original track with a plain nested-sequence clip spanning the same
    /// `start_secs`/duration. Only `Video`-track clips. A no-op if nothing is selected or the
    /// selection isn't a `Video`-track clip.
    ///
    /// **Multi-clip compounding**: if the current ctrl-click multi-selection
    /// ([`App::multi_selected_clip_ids`]) has more than one member, includes the selected clip,
    /// and every member sits on that same track with no `nested_sequence_id` of its own already
    /// — the unambiguous case, one nested track, no z-order/flattening decision to make — the
    /// whole selection is compounded together instead of just the one clip: each member is
    /// rebased relative to the *earliest* member's own `start_secs` (not `0.0` individually) so
    /// their relative spacing/gaps survive inside the nested sequence, and the single resulting
    /// wrapper clip spans from the earliest member's start to the latest member's own end.
    /// Falls back to the single-clip behavior above (silently, not an error) when the
    /// multi-selection spans more than one track or already contains a compound clip — this
    /// method never guesses across tracks. A composite-group (`ClipInstance::composite_id`)
    /// multi-clip compound isn't supported yet either way — real groups routinely pair a video
    /// clip with an audio one on different tracks, which needs the same multi-track nested
    /// handling this pass deliberately doesn't attempt; a real scope cut, not an oversight.
    pub fn create_compound_clip_from_selected_clip(&mut self) {
        let Some(clip_id) = self.selected_clip_id else {
            return;
        };
        if self.selected_clip_track_kind() != Some(TrackKind::Video) {
            return;
        }

        let timeline = self.active_project().timeline();
        let Some(track) = timeline
            .tracks
            .iter()
            .find(|t| t.clips.iter().any(|c| c.id == clip_id))
        else {
            return;
        };
        let group_ids: Vec<u64> = if self.multi_selected_clip_ids.len() > 1
            && self.multi_selected_clip_ids.contains(&clip_id)
            && self.multi_selected_clip_ids.iter().all(|id| {
                track
                    .clips
                    .iter()
                    .any(|c| c.id == *id && c.nested_sequence_id.is_none())
            }) {
            self.multi_selected_clip_ids.iter().copied().collect()
        } else {
            vec![clip_id]
        };
        let mut members: Vec<ClipInstance> = track
            .clips
            .iter()
            .filter(|c| group_ids.contains(&c.id))
            .cloned()
            .collect();
        if members.is_empty() {
            return;
        }
        members.sort_by(|a, b| a.start_secs.total_cmp(&b.start_secs));
        let primary = members
            .iter()
            .find(|c| c.id == clip_id)
            .cloned()
            .unwrap_or_else(|| members[0].clone());
        let min_start = members[0].start_secs;
        let max_end = members
            .iter()
            .map(|c| c.start_secs + c.duration_secs())
            .fold(f64::NEG_INFINITY, f64::max);
        let duration_secs = max_end - min_start;

        self.push_undo_snapshot();
        let project = self.active_project_mut();
        let new_sequence_id = project.sequences.iter().map(|s| s.id).max().unwrap_or(0) + 1;
        let compound_number = project.sequences.len() + 1;
        let export_settings = project.active_sequence().export_settings;

        let mut nested_timeline = avcore::timeline::Timeline {
            tracks: Vec::new(),
            playhead_secs: 0.0,
            markers: Vec::new(),
            multicam_groups: Vec::new(),
        };
        let track_index = create_new_track(&mut nested_timeline, TrackKind::Video);
        for member in &members {
            let mut inner_clip = member.clone();
            inner_clip.start_secs = member.start_secs - min_start;
            nested_timeline.tracks[track_index].clips.push(inner_clip);
        }

        project.sequences.push(avcore::project::Sequence {
            id: new_sequence_id,
            name: format!("Compound {compound_number}"),
            timeline: nested_timeline,
            export_settings,
        });

        let timeline = project.timeline_mut();
        for track in &mut timeline.tracks {
            if track.clips.iter().any(|c| group_ids.contains(&c.id)) {
                track.clips.retain(|c| !group_ids.contains(&c.id));
                let mut wrapper =
                    default_clip_instance(clip_id, 0, min_start, 0.0, duration_secs, false);
                wrapper.nested_sequence_id = Some(new_sequence_id);
                wrapper.composite_id = if group_ids.len() == 1 {
                    primary.composite_id
                } else {
                    None
                };
                wrapper.color_label = primary.color_label;
                track.clips.push(wrapper);
                break;
            }
        }
    }

    /// Switches the Editor's active tab to `sequence_id` — what double-clicking a compound clip
    /// (nested-sequence `ClipInstance`) on the timeline does, the common "enter" affordance
    /// every NLE with this feature has. A no-op if `sequence_id` doesn't exist.
    pub fn open_nested_sequence(&mut self, sequence_id: u64) {
        let project = self.active_project_mut();
        if let Some(index) = project.sequences.iter().position(|s| s.id == sequence_id) {
            project.active_sequence = index;
        }
    }

    /// Inserts an existing `Sequence` (elsewhere in the same project) as a compound clip onto
    /// the active sequence's timeline — the "drag an existing sequence tab onto another
    /// timeline as a nested clip" direction [`App::create_compound_clip_from_selected_clip`]'s
    /// own doc comment flags as not shipped (only "wrap the current selection in a brand-new
    /// nested sequence" does). Lands on the first `Video` track (auto-creating one if none
    /// exists), right after whatever's already there — the same "append" placement
    /// [`App::add_asset_to_timeline`] already uses for a double-clicked media-library asset,
    /// reused here since inserting a compound clip this way has no drag position of its own to
    /// place it at.
    ///
    /// The new clip's duration comes from `avcore::timeline::Timeline::duration_secs` — the
    /// nested sequence's own edited length — not a real FFmpeg render/probe (that only happens
    /// lazily, in the background, the first time this clip is actually previewed or exported;
    /// see `App::materialize_nested_sequences_for_active_sequence`), matching how
    /// [`App::create_compound_clip_from_selected_clip`] already treats a compound clip's box
    /// length as a cheap timeline-derived value rather than something that requires rendering
    /// up front.
    ///
    /// A no-op if `sequence_id` doesn't exist in the active project, is the active sequence
    /// itself (the trivial direct self-nesting cycle — a deeper indirect cycle, e.g. inserting
    /// sequence A into B while B is already nested inside A, is instead caught gracefully at
    /// render/materialize time by `avcore::nested_sequence`'s own cycle detection, the same path
    /// every other nested clip already goes through), or is itself empty
    /// (`Timeline::duration_secs() == 0.0` — nothing meaningful to place a clip spanning).
    pub fn insert_sequence_as_compound_clip(&mut self, sequence_id: u64) {
        let duration_secs = {
            let project = self.active_project();
            if project.active_sequence().id == sequence_id {
                return;
            }
            match project.sequences.iter().find(|s| s.id == sequence_id) {
                Some(nested) => nested.timeline.duration_secs(),
                None => return,
            }
        };
        if duration_secs <= 0.0 {
            return;
        }

        self.push_undo_snapshot();
        let timeline = self.active_project_mut().timeline_mut();
        let track_index = resolve_or_create_track(timeline, TrackKind::Video, None);
        let start_secs = timeline.tracks[track_index].duration_secs();
        let clip_id = next_clip_id(timeline);
        let mut clip = default_clip_instance(clip_id, 0, start_secs, 0.0, duration_secs, false);
        clip.nested_sequence_id = Some(sequence_id);
        timeline.tracks[track_index].clips.push(clip);
    }

    /// Applies a smooth, continuous speed ramp to the selected clip — the P4 item 29 follow-up
    /// [`App::apply_speed_ramp_to_selected_clip`]'s own doc comment flagged as needing a
    /// `log()`-based `setpts` derivation this codebase's sandboxed development environment
    /// couldn't render/verify at the time. Unlike the stepped approximation, this needs no
    /// splitting at all: [`avcore::timeline::ClipInstance::speed_ramp_end_factor`] carries both
    /// endpoints directly, and export ([`avcore::render::resolve_timeline_segments`]/`_multi`)
    /// resolves the whole ramp to one continuous `setpts` expression
    /// (`avbridge::ClipSegment::smooth_speed_ramp_end_factor`). A no-op if nothing is selected.
    pub fn apply_smooth_speed_ramp_to_selected_clip(&mut self, start_speed: f32, end_speed: f32) {
        let Some(clip_id) = self.selected_clip_id else {
            return;
        };
        self.push_undo_snapshot();
        let timeline = self.active_project_mut().timeline_mut();
        if let Some(clip) = timeline.clip_mut(clip_id) {
            clip.speed_factor =
                start_speed.clamp(*SPEED_FACTOR_RANGE.start(), *SPEED_FACTOR_RANGE.end());
            clip.speed_ramp_end_factor =
                Some(end_speed.clamp(*SPEED_FACTOR_RANGE.start(), *SPEED_FACTOR_RANGE.end()));
        }
    }

    /// Looks up `asset_id` in the active project's media library and returns its track kind
    /// and duration, or `None` if it isn't there.
    pub(super) fn asset_kind_and_duration(&self, asset_id: u64) -> Option<(TrackKind, f64)> {
        let asset = self
            .active_project()
            .media_library
            .iter()
            .find(|a| a.id == asset_id)?;
        let kind = match asset.kind {
            avcore::MediaKind::Video => TrackKind::Video,
            avcore::MediaKind::Audio => TrackKind::Audio,
        };
        Some((kind, asset.duration_secs))
    }

    /// Splits whichever clip covers the timeline playhead, on every track that has one there,
    /// into two — what `Ctrl+B` and the toolbar's "Cortar / Split" button do. Cutting every
    /// track at once (rather than just a clicked clip) keeps V1/A1/A2 in sync, which is the
    /// point of a gameplay edit. A no-op on any track where nothing covers the playhead.
    pub fn split_at_playhead(&mut self) {
        self.push_undo_snapshot();
        let at_secs = self.active_project().timeline().playhead_secs;
        let mut next_id = self
            .active_project()
            .timeline()
            .tracks
            .iter()
            .flat_map(|t| &t.clips)
            .map(|c| c.id)
            .max()
            .unwrap_or(0)
            + 1;

        for track in &mut self.active_project_mut().timeline_mut().tracks {
            if track.split_clip_at(at_secs, next_id) {
                next_id += 1;
            }
        }
    }

    /// Splits whichever clip covers `at_secs` on `track_id` alone — the Razor Tool's own
    /// click-to-split (`CINECUT_UI_UX_SPEC_v1.0.md`'s product-decisions addendum, Section 4),
    /// distinct from `split_at_playhead`'s "every track at the playhead" scope: a Razor click
    /// only ever touches the one clip actually clicked. A no-op if `track_id` doesn't exist or
    /// nothing on it covers `at_secs`.
    pub fn split_track_clip_at(&mut self, track_id: u64, at_secs: f64) {
        self.push_undo_snapshot();
        let next_id = self
            .active_project()
            .timeline()
            .tracks
            .iter()
            .flat_map(|t| &t.clips)
            .map(|c| c.id)
            .max()
            .unwrap_or(0)
            + 1;
        let timeline = self.active_project_mut().timeline_mut();
        if let Some(track) = timeline.tracks.iter_mut().find(|t| t.id == track_id) {
            track.split_clip_at(at_secs, next_id);
        }
    }

    /// Removes `selected_clip_id` from whichever track has it and clears the selection — what
    /// pressing `Delete` on the timeline does. Leaves a gap rather than rippling later clips
    /// left, matching `split_at_playhead`'s equally simple non-ripple editing model. If the
    /// selected clip is a composite block member (`composite_id.is_some()`), every clip
    /// sharing that id is removed too — deleting one member deletes the whole block, per
    /// `request.md`'s Fase 3 "reutilizado ... como se fosse um clipe só" spec. A no-op if
    /// nothing is selected.
    pub fn delete_selected_clip(&mut self) {
        let Some(clip_id) = self.selected_clip_id else {
            return;
        };
        self.push_undo_snapshot();
        let timeline = self.active_project_mut().timeline_mut();
        let composite_id = timeline
            .tracks
            .iter()
            .flat_map(|t| &t.clips)
            .find(|c| c.id == clip_id)
            .and_then(|c| c.composite_id);
        for track in &mut timeline.tracks {
            match composite_id {
                Some(group) => track.clips.retain(|c| c.composite_id != Some(group)),
                None => track.clips.retain(|c| c.id != clip_id),
            }
        }
        self.selected_clip_id = None;
    }

    /// Sets `clip_id`'s color label ([`avcore::timeline::ClipInstance::color_label`]) — what
    /// picking a swatch (or "Limpar") in the timeline clip's context menu does. Takes an
    /// explicit `clip_id` rather than acting on `selected_clip_id` since the context menu can
    /// set a label on a clip that isn't the current selection. A no-op if `clip_id` doesn't
    /// exist.
    pub fn set_clip_color_label(&mut self, clip_id: u64, color_label: Option<[u8; 3]>) {
        self.push_undo_snapshot_for_drag();
        let timeline = self.active_project_mut().timeline_mut();
        if let Some(clip) = timeline.clip_mut(clip_id) {
            clip.color_label = color_label;
        }
    }

    /// Calls `f` with a mutable borrow of the selected clip, if any — the shared dispatch path
    /// for every `set_selected_clip_*` setter. Pushes an undo snapshot first via
    /// [`App::push_undo_snapshot_for_drag`], so every effect-property setter (gain, crop,
    /// color adjustments, keyframes, ...) gets undo coverage for free without each of their
    /// ~20 call sites in `properties_panel.rs` needing its own drag-started check.
    pub(super) fn with_selected_clip_mut(
        &mut self,
        f: impl FnOnce(&mut avcore::timeline::ClipInstance),
    ) {
        let Some(clip_id) = self.selected_clip_id else {
            return;
        };
        self.push_undo_snapshot_for_drag();
        let (balance, net_sigma, chroma_key, crop, pixelize_intensity, shake_intensity, mask) = {
            let Some(clip) = self.active_project_mut().timeline_mut().clip_mut(clip_id) else {
                return;
            };
            f(clip);
            let effective_saturation =
                if clip.color_filter == avcore::timeline::ColorFilter::BlackAndWhite {
                    0.0
                } else {
                    clip.saturation
                };
            // Same *4.0 scale build_video_filter_bin itself uses -- see its own doc comment.
            let net_sigma = clip.blur_intensity as f64 * 4.0 - clip.sharpen as f64 * 4.0;
            (
                (clip.brightness, clip.contrast, effective_saturation),
                net_sigma,
                (clip.chroma_key_color, clip.chroma_key_tolerance),
                (clip.crop_x, clip.crop_y, clip.crop_w, clip.crop_h),
                clip.pixelize_intensity,
                clip.shake_intensity,
                (clip.mask_shape, clip.mask_corner_radius),
            )
        };
        // Cheap and harmless even for a setter that didn't touch color balance/blur/chroma
        // key/crop/pixelize/shake/mask at all -- a no-op push of the clip's own unchanged
        // values. See App::push_live_balance_update's doc comment for why this lives here
        // rather than in each of the ~20 individual set_selected_clip_* setters.
        self.push_live_balance_update(clip_id, balance.0, balance.1, balance.2);
        self.push_live_blur_update(clip_id, net_sigma);
        self.push_live_chroma_key_update(clip_id, chroma_key.0, chroma_key.1);
        self.push_live_crop_update(clip_id, crop.0, crop.1, crop.2, crop.3);
        self.push_live_pixelize_update(clip_id, pixelize_intensity);
        self.push_live_shake_update(clip_id, shake_intensity);
        self.push_live_mask_update(clip_id, mask.0, mask.1);
    }
}

/// Appends a brand-new, always-fresh empty track of `kind` to `timeline` — unlike
/// [`resolve_or_create_track`], this never reuses an existing track, since callers that need
/// this (layer templates applying several layers of the same kind, e.g. two video layers) need
/// each layer on its own track regardless of what's already there. Name is `V<n>`/`A<n>`/`T<n>`
/// where `n` is one past the count of existing tracks of that kind — so a timeline with one
/// video track "V1" gets a second track "V2". Returns the new track's index.
pub(super) fn create_new_track(
    timeline: &mut avcore::timeline::Timeline,
    kind: TrackKind,
) -> usize {
    timeline.create_track(kind)
}

/// Builds a fresh, entirely-default `ClipInstance` at `start_secs`, trimmed to
/// `source_in_secs..source_out_secs` of `asset_id` — the shared literal [`App::
/// add_asset_to_timeline`], [`App::add_asset_to_timeline_at`], and [`App::
/// detach_audio_from_selected_clip`] all build a new clip from, differing only in which asset,
/// trim range, and placement they start it at. `voice_cleanup_enabled` is the one field that
/// isn't a flat default — see [`Self::add_asset_to_timeline`]'s own doc comment for why it's the
/// caller's job to pass `track.audio_role == AudioRole::Mic` here (CF-03's own "Mic by default,
/// explicit override elsewhere" acceptance criterion, applied only at clip *creation* time, never
/// retroactively when a track's role is reassigned later).
fn default_clip_instance(
    id: u64,
    asset_id: u64,
    start_secs: f64,
    source_in_secs: f64,
    source_out_secs: f64,
    voice_cleanup_enabled: bool,
) -> ClipInstance {
    ClipInstance {
        voice_cleanup_enabled,
        ..ClipInstance::new(id, asset_id, start_secs, source_in_secs, source_out_secs)
    }
}

/// Finds the track to place a new clip of `kind` on, for [`App::add_asset_to_timeline`] and
/// [`App::add_asset_to_timeline_at`]: `preferred_track_id` if it exists and matches `kind`,
/// else the first existing track of that kind, else a newly created `"V1"`/`"A1"` track
/// appended to `timeline.tracks`. Returns the resolved track's index.
pub(super) fn resolve_or_create_track(
    timeline: &mut avcore::timeline::Timeline,
    kind: TrackKind,
    preferred_track_id: Option<u64>,
) -> usize {
    timeline.resolve_or_create_track(kind, preferred_track_id)
}

/// The next free clip id across every track in `timeline`, including text and shape clips — one
/// past the current max, `1` if the timeline has no clips yet. Covers [`ClipInstance`]s,
/// [`TextClip`]s, and [`ShapeClip`]s so their ids are globally unique within a timeline.
pub(super) fn next_clip_id(timeline: &avcore::timeline::Timeline) -> u64 {
    timeline.next_clip_id()
}

#[cfg(test)]
#[path = "timeline_ops/timeline_ops_test.rs"]
mod tests;
