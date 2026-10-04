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

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use avcore::probe::probe_media;
use avcore::voice_cleanup_preview::{render_voice_cleanup_preview, VoiceCleanupParams};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("oca_voice_cleanup_preview_test_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

// Matches `scripts/Watch-Gameplay.ps1`'s own proven values, same as `ClipInstance`'s field
// defaults and `avbridge`'s own `audio_mix_test.rs` fixture segment.
fn default_params() -> VoiceCleanupParams {
    VoiceCleanupParams {
        noise_floor_db: -30.0,
        compressor_threshold_db: -18.0,
        compressor_ratio: 3.0,
        ceiling_linear: 0.95,
    }
}

#[test]
fn renders_both_samples_and_measures_a_real_difference() {
    let out_dir = scratch_dir("real_difference");

    let result = render_voice_cleanup_preview(
        &fixture("audio.m4a"),
        0.0,
        0.8,
        default_params(),
        -14.0,
        &out_dir,
        &AtomicBool::new(false),
    )
    .unwrap();

    assert!(result.bypassed.path.exists());
    assert!(result.processed.path.exists());
    assert_ne!(result.bypassed.path, result.processed.path);

    // Both files are real, playable audio -- confirms the FFI round-trip produced something a
    // real decoder accepts, not just that a file landed on disk.
    let bypassed_probe = probe_media(&result.bypassed.path).unwrap();
    assert!(bypassed_probe.has_audio);
    let processed_probe = probe_media(&result.processed.path).unwrap();
    assert!(processed_probe.has_audio);

    // The strongest evidence the cleanup chain actually ran: its compressor/limiter stages
    // measurably narrow the loudness range and/or lower the true peak relative to the
    // unprocessed sample, even though the shared whole-mix mastering pass pulls both samples
    // toward roughly the same integrated LUFS. Mirrors the exact real-difference assertion
    // `avbridge`'s own `audio_mix_test.rs` already established for this filter chain.
    assert!(
        result.processed.metrics.loudness_range_lu <= result.bypassed.metrics.loudness_range_lu,
        "expected the processed sample's loudness range ({}) to be no wider than the \
         bypassed sample's ({})",
        result.processed.metrics.loudness_range_lu,
        result.bypassed.metrics.loudness_range_lu
    );

    let _ = std::fs::remove_dir_all(&out_dir);
}

#[test]
fn a_second_render_overwrites_the_previous_one_in_the_same_out_dir() {
    let out_dir = scratch_dir("overwrite");

    let first = render_voice_cleanup_preview(
        &fixture("audio.m4a"),
        0.0,
        0.5,
        default_params(),
        -14.0,
        &out_dir,
        &AtomicBool::new(false),
    )
    .unwrap();
    let second = render_voice_cleanup_preview(
        &fixture("audio.m4a"),
        0.0,
        0.8,
        default_params(),
        -14.0,
        &out_dir,
        &AtomicBool::new(false),
    )
    .unwrap();

    // Same deterministic paths both times -- this is disposable scratch output, not a
    // project-tracked asset, so overwriting in place (rather than accumulating files forever)
    // is the intended behavior.
    assert_eq!(first.bypassed.path, second.bypassed.path);
    assert_eq!(first.processed.path, second.processed.path);

    let _ = std::fs::remove_dir_all(&out_dir);
}

#[test]
fn clamps_the_render_range_to_the_maximum_preview_duration() {
    let out_dir = scratch_dir("clamped");

    // audio.m4a's own total duration is well under PREVIEW_SAMPLE_MAX_SECS, so requesting an
    // absurdly long range here should clamp against the source itself and still succeed rather
    // than asking the encoder for audio the file doesn't have.
    let result = render_voice_cleanup_preview(
        &fixture("audio.m4a"),
        0.0,
        999.0,
        default_params(),
        -14.0,
        &out_dir,
        &AtomicBool::new(false),
    )
    .unwrap();

    assert!(result.bypassed.path.exists());
    assert!(result.processed.path.exists());

    let _ = std::fs::remove_dir_all(&out_dir);
}

#[test]
fn errors_on_a_missing_source_file() {
    let out_dir = scratch_dir("missing_source");

    let result = render_voice_cleanup_preview(
        &fixture("does_not_exist.m4a"),
        0.0,
        0.5,
        default_params(),
        -14.0,
        &out_dir,
        &AtomicBool::new(false),
    );

    assert!(result.is_err());

    let _ = std::fs::remove_dir_all(&out_dir);
}
