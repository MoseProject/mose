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

//! P2 item 10, "Multicam editing" (`spec/ROADMAP.md`): sync-by-audio-waveform for a group of
//! sources recorded simultaneously (e.g. game capture, webcam, mic), so the editor knows the
//! time offset between each source's own timeline and a shared reference. Reuses
//! `avbridge::extract_pcm_16k_mono` (already used by [`crate::transcribe`] for whisper.cpp) as
//! the raw-signal input rather than adding new `avbridge`/C decode code -- see the roadmap
//! item's own scoping note for why this is a pure-Rust `core` module, not new avfilter wiring.
//!
//! Correlating two full 16kHz PCM buffers directly is too slow for a several-minute recording
//! (a lag search over even a modest window is millions of sample-pair multiplications per
//! candidate lag). [`amplitude_envelope`] first collapses each buffer into a coarse RMS-per-
//! window envelope -- the same "throw away everything but loudness over time" idea
//! [`crate::waveform`]'s peaks use for display, just fixed-width instead of fixed-bucket-count
//! so two differently-sampled sources land on directly comparable envelope indices.
//! [`best_lag_windows`] then searches a bounded lag range on that much smaller signal.

/// Downsamples raw PCM into an amplitude envelope: RMS magnitude of each non-overlapping
/// `window_secs`-wide window, in `[0.0, 1.0]`-ish range (raw PCM is normalized audio, RMS of a
/// full-scale window tops out at 1.0). Empty if `pcm` or `window_secs`/`sample_rate_hz` aren't
/// positive.
pub fn amplitude_envelope(pcm: &[f32], sample_rate_hz: f64, window_secs: f64) -> Vec<f32> {
    if pcm.is_empty() || sample_rate_hz <= 0.0 || window_secs <= 0.0 {
        return Vec::new();
    }
    let window_len = ((sample_rate_hz * window_secs).round() as usize).max(1);
    pcm.chunks(window_len)
        .map(|chunk| {
            let sum_sq: f64 = chunk.iter().map(|&s| (s as f64) * (s as f64)).sum();
            ((sum_sq / chunk.len() as f64).sqrt()) as f32
        })
        .collect()
}

/// Finds the lag (in envelope-window units) that best aligns `other` to `reference`: the lag
/// maximizing the normalized dot product of the two envelopes' overlapping region. `other`'s
/// window `i` is compared against `reference`'s window `i + lag` -- so a positive result means
/// the content `other` shows at its own window `i` is what `reference` shows *later*, at window
/// `i + lag`, i.e. `other`'s recording started earlier than `reference`'s (equivalently,
/// `other`'s timeline needs to be delayed by `lag` windows to line its content up with
/// `reference`'s). Searches only `-max_lag_windows..=max_lag_windows`. `None` if either envelope
/// is empty or `max_lag_windows` is negative.
pub fn best_lag_windows(reference: &[f32], other: &[f32], max_lag_windows: i64) -> Option<i64> {
    if reference.is_empty() || other.is_empty() || max_lag_windows < 0 {
        return None;
    }

    let mut best_lag = 0i64;
    let mut best_score = f64::MIN;
    for lag in -max_lag_windows..=max_lag_windows {
        let mut sum = 0.0f64;
        let mut count = 0usize;
        for (i, &o) in other.iter().enumerate() {
            let ref_index = i as i64 + lag;
            if ref_index < 0 {
                continue;
            }
            let Some(&r) = reference.get(ref_index as usize) else {
                continue;
            };
            sum += (o as f64) * (r as f64);
            count += 1;
        }
        // Normalize by overlap length so a lag with almost no overlap (near the edges of the
        // search range) can't win purely by having fewer, coincidentally-aligned terms.
        if count == 0 {
            continue;
        }
        let score = sum / count as f64;
        if score > best_score {
            best_score = score;
            best_lag = lag;
        }
    }
    Some(best_lag)
}

/// Default envelope window width -- coarse enough to keep [`best_lag_windows`]'s search fast
/// over a multi-minute recording, fine enough that the resulting sync offset is accurate well
/// within a video frame's worth of time (at 30fps, one frame is ~33ms).
pub const DEFAULT_ENVELOPE_WINDOW_SECS: f64 = 0.02;
/// Default maximum offset [`compute_sync_offset_secs`] searches for -- generous for sources
/// started a little apart by hand (pressing "record" on a capture card, a webcam app, and a
/// separate mic recorder are rarely started in the same second), well short of the correlation
/// search becoming slow.
pub const DEFAULT_MAX_SYNC_OFFSET_SECS: f64 = 30.0;

