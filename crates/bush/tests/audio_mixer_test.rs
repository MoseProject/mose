use bush::{AudioBlock, AudioMixer, MixError, MixInput};

fn block(timestamp_secs: f64, samples: Vec<f32>) -> AudioBlock {
    AudioBlock::new(10, 1, timestamp_secs, samples).unwrap()
}

#[test]
fn mixes_overlapping_gain_controlled_audio() {
    let first = block(0.0, vec![0.5, 0.5]);
    let second = block(0.1, vec![0.5, -0.5]);
    let mixer = AudioMixer::new(10, 1, 10).unwrap();

    let output = mixer
        .mix(&[
            MixInput {
                block: &first,
                gain: 1.0,
                muted: false,
            },
            MixInput {
                block: &second,
                gain: 0.5,
                muted: false,
            },
        ])
        .unwrap();

    assert_eq!(output.timestamp_secs(), 0.0);
    assert_eq!(output.samples(), &[0.5, 0.75, -0.25]);
}

#[test]
fn mute_skips_contributor_and_limiter_bounds_output() {
    let loud = block(0.0, vec![0.8]);
    let muted = block(0.0, vec![0.8]);
    let mixer = AudioMixer::new(10, 1, 10).unwrap();

    let output = mixer
        .mix(&[
            MixInput {
                block: &loud,
                gain: 2.0,
                muted: false,
            },
            MixInput {
                block: &muted,
                gain: 1.0,
                muted: true,
            },
        ])
        .unwrap();

    assert_eq!(output.samples(), &[1.0]);
}

#[test]
fn rejects_mismatched_or_non_aligned_input() {
    let mono = block(0.0, vec![0.0]);
    let wrong_rate = AudioBlock::new(11, 1, 0.0, vec![0.0]).unwrap();
    let non_aligned = block(0.05, vec![0.0]);
    let mixer = AudioMixer::new(10, 1, 10).unwrap();

    assert_eq!(
        mixer.mix(&[MixInput {
            block: &wrong_rate,
            gain: 1.0,
            muted: false
        }]),
        Err(MixError::SampleRateMismatch {
            expected: 10,
            actual: 11
        })
    );
    assert!(matches!(
        mixer.mix(&[MixInput { block: &mono, gain: f32::NAN, muted: false }]),
        Err(MixError::InvalidGain(value)) if value.is_nan()
    ));
    assert_eq!(
        mixer.mix(&[
            MixInput {
                block: &mono,
                gain: 1.0,
                muted: false
            },
            MixInput {
                block: &non_aligned,
                gain: 1.0,
                muted: false
            },
        ]),
        Err(MixError::NonFrameAlignedTimestamp(0.05))
    );
}

#[test]
fn enforces_output_frame_budget() {
    let input = block(0.0, vec![0.0, 0.0, 0.0]);
    let mixer = AudioMixer::new(10, 1, 2).unwrap();

    assert_eq!(
        mixer.mix(&[MixInput {
            block: &input,
            gain: 1.0,
            muted: false
        }]),
        Err(MixError::OutputExceedsBudget {
            requested_frames: 3,
            budget_frames: 2
        })
    );
}
