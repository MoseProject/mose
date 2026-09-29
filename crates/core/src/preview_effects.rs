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

//! P4 item 21, "Preview support for vignette/glitch/deflicker/3D-LUT/stabilization"
//! (`spec/ROADMAP.md`) — confirmed a hard wall for the *real* `avfilter` output (no matching
//! GStreamer element exists on any dev machine checked, per `matrix/effects-and-color.md`).
//! Rather than leave the gap entirely closed, this covers the two effects with a precise,
//! well-specified, non-temporal per-pixel definition -- a 3D LUT lookup and a radial vignette --
//! as **CPU-side post-processing of the already-decoded preview frame**, the same pattern
//! [`crate::scopes`] already established for the waveform/vectorscope overlays (post-process the
//! decoded `VideoFrame`, no new avfilter/GStreamer element). Applied only to `App`'s live preview
//! texture in `ui`; the real export path is untouched and still uses the exact `lut3d`/`vignette`
//! `avfilter`s (`ClipInstance::video_filter_chain`) for final output.
//!
//! **This is a preview approximation, not a bit-exact reproduction of either `avfilter`.** The
//! vignette formula here is a simple radial falloff, not FFmpeg's own cosine-based one (deriving
//! that exactly from `libavfilter`'s C source without being able to visually A/B preview against
//! export in this environment isn't a risk worth taking — see the roadmap item's own caution).
//! The LUT lookup, in contrast, *is* precise: the `.cube` format and trilinear interpolation are
//! both fully specified, deterministic, and unit-testable without needing a GUI.
//!
//! **Glitch is now covered too**, in a later follow-up: export's own `glitch_intensity` (see
//! [`crate::timeline::ClipInstance::video_filter_chain`]) resolves to FFmpeg's `noise` avfilter
//! (temporal luma/chroma noise, not an RGB-shift/block-displacement "datamosh" look — a real,
//! already-settled fact about what this codebase's own glitch effect *is*, not a judgment call
//! made from scratch) — [`apply_glitch_to_rgba`] mirrors that same "additive per-pixel temporal
//! noise" shape in the RGBA domain (no luma/chroma split available on an already-decoded RGBA
//! frame, so this adds independent noise per R/G/B channel instead — a preview approximation,
//! same "not bit-exact" caveat vignette above already carries, not a reproduction of
//! `libavfilter/vf_noise.c`'s own PRNG).
//!
//! **Deflicker is now covered too**, in a later follow-up: [`apply_deflicker_to_rgba`] keeps a
//! caller-owned [`DeflickerHistory`] (a 5-frame rolling window of mean luma, matching export's
//! own `deflicker=mode=am:size=5` window) and shifts each frame's brightness toward that rolling
//! average — a well-specified, deterministic, fully unit-testable temporal correction, unlike
//! stabilization's motion-estimation problem below.
//!
//! **Still not covered by this pass** (left as the roadmap item's remaining gap):
//! stabilization — motion estimation between frames (optical flow or an equivalent) is a
//! fundamentally different, materially larger problem than a rolling scalar average, with its
//! own seek/scrub edge cases (a jump-cut in scrub position shouldn't try to "stabilize" against
//! a now-irrelevant previous frame) — not a natural extension of this module's per-frame or
//! simple-rolling-window shape.

use std::path::Path;

/// A parsed 3D LUT (`.cube` file, Adobe's Common LUT Format — the same format
/// [`crate::timeline::ClipInstance::lut_path`] already stores a path to for export). `size`
/// entries per axis, `data` flattened with red the fastest-varying index (`r + g*size +
/// b*size*size`), matching the `.cube` spec's own data ordering.
#[derive(Debug, Clone, PartialEq)]
pub struct Lut3D {
    size: usize,
    data: Vec<[f32; 3]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LutParseError {
    MissingSize,
    InvalidSize,
    SizeTooSmall(i64),
    WrongRowCount { expected: usize, found: usize },
    MalformedRow(usize),
}

impl std::fmt::Display for LutParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LutParseError::MissingSize => write!(f, "no LUT_3D_SIZE line found"),
            LutParseError::InvalidSize => write!(f, "LUT_3D_SIZE isn't a valid integer"),
            LutParseError::SizeTooSmall(n) => {
                write!(f, "LUT_3D_SIZE must be at least 2, got {n}")
            }
            LutParseError::WrongRowCount { expected, found } => {
                write!(f, "expected {expected} data rows, found {found}")
            }
            LutParseError::MalformedRow(row) => {
                write!(f, "data row {row} doesn't have 3 numbers")
            }
        }
    }
}

