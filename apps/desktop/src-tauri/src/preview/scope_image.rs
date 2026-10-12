//! The monitors of the colour tools (`oca://scope/<kind>/<seq>`): a waveform, RGB parade,
//! histogram or vectorscope of the frame the preview shows, drawn with reference lines and served
//! as a JPEG. Computed on request from the latest preview frame at a small size, so a scope costs
//! nothing while no panel asks for it.

use avcore::ScopeKind;

use super::frames::encode_jpeg_sharp;

/// Width the frame is resampled to before the scope counts its pixels: a scope shows the
/// distribution of values, which a small copy of the picture keeps. As wide as the scope images,
/// so every column of a waveform receives pixels (a narrower source leaves gaps between traces).
const SOURCE_WIDTH: u32 = WIDE_SIZE.0;
/// Size of the waveform, parade and histogram images.
pub const WIDE_SIZE: (u32, u32) = (384, 192);
/// Side of the (square) vectorscope image.
pub const VECTOR_SIZE: u32 = 192;

/// The scope named in a request path (`waveform`, `parade`, `histogram`, `vectorscope`).
pub fn parse_kind(name: &str) -> Option<ScopeKind> {
    match name {
        "waveform" => Some(ScopeKind::Waveform),
        "parade" => Some(ScopeKind::Parade),
        "histogram" => Some(ScopeKind::Histogram),
        "vectorscope" => Some(ScopeKind::Vectorscope),
        _ => None,
    }
}

/// The scope of the picture in `jpeg` (a preview frame) as a JPEG, or `None` when it cannot be
/// decoded.
pub fn render(kind: ScopeKind, jpeg: &[u8]) -> Option<Vec<u8>> {
    let picture = image::load_from_memory_with_format(jpeg, image::ImageFormat::Jpeg)
        .ok()?
        .to_rgba8();
    let (width, height) = picture.dimensions();
    if width == 0 || height == 0 {
        return None;
    }
    // Nearest-neighbour resampling: the scopes only count pixel values.
    let (small_w, small_h) = (SOURCE_WIDTH, (height * SOURCE_WIDTH / width).max(1));
    let mut small = Vec::with_capacity((small_w * small_h * 4) as usize);
    for y in 0..small_h {
        for x in 0..small_w {
            small.extend_from_slice(
                &picture
                    .get_pixel(x * width / small_w, y * height / small_h)
                    .0,
            );
        }
    }

    let (wide_w, wide_h) = WIDE_SIZE;
    let (mut scope, out_w, out_h) = match kind {
        ScopeKind::Waveform => (
            avcore::luma_waveform_rgba(&small, small_w, small_h, wide_w, wide_h),
            wide_w,
            wide_h,
        ),
        ScopeKind::Parade => (
            avcore::rgb_parade_rgba(&small, small_w, small_h, wide_w, wide_h),
            wide_w,
            wide_h,
        ),
        ScopeKind::Histogram => (
            avcore::histogram_rgba(&small, small_w, small_h, wide_w, wide_h),
            wide_w,
            wide_h,
        ),
        ScopeKind::Vectorscope => (
            avcore::vectorscope_rgba(&small, small_w, small_h, VECTOR_SIZE),
            VECTOR_SIZE,
            VECTOR_SIZE,
        ),
    };
    if scope.is_empty() {
        return None;
    }
    avcore::draw_graticule(&mut scope, out_w, out_h, kind);
    encode_jpeg_sharp(out_w, out_h, &scope)
}

#[cfg(test)]
#[path = "scope_image_test.rs"]
mod tests;
