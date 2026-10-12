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

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::export::ExportAspectRatio;
use crate::frame_rate::FrameRate;
use crate::media::MediaAsset;
use crate::smart_bins::SmartBin;
use crate::timeline::{ClipInstance, Timeline, TrackKind};

/// How long ago a project was last edited. Locale-neutral by design — the UI layer is
/// responsible for turning this into a translated label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Recency {
    HoursAgo(u32),
    Yesterday,
    DaysAgo(u32),
}

/// One editable cut within a project — its own timeline and export defaults, independently
/// zoomable/playable and shown as its own tab in the Editor (per `request.md`'s Fase 3 "abas de
/// projeto" spec, e.g. one sequence for trimmed highlights, another for the full unedited
/// recording). Every project has at least one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sequence {
    pub id: u64,
    pub name: String,
    pub timeline: Timeline,
    /// Export defaults owned by this tab. A queued [`crate::ExportJob`] still snapshots the
    /// resolved values, so later edits here never change a job already in flight.
    #[serde(default)]
    pub export_settings: SequenceExportSettings,
}

impl Sequence {
    /// A copy of this sequence holding only the clip `clip_id`, moved to the start of the
    /// timeline: what exporting that one clip as its own video renders. Every other clip, text
    /// and shape is left out, and tracks left empty are dropped. `None` when no such clip exists.
    pub fn isolating_clip(&self, clip_id: u64) -> Option<Sequence> {
        let start = self
            .timeline
            .tracks
            .iter()
            .flat_map(|track| track.clips.iter())
            .find(|clip| clip.id == clip_id)?
            .start_secs;
        let mut sequence = self.clone();
        sequence.timeline.markers.clear();
        sequence.timeline.playhead_secs = 0.0;
        for track in &mut sequence.timeline.tracks {
            track.clips.retain(|clip| clip.id == clip_id);
            track.text_clips.clear();
            track.shape_clips.clear();
            for clip in &mut track.clips {
                clip.start_secs -= start;
                clip.transition_in = crate::timeline::TransitionType::None;
            }
        }
        sequence
            .timeline
            .tracks
            .retain(|track| !track.clips.is_empty());
        Some(sequence)
    }
}

/// Persisted export choices for one [`Sequence`]. Output path and GPU preference remain
/// app/job-level concerns; aspect ratio and normalization target describe the sequence's
/// intended presentation and therefore follow the tab across saves and project switches.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SequenceExportSettings {
    #[serde(default)]
    pub aspect_ratio: ExportAspectRatio,
    #[serde(default = "default_target_lufs")]
    pub target_lufs: f32,
}

impl Default for SequenceExportSettings {
    fn default() -> Self {
        Self {
            aspect_ratio: ExportAspectRatio::Original,
            target_lufs: default_target_lufs(),
        }
    }
}

fn default_target_lufs() -> f32 {
    -14.0
}

