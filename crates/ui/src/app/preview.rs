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

use avcore::TrackKind;
use eframe::egui;

use super::App;

/// Output resolution for [`App::pump_preview_frame`]'s waveform scope texture — wide enough to
/// resolve per-column detail against a typical Editor panel width, short enough that the per-
/// frame `avcore::luma_waveform_rgba` pass (opt-in via `scopes_enabled`) stays cheap.
const SCOPE_WAVEFORM_SIZE: (u32, u32) = (256, 128);
/// Output resolution (square) for the vectorscope texture.
const SCOPE_VECTORSCOPE_SIZE: u32 = 128;

/// Playhead for a frozen clip after `elapsed_secs` of wall-clock playback since
/// `playhead_at_start`, clamped to the clip's own end (`clip_start_secs + clip_duration_secs`)
/// — pulled out of [`App::pump_preview_frame`] as a pure function so the math is unit
/// testable without a live `Preview` pipeline (every other preview test in `app_test.rs` runs
/// against a nonexistent source path specifically to avoid needing one).
pub(crate) fn frozen_playhead(
    playhead_at_start: f64,
    elapsed_secs: f64,
    clip_start_secs: f64,
    clip_duration_secs: f64,
) -> f64 {
    (playhead_at_start + elapsed_secs).min(clip_start_secs + clip_duration_secs)
}

impl App {
    /// Updates the timeline cursor immediately and seeks the preview during a ruler drag.
    ///
    /// GStreamer seeks can block long enough to make the cursor trail behind the pointer when
    /// invoked for every drag frame. Seeks are coalesced to preview cadence, then the ruler
    /// sends an exact final seek when the gesture ends.
    pub(crate) fn scrub_preview_playhead(&mut self, position_secs: f64) {
        self.active_project_mut_untracked()
            .timeline_mut()
            .playhead_secs = position_secs;

        // Keep the ruler glued to the pointer on every event, but only hand the latest sample
        // to GStreamer at preview cadence. GStreamer seeks are asynchronous, yet dispatching
        // more than the display can show still wastes UI-frame time and made the ruler trail
        // the mouse. A 30 Hz preview is visually live while limiting obsolete requests.
        const SCRUB_SEEK_INTERVAL: std::time::Duration = std::time::Duration::from_millis(33);
        let now = std::time::Instant::now();
        let should_seek = self
            .preview_state
            .last_scrub_seek_at
            .is_none_or(|last_seek| now.duration_since(last_seek) >= SCRUB_SEEK_INTERVAL);
        if should_seek {
            self.preview_state.last_scrub_seek_at = Some(now);
            self.seek_preview(position_secs);
        }
    }

    /// Sends the final ruler position to the preview and resets drag-local seek coalescing.
    pub(crate) fn finish_preview_scrub(&mut self, position_secs: f64) {
        self.preview_state.last_scrub_seek_at = None;
        self.seek_preview(position_secs);
    }

    /// Drops the current pipeline/texture so the next Editor frame rebuilds overlay branches
    /// from their latest styling. Word-timing changes can replace a text branch's buffer live,
    /// but arbitrary text/shape property edits still use this conservative full rebuild.
    pub fn invalidate_preview_rendering(&mut self) {
        self.preview_state.worker = None;
        self.preview_state.worker_ready_generation = None;
        self.preview_state.worker_error = None;
        self.preview_state.worker_audio_level = avcore::AudioLevel::default();
        self.preview_state.worker_generation = self.preview_state.worker_generation.wrapping_add(1);
        self.preview_state.preview_texture = None;
        self.preview_state.last_frame = None;
        self.preview_state.waveform_texture = None;
        self.preview_state.vectorscope_texture = None;
        self.preview_state.preview_clip_id = None;
        self.preview_state.preview_overlay_clip_ids.clear();
        self.preview_state.preview_audio_clip_ids.clear();
        self.preview_state.preview_text_clip_ids.clear();
        self.preview_state.preview_shape_clip_ids.clear();
    }

    /// Applies the persisted hardware-decoding preference and drops any currently-open
    /// pipeline so the next Editor paint rebuilds it with the new decoder policy.
    pub fn set_preview_hardware_decode(&mut self, enabled: bool) {
        if self.prefs.preview_hardware_decode == enabled {
            return;
        }
        self.prefs.preview_hardware_decode = enabled;
        self.invalidate_preview_rendering();
    }

