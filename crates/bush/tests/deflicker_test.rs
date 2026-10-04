use bush::{Deflicker, PixelFormat, VideoFrame};

fn flat(grey: u8, timestamp_secs: f64) -> VideoFrame {
    VideoFrame::new(
        PixelFormat::Rgba8,
        4,
        4,
        16,
        timestamp_secs,
        [grey, grey, grey, 255].repeat(16),
    )
    .unwrap()
}

fn grey_of(frame: &VideoFrame) -> f32 {
    frame.pixels()[0] as f32
}

fn spread(values: &[f32]) -> f32 {
    let max = values.iter().cloned().fold(f32::MIN, f32::max);
    let min = values.iter().cloned().fold(f32::MAX, f32::min);
    max - min
}

#[test]
fn rejects_invalid_smoothing() {
    for bad in [0.0, -0.1, 1.5, f32::NAN, f32::INFINITY] {
        assert!(Deflicker::new(bad).is_none(), "{bad}");
    }
    assert!(Deflicker::new(1.0).is_some());
}

#[test]
fn first_frame_passes_through_unchanged() {
    let mut deflicker = Deflicker::new(0.2).unwrap();
    let input = flat(100, 0.0);
    assert_eq!(deflicker.process(&input), input);
}

#[test]
fn alternating_brightness_is_damped() {
    let mut deflicker = Deflicker::new(0.1).unwrap();
    let inputs: Vec<f32> = (0..40)
        .map(|index| if index % 2 == 0 { 100.0 } else { 140.0 })
        .collect();
    let outputs: Vec<f32> = inputs
        .iter()
        .enumerate()
        .map(|(index, grey)| grey_of(&deflicker.process(&flat(*grey as u8, index as f64 * 0.04))))
        .collect();

    let late_input = spread(&inputs[20..]);
    let late_output = spread(&outputs[20..]);
    assert!(
        late_output < late_input * 0.5,
        "output spread {late_output} vs input spread {late_input}"
    );
}

#[test]
fn a_steady_picture_is_left_alone() {
    let mut deflicker = Deflicker::new(0.3).unwrap();
    for index in 0..10 {
        let input = flat(120, index as f64 * 0.04);
        assert_eq!(deflicker.process(&input), input);
    }
}

#[test]
fn repeating_the_same_frame_does_not_drift() {
    let mut deflicker = Deflicker::new(0.5).unwrap();
    deflicker.process(&flat(100, 0.0));
    let first = deflicker.process(&flat(160, 0.04));
    for _ in 0..20 {
        assert_eq!(deflicker.process(&flat(160, 0.04)), first);
    }
}

#[test]
fn jumps_and_seeks_restart_the_average() {
    let mut deflicker = Deflicker::new(0.1).unwrap();
    for index in 0..10 {
        deflicker.process(&flat(100, index as f64 * 0.04));
    }
    // A forward jump past the continuity window: the next frame is a fresh first frame.
    let after_jump = flat(200, 30.0);
    assert_eq!(deflicker.process(&after_jump), after_jump);
    // And so is a backwards seek.
    let after_seek = flat(50, 1.0);
    assert_eq!(deflicker.process(&after_seek), after_seek);

    let mut explicit = Deflicker::new(0.1).unwrap();
    explicit.process(&flat(100, 0.0));
    explicit.reset();
    let fresh = flat(180, 0.04);
    assert_eq!(explicit.process(&fresh), fresh);
}

#[test]
fn correction_is_bounded_so_real_fades_survive() {
    let mut deflicker = Deflicker::new(0.01).unwrap();
    deflicker.process(&flat(200, 0.0));
    // A cut to near-black would need >2x gain to hold the average; it is capped at 2x.
    let dark = deflicker.process(&flat(20, 0.04));
    assert!(grey_of(&dark) <= 40.0 + 1.0);
}

#[test]
fn a_black_frame_is_left_untouched() {
    let black = bush::VideoFrame::new(
        bush::PixelFormat::Rgba8,
        2,
        2,
        8,
        0.0,
        vec![0, 0, 0, 255].repeat(4),
    )
    .unwrap();
    let mut deflicker = bush::Deflicker::new(0.5).unwrap();
    assert_eq!(deflicker.process(&black).pixels(), black.pixels());
}
