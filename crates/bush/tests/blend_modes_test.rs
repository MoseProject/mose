//! Blend modes against FFmpeg's lend filter. The export path composites with that filter, so
//! every mode must produce the same bytes. Goldens are CRC32s of the full 256x256 truth tables
//! measured from a real FFmpeg build (source value on x, backdrop value on y).
//!
//! interpolate (float rounding) and hardoverlay (one input pair) are not bit exact, so they
//! are checked on a grid of measured spot values instead. When an fmpeg binary is on PATH a
//! second test re-measures every mode live and compares.

use bush::{BlendMode, CpuRenderer, LayerFrame, PixelFormat, VideoFrame};

fn gradient(horizontal: bool) -> VideoFrame {
    let mut pixels = Vec::with_capacity(256 * 256 * 4);
    for y in 0..256u32 {
        for x in 0..256u32 {
            let value = if horizontal { x } else { y } as u8;
            pixels.extend_from_slice(&[value, value, value, 255]);
        }
    }
    VideoFrame::new(PixelFormat::Rgba8, 256, 256, 1024, 0.0, pixels).unwrap()
}

/// The red channel of source (x gradient) blended over ackdrop (y gradient) with mode.
fn table(mode: BlendMode) -> Vec<u8> {
    let source = gradient(true);
    let backdrop = gradient(false);
    let mut top = LayerFrame::new(&source);
    top.blend = mode;
    let output = CpuRenderer::new(256 * 256 * 4)
        .unwrap()
        .render(256, 256, 0.0, &[LayerFrame::new(&backdrop), top])
        .unwrap();
    output
        .pixels()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| p[0])
        .collect()
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// (mode, FFmpeg all_mode name, CRC32 of the FFmpeg table).
const EXACT: &[(BlendMode, &str, u32)] = &[
    (BlendMode::Add, "addition", 0x7500048C),
    (BlendMode::And, "and", 0xBBBF6A7F),
    (BlendMode::Average, "average", 0x9AA157C7),
    (BlendMode::Bleach, "bleach", 0x0F78291E),
    (BlendMode::Burn, "burn", 0x20FE00E0),
    (BlendMode::Darken, "darken", 0xF718CB59),
    (BlendMode::Difference, "difference", 0x537644B6),
    (BlendMode::Divide, "divide", 0x1B549AE2),
    (BlendMode::Dodge, "dodge", 0xE0A7D86B),
    (BlendMode::Exclusion, "exclusion", 0x9765C769),
    (BlendMode::Extremity, "extremity", 0x843EC396),
    (BlendMode::Freeze, "freeze", 0x2DB5B257),
    (BlendMode::Geometric, "geometric", 0x8A0EB32D),
    (BlendMode::Glow, "glow", 0xC76DFD38),
    (BlendMode::GrainExtract, "grainextract", 0xDB867A47),
    (BlendMode::GrainMerge, "grainmerge", 0xAB8B79F2),
    (BlendMode::HardLight, "hardlight", 0xC5A00AC6),
    (BlendMode::HardMix, "hardmix", 0xEA030AAB),
    (BlendMode::HardOverlay, "hardoverlay", 0x5AD63320),
    (BlendMode::Harmonic, "harmonic", 0xF41DD662),
    (BlendMode::Heat, "heat", 0xDDF84580),
    (BlendMode::Interpolate, "interpolate", 0x33EFC60E),
    (BlendMode::Lighten, "lighten", 0x5AACF832),
    (BlendMode::LinearLight, "linearlight", 0x6D063AD4),
    (BlendMode::Multiply, "multiply", 0x8C8AA921),
    (BlendMode::Multiply128, "multiply128", 0x2B932869),
    (BlendMode::Negation, "negation", 0x8D023333),
    (BlendMode::Or, "or", 0x160B5914),
    (BlendMode::Overlay, "overlay", 0xCE5B6BBC),
    (BlendMode::Phoenix, "phoenix", 0x5A4AB413),
    (BlendMode::PinLight, "pinlight", 0x53AB17F6),
    (BlendMode::Reflect, "reflect", 0x78AC97C2),
    (BlendMode::Screen, "screen", 0x97C1190A),
    (BlendMode::SoftDifference, "softdifference", 0x6E316377),
    (BlendMode::SoftLight, "softlight", 0x5588DA8C),
    (BlendMode::Stain, "stain", 0x328D89C9),
    (BlendMode::Subtract, "subtract", 0xB3E800B9),
    (BlendMode::VividLight, "vividlight", 0xB3B9B561),
    (BlendMode::Xor, "xor", 0x7A23BD80),
];