/// A single edit project: its imported media, its sequences, and display metadata for the
/// Início screen's project list. This is the root of everything a user works on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: u64,
    pub name: String,
    pub last_edited: Recency,
    /// Short user-facing description shown on the project card (e.g. "Cortes dos boss
    /// fights, um vídeo por chefe.") — free text, not a translatable UI label.
    pub summary: String,
    pub media_library: Vec<MediaAsset>,
    /// Every sequence (tab) in this project — always non-empty; [`Project::new_sequence`]
    /// and every construction site guarantee at least one.
    pub sequences: Vec<Sequence>,
    /// Index into `sequences` of the tab currently shown in the Editor.
    pub active_sequence: usize,
    /// Where this project was last saved to/loaded from, if anywhere. Not serialized — the
    /// file *contains* this data, it doesn't need to know its own path, and a copied/renamed
    /// project file shouldn't carry a stale path forward.
    #[serde(skip)]
    pub file_path: Option<PathBuf>,
    /// The Editor's panel widths/timeline height, when `ui`'s `LayoutScope::PerProject` is
    /// active — `request.md`'s Fase 3 "layout salvo por projeto ou por usuário" spec, the
    /// per-project half (per-user persistence already lives in `ui::PrefsState`). `None` for
    /// a project that's never been saved under the per-project scope (including every older
    /// saved project, via `#[serde(default)]`) — `ui::App` falls back to the per-user prefs
    /// values in that case rather than resetting to some arbitrary default.
    #[serde(default)]
    pub panel_layout: Option<PanelLayout>,
    /// Rule-based media-pool folders (P4 item 22, "Smart bins") — `#[serde(default)]` so a
    /// project saved before this field existed still loads (empty bin list).
    #[serde(default)]
    pub smart_bins: Vec<SmartBin>,
    /// Media-library asset ids most recently added to this project's timeline, most-recent
    /// first, deduplicated (re-adding an already-present id moves it back to the front rather
    /// than appearing twice) and capped at [`RECENT_ASSET_CAPACITY`] — the Media library
    /// panel's Recent filter's backing data. Deliberately tracks *timeline usage*, not import
    /// time or library-browsing: an asset just sitting in the library isn't "recent" until it's
    /// actually been edited with. `#[serde(default)]` so a project saved before this field
    /// existed loads with an empty recent list rather than failing to deserialize.
    #[serde(default)]
    pub recent_asset_ids: Vec<u64>,
    /// The timeline's frame rate. When set, edits made from the editor land on whole frame
    /// boundaries ([`Project::snap_secs`]); `None` leaves times as given. **Keep this the last
    /// field**: the project file is positional, and a version-3 file is upgraded by appending
    /// the encoding of `None` (see `persistence::upgrade_v3`).
    #[serde(default)]
    pub frame_rate: Option<FrameRate>,
}

/// Cap on [`Project::recent_asset_ids`] — same order of magnitude as `ui::PrefsState::
/// recent_project_paths`' own cap (10), a little larger since library assets are used more
/// often per session than whole projects are opened.
pub const RECENT_ASSET_CAPACITY: usize = 20;

/// See [`Project::panel_layout`]. A plain value type — `core` has no opinion on layout scope
/// itself (that's `ui::LayoutScope`), it just knows how to carry these three numbers along
/// with the rest of a saved project.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PanelLayout {
    pub lib_panel_width: f32,
    pub props_panel_width: f32,
    pub timeline_height: f32,
}

impl Project {
    /// `secs` moved to the nearest frame boundary of [`Project::frame_rate`], or unchanged when
    /// the project has none.
    pub fn snap_secs(&self, secs: f64) -> f64 {
        match self.frame_rate {
            Some(rate) if rate.is_valid() => rate.snap_secs(secs),
            _ => secs,
        }
    }

    /// The active tab's timeline — what the Editor screen and every clip-editing `ui::App`
    /// method actually read. Panics if `active_sequence` is out of bounds, which shouldn't
    /// happen given `sequences` is never empty and every mutation keeps the index in range —
    /// same trust-the-invariant style as `ui::App::active_project`'s own indexing.
    pub fn timeline(&self) -> &Timeline {
        &self.sequences[self.active_sequence].timeline
    }

    /// Mutable counterpart of [`Project::timeline`].
    pub fn timeline_mut(&mut self) -> &mut Timeline {
        &mut self.sequences[self.active_sequence].timeline
    }

    /// Whether any sequence has a clip that plays `asset_id`.
    pub fn asset_in_use(&self, asset_id: u64) -> bool {
        self.sequences.iter().any(|sequence| {
            sequence
                .timeline
                .tracks
                .iter()
                .any(|track| track.clips.iter().any(|clip| clip.asset_id == asset_id))
        })
    }

    /// Drops `asset_id` from the media library. Refused (returns `false`, library untouched)
    /// while a clip still uses it, so removing media can never leave a dangling clip.
    /// Unknown ids are a no-op that also returns `false`.
    pub fn remove_asset(&mut self, asset_id: u64) -> bool {
        if self.asset_in_use(asset_id) {
            return false;
        }
        let before = self.media_library.len();
        self.media_library.retain(|asset| asset.id != asset_id);
        self.recent_asset_ids.retain(|id| *id != asset_id);
        self.media_library.len() != before
    }

