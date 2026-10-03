use std::path::Path;
use std::sync::atomic::AtomicBool;

use avbridge::{DecodeError, DecodePolicy, DecodeSession};

const VIDEO: &str = "tests/fixtures/video.mp4";

#[test]
fn decodes_packed_rgba_frames_from_fixture() {
    let (mut session, info) = DecodeSession::open(Path::new(VIDEO)).unwrap();
    let frame = session.next_rgba(&AtomicBool::new(false)).unwrap().unwrap();

    assert_eq!((frame.width, frame.height), (info.width, info.height));
    assert_eq!(frame.stride_bytes, frame.width as usize * 4);
    assert_eq!(
        frame.pixels.len(),
        frame.stride_bytes * frame.height as usize
    );
    assert!(frame.timestamp_secs.is_finite());
}

#[test]
fn seek_flushes_previous_decode_state() {
    let (mut session, _) = DecodeSession::open(Path::new(VIDEO)).unwrap();
    let first = session.next_rgba(&AtomicBool::new(false)).unwrap().unwrap();

    session.seek(0.5).unwrap();
    let after_seek = session.next_rgba(&AtomicBool::new(false)).unwrap().unwrap();

    assert!(after_seek.timestamp_secs >= first.timestamp_secs);
}

#[test]
fn rejects_invalid_seek_before_native_call() {
    let (mut session, _) = DecodeSession::open(Path::new(VIDEO)).unwrap();

    assert!(matches!(
        session.seek(f64::NAN),
        Err(DecodeError::InvalidSeekTimestamp)
    ));
    assert!(matches!(
        session.seek(-1.0),
        Err(DecodeError::InvalidSeekTimestamp)
    ));
}

#[test]
fn cancellation_returns_without_publishing_a_frame() {
    let (mut session, _) = DecodeSession::open(Path::new(VIDEO)).unwrap();
    let cancel = AtomicBool::new(true);

    assert!(session.next_rgba(&cancel).unwrap().is_none());
}

const H264: &str = "tests/fixtures/video_h264.mp4";

fn mean_abs_difference(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len());
    a.iter()
        .zip(b)
        .map(|(x, y)| f64::from(x.abs_diff(*y)))
        .sum::<f64>()
        / a.len() as f64
}

#[test]
fn software_policy_never_reports_hardware() {
    let (_, info) =
        DecodeSession::open_with_policy(Path::new(H264), DecodePolicy::Software).unwrap();
    assert!(!info.hardware);
}

#[test]
fn hardware_policy_opens_and_decodes_every_fixture_codec() {
    // Whether MPEG-4 part 2 runs on a device depends on the GPU; either path must decode.
    for path in [VIDEO, H264] {
        let (mut session, info) =
            DecodeSession::open_with_policy(Path::new(path), DecodePolicy::HardwareWithFallback)
                .unwrap();
        eprintln!("{path}: hardware = {}", info.hardware);
        let frame = session.next_rgba(&AtomicBool::new(false)).unwrap().unwrap();
        assert_eq!((frame.width, frame.height), (info.width, info.height));
    }
}

#[test]
fn hardware_decode_matches_software_output_when_a_device_is_available() {
    let cancel = AtomicBool::new(false);
    let (mut software, _) =
        DecodeSession::open_with_policy(Path::new(H264), DecodePolicy::Software).unwrap();
    let (mut hardware, info) =
        DecodeSession::open_with_policy(Path::new(H264), DecodePolicy::HardwareWithFallback)
            .unwrap();
    eprintln!("hardware decode path taken: {}", info.hardware);

    // Compare the frame presented after seeking mid-stream, hardware vs software.
    software.seek(0.4).unwrap();
    hardware.seek(0.4).unwrap();
    let expected = software.next_rgba(&cancel).unwrap().unwrap();
    let actual = hardware.next_rgba(&cancel).unwrap().unwrap();

    assert_eq!(
        (actual.width, actual.height),
        (expected.width, expected.height)
    );
    assert_eq!(actual.timestamp_secs, expected.timestamp_secs);
    let difference = mean_abs_difference(&actual.pixels, &expected.pixels);
    assert!(
        difference < 3.0,
        "hardware vs software mean abs difference {difference}"
    );
}

#[test]
fn hardware_session_decodes_the_whole_stream_and_tears_down() {
    let cancel = AtomicBool::new(false);
    let (mut session, _) =
        DecodeSession::open_with_policy(Path::new(H264), DecodePolicy::HardwareWithFallback)
            .unwrap();
    let mut frames = 0;
    while session.next_rgba(&cancel).unwrap().is_some() {
        frames += 1;
    }
    assert_eq!(frames, 25);
}
