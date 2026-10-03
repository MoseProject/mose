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

//! CPU reimplementation of FFmpeg's `blend` filter's 8-bit `all_mode` formulas
//! ([`crate::timeline::BlendMode`]), for live-preview compositing where GStreamer's
//! `compositor` element (only `source`/`over`/`add` Porter-Duff operators — verified via
//! `gst-inspect-1.0 compositor` against this project's GStreamer) can't do the job
//! (the old GStreamer preview built a dedicated `appsink` branch per blend-mode layer instead and
//! composites its frames against the accumulated stack here, per-frame).
//!
//! Every formula below is transcribed term-for-term from FFmpeg's own source
//! (`libavfilter/blend_modes.c`, `DEPTH == 8` branch, `n6.1.1`) rather than reimplemented from
//! documentation or memory, and cross-checked against real `ffmpeg -f lavfi ... blend=all_mode=`
//! output on 256x256 gradient test images (every mode: max abs diff 0, except `interpolate`
//! which is 1 — float rounding) before being trusted here. FFmpeg's macro names are kept as
//! local helper functions (`multiply`, `screen`, `burn`, `dodge`, `geometric`) to keep the
//! mapping obvious; `A`/`B` in FFmpeg's source are `top`/`bottom` respectively — this crate's own
//! doc comments elsewhere establish the same top/bottom convention (a blend-mode layer is "top",
//! blending onto the accumulated stack beneath it as "bottom").
//!
//! FFmpeg's formulas are written in C `int` arithmetic and *rely on unsigned-narrowing wraparound
//! on assignment to `uint8_t`* for a few modes that are not explicitly clamped in the source
//! (`bleach`, `stain`) — e.g. `stain`'s `2*MAX - A - B` can reach 510 and wraps to `254`,
//! `(510 as u8)` in two's-complement truncation, not a clamp to 255. [`blend_channel`] reproduces
//! this exactly via `as u8` (Rust's `i32 -> u8` cast is the same bit-truncating conversion C's
//! implicit unsigned-narrowing assignment performs), rather than clamping — clamping would silently
//! diverge from what export (FFmpeg's own filter) actually produces for those two modes.

use crate::timeline::BlendMode;

const MAX: i32 = 255;
const HALF: i32 = 128;

fn multiply(x: i32, a: i32, b: i32) -> i32 {
    x * ((a * b) / MAX)
}

fn screen(x: i32, a: i32, b: i32) -> i32 {
    MAX - x * ((MAX - a) * (MAX - b) / MAX)
}

fn burn(a: i32, b: i32) -> i32 {
    if a == 0 {
        a
    } else {
        0.max(MAX - ((MAX - b) << 8) / a)
    }
}

fn dodge(a: i32, b: i32) -> i32 {
    if a == MAX {
        a
    } else {
        MAX.min((b << 8) / (MAX - a))
    }
}

fn geometric(a: i32, b: i32) -> i32 {
    ((a.max(0) as f64 * b.max(0) as f64).sqrt()).round() as i32
}