    /// The active tab itself — for whole-sequence operations (undo/redo snapshots) that
    /// `timeline()` doesn't cover, since [`Sequence`] also carries `export_settings`.
    pub fn active_sequence(&self) -> &Sequence {
        &self.sequences[self.active_sequence]
    }

    /// Mutable counterpart of [`Project::active_sequence`].
    pub fn active_sequence_mut(&mut self) -> &mut Sequence {
        &mut self.sequences[self.active_sequence]
    }

    /// Appends a new, empty sequence named `name` and switches `active_sequence` to it —
    /// what the Editor's tab bar "+" button does. Returns the new sequence's id.
    pub fn new_sequence(&mut self, name: String) -> u64 {
        let id = self.sequences.iter().map(|s| s.id).max().unwrap_or(0) + 1;
        let export_settings = self
            .sequences
            .get(self.active_sequence)
            .map(|sequence| sequence.export_settings)
            .unwrap_or_default();
        self.sequences.push(Sequence {
            id,
            name,
            timeline: Timeline {
                tracks: Vec::new(),
                playhead_secs: 0.0,
                markers: Vec::new(),
                multicam_groups: Vec::new(),
            },
            export_settings,
        });
        self.active_sequence = self.sequences.len() - 1;
        id
    }

    /// Clones `sequences[index]` into a new tab immediately after it, assigns a fresh sequence
    /// id, replaces the clone's name with `name`, and switches to it. Clip/track ids remain
    /// unchanged because they are scoped to their owning sequence. Returns the fresh id, or
    /// `None` when `index` is out of range.
    pub fn duplicate_sequence(&mut self, index: usize, name: String) -> Option<u64> {
        let mut duplicate = self.sequences.get(index)?.clone();
        let id = self.sequences.iter().map(|s| s.id).max().unwrap_or(0) + 1;
        duplicate.id = id;
        duplicate.name = name;
        let insert_at = index + 1;
        self.sequences.insert(insert_at, duplicate);
        self.active_sequence = insert_at;
        Some(id)
    }

    /// Removes one sequence while preserving the invariant that every project has at least
    /// one. If the active tab is removed, the tab now occupying that position becomes active,
    /// falling back to the previous tab when the removed one was last. Returns whether a tab
    /// was actually removed.
    pub fn remove_sequence(&mut self, index: usize) -> bool {
        if self.sequences.len() <= 1 || index >= self.sequences.len() {
            return false;
        }
        self.sequences.remove(index);
        self.active_sequence = match self.active_sequence.cmp(&index) {
            std::cmp::Ordering::Less => self.active_sequence,
            std::cmp::Ordering::Equal => index.min(self.sequences.len() - 1),
            std::cmp::Ordering::Greater => self.active_sequence - 1,
        };
        true
    }

    /// Every sequence in this project (`sequence_id` itself excluded — a sequence can't nest
    /// itself, see `nested_sequence::materialize_nested_sequences`'s own cycle guard) containing
    /// at least one clip whose `ClipInstance::nested_sequence_id` points at `sequence_id` —
    /// i.e. every place `sequence_id` is used as a compound clip elsewhere in the project.
    /// [`Project::remove_sequence`] itself doesn't check this: a dangling `nested_sequence_id`
    /// left behind by a delete degrades gracefully at render time
    /// (`RenderError::MissingNestedSequence`, clip skipped, not a crash) rather than erroring —
    /// but a caller that wants to warn the user *before* deleting can call this first.
    pub fn sequences_referencing_as_compound_clip(&self, sequence_id: u64) -> Vec<&Sequence> {
        self.sequences
            .iter()
            .filter(|s| s.id != sequence_id)
            .filter(|s| {
                s.timeline.tracks.iter().any(|t| {
                    t.clips
                        .iter()
                        .any(|c| c.nested_sequence_id == Some(sequence_id))
                })
            })
            .collect()
    }

