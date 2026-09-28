use bush::{PixelFormat, VideoFrame, VideoFrameError};

fn rgba_frame(stride_bytes: usize, pixels: Vec<u8>) -> Result<VideoFrame, VideoFrameError> {
    VideoFrame::new(PixelFormat::Rgba8, 2, 2, stride_bytes, 1.25, pixels)
}

#[test]
fn valid_rgba_frame_retains_layout_and_timestamp() {
    let frame = rgba_frame(8, vec![0; 16]).unwrap();

    assert_eq!(frame.format(), PixelFormat::Rgba8);
    assert_eq!(
        (frame.width(), frame.height(), frame.stride_bytes()),
        (2, 2, 8)
    );
    assert_eq!(frame.timestamp_secs(), 1.25);
    assert_eq!(frame.retained_bytes(), 16);
}

#[test]
fn padded_rows_are_valid_when_all_rows_are_present() {
    let frame = rgba_frame(12, vec![0; 24]).unwrap();

    assert_eq!(frame.stride_bytes(), 12);
}

#[test]
fn frame_rejects_invalid_dimensions_timestamp_and_stride() {
    assert!(matches!(
        VideoFrame::new(PixelFormat::Rgba8, 0, 2, 8, 0.0, vec![0; 16]),
        Err(VideoFrameError::EmptyDimensions { .. })
    ));
    assert!(matches!(
        VideoFrame::new(PixelFormat::Rgba8, 2, 2, 8, f64::NAN, vec![0; 16]),
        Err(VideoFrameError::InvalidTimestamp(value)) if value.is_nan()
    ));
    assert!(matches!(
        VideoFrame::new(PixelFormat::Rgba8, 2, 2, 8, -0.1, vec![0; 16]),
        Err(VideoFrameError::InvalidTimestamp(-0.1))
    ));
    assert!(matches!(
        rgba_frame(7, vec![0; 16]),
        Err(VideoFrameError::StrideTooSmall {
            minimum_bytes: 8,
            ..
        })
    ));
}

#[test]
fn frame_rejects_short_buffer() {
    assert_eq!(
        rgba_frame(8, vec![0; 15]),
        Err(VideoFrameError::BufferTooShort {
            actual_bytes: 15,
            required_bytes: 16,
        })
    );
}