const INTERPOLATE_SPOTS: &[(u8, u8, u8)] = &[
    (0, 0, 0),
    (0, 31, 5),
    (0, 62, 18),
    (0, 93, 37),
    (0, 124, 61),
    (0, 155, 85),
    (0, 186, 106),
    (0, 217, 121),
    (0, 248, 127),
    (29, 0, 4),
    (29, 31, 9),
    (29, 62, 22),
    (29, 93, 41),
    (29, 124, 65),
    (29, 155, 89),
    (29, 186, 110),
    (29, 217, 125),
    (29, 248, 131),
    (58, 0, 16),
    (58, 31, 20),
    (58, 62, 33),
    (58, 93, 53),
    (58, 124, 77),
    (58, 155, 101),
    (58, 186, 121),
    (58, 217, 136),
    (58, 248, 143),
    (87, 0, 33),
    (87, 31, 38),
    (87, 62, 51),
    (87, 93, 71),
    (87, 124, 94),
    (87, 155, 118),
    (87, 186, 139),
    (87, 217, 154),
    (87, 248, 161),
    (116, 0, 55),
    (116, 31, 59),
    (116, 62, 72),
    (116, 93, 92),
    (116, 124, 116),
    (116, 155, 140),
    (116, 186, 161),
    (116, 217, 175),
    (116, 248, 182),
    (145, 0, 77),
    (145, 31, 82),
    (145, 62, 95),
    (145, 93, 115),
    (145, 124, 138),
    (145, 155, 162),
    (145, 186, 183),
    (145, 217, 198),
    (145, 248, 205),
    (174, 0, 98),
    (174, 31, 103),
    (174, 62, 116),
    (174, 93, 136),
    (174, 124, 159),
    (174, 155, 183),
    (174, 186, 204),
    (174, 217, 219),
    (174, 248, 226),
    (203, 0, 115),
    (203, 31, 119),
    (203, 62, 133),
    (203, 93, 152),
    (203, 124, 176),
    (203, 155, 200),
    (203, 186, 221),
    (203, 217, 235),
    (203, 248, 242),
    (232, 0, 125),
    (232, 31, 130),
    (232, 62, 143),
    (232, 93, 162),
    (232, 124, 186),
    (232, 155, 210),
    (232, 186, 231),
    (232, 217, 246),
    (232, 248, 252),
];
const HARDOVERLAY_SPOTS: &[(u8, u8, u8)] = &[
    (0, 0, 0),
    (0, 31, 0),
    (0, 62, 0),
    (0, 93, 0),
    (0, 124, 0),
    (0, 155, 0),
    (0, 186, 0),
    (0, 217, 0),
    (0, 248, 0),
    (29, 0, 0),
    (29, 31, 7),
    (29, 62, 14),
    (29, 93, 21),
    (29, 124, 28),
    (29, 155, 35),
    (29, 186, 42),
    (29, 217, 49),
    (29, 248, 56),
    (58, 0, 0),
    (58, 31, 14),
    (58, 62, 28),
    (58, 93, 42),
    (58, 124, 56),
    (58, 155, 70),
    (58, 186, 84),
    (58, 217, 98),
    (58, 248, 112),
    (87, 0, 0),
    (87, 31, 21),
    (87, 62, 42),
    (87, 93, 63),
    (87, 124, 84),
    (87, 155, 105),
    (87, 186, 126),
    (87, 217, 148),
    (87, 248, 169),
    (116, 0, 0),
    (116, 31, 28),
    (116, 62, 56),
    (116, 93, 84),
    (116, 124, 112),
    (116, 155, 141),
    (116, 186, 169),
    (116, 217, 197),
    (116, 248, 225),
    (145, 0, 0),
    (145, 31, 35),
    (145, 62, 71),
    (145, 93, 107),
    (145, 124, 143),
    (145, 155, 179),
    (145, 186, 215),
    (145, 217, 251),
    (145, 248, 255),
    (174, 0, 0),
    (174, 31, 48),
    (174, 62, 97),
    (174, 93, 146),
    (174, 124, 195),
    (174, 155, 243),
    (174, 186, 255),
    (174, 217, 255),
    (174, 248, 255),
    (203, 0, 0),
    (203, 31, 76),
    (203, 62, 152),
    (203, 93, 228),
    (203, 124, 255),
    (203, 155, 255),
    (203, 186, 255),
    (203, 217, 255),
    (203, 248, 255),
    (232, 0, 0),
    (232, 31, 171),
    (232, 62, 255),
    (232, 93, 255),
    (232, 124, 255),
    (232, 155, 255),
    (232, 186, 255),
    (232, 217, 255),
    (232, 248, 255),
];