    /// Wraps `clip_id` (and, per the multi-select rule below, its whole current selection) in a
    /// new nested sequence ("compound clip" — Premiere/DaVinci/FCP all have this) on the active
    /// sequence's timeline. Ported from `ui::App::create_compound_clip_from_selected_clip` so
    /// both shells share one implementation — see [`crate::nested_sequence`]'s own module doc
    /// comment for the underlying design and its "renders synchronously, blocks briefly" known
    /// cost at preview/export time.
    ///
    /// Moves the clip's own data (trim range, every effect/keyframe) into a fresh [`Sequence`]'s
    /// own new V1 track, rebased to start at `0.0`, then replaces it in place on the original
    /// track with a plain nested-sequence clip (same id as `clip_id`) spanning the same
    /// start/duration. Only `Video`-track clips.
    ///
    /// **Multi-clip compounding**: if `selected_clip_ids` has more than one member, includes
    /// `clip_id`, and every member sits on that same track with no `nested_sequence_id` of its
    /// own already — the unambiguous case, one nested track, no z-order/flattening decision to
    /// make — the whole selection is compounded together instead of just `clip_id`: each member
    /// is rebased relative to the *earliest* member's own `start_secs` (not `0.0` individually)
    /// so their relative spacing/gaps survive inside the nested sequence, and the single
    /// resulting wrapper clip spans from the earliest member's start to the latest member's own
    /// end. Falls back to the single-clip behavior above (silently, not an error) when the
    /// selection spans more than one track or already contains a compound clip.
    ///
    /// Returns `None` (no-op) if `clip_id` isn't on any `Video` track in the active sequence —
    /// else the new nested [`Sequence`]'s id.
    pub fn create_compound_clip(&mut self, clip_id: u64, selected_clip_ids: &[u64]) -> Option<u64> {
        let timeline = self.timeline();
        let track_index = timeline
            .tracks
            .iter()
            .position(|t| t.kind == TrackKind::Video && t.clips.iter().any(|c| c.id == clip_id))?;
        let track = &timeline.tracks[track_index];

        let group_ids: Vec<u64> = if selected_clip_ids.len() > 1
            && selected_clip_ids.contains(&clip_id)
            && selected_clip_ids.iter().all(|id| {
                track
                    .clips
                    .iter()
                    .any(|c| c.id == *id && c.nested_sequence_id.is_none())
            }) {
            selected_clip_ids.to_vec()
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
            return None;
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

        let new_sequence_id = self.sequences.iter().map(|s| s.id).max().unwrap_or(0) + 1;
        let compound_number = self.sequences.len() + 1;
        let export_settings = self.active_sequence().export_settings;

        let mut nested_timeline = Timeline {
            tracks: Vec::new(),
            playhead_secs: 0.0,
            markers: Vec::new(),
            multicam_groups: Vec::new(),
        };
        let nested_track_index = nested_timeline.create_track(TrackKind::Video);
        for member in &members {
            let mut inner_clip = member.clone();
            inner_clip.start_secs = member.start_secs - min_start;
            nested_timeline.tracks[nested_track_index]
                .clips
                .push(inner_clip);
        }

        self.sequences.push(Sequence {
            id: new_sequence_id,
            name: format!("Compound {compound_number}"),
            timeline: nested_timeline,
            export_settings,
        });

        let timeline = self.timeline_mut();
        for track in &mut timeline.tracks {
            if track.clips.iter().any(|c| group_ids.contains(&c.id)) {
                track.clips.retain(|c| !group_ids.contains(&c.id));
                let mut wrapper = ClipInstance::new(clip_id, 0, min_start, 0.0, duration_secs);
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
        Some(new_sequence_id)
    }

    /// Inserts an existing `Sequence` (elsewhere in this project) as a compound clip onto the
    /// active sequence's timeline — the "drag an existing sequence tab onto another timeline as
    /// a nested clip" direction, a secondary entry point distinct from
    /// [`Project::create_compound_clip`]'s "wrap the current selection in a brand-new nested
    /// sequence." Ported from `ui::App::insert_sequence_as_compound_clip`.
    ///
    /// Lands on the first `Video` track (auto-creating one if none exists), right after
    /// whatever's already there — the same "append" placement a double-clicked media-library
    /// asset already uses. The new clip's duration comes from the nested sequence's own edited
    /// [`Timeline::duration_secs`] — not a real FFmpeg render/probe (that only happens lazily,
    /// the first time this clip is actually previewed or exported, via
    /// [`crate::nested_sequence::materialize_nested_sequences`]).
    ///
    /// Returns `false` (no-op) if `sequence_id` doesn't exist in this project, is the active
    /// sequence itself (the trivial direct self-nesting cycle — a deeper indirect cycle is
    /// instead caught gracefully at render/materialize time by `nested_sequence`'s own cycle
    /// detection), or is itself empty (`Timeline::duration_secs() == 0.0`).
    pub fn insert_sequence_as_compound_clip(&mut self, sequence_id: u64) -> bool {
        if self.active_sequence().id == sequence_id {
            return false;
        }
        let duration_secs = match self.sequences.iter().find(|s| s.id == sequence_id) {
            Some(nested) => nested.timeline.duration_secs(),
            None => return false,
        };
        if duration_secs <= 0.0 {
            return false;
        }

        let timeline = self.timeline_mut();
        let track_index = timeline.resolve_or_create_track(TrackKind::Video, None);
        let start_secs = timeline.tracks[track_index].duration_secs();
        let clip_id = timeline.next_clip_id();
        let mut clip = ClipInstance::new(clip_id, 0, start_secs, 0.0, duration_secs);
        clip.nested_sequence_id = Some(sequence_id);
        timeline.tracks[track_index].clips.push(clip);
        true
    }

    /// Moves one sequence to `target_index`, keeping the currently active sequence active even
    /// when indices shift around it. Returns `false` for invalid indices or a no-op move.
    pub fn move_sequence(&mut self, from_index: usize, target_index: usize) -> bool {
        if from_index >= self.sequences.len()
            || target_index >= self.sequences.len()
            || from_index == target_index
        {
            return false;
        }
        let active_id = self.sequences[self.active_sequence].id;
        let sequence = self.sequences.remove(from_index);
        self.sequences.insert(target_index, sequence);
        self.active_sequence = self
            .sequences
            .iter()
            .position(|sequence| sequence.id == active_id)
            .expect("the active sequence is only reordered, never removed");
        true
    }

    /// Adds a new, unnamed-rule [`SmartBin`] (see [`SmartBin::new`]) and returns its freshly
    /// assigned id — "max + 1", the same convention every other entity id in this codebase uses.
    pub fn add_smart_bin(&mut self, name: String) -> u64 {
        let id = self.smart_bins.iter().map(|b| b.id).max().unwrap_or(0) + 1;
        self.smart_bins.push(SmartBin::new(id, name));
        id
    }

    /// Removes the smart bin with `bin_id`, if any. `true` if a bin was actually removed.
    pub fn remove_smart_bin(&mut self, bin_id: u64) -> bool {
        let before = self.smart_bins.len();
        self.smart_bins.retain(|b| b.id != bin_id);
        self.smart_bins.len() != before
    }

    /// Mutable access to the smart bin with `bin_id`, if it exists.
    pub fn smart_bin_mut(&mut self, bin_id: u64) -> Option<&mut SmartBin> {
        self.smart_bins.iter_mut().find(|b| b.id == bin_id)
    }

    /// Moves `asset_id` to the front of [`Self::recent_asset_ids`] (inserting it if not
    /// already present), then truncates to [`RECENT_ASSET_CAPACITY`] — what `ui`'s
    /// `App::add_asset_to_timeline`/`add_asset_to_timeline_at` call every time an asset lands
    /// on the timeline.
    pub fn record_recent_asset(&mut self, asset_id: u64) {
        self.recent_asset_ids.retain(|&id| id != asset_id);
        self.recent_asset_ids.insert(0, asset_id);
        self.recent_asset_ids.truncate(RECENT_ASSET_CAPACITY);
    }
}