    /// Re-applies the selected media clip's new playback speed at the current timeline
    /// playhead. [`App::seek_preview`] already owns the branch-matching and single/composited
    /// seek rules, so using it here preserves every active branch's synchronization and does
    /// not rebuild the pipeline or jump back to the clip start. If the edited clip is not part
    /// of the currently loaded preview, there is nothing live to update.
    pub(super) fn refresh_preview_speed(&mut self, clip_id: u64) {
        let clip_is_loaded = self.preview_state.preview_clip_id == Some(clip_id)
            || self
                .preview_state
                .preview_overlay_clip_ids
                .contains(&clip_id)
            || self.preview_state.preview_audio_clip_ids.contains(&clip_id);
        if !clip_is_loaded || self.preview_state.worker.is_none() {
            return;
        }
        let playhead_secs = self.active_project().timeline().playhead_secs;
        self.seek_preview(playhead_secs);
    }

    /// Reopens the preview pipeline whenever the clip covering the timeline playhead
    /// ([`App::current_preview_clip`]) differs from the one last opened for
    /// (`preview_clip_id`) — called once per frame from the Editor's preview panel, right
    /// before it reads any preview state, so the first paint after the playhead moves onto a
    /// different clip is what actually asks the worker to open a pipeline. Prefers the editing proxy if
    /// one exists (lighter to decode), otherwise the original source file. Leaves `preview` as
    /// unavailable without an error dialog if opening fails (e.g. a source file that's been
    /// moved or deleted since import) — the Editor screen shows a muted "preview unavailable"
    /// label instead, and won't retry until the playhead moves onto a different clip. Seeks
    /// into the newly opened clip at the playhead's own offset, and resumes playback
    /// (`preview_playing` was already `true`) so crossing a cut doesn't pause playback, just
    /// hitches while the new pipeline opens.
    pub fn ensure_preview_loaded(&mut self) {
        // Cheap id-only pass first -- called once per frame, including every frame of
        // uninterrupted playback where nothing at the playhead has actually changed, so this
        // avoids paying current_preview_clip()'s (and its overlay/audio/text/shape siblings')
        // full ClipInstance/MediaAsset clone cost (every keyframe Vec on the clip, an asset
        // clone entirely unused once ids are known to match) on the common no-op case. The full
        // clone-based resolution below only runs when something actually changed, which is also
        // exactly when a reopen needs that owned data anyway.
        if self.current_preview_clip_id() == self.preview_state.preview_clip_id
            && self.current_preview_overlay_clip_ids()
                == self.preview_state.preview_overlay_clip_ids
            && self.current_preview_audio_clip_ids() == self.preview_state.preview_audio_clip_ids
            && self.current_preview_text_clip_ids() == self.preview_state.preview_text_clip_ids
            && self.current_preview_shape_clip_ids() == self.preview_state.preview_shape_clip_ids
        {
            return;
        }

        // Compound clips (nested sequences) resolve through this same media-library-lookup
        // path as an ordinary asset — merging in materialize_nested_sequences_for_active_
        // sequence's synthetic assets here means current_preview_clip/current_preview_overlay_
        // clips/current_preview_audio_clips need no nested-sequence awareness of their own.
        // Cache-backed (see avcore::nested_sequence's own doc comment), so this only actually
        // re-renders once per edit to a referenced nested sequence, not once per frame.
        let nested_assets = self.materialize_nested_sequences_for_active_sequence();
        let mut media_library = self.active_project().media_library.clone();
        media_library.extend(nested_assets);

        let current = self.current_preview_clip(&media_library);
        let (overlays, audio_clips, text_clips, shape_clips) = if current.is_some() {
            (
                self.current_preview_overlay_clips(&media_library),
                self.current_preview_audio_clips(&media_library),
                self.current_preview_text_clips(),
                self.current_preview_shape_clips(),
            )
        } else {
            (Vec::new(), Vec::new(), Vec::new(), Vec::new())
        };
        let overlay_ids: Vec<u64> = overlays.iter().map(|(c, _)| c.id).collect();
        let audio_ids: Vec<u64> = audio_clips.iter().map(|(clip, _)| clip.id).collect();
        let text_ids: Vec<u64> = text_clips.iter().map(|c| c.id).collect();
        let shape_ids: Vec<u64> = shape_clips.iter().map(|c| c.id).collect();
        let current_clip_id = current.as_ref().map(|(c, _)| c.id);
        self.preview_state.preview_texture = None;
        self.preview_state.last_frame = None;
        self.preview_state.waveform_texture = None;
        self.preview_state.vectorscope_texture = None;
        // Not just the background id — `preview_clip_present()` (and the Editor's "preview
        // unavailable" vs. plain placeholder choice) needs to tell "a clip is here but its
        // pipeline failed to open" apart from "there's nothing to preview at all", and an
        // unresolvable asset is the latter, not the former.
        self.preview_state.preview_clip_id = current_clip_id;
        self.preview_state.preview_overlay_clip_ids = overlay_ids;
        self.preview_state.preview_audio_clip_ids = audio_ids;
        self.preview_state.preview_text_clip_ids = text_ids;
        self.preview_state.preview_shape_clip_ids = shape_ids;

        let Some((clip, asset)) = current else {
            // Nothing covers the new playhead -- either the timeline just ran out (scrubbed/
            // played past the last clip) or the user seeked into a gap. `loop_enabled` turns
            // the former into "restart from 0 instead of stopping": only when playback was
            // actually running (a paused seek into a gap should stay paused, not start
            // playing), seek to the start and leave `preview_playing` as-is so the next call
            // (next frame) resolves and opens the clip at 0.0 the same way any other playhead
            // move does.
            if self.preview_state.loop_enabled && self.preview_state.preview_playing {
                self.active_project_mut_untracked()
                    .timeline_mut()
                    .playhead_secs = 0.0;
                return;
            }
            self.preview_state.preview_playing = false;
            return;
        };
        let Some(path) = Self::preview_source_path(&asset) else {
            // A moved/deleted source file would otherwise still spin up a whole GStreamer
            // pipeline just to watch it fail to open a file that isn't there.
            return;
        };
        let playhead = self.active_project().timeline().playhead_secs;

        let wants_composited = !(overlays.is_empty()
            && audio_clips.is_empty()
            && text_clips.is_empty()
            && shape_clips.is_empty());
        let overlay_paths: Vec<Option<std::path::PathBuf>> = overlays
            .iter()
            .map(|(_, a)| Self::preview_source_path(a))
            .collect();
        let audio_paths: Vec<Option<std::path::PathBuf>> = audio_clips
            .iter()
            .map(|(_, asset)| Self::preview_source_path(asset))
            .collect();
        // Every video overlay clip needs a resolvable path too — falling back to the plain
        // single-clip pipeline (background only, dropping any text/shape overlays as well)
        // rather than silently dropping just the unresolvable overlay would misrepresent which
        // clips are actually compositing.
        let composited = wants_composited
            && overlay_paths.iter().all(Option::is_some)
            && audio_paths.iter().all(Option::is_some);

        let request = if composited {
            crate::app::PreviewOpenRequest::Composited {
                path,
                clip: clip.clone(),
                overlays: overlay_paths
                    .into_iter()
                    .zip(overlays)
                    .map(|(path, (clip, _))| (path.expect("checked above"), clip))
                    .collect(),
                audio: audio_paths
                    .into_iter()
                    .zip(audio_clips)
                    .map(|(path, (clip, _))| (path.expect("checked above"), clip))
                    .collect(),
                text: text_clips,
                shapes: shape_clips,
                playhead_secs: playhead,
                hardware_decode: self.prefs.preview_hardware_decode,
            }
        } else {
            crate::app::PreviewOpenRequest::Single {
                path,
                clip: clip.clone(),
                playhead_secs: playhead,
                hardware_decode: self.prefs.preview_hardware_decode,
            }
        };
        let generation = self.preview_state.worker_generation.wrapping_add(1);
        self.preview_state.worker_generation = generation;
        self.preview_state.worker_ready_generation = None;
        self.preview_state.worker_error = None;
        let worker = self
            .preview_state
            .worker
            .get_or_insert_with(crate::app::PreviewWorker::spawn);
        worker.open(generation, request);
        self.preview_state.preview_frozen_since = (clip.frozen
            && self.preview_state.preview_playing)
            .then_some((std::time::Instant::now(), playhead));
        if self.preview_state.preview_playing && !clip.frozen {
            worker.set_playing(true);
        }
    }

