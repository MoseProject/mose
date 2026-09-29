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

//! D2 (`spec/architecture/differentiators.md`): scans a long VOD for simultaneous game-audio +
//! mic spikes (scream, death, chat reaction) and marks highlight candidates. Built on
//! [`crate::waveform`]'s per-bucket peaks and [`crate::timeline::AudioRole`] (added specifically
//! to answer "which track is which" — see `ROADMAP.md` P3 item 17's own investigation note for
//! why this couldn't be built blind the way D1/D4/D5 were).
//!
//! [`clip_amplitude_samples`] maps one clip's asset waveform into timeline-relative amplitude
//! samples — same source-to-timeline mapping [`crate::silence_detection::clip_silence_gaps`]
//! uses, just reporting every bucket's peak instead of only below-threshold runs, since a
//! highlight cares about the *loud* moments, not the quiet ones. [`detect_highlight_candidates`]
//! then correlates two independently-sampled/irregularly-spaced series (a game-audio track and a
//! mic track rarely share the same waveform bucket timestamps) onto a common coarse time grid,
//! and flags grid windows where both channels spike at once.

use crate::timeline::ClipInstance;

/// One timeline-relative amplitude sample — a single waveform bucket's peak (`min.abs().max(
/// max.abs())`, `[0.0, 1.0]`), already mapped from asset/source coordinates into timeline
/// coordinates for one clip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimelineAmplitudeSample {
    pub at_secs: f64,
    pub amplitude: f32,
}

/// One detected highlight window — a stretch where the game-audio and mic channels both spiked
/// at the same time. `score` is the weaker of the two channels' peak amplitude across the
/// window (a highlight needs *both* to be loud, not just one), useful for ranking candidates or
/// retuning the threshold without re-sampling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HighlightCandidate {
    pub start_secs: f64,
    pub end_secs: f64,
    pub score: f32,
}

impl HighlightCandidate {
    pub fn duration_secs(&self) -> f64 {
        self.end_secs - self.start_secs
    }
}

/// Default minimum amplitude to count as a "spike" on either channel — roughly -6 dBFS, well
/// above ordinary commentary/game-ambience level, low enough to still catch a real scream/death-
/// sound/chat-alert without needing full-scale clipping.
pub const DEFAULT_HIGHLIGHT_THRESHOLD_LINEAR: f32 = 0.5;
/// Width of the common time grid [`detect_highlight_candidates`] correlates both channels on —
/// coarse enough that a game-audio bucket and a mic bucket a few hundred ms apart (they're
/// rarely sampled at identical timestamps, coming from different clips/assets) still land in the
/// same window, tight enough to localize a highlight to roughly the right few seconds.
pub const DEFAULT_HIGHLIGHT_GRID_SECS: f64 = 0.5;
/// Default minimum candidate duration — shorter simultaneous spikes are more likely coincidental
/// overlap than a genuine reaction moment.
pub const DEFAULT_HIGHLIGHT_MIN_DURATION_SECS: f64 = 1.0;
/// Hard upper bound for one correlation pass. At the default 0.5s resolution this still covers
/// nearly six days of continuous timeline, while preventing a corrupt timestamp from requesting
/// an unbounded allocation.
pub const MAX_HIGHLIGHT_GRID_WINDOWS: usize = 1_000_000;

/// [`TimelineAmplitudeSample`]s for every waveform bucket of `asset_peaks`/`asset_duration_secs`
/// that falls within `clip`'s visible `source_in_secs..source_out_secs` range, mapped into
/// timeline-relative seconds — same source-to-timeline formula
/// [`crate::silence_detection::clip_silence_gaps`] uses. Empty for a clip with a non-positive
/// `speed_factor`.
pub fn clip_amplitude_samples(
    asset_peaks: &[(f32, f32)],
    asset_duration_secs: f64,
    clip: &ClipInstance,
) -> Vec<TimelineAmplitudeSample> {
    if !clip.speed_factor.is_finite()
        || clip.speed_factor <= 0.0
        || asset_peaks.is_empty()
        || !asset_duration_secs.is_finite()
        || asset_duration_secs <= 0.0
        || !clip.start_secs.is_finite()
        || !clip.source_in_secs.is_finite()
        || !clip.source_out_secs.is_finite()
        || clip.source_out_secs <= clip.source_in_secs
    {
        return Vec::new();
    }

    let bucket_secs = asset_duration_secs / asset_peaks.len() as f64;
    let to_timeline_secs = |source_secs: f64| {
        clip.start_secs + (source_secs - clip.source_in_secs) / clip.speed_factor as f64
    };

    asset_peaks
        .iter()
        .enumerate()
        .filter_map(|(i, &(min, max))| {
            if !min.is_finite() || !max.is_finite() {
                return None;
            }
            let bucket_start = i as f64 * bucket_secs;
            let bucket_end = bucket_start + bucket_secs;
            if bucket_end <= clip.source_in_secs || bucket_start >= clip.source_out_secs {
                return None;
            }
            let bucket_center = (bucket_start + bucket_end) / 2.0;
            Some(TimelineAmplitudeSample {
                at_secs: to_timeline_secs(bucket_center),
                amplitude: min.abs().max(max.abs()),
            })
        })
        .collect()
}

