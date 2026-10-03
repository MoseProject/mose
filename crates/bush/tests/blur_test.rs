use bush::{gaussian_blur, sharpen, PixelFormat, VideoFrame};

fn frame(width: u32, height: u32, rgba: impl Fn(u32, u32) -> [u8; 4]) -> VideoFrame {
    let mut pixels = Vec::new();
    for y in 0..height {
        for x in 0..width {
            pixels.extend_from_slice(&rgba(x, y));
        }
    }
    VideoFrame::new(
        PixelFormat::Rgba8,
        width,
        height,
        width as usize * 4,
        0.25,
        pixels,
    )
    .unwrap()
}

fn at(frame: &VideoFrame, x: u32, y: u32) -> [u8; 4] {
    let i = (y as usize * frame.width() as usize + x as usize) * 4;
    frame.pixels()[i..i + 4].try_into().unwrap()
}

fn luma_sum(frame: &VideoFrame) -> u64 {
    frame
        .pixels()
        .chunks_exact(4)
        .map(|p| u64::from(p[0]))
        .sum()
}

#[test]
fn zero_sigma_and_zero_amount_are_identity() {
    let source = frame(16, 16, |x, y| [(x * 9) as u8, (y * 7) as u8, 50, 255]);
    assert_eq!(gaussian_blur(&source, 0.0), source);
    assert_eq!(gaussian_blur(&source, -3.0), source);
    assert_eq!(gaussian_blur(&source, f32::NAN), source);
    assert_eq!(sharpen(&source, 0.0, 1.5), source);
}

#[test]
fn a_flat_picture_is_unchanged_by_blur_and_sharpen() {
    let source = frame(40, 30, |_, _| [90, 140, 200, 255]);
    for sigma in [0.5, 2.0, 7.0, 30.0] {
        assert_eq!(gaussian_blur(&source, sigma), source, "sigma {sigma}");
    }
    assert_eq!(sharpen(&source, 2.0, 1.5), source);
}

#[test]
fn an_impulse_spreads_symmetrically_and_conserves_energy() {
    let source = frame(63, 63, |x, y| {
        if x == 31 && y == 31 {
            [255, 255, 255, 255]
        } else {
            [0, 0, 0, 255]
        }
    });
    let blurred = gaussian_blur(&source, 3.0);

    // Peak at the centre, falling off, mirrored left/right and up/down.
    assert!(at(&blurred, 31, 31)[0] > at(&blurred, 34, 31)[0]);
    assert!(at(&blurred, 34, 31)[0] > at(&blurred, 40, 31)[0]);
    for offset in 1..8 {
        let right = i32::from(at(&blurred, 31 + offset, 31)[0]);
        let left = i32::from(at(&blurred, 31 - offset, 31)[0]);
        let down = i32::from(at(&blurred, 31, 31 + offset)[0]);
        assert!((right - left).abs() <= 1, "asymmetric at {offset}");
        assert!((right - down).abs() <= 1, "anisotropic at {offset}");
    }
    // Energy is preserved up to rounding of 8-bit samples (many tiny values round away).
    let before = luma_sum(&source) as f64;
    let after = luma_sum(&blurred) as f64;
    assert!(
        (after - before).abs() / before < 0.35,
        "energy {before} -> {after}"
    );
    assert!(blurred.pixels().chunks_exact(4).all(|p| p[3] == 255));
}

#[test]
fn blur_width_tracks_sigma() {
    let edge = frame(120, 8, |x, _| {
        if x < 60 {
            [0, 0, 0, 255]
        } else {
            [255, 255, 255, 255]
        }
    });
    // The 10%..90% transition of a blurred step is about 2.56 sigma wide.
    for sigma in [2.0_f32, 5.0, 10.0] {
        let blurred = gaussian_blur(&edge, sigma);
        let row: Vec<u8> = (0..120).map(|x| at(&blurred, x, 4)[0]).collect();
        let low = row.iter().position(|v| *v > 25).unwrap();
        let high = row.iter().position(|v| *v > 229).unwrap();
        let width = (high - low) as f32;
        let expected = 2.56 * sigma;
        assert!(
            (width - expected).abs() < expected * 0.35 + 1.5,
            "sigma {sigma}: width {width} vs {expected}"
        );
    }
}

#[test]
fn translucent_edges_keep_their_colour() {
    // Opaque red on the left, fully transparent black on the right: a premultiplied blur must
    // not darken the red at the boundary.
    let source = frame(40, 8, |x, _| {
        if x < 20 {
            [255, 0, 0, 255]
        } else {
            [0, 0, 0, 0]
        }
    });
    let blurred = gaussian_blur(&source, 4.0);
    for x in 12..28 {
        let pixel = at(&blurred, x, 4);
        if pixel[3] > 20 {
            assert!(
                pixel[0] >= 245,
                "red darkened to {} at x={x} (alpha {})",
                pixel[0],
                pixel[3]
            );
        }
    }
    let alpha: Vec<u8> = (10..30).map(|x| at(&blurred, x, 4)[3]).collect();
    assert!(alpha.windows(2).all(|pair| pair[1] <= pair[0]));
    assert!(alpha[0] > 200 && alpha[19] < 50);
}

#[test]
fn sharpen_overshoots_at_edges_and_preserves_geometry_and_alpha() {
    let edge = frame(60, 10, |x, _| {
        if x < 30 {
            [100, 100, 100, 200]
        } else {
            [160, 160, 160, 200]
        }
    });
    let sharp = sharpen(&edge, 1.5, 1.5);
    assert!(at(&sharp, 29, 5)[0] < 100);
    assert!(at(&sharp, 30, 5)[0] > 160);
    assert_eq!(at(&sharp, 5, 5), at(&edge, 5, 5));
    assert_eq!(at(&sharp, 55, 5), at(&edge, 55, 5));
    assert!(sharp.pixels().chunks_exact(4).all(|p| p[3] == 200));
    assert_eq!(
        (sharp.width(), sharp.height(), sharp.timestamp_secs()),
        (60, 10, 0.25)
    );
}

#[test]
fn padded_strides_and_large_parallel_frames_blur_like_small_ones() {
    let (width, height) = (200u32, 150u32);
    let pattern = |x: u32, y: u32| {
        [
            ((x * 5) % 256) as u8,
            ((y * 3) % 256) as u8,
            ((x + y) % 256) as u8,
            255,
        ]
    };
    let tight = frame(width, height, pattern);
    let stride = (width as usize + 6) * 4;
    let mut padded = vec![9u8; stride * height as usize];
    for y in 0..height {
        for x in 0..width {
            let at = y as usize * stride + x as usize * 4;
            padded[at..at + 4].copy_from_slice(&pattern(x, y));
        }
    }
    let padded = VideoFrame::new(PixelFormat::Rgba8, width, height, stride, 0.25, padded).unwrap();
    assert_eq!(gaussian_blur(&padded, 6.0), gaussian_blur(&tight, 6.0));
    let huge = gaussian_blur(&tight, 1_000.0);
    assert_eq!((huge.width(), huge.height()), (width, height));
}