    /// Toggles play/pause on the current preview pipeline. A no-op if nothing is selected or
    /// the pipeline failed to open. A frozen clip's underlying pipeline stays `Paused`
    /// regardless — only [`App::preview_frozen_since`] starts/stops, driving the playhead
    /// forward on its own since there's no advancing pipeline position to read for a held
    /// frame.
    pub fn toggle_preview_playback(&mut self) {
        let Some(worker) = &self.preview_state.worker else {
            return;
        };
        let frozen = self.current_preview_video_clip().is_some_and(|c| c.frozen);
        let now_playing = !self.preview_state.preview_playing;
        if !frozen {
            worker.set_playing(now_playing);
        }
        self.preview_state.preview_playing = now_playing;
        self.preview_state.preview_frozen_since = (frozen && now_playing).then_some((
            std::time::Instant::now(),
            self.active_project().timeline().playhead_secs,
        ));
    }

    /// Seeks to `position_secs` (timeline-relative). Takes the fast path — seeking the
    /// already-open pipeline directly — when `position_secs` still falls within the clip it's
    /// currently loaded for; otherwise updates the timeline playhead and lets
    /// [`App::ensure_preview_loaded`] open the right clip's pipeline next frame. A no-op if
    /// nothing is selected or the pipeline failed to open.
    pub fn seek_preview(&mut self, position_secs: f64) {
        let same_clip = self
            .preview_state
            .preview_clip_id
            .zip(self.current_preview_video_clip())
            .filter(|(loaded_id, clip)| *loaded_id == clip.id)
            .map(|(_, clip)| clip);

        // For a composited pipeline, the overlay set covering the playhead — video, text, and
        // shape alike — must also still match exactly what was opened for: a plain
        // background-id match isn't enough once there are overlay branches, since the fast
        // path below reuses the already-open pipeline's branches as-is rather than rebuilding
        // them.
        let timeline = self.active_project().timeline();
        let overlays_still_match = {
            let overlay_ids: Vec<u64> = timeline
                .tracks
                .iter()
                .filter(|t| t.kind == TrackKind::Video && t.visible)
                .skip(1)
                .filter_map(|t| t.clip_at(position_secs).map(|c| c.id))
                .collect();
            overlay_ids == self.preview_state.preview_overlay_clip_ids
        };
        let audio_still_match = {
            let audio_ids: Vec<u64> = timeline
                .tracks
                .iter()
                .filter(|track| track.kind == TrackKind::Audio && track.visible)
                .filter_map(|track| track.clip_at(position_secs).map(|clip| clip.id))
                .collect();
            audio_ids == self.preview_state.preview_audio_clip_ids
        };
        let text_still_match = {
            let text_ids: Vec<u64> = timeline
                .tracks
                .iter()
                .filter(|t| t.kind == TrackKind::Text)
                .filter_map(|t| {
                    t.text_clips
                        .iter()
                        .find(|c| {
                            position_secs >= c.start_secs
                                && position_secs < c.start_secs + c.duration_secs
                        })
                        .map(|c| c.id)
                })
                .collect();
            text_ids == self.preview_state.preview_text_clip_ids
        };
        let shape_still_match = {
            let shape_ids: Vec<u64> = timeline
                .tracks
                .iter()
                .filter(|t| t.kind == TrackKind::Shape)
                .filter_map(|t| {
                    t.shape_clips
                        .iter()
                        .find(|c| {
                            position_secs >= c.start_secs
                                && position_secs < c.start_secs + c.duration_secs
                        })
                        .map(|c| c.id)
                })
                .collect();
            shape_ids == self.preview_state.preview_shape_clip_ids
        };
        let is_composited = !self.preview_state.preview_overlay_clip_ids.is_empty()
            || !self.preview_state.preview_audio_clip_ids.is_empty()
            || !self.preview_state.preview_text_clip_ids.is_empty()
            || !self.preview_state.preview_shape_clip_ids.is_empty();
        let branches_still_match =
            overlays_still_match && audio_still_match && text_still_match && shape_still_match;

        // The UI owns the timeline position, but the worker exclusively owns and seeks the
        // GStreamer pipeline. A new scrub request overwrites any one that has not started.
        if let (Some(worker), Some(clip)) = (
            self.preview_state.worker.as_ref(),
            same_clip.as_ref().filter(|_| branches_still_match),
        ) {
            let seek = if is_composited {
                let timeline = self.active_project().timeline();
                let overlays = timeline
                    .tracks
                    .iter()
                    .filter(|track| track.visible && track.kind == TrackKind::Video)
                    .skip(1)
                    .filter_map(|track| track.clip_at(position_secs));
                let audio = timeline
                    .tracks
                    .iter()
                    .filter(|track| track.visible && track.kind == TrackKind::Audio)
                    .filter_map(|track| track.clip_at(position_secs));
                let (offsets, rates) = Self::preview_seek_parameters(
                    std::iter::once(clip).chain(overlays).chain(audio),
                    position_secs,
                );
                crate::app::PreviewSeek::Composited { offsets, rates }
            } else {
                crate::app::PreviewSeek::Single {
                    offset_secs: Self::clip_seek_offset(clip, position_secs),
                    rate: clip.speed_factor.max(0.01) as f64,
                }
            };
            let generation = self.preview_state.worker_generation.wrapping_add(1);
            self.preview_state.worker_generation = generation;
            worker.seek(generation, seek);
            self.active_project_mut_untracked()
                .timeline_mut()
                .playhead_secs = position_secs;
            self.refresh_preview_text_highlights(position_secs);
            return;
        }

        self.active_project_mut_untracked()
            .timeline_mut()
            .playhead_secs = position_secs;
        self.refresh_preview_text_highlights(position_secs);
    }