/// Correlates `game_audio_samples` and `mic_samples` (each typically built from
/// [`clip_amplitude_samples`] across every clip on a differently-`AudioRole`-tagged track) onto
/// a common `grid_secs`-wide time grid, taking each grid window's *max* amplitude per channel
/// (a spike is brief, so a max, not an average, is what should trigger it). A window counts as
/// "hot" when both channels' max amplitude in that window is at least `threshold_linear`;
/// consecutive hot windows merge into one [`HighlightCandidate`], dropped if its total duration
/// is under `min_duration_secs`. Empty if either channel has no samples.
pub fn detect_highlight_candidates(
    game_audio_samples: &[TimelineAmplitudeSample],
    mic_samples: &[TimelineAmplitudeSample],
    threshold_linear: f32,
    grid_secs: f64,
    min_duration_secs: f64,
) -> Vec<HighlightCandidate> {
    if game_audio_samples.is_empty()
        || mic_samples.is_empty()
        || !grid_secs.is_finite()
        || grid_secs <= 0.0
        || !threshold_linear.is_finite()
        || !min_duration_secs.is_finite()
    {
        return Vec::new();
    }

    let valid_samples = game_audio_samples
        .iter()
        .chain(mic_samples)
        .filter(|sample| {
            sample.at_secs.is_finite() && sample.at_secs >= 0.0 && sample.amplitude.is_finite()
        })
        .collect::<Vec<_>>();
    let Some(min_at_secs) = valid_samples
        .iter()
        .map(|sample| sample.at_secs)
        .min_by(|a, b| a.total_cmp(b))
    else {
        return Vec::new();
    };
    let max_at_secs = valid_samples
        .iter()
        .map(|sample| sample.at_secs)
        .max_by(|a, b| a.total_cmp(b))
        .expect("valid samples are non-empty");
    let grid_start_secs = (min_at_secs / grid_secs).floor() * grid_secs;
    let grid_len = ((max_at_secs - grid_start_secs) / grid_secs).floor() + 1.0;
    if !grid_len.is_finite() || grid_len > MAX_HIGHLIGHT_GRID_WINDOWS as f64 {
        return Vec::new();
    }
    let grid_len = grid_len as usize;

    let grid_max = |samples: &[TimelineAmplitudeSample]| -> Vec<f32> {
        let mut grid = vec![0.0f32; grid_len];
        for sample in samples {
            if !sample.at_secs.is_finite() || sample.at_secs < 0.0 || !sample.amplitude.is_finite()
            {
                continue;
            }
            let index = ((sample.at_secs - grid_start_secs) / grid_secs).floor() as usize;
            if let Some(slot) = grid.get_mut(index) {
                *slot = slot.max(sample.amplitude);
            }
        }
        grid
    };
    let game_grid = grid_max(game_audio_samples);
    let mic_grid = grid_max(mic_samples);

    let mut candidates = Vec::new();
    let mut run_start: Option<(usize, f32)> = None; // (start index, min-of-the-two score so far)

    let flush =
        |run: &mut Option<(usize, f32)>, end_index: usize, out: &mut Vec<HighlightCandidate>| {
            if let Some((start_index, score)) = run.take() {
                let start_secs = grid_start_secs + start_index as f64 * grid_secs;
                let end_secs = grid_start_secs + end_index as f64 * grid_secs;
                if end_secs - start_secs >= min_duration_secs {
                    out.push(HighlightCandidate {
                        start_secs,
                        end_secs,
                        score,
                    });
                }
            }
        };

    for i in 0..grid_len {
        let both_hot = game_grid[i] >= threshold_linear && mic_grid[i] >= threshold_linear;
        if both_hot {
            let window_score = game_grid[i].min(mic_grid[i]);
            run_start = Some(match run_start {
                Some((start, best)) => (start, best.max(window_score)),
                None => (i, window_score),
            });
        } else {
            flush(&mut run_start, i, &mut candidates);
        }
    }
    flush(&mut run_start, grid_len, &mut candidates);

    candidates
}

