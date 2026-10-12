// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! Waveform/vectorscope monitors for calibrated color grading — per `request.md`'s Fase 4 color
//! tools and `spec/matrix/effects-and-color.md`'s "Known gaps" entry (LUTs/filters exist, no way
//! to calibrate exposure/saturation precisely). Pure pixel-crunching against an already-decoded
//! RGBA frame ([`crate::SampledFrame::rgba`]) — no new avfilter/GStreamer element
//! involved, so this works against exactly the same frame the preview already shows.
//!
//! **Simplification, not a hard wall**: a professional scope draws a green-phosphor trace with a
//! calibrated graticule (IRE/percent gridlines, hue targets on the vectorscope). These render a
//! grayscale intensity image instead — accurate for judging exposure spread (waveform) and
//! saturation/hue spread (vectorscope) at a glance, just without the reference overlay. Good
//! enough to spot a clipped highlight or an oversaturated color cast; not a substitute for a
//! calibrated broadcast monitor.

/// BT.709 luma weights (matches `ClipInstance`'s own color-adjustment stage's assumptions
/// elsewhere in this crate) — `0..=255`.
fn luma(r: u8, g: u8, b: u8) -> u8 {
    (0.2126 * r as f64 + 0.7152 * g as f64 + 0.0722 * b as f64).round() as u8
}

/// BT.601 Cb/Cr chroma components (`0..=255`, `128` = neutral gray) — the same matrix
/// `ffmpeg`'s own `format=yuv420p` conversion uses, chosen for consistency with what export's
/// avfilter pipeline actually sees, not because it's more "correct" than BT.709 for this
/// purpose (a vectorscope only needs *a* consistent chroma space, not a specific one).
fn chroma_cb_cr(r: u8, g: u8, b: u8) -> (u8, u8) {
    let (r, g, b) = (r as f64, g as f64, b as f64);
    let cb = (-0.168736 * r - 0.331264 * g + 0.5 * b + 128.0).clamp(0.0, 255.0);
    let cr = (0.5 * r - 0.418688 * g - 0.081312 * b + 128.0).clamp(0.0, 255.0);
    (cb.round() as u8, cr.round() as u8)
}

