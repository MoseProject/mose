//! The preview strip against the export mix: the same settings must change a tone by about the
//! same amount in both, so what is heard while editing is what the export writes.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use avbridge::{mix_audio_timeline, AudioMixOutcome, AudioSegment, AudioStrip};
use bush::{Strip, StripState};

const RATE: u32 = 48_000;
const SECONDS: f32 = 0.6;

fn sine_wav(frequency: f32) -> PathBuf {
    let path = std::env::temp_dir().join(format!("bush_strip_sine_{frequency}.wav"));
    let frames = (SECONDS * RATE as f32) as u32;
    let mut data = Vec::new();
    for n in 0..frames {
        let value = (0.25
            * (2.0 * std::f32::consts::PI * frequency * n as f32 / RATE as f32).sin()
            * 32767.0) as i16;
        data.extend_from_slice(&value.to_le_bytes());
    }
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&RATE.to_le_bytes());
    wav.extend_from_slice(&(RATE * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
    wav.extend_from_slice(&data);
    std::fs::write(&path, wav).unwrap();
    path
}

fn strip_for_export(strip: Strip) -> AudioStrip {
    AudioStrip {
        eq_low_db: strip.eq_low_db,
        eq_mid_db: strip.eq_mid_db,
        eq_high_db: strip.eq_high_db,
        highpass_hz: strip.highpass_hz,
        lowpass_hz: strip.lowpass_hz,
        comp_threshold_db: strip.comp_threshold_db,
        comp_ratio: strip.comp_ratio,
    }
}

fn rms_db(samples: &[f32]) -> f32 {
    let power = samples.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / samples.len() as f64;
    10.0 * (power.max(1e-12)).log10() as f32
}

/// The level change in dB the preview strip makes to a sine, over the whole tone.
fn preview_change_db(frequency: f32, strip: Strip) -> f32 {
    let frames = (SECONDS * RATE as f32) as usize;
    let mut stereo = Vec::with_capacity(frames * 2);
    for n in 0..frames {
        let value = 0.25 * (2.0 * std::f32::consts::PI * frequency * n as f32 / RATE as f32).sin();
        stereo.extend_from_slice(&[value, value]);
    }
    let before = rms_db(&stereo);
    let mut state = StripState::new(RATE, 2);
    // Block by block, as playback does.
    for block in stereo.chunks_mut(1024 * 2) {
        state.process(block, strip);
    }
    rms_db(&stereo) - before
}

/// The same change measured through the export mix, decoded back.
fn export_change_db(frequency: f32, strip: Strip) -> f32 {
    let level = |strip: AudioStrip| {
        let out = std::env::temp_dir().join(format!(
            "bush_strip_{frequency}_{}.m4a",
            if strip == AudioStrip::default() {
                "flat"
            } else {
                "set"
            }
        ));
        let _ = std::fs::remove_file(&out);
        let mut segment = AudioSegment::plain(sine_wav(frequency), 0.0, f64::from(SECONDS), 0.0);
        segment.strip = strip;
        let outcome = mix_audio_timeline(
            &[segment],
            f64::from(SECONDS),
            &out,
            f32::NAN,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(outcome, AudioMixOutcome::Completed);
        let (mut session, _) = avbridge::AudioDecodeSession::open(&out, RATE, 2).unwrap();
        let cancel = AtomicBool::new(false);
        let mut all = Vec::new();
        while let Some(block) = session.next_block(&cancel).unwrap() {
            all.extend_from_slice(&block.samples);
        }
        let _ = std::fs::remove_file(&out);
        rms_db(&all)
    };
    level(strip_for_export(strip)) - level(AudioStrip::default())
}

#[test]
fn a_neutral_strip_leaves_the_samples_untouched() {
    let original: Vec<f32> = (0..2048).map(|n| (n as f32 * 0.01).sin() * 0.3).collect();
    let mut samples = original.clone();
    StripState::new(RATE, 2).process(&mut samples, Strip::default());
    assert_eq!(samples, original);
}

#[test]
fn each_control_changes_its_own_band_in_the_preview() {
    let change = |f, strip| preview_change_db(f, strip);
    let low = Strip {
        eq_low_db: 9.0,
        ..Strip::default()
    };
    assert!(change(50.0, low) > 5.0 && change(10_000.0, low).abs() < 1.0);
    let mid = Strip {
        eq_mid_db: -9.0,
        ..Strip::default()
    };
    assert!(change(1000.0, mid) < -6.0 && change(50.0, mid).abs() < 1.5);
    let high = Strip {
        eq_high_db: 9.0,
        ..Strip::default()
    };
    assert!(change(10_000.0, high) > 6.0 && change(50.0, high).abs() < 1.0);
    let hp = Strip {
        highpass_hz: 400.0,
        ..Strip::default()
    };
    assert!(change(50.0, hp) < -9.0 && change(1000.0, hp).abs() < 1.5);
    let lp = Strip {
        lowpass_hz: 2000.0,
        ..Strip::default()
    };
    assert!(change(10_000.0, lp) < -9.0 && change(50.0, lp).abs() < 1.5);
}

#[test]
fn a_compressor_only_reduces_what_is_over_its_threshold() {
    let tone = |threshold| Strip {
        comp_threshold_db: threshold,
        comp_ratio: 4.0,
        ..Strip::default()
    };
    // The tone sits near -15 dB: a threshold far above it does nothing, one below it reduces.
    assert!(preview_change_db(1000.0, tone(-3.0)).abs() < 0.3);
    assert!(preview_change_db(1000.0, tone(-30.0)) < -3.0);
}

#[test]
fn block_edges_are_inaudible() {
    // The same signal in one block or in many gives the same result, so playback has no clicks.
    let frames = 8192;
    let signal: Vec<f32> = (0..frames * 2)
        .map(|n| ((n / 2) as f32 * 0.05).sin() * 0.3)
        .collect();
    let strip = Strip {
        eq_low_db: 6.0,
        highpass_hz: 100.0,
        comp_ratio: 3.0,
        ..Strip::default()
    };
    let mut whole = signal.clone();
    StripState::new(RATE, 2).process(&mut whole, strip);
    let mut chunked = signal;
    let mut state = StripState::new(RATE, 2);
    for block in chunked.chunks_mut(500 * 2) {
        state.process(block, strip);
    }
    for (a, b) in whole.iter().zip(&chunked) {
        assert!((a - b).abs() < 1e-5);
    }
}

/// The preview and the export agree on how much each setting changes a tone.
#[test]
fn the_preview_strip_matches_the_export_mix() {
    let cases: [(f32, Strip); 8] = [
        (
            50.0,
            Strip {
                eq_low_db: 9.0,
                ..Strip::default()
            },
        ),
        (
            1000.0,
            Strip {
                eq_mid_db: -9.0,
                ..Strip::default()
            },
        ),
        (
            10_000.0,
            Strip {
                eq_high_db: 9.0,
                ..Strip::default()
            },
        ),
        (
            150.0,
            Strip {
                eq_low_db: -6.0,
                ..Strip::default()
            },
        ),
        (
            200.0,
            Strip {
                highpass_hz: 400.0,
                ..Strip::default()
            },
        ),
        (
            4000.0,
            Strip {
                lowpass_hz: 2000.0,
                ..Strip::default()
            },
        ),
        (
            1000.0,
            Strip {
                comp_threshold_db: -30.0,
                comp_ratio: 4.0,
                ..Strip::default()
            },
        ),
        (
            1000.0,
            Strip {
                comp_threshold_db: -24.0,
                comp_ratio: 2.0,
                ..Strip::default()
            },
        ),
    ];
    for (frequency, strip) in cases {
        let preview = preview_change_db(frequency, strip);
        let export = export_change_db(frequency, strip);
        eprintln!("{frequency} Hz {strip:?}: preview {preview:.2} dB, export {export:.2} dB");
        assert!(
            (preview - export).abs() < 1.5,
            "{frequency} Hz {strip:?}: preview {preview:.2} dB, export {export:.2} dB"
        );
    }
}