/// Blends one 0-255 channel value (`top` composited onto `bottom`) per `mode`'s FFmpeg formula.
/// `mode` must not be [`BlendMode::Normal`] — callers gate on [`crate::timeline::ClipInstance::has_blend_mode`]
/// before reaching for this, same as `ffmpeg_name`'s own doc comment requires.
pub fn blend_channel(mode: BlendMode, top: u8, bottom: u8) -> u8 {
    let a = top as i32;
    let b = bottom as i32;
    let result = match mode {
        BlendMode::Normal => a,
        BlendMode::Addition => MAX.min(a + b),
        BlendMode::And => a & b,
        BlendMode::Average => (a + b) / 2,
        BlendMode::Burn => burn(a, b),
        BlendMode::Darken => a.min(b),
        BlendMode::Difference => (a - b).abs(),
        BlendMode::GrainExtract => (HALF + a - b).clamp(0, MAX),
        BlendMode::Divide => {
            if b == 0 {
                MAX
            } else {
                (MAX * a / b).clamp(0, MAX)
            }
        }
        BlendMode::Dodge => dodge(a, b),
        BlendMode::Exclusion => a + b - 2 * a * b / MAX,
        BlendMode::HardLight => {
            if b < HALF {
                multiply(2, b, a)
            } else {
                screen(2, b, a)
            }
        }
        BlendMode::Lighten => a.max(b),
        BlendMode::Multiply => multiply(1, a, b),
        BlendMode::Negation => MAX - (MAX - a - b).abs(),
        BlendMode::Or => a | b,
        BlendMode::Overlay => {
            if a < HALF {
                multiply(2, a, b)
            } else {
                screen(2, a, b)
            }
        }
        BlendMode::Phoenix => a.min(b) - a.max(b) + MAX,
        BlendMode::PinLight => {
            if b < HALF {
                a.min(2 * b)
            } else {
                a.max(2 * (b - HALF))
            }
        }
        BlendMode::Reflect => {
            if b == MAX {
                b
            } else {
                MAX.min(a * a / (MAX - b))
            }
        }
        BlendMode::Screen => screen(1, a, b),
        BlendMode::SoftLight => {
            (a * a / MAX + (2 * (b * ((a * (MAX - a)) / MAX) / MAX))).clamp(0, MAX)
        }
        BlendMode::Subtract => 0.max(a - b),
        BlendMode::VividLight => {
            if a < HALF {
                burn(2 * a, b)
            } else {
                dodge(2 * (a - HALF), b)
            }
        }
        BlendMode::Xor => a ^ b,
        BlendMode::HardMix => {
            if a < (MAX - b) {
                0
            } else {
                MAX
            }
        }
        BlendMode::LinearLight => {
            if b < HALF {
                b + 2 * a - MAX
            } else {
                b + 2 * (a - HALF)
            }
        }
        .clamp(0, MAX),
        BlendMode::Glow => {
            if a == MAX {
                a
            } else {
                MAX.min(b * b / (MAX - a))
            }
        }
        BlendMode::GrainMerge => (a + b - HALF).clamp(0, MAX),
        BlendMode::Multiply128 => ((a - HALF) * b / 32 + HALF).clamp(0, MAX),
        BlendMode::Heat => {
            if a == 0 {
                0
            } else {
                MAX - (((MAX - b) * (MAX - b)) / a).min(MAX)
            }
        }
        BlendMode::Freeze => {
            if b == 0 {
                0
            } else {
                MAX - (((MAX - a) * (MAX - a)) / b).min(MAX)
            }
        }
        BlendMode::Extremity => (MAX - a - b).abs(),
        BlendMode::SoftDifference => {
            if a > b {
                if b == MAX {
                    0
                } else {
                    (a - b) * MAX / (MAX - b)
                }
            } else if b == 0 {
                0
            } else {
                (b - a) * MAX / b
            }
        }
        .clamp(0, MAX),
        BlendMode::Geometric => geometric(a, b),
        BlendMode::Harmonic => {
            if a == 0 && b == 0 {
                0
            } else {
                2 * a * b / (a + b)
            }
        }
        BlendMode::Bleach => (MAX - b) + (MAX - a) - MAX,
        BlendMode::Stain => 2 * MAX - a - b,
        BlendMode::Interpolate => {
            let a = a as f64;
            let b = b as f64;
            (MAX as f64
                * (2.0
                    - (a * std::f64::consts::PI / MAX as f64).cos()
                    - (b * std::f64::consts::PI / MAX as f64).cos())
                * 0.25)
                .round() as i32
        }
        BlendMode::HardOverlay => {
            if a == MAX {
                MAX
            } else {
                let hi = if a > HALF {
                    MAX * b / (2 * MAX - 2 * a)
                } else {
                    0
                };
                let lo = if a <= HALF { 2 * a * b / MAX } else { 0 };
                MAX.min(hi + lo)
            }
        }
    };
    // Reproduces C's implicit narrowing conversion to `uint8_t` bit-for-bit, wraparound included
    // (needed for `Bleach`/`Stain`, see module doc comment) — never clamp here.
    result as u8
}

#[cfg(test)]
#[path = "blend_mode_test.rs"]
mod tests;
