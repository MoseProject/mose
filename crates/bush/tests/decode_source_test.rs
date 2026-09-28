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
