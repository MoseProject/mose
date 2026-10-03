//! 8-bit blend functions that reproduce FFmpeg's `blend` filter exactly.
//!
//! The export path composites with `blend=all_mode=<name>` on 8-bit planes, so a preview that
//! shares those integer formulas matches the export bit for bit on opaque pixels. Every function
//! below was fitted against tables measured from a real FFmpeg build (all 65536 input pairs per
//! mode): `a` is the blended layer (the filter's first, "top" input) and `b` the backdrop (the
//! second, "bottom" input). Note FFmpeg's naming differs from Photoshop's: its `overlay` tests
//! the top layer and its `hardlight` tests the backdrop.
//!
//! `softlight` has no closed form that reproduces FFmpeg's rounding, so it is a measured table.
//! `interpolate` is within one level of FFmpeg on 0.16 % of input pairs (float rounding) and
//! `hardoverlay` on one input pair out of 65536.

use crate::BlendMode;

/// FFmpeg's `softlight` for every `(a, b)`, row-major with `a` as the column: `table[b * 256 + a]`.
const SOFTLIGHT: &[u8; 65536] = include_bytes!("blend_softlight.bin");

fn clip(value: i32) -> u8 {
    value.clamp(0, 255) as u8
}

fn multiply(scale: i32, a: i32, b: i32) -> i32 {
    scale * (a * b / 255)
}

fn screen(scale: i32, a: i32, b: i32) -> i32 {
    255 - scale * ((255 - a) * (255 - b) / 255)
}

fn burn(a: i32, b: i32) -> i32 {
    if a == 0 {
        a
    } else {
        (255 - ((255 - b) << 8) / a).max(0)
    }
}

fn dodge(a: i32, b: i32) -> i32 {
    if a == 255 {
        a
    } else {
        ((b << 8) / (255 - a)).min(255)
    }
}

/// Whether `mode` is computed by [`blend8`]: every mode except `Normal`, which has no blend
/// function (the source replaces the backdrop).
pub(crate) fn uses_integer_domain(mode: BlendMode) -> bool {
    mode != BlendMode::Normal
}

/// The blended channel for source byte `a` over backdrop byte `b`.
pub(crate) fn blend8(mode: BlendMode, a: u8, b: u8) -> u8 {
    let (ia, ib) = (i32::from(a), i32::from(b));
    match mode {
        BlendMode::Multiply => (ia * ib / 255) as u8,
        BlendMode::Screen => (255 - (255 - ia) * (255 - ib) / 255) as u8,
        BlendMode::Darken => a.min(b),
        BlendMode::Lighten => a.max(b),
        BlendMode::Add => clip(ia + ib),
        BlendMode::Difference => (ia - ib).unsigned_abs() as u8,
        BlendMode::And => a & b,
        BlendMode::Or => a | b,
        BlendMode::Xor => a ^ b,
        BlendMode::Average => ((ia + ib) / 2) as u8,
        BlendMode::Burn => clip(burn(ia, ib)),
        BlendMode::Dodge => clip(dodge(ia, ib)),
        BlendMode::GrainExtract => clip(ia - ib + 128),
        BlendMode::GrainMerge => clip(ia + ib - 128),
        BlendMode::Divide => {
            if ib == 0 {
                255
            } else {
                clip(255 * ia / ib)
            }
        }
        BlendMode::Exclusion => clip(ia + ib - 2 * ia * ib / 255),
        BlendMode::HardLight => {
            if ib < 128 {
                clip(multiply(2, ia, ib))
            } else {
                clip(screen(2, ia, ib))
            }
        }
        BlendMode::Overlay => {
            if ia < 128 {
                clip(multiply(2, ib, ia))
            } else {
                clip(screen(2, ib, ia))
            }
        }
        BlendMode::HardMix => {
            if ia >= 255 - ib {
                255
            } else {
                0
            }
        }
        BlendMode::LinearLight => {
            if ib < 128 {
                clip(ib + 2 * ia - 255)
            } else {
                clip(ib + 2 * (ia - 128))
            }
        }
        BlendMode::Negation => (255 - (255 - ia - ib).abs()) as u8,
        BlendMode::Phoenix => (ia.min(ib) - ia.max(ib) + 255) as u8,
        BlendMode::PinLight => {
            if ib < 128 {
                clip(ia.min(2 * ib))
            } else {
                clip(ia.max(2 * (ib - 128)))
            }
        }
        BlendMode::Reflect => {
            if ib == 255 {
                255
            } else {
                clip((ia * ia / (255 - ib)).min(255))
            }
        }
        BlendMode::Glow => {
            if ia == 255 {
                255
            } else {
                clip((ib * ib / (255 - ia)).min(255))
            }
        }
        BlendMode::Subtract => clip(ia - ib),
        BlendMode::VividLight => {
            if ia < 128 {
                clip(burn(2 * ia, ib))
            } else {
                clip(dodge(2 * (ia - 128), ib))
            }
        }
        BlendMode::Multiply128 => {
            let value = (ia - 128) * ib;
            clip(value.div_euclid(32) + 128)
        }
        BlendMode::Heat => {
            if ia == 0 {
                0
            } else {
                clip(255 - ((255 - ib) * (255 - ib) / ia).min(255))
            }
        }
        BlendMode::Freeze => {
            if ib == 0 {
                0
            } else {
                clip(255 - ((255 - ia) * (255 - ia) / ib).min(255))
            }
        }
        BlendMode::Extremity => (255 - ia - ib).unsigned_abs() as u8,
        BlendMode::SoftDifference => {
            if ia > ib {
                // `ia <= 255`, so `ib < 255` here and the divisor is never zero.
                clip(((ia - ib) * 255 / (255 - ib)).min(255))
            } else if ib == 0 {
                0
            } else {
                clip(((ib - ia) * 255 / ib).min(255))
            }
        }
        BlendMode::Geometric => (f64::from(ia * ib).sqrt()).round() as u8,
        BlendMode::Harmonic => {
            if ia == 0 && ib == 0 {
                0
            } else {
                (2 * ia * ib / (ia + ib)) as u8
            }
        }
        BlendMode::Bleach => ((255 - ia - ib) & 255) as u8,
        BlendMode::Stain => ((254 - ia - ib) & 255) as u8,
        BlendMode::Interpolate => {
            let pi = std::f64::consts::PI;
            let value = 255.0
                * (0.5
                    - 0.25 * (pi * f64::from(a) / 255.0).cos()
                    - 0.25 * (pi * f64::from(b) / 255.0).cos());
            clip(value.round() as i32)
        }
        BlendMode::HardOverlay => {
            if ia < 128 {
                clip(2 * ia * ib / 255)
            } else if ia == 255 {
                255
            } else {
                clip((ib * 255 / (2 * (255 - ia))).min(255))
            }
        }
        BlendMode::SoftLight => SOFTLIGHT[usize::from(b) * 256 + usize::from(a)],
        BlendMode::Normal => a,
    }
}

/// The measured softlight table, for uploading to the GPU renderer.
#[cfg_attr(not(feature = "gpu"), allow(dead_code))]
pub(crate) fn softlight_table() -> &'static [u8; 65536] {
    SOFTLIGHT
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_softlight_table_is_a_full_256_by_256_lookup() {
        let table = super::softlight_table();
        assert_eq!(table.len(), 256 * 256);
        // Measured from FFmpeg: mid-grey over mid-grey comes out two levels darker.
        assert_eq!(table[128 * 256 + 128], 126);
    }
}