/// CF-02 (`spec/architecture/competitive-feature-plan.md`)'s "event-only" highlight scoring —
/// one [`HighlightCandidate`] window per [`crate::gameplay_events::GameplayEvent`], independently
/// testable from the audio-only detector above per that item's own acceptance criteria.
/// `to_timeline_secs` maps an event's source-relative timestamp onto the timeline, same shape
/// [`crate::gameplay_events::import_events_as_markers`] takes. `default_pre_roll_secs`/
/// `default_post_roll_secs` apply whenever an event doesn't specify its own override. Overlapping
/// windows (two events close enough together) merge into one, taking the higher `confidence` —
/// the same "don't double-count adjacent evidence" reasoning [`detect_highlight_candidates`]'s
/// own consecutive-hot-window merging already uses.
pub fn event_highlight_candidates(
    events: &[crate::gameplay_events::GameplayEvent],
    to_timeline_secs: impl Fn(f64) -> f64,
    default_pre_roll_secs: f64,
    default_post_roll_secs: f64,
) -> Vec<HighlightCandidate> {
    let mut windows: Vec<HighlightCandidate> = events
        .iter()
        .map(|event| {
            let center = to_timeline_secs(event.source_timestamp_secs);
            let pre = event
                .pre_roll_secs
                .map(|v| v as f64)
                .unwrap_or(default_pre_roll_secs);
            let post = event
                .post_roll_secs
                .map(|v| v as f64)
                .unwrap_or(default_post_roll_secs);
            HighlightCandidate {
                start_secs: (center - pre).max(0.0),
                end_secs: center + post,
                score: event.confidence,
            }
        })
        .collect();
    windows.sort_by(|a, b| a.start_secs.total_cmp(&b.start_secs));

    let mut merged: Vec<HighlightCandidate> = Vec::with_capacity(windows.len());
    for window in windows {
        if let Some(last) = merged.last_mut() {
            if window.start_secs <= last.end_secs {
                last.end_secs = last.end_secs.max(window.end_secs);
                last.score = last.score.max(window.score);
                continue;
            }
        }
        merged.push(window);
    }
    merged
}