#[test]
fn bit_exact_modes_reproduce_ffmpeg_tables() {
    for (mode, name, expected) in EXACT {
        if matches!(mode, BlendMode::Interpolate | BlendMode::HardOverlay) {
            continue;
        }
        let actual = crc32(&table(*mode));
        assert_eq!(actual, *expected, "{name}: table differs from FFmpeg");
    }
}

#[test]
fn near_exact_modes_match_measured_spot_values() {
    for (mode, spots) in [
        (BlendMode::Interpolate, INTERPOLATE_SPOTS),
        (BlendMode::HardOverlay, HARDOVERLAY_SPOTS),
    ] {
        let table = table(mode);
        for (a, b, expected) in spots {
            let got = table[usize::from(*b) * 256 + usize::from(*a)];
            assert!(
                got.abs_diff(*expected) <= 1,
                "{mode:?} at ({a}, {b}): {got} vs {expected}"
            );
        }
    }
}

#[test]
fn normal_replaces_the_backdrop_and_overlay_keys_on_the_blended_layer() {
    let normal = table(BlendMode::Normal);
    assert!((0..65536).all(|i| normal[i] == (i % 256) as u8));
    // FFmpeg overlay tests the top layer (x): below 128 it multiplies, above it screens.
    let overlay = table(BlendMode::Overlay);
    // Source value 64 over backdrop 100 (index = backdrop * 256 + source).
    assert_eq!(overlay[100 * 256 + 64], (2 * (100 * 64 / 255)) as u8);
    // Source value 200 over backdrop 100.
    assert_eq!(
        overlay[100 * 256 + 200],
        (255 - 2 * ((255 - 100) * (255 - 200) / 255)) as u8
    );
}

#[test]
fn every_core_blend_mode_is_available() {
    // The 40 modes of the core enum, by FFmpeg name, all have a Bush counterpart.
    assert_eq!(EXACT.len() + 1, 40, "39 table modes plus Normal");
}

#[test]
fn translucent_blend_follows_the_w3c_composite_around_the_ffmpeg_function() {
    // Half-covering source with multiply over an opaque backdrop: the result lies between the
    // backdrop and the fully blended value.
    let source =
        VideoFrame::new(PixelFormat::Rgba8, 1, 1, 4, 0.0, vec![200, 200, 200, 255]).unwrap();
    let backdrop =
        VideoFrame::new(PixelFormat::Rgba8, 1, 1, 4, 0.0, vec![100, 100, 100, 255]).unwrap();
    let mut top = LayerFrame::new(&source);
    top.blend = BlendMode::Multiply;
    top.opacity = 0.5;
    let out = CpuRenderer::new(16)
        .unwrap()
        .render(1, 1, 0.0, &[LayerFrame::new(&backdrop), top])
        .unwrap();
    let full = 200 * 100 / 255;
    let expected = (100 + full) as f32 / 2.0;
    assert!((f32::from(out.pixels()[0]) - expected).abs() <= 1.5);
}

fn ffmpeg_table(name: &str) -> Option<Vec<u8>> {
    let output = std::process::Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=256x256:r=1,format=gray,geq=lum='X'",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=256x256:r=1,format=gray,geq=lum='Y'",
            "-filter_complex",
        ])
        .arg(format!("[0:v][1:v]blend=all_mode={name},format=gray"))
        .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "gray", "-"])
        .output()
        .ok()?;
    (output.stdout.len() == 65536).then_some(output.stdout)
}

#[test]
fn modes_match_a_live_ffmpeg_when_one_is_installed() {
    if ffmpeg_table("multiply").is_none() {
        eprintln!("skipped: no ffmpeg binary on PATH");
        return;
    }
    for (mode, name, _) in EXACT {
        let truth = ffmpeg_table(name).expect("ffmpeg supports every blend mode");
        let ours = table(*mode);
        let differing = truth.iter().zip(&ours).filter(|(a, b)| a != b).count();
        let tolerance = match mode {
            BlendMode::Interpolate => 200,
            BlendMode::HardOverlay => 2,
            _ => 0,
        };
        assert!(
            differing <= tolerance,
            "{name}: {differing} of 65536 pairs differ"
        );
    }
}