    /// Fallback fps for [`App::step_preview_frame`] when the previewed clip's own asset has no
    /// probed fps — matches [`avcore::render`]'s own `unwrap_or(30.0)` convention for the same
    /// situation, rather than a second, different guess.
    const STEP_FRAME_FALLBACK_FPS: f32 = 30.0;

    /// Steps the playhead by exactly one frame (`delta_frames` of `1` or `-1`), at the fps of
    /// the clip currently covering it (falling back to [`Self::STEP_FRAME_FALLBACK_FPS`] when
    /// unknown) — the OCA mockup's transport-row step-back/step-forward buttons
    /// (`spec/architecture/editor-ui-visual-redesign.md`'s Program monitor mapping), which had
    /// no equivalent before this. Clamped to `[0, timeline duration]` and routed through
    /// [`App::seek_preview`], same as any other playhead move — no separate frame-stepping
    /// pipeline path.
    pub fn step_preview_frame(&mut self, delta_frames: i64) {
        let fps = self
            .current_preview_fps()
            .unwrap_or(Self::STEP_FRAME_FALLBACK_FPS)
            .max(1.0) as f64;
        let playhead = self.active_project().timeline().playhead_secs;
        let duration = self.active_project().timeline().duration_secs();
        let new_position = (playhead + delta_frames as f64 / fps).clamp(0.0, duration);
        self.seek_preview(new_position);
    }