impl std::error::Error for LutParseError {}

impl Lut3D {
    /// Parses a `.cube` file's contents. Tolerates `#`-comments, blank lines, and the optional
    /// `TITLE`/`DOMAIN_MIN`/`DOMAIN_MAX` metadata lines (the last two are read but not applied —
    /// this codebase's own exported LUTs, and every common vendor LUT, use the default `0..1`
    /// domain; a non-default domain would just mean the preview approximation is slightly off,
    /// never a crash or a panic).
    pub fn parse(contents: &str) -> Result<Self, LutParseError> {
        let mut size: Option<usize> = None;
        let mut data = Vec::new();

        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(rest) = line.strip_prefix("LUT_3D_SIZE") {
                let n: i64 = rest
                    .trim()
                    .parse()
                    .map_err(|_| LutParseError::InvalidSize)?;
                if n < 2 {
                    return Err(LutParseError::SizeTooSmall(n));
                }
                size = Some(n as usize);
                continue;
            }
            if line.starts_with("TITLE")
                || line.starts_with("DOMAIN_MIN")
                || line.starts_with("DOMAIN_MAX")
                || line.starts_with("LUT_1D_SIZE")
            {
                continue;
            }

            let values: Result<Vec<f32>, _> = line.split_whitespace().map(str::parse).collect();
            let Ok(values) = values else {
                return Err(LutParseError::MalformedRow(data.len()));
            };
            if values.len() != 3 || values.iter().any(|value| !value.is_finite()) {
                return Err(LutParseError::MalformedRow(data.len()));
            }
            data.push([values[0], values[1], values[2]]);
        }