/// CF-02's "combined" highlight scoring: unions `audio_candidates` (from
/// [`detect_highlight_candidates`]) and `event_candidates` (from
/// [`event_highlight_candidates`]), independently testable from either input alone per that
/// item's acceptance criteria. A window flagged by *both* sources is strictly stronger evidence
/// than either alone, so an overlap's merged score is boosted to `1.0` (maximum confidence)
/// rather than averaged; a window flagged by only one source keeps that source's own score
/// unchanged. Unlike [`detect_highlight_candidates`] itself (which requires *both* game-audio
/// and mic channels hot at once), combining doesn't require both an audio spike and an event —
/// CF-02's own goal is refining the existing detector with events as an additional signal, not
/// replacing it with a stricter one.
pub fn combine_highlight_candidates(
    audio_candidates: &[HighlightCandidate],
    event_candidates: &[HighlightCandidate],
) -> Vec<HighlightCandidate> {
    struct Tagged {
        window: HighlightCandidate,
        from_audio: bool,
        from_event: bool,
    }

    let mut tagged: Vec<Tagged> = audio_candidates
        .iter()
        .map(|c| Tagged {
            window: *c,
            from_audio: true,
            from_event: false,
        })
        .chain(event_candidates.iter().map(|c| Tagged {
            window: *c,
            from_audio: false,
            from_event: true,
        }))
        .collect();
    tagged.sort_by(|a, b| a.window.start_secs.total_cmp(&b.window.start_secs));

    let mut merged: Vec<Tagged> = Vec::with_capacity(tagged.len());
    for t in tagged {
        if let Some(last) = merged.last_mut() {
            if t.window.start_secs <= last.window.end_secs {
                last.window.end_secs = last.window.end_secs.max(t.window.end_secs);
                last.window.score = last.window.score.max(t.window.score);
                last.from_audio |= t.from_audio;
                last.from_event |= t.from_event;
                continue;
            }
        }
        merged.push(t);
    }

    merged
        .into_iter()
        .map(|t| HighlightCandidate {
            score: if t.from_audio && t.from_event {
                1.0
            } else {
                t.window.score
            },
            ..t.window
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::{BlendMode, ColorFilter, MaskShape, TransitionType};

    fn test_clip(
        start_secs: f64,
        source_in_secs: f64,
        source_out_secs: f64,
        speed_factor: f32,
    ) -> ClipInstance {
        ClipInstance {
            id: 1,
            asset_id: 1,
            start_secs,
            source_in_secs,
            source_out_secs,
            composite_id: None,
            color_label: None,
            gain_db: 0.0,
            frozen: false,
            speed_factor,
            speed_ramp_end_factor: None,
            nested_sequence_id: None,
            crop_x: 0.0,
            crop_y: 0.0,
            crop_w: 1.0,
            crop_h: 1.0,
            mask_shape: MaskShape::None,
            mask_corner_radius: 0.0,
            flipped_h: false,
            color_filter: ColorFilter::None,
            vignette_intensity: 0.0,
            brightness: 0.0,
            contrast: 1.0,
            saturation: 1.0,
            sharpen: 0.0,
            chroma_key_enabled: false,
            chroma_key_color: [0, 255, 0],
            chroma_key_tolerance: 0.4,
            blur_intensity: 0.0,
            shake_intensity: 0.0,
            glitch_intensity: 0.0,
            pixelize_intensity: 0.0,
            transition_in: TransitionType::None,
            transition_duration_secs: 0.5,
            position_keyframes: vec![],
            scale_keyframes: vec![],
            rotation_keyframes: vec![],
            opacity_keyframes: vec![],
            gain_keyframes: vec![],
            voice_cleanup_enabled: false,
            voice_cleanup_noise_floor_db: -30.0,
            voice_cleanup_compressor_threshold_db: -18.0,
            voice_cleanup_compressor_ratio: 3.0,
            voice_cleanup_ceiling_linear: 0.95,
            brightness_keyframes: vec![],
            contrast_keyframes: vec![],
            saturation_keyframes: vec![],
            crop_x_keyframes: vec![],
            crop_y_keyframes: vec![],
            crop_w_keyframes: vec![],
            crop_h_keyframes: vec![],
            deflicker_enabled: false,
            lut_path: String::new(),
            layer_scale_x: 1.0,
            layer_scale_y: 1.0,
            stabilization_intensity: 0.0,
            background_removal_enabled: false,
            background_removal_mask_path: String::new(),
            blend_mode: BlendMode::Normal,
            anchor_x: 0.5,
            anchor_y: 0.5,
            reframe_seed_point: None,
            privacy_blur_enabled: false,
            privacy_blur_mask_path: String::new(),
            privacy_blur_sigma: 15.0,
            privacy_blur_seed_vertices: Vec::new(),
            privacy_blur_seed_center_x_frac: 0.0,
            privacy_blur_seed_center_y_frac: 0.0,
        }
    }

    #[test]
    fn clip_amplitude_samples_maps_every_bucket_into_timeline_coordinates() {
        // 10 one-second buckets, unity speed, clip starts at timeline 100 showing full source.
        let peaks = vec![(-0.1, 0.1); 10];
        let clip = test_clip(100.0, 0.0, 10.0, 1.0);

        let samples = clip_amplitude_samples(&peaks, 10.0, &clip);

        assert_eq!(samples.len(), 10);
        assert_eq!(samples[0].at_secs, 100.5); // bucket [0,1) center 0.5 -> timeline 100.5
        assert!((samples[0].amplitude - 0.1).abs() < 1e-6);
    }

    #[test]
    fn clip_amplitude_samples_excludes_buckets_outside_the_clips_trim_range() {
        let peaks = vec![(-0.1, 0.1); 20];
        // Clip only shows source [5, 10).
        let clip = test_clip(0.0, 5.0, 10.0, 1.0);

        let samples = clip_amplitude_samples(&peaks, 20.0, &clip);

        assert_eq!(samples.len(), 5);
    }

    #[test]
    fn clip_amplitude_samples_is_empty_for_non_positive_speed_factor() {
        let peaks = vec![(-0.1, 0.1); 10];
        let clip = test_clip(0.0, 0.0, 10.0, 0.0);
        assert!(clip_amplitude_samples(&peaks, 10.0, &clip).is_empty());
    }

    fn sample(at_secs: f64, amplitude: f32) -> TimelineAmplitudeSample {
        TimelineAmplitudeSample { at_secs, amplitude }
    }

    #[test]
    fn detects_a_candidate_where_both_channels_spike_together() {
        let game = vec![
            sample(0.1, 0.1),
            sample(1.1, 0.9), // spike
            sample(2.1, 0.9), // spike
            sample(3.1, 0.1),
        ];
        let mic = vec![
            sample(0.2, 0.1),
            sample(1.2, 0.8), // spike
            sample(2.2, 0.8), // spike
            sample(3.2, 0.1),
        ];

        let candidates = detect_highlight_candidates(
            &game,
            &mic,
            DEFAULT_HIGHLIGHT_THRESHOLD_LINEAR,
            1.0,
            DEFAULT_HIGHLIGHT_MIN_DURATION_SECS,
        );

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].start_secs, 1.0);
        assert_eq!(candidates[0].end_secs, 3.0);
        assert!(
            (candidates[0].score - 0.8).abs() < 1e-6,
            "weaker of the two channels"
        );
    }

    #[test]
    fn does_not_flag_a_spike_on_only_one_channel() {
        let game = vec![sample(1.1, 0.9)]; // spike, but mic stays quiet
        let mic = vec![sample(1.2, 0.1)];

        let candidates = detect_highlight_candidates(
            &game,
            &mic,
            DEFAULT_HIGHLIGHT_THRESHOLD_LINEAR,
            1.0,
            DEFAULT_HIGHLIGHT_MIN_DURATION_SECS,
        );

        assert!(candidates.is_empty());
    }

    #[test]
    fn drops_a_candidate_shorter_than_the_minimum_duration() {
        let game = vec![sample(1.1, 0.9)];
        let mic = vec![sample(1.2, 0.9)];

        let candidates =
            detect_highlight_candidates(&game, &mic, DEFAULT_HIGHLIGHT_THRESHOLD_LINEAR, 1.0, 5.0);

        assert!(candidates.is_empty());
    }

    #[test]
    fn empty_channel_yields_no_candidates() {
        let mic = vec![sample(1.0, 0.9)];
        assert!(detect_highlight_candidates(&[], &mic, 0.5, 1.0, 1.0).is_empty());
        assert!(detect_highlight_candidates(&mic, &[], 0.5, 1.0, 1.0).is_empty());
    }

    #[test]
    fn ignores_non_finite_samples_and_refuses_an_unbounded_time_grid() {
        let valid_game = vec![sample(10.1, 0.9)];
        let valid_mic = vec![sample(10.2, 0.9)];
        assert_eq!(
            detect_highlight_candidates(
                &[sample(f64::NAN, 1.0)],
                &valid_mic,
                DEFAULT_HIGHLIGHT_THRESHOLD_LINEAR,
                1.0,
                1.0,
            ),
            Vec::new()
        );
        assert_eq!(
            detect_highlight_candidates(
                &valid_game,
                &[sample(1.0e12, 0.9)],
                DEFAULT_HIGHLIGHT_THRESHOLD_LINEAR,
                0.5,
                1.0,
            ),
            Vec::new()
        );
    }

    #[test]
    fn correlation_grid_is_anchored_near_the_samples_not_timeline_zero() {
        let game = vec![sample(10_000.1, 0.9), sample(10_001.1, 0.9)];
        let mic = vec![sample(10_000.2, 0.9), sample(10_001.2, 0.9)];
        let candidates = detect_highlight_candidates(&game, &mic, 0.5, 1.0, 1.0);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].start_secs, 10_000.0);
        assert_eq!(candidates[0].end_secs, 10_002.0);
    }

    fn test_event(source_secs: f64, confidence: f32) -> crate::gameplay_events::GameplayEvent {
        crate::gameplay_events::GameplayEvent {
            kind: crate::gameplay_events::GameplayEventKind::Kill,
            source_timestamp_secs: source_secs,
            confidence,
            pre_roll_secs: None,
            post_roll_secs: None,
        }
    }

    #[test]
    fn event_highlight_candidates_windows_each_event_by_the_default_roll() {
        let events = vec![test_event(100.0, 0.7)];
        let candidates = event_highlight_candidates(&events, |t| t, 5.0, 2.0);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].start_secs, 95.0);
        assert_eq!(candidates[0].end_secs, 102.0);
        assert_eq!(candidates[0].score, 0.7);
    }

    #[test]
    fn event_highlight_candidates_uses_the_events_own_roll_override() {
        let mut event = test_event(100.0, 1.0);
        event.pre_roll_secs = Some(1.0);
        event.post_roll_secs = Some(0.5);
        let candidates = event_highlight_candidates(&[event], |t| t, 5.0, 2.0);
        assert_eq!(candidates[0].start_secs, 99.0);
        assert_eq!(candidates[0].end_secs, 100.5);
    }

    #[test]
    fn event_highlight_candidates_clamps_the_window_start_to_zero() {
        let events = vec![test_event(1.0, 1.0)];
        let candidates = event_highlight_candidates(&events, |t| t, 5.0, 2.0);
        assert_eq!(candidates[0].start_secs, 0.0);
    }

    #[test]
    fn event_highlight_candidates_maps_through_the_timeline_offset() {
        let events = vec![test_event(10.0, 1.0)];
        let candidates = event_highlight_candidates(&events, |t| t + 50.0, 1.0, 1.0);
        assert_eq!(candidates[0].start_secs, 59.0);
        assert_eq!(candidates[0].end_secs, 61.0);
    }

    #[test]
    fn event_highlight_candidates_merges_overlapping_windows_keeping_the_higher_score() {
        let events = vec![test_event(100.0, 0.4), test_event(102.0, 0.9)];
        let candidates = event_highlight_candidates(&events, |t| t, 3.0, 3.0);
        assert_eq!(candidates.len(), 1, "the two 3s-wide windows overlap");
        assert_eq!(candidates[0].score, 0.9);
    }

    #[test]
    fn event_highlight_candidates_is_empty_for_no_events() {
        assert!(event_highlight_candidates(&[], |t| t, 1.0, 1.0).is_empty());
    }

    #[test]
    fn combine_keeps_non_overlapping_windows_from_both_sources_unchanged() {
        let audio = vec![HighlightCandidate {
            start_secs: 0.0,
            end_secs: 2.0,
            score: 0.6,
        }];
        let event = vec![HighlightCandidate {
            start_secs: 10.0,
            end_secs: 12.0,
            score: 0.5,
        }];
        let combined = combine_highlight_candidates(&audio, &event);
        assert_eq!(combined.len(), 2);
        assert_eq!(combined[0].score, 0.6);
        assert_eq!(combined[1].score, 0.5);
    }

    #[test]
    fn combine_boosts_an_overlapping_window_to_maximum_confidence() {
        let audio = vec![HighlightCandidate {
            start_secs: 0.0,
            end_secs: 5.0,
            score: 0.6,
        }];
        let event = vec![HighlightCandidate {
            start_secs: 3.0,
            end_secs: 8.0,
            score: 0.5,
        }];
        let combined = combine_highlight_candidates(&audio, &event);
        assert_eq!(combined.len(), 1, "overlapping windows merge into one");
        assert_eq!(combined[0].start_secs, 0.0);
        assert_eq!(combined[0].end_secs, 8.0);
        assert_eq!(combined[0].score, 1.0);
    }

    #[test]
    fn combine_is_empty_when_both_inputs_are_empty() {
        assert!(combine_highlight_candidates(&[], &[]).is_empty());
    }

    #[test]
    fn combine_returns_audio_only_candidates_when_no_events_exist() {
        let audio = vec![HighlightCandidate {
            start_secs: 0.0,
            end_secs: 2.0,
            score: 0.8,
        }];
        assert_eq!(combine_highlight_candidates(&audio, &[]), audio);
    }
}