/// High-level sync: PCM -> envelope -> best lag -> offset in seconds. `offset_secs` is defined
/// so that `other_track_time = reference_track_time + offset_secs` for the same real-world
/// moment -- i.e. to find what `other` was showing when `reference` was at time `t`, look up
/// `other` at `t + offset_secs`. `None` if either PCM buffer is empty or `sample_rate_hz`/
/// `window_secs`/`max_offset_secs` aren't positive.
pub fn compute_sync_offset_secs(
    reference_pcm: &[f32],
    other_pcm: &[f32],
    sample_rate_hz: f64,
    window_secs: f64,
    max_offset_secs: f64,
) -> Option<f64> {
    if max_offset_secs <= 0.0 {
        return None;
    }
    let reference_env = amplitude_envelope(reference_pcm, sample_rate_hz, window_secs);
    let other_env = amplitude_envelope(other_pcm, sample_rate_hz, window_secs);
    let max_lag_windows = (max_offset_secs / window_secs).round() as i64;
    let lag = best_lag_windows(&reference_env, &other_env, max_lag_windows)?;
    Some(-(lag as f64) * window_secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A short burst of full-scale noise-like signal (alternating +1/-1, so RMS is exactly 1.0
    /// per window regardless of window alignment) surrounded by silence -- a synthetic "loud
    /// moment" a real cross-correlation needs to locate the lag of.
    fn burst_signal(total_len: usize, burst_start: usize, burst_len: usize) -> Vec<f32> {
        let mut signal = vec![0.0f32; total_len];
        for i in burst_start..(burst_start + burst_len).min(total_len) {
            // Sign alternates by position *within* the burst, not absolute index -- keyed on
            // absolute index, an odd-length shift would flip the whole waveform's sign and
            // break correlation between an unshifted and shifted copy of the "same" burst.
            signal[i] = if (i - burst_start).is_multiple_of(2) {
                1.0
            } else {
                -1.0
            };
        }
        signal
    }

    #[test]
    fn amplitude_envelope_is_empty_for_empty_input_or_non_positive_params() {
        assert!(amplitude_envelope(&[], 100.0, 0.1).is_empty());
        assert!(amplitude_envelope(&[0.5, 0.5], 0.0, 0.1).is_empty());
        assert!(amplitude_envelope(&[0.5, 0.5], 100.0, 0.0).is_empty());
    }

    #[test]
    fn amplitude_envelope_reports_rms_per_window() {
        // 4 samples/window, one window of all 1.0 (RMS 1.0), one of all 0.0 (RMS 0.0).
        let pcm = [1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0];
        let envelope = amplitude_envelope(&pcm, 40.0, 0.1); // 40Hz * 0.1s = 4 samples/window

        assert_eq!(envelope.len(), 2);
        assert!((envelope[0] - 1.0).abs() < 1e-6);
        assert!((envelope[1] - 0.0).abs() < 1e-6);
    }

    #[test]
    fn best_lag_windows_finds_a_burst_shifted_later() {
        // Reference has its burst at window 20; other has the *same* burst at window 15 --
        // other's content at window i matches reference's content at window i + 5.
        let reference = burst_signal(40, 20, 3);
        let other = burst_signal(40, 15, 3);

        let lag = best_lag_windows(&reference, &other, 10).unwrap();

        assert_eq!(lag, 5);
    }

    #[test]
    fn best_lag_windows_finds_a_burst_shifted_earlier() {
        let reference = burst_signal(40, 10, 3);
        let other = burst_signal(40, 18, 3);

        let lag = best_lag_windows(&reference, &other, 10).unwrap();

        assert_eq!(lag, -8);
    }

    #[test]
    fn best_lag_windows_is_none_for_empty_input_or_negative_max_lag() {
        assert!(best_lag_windows(&[], &[1.0], 5).is_none());
        assert!(best_lag_windows(&[1.0], &[], 5).is_none());
        assert!(best_lag_windows(&[1.0], &[1.0], -1).is_none());
    }

    #[test]
    fn compute_sync_offset_secs_matches_a_known_delay() {
        // 16kHz-equivalent low rate for a fast test: reference's burst at 2.0s, other's burst at
        // 2.5s -- other started 0.5s *later* than reference, so to see what reference shows at
        // time t, look up other at t + 0.5.
        let sample_rate_hz = 1000.0;
        let window_secs = 0.05; // 50 samples/window
        let total_secs = 6.0;
        let total_len = (sample_rate_hz * total_secs) as usize;
        let burst_len = (sample_rate_hz * 0.1) as usize;
        let reference = burst_signal(total_len, (sample_rate_hz * 2.0) as usize, burst_len);
        let other = burst_signal(total_len, (sample_rate_hz * 2.5) as usize, burst_len);

        let offset =
            compute_sync_offset_secs(&reference, &other, sample_rate_hz, window_secs, 5.0).unwrap();

        assert!(
            (offset - 0.5).abs() < window_secs,
            "expected ~0.5s offset, got {offset}"
        );
    }

    #[test]
    fn compute_sync_offset_secs_is_none_for_non_positive_max_offset() {
        assert!(compute_sync_offset_secs(&[1.0], &[1.0], 100.0, 0.1, 0.0).is_none());
    }
}
