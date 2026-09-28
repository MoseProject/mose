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

//! Resolve timeline clips and overlay content for the live preview.

use avcore::timeline::{ShapeClip, TextClip};
use avcore::{ClipInstance, MediaAsset, TrackKind};

use super::{App, LiveUpdateKey};
use tracing::warn;

impl App {
    /// Just the id [`App::current_preview_clip`] would resolve to, without cloning the
    /// `ClipInstance`/`MediaAsset` — see [`App::ensure_preview_loaded`]'s doc comment for why
    /// this cheap pass exists.
    pub(super) fn current_preview_clip_id(&self) -> Option<u64> {
        let project = self.active_project();
        let timeline = project.timeline();
        let track = timeline
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Video && t.visible)?;
        let clip = track.clip_at(timeline.playhead_secs)?;
        // A compound clip (nested sequence) has no real `asset_id` to resolve — its own
        // `nested_sequence_id` being set is proof enough it "resolves" for this cheap
        // change-detection pass, same as an ordinary clip resolving via `media_library`. Getting
        // this wrong would make ensure_preview_loaded's fast path see "still None, nothing
        // changed" forever for a nested clip and never actually open its pipeline.
        if clip.nested_sequence_id.is_none() {
            project
                .media_library
                .iter()
                .find(|a| a.id == clip.asset_id)?;
        }
        Some(clip.id)
    }

    /// Just the ids [`App::current_preview_overlay_clips`] would resolve to, without cloning.
    pub(super) fn current_preview_overlay_clip_ids(&self) -> Vec<u64> {
        let project = self.active_project();
        let timeline = project.timeline();
        timeline
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Video && t.visible)
            .skip(1)
            .filter_map(|t| {
                let clip = t.clip_at(timeline.playhead_secs)?;
                // See current_preview_clip_id's own comment on why nested clips skip this check.
                if clip.nested_sequence_id.is_none() {
                    project
                        .media_library
                        .iter()
                        .find(|a| a.id == clip.asset_id)?;
                }
                Some(clip.id)
            })
            .collect()
    }

    /// Just the ids [`App::current_preview_audio_clips`] would resolve to, without cloning.
    pub(super) fn current_preview_audio_clip_ids(&self) -> Vec<u64> {
        let project = self.active_project();
        let timeline = project.timeline();
        timeline
            .tracks
            .iter()
            .filter(|track| track.kind == TrackKind::Audio && track.visible)
            .filter_map(|track| {
                let clip = track.clip_at(timeline.playhead_secs)?;
                project
                    .media_library
                    .iter()
                    .find(|asset| asset.id == clip.asset_id && asset.has_audio)?;
                Some(clip.id)
            })
            .collect()
    }

    /// Just the ids [`App::current_preview_text_clips`] would resolve to, without cloning.
    pub(super) fn current_preview_text_clip_ids(&self) -> Vec<u64> {
        let timeline = self.active_project().timeline();
        timeline
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Text)
            .filter_map(|t| {
                t.text_clips.iter().find(|c| {
                    timeline.playhead_secs >= c.start_secs
                        && timeline.playhead_secs < c.start_secs + c.duration_secs
                })
            })
            .map(|c| c.id)
            .collect()
    }

    /// Just the ids [`App::current_preview_shape_clips`] would resolve to, without cloning.
    pub(super) fn current_preview_shape_clip_ids(&self) -> Vec<u64> {
        let timeline = self.active_project().timeline();
        timeline
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Shape)
            .filter_map(|t| {
                t.shape_clips.iter().find(|c| {
                    timeline.playhead_secs >= c.start_secs
                        && timeline.playhead_secs < c.start_secs + c.duration_secs
                })
            })
            .map(|c| c.id)
            .collect()
    }

    /// The clip covering the active sequence's timeline playhead, and the asset it plays from,
    /// if both resolve — `None` if the video track is missing/empty, nothing covers the
    /// playhead ([`avcore::timeline::Track::clip_at`]), or the clip's `asset_id` isn't in
    /// `media_library`. Always the *first* video track (background/track 0) — see
    /// [`App::current_preview_overlay_clips`] for the rest. `media_library` is the caller's own
    /// (real assets plus, for a compound clip, [`App::materialize_nested_sequences_for_active_sequence`]'s
    /// synthetic ones) rather than reading `self.active_project().media_library` directly, so a
    /// nested-sequence clip resolves here exactly like an ordinary one.
    pub(super) fn current_preview_clip(
        &self,
        media_library: &[MediaAsset],
    ) -> Option<(ClipInstance, MediaAsset)> {
        let timeline = self.active_project().timeline();
        let track = timeline
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Video && t.visible)?;
        let clip = track.clip_at(timeline.playhead_secs)?;
        let asset = media_library.iter().find(|a| a.id == clip.asset_id)?;
        Some((clip.clone(), asset.clone()))
    }

    /// The fps of the asset behind the clip currently covering the playhead, for the preview
    /// panel's resolution+fps HUD chip (`spec/architecture/editor-ui-visual-redesign.md`'s
    /// Program monitor mapping — "fps ... available from the active sequence", corrected here:
    /// oca has no per-sequence fps, only per-asset, the same field the properties panel's own
    /// fps row already reads). `None` when nothing resolves (empty/hidden video track, nothing
    /// covers the playhead, missing asset, or the asset's own fps is unknown) — the chip omits
    /// the fps suffix in that case rather than guessing one.
    pub fn current_preview_fps(&self) -> Option<f32> {
        let (_, asset) = self.current_preview_clip(&self.active_project().media_library)?;
        asset.fps
    }

    /// Just the [`ClipInstance`] covering the active sequence's timeline playhead on the first
    /// visible `Video` track, without resolving the asset it plays from — for callers
    /// ([`App::toggle_preview_playback`], [`App::seek_preview`]) that only ever read clip-level
    /// fields (`frozen`, `speed_factor`, `id`), never the asset. This works for a compound clip
    /// (nested sequence) too, unlike [`App::current_preview_clip`] on its own, since it never
    /// needs a `media_library` lookup to succeed in the first place.
    pub(super) fn current_preview_video_clip(&self) -> Option<ClipInstance> {
        let timeline = self.active_project().timeline();
        let track = timeline
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Video && t.visible)?;
        track.clip_at(timeline.playhead_secs).cloned()
    }

    /// Just [`ClipInstance::lut_path`]/[`ClipInstance::vignette_intensity`]/
    /// [`ClipInstance::glitch_intensity`] for the clip at the playhead, without
    /// [`App::current_preview_clip`]'s full `ClipInstance`/`MediaAsset` clone (which includes
    /// every keyframe `Vec` on the clip) or its unused media-library lookup —
    /// [`App::pump_preview_frame`] calls this every frame during playback and only ever reads
    /// these three scalar fields, so paying for the rest was pure waste on the hottest UI-thread
    /// path in the app. Defaults (`String::new()`, `0.0`, `0.0`) when nothing covers the
    /// playhead, same as the `unwrap_or_default()` the caller used to apply to the full clone.
    pub(super) fn current_preview_cpu_effects(&self) -> avcore::PreviewCpuEffects {
        let timeline = self.active_project().timeline();
        let Some(track) = timeline
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Video && t.visible)
        else {
            return avcore::PreviewCpuEffects::default();
        };
        let Some(clip) = track.clip_at(timeline.playhead_secs) else {
            return avcore::PreviewCpuEffects::default();
        };
        avcore::PreviewCpuEffects::for_clip(Some(clip))
    }

    /// Every overlay-track (video track index 1+, in track order — matches
    /// [`avcore::render::resolve_timeline_segments_multi`]'s "track index = z-order"
    /// convention) clip covering the playhead, with the asset it plays from — a track with
    /// nothing at the playhead, or whose clip's asset doesn't resolve, is just skipped rather
    /// than aborting the whole list, same "degrade gracefully" shape
    /// [`App::current_preview_clip`] already has for the background track.
    pub(super) fn current_preview_overlay_clips(
        &self,
        media_library: &[MediaAsset],
    ) -> Vec<(ClipInstance, MediaAsset)> {
        let timeline = self.active_project().timeline();
        timeline
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Video && t.visible)
            .skip(1)
            .filter_map(|t| {
                let clip = t.clip_at(timeline.playhead_secs)?;
                let asset = media_library.iter().find(|a| a.id == clip.asset_id)?;
                Some((clip.clone(), asset.clone()))
            })
            .collect()
    }

    /// Every audio-only track clip covering the playhead, in track order. Video clips with
    /// embedded audio are already represented by the background/overlay branch lists.
    pub(super) fn current_preview_audio_clips(
        &self,
        media_library: &[MediaAsset],
    ) -> Vec<(ClipInstance, MediaAsset)> {
        let timeline = self.active_project().timeline();
        timeline
            .tracks
            .iter()
            .filter(|track| track.kind == TrackKind::Audio && track.visible)
            .filter_map(|track| {
                let clip = track.clip_at(timeline.playhead_secs)?;
                let asset = media_library
                    .iter()
                    .find(|asset| asset.id == clip.asset_id && asset.has_audio)?;
                Some((clip.clone(), asset.clone()))
            })
            .collect()
    }

    /// Every [`TextClip`] covering the playhead, across every `TrackKind::Text` track — same
    /// "collect what's here, skip what isn't" shape [`App::current_preview_overlay_clips`] has,
    /// but there's no asset to resolve (text has no source file) and no track-index-as-z-order
    /// convention (there's no `overlay`/`background` distinction among text tracks — every one
    /// composites the same way, on top of every video branch, matching export's own
    /// post-processing-pass ordering).
    fn preview_text_clips_at(&self, position_secs: f64) -> Vec<TextClip> {
        let timeline = self.active_project().timeline();
        timeline
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Text)
            .filter_map(|t| {
                t.text_clips.iter().find(|c| {
                    position_secs >= c.start_secs && position_secs < c.start_secs + c.duration_secs
                })
            })
            .cloned()
            .collect()
    }

    pub(super) fn current_preview_text_clips(&self) -> Vec<TextClip> {
        self.preview_text_clips_at(self.active_project().timeline().playhead_secs)
    }

    /// Pushes a replacement RGBA frame only when `position_secs` crosses into another timed
    /// word (or a gap between words). `Preview` owns the last active-word state and ignores
    /// same-word calls, so invoking this every UI frame during playback remains cheap.
    pub(super) fn refresh_preview_text_highlights(&mut self, position_secs: f64) {
        if self.preview_state.preview_text_clip_ids.is_empty() {
            return;
        }
        let clips = self.preview_text_clips_at(position_secs);
        let ids: Vec<u64> = clips.iter().map(|clip| clip.id).collect();
        if ids != self.preview_state.preview_text_clip_ids {
            return;
        }
        let timed_clips: Vec<(TextClip, f64)> = clips
            .iter()
            .map(|clip| (clip.clone(), position_secs - clip.start_secs))
            .collect();
        if let Some(worker) = &self.preview_state.worker {
            worker.live(LiveUpdateKey::global("text"), move |preview| {
                let refs: Vec<_> = timed_clips
                    .iter()
                    .map(|(clip, time)| (clip, *time))
                    .collect();
                if let Err(error) = preview.update_text_overlays(&refs) {
                    warn!(%error, "failed to refresh preview text overlays");
                }
            });
        }
    }

    /// Same role as [`App::current_preview_text_clips`], for [`ShapeClip`]s on
    /// `TrackKind::Shape` tracks.
    pub(super) fn current_preview_shape_clips(&self) -> Vec<ShapeClip> {
        let timeline = self.active_project().timeline();
        timeline
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Shape)
            .filter_map(|t| {
                t.shape_clips.iter().find(|c| {
                    timeline.playhead_secs >= c.start_secs
                        && timeline.playhead_secs < c.start_secs + c.duration_secs
                })
            })
            .cloned()
            .collect()
    }

    /// The resolved source path (proxy preferred) for a clip/asset pair, or `None` if it
    /// doesn't exist on disk — the same "don't spin up a pipeline for a file that's known to be
    /// missing" check [`App::ensure_preview_loaded`] already applied to the background clip.
    pub(super) fn preview_source_path(asset: &MediaAsset) -> Option<std::path::PathBuf> {
        let path = asset
            .proxy_path
            .clone()
            .unwrap_or_else(|| asset.source_path.clone());
        path.exists().then_some(path)
    }

    /// This clip's own timeline-to-source offset at `playhead_secs`, honoring `frozen` (always
    /// the held anchor frame) and `speed_factor` — the same math
    /// [`App::ensure_preview_loaded`]/[`App::seek_preview`] already apply to the background
    /// clip, pulled out so overlay branches can each compute their own independently (every
    /// branch has its own trim points and may have its own `frozen`/`speed_factor`).
    pub(super) fn clip_seek_offset(clip: &ClipInstance, playhead_secs: f64) -> f64 {
        if clip.frozen {
            clip.source_in_secs
        } else {
            let speed = clip.speed_factor.max(0.01) as f64;
            clip.source_in_secs + (playhead_secs - clip.start_secs) * speed
        }
    }

    /// Resolves source offsets and playback rates in the exact branch order expected by
    /// [`avcore::preview::Preview::seek_composited`]. Keeping this calculation shared by the
    /// initial open, timeline scrubbing, and live speed edits prevents those paths from
    /// drifting into subtly different synchronization rules.
    pub(super) fn preview_seek_parameters<'a>(
        clips: impl IntoIterator<Item = &'a ClipInstance>,
        playhead_secs: f64,
    ) -> (Vec<f64>, Vec<f64>) {
        clips
            .into_iter()
            .map(|clip| {
                (
                    Self::clip_seek_offset(clip, playhead_secs),
                    clip.speed_factor.max(0.01) as f64,
                )
            })
            .unzip()
    }
}