/// Returns whether `src_rgba` is a complete, tightly-packed RGBA frame of the declared size.
/// Scope rendering has no stride parameter, so accepting a partial or padded frame would make
/// its pixel addressing ambiguous.
fn is_packed_rgba_frame(src_rgba: &[u8], src_w: u32, src_h: u32) -> bool {
    (src_w as usize)
        .checked_mul(src_h as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        == Some(src_rgba.len())
}

/// Renders a luma waveform monitor from `src_rgba` (`src_w x src_h`, packed RGBA) into an
/// `out_w x out_h` grayscale-as-RGBA image: each source column contributes its pixels' luma
/// values as brightness accumulated into that same output column, brightest row = the darkest
/// luma (`0`) at the bottom, brightest luma (`255`) at the top — the standard waveform
/// orientation (a flat white line means an evenly-exposed flat field; a spike pinned to the top
/// or bottom means clipped highlights/shadows).
///
/// Each output cell's brightness is `count / src_h` (the maximum any single cell in a column
/// could ever reach) times a fixed gain, clamped to `255` — normalized per-column by the
/// column's own total sample count rather than a single global max, so one unusually
/// concentrated column (e.g. a solid-color letterbox bar) doesn't wash out every other column's
/// contrast.
pub fn luma_waveform_rgba(
    src_rgba: &[u8],
    src_w: u32,
    src_h: u32,
    out_w: u32,
    out_h: u32,
) -> Vec<u8> {
    if src_w == 0
        || src_h == 0
        || out_w == 0
        || out_h == 0
        || !is_packed_rgba_frame(src_rgba, src_w, src_h)
    {
        return Vec::new();
    }
    const GAIN: f64 = 6.0;
    let mut counts = vec![0u32; (out_w * out_h) as usize];
    for y in 0..src_h {
        for x in 0..src_w {
            let i = ((y * src_w + x) * 4) as usize;
            let l = luma(src_rgba[i], src_rgba[i + 1], src_rgba[i + 2]);
            let col = (x * out_w / src_w).min(out_w - 1);
            let row = (out_h - 1).saturating_sub((l as u32 * (out_h - 1)) / 255);
            counts[(row * out_w + col) as usize] += 1;
        }
    }
    let column_total = src_h as f64;
    let mut rgba = vec![0u8; (out_w * out_h * 4) as usize];
    for (i, &count) in counts.iter().enumerate() {
        let intensity = ((count as f64 / column_total) * GAIN * 255.0).clamp(0.0, 255.0) as u8;
        let px = i * 4;
        rgba[px] = intensity;
        rgba[px + 1] = intensity;
        rgba[px + 2] = intensity;
        rgba[px + 3] = 255;
    }
    rgba
}

/// Renders a Cb/Cr vectorscope from `src_rgba` (`src_w x src_h`, packed RGBA) into an
/// `out_size x out_size` grayscale-as-RGBA scatter image: each pixel's chroma
/// ([`chroma_cb_cr`]) plots as one point, `Cb` left-to-right, `Cr` bottom-to-top (increasing
/// red-ness goes up, matching a conventional vectorscope's orientation), both centered on
/// neutral gray at the image's center. A tight cluster near the center means low saturation; a
/// trace pushed toward the edge in one direction means a color cast in that hue.
///
/// Same per-cell "count relative to this source's total pixel count" normalization
/// [`luma_waveform_rgba`] uses, rather than a global max, for the same reason (one saturated
/// blob shouldn't suppress the rest of the trace's visibility).
pub fn vectorscope_rgba(src_rgba: &[u8], src_w: u32, src_h: u32, out_size: u32) -> Vec<u8> {
    if src_w == 0 || src_h == 0 || out_size == 0 || !is_packed_rgba_frame(src_rgba, src_w, src_h) {
        return Vec::new();
    }
    const GAIN: f64 = 40.0;
    let mut counts = vec![0u32; (out_size * out_size) as usize];
    for y in 0..src_h {
        for x in 0..src_w {
            let i = ((y * src_w + x) * 4) as usize;
            let (cb, cr) = chroma_cb_cr(src_rgba[i], src_rgba[i + 1], src_rgba[i + 2]);
            let gx = ((cb as u32) * (out_size - 1).max(1)) / 255;
            let gy = (out_size - 1).saturating_sub(((cr as u32) * (out_size - 1).max(1)) / 255);
            let gx = gx.min(out_size - 1);
            let gy = gy.min(out_size - 1);
            counts[(gy * out_size + gx) as usize] += 1;
        }
    }
    let total_pixels = (src_w as u64 * src_h as u64) as f64;
    let mut rgba = vec![0u8; (out_size * out_size * 4) as usize];
    for (i, &count) in counts.iter().enumerate() {
        let intensity = ((count as f64 / total_pixels) * GAIN * 255.0).clamp(0.0, 255.0) as u8;
        let px = i * 4;
        rgba[px] = intensity;
        rgba[px + 1] = intensity;
        rgba[px + 2] = intensity;
        rgba[px + 3] = 255;
    }
    rgba
}

/// Renders both [`luma_waveform_rgba`] and [`vectorscope_rgba`] from the same `src_rgba` frame in
/// a single pass over the source pixels, instead of the two independent full-frame passes calling
/// them separately would do. `App::pump_preview_frame` always wants both together whenever
/// `scopes_enabled` (see `perf/REPORT.md`'s scopes section for the measured savings) — every other
/// aspect (normalization, orientation, output format) is identical to calling the two functions
/// on their own; this is purely a shared-iteration optimization, not a behavior change. Returns
/// `(waveform_rgba, vectorscope_rgba)`, each shaped exactly as its standalone function's own doc
/// comment describes.
pub fn render_scopes_rgba(
    src_rgba: &[u8],
    src_w: u32,
    src_h: u32,
    waveform_out_w: u32,
    waveform_out_h: u32,
    vectorscope_out_size: u32,
) -> (Vec<u8>, Vec<u8>) {
    if src_w == 0 || src_h == 0 || !is_packed_rgba_frame(src_rgba, src_w, src_h) {
        return (Vec::new(), Vec::new());
    }
    let waveform_ready = waveform_out_w != 0 && waveform_out_h != 0;
    let vectorscope_ready = vectorscope_out_size != 0;

    let mut waveform_counts = vec![0u32; (waveform_out_w * waveform_out_h) as usize];
    let mut vectorscope_counts = vec![0u32; (vectorscope_out_size * vectorscope_out_size) as usize];

    for y in 0..src_h {
        for x in 0..src_w {
            let i = ((y * src_w + x) * 4) as usize;
            let (r, g, b) = (src_rgba[i], src_rgba[i + 1], src_rgba[i + 2]);
            if waveform_ready {
                let l = luma(r, g, b);
                let col = (x * waveform_out_w / src_w).min(waveform_out_w - 1);
                let row =
                    (waveform_out_h - 1).saturating_sub((l as u32 * (waveform_out_h - 1)) / 255);
                waveform_counts[(row * waveform_out_w + col) as usize] += 1;
            }
            if vectorscope_ready {
                let (cb, cr) = chroma_cb_cr(r, g, b);
                let gx = ((cb as u32) * (vectorscope_out_size - 1).max(1)) / 255;
                let gy = (vectorscope_out_size - 1)
                    .saturating_sub(((cr as u32) * (vectorscope_out_size - 1).max(1)) / 255);
                let gx = gx.min(vectorscope_out_size - 1);
                let gy = gy.min(vectorscope_out_size - 1);
                vectorscope_counts[(gy * vectorscope_out_size + gx) as usize] += 1;
            }
        }
    }

    let waveform_rgba = if waveform_ready {
        const GAIN: f64 = 6.0;
        let column_total = src_h as f64;
        let mut rgba = vec![0u8; (waveform_out_w * waveform_out_h * 4) as usize];
        for (i, &count) in waveform_counts.iter().enumerate() {
            let intensity = ((count as f64 / column_total) * GAIN * 255.0).clamp(0.0, 255.0) as u8;
            let px = i * 4;
            rgba[px] = intensity;
            rgba[px + 1] = intensity;
            rgba[px + 2] = intensity;
            rgba[px + 3] = 255;
        }
        rgba
    } else {
        Vec::new()
    };

    let vectorscope_rgba = if vectorscope_ready {
        const GAIN: f64 = 40.0;
        let total_pixels = (src_w as u64 * src_h as u64) as f64;
        let mut rgba = vec![0u8; (vectorscope_out_size * vectorscope_out_size * 4) as usize];
        for (i, &count) in vectorscope_counts.iter().enumerate() {
            let intensity = ((count as f64 / total_pixels) * GAIN * 255.0).clamp(0.0, 255.0) as u8;
            let px = i * 4;
            rgba[px] = intensity;
            rgba[px + 1] = intensity;
            rgba[px + 2] = intensity;
            rgba[px + 3] = 255;
        }
        rgba
    } else {
        Vec::new()
    };

    (waveform_rgba, vectorscope_rgba)
}

/// Renders an RGB parade from `src_rgba`: the red, green and blue waveforms side by side in thirds
/// of `out_w`, each drawn in its own colour on black. Same orientation and per-column
/// normalization as [`luma_waveform_rgba`] (`0` at the bottom, `255` at the top). Judges colour
/// balance: three traces at the same height are neutral, one riding higher is a cast.
pub fn rgb_parade_rgba(src_rgba: &[u8], src_w: u32, src_h: u32, out_w: u32, out_h: u32) -> Vec<u8> {
    if src_w == 0
        || src_h == 0
        || out_w < 3
        || out_h == 0
        || !is_packed_rgba_frame(src_rgba, src_w, src_h)
    {
        return Vec::new();
    }
    const GAIN: f64 = 6.0;
    let third = out_w / 3;
    let mut counts = vec![[0u32; 3]; (out_w * out_h) as usize];
    for y in 0..src_h {
        for x in 0..src_w {
            let i = ((y * src_w + x) * 4) as usize;
            let col_in_third = (x * third / src_w).min(third - 1);
            for channel in 0..3u32 {
                let value = src_rgba[i + channel as usize] as u32;
                let row = (out_h - 1).saturating_sub((value * (out_h - 1)) / 255);
                let col = channel * third + col_in_third;
                counts[(row * out_w + col) as usize][channel as usize] += 1;
            }
        }
    }
    let column_total = src_h as f64;
    let mut rgba = vec![0u8; (out_w * out_h * 4) as usize];
    for (i, cell) in counts.iter().enumerate() {
        let px = i * 4;
        let column = i as u32 % out_w;
        let channel = (column / third.max(1)).min(2) as usize;
        let intensity =
            ((cell[channel] as f64 / column_total) * GAIN * 255.0).clamp(0.0, 255.0) as u8;
        rgba[px + channel] = intensity;
        rgba[px + 3] = 255;
    }
    rgba
}

/// Renders an RGB histogram from `src_rgba`: one bar per intensity level across `out_w`, the three
/// channels drawn over one another (additively, so grey where they agree) and scaled to the
/// tallest bin. A pile against the left edge is crushed shadows, against the right is clipped
/// highlights.
pub fn histogram_rgba(src_rgba: &[u8], src_w: u32, src_h: u32, out_w: u32, out_h: u32) -> Vec<u8> {
    if src_w == 0
        || src_h == 0
        || out_w == 0
        || out_h == 0
        || !is_packed_rgba_frame(src_rgba, src_w, src_h)
    {
        return Vec::new();
    }
    let mut bins = [[0u32; 256]; 3];
    for pixel in src_rgba.chunks_exact(4) {
        for channel in 0..3 {
            bins[channel][pixel[channel] as usize] += 1;
        }
    }
    // Scaled to the tallest bin that is not a pile against either end, so a clipped highlight
    // does not flatten the rest of the curve; the piles themselves are clamped to the top.
    let tallest = bins
        .iter()
        .flat_map(|channel| channel[1..255].iter().copied())
        .max()
        .unwrap_or(1)
        .max(1) as f64;
    let mut rgba = vec![0u8; (out_w * out_h * 4) as usize];
    for px in rgba.chunks_exact_mut(4) {
        px[3] = 255;
    }
    for x in 0..out_w {
        let level = ((x as u64 * 256) / out_w as u64).min(255) as usize;
        for channel in 0..3 {
            let height = ((bins[channel][level] as f64 / tallest).min(1.0) * out_h as f64) as u32;
            for row in 0..height {
                let y = out_h - 1 - row;
                rgba[((y * out_w + x) * 4) as usize + channel] = 255;
            }
        }
    }
    rgba
}

/// Which scope a graticule is drawn for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    Waveform,
    Parade,
    Histogram,
    Vectorscope,
}