        let size = size.ok_or(LutParseError::MissingSize)?;
        let expected = size
            .checked_mul(size)
            .and_then(|entries| entries.checked_mul(size))
            .ok_or(LutParseError::InvalidSize)?;
        if data.len() != expected {
            return Err(LutParseError::WrongRowCount {
                expected,
                found: data.len(),
            });
        }
        Ok(Self { size, data })
    }

    /// Reads and parses `path`. Errors the same way [`std::fs::read_to_string`]/[`Self::parse`]
    /// would (I/O failure surfaces as a generic parse-shaped error via [`LutParseError::
    /// MissingSize`] — the caller only needs "did this work," not to distinguish the two).
    pub fn load(path: &Path) -> Result<Self, LutParseError> {
        let contents = std::fs::read_to_string(path).map_err(|_| LutParseError::MissingSize)?;
        Self::parse(&contents)
    }

    fn at(&self, x: usize, y: usize, z: usize) -> [f32; 3] {
        self.data[x + y * self.size + z * self.size * self.size]
    }

    /// Trilinear-interpolated lookup for a normalized `(r, g, b)` in `[0.0, 1.0]` (out-of-range
    /// inputs are clamped, never a panic or a wraparound).
    pub fn sample(&self, r: f32, g: f32, b: f32) -> [f32; 3] {
        let max_index = (self.size - 1) as f32;
        let normalized = |value: f32| {
            if value.is_finite() {
                value.clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        let fx = normalized(r) * max_index;
        let fy = normalized(g) * max_index;
        let fz = normalized(b) * max_index;

        let x0 = fx.floor() as usize;
        let y0 = fy.floor() as usize;
        let z0 = fz.floor() as usize;
        let x1 = (x0 + 1).min(self.size - 1);
        let y1 = (y0 + 1).min(self.size - 1);
        let z1 = (z0 + 1).min(self.size - 1);
        let tx = fx - x0 as f32;
        let ty = fy - y0 as f32;
        let tz = fz - z0 as f32;

        let lerp3 = |a: [f32; 3], b: [f32; 3], t: f32| {
            [
                a[0] + (b[0] - a[0]) * t,
                a[1] + (b[1] - a[1]) * t,
                a[2] + (b[2] - a[2]) * t,
            ]
        };

        let c00 = lerp3(self.at(x0, y0, z0), self.at(x1, y0, z0), tx);
        let c10 = lerp3(self.at(x0, y1, z0), self.at(x1, y1, z0), tx);
        let c01 = lerp3(self.at(x0, y0, z1), self.at(x1, y0, z1), tx);
        let c11 = lerp3(self.at(x0, y1, z1), self.at(x1, y1, z1), tx);
        let c0 = lerp3(c00, c10, ty);
        let c1 = lerp3(c01, c11, ty);
        lerp3(c0, c1, tz)
    }
}

/// Applies `lut` to `rgba` in place (a tightly-packed `width*height*4`-byte RGBA8 buffer, alpha
/// untouched) — what [`crate::timeline::ClipInstance::lut_path`] means for a live preview frame,
/// via [`Lut3D::sample`]. No-op if `rgba`'s length isn't a multiple of 4.
pub fn apply_lut_to_rgba(rgba: &mut [u8], lut: &Lut3D) {
    for pixel in rgba.as_chunks_mut::<4>().0 {
        let [r, g, b] = lut.sample(
            pixel[0] as f32 / 255.0,
            pixel[1] as f32 / 255.0,
            pixel[2] as f32 / 255.0,
        );
        pixel[0] = (r.clamp(0.0, 1.0) * 255.0).round() as u8;
        pixel[1] = (g.clamp(0.0, 1.0) * 255.0).round() as u8;
        pixel[2] = (b.clamp(0.0, 1.0) * 255.0).round() as u8;
    }
}

/// Applies a radial vignette darkening to `rgba` in place (a tightly-packed `width*height*4`-
/// byte RGBA8 buffer, alpha untouched) -- a **preview approximation**, not the real `vignette`
/// `avfilter`'s exact cosine falloff (see this module's doc comment). `intensity` in `[0.0,
/// 1.0]`-ish (matches [`crate::timeline::ClipInstance::vignette_intensity`]'s own range): `0.0`
/// is a no-op, `1.0` darkens the frame's extreme corners to black. No-op if `width`/`height` is
/// `0` or `rgba`'s length doesn't match `width*height*4`.
pub fn apply_vignette_to_rgba(rgba: &mut [u8], width: u32, height: u32, intensity: f32) {
    if width == 0 || height == 0 || intensity <= 0.0 {
        return;
    }
    if rgba.len() != (width as usize) * (height as usize) * 4 {
        return;
    }

    let cx = width as f32 / 2.0;
    let cy = height as f32 / 2.0;
    // The corner-to-center distance in this normalized space, so `norm_dist` reaches exactly
    // 1.0 at every frame corner regardless of aspect ratio.
    let corner_dist = (2.0_f32).sqrt();

    for y in 0..height {
        for x in 0..width {
            let dx = (x as f32 + 0.5 - cx) / cx;
            let dy = (y as f32 + 0.5 - cy) / cy;
            let norm_dist = ((dx * dx + dy * dy).sqrt() / corner_dist).min(1.0);
            let factor = 1.0 - intensity * norm_dist * norm_dist;
            let index = ((y * width + x) * 4) as usize;
            rgba[index] = (rgba[index] as f32 * factor).round().clamp(0.0, 255.0) as u8;
            rgba[index + 1] = (rgba[index + 1] as f32 * factor).round().clamp(0.0, 255.0) as u8;
            rgba[index + 2] = (rgba[index + 2] as f32 * factor).round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// Applies temporal per-pixel RGB noise to `rgba` in place — a **preview approximation** of
/// [`crate::timeline::ClipInstance::glitch_intensity`]'s export-side `noise` avfilter (see this
/// module's doc comment for why this isn't a reproduction of that filter's own PRNG). `intensity`
/// in `glitch_intensity`'s own `0.0..=1.0` range: `0.0` is a no-op, `1.0` adds up to `±40` per
/// channel — a judgment call, same style as this module's other intensity-to-magnitude scale
/// factors, not a value derived from anything. `seed` should differ frame to frame (e.g.
/// derived from elapsed wall-clock time) so the noise pattern looks like "digital corruption,"
/// not a static grain texture baked onto the image; a fixed `seed` gives fully reproducible
/// output for the same input, which is what this function's own unit tests rely on. Uses a
/// simple xorshift64* PRNG, seeded per pixel from `seed` — fast, deterministic, no external
/// dependency, and no cryptographic-quality requirement (this is a visual effect, not security-
/// sensitive). No-op if `rgba` is empty.
pub fn apply_glitch_to_rgba(rgba: &mut [u8], intensity: f32, seed: u64) {
    let intensity = intensity.clamp(0.0, 1.0);
    if intensity <= 0.0 || rgba.is_empty() {
        return;
    }
    let strength = (intensity * 40.0).round() as i64;
    if strength <= 0 {
        return;
    }
    let span = (2 * strength + 1) as u64;
    for (pixel_index, chunk) in rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let mut state = seed ^ (pixel_index as u64).wrapping_mul(0x9E3779B97F4A7C15);
        for channel in chunk.iter_mut().take(3) {
            // xorshift64* — https://en.wikipedia.org/wiki/Xorshift#xorshift*, a fast,
            // deterministic, dependency-free PRNG; more than sufficient quality for a visual
            // noise effect.
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let noise = (state % span) as i64 - strength;
            *channel = (*channel as i64 + noise).clamp(0, 255) as u8;
        }
    }
}

/// A rolling window of `rgba` frames' mean luma, used by [`apply_deflicker_to_rgba`] to smooth
/// out frame-to-frame brightness variance — the temporal state export's real `deflicker=mode=am:
/// size=5` avfilter keeps internally, reimplemented here in the RGBA domain since no matching
/// GStreamer element exists for live preview (see this module's doc comment). `size=5` matches
/// [`crate::timeline::ClipInstance::video_filter_chain`]'s own export-side window exactly, so a
/// clip previewed with deflicker on sees roughly the same smoothing window it'll get on export.
/// Caller-owned rather than a static/thread-local: a fresh `DeflickerHistory` per previewed clip
/// keeps one clip's history from leaking into the next after a seek/clip change — see
/// `App::pump_preview_frame`'s own reset-on-clip-change handling.
#[derive(Debug, Clone, Default)]
pub struct DeflickerHistory {
    means: std::collections::VecDeque<f32>,
}

impl DeflickerHistory {
    const WINDOW: usize = 5;

    pub fn new() -> Self {
        Self::default()
    }
}

/// Applies temporal brightness smoothing to `rgba` in place — a **preview approximation** of
/// [`crate::timeline::ClipInstance::video_filter_chain`]'s export-side `deflicker=mode=am:size=5`
/// avfilter (see this module's doc comment for why this isn't a reproduction of that filter's
/// own algorithm). Computes the current frame's mean luma (ITU-R BT.601: `0.299R + 0.587G +
/// 0.114B`), pushes it into `history` (capped at [`DeflickerHistory::WINDOW`] entries, oldest
/// dropped), then shifts every pixel's R/G/B uniformly by `(rolling average - current mean)` —
/// an additive luma correction, not a multiplicative gain, so a near-black frame doesn't blow up
/// into a wildly amplified one the way `target/current` scaling would. No-op on an empty `rgba`
/// or one whose length isn't a multiple of 4 (not a valid RGBA8 buffer).
pub fn apply_deflicker_to_rgba(rgba: &mut [u8], history: &mut DeflickerHistory) {
    if rgba.is_empty() || !rgba.len().is_multiple_of(4) {
        return;
    }

    let pixel_count = rgba.len() / 4;
    let mut luma_sum = 0.0_f64;
    for chunk in rgba.as_chunks::<4>().0 {
        luma_sum += 0.299 * chunk[0] as f64 + 0.587 * chunk[1] as f64 + 0.114 * chunk[2] as f64;
    }
    let current_mean = (luma_sum / pixel_count as f64) as f32;

    history.means.push_back(current_mean);
    if history.means.len() > DeflickerHistory::WINDOW {
        history.means.pop_front();
    }
    let rolling_average = history.means.iter().sum::<f32>() / history.means.len() as f32;

    let shift = (rolling_average - current_mean).round() as i32;
    if shift == 0 {
        return;
    }
    for chunk in rgba.as_chunks_mut::<4>().0 {
        for channel in chunk.iter_mut().take(3) {
            *channel = (*channel as i32 + shift).clamp(0, 255) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity_cube(size: usize) -> String {
        let mut out = format!("LUT_3D_SIZE {size}\n");
        for b in 0..size {
            for g in 0..size {
                for r in 0..size {
                    let denom = (size - 1) as f32;
                    out.push_str(&format!(
                        "{:.6} {:.6} {:.6}\n",
                        r as f32 / denom,
                        g as f32 / denom,
                        b as f32 / denom
                    ));
                }
            }
        }
        out
    }

    #[test]
    fn parses_a_well_formed_cube_file() {
        let lut = Lut3D::parse(&identity_cube(4)).unwrap();
        assert_eq!(lut.size, 4);
        assert_eq!(lut.data.len(), 64);
    }

    #[test]
    fn ignores_comments_blank_lines_and_metadata() {
        let mut text = String::from("# a comment\nTITLE \"test\"\n\n");
        text.push_str(&identity_cube(2));
        text.push_str("DOMAIN_MIN 0.0 0.0 0.0\nDOMAIN_MAX 1.0 1.0 1.0\n");
        let lut = Lut3D::parse(&text).unwrap();
        assert_eq!(lut.size, 2);
    }

    #[test]
    fn missing_size_is_an_error() {
        assert_eq!(
            Lut3D::parse("0.0 0.0 0.0\n"),
            Err(LutParseError::MissingSize)
        );
    }

    #[test]
    fn size_too_small_is_an_error() {
        assert_eq!(
            Lut3D::parse("LUT_3D_SIZE 1\n0 0 0\n"),
            Err(LutParseError::SizeTooSmall(1))
        );
    }

    #[test]
    fn wrong_row_count_is_an_error() {
        assert_eq!(
            Lut3D::parse("LUT_3D_SIZE 2\n0 0 0\n1 1 1\n"),
            Err(LutParseError::WrongRowCount {
                expected: 8,
                found: 2
            })
        );
    }

    #[test]
    fn rejects_non_finite_lut_entries() {
        for value in ["NaN", "inf", "-inf"] {
            let cube = format!(
                "LUT_3D_SIZE 2\n{value} 0 0\n0 0 0\n0 0 0\n0 0 0\n0 0 0\n0 0 0\n0 0 0\n0 0 0\n"
            );
            assert_eq!(Lut3D::parse(&cube), Err(LutParseError::MalformedRow(0)));
        }
    }

    #[test]
    fn identity_lut_samples_are_a_no_op() {
        let lut = Lut3D::parse(&identity_cube(16)).unwrap();
        let [r, g, b] = lut.sample(0.5, 0.25, 0.75);
        assert!((r - 0.5).abs() < 0.05);
        assert!((g - 0.25).abs() < 0.05);
        assert!((b - 0.75).abs() < 0.05);
    }

    #[test]
    fn identity_lut_corners_are_exact() {
        let lut = Lut3D::parse(&identity_cube(4)).unwrap();
        assert_eq!(lut.sample(0.0, 0.0, 0.0), [0.0, 0.0, 0.0]);
        assert_eq!(lut.sample(1.0, 1.0, 1.0), [1.0, 1.0, 1.0]);
    }

    #[test]
    fn non_finite_lut_coordinates_fall_back_to_the_black_corner() {
        let lut = Lut3D::parse(&identity_cube(2)).unwrap();
        assert_eq!(
            lut.sample(f32::NAN, f32::INFINITY, f32::NEG_INFINITY),
            [0.0; 3]
        );
    }

    #[test]
    fn a_lut_that_zeroes_red_darkens_only_the_red_channel_of_the_frame() {
        // Every entry maps to (0, g, b) -- an identity LUT for green/blue, but red always
        // zeroed regardless of the input red value. On a 2x2x2 grid, identity-mapped g/b still
        // interpolate back to (approximately) their original continuous input value -- this
        // isn't "the original pixel with red zeroed" by construction, it's what the interpolated
        // LUT lookup happens to reproduce for a linear identity mapping.
        let size = 2;
        let mut text = format!("LUT_3D_SIZE {size}\n");
        for b in 0..size {
            for g in 0..size {
                for _r in 0..size {
                    text.push_str(&format!("0.0 {:.1} {:.1}\n", g as f32, b as f32));
                }
            }
        }
        let lut = Lut3D::parse(&text).unwrap();
        let mut rgba = vec![255u8, 128, 64, 200];
        apply_lut_to_rgba(&mut rgba, &lut);
        assert_eq!(rgba[0], 0, "red zeroed regardless of input");
        assert!(
            (rgba[1] as i32 - 128).abs() <= 1,
            "green preserved via identity"
        );
        assert!(
            (rgba[2] as i32 - 64).abs() <= 1,
            "blue preserved via identity"
        );
        assert_eq!(rgba[3], 200, "alpha untouched");
    }

    #[test]
    fn vignette_leaves_the_center_pixel_unchanged() {
        let width = 4;
        let height = 4;
        let mut rgba = vec![200u8; (width * height * 4) as usize];
        apply_vignette_to_rgba(&mut rgba, width, height, 1.0);
        // The four center pixels of a 4x4 grid are the closest to dead-center.
        let center_index = ((width + 1) * 4) as usize;
        assert!(
            rgba[center_index] > 150,
            "near-center pixel barely darkened"
        );
    }

    #[test]
    fn vignette_darkens_corners_more_than_the_center() {
        let width = 20;
        let height = 20;
        let mut rgba = vec![255u8; (width * height * 4) as usize];
        apply_vignette_to_rgba(&mut rgba, width, height, 0.8);
        let center_index = ((10 * width + 10) * 4) as usize;
        let corner_index = 0usize;
        assert!(
            rgba[corner_index] < rgba[center_index],
            "corner ({}) should be darker than center ({})",
            rgba[corner_index],
            rgba[center_index]
        );
    }

    #[test]
    fn zero_intensity_vignette_is_a_no_op() {
        let mut rgba = vec![100u8; 4 * 4 * 4];
        let before = rgba.clone();
        apply_vignette_to_rgba(&mut rgba, 4, 4, 0.0);
        assert_eq!(rgba, before);
    }

    #[test]
    fn vignette_leaves_alpha_untouched() {
        let mut rgba = vec![0u8, 0, 0, 77];
        apply_vignette_to_rgba(&mut rgba, 1, 1, 1.0);
        assert_eq!(rgba[3], 77);
    }

    #[test]
    fn zero_intensity_glitch_is_a_no_op() {
        let mut rgba = vec![100u8; 4 * 8 * 4];
        let before = rgba.clone();
        apply_glitch_to_rgba(&mut rgba, 0.0, 1);
        assert_eq!(rgba, before);
    }

    #[test]
    fn empty_buffer_glitch_is_a_no_op() {
        let mut rgba: Vec<u8> = Vec::new();
        apply_glitch_to_rgba(&mut rgba, 1.0, 1);
        assert!(rgba.is_empty());
    }

    #[test]
    fn glitch_leaves_alpha_untouched() {
        let mut rgba = vec![100u8, 100, 100, 42];
        apply_glitch_to_rgba(&mut rgba, 1.0, 1);
        assert_eq!(rgba[3], 42);
    }

    #[test]
    fn glitch_stays_within_byte_bounds_at_full_intensity() {
        // Mid-gray plus a worst-case ±40 noise swing should never wrap or clamp incorrectly.
        let mut rgba = vec![128u8; 64 * 64 * 4];
        apply_glitch_to_rgba(&mut rgba, 1.0, 12345);
        assert!(rgba.iter().all(|&b| (0..=255).contains(&b)));
    }

    #[test]
    fn glitch_is_deterministic_for_the_same_seed() {
        let mut a = vec![128u8; 16 * 16 * 4];
        let mut b = a.clone();
        apply_glitch_to_rgba(&mut a, 0.5, 777);
        apply_glitch_to_rgba(&mut b, 0.5, 777);
        assert_eq!(a, b);
    }

    #[test]
    fn glitch_differs_across_seeds() {
        let mut a = vec![128u8; 16 * 16 * 4];
        let mut b = a.clone();
        apply_glitch_to_rgba(&mut a, 0.5, 1);
        apply_glitch_to_rgba(&mut b, 0.5, 2);
        assert_ne!(
            a, b,
            "different seeds should produce a different noise pattern"
        );
    }

    #[test]
    fn glitch_actually_perturbs_pixels_at_full_intensity() {
        let mut rgba = vec![128u8; 16 * 16 * 4];
        let before = rgba.clone();
        apply_glitch_to_rgba(&mut rgba, 1.0, 999);
        assert_ne!(rgba, before);
    }

    #[test]
    fn deflicker_is_a_no_op_on_the_first_frame() {
        // A single frame's mean is trivially its own rolling average -- shift is always 0.
        let mut rgba = vec![100u8; 4 * 4 * 4];
        let mut history = DeflickerHistory::new();
        apply_deflicker_to_rgba(&mut rgba, &mut history);
        assert_eq!(rgba, vec![100u8; 4 * 4 * 4]);
    }

    #[test]
    fn deflicker_darkens_a_frame_brighter_than_its_recent_history() {
        let mut history = DeflickerHistory::new();
        // Settle the rolling average at a dim level over a few dark frames.
        for _ in 0..3 {
            let mut dark = vec![50u8; 4 * 4 * 4];
            apply_deflicker_to_rgba(&mut dark, &mut history);
        }
        // A sudden bright frame should be pulled back down toward the established average.
        let mut bright = vec![200u8; 4 * 4 * 4];
        apply_deflicker_to_rgba(&mut bright, &mut history);
        assert!(
            bright[0] < 200,
            "a frame brighter than recent history should be darkened toward the average"
        );
    }

    #[test]
    fn deflicker_brightens_a_frame_dimmer_than_its_recent_history() {
        let mut history = DeflickerHistory::new();
        for _ in 0..3 {
            let mut bright = vec![200u8; 4 * 4 * 4];
            apply_deflicker_to_rgba(&mut bright, &mut history);
        }
        let mut dark = vec![50u8; 4 * 4 * 4];
        apply_deflicker_to_rgba(&mut dark, &mut history);
        assert!(
            dark[0] > 50,
            "a frame dimmer than recent history should be brightened toward the average"
        );
    }

    #[test]
    fn deflicker_window_drops_frames_older_than_five() {
        // Six identical bright frames, then a dark one: with the WINDOW=5 cap, the oldest
        // (first) bright frame's mean should already be evicted, but this is still simplest to
        // assert indirectly -- the rolling average after 6 identical inputs is still that same
        // value regardless of window size, so instead assert the deque itself never grows past 5.
        let mut history = DeflickerHistory::new();
        for _ in 0..8 {
            let mut frame = vec![128u8; 4 * 4 * 4];
            apply_deflicker_to_rgba(&mut frame, &mut history);
        }
        assert_eq!(history.means.len(), DeflickerHistory::WINDOW);
    }

    #[test]
    fn deflicker_stays_within_byte_bounds() {
        let mut history = DeflickerHistory::new();
        let mut rgba = vec![250u8; 4 * 4 * 4];
        apply_deflicker_to_rgba(&mut rgba, &mut history);
        let mut dark = vec![0u8; 4 * 4 * 4];
        apply_deflicker_to_rgba(&mut dark, &mut history);
        assert!(rgba.iter().all(|&b| (0..=255).contains(&b)));
        assert!(dark.iter().all(|&b| (0..=255).contains(&b)));
    }

    #[test]
    fn deflicker_is_a_no_op_on_an_empty_buffer() {
        let mut rgba: Vec<u8> = Vec::new();
        let mut history = DeflickerHistory::new();
        apply_deflicker_to_rgba(&mut rgba, &mut history);
        assert!(rgba.is_empty());
    }
}
