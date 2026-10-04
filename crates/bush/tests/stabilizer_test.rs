use bush::{PixelFormat, Stabilizer, VideoFrame};

/// A smooth, textured scene sampled at a fractional shift (content moves by `(sx, sy)`).
fn scene_at(width: u32, height: u32, shift: (f32, f32), timestamp: f64) -> VideoFrame {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let (u, v) = (x as f32 - shift.0, y as f32 - shift.1);
            let value = 128.0
                + 55.0 * (u * 0.11 + 0.3).sin() * (v * 0.07 + 1.1).cos()
                + 40.0 * (u * 0.043 + v * 0.061 + 2.0).sin()
                + 25.0 * (v * 0.19 - u * 0.05).cos();
            let value = value.clamp(0.0, 255.0) as u8;
            pixels.extend_from_slice(&[value, value, value, 255]);
        }
    }
    VideoFrame::new(
        PixelFormat::Rgba8,
        width,
        height,
        width as usize * 4,
        timestamp,
        pixels,
    )
    .unwrap()
}

fn jitter(step: u32) -> (f32, f32) {
    // Deterministic pseudo-random jitter within +-6 pixels.
    let mut state = u64::from(step)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(12345);
    state ^= state >> 29;
    state = state.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    state ^= state >> 32;
    let x = ((state & 0xFFFF) as f32 / 65535.0 - 0.5) * 12.0;
    let y = (((state >> 16) & 0xFFFF) as f32 / 65535.0 - 0.5) * 12.0;
    (x, y)
}

fn std_dev(values: &[f32]) -> f32 {
    let mean = values.iter().sum::<f32>() / values.len() as f32;
    (values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / values.len() as f32).sqrt()
}

#[test]
fn rejects_invalid_smoothing() {
    for bad in [0.0, -0.5, 1.2, f32::NAN, f32::INFINITY] {
        assert!(Stabilizer::new(bad).is_none(), "{bad}");
    }
    assert!(Stabilizer::new(1.0).is_some());
}

#[test]
fn first_frame_and_static_scenes_need_no_correction() {
    let mut stabilizer = Stabilizer::new(0.2).unwrap();
    assert_eq!(
        stabilizer.analyze(&scene_at(320, 180, (0.0, 0.0), 0.0)),
        (0.0, 0.0)
    );
    for step in 1..6 {
        let correction =
            stabilizer.analyze(&scene_at(320, 180, (0.0, 0.0), f64::from(step) * 0.04));
        assert!(
            correction.0.abs() < 0.1 && correction.1.abs() < 0.1,
            "{correction:?}"
        );
    }
}

#[test]
fn a_single_shift_is_measured_to_sub_pixel_accuracy() {
    // Smoothing 0.25: correction = -(1 - 0.25) * motion.
    let mut stabilizer = Stabilizer::new(0.25).unwrap();
    stabilizer.analyze(&scene_at(320, 180, (0.0, 0.0), 0.0));
    let (cx, cy) = stabilizer.analyze(&scene_at(320, 180, (4.0, -3.0), 0.04));
    assert!((cx - -3.0).abs() < 0.6, "x correction {cx}");
    assert!((cy - 2.25).abs() < 0.6, "y correction {cy}");
}

#[test]
fn jitter_is_removed_from_the_resulting_camera_path() {
    let smoothing = 0.12;
    let mut stabilizer = Stabilizer::new(smoothing).unwrap();
    let mut raw = Vec::new();
    let mut stabilized = Vec::new();
    for step in 0..40 {
        let shift = jitter(step);
        let frame = scene_at(320, 180, shift, f64::from(step) * 0.04);
        let (cx, _) = stabilizer.analyze(&frame);
        if step >= 8 {
            raw.push(shift.0);
            // Where the content ends up after the correction is applied.
            stabilized.push(shift.0 + cx);
        }
    }
    let before = std_dev(&raw);
    let after = std_dev(&stabilized);
    assert!(before > 2.0, "test jitter too small: {before}");
    assert!(after < before * 0.5, "jitter {before} -> {after}");
}

#[test]
fn a_steady_pan_is_followed_with_bounded_lag() {
    let mut stabilizer = Stabilizer::new(0.3).unwrap();
    let mut last = (0.0, 0.0);
    for step in 0..30 {
        let frame = scene_at(320, 180, (step as f32 * 1.0, 0.0), f64::from(step) * 0.04);
        last = stabilizer.analyze(&frame);
    }
    // Steady state of an EMA on a ramp: smoothed lags the ramp by (1-w)/w * velocity.
    let expected_lag = (1.0 - 0.3) / 0.3;
    assert!(
        (last.0 - -expected_lag).abs() < 0.8,
        "lag correction {last:?}"
    );
    assert!(last.1.abs() < 0.3);
}

#[test]
fn repeats_reuse_the_correction_and_jumps_restart_the_path() {
    let mut stabilizer = Stabilizer::new(0.2).unwrap();
    stabilizer.analyze(&scene_at(320, 180, (0.0, 0.0), 0.0));
    let moved = scene_at(320, 180, (5.0, 2.0), 0.04);
    let first = stabilizer.analyze(&moved);
    for _ in 0..10 {
        assert_eq!(stabilizer.analyze(&moved), first);
    }

    // A jump far in time (or backwards) is a new shot: no correction for its first frame.
    assert_eq!(
        stabilizer.analyze(&scene_at(320, 180, (-8.0, 5.0), 30.0)),
        (0.0, 0.0)
    );
    assert_eq!(
        stabilizer.analyze(&scene_at(320, 180, (3.0, 3.0), 1.0)),
        (0.0, 0.0)
    );
    stabilizer.reset();
    assert_eq!(
        stabilizer.analyze(&scene_at(320, 180, (9.0, 9.0), 1.04)),
        (0.0, 0.0)
    );
}

#[test]
fn tiny_frames_are_handled_without_panicking() {
    let mut stabilizer = Stabilizer::new(0.5).unwrap();
    for step in 0..4 {
        let frame = scene_at(12, 8, (step as f32, 0.0), f64::from(step) * 0.04);
        let correction = stabilizer.analyze(&frame);
        assert!(correction.0.is_finite() && correction.1.is_finite());
    }
}

#[test]
fn motion_beyond_the_search_window_is_reported_at_its_edge() {
    // The analysis grid is 160 pixels: a 20 pixel jump is past the +-8 pixel search radius, so
    // the best match sits on the border of the window and gets no sub-pixel refinement.
    let mut stabilizer = Stabilizer::new(0.5).unwrap();
    stabilizer.analyze(&scene_at(160, 160, (0.0, 0.0), 0.0));
    let (dx, dy) = stabilizer.analyze(&scene_at(160, 160, (20.0, 20.0), 1.0 / 30.0));
    assert!(dx.is_finite() && dy.is_finite());
    assert!(dx.abs() <= 8.5 && dy.abs() <= 8.5, "({dx}, {dy})");
}
