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

//! A clip's colour grade: exposure, temperature and tint, lift/gamma/gain and shadows/highlights.
//!
//! The grade is one per-pixel colour function, so it is **baked into a 3D lookup table** that the
//! preview and the export both apply through their existing LUT stage (`lut3d` in the export,
//! trilinear sampling in Bush). One formula, one table: the two cannot drift apart, and the
//! export needs no filter per tool.

use serde::{Deserialize, Serialize};

use crate::graphics::lut::Lut3D;

/// Entries per axis of a baked grade.
pub const GRADE_LUT_SIZE: usize = 33;

/// Largest exposure move in stops, either way.
pub const MAX_EXPOSURE_STOPS: f32 = 4.0;

/// A clip's grade. Neutral by default. **Fields are positional in the project file**: a new one
/// is a format change.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Grade {
    /// Exposure in stops (`-4.0..=4.0`), a gain of `2^stops` in linear light.
    pub exposure: f32,
    /// Colour temperature, `-1.0` (cooler, bluer) to `1.0` (warmer, redder).
    pub temperature: f32,
    /// Tint, `-1.0` (greener) to `1.0` (more magenta).
    pub tint: f32,
    /// Lift (blacks), gamma (mids) and gain (whites), per red, green and blue. Neutral is lift
    /// `0`, gamma `1`, gain `1`.
    pub lift: [f32; 3],
    pub gamma: [f32; 3],
    pub gain: [f32; 3],
    /// Brightens (`> 0`) or darkens the dark part of the picture, `-1.0..=1.0`.
    pub shadows: f32,
    /// Brightens (`> 0`) or darkens the bright part of the picture, `-1.0..=1.0`.
    pub highlights: f32,
}

impl Default for Grade {
    fn default() -> Self {
        Self {
            exposure: 0.0,
            temperature: 0.0,
            tint: 0.0,
            lift: [0.0; 3],
            gamma: [1.0; 3],
            gain: [1.0; 3],
            shadows: 0.0,
            highlights: 0.0,
        }
    }
}

/// The transfer function the grade's light-space steps use (a plain power law, close enough to
/// the display curve for a creative grade).
const GAMMA: f32 = 2.4;

impl Grade {
    /// Whether the grade changes nothing, so no table is needed.
    pub fn is_neutral(&self) -> bool {
        *self == Self::default()
    }

    /// The grade inside its limits; a value that is not a number becomes neutral.
    pub fn clamped(self) -> Self {
        let within = |v: f32, low: f32, high: f32, neutral: f32| {
            if v.is_finite() {
                v.clamp(low, high)
            } else {
                neutral
            }
        };
        let triple = |values: [f32; 3], low: f32, high: f32, neutral: f32| {
            values.map(|v| within(v, low, high, neutral))
        };
        Self {
            exposure: within(self.exposure, -MAX_EXPOSURE_STOPS, MAX_EXPOSURE_STOPS, 0.0),
            temperature: within(self.temperature, -1.0, 1.0, 0.0),
            tint: within(self.tint, -1.0, 1.0, 0.0),
            lift: triple(self.lift, -0.5, 0.5, 0.0),
            gamma: triple(self.gamma, 0.2, 5.0, 1.0),
            gain: triple(self.gain, 0.0, 4.0, 1.0),
            shadows: within(self.shadows, -1.0, 1.0, 0.0),
            highlights: within(self.highlights, -1.0, 1.0, 0.0),
        }
    }

    /// Maps one display-referred RGB colour (`0.0..=1.0` per channel) through the grade.
    pub fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {
        let mut v = rgb.map(|c| c.clamp(0.0, 1.0));

        // Exposure, temperature and tint multiply in linear light.
        let linear_gain = [
            (1.0 + 0.3 * self.temperature),
            (1.0 - 0.3 * self.tint),
            (1.0 - 0.3 * self.temperature),
        ];
        let stops = 2f32.powf(self.exposure);
        for (channel, balance) in v.iter_mut().zip(linear_gain) {
            let linear = channel.powf(GAMMA) * stops * balance.max(0.0);
            *channel = linear.max(0.0).powf(1.0 / GAMMA).min(1.0);
        }

        // Lift, gamma and gain: lift raises the blacks and fades out towards white, gain scales,
        // gamma bends the mids.
        for (i, channel) in v.iter_mut().enumerate() {
            let lifted = *channel * self.gain[i] + self.lift[i] * (1.0 - *channel);
            *channel = lifted.max(0.0).powf(1.0 / self.gamma[i].max(0.05)).min(1.0);
        }

        // Shadows and highlights move the dark and the bright part by a luma-weighted amount.
        if self.shadows != 0.0 || self.highlights != 0.0 {
            let luma = 0.2126 * v[0] + 0.7152 * v[1] + 0.0722 * v[2];
            let shadow_weight = (1.0 - luma).powi(2);
            let highlight_weight = luma.powi(2);
            let shift = 0.25 * (self.shadows * shadow_weight + self.highlights * highlight_weight);
            for channel in &mut v {
                *channel = (*channel + shift).clamp(0.0, 1.0);
            }
        }
        v
    }

    /// The grade as a lookup table of [`GRADE_LUT_SIZE`] entries per axis. With `base` (the clip's
    /// own LUT) the grade runs after it: `grade(base(x))`.
    pub fn bake(&self, base: Option<&Lut3D>) -> Lut3D {
        let size = GRADE_LUT_SIZE;
        let step = 1.0 / (size - 1) as f32;
        let mut entries = Vec::with_capacity(size * size * size);
        for b in 0..size {
            for g in 0..size {
                for r in 0..size {
                    let input = [r as f32 * step, g as f32 * step, b as f32 * step];
                    let after_base = match base {
                        Some(lut) => lut.sample(input[0], input[1], input[2]),
                        None => input,
                    };
                    entries.push(self.apply(after_base));
                }
            }
        }
        Lut3D::new(size, entries).expect("a baked grade has size^3 entries")
    }
}
