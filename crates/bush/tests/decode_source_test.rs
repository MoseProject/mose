use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use bush::{DecodeSource, DecodeSourceError};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../avbridge/tests/fixtures/video.mp4")
}

#[test]
fn decodes_fixture_into_bush_frame_contract() {
    let mut source = DecodeSource::open(&fixture()).unwrap();
    let info = source.info();
    let frame = source.next_frame(&AtomicBool::new(false)).unwrap().unwrap();

    assert_eq!((frame.width(), frame.height()), (info.width, info.height));
    assert_eq!(frame.stride_bytes(), frame.width() as usize * 4);
    assert!(frame.timestamp_secs().is_finite());
}

#[test]
fn seek_and_cancellation_do_not_publish_stale_frame() {
    let mut source = DecodeSource::open(&fixture()).unwrap();
    source.seek(0.5).unwrap();

    let cancelled = AtomicBool::new(true);
    assert!(source.next_frame(&cancelled).unwrap().is_none());
}

#[test]
fn invalid_seek_remains_an_explicit_decode_error() {
    let mut source = DecodeSource::open(&fixture()).unwrap();

    assert!(matches!(
        source.seek(f64::NAN),
        Err(DecodeSourceError::Decode(_))
    ));
}

#[test]
fn frame_at_returns_the_frame_presented_at_the_target() {
    let mut source = DecodeSource::open(&fixture()).unwrap();
    let fps = source.info().fps.expect("fixture has a frame rate");
    let cancel = AtomicBool::new(false);
    let target = 0.5;

    let frame = source.frame_at(target, &cancel).unwrap().unwrap();

    assert!(frame.timestamp_secs() <= target + 1e-6);
    assert!(
        target - frame.timestamp_secs() < 1.5 / fps,
        "frame at {} is more than one frame behind {target}",
        frame.timestamp_secs()
    );
}

#[test]
fn frame_at_is_stable_across_non_linear_seeks() {
    let mut source = DecodeSource::open(&fixture()).unwrap();
    let cancel = AtomicBool::new(false);

    let first = source.frame_at(0.8, &cancel).unwrap().unwrap();
    let _ = source.frame_at(0.1, &cancel).unwrap().unwrap();
    let again = source.frame_at(0.8, &cancel).unwrap().unwrap();

    assert_eq!(first, again);
}

#[test]
fn frame_at_before_stream_start_returns_the_first_frame() {
    let mut source = DecodeSource::open(&fixture()).unwrap();
    let frame = source
        .frame_at(0.0, &AtomicBool::new(false))
        .unwrap()
        .unwrap();
    assert!(frame.timestamp_secs() >= 0.0);
}

#[test]
fn frame_at_honours_cancellation() {
    let mut source = DecodeSource::open(&fixture()).unwrap();
    assert!(source
        .frame_at(0.5, &AtomicBool::new(true))
        .unwrap()
        .is_none());
}
