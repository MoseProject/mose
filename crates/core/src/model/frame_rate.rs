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

//! The frame rate of a project's timeline, as an exact fraction so 23.976, 29.97 and 59.94 are
//! 24000/1001, 30000/1001 and 60000/1001 rather than approximations.
//!
//! Times in the editor are still seconds as `f64`. A frame boundary `n * den / num` is within
//! one part in 10^15 of exact, and the frame number is always recovered by rounding, so snapping
//! an edit to a boundary ([`FrameRate::snap_secs`]) makes cuts land on whole frames without
//! changing how time is stored.

use serde::{Deserialize, Serialize};

/// Frames per second as `num / den`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameRate {
    pub num: u32,
    pub den: u32,
}

impl FrameRate {
    pub const FPS_23_976: Self = Self::new(24_000, 1_001);
    pub const FPS_24: Self = Self::new(24, 1);
    pub const FPS_25: Self = Self::new(25, 1);
    pub const FPS_29_97: Self = Self::new(30_000, 1_001);
    pub const FPS_30: Self = Self::new(30, 1);
    pub const FPS_50: Self = Self::new(50, 1);
    pub const FPS_59_94: Self = Self::new(60_000, 1_001);
    pub const FPS_60: Self = Self::new(60, 1);

    /// The rates the editor offers, in the order a picker shows them.
    pub const COMMON: [Self; 8] = [
        Self::FPS_23_976,
        Self::FPS_24,
        Self::FPS_25,
        Self::FPS_29_97,
        Self::FPS_30,
        Self::FPS_50,
        Self::FPS_59_94,
        Self::FPS_60,
    ];

    pub const fn new(num: u32, den: u32) -> Self {
        Self { num, den }
    }

    /// False for a zero numerator or denominator, which no timeline can use.
    pub const fn is_valid(self) -> bool {
        self.num > 0 && self.den > 0
    }

    pub fn fps(self) -> f64 {
        f64::from(self.num) / f64::from(self.den)
    }

    /// How long one frame lasts, in seconds.
    pub fn frame_secs(self) -> f64 {
        f64::from(self.den) / f64::from(self.num)
    }

    /// The whole frame number nearest to `secs`.
    pub fn frame_at(self, secs: f64) -> i64 {
        (secs * f64::from(self.num) / f64::from(self.den)).round() as i64
    }

    /// The time at which frame `frame` starts.
    pub fn secs_of(self, frame: i64) -> f64 {
        frame as f64 * f64::from(self.den) / f64::from(self.num)
    }

    /// `secs` moved to the nearest frame boundary. Non-finite input is returned unchanged.
    pub fn snap_secs(self, secs: f64) -> f64 {
        if !secs.is_finite() {
            return secs;
        }
        self.secs_of(self.frame_at(secs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_boundary_maps_to_its_own_frame_number_at_every_common_rate() {
        for rate in FrameRate::COMMON {
            for frame in [0, 1, 2, 29, 30, 1_000, 107_892, 1_000_000] {
                assert_eq!(
                    rate.frame_at(rate.secs_of(frame)),
                    frame,
                    "{rate:?} frame {frame}"
                );
            }
        }
    }

    #[test]
    fn snapping_is_idempotent() {
        for rate in FrameRate::COMMON {
            for secs in [0.0, 0.013, 1.0, 12.3456, 3_599.999, 7_200.5] {
                let once = rate.snap_secs(secs);
                assert_eq!(rate.snap_secs(once), once, "{rate:?} at {secs}");
            }
        }
    }

    #[test]
    fn a_time_snaps_to_the_nearest_boundary_never_further_than_half_a_frame() {
        for rate in FrameRate::COMMON {
            for secs in [0.001, 0.5, 1.2345, 59.97, 1234.5678] {
                let snapped = rate.snap_secs(secs);
                assert!(
                    (snapped - secs).abs() <= rate.frame_secs() / 2.0 + 1e-9,
                    "{rate:?} moved {secs} to {snapped}"
                );
            }
        }
    }

    #[test]
    fn ntsc_rates_do_not_drift_over_a_long_edit() {
        let rate = FrameRate::FPS_29_97;
        // An hour of non-drop timecode is 108,000 frames, which last 3,603.6 s at 29.97.
        let hour = rate.secs_of(108_000);
        assert!((hour - 3_603.6).abs() < 1e-9, "{hour}");
        assert_eq!(rate.frame_at(hour), 108_000);
        assert_eq!(rate.snap_secs(hour + 0.0001), hour);
    }

    #[test]
    fn halfway_rounds_away_from_zero_and_negative_times_snap_too() {
        let rate = FrameRate::FPS_25;
        assert_eq!(rate.frame_at(0.02), 1, "exactly one frame at 25 fps");
        assert!((rate.snap_secs(-0.04) + 0.04).abs() < 1e-12);
        assert_eq!(rate.frame_at(-0.1), -3, "-2.5 frames rounds away from zero");
    }

    #[test]
    fn non_finite_times_are_left_alone() {
        let rate = FrameRate::FPS_30;
        assert!(rate.snap_secs(f64::NAN).is_nan());
        assert_eq!(rate.snap_secs(f64::INFINITY), f64::INFINITY);
    }

    #[test]
    fn a_zero_component_is_invalid() {
        assert!(!FrameRate::new(0, 1).is_valid());
        assert!(!FrameRate::new(30, 0).is_valid());
        assert!(FrameRate::FPS_30.is_valid());
    }
}
