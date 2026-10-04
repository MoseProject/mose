use bush::TimeStretcher;

const RATE: f64 = 44_100.0;

fn tone(frequency: f64, frames: usize, channels: usize) -> Vec<f32> {
    let mut samples = Vec::with_capacity(frames * channels);
    for frame in 0..frames {
        let value =
            (2.0 * std::f64::consts::PI * frequency * frame as f64 / RATE).sin() as f32 * 0.5;
        for _ in 0..channels {
            samples.push(value);
        }
    }
    samples
}

/// Reader over an in-memory source, zero padded outside it.
fn reader(source: &[f32], channels: usize) -> impl FnMut(i64, usize) -> Option<Vec<f32>> + '_ {
    move |start, frames| {
        let mut out = vec![0.0; frames * channels];
        for frame in 0..frames {
            let at = start + frame as i64;
            if at >= 0 && (at as usize) < source.len() / channels {
                for channel in 0..channels {
                    out[frame * channels + channel] = source[at as usize * channels + channel];
                }
            }
        }
        Some(out)
    }
}

/// Frequency estimate from positive-going zero crossings.
fn estimate_frequency(samples: &[f32]) -> f64 {
    let mut crossings = Vec::new();
    for index in 1..samples.len() {
        if samples[index - 1] < 0.0 && samples[index] >= 0.0 {
            let fraction = -samples[index - 1] / (samples[index] - samples[index - 1]);
            crossings.push(index as f64 - 1.0 + f64::from(fraction));
        }
    }
    assert!(crossings.len() > 10, "too few zero crossings");
    (crossings.len() - 1) as f64 * RATE / (crossings[crossings.len() - 1] - crossings[0])
}

fn rms(samples: &[f32]) -> f64 {
    (samples.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / samples.len() as f64).sqrt()
}

#[test]
fn rejects_unsupported_speeds_and_channels() {
    for speed in [0.0, 0.2, 4.5, f64::NAN, f64::INFINITY, -1.0] {
        assert!(TimeStretcher::new(speed, 1).is_none(), "{speed}");
    }
    assert!(TimeStretcher::new(1.0, 0).is_none());
    assert!(TimeStretcher::new(0.25, 2).is_some());
    assert!(TimeStretcher::new(4.0, 2).is_some());
}

#[test]
fn speed_changes_duration_but_not_pitch() {
    let source = tone(440.0, 44_100 * 4, 1);
    for speed in [0.5, 0.75, 1.5, 2.0, 3.0] {
        let mut stretcher = TimeStretcher::new(speed, 1).unwrap();
        stretcher.reset(0.0);
        let mut read = reader(&source, 1);
        // Output frames that consume about two seconds of source.
        let frames = (2.0 * RATE / speed) as usize;
        let output = stretcher.render(frames, &mut read).unwrap();

        assert_eq!(output.len(), frames);
        // Skip the first window (ramp-up) and last (needs more source) for the estimate.
        let steady = &output[2_048..frames - 2_048];
        let frequency = estimate_frequency(steady);
        assert!(
            (frequency - 440.0).abs() < 4.0,
            "speed {speed}: pitch {frequency} Hz"
        );
        // Energy is preserved: Hann windows at 50 % overlap sum to unity gain.
        let level = rms(steady);
        assert!(
            (level - 0.5 / 2.0_f64.sqrt()).abs() < 0.03,
            "speed {speed}: rms {level}"
        );
    }
}

#[test]
fn unit_speed_is_nearly_transparent() {
    let source = tone(330.0, 44_100, 1);
    let mut stretcher = TimeStretcher::new(1.0, 1).unwrap();
    stretcher.reset(1_000.0);
    let mut read = reader(&source, 1);
    let output = stretcher.render(8_192, &mut read).unwrap();
    // After the ramp-up the stretched signal equals the source from the start position.
    let mut worst = 0.0_f32;
    for frame in 1_024..8_192 {
        worst = worst.max((output[frame] - source[1_000 + frame]).abs());
    }
    assert!(worst < 0.02, "worst deviation {worst}");
}

#[test]
fn consecutive_blocks_equal_one_large_block() {
    let source = tone(523.0, 44_100 * 3, 2);
    let mut whole = TimeStretcher::new(1.7, 2).unwrap();
    whole.reset(0.0);
    let single = whole.render(20_000, &mut reader(&source, 2)).unwrap();

    let mut chunked = TimeStretcher::new(1.7, 2).unwrap();
    chunked.reset(0.0);
    let mut read = reader(&source, 2);
    let mut joined = Vec::new();
    for size in [1, 700, 4_000, 123, 9_999, 5_177] {
        joined.extend(chunked.render(size, &mut read).unwrap());
    }
    assert_eq!(joined.len(), single.len());
    assert_eq!(
        joined, single,
        "block boundaries must not change the output"
    );
}

#[test]
fn reset_restarts_at_the_new_position_and_cancellation_propagates() {
    let source = tone(440.0, 44_100 * 2, 1);
    let mut stretcher = TimeStretcher::new(2.0, 1).unwrap();
    stretcher.reset(0.0);
    stretcher.render(3_000, &mut reader(&source, 1)).unwrap();

    stretcher.reset(30_000.0);
    let mut positions = Vec::new();
    let mut recording = |start: i64, frames: usize| {
        positions.push(start);
        reader(&source, 1)(start, frames)
    };
    stretcher.render(600, &mut recording).unwrap();
    assert!(
        positions[0] >= 30_000 - 256 - 1 && positions[0] <= 30_000,
        "{positions:?}"
    );

    let mut cancelled = |_: i64, _: usize| None;
    stretcher.reset(0.0);
    assert!(stretcher.render(100, &mut cancelled).is_none());
}

#[test]
fn silence_stays_silent_and_stereo_channels_stay_in_phase() {
    let mut stretcher = TimeStretcher::new(1.3, 2).unwrap();
    stretcher.reset(0.0);
    let silent = vec![0.0_f32; 2 * 20_000];
    let output = stretcher.render(5_000, &mut reader(&silent, 2)).unwrap();
    assert!(output.iter().all(|sample| *sample == 0.0));

    let source = tone(300.0, 44_100, 2);
    let mut stretcher = TimeStretcher::new(0.8, 2).unwrap();
    stretcher.reset(0.0);
    let output = stretcher.render(10_000, &mut reader(&source, 2)).unwrap();
    for pair in output.chunks_exact(2) {
        assert_eq!(pair[0], pair[1]);
    }
}

#[test]
fn the_speed_can_change_inside_the_supported_range_only() {
    let mut stretcher = bush::TimeStretcher::new(1.0, 1).unwrap();
    assert_eq!(stretcher.speed(), 1.0);
    assert!(stretcher.set_speed(1.5));
    assert_eq!(stretcher.speed(), 1.5);
    for rejected in [
        f64::NAN,
        f64::INFINITY,
        bush::MAX_SPEED + 1.0,
        bush::MIN_SPEED / 2.0,
    ] {
        assert!(!stretcher.set_speed(rejected), "{rejected}");
        assert_eq!(stretcher.speed(), 1.5);
    }
}
