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

//! 3D colour lookup tables in the Adobe `.cube` format: parsing, loading and trilinear
//! sampling. [`crate::timeline::ClipInstance::lut_path`] stores a path to one; the preview
//! converts the parsed table for Bush and export reads the file through FFmpeg.

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

    /// Entries per axis.
    pub fn size(&self) -> usize {
        self.size
    }

    /// The table, red varying fastest, then green, then blue (the `.cube` data order).
    pub fn entries(&self) -> &[[f32; 3]] {
        &self.data
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
}
