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

//! D6 (`spec/architecture/differentiators.md`, one-click shorts pack): turns one detected
//! highlight window into its own standalone, exportable mini-[`Timeline`] — the piece that lets
//! a single `HighlightCandidate`/`MarkerKind::Highlight` position become a queueable export job
//! on its own, independent of the rest of the sequence.

use crate::timeline::{Timeline, Track};

const EPSILON: f64 = 1e-6;

/// Extracts the `[window_start_secs, window_end_secs)` slice of `timeline` into a new,
/// standalone [`Timeline`] rebased so the window's start lands at `0.0`.
///
/// Video/Audio track clips ([`crate::timeline::ClipInstance`]) straddling either boundary are
/// split precisely (via [`Track::split_clip_at`], consuming ids from `next_clip_id`) so the
/// short's visual/audio content isn't truncated mid-clip — the same split primitive
/// `App::split_at_playhead` already uses. Text/Shape overlays are simpler but less safe to
/// split: a [`crate::timeline::TextClip`]'s word-highlight timing is relative to the clip's own
/// `start_secs`, so trimming it mid-clip without also re-deriving which words still fit would
/// silently desync captions from audio. Only overlay clips *entirely* within the window are
/// kept; one straddling either edge is dropped rather than risking that.
///
/// Returns an empty-tracks [`Timeline`] if `window_end_secs <= window_start_secs`.
pub fn extract_timeline_window(
    timeline: &Timeline,
    window_start_secs: f64,
    window_end_secs: f64,
    next_clip_id: &mut u64,
) -> Timeline {
    if window_end_secs <= window_start_secs {
        return Timeline {
            tracks: Vec::new(),
            playhead_secs: 0.0,
            markers: Vec::new(),
            multicam_groups: Vec::new(),
        };
    }

    let tracks = timeline
        .tracks
        .iter()
        .map(|track| window_track(track, window_start_secs, window_end_secs, next_clip_id))
        .collect();

    Timeline {
        tracks,
        playhead_secs: 0.0,
        markers: Vec::new(),
        multicam_groups: Vec::new(),
    }
}

fn window_track(
    track: &Track,
    window_start_secs: f64,
    window_end_secs: f64,
    next_clip_id: &mut u64,
) -> Track {
    let mut windowed = track.clone();

    if windowed.split_clip_at(window_start_secs, *next_clip_id) {
        *next_clip_id += 1;
    }
    if windowed.split_clip_at(window_end_secs, *next_clip_id) {
        *next_clip_id += 1;
    }
    windowed.clips.retain(|c| {
        let end = c.start_secs + c.duration_secs();
        c.start_secs >= window_start_secs - EPSILON && end <= window_end_secs + EPSILON
    });
    for clip in &mut windowed.clips {
        clip.start_secs -= window_start_secs;
    }

    windowed.text_clips.retain(|t| {
        let end = t.start_secs + t.duration_secs;
        t.start_secs >= window_start_secs - EPSILON && end <= window_end_secs + EPSILON
    });
    for text_clip in &mut windowed.text_clips {
        text_clip.start_secs -= window_start_secs;
    }

    windowed.shape_clips.retain(|s| {
        let end = s.start_secs + s.duration_secs;
        s.start_secs >= window_start_secs - EPSILON && end <= window_end_secs + EPSILON
    });
    for shape_clip in &mut windowed.shape_clips {
        shape_clip.start_secs -= window_start_secs;
    }

    windowed
}

#[cfg(test)]
#[path = "timeline_window/timeline_window_test.rs"]
mod tests;
