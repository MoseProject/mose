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

fn mean_rgb(frame: &bush::VideoFrame) -> [f64; 3] {
    let mut sum = [0u64; 3];
    for pixel in frame.pixels().chunks_exact(4) {
        for channel in 0..3 {
            sum[channel] += u64::from(pixel[channel]);
        }
    }
    let count = (frame.pixels().len() / 4) as f64;
    sum.map(|total| total as f64 / count)
}

#[test]
fn a_limit_shrinks_the_decoded_frame_and_keeps_its_content() {
    let cancel = AtomicBool::new(false);
    let mut source = DecodeSource::open(&fixture()).unwrap();
    let info = source.info();
    let full = source.frame_at(0.4, &cancel).unwrap().unwrap();
    assert_eq!((full.width(), full.height()), (info.width, info.height));

    // A box about a third of the size: the aspect ratio stays, the average colour stays.
    source.set_output_limit(Some((info.width / 3, info.height / 3)));
    let small = source.frame_at(0.4, &cancel).unwrap().unwrap();
    assert!(small.width() <= info.width / 3 + 1 && small.width() >= info.width / 3);
    assert!(small.height() <= info.height / 3 + 1 && small.height() >= info.height / 3);
    assert_eq!(
        small.pixels().len(),
        small.width() as usize * small.height() as usize * 4
    );
    for (a, b) in mean_rgb(&full).iter().zip(mean_rgb(&small)) {
        assert!((a - b).abs() < 3.0, "mean colour moved from {a} to {b}");
    }

    // A box that is not smaller, or none, goes back to the decoded size, and the frame held for
    // a repeated target is not the small one.
    source.set_output_limit(Some((info.width * 2, info.height * 2)));
    let again = source.frame_at(0.4, &cancel).unwrap().unwrap();
    assert_eq!((again.width(), again.height()), (info.width, info.height));
    assert_eq!(again.pixels(), full.pixels());
}

#[test]
fn decoded_video_is_opaque_whatever_its_size() {
    let cancel = AtomicBool::new(false);
    let mut source = DecodeSource::open(&fixture()).unwrap();
    let info = source.info();
    assert!(source.frame_at(0.4, &cancel).unwrap().unwrap().is_opaque());
    source.set_output_limit(Some((info.width / 2, info.height / 2)));
    assert!(source.frame_at(0.4, &cancel).unwrap().unwrap().is_opaque());
}

/// swscale left the alpha of the last pixels of each row unwritten when the width is not a
/// multiple of 8 -- the default 480p proxy is 854 wide -- which showed as a transparent strip.
#[test]
fn a_width_that_is_not_a_multiple_of_eight_is_opaque_to_the_last_pixel() {
    let Some(ffmpeg) = ["ffmpeg"].into_iter().find(|name| {
        std::process::Command::new(name)
            .arg("-version")
            .output()
            .is_ok()
    }) else {
        return;
    };
    let dir = std::env::temp_dir().join(format!("bush-odd-width-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cancel = AtomicBool::new(false);
    for width in [642, 850, 854, 1366] {
        let path = dir.join(format!("w{width}.mp4"));
        let made = std::process::Command::new(ffmpeg)
            .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
            .arg(format!("testsrc2=size={width}x120:rate=30:duration=0.5"))
            .args(["-c:v", "mpeg4", "-pix_fmt", "yuv420p"])
            .arg(&path)
            .status()
            .unwrap();
        assert!(made.success());
        let mut source = DecodeSource::open(&path).unwrap();
        let frame = source.frame_at(0.2, &cancel).unwrap().unwrap();
        assert_eq!(frame.width(), width);
        assert!(frame.is_opaque(), "{width} px wide has see-through pixels");
        // Also when the frame is reduced on the way out.
        source.set_output_limit(Some((width / 2, 60)));
        let reduced = source.frame_at(0.2, &cancel).unwrap().unwrap();
        assert!(reduced.is_opaque(), "{width} px wide, reduced");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
