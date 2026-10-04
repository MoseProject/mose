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

#[test]
fn sequential_targets_decode_forward_and_match_a_fresh_seek() {
    let mut sequential = DecodeSource::open(&fixture()).unwrap();
    let cancel = AtomicBool::new(false);
    let fps = sequential.info().fps.unwrap();

    for step in 0..12 {
        let target = 0.2 + f64::from(step) / fps;
        let forward = sequential.frame_at(target, &cancel).unwrap().unwrap();
        let seeked = {
            let mut one_shot = DecodeSource::open(&fixture()).unwrap();
            one_shot.frame_at(target, &cancel).unwrap().unwrap()
        };
        assert_eq!(forward, seeked, "frame mismatch at step {step}");
    }
}

#[test]
fn repeated_target_returns_the_same_shared_frame() {
    let mut source = DecodeSource::open(&fixture()).unwrap();
    let cancel = AtomicBool::new(false);
    let first = source.frame_at(0.4, &cancel).unwrap().unwrap();
    let second = source.frame_at(0.4, &cancel).unwrap().unwrap();
    assert!(std::sync::Arc::ptr_eq(&first, &second));
}

#[test]
fn a_target_past_the_end_presents_the_last_frame() {
    let mut source = DecodeSource::open(&fixture()).unwrap();
    let cancel = AtomicBool::new(false);
    let frame = source.frame_at(10_000.0, &cancel).unwrap().unwrap();
    assert!(frame.timestamp_secs() < 10_000.0);
    // The stream is exhausted, so asking again keeps presenting that same last frame.
    let again = source.frame_at(10_000.0, &cancel).unwrap().unwrap();
    assert_eq!(frame.timestamp_secs(), again.timestamp_secs());
}

#[test]
fn an_invalid_native_frame_is_reported_as_invalid_frame() {
    let invalid =
        bush::VideoFrame::new(bush::PixelFormat::Rgba8, 2, 2, 1, 0.0, vec![0; 16]).unwrap_err();
    assert!(matches!(
        DecodeSourceError::from(invalid),
        DecodeSourceError::InvalidFrame(_)
    ));
}

#[test]
fn a_target_further_than_the_frame_cap_is_a_search_limit_error() {
    let mut source = DecodeSource::open(&fixture()).unwrap();
    let cancel = AtomicBool::new(false);
    // The target is several frames past the first one, so a cap of one frame cannot reach it.
    let error = source.frame_at_within(0.6, &cancel, 1).unwrap_err();
    assert!(matches!(
        error,
        DecodeSourceError::SeekSearchLimit { limit: 1 }
    ));
    // The same request within the default cap succeeds.
    assert!(source.frame_at(0.6, &cancel).unwrap().is_some());
}