/// Draws the reference lines of `kind` over a scope image of `width x height` (packed RGBA):
/// horizontal lines at 0, 25, 50, 75 and 100 per cent for the waveform and the parade, vertical at
/// 25, 50 and 75 for the histogram, and for the vectorscope the axes, a circle and the six colour
/// targets (R, Yl, G, Cy, B, Mg) at 75% saturation. Blended in a dim cyan so the trace stays
/// readable; the strong lines (0, 50, 100) are a little brighter.
pub fn draw_graticule(rgba: &mut [u8], width: u32, height: u32, kind: ScopeKind) {
    if width == 0 || height == 0 || rgba.len() != (width * height * 4) as usize {
        return;
    }
    let mut blend = |x: u32, y: u32, strength: u8| {
        if x < width && y < height {
            let at = ((y * width + x) * 4) as usize;
            for (channel, tint) in [(0usize, 40u32), (1, 160), (2, 180)] {
                let base = rgba[at + channel] as u32;
                let target = tint * strength as u32 / 255;
                rgba[at + channel] = base.max(target) as u8;
            }
        }
    };
    match kind {
        ScopeKind::Waveform | ScopeKind::Parade => {
            for (index, percent) in [0u32, 25, 50, 75, 100].into_iter().enumerate() {
                let y = (height - 1) - (percent * (height - 1) / 100);
                let strength = if index % 2 == 0 { 140 } else { 70 };
                for x in 0..width {
                    blend(x, y, strength);
                }
            }
            if kind == ScopeKind::Parade {
                for divider in [width / 3, 2 * width / 3] {
                    for y in 0..height {
                        blend(divider, y, 70);
                    }
                }
            }
        }
        ScopeKind::Histogram => {
            for percent in [25u32, 50, 75] {
                let x = percent * (width - 1) / 100;
                for y in 0..height {
                    blend(x, y, if percent == 50 { 100 } else { 60 });
                }
            }
        }
        ScopeKind::Vectorscope => {
            let centre = (width - 1) as f64 / 2.0;
            for step in 0..width {
                blend(step, (height - 1) / 2, 70);
                blend((width - 1) / 2, step, 70);
            }
            let radius = centre * 0.9;
            for degrees in 0..720 {
                let angle = f64::from(degrees) * std::f64::consts::PI / 360.0;
                let x = centre + radius * angle.cos();
                let y = centre - radius * angle.sin();
                blend(x.round() as u32, y.round() as u32, 60);
            }
            for (r, g, b) in [
                (255u8, 0u8, 0u8),
                (255, 255, 0),
                (0, 255, 0),
                (0, 255, 255),
                (0, 0, 255),
                (255, 0, 255),
            ] {
                // Colour targets at 75% saturation, where a colour bar test pattern puts them.
                let mix = |channel: u8| (255.0 * 0.25 + f64::from(channel) * 0.75).round() as u8;
                let (cb, cr) = chroma_cb_cr(mix(r), mix(g), mix(b));
                let x = (f64::from(cb) * (width - 1) as f64 / 255.0).round() as u32;
                let y = (height - 1)
                    .saturating_sub((f64::from(cr) * (height - 1) as f64 / 255.0).round() as u32);
                for dx in 0..5u32 {
                    for dy in 0..5u32 {
                        if dx == 0 || dx == 4 || dy == 0 || dy == 4 {
                            blend((x + dx).saturating_sub(2), (y + dy).saturating_sub(2), 200);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "scopes/scopes_test.rs"]
mod tests;
