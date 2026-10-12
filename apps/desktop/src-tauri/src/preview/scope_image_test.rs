use avcore::ScopeKind;

use super::*;
use crate::frames::encode_jpeg_rgba;

/// A preview frame the way the engine stores it: a flat colour, JPEG encoded.
fn frame(r: u8, g: u8, b: u8) -> Vec<u8> {
    let rgba: Vec<u8> = (0..640 * 360).flat_map(|_| [r, g, b, 255]).collect();
    encode_jpeg_rgba(640, 360, &rgba, 1280).expect("encodes")
}

fn decoded(jpeg: &[u8]) -> image::RgbaImage {
    image::load_from_memory_with_format(jpeg, image::ImageFormat::Jpeg)
        .expect("a JPEG")
        .to_rgba8()
}

#[test]
fn every_scope_is_a_jpeg_of_its_own_size() {
    let jpeg = frame(200, 60, 40);
    for (kind, size) in [
        (ScopeKind::Waveform, WIDE_SIZE),
        (ScopeKind::Parade, WIDE_SIZE),
        (ScopeKind::Histogram, WIDE_SIZE),
        (ScopeKind::Vectorscope, (VECTOR_SIZE, VECTOR_SIZE)),
    ] {
        let scope = render(kind, &jpeg).unwrap_or_else(|| panic!("{kind:?} renders"));
        assert_eq!(decoded(&scope).dimensions(), size, "{kind:?}");
    }
}

/// The strongest value of `channel` over `rows` of column `x`: the trace is one row thick and
/// the JPEG blurs it a little, so a test looks at a few rows around where it should be.
fn strongest(image: &image::RgbaImage, x: u32, rows: std::ops::Range<u32>, channel: usize) -> u8 {
    rows.map(|y| image.get_pixel(x, y).0[channel])
        .max()
        .unwrap()
}

#[test]
fn the_waveform_of_a_flat_picture_is_one_line_at_its_luma() {
    // A white picture rides the top of the waveform, a black one the bottom.
    let bright = decoded(&render(ScopeKind::Waveform, &frame(255, 255, 255)).unwrap());
    let dark = decoded(&render(ScopeKind::Waveform, &frame(0, 0, 0)).unwrap());
    assert!(
        strongest(&bright, 180, 0..4, 0) > 150,
        "white sits at the top"
    );
    assert!(
        strongest(&bright, 180, 100..140, 0) < 60,
        "and nowhere near the middle"
    );
    assert!(
        strongest(&dark, 180, 188..192, 0) > 150,
        "black sits at the bottom"
    );
    assert!(strongest(&dark, 180, 60..100, 0) < 60);
}

#[test]
fn the_parade_of_a_red_picture_lights_only_the_red_third() {
    let image = decoded(&render(ScopeKind::Parade, &frame(255, 0, 0)).unwrap());
    assert!(
        strongest(&image, 40, 0..4, 0) > 150,
        "the red third carries the trace at 100%"
    );
    assert!(
        strongest(&image, 190, 0..4, 1) < 120,
        "green is at 0, not at the top"
    );
    assert!(
        strongest(&image, 330, 0..4, 2) < 120,
        "blue is at 0, not at the top"
    );
}

#[test]
fn something_that_is_not_a_jpeg_gives_no_scope() {
    assert!(render(ScopeKind::Waveform, b"not a picture").is_none());
    assert!(render(ScopeKind::Waveform, &[]).is_none());
}

#[test]
fn request_names_map_to_scopes() {
    assert_eq!(parse_kind("waveform"), Some(ScopeKind::Waveform));
    assert_eq!(parse_kind("parade"), Some(ScopeKind::Parade));
    assert_eq!(parse_kind("histogram"), Some(ScopeKind::Histogram));
    assert_eq!(parse_kind("vectorscope"), Some(ScopeKind::Vectorscope));
    assert_eq!(parse_kind("nonsense"), None);
}
