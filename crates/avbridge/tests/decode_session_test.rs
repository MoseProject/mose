use std::path::Path;
use std::sync::atomic::AtomicBool;

use avbridge::{DecodeError, DecodeSession};

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
