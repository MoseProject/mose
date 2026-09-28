use bush::{AudioBlock, AudioBlockError};

#[test]
fn valid_interleaved_pcm_retains_timing_and_size() {
    let block = AudioBlock::new(48_000, 2, 2.5, vec![0.25, -0.25, 0.5, -0.5]).unwrap();

    assert_eq!(block.sample_rate(), 48_000);
    assert_eq!(block.channels(), 2);
    assert_eq!(block.timestamp_secs(), 2.5);
    assert_eq!(block.frame_count(), 2);
    assert_eq!(block.retained_bytes(), 16);
}

#[test]
fn empty_audio_block_is_a_valid_no_op() {
    let block = AudioBlock::new(48_000, 2, 0.0, Vec::new()).unwrap();

    assert_eq!(block.frame_count(), 0);
}

#[test]
fn rejects_invalid_stream_shape_and_timestamps() {
    assert_eq!(
        AudioBlock::new(0, 2, 0.0, Vec::new()),
        Err(AudioBlockError::ZeroSampleRate)
    );
    assert_eq!(
        AudioBlock::new(48_000, 0, 0.0, Vec::new()),
        Err(AudioBlockError::ZeroChannels)
    );
    assert!(matches!(
        AudioBlock::new(48_000, 2, f64::NAN, Vec::new()),
        Err(AudioBlockError::InvalidTimestamp(value)) if value.is_nan()
    ));
    assert_eq!(
        AudioBlock::new(48_000, 2, 0.0, vec![0.0]),
        Err(AudioBlockError::PartialInterleavedFrame {
            sample_count: 1,
            channels: 2,
        })
    );
}

#[test]
fn rejects_non_finite_pcm_samples() {
    assert_eq!(
        AudioBlock::new(48_000, 1, 0.0, vec![f32::INFINITY]),
        Err(AudioBlockError::NonFiniteSample)
    );
}