    /// Whether the clip at the timeline playhead has a live preview pipeline — `false` before
    /// any clip covers the playhead, before [`App::ensure_preview_loaded`] has run for it,
    /// and when it couldn't open one.
    pub fn preview_available(&self) -> bool {
        self.preview_state.worker_ready_generation == Some(self.preview_state.worker_generation)
    }

    /// Whether a clip currently covers the timeline playhead, whether or not its preview
    /// pipeline could actually be opened — distinguishes "nothing to preview here" from
    /// "something's here but its preview failed to open" for the Editor's empty-state label.
    pub fn preview_clip_present(&self) -> bool {
        self.preview_state.preview_clip_id.is_some()
    }

    /// The live playback audio level (peak/RMS) for the Editor preview panel's meter widget —
    /// `spec/ROADMAP.md` P4 item 30. Silent default (`AudioLevel::default()`) when no preview
    /// pipeline is open at all, same as the worker reports when the pipeline has
    /// already reports when the pipeline is open but nothing has decoded yet (e.g. before the
    /// first play) or the clip has no audio.
    pub fn current_audio_level(&self) -> avcore::AudioLevel {
        self.preview_state.worker_audio_level
    }

    /// Pulls the latest decoded video frame (if any) into `preview_texture`, and — while
    /// playing — mirrors the pipeline's position into the active project's timeline playhead,
    /// converting from the clip-relative position `Preview` reports back to timeline time.
    /// Once the clip currently loaded (`preview_clip_id`) plays past its own
    /// `source_out_secs`, advances the playhead to that clip's end instead — the next frame's
    /// `ensure_preview_loaded` call then naturally opens whatever clip (if any) covers that
    /// position, continuing playback across the cut. Called once per frame from
    /// [`eframe::App::ui`], before the screens draw.
    pub(super) fn pump_preview_frame(&mut self, ctx: &egui::Context) {
        // Home intentionally supports a fresh session with no project. The frame loop still
        // pumps background queues in that state, so preview work must not resolve an active
        // timeline until a project exists.
        if self.open_projects.is_empty() {
            return;
        }
        // Resolve all model-owned effect settings before taking a frame from the worker so the
        // decoded image can be processed and uploaded without a second project lookup.
        let cpu_effects = self.current_preview_cpu_effects();
        if self.preview_state.preview_deflicker_history.0 != cpu_effects.clip_id {
            self.preview_state.preview_deflicker_history =
                (cpu_effects.clip_id, avcore::DeflickerHistory::new());
        }

        let Some(worker) = &self.preview_state.worker else {
            return;
        };
        while let Some(status) = worker.take_status() {
            match status {
                avcore::preview_worker::PreviewStatus::Ready { generation }
                    if generation == self.preview_state.worker_generation =>
                {
                    self.preview_state.worker_ready_generation = Some(generation);
                }
                avcore::preview_worker::PreviewStatus::Failed {
                    generation,
                    message,
                } if generation == self.preview_state.worker_generation => {
                    self.preview_state.worker_ready_generation = None;
                    self.preview_state.worker_error = Some(message);
                }
                _ => {}
            }
        }
        let Some(ready) = worker.take_frame() else {
            return;
        };
        if !avcore::preview_worker::generation_is_current(
            self.preview_state.worker_generation,
            ready.generation,
        ) {
            return;
        }
        self.preview_state.worker_ready_generation = Some(ready.generation);
        let frame_position_secs = ready.position_secs;
        self.preview_state.worker_audio_level = ready.audio_level;
        let mut frame = ready.frame;

        {
            // P4 item 21 (`spec/ROADMAP.md`) -- CPU-side preview approximation for the two
            // effects with no matching GStreamer element on any dev machine checked (see
            // `avcore::preview_effects`'s own doc comment for why only these two, and why this
            // is an approximation, not bit-exact to the real `lut3d`/`vignette` avfilters export
            // uses). Applied in place, before upload, so it costs nothing when neither is set.
            if !cpu_effects.lut_path.is_empty() {
                let needs_reparse = self
                    .preview_state
                    .preview_lut_cache
                    .as_ref()
                    .is_none_or(|(cached_path, _)| cached_path != &cpu_effects.lut_path);
                if needs_reparse {
                    let parsed =
                        avcore::Lut3D::load(std::path::Path::new(&cpu_effects.lut_path)).ok();
                    self.preview_state.preview_lut_cache =
                        Some((cpu_effects.lut_path.clone(), parsed));
                }
                if let Some((_, Some(lut))) = &self.preview_state.preview_lut_cache {
                    avcore::apply_lut_to_rgba(&mut frame.rgba, lut);
                }
            }
            if cpu_effects.vignette_intensity > 0.0 {
                avcore::apply_vignette_to_rgba(
                    &mut frame.rgba,
                    frame.width,
                    frame.height,
                    cpu_effects.vignette_intensity,
                );
            }
            if cpu_effects.glitch_intensity > 0.0 {
                // Elapsed wall-clock time bit-cast to a seed -- differs every frame (unlike a
                // frame counter, doesn't need a new PreviewState field), giving the temporal
                // "digital corruption" look apply_glitch_to_rgba's own doc comment describes
                // rather than a static grain texture.
                let seed = ctx.input(|i| i.time).to_bits();
                avcore::apply_glitch_to_rgba(&mut frame.rgba, cpu_effects.glitch_intensity, seed);
            }
            if cpu_effects.deflicker_enabled {
                avcore::apply_deflicker_to_rgba(
                    &mut frame.rgba,
                    &mut self.preview_state.preview_deflicker_history.1,
                );
            }

            let image = egui::ColorImage::from_rgba_unmultiplied(
                [frame.width as usize, frame.height as usize],
                &frame.rgba,
            );
            match &mut self.preview_state.preview_texture {
                Some(texture) => texture.set(image, egui::TextureOptions::LINEAR),
                None => {
                    self.preview_state.preview_texture =
                        Some(ctx.load_texture("preview", image, egui::TextureOptions::LINEAR));
                }
            }
            self.preview_state.last_frame = Some(frame.clone());

            if self.preview_state.scopes_enabled {
                let (waveform_rgba, vectorscope_rgba) = avcore::render_scopes_rgba(
                    &frame.rgba,
                    frame.width,
                    frame.height,
                    SCOPE_WAVEFORM_SIZE.0,
                    SCOPE_WAVEFORM_SIZE.1,
                    SCOPE_VECTORSCOPE_SIZE,
                );
                let waveform_image = egui::ColorImage::from_rgba_unmultiplied(
                    [
                        SCOPE_WAVEFORM_SIZE.0 as usize,
                        SCOPE_WAVEFORM_SIZE.1 as usize,
                    ],
                    &waveform_rgba,
                );
                match &mut self.preview_state.waveform_texture {
                    Some(texture) => texture.set(waveform_image, egui::TextureOptions::LINEAR),
                    None => {
                        self.preview_state.waveform_texture = Some(ctx.load_texture(
                            "scope-waveform",
                            waveform_image,
                            egui::TextureOptions::LINEAR,
                        ));
                    }
                }

                let vectorscope_image = egui::ColorImage::from_rgba_unmultiplied(
                    [
                        SCOPE_VECTORSCOPE_SIZE as usize,
                        SCOPE_VECTORSCOPE_SIZE as usize,
                    ],
                    &vectorscope_rgba,
                );
                match &mut self.preview_state.vectorscope_texture {
                    Some(texture) => texture.set(vectorscope_image, egui::TextureOptions::LINEAR),
                    None => {
                        self.preview_state.vectorscope_texture = Some(ctx.load_texture(
                            "scope-vectorscope",
                            vectorscope_image,
                            egui::TextureOptions::LINEAR,
                        ));
                    }
                }
            }
        }

        if !self.preview_state.preview_playing {
            return;
        }
        let Some(clip_id) = self.preview_state.preview_clip_id else {
            return;
        };
        // Just the scalar fields this playhead-advance math actually reads, not a full
        // ClipInstance clone (which would drag along every keyframe Vec on the clip) — this
        // runs every frame during playback, the same hot-path discipline
        // current_preview_clip_lut_and_vignette already applies a few lines up.
        let Some((
            frozen,
            start_secs,
            duration_secs,
            source_in_secs,
            source_out_secs,
            speed_factor,
        )) = self
            .active_project()
            .timeline()
            .tracks
            .iter()
            .find_map(|t| {
                t.clips.iter().find(|c| c.id == clip_id).map(|c| {
                    (
                        c.frozen,
                        c.start_secs,
                        c.duration_secs(),
                        c.source_in_secs,
                        c.source_out_secs,
                        c.speed_factor,
                    )
                })
            })
        else {
            return;
        };

        let new_playhead = if frozen {
            // The pipeline is paused on the held anchor frame (see ensure_preview_loaded), so
            // there's no Preview::position_secs to derive playback progress from — advance the
            // playhead by wall-clock time elapsed since playback started instead, same
            // real-time rate a normally-decoding clip's own position would advance at.
            let (started_at, playhead_at_start) = *self
                .preview_state
                .preview_frozen_since
                .get_or_insert_with(|| (std::time::Instant::now(), start_secs));
            let elapsed = started_at.elapsed().as_secs_f64();
            frozen_playhead(playhead_at_start, elapsed, start_secs, duration_secs)
        } else {
            let Some(position) = frame_position_secs else {
                return;
            };
            if position >= source_out_secs {
                start_secs + duration_secs
            } else {
                let speed = speed_factor.max(0.01) as f64;
                start_secs + (position - source_in_secs) / speed
            }
        };
        // Not `active_project_mut()` -- that marks the project dirty and resets the autosave
        // debounce timer, and this runs every frame during playback. Left unfixed, the 2s
        // debounce never elapses (last_edit_instant is refreshed every frame) so only the 30s
        // ceiling remains, and it always finds a "dirty" project -- a full synchronous
        // gzip+msgpack serialize (pump_autosave) firing on the UI thread roughly every 30s of
        // playback, visible as a periodic multi-hundred-ms stutter. Mirroring the pipeline's own
        // playhead position is not a user edit worth autosave-protecting at every frame anyway.
        self.active_project_mut_untracked()
            .timeline_mut()
            .playhead_secs = new_playhead;
        self.refresh_preview_text_highlights(new_playhead);
    }
}
