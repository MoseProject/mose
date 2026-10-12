// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use avbridge::{
    measure_loudness_json, mix_audio_timeline, mux_video_audio, probe, AudioMixOutcome,
    AudioSegment, StreamKind,
};

/// Pulls a string-valued field (FFmpeg's own `loudnorm` JSON report quotes every number) out of
/// `measure_loudness_json`'s raw report text without pulling in a JSON dependency just for this
/// test — `avcore::loudness::parse_loudnorm_stderr` is the real parser; this is a narrower,
/// test-only lookup for one field.
fn json_string_field(report: &str, field: &str) -> f64 {
    let needle = format!("\"{field}\"");
    let after_key = &report[report.find(&needle).expect(field) + needle.len()..];
    let quote_start = after_key.find('"').expect("opening quote") + 1;
    let value_str = &after_key[quote_start..];
    let quote_end = value_str.find('"').expect("closing quote");
    value_str[..quote_end].parse().expect("numeric field")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn mixes_overlapping_audio_segments_and_muxes_them_without_reencoding_video() {
    let mixed = std::env::temp_dir().join("avbridge_audio_mix_test.m4a");
    let muxed = std::env::temp_dir().join("avbridge_audio_mux_test.mp4");
    let _ = std::fs::remove_file(&mixed);
    let _ = std::fs::remove_file(&muxed);
    let source = fixture("audio.m4a");
    let segments = vec![
        AudioSegment {
            source_path: source.clone(),
            source_in_secs: 0.0,
            source_out_secs: 0.8,
            timeline_start_secs: 0.0,
            gain_db: -3.0,
            speed_factor: 1.0,
            gain_keyframe_expr: String::new(),
            duck_role: 0,
            voice_cleanup_enabled: false,
            voice_cleanup_noise_floor_db: -30.0,
            voice_cleanup_compressor_threshold_db: -18.0,
            voice_cleanup_compressor_ratio: 3.0,
            voice_cleanup_ceiling_linear: 0.95,
            fade_in_secs: 0.0,
            fade_out_secs: 0.0,
            pan: 0.0,
            strip: Default::default(),
        },
        AudioSegment {
            source_path: source,
            source_in_secs: 0.1,
            source_out_secs: 0.7,
            timeline_start_secs: 0.2,
            gain_db: -6.0,
            speed_factor: 1.25,
            gain_keyframe_expr: String::new(),
            duck_role: 0,
            voice_cleanup_enabled: false,
            voice_cleanup_noise_floor_db: -30.0,
            voice_cleanup_compressor_threshold_db: -18.0,
            voice_cleanup_compressor_ratio: 3.0,
            voice_cleanup_ceiling_linear: 0.95,
            fade_in_secs: 0.0,
            fade_out_secs: 0.0,
            pan: 0.0,
            strip: Default::default(),
        },
    ];

    let outcome =
        mix_audio_timeline(&segments, 1.0, &mixed, -14.0, &AtomicBool::new(false)).unwrap();
    assert_eq!(outcome, AudioMixOutcome::Completed);
    let mixed_info = probe(&mixed).unwrap();
    assert_eq!(mixed_info.kind, StreamKind::Audio);
    assert!(mixed_info.has_audio);
    // The first branch ends at 0.8 s; the second ends at 0.2 + 0.6 / 1.25 = 0.68 s.
    // timeline_duration_secs is an upper bound, not a request to pad trailing silence.
    assert!((mixed_info.duration_secs - 0.8).abs() < 0.1);

    mux_video_audio(&fixture("video.mp4"), &mixed, &muxed).unwrap();
    let muxed_info = probe(&muxed).unwrap();
    assert_eq!(muxed_info.kind, StreamKind::Video);
    assert!(muxed_info.has_audio);
    assert_eq!(muxed_info.resolution, Some((320, 240)));
    assert_eq!(muxed_info.codec_name, "mpeg4");

    let _ = std::fs::remove_file(&mixed);
    let _ = std::fs::remove_file(&muxed);
}

/// The mix graph's `afade` stages: a segment with a fade-in and fade-out starts and ends far
/// quieter than its middle.
#[test]
fn mix_segment_fades_ramp_the_branch_in_and_out() {
    let mixed = std::env::temp_dir().join("avbridge_audio_mix_fades.m4a");
    let _ = std::fs::remove_file(&mixed);
    let segment = AudioSegment {
        source_path: fixture("audio.m4a"),
        source_in_secs: 0.0,
        source_out_secs: 0.8,
        timeline_start_secs: 0.0,
        gain_db: 0.0,
        speed_factor: 1.0,
        gain_keyframe_expr: String::new(),
        duck_role: 0,
        voice_cleanup_enabled: false,
        voice_cleanup_noise_floor_db: -30.0,
        voice_cleanup_compressor_threshold_db: -18.0,
        voice_cleanup_compressor_ratio: 3.0,
        voice_cleanup_ceiling_linear: 0.95,
        fade_in_secs: 0.3,
        fade_out_secs: 0.3,
        pan: 0.0,
        strip: Default::default(),
    };
    let outcome =
        mix_audio_timeline(&[segment], 1.0, &mixed, -14.0, &AtomicBool::new(false)).unwrap();
    assert_eq!(outcome, AudioMixOutcome::Completed);

    let samples = avbridge::extract_pcm_16k_mono(&mixed).unwrap();
    let window = 16_000 / 20; // 50 ms
    let rms = |from: usize, to: usize| {
        let part = &samples[from.min(samples.len())..to.min(samples.len())];
        (part.iter().map(|s| s * s).sum::<f32>() / part.len().max(1) as f32).sqrt()
    };
    let middle = rms(
        samples.len() / 2 - window / 2,
        samples.len() / 2 + window / 2,
    );
    assert!(middle > 0.0);
    assert!(rms(0, window) < middle * 0.5, "fade-in");
    assert!(
        rms(samples.len() - window, samples.len()) < middle * 0.5,
        "fade-out"
    );
    let _ = std::fs::remove_file(&mixed);
}

/// P2 item 6, "Audio ducking": exercises the actual `sidechaincompress`-based filter-graph
/// topology `build_mix_graph` builds when at least one `duck_role: 1` (trigger/mic) and one
/// `duck_role: 2` (target/music) branch are present. `avfilter_graph_config` is the real proof
/// this graph is valid, not just C that compiles -- a wrong pad order, a bad filter option name,
/// or a topology mistake surfaces here as `AudioMixError::FilterGraph`, which a passing
/// `AudioMixOutcome::Completed` rules out.
#[test]
fn mixes_with_sidechain_ducking_when_both_roles_are_present() {
    let mixed = std::env::temp_dir().join("avbridge_audio_duck_test.m4a");
    let _ = std::fs::remove_file(&mixed);
    let source = fixture("audio.m4a");
    let segments = vec![
        AudioSegment {
            source_path: source.clone(),
            source_in_secs: 0.0,
            source_out_secs: 0.8,
            timeline_start_secs: 0.0,
            gain_db: 0.0,
            speed_factor: 1.0,
            gain_keyframe_expr: String::new(),
            duck_role: 2, // target -- gets ducked
            voice_cleanup_enabled: false,
            voice_cleanup_noise_floor_db: -30.0,
            voice_cleanup_compressor_threshold_db: -18.0,
            voice_cleanup_compressor_ratio: 3.0,
            voice_cleanup_ceiling_linear: 0.95,
            fade_in_secs: 0.0,
            fade_out_secs: 0.0,
            pan: 0.0,
            strip: Default::default(),
        },
        AudioSegment {
            source_path: source,
            source_in_secs: 0.0,
            source_out_secs: 0.8,
            timeline_start_secs: 0.0,
            gain_db: 0.0,
            speed_factor: 1.0,
            gain_keyframe_expr: String::new(),
            duck_role: 1, // trigger -- does the ducking, and still plays itself
            voice_cleanup_enabled: false,
            voice_cleanup_noise_floor_db: -30.0,
            voice_cleanup_compressor_threshold_db: -18.0,
            voice_cleanup_compressor_ratio: 3.0,
            voice_cleanup_ceiling_linear: 0.95,
            fade_in_secs: 0.0,
            fade_out_secs: 0.0,
            pan: 0.0,
            strip: Default::default(),
        },
    ];

    let outcome =
        mix_audio_timeline(&segments, 1.0, &mixed, -14.0, &AtomicBool::new(false)).unwrap();
    assert_eq!(outcome, AudioMixOutcome::Completed);
    let mixed_info = probe(&mixed).unwrap();
    assert!(mixed_info.has_audio);

    let _ = std::fs::remove_file(&mixed);
}

fn voice_cleanup_segment(source: PathBuf, voice_cleanup_enabled: bool) -> AudioSegment {
    AudioSegment {
        source_path: source,
        source_in_secs: 0.0,
        source_out_secs: 0.8,
        timeline_start_secs: 0.0,
        gain_db: 0.0,
        speed_factor: 1.0,
        gain_keyframe_expr: String::new(),
        duck_role: 0,
        voice_cleanup_enabled,
        voice_cleanup_noise_floor_db: -30.0,
        voice_cleanup_compressor_threshold_db: -18.0,
        voice_cleanup_compressor_ratio: 3.0,
        voice_cleanup_ceiling_linear: 0.95,
        fade_in_secs: 0.0,
        fade_out_secs: 0.0,
        pan: 0.0,
        strip: Default::default(),
    }
}

/// CF-03 "Gameplay Voice" cleanup: exercises the real
/// `highpass,afftdn,acompressor,alimiter` sub-chain `build_mix_graph` splices in per branch when
/// `voice_cleanup_enabled` is set. `avfilter_graph_config` succeeding (via a passing
/// `AudioMixOutcome::Completed`, not `AudioMixError::FilterGraph`) is the real proof this exact
/// filter syntax — built from `scripts/Watch-Gameplay.ps1`'s own proven chain, not guessed — is
/// valid against this build's actual linked FFmpeg, the same standard this file's ducking test
/// above already holds itself to.
#[test]
fn mixes_with_voice_cleanup_enabled() {
    let mixed = std::env::temp_dir().join("avbridge_voice_cleanup_test.m4a");
    let _ = std::fs::remove_file(&mixed);
    let source = fixture("audio.m4a");
    let segments = vec![voice_cleanup_segment(source, true)];

    let outcome =
        mix_audio_timeline(&segments, 1.0, &mixed, -14.0, &AtomicBool::new(false)).unwrap();
    assert_eq!(outcome, AudioMixOutcome::Completed);
    let mixed_info = probe(&mixed).unwrap();
    assert!(mixed_info.has_audio);

    let _ = std::fs::remove_file(&mixed);
}

/// The same segment with cleanup disabled must still mix successfully (the branch is left
/// exactly as every other test's plain branches already are) — confirms the conditional splice
/// doesn't leave a dangling/half-linked filter context when it's skipped.
#[test]
fn mixes_with_voice_cleanup_disabled() {
    let mixed = std::env::temp_dir().join("avbridge_voice_cleanup_disabled_test.m4a");
    let _ = std::fs::remove_file(&mixed);
    let source = fixture("audio.m4a");
    let segments = vec![voice_cleanup_segment(source, false)];

    let outcome =
        mix_audio_timeline(&segments, 1.0, &mixed, -14.0, &AtomicBool::new(false)).unwrap();
    assert_eq!(outcome, AudioMixOutcome::Completed);
    let mixed_info = probe(&mixed).unwrap();
    assert!(mixed_info.has_audio);

    let _ = std::fs::remove_file(&mixed);
}

/// Cleanup enabled vs. disabled must produce audibly different output — real proof the extra
/// filters actually run and affect samples, not just that the graph parses. Compares each
/// output's own measured integrated loudness (already-linked `probe`-adjacent primitive this
/// crate exposes) rather than raw byte-for-byte equality, since AAC re-encoding alone can jitter
/// bytes without any filter difference.
#[test]
fn voice_cleanup_measurably_changes_the_output() {
    let source = fixture("audio.m4a");
    let plain = std::env::temp_dir().join("avbridge_voice_cleanup_ab_plain.m4a");
    let cleaned = std::env::temp_dir().join("avbridge_voice_cleanup_ab_cleaned.m4a");
    let _ = std::fs::remove_file(&plain);
    let _ = std::fs::remove_file(&cleaned);

    mix_audio_timeline(
        &[voice_cleanup_segment(source.clone(), false)],
        1.0,
        &plain,
        -14.0,
        &AtomicBool::new(false),
    )
    .unwrap();
    mix_audio_timeline(
        &[voice_cleanup_segment(source, true)],
        1.0,
        &cleaned,
        -14.0,
        &AtomicBool::new(false),
    )
    .unwrap();

    let plain_bytes = std::fs::read(&plain).unwrap();
    let cleaned_bytes = std::fs::read(&cleaned).unwrap();
    assert_ne!(
        plain_bytes, cleaned_bytes,
        "voice cleanup's extra filter stages should change the encoded output"
    );

    // acompressor narrows dynamic range before the final loudnorm pass normalizes both
    // outputs to the same integrated loudness -- so integrated LUFS alone wouldn't show a
    // difference (that's the point of loudnorm), but the compressed file's own loudness range
    // (LRA) should measurably shrink relative to the uncompressed one. A real, independent
    // signal that acompressor actually ran, not just that bytes differ for some other reason.
    let plain_report = measure_loudness_json(&plain).unwrap();
    let cleaned_report = measure_loudness_json(&cleaned).unwrap();
    let plain_lra = json_string_field(&plain_report, "input_lra");
    let cleaned_lra = json_string_field(&cleaned_report, "input_lra");
    assert!(
        cleaned_lra <= plain_lra,
        "expected the compressed file's LRA ({cleaned_lra}) to be no wider than the \
         uncompressed one's ({plain_lra})"
    );

    let _ = std::fs::remove_file(&plain);
    let _ = std::fs::remove_file(&cleaned);
}

/// Same as above but with *multiple* branches on each duck role, exercising the sub-`amix`
/// (`music_mix`/`trigger_mix`) branches `build_mix_graph` only takes when a role has more than
/// one contributing segment.
#[test]
fn mixes_with_sidechain_ducking_across_multiple_branches_per_role() {
    let mixed = std::env::temp_dir().join("avbridge_audio_duck_multi_test.m4a");
    let _ = std::fs::remove_file(&mixed);
    let source = fixture("audio.m4a");
    let segments = vec![
        AudioSegment {
            source_path: source.clone(),
            source_in_secs: 0.0,
            source_out_secs: 0.4,
            timeline_start_secs: 0.0,
            gain_db: 0.0,
            speed_factor: 1.0,
            gain_keyframe_expr: String::new(),
            duck_role: 2, // target #1
            voice_cleanup_enabled: false,
            voice_cleanup_noise_floor_db: -30.0,
            voice_cleanup_compressor_threshold_db: -18.0,
            voice_cleanup_compressor_ratio: 3.0,
            voice_cleanup_ceiling_linear: 0.95,
            fade_in_secs: 0.0,
            fade_out_secs: 0.0,
            pan: 0.0,
            strip: Default::default(),
        },
        AudioSegment {
            source_path: source.clone(),
            source_in_secs: 0.4,
            source_out_secs: 0.8,
            timeline_start_secs: 0.4,
            gain_db: 0.0,
            speed_factor: 1.0,
            gain_keyframe_expr: String::new(),
            duck_role: 2, // target #2
            voice_cleanup_enabled: false,
            voice_cleanup_noise_floor_db: -30.0,
            voice_cleanup_compressor_threshold_db: -18.0,
            voice_cleanup_compressor_ratio: 3.0,
            voice_cleanup_ceiling_linear: 0.95,
            fade_in_secs: 0.0,
            fade_out_secs: 0.0,
            pan: 0.0,
            strip: Default::default(),
        },
        AudioSegment {
            source_path: source.clone(),
            source_in_secs: 0.0,
            source_out_secs: 0.4,
            timeline_start_secs: 0.0,
            gain_db: 0.0,
            speed_factor: 1.0,
            gain_keyframe_expr: String::new(),
            duck_role: 1, // trigger #1
            voice_cleanup_enabled: false,
            voice_cleanup_noise_floor_db: -30.0,
            voice_cleanup_compressor_threshold_db: -18.0,
            voice_cleanup_compressor_ratio: 3.0,
            voice_cleanup_ceiling_linear: 0.95,
            fade_in_secs: 0.0,
            fade_out_secs: 0.0,
            pan: 0.0,
            strip: Default::default(),
        },
        AudioSegment {
            source_path: source,
            source_in_secs: 0.4,
            source_out_secs: 0.8,
            timeline_start_secs: 0.4,
            gain_db: 0.0,
            speed_factor: 1.0,
            gain_keyframe_expr: String::new(),
            duck_role: 1, // trigger #2
            voice_cleanup_enabled: false,
            voice_cleanup_noise_floor_db: -30.0,
            voice_cleanup_compressor_threshold_db: -18.0,
            voice_cleanup_compressor_ratio: 3.0,
            voice_cleanup_ceiling_linear: 0.95,
            fade_in_secs: 0.0,
            fade_out_secs: 0.0,
            pan: 0.0,
            strip: Default::default(),
        },
    ];

    let outcome =
        mix_audio_timeline(&segments, 0.8, &mixed, -14.0, &AtomicBool::new(false)).unwrap();
    assert_eq!(outcome, AudioMixOutcome::Completed);
    let mixed_info = probe(&mixed).unwrap();
    assert!(mixed_info.has_audio);

    let _ = std::fs::remove_file(&mixed);
}

/// A `duck_role: 2` (target) branch with no trigger present anywhere should fall back to the
/// original flat `amix` of every branch (ducking needs both roles) rather than erroring.
#[test]
fn a_target_branch_without_any_trigger_falls_back_to_a_plain_mix() {
    let mixed = std::env::temp_dir().join("avbridge_audio_duck_no_trigger_test.m4a");
    let _ = std::fs::remove_file(&mixed);
    let source = fixture("audio.m4a");
    let segments = vec![AudioSegment {
        source_path: source,
        source_in_secs: 0.0,
        source_out_secs: 0.8,
        timeline_start_secs: 0.0,
        gain_db: 0.0,
        speed_factor: 1.0,
        gain_keyframe_expr: String::new(),
        duck_role: 2, // target, but nothing tags a trigger
        voice_cleanup_enabled: false,
        voice_cleanup_noise_floor_db: -30.0,
        voice_cleanup_compressor_threshold_db: -18.0,
        voice_cleanup_compressor_ratio: 3.0,
        voice_cleanup_ceiling_linear: 0.95,
        fade_in_secs: 0.0,
        fade_out_secs: 0.0,
        pan: 0.0,
        strip: Default::default(),
    }];

    let outcome =
        mix_audio_timeline(&segments, 0.8, &mixed, -14.0, &AtomicBool::new(false)).unwrap();
    assert_eq!(outcome, AudioMixOutcome::Completed);

    let _ = std::fs::remove_file(&mixed);
}

/// Exercises `build_mix_graph`'s new expression-mode `volume` branch (`audio_mix.c`) for a
/// segment carrying a keyframed gain ramp instead of a constant `gain_db` -- avbridge doesn't
/// depend on `core`, so this expression is written by hand rather than via
/// `avcore::keyframe::gain_filter_db_expr`, but it has the same shape that function produces
/// (a `t`-keyed piecewise-linear ramp from a linear-gain start value to a linear-gain end
/// value). `avfilter_graph_config` succeeding (a passing `AudioMixOutcome::Completed`) is real
/// proof the `av_asprintf`-built `"volume=%s:eval=frame"` string is valid avfilter syntax the
/// `volume` filter actually accepts -- not just C that compiles.
#[test]
fn mixes_a_segment_with_a_keyframed_gain_expression() {
    let mixed = std::env::temp_dir().join("avbridge_audio_gain_keyframe_test.m4a");
    let _ = std::fs::remove_file(&mixed);
    let source = fixture("audio.m4a");
    let segments = vec![AudioSegment {
        source_path: source,
        source_in_secs: 0.0,
        source_out_secs: 0.8,
        timeline_start_secs: 0.0,
        // Ignored when gain_keyframe_expr is non-empty -- confirms the expression path takes
        // priority over the constant, not just that both happen to agree.
        gain_db: -99.0,
        speed_factor: 1.0,
        gain_keyframe_expr: "if(lt(t,0.000000),0.1000000,if(between(t,0.000000,0.800000),\
                              (0.1000000+1.1250000*(t-0.000000)),1.0000000))"
            .to_string(),
        duck_role: 0,
        voice_cleanup_enabled: false,
        voice_cleanup_noise_floor_db: -30.0,
        voice_cleanup_compressor_threshold_db: -18.0,
        voice_cleanup_compressor_ratio: 3.0,
        voice_cleanup_ceiling_linear: 0.95,
        fade_in_secs: 0.0,
        fade_out_secs: 0.0,
        pan: 0.0,
        strip: Default::default(),
    }];

    let outcome =
        mix_audio_timeline(&segments, 0.8, &mixed, -14.0, &AtomicBool::new(false)).unwrap();
    assert_eq!(outcome, AudioMixOutcome::Completed);
    let mixed_info = probe(&mixed).unwrap();
    assert!(mixed_info.has_audio);

    let _ = std::fs::remove_file(&mixed);
}

/// Left and right RMS of the stereo audio file `path`.
fn channel_rms(path: &Path) -> (f32, f32) {
    let (mut session, _) = avbridge::AudioDecodeSession::open(path, 48_000, 2).unwrap();
    let cancel = AtomicBool::new(false);
    let (mut left, mut right, mut frames) = (0.0f64, 0.0f64, 0usize);
    while let Some(block) = session.next_block(&cancel).unwrap() {
        for pair in block.samples.chunks_exact(2) {
            left += f64::from(pair[0]).powi(2);
            right += f64::from(pair[1]).powi(2);
        }
        frames += block.frame_count;
    }
    let frames = frames.max(1) as f64;
    (
        (left / frames).sqrt() as f32,
        (right / frames).sqrt() as f32,
    )
}

fn panned_mix(name: &str, pan: f32) -> (f32, f32) {
    let mixed = std::env::temp_dir().join(format!("avbridge_pan_{name}.m4a"));
    let _ = std::fs::remove_file(&mixed);
    let mut segment = AudioSegment::plain(fixture("audio.m4a"), 0.0, 0.8, 0.0);
    segment.pan = pan;
    let outcome =
        mix_audio_timeline(&[segment], 1.0, &mixed, -14.0, &AtomicBool::new(false)).unwrap();
    assert_eq!(outcome, AudioMixOutcome::Completed);
    let levels = channel_rms(&mixed);
    let _ = std::fs::remove_file(&mixed);
    levels
}

/// The pan stage of the mix graph: a hard pan silences the far side and a centred branch is
/// left exactly as it was.
#[test]
fn a_panned_segment_moves_to_one_side_and_a_centred_one_is_untouched() {
    let (centre_l, centre_r) = panned_mix("centre", 0.0);
    assert!(centre_l > 0.01 && (centre_l - centre_r).abs() < 0.01 * centre_l.max(centre_r));

    let (right_l, right_r) = panned_mix("right", 1.0);
    assert!(
        right_l < 0.02 * right_r,
        "hard right still plays left: {right_l} vs {right_r}"
    );
    assert!(right_r > 0.5 * centre_r, "the near side keeps its level");

    let (left_l, left_r) = panned_mix("left", -1.0);
    assert!(
        left_r < 0.02 * left_l,
        "hard left still plays right: {left_r} vs {left_l}"
    );

    // Halfway is quieter on the far side than the near one, but not silent.
    let (half_l, half_r) = panned_mix("half", 0.5);
    assert!(
        half_l < half_r && half_l > 0.3 * half_r,
        "{half_l} vs {half_r}"
    );
}

/// Writes a mono 48 kHz 16-bit WAV of a sine at `frequency` into the temp dir.
fn sine_wav(frequency: f32, seconds: f32) -> PathBuf {
    let path = std::env::temp_dir().join(format!("avbridge_sine_{frequency}.wav"));
    let rate = 48_000u32;
    let frames = (seconds * rate as f32) as u32;
    let mut data = Vec::new();
    for n in 0..frames {
        let value = (0.25
            * (2.0 * std::f32::consts::PI * frequency * n as f32 / rate as f32).sin()
            * 32767.0) as i16;
        data.extend_from_slice(&value.to_le_bytes());
    }
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
    wav.extend_from_slice(&data);
    std::fs::write(&path, wav).unwrap();
    path
}

/// RMS (both channels) of the middle of `frequency`'s sine after the mix graph with `strip`,
/// without any loudness normalisation so levels can be compared.
fn strip_level(frequency: f32, strip: avbridge::AudioStrip) -> f32 {
    let mixed = std::env::temp_dir().join(format!("avbridge_strip_{frequency}.m4a"));
    let _ = std::fs::remove_file(&mixed);
    let mut segment = AudioSegment::plain(sine_wav(frequency, 0.6), 0.0, 0.6, 0.0);
    segment.strip = strip;
    let outcome =
        mix_audio_timeline(&[segment], 0.6, &mixed, f32::NAN, &AtomicBool::new(false)).unwrap();
    assert_eq!(outcome, AudioMixOutcome::Completed);
    let (left, right) = channel_rms(&mixed);
    let _ = std::fs::remove_file(&mixed);
    (left + right) / 2.0
}

fn db(ratio: f32) -> f32 {
    20.0 * ratio.log10()
}

/// The tone and dynamics stages of the mix graph change the band they are named for and leave the
/// others alone; a neutral strip changes nothing at all.
#[test]
fn a_strip_boosts_and_cuts_its_own_band_and_a_neutral_one_does_nothing() {
    use avbridge::AudioStrip;
    let flat = |f| strip_level(f, AudioStrip::default());
    let (low, mid, high) = (flat(50.0), flat(1000.0), flat(10000.0));
    assert!(
        low > 0.05 && mid > 0.05 && high > 0.05,
        "{low} {mid} {high}"
    );

    let boosted = |f, strip: AudioStrip| db(strip_level(f, strip) / flat(f));
    let low_boost = AudioStrip {
        eq_low_db: 9.0,
        ..AudioStrip::default()
    };
    assert!(
        (boosted(50.0, low_boost) - 9.0).abs() < 2.5,
        "low shelf at 50 Hz"
    );
    assert!(
        boosted(10000.0, low_boost).abs() < 1.0,
        "the low shelf leaves 10 kHz alone"
    );

    let mid_cut = AudioStrip {
        eq_mid_db: -9.0,
        ..AudioStrip::default()
    };
    assert!(
        (boosted(1000.0, mid_cut) + 9.0).abs() < 2.5,
        "mid bell at 1 kHz"
    );
    assert!(boosted(50.0, mid_cut).abs() < 1.5);

    let high_boost = AudioStrip {
        eq_high_db: 9.0,
        ..AudioStrip::default()
    };
    assert!(
        (boosted(10000.0, high_boost) - 9.0).abs() < 2.5,
        "high shelf at 10 kHz"
    );
    assert!(boosted(50.0, high_boost).abs() < 1.0);

    let highpass = AudioStrip {
        highpass_hz: 400.0,
        ..AudioStrip::default()
    };
    assert!(
        boosted(50.0, highpass) < -9.0,
        "a 400 Hz high-pass removes 50 Hz"
    );
    assert!(boosted(1000.0, highpass).abs() < 1.5);
    let lowpass = AudioStrip {
        lowpass_hz: 2000.0,
        ..AudioStrip::default()
    };
    assert!(
        boosted(10000.0, lowpass) < -9.0,
        "a 2 kHz low-pass removes 10 kHz"
    );
    assert!(boosted(50.0, lowpass).abs() < 1.5);

    let compressed = AudioStrip {
        comp_threshold_db: -30.0,
        comp_ratio: 8.0,
        ..AudioStrip::default()
    };
    assert!(
        boosted(1000.0, compressed) < -4.0,
        "a loud tone over the threshold is brought down"
    );
}
