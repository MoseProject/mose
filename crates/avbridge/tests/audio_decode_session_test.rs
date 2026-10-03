use std::path::Path;
use std::sync::atomic::AtomicBool;

use avbridge::{AudioDecodeError, AudioDecodeSession};

const AUDIO: &str = "tests/fixtures/audio.m4a";

fn drain(session: &mut AudioDecodeSession) -> Vec<avbridge::DecodedAudioBlock> {
    let cancel = AtomicBool::new(false);
    let mut blocks = Vec::new();
    while let Some(block) = session.next_block(&cancel).unwrap() {
        blocks.push(block);
    }
    blocks
}

#[test]
fn decodes_the_whole_fixture_at_its_native_rate() {
    let (mut session, info) = AudioDecodeSession::open(Path::new(AUDIO), 44_100, 1).unwrap();
    assert_eq!((info.source_sample_rate, info.source_channels), (44_100, 1));

    let blocks = drain(&mut session);
    let frames: usize = blocks.iter().map(|b| b.frame_count).sum();

    // A 1.0s fixture: encoder priming/padding may shift the count by one AAC frame.
    assert!((frames as i64 - 44_100).abs() <= 2_048, "frames = {frames}");
    for block in &blocks {
        assert_eq!(block.samples.len(), block.frame_count);
        assert!(block.samples.iter().all(|s| s.is_finite()));
    }
    assert!(blocks
        .windows(2)
        .all(|pair| pair[1].timestamp_secs > pair[0].timestamp_secs));
}

#[test]
fn resamples_and_upmixes_to_the_requested_format() {
    let (mut session, info) = AudioDecodeSession::open(Path::new(AUDIO), 48_000, 2).unwrap();
    assert_eq!((info.sample_rate, info.channels), (48_000, 2));

    let blocks = drain(&mut session);
    let frames: usize = blocks.iter().map(|b| b.frame_count).sum();

    assert!((frames as i64 - 48_000).abs() <= 2_300, "frames = {frames}");
    for block in &blocks {
        assert_eq!(block.samples.len(), block.frame_count * 2);
        // Mono -> stereo duplicates the channel.
        for pair in block.samples.chunks_exact(2) {
            assert_eq!(pair[0], pair[1]);
        }
    }
}

#[test]
fn seek_restarts_decoding_near_the_target() {
    let (mut session, _) = AudioDecodeSession::open(Path::new(AUDIO), 44_100, 1).unwrap();
    let all = drain(&mut session);

    session.seek(0.5).unwrap();
    let after = drain(&mut session);

    let first = &after[0];
    assert!(
        first.timestamp_secs <= 0.5 && first.timestamp_secs > 0.5 - 0.1,
        "first block at {}",
        first.timestamp_secs
    );
    assert!(after.len() < all.len());
    let last = after.last().unwrap();
    let all_last = all.last().unwrap();
    assert_eq!(last.timestamp_secs, all_last.timestamp_secs);
    // AAC overlap state differs slightly after a seek; the signal must still match closely.
    let max_error = last
        .samples
        .iter()
        .zip(&all_last.samples)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0_f32, f32::max);
    assert_eq!(last.samples.len(), all_last.samples.len());
    assert!(
        max_error < 1e-3,
        "max sample error after seek = {max_error}"
    );
}

#[test]
fn cancellation_yields_no_block() {
    let (mut session, _) = AudioDecodeSession::open(Path::new(AUDIO), 44_100, 1).unwrap();
    assert!(session
        .next_block(&AtomicBool::new(true))
        .unwrap()
        .is_none());
}

#[test]
fn rejects_invalid_requests_and_unreadable_input() {
    assert!(matches!(
        AudioDecodeSession::open(Path::new(AUDIO), 100, 1),
        Err(AudioDecodeError::UnsupportedFormat { .. })
    ));
    assert!(matches!(
        AudioDecodeSession::open(Path::new(AUDIO), 44_100, 6),
        Err(AudioDecodeError::UnsupportedFormat { .. })
    ));
    assert!(AudioDecodeSession::open(Path::new("tests/fixtures/garbage.bin"), 44_100, 1).is_err());
    assert!(AudioDecodeSession::open(Path::new("tests/fixtures/missing.m4a"), 44_100, 1).is_err());

    let (mut session, _) = AudioDecodeSession::open(Path::new(AUDIO), 44_100, 1).unwrap();
    assert!(matches!(
        session.seek(f64::NAN),
        Err(AudioDecodeError::InvalidSeekTimestamp)
    ));
}
