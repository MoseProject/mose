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

//! A general keyframe system: position, scale, rotation, and opacity can each be animated by
//! a list of time-anchored points on a [`crate::timeline::ClipInstance`], with the value
//! interpolated automatically between them — per `features/request.md`'s Fase 4 "Keyframes"
//! spec. Replaces the old `ClipInstance::zoom_start`/`zoom_end` two-endpoint special case (its
//! own doc comment already called itself "not a general keyframe system, that's future work") —
//! a 2-point [`Keyframe<f32>`] list on [`crate::timeline::ClipInstance::scale_keyframes`]
//! reproduces the same Ken-Burns behavior as a degenerate case.
//!
//! This pass wires keyframes into **export** only (single-track and multi-track/overlay avfilter
//! compilation below) — live GStreamer preview isn't animated yet, the same "export first,
//! preview wired later" shape several other effects in this codebase already have.
//! Position/opacity only have a visible effect on an **overlay-track** clip, not a
//! single/background track — the background track's final `format=yuv420p` conform drops the
//! alpha plane they need, the same pre-existing caveat `mask_shape`/`chroma_key` already have.

use serde::{Deserialize, Serialize};

/// A single time-value point in a keyframed property animation. `time_fraction` is relative to
/// the clip's own current playable duration (`0.0` = clip start, `1.0` = clip end), not
/// absolute source or timeline time — so a keyframe stays meaningful (e.g. "80% through this
/// clip") across trim edits without needing to be rescaled by every edit that changes the
/// clip's boundaries. Matches the normalized `0.0..=1.0` convention already used elsewhere in
/// this codebase (`ClipInstance::crop_x`/`crop_y`, `TextClip::pos_x`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Keyframe<T> {
    pub time_fraction: f32,
    pub value: T,
}

/// A normalized (fraction of canvas width/height) 2D offset, for position keyframes — named
/// fields rather than a tuple so `.ocproj`'s MessagePack struct-map encoding keeps them
/// self-describing, consistent with every other field in this codebase.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub x: f32,
    pub y: f32,
}

/// Linear interpolation between two values of the same type, at `t` (`0.0..=1.0`).
pub trait Lerp {
    fn lerp(self, other: Self, t: f32) -> Self;
}

impl Lerp for f32 {
    fn lerp(self, other: f32, t: f32) -> f32 {
        self + (other - self) * t
    }
}

impl Lerp for Position {
    fn lerp(self, other: Position, t: f32) -> Position {
        Position {
            x: self.x.lerp(other.x, t),
            y: self.y.lerp(other.y, t),
        }
    }
}

/// Evaluates a piecewise-linear keyframe animation at `time_fraction` (`0.0..=1.0`): 0
/// keyframes -> `default`; 1 keyframe -> that constant value regardless of its own
/// `time_fraction`; 2+ -> linear interpolation between the two keyframes surrounding
/// `time_fraction`, holding the nearest endpoint's value outside the keyframed range. Assumes
/// `keyframes` is sorted ascending by `time_fraction` — callers/mutators are responsible for
/// that, the same way this codebase keeps invariants at the mutation site rather than
/// re-checking on every read.
pub fn evaluate_keyframes<T: Lerp + Copy>(
    keyframes: &[Keyframe<T>],
    time_fraction: f32,
    default: T,
) -> T {
    match keyframes.len() {
        0 => default,
        1 => keyframes[0].value,
        _ => {
            let last = keyframes.len() - 1;
            if time_fraction <= keyframes[0].time_fraction {
                return keyframes[0].value;
            }
            if time_fraction >= keyframes[last].time_fraction {
                return keyframes[last].value;
            }
            for w in keyframes.windows(2) {
                let (a, b) = (w[0], w[1]);
                if time_fraction >= a.time_fraction && time_fraction <= b.time_fraction {
                    let span = b.time_fraction - a.time_fraction;
                    let t = if span > 1e-6 {
                        (time_fraction - a.time_fraction) / span
                    } else {
                        0.0
                    };
                    return a.value.lerp(b.value, t);
                }
            }
            keyframes[last].value
        }
    }
}

/// Splits a keyframe list at `frac` (relative to the *original* clip's duration) into two
/// lists, each rescaled to `0.0..=1.0` over its own half's new duration, with a synthetic point
/// inserted at the split boundary on both halves (interpolated via [`evaluate_keyframes`]) so
/// the animation has no visual jump right at the cut. Fixes a real gap the old
/// `zoom_start`/`zoom_end` split behavior had (`Track::split_clip_at` used to copy the same
/// `zoom_start`/`zoom_end` onto both halves unscaled) — this does it properly instead of
/// carrying that bug forward into the new fields. Returns `(vec![], vec![])` if `keyframes` is
/// empty (nothing to split).
pub fn split_keyframes_at<T: Lerp + Copy>(
    keyframes: &[Keyframe<T>],
    frac: f32,
    default: T,
) -> (Vec<Keyframe<T>>, Vec<Keyframe<T>>) {
    if keyframes.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let frac = frac.clamp(0.0, 1.0);
    let boundary_value = evaluate_keyframes(keyframes, frac, default);

    let mut first: Vec<Keyframe<T>> = keyframes
        .iter()
        .filter(|k| k.time_fraction < frac)
        .map(|k| Keyframe {
            time_fraction: if frac > 1e-6 {
                (k.time_fraction / frac).min(1.0)
            } else {
                0.0
            },
            value: k.value,
        })
        .collect();
    first.push(Keyframe {
        time_fraction: 1.0,
        value: boundary_value,
    });

    let remaining_span = 1.0 - frac;
    let mut second: Vec<Keyframe<T>> = keyframes
        .iter()
        .filter(|k| k.time_fraction > frac)
        .map(|k| Keyframe {
            time_fraction: if remaining_span > 1e-6 {
                ((k.time_fraction - frac) / remaining_span).clamp(0.0, 1.0)
            } else {
                1.0
            },
            value: k.value,
        })
        .collect();
    second.insert(
        0,
        Keyframe {
            time_fraction: 0.0,
            value: boundary_value,
        },
    );

    (first, second)
}

/// Timeline-elapsed duration of a smooth speed ramp (`spec/ROADMAP.md` P4 item 29's "smooth
/// continuous curve" follow-up) over `source_duration_secs` of source time, with speed varying
/// *linearly* in source time from `start_speed` to `end_speed` — the closed form for
/// `∫₀^source_duration_secs 1/speed(t) dt`. When `speed(t) = v0 + b*t` (`b = (v1-v0)/D`), the
/// integral works out to `(D/(v1-v0)) * ln(v1/v0)` — a natural-log term, not a simple average,
/// which is exactly why the earlier stepped approximation existed in the first place (this is
/// the derivation that approximation's own doc comment flagged as needing one). Falls back to
/// plain `D/start_speed` when `start_speed`/`end_speed` are equal (the log form's own
/// denominator would be zero) — mathematically the same limit, taken directly instead of via
/// L'Hopital's rule.
pub fn smooth_speed_ramp_duration_secs(
    source_duration_secs: f64,
    start_speed: f32,
    end_speed: f32,
) -> f64 {
    smooth_speed_ramp_elapsed_timeline_secs(
        source_duration_secs,
        source_duration_secs,
        start_speed,
        end_speed,
    )
}

/// General form [`smooth_speed_ramp_duration_secs`] is a special case of
/// (`source_elapsed_secs == source_duration_secs`, the whole ramp) — timeline-elapsed time after
/// `source_elapsed_secs` of *source* time have played into a ramp whose speed varies linearly
/// from `start_speed` to `end_speed` across the *full* `source_duration_secs` span. Kept private
/// and separate from the public duration function since every external caller only ever wants
/// one or the other end of the ramp (the whole clip's duration, or — via
/// [`smooth_speed_ramp_source_secs_at`]'s inverse — a specific point within it), never this raw
/// partial form directly.
fn smooth_speed_ramp_elapsed_timeline_secs(
    source_elapsed_secs: f64,
    source_duration_secs: f64,
    start_speed: f32,
    end_speed: f32,
) -> f64 {
    let (v0, v1, d, t) = (
        (start_speed as f64).max(1e-6),
        (end_speed as f64).max(1e-6),
        source_duration_secs.max(1e-9),
        source_elapsed_secs.clamp(0.0, source_duration_secs.max(0.0)),
    );
    if (v1 - v0).abs() < 1e-6 {
        return t / v0;
    }
    let speed_at_t = v0 + (v1 - v0) * t / d;
    (d / (v1 - v0)) * (speed_at_t / v0).ln()
}

/// Inverse of [`smooth_speed_ramp_duration_secs`]: how much *source* time has elapsed once
/// `timeline_elapsed_secs` of *output* (timeline) time have played into a ramp. Needed by
/// [`crate::timeline::Track::split_clip_at`] to find the correct `source_in_secs`/
/// `source_out_secs` split point for a ramped clip — unlike a constant-speed clip, source time
/// isn't linear in timeline time here, so a plain `timeline_elapsed_secs * start_speed` would
/// land the split at the wrong source frame. Derived by solving
/// `timeline_elapsed_secs = (D/(v1-v0)) * ln(speed(t)/v0)` for `t`.
pub fn smooth_speed_ramp_source_secs_at(
    timeline_elapsed_secs: f64,
    source_duration_secs: f64,
    start_speed: f32,
    end_speed: f32,
) -> f64 {
    let (v0, v1, d) = (
        (start_speed as f64).max(1e-6),
        (end_speed as f64).max(1e-6),
        source_duration_secs.max(0.0),
    );
    if (v1 - v0).abs() < 1e-6 {
        return timeline_elapsed_secs * v0;
    }
    let k = d / (v1 - v0);
    (k * v0 * ((timeline_elapsed_secs / k).exp() - 1.0)).clamp(0.0, d)
}

/// Builds a nested `if(between(<var>,p0,p1), v0+slope*(<var>-p0), ...)` expression evaluating a
/// piecewise-linear ramp across `sorted` keyframes' (already-`xform`-ed) values, along an axis
/// measured in `<var>`'s units (frame count for `N`, seconds for `t`) from `0` to `axis_length`.
/// `sorted` must have at least 2 entries and be sorted ascending by `time_fraction`, and must
/// not be empty — callers are expected to have already handled the 0/1-keyframe fast paths.
fn piecewise_expr(
    sorted: &[Keyframe<f32>],
    axis_length: f64,
    var: &str,
    xform: impl Fn(f32) -> f32,
) -> String {
    let point_at = |frac: f32| (frac.clamp(0.0, 1.0) as f64 * axis_length) as f32;
    let last_value = xform(sorted[sorted.len() - 1].value);
    let mut expr = format!("{last_value:.7}");
    for w in sorted.windows(2).rev() {
        let (a, b) = (w[0], w[1]);
        let pa = point_at(a.time_fraction);
        let pb = point_at(b.time_fraction);
        let span = (pb - pa).max(1e-6);
        let av = xform(a.value);
        let slope = (xform(b.value) - av) / span;
        expr = format!(
            "if(between({var},{pa:.6},{pb:.6}),({av:.7}+{slope:.9}*({var}-{pa:.6})),{expr})"
        );
    }
    let first_point = point_at(sorted[0].time_fraction);
    let first_value = xform(sorted[0].value);
    format!("if(lt({var},{first_point:.6}),{first_value:.7},{expr})")
}

/// Sorts a copy of `keyframes` by `time_fraction`, and reports whether every (transformed)
/// value is close enough to `default_xformed` that there's nothing worth animating.
fn sorted_and_all_default(
    keyframes: &[Keyframe<f32>],
    xform: impl Fn(f32) -> f32,
    default_xformed: f32,
) -> (Vec<Keyframe<f32>>, bool) {
    let mut sorted = keyframes.to_vec();
    sorted.sort_by(|a, b| a.time_fraction.total_cmp(&b.time_fraction));
    let all_default = sorted
        .iter()
        .all(|k| (xform(k.value) - default_xformed).abs() <= 1e-4);
    (sorted, all_default)
}

const KENBURNS_MIN_SCALE: f32 = 0.1;
const KENBURNS_MAX_SCALE: f32 = 20.0;

fn clamp_scale(v: f32) -> f32 {
    v.clamp(KENBURNS_MIN_SCALE, KENBURNS_MAX_SCALE)
}

/// Builds the scale-keyframe avfilter fragment — a `geq` per-pixel inverse-sample, generalizing
/// `zoom_start`/`zoom_end`'s old `A + B*N` linear-ramp math (chosen over `crop`/`scale` with
/// `eval=frame` specifically because that combination corrupted the heap in a real export, see
/// CLAUDE.md) to piecewise-linear interpolation across N keyframes. Returns `None` if there's
/// nothing to animate (0 keyframes, or every keyframe at unity scale). A single non-unity
/// keyframe produces a plain static `crop`+`scale` pair (no frame variable needed), matching
/// `zoom_start == zoom_end`'s old fast path exactly.
pub fn scale_filter_expr(
    keyframes: &[Keyframe<f32>],
    fps_num: u32,
    fps_den: u32,
    timeline_duration_secs: f64,
) -> Option<String> {
    if keyframes.is_empty() {
        return None;
    }
    if keyframes.len() == 1 {
        let a = clamp_scale(keyframes[0].value);
        if (a - 1.0).abs() <= 1e-4 {
            return None;
        }
        return Some(format!(
            "crop=iw/{a:.5}:ih/{a:.5}:iw*(1-1/{a:.5})/2:ih*(1-1/{a:.5})/2,scale=iw*{a:.5}:ih*{a:.5}"
        ));
    }

    let (sorted, all_default) = sorted_and_all_default(keyframes, clamp_scale, 1.0);
    if all_default {
        return None;
    }

    let total_frames = (timeline_duration_secs * fps_num as f64 / fps_den.max(1) as f64).max(1.0);
    let n_last = (total_frames - 1.0).max(1.0);

    let z = piecewise_expr(&sorted, n_last, "N", clamp_scale);
    let sx = format!("((X-W/2)/({z})+W/2)");
    let sy = format!("((Y-H/2)/({z})+H/2)");
    let inside = format!("(1-lt({sx},0))*lt({sx},W)*(1-lt({sy},0))*lt({sy},H)");
    Some(format!(
        "geq=lum='p({sx},{sy})*{inside}':cb='128+(cb({sx},{sy})-128)*{inside}':cr='128+(cr({sx},{sy})-128)*{inside}'"
    ))
}

/// Builds the rotation-keyframe angle expression, in radians (as `rotate`'s `angle` option
/// expects — confirmed via `ffmpeg -h filter=rotate` on the pinned FFmpeg build), keyed off `t`
/// (elapsed seconds) since `rotate` is evaluated through FFmpeg's general per-option expression
/// framework, not `geq`'s per-pixel one. Returns `None` if there's nothing to animate (0
/// keyframes, or every keyframe at 0 degrees).
pub fn rotation_filter_angle_expr(
    keyframes: &[Keyframe<f32>],
    timeline_duration_secs: f64,
) -> Option<String> {
    if keyframes.is_empty() {
        return None;
    }
    let to_radians = |deg: f32| deg.to_radians();
    if keyframes.len() == 1 {
        let rad = to_radians(keyframes[0].value);
        return if rad.abs() <= 1e-4 {
            None
        } else {
            Some(format!("{rad:.7}"))
        };
    }
    let (sorted, all_default) = sorted_and_all_default(keyframes, to_radians, 0.0);
    if all_default {
        return None;
    }
    Some(piecewise_expr(
        &sorted,
        timeline_duration_secs,
        "t",
        to_radians,
    ))
}

/// Builds the crop/pan-keyframe avfilter fragment — a `geq` per-pixel inverse-sample
/// generalizing [`scale_filter_expr`]'s single symmetric zoom to four independent axes (the
/// sampled window's x, y, width, height), so a moving/resizing crop window can animate over a
/// clip without the frame's own resolution changing frame-to-frame (a `geq`-based per-pixel
/// approach is used here for the same reason `scale_filter_expr`'s own doc comment gives for
/// avoiding `crop`/`scale` with `eval=frame` — see CLAUDE.md). `crop_x_keyframes`/
/// `crop_y_keyframes` are the window's top-left corner as a fraction of frame width/height
/// (`0.0..=1.0`); `crop_w_keyframes`/`crop_h_keyframes` are its size — same units and meaning as
/// the existing static `ClipInstance::crop_x`/`crop_y`/`crop_w`/`crop_h` fields, each overridden
/// independently when its own keyframe list is non-empty (same "keyframes win when present"
/// relationship [`gain_filter_db_expr`] has with `gain_db`). Returns `None` only when nothing is
/// animated on any axis and every constant is already the full, uncropped frame (`0,0,1,1`).
#[allow(clippy::too_many_arguments)]
pub fn crop_filter_expr(
    crop_x_keyframes: &[Keyframe<f32>],
    crop_y_keyframes: &[Keyframe<f32>],
    crop_w_keyframes: &[Keyframe<f32>],
    crop_h_keyframes: &[Keyframe<f32>],
    crop_x: f32,
    crop_y: f32,
    crop_w: f32,
    crop_h: f32,
    fps_num: u32,
    fps_den: u32,
    timeline_duration_secs: f64,
) -> Option<String> {
    let animated = !crop_x_keyframes.is_empty()
        || !crop_y_keyframes.is_empty()
        || !crop_w_keyframes.is_empty()
        || !crop_h_keyframes.is_empty();
    if !animated && crop_x == 0.0 && crop_y == 0.0 && crop_w == 1.0 && crop_h == 1.0 {
        return None;
    }

    let total_frames = (timeline_duration_secs * fps_num as f64 / fps_den.max(1) as f64).max(1.0);
    let n_last = (total_frames - 1.0).max(1.0);
    let identity = |v: f32| v;

    let axis = |keyframes: &[Keyframe<f32>], constant: f32| -> String {
        match keyframes.len() {
            0 => format!("{constant:.7}"),
            1 => format!("{:.7}", keyframes[0].value),
            _ => {
                let mut sorted = keyframes.to_vec();
                sorted.sort_by(|a, b| a.time_fraction.total_cmp(&b.time_fraction));
                piecewise_expr(&sorted, n_last, "N", identity)
            }
        }
    };

    let cx = axis(crop_x_keyframes, crop_x);
    let cy = axis(crop_y_keyframes, crop_y);
    let cw = axis(crop_w_keyframes, crop_w);
    let ch = axis(crop_h_keyframes, crop_h);

    let sx = format!("(({cx})*W+(X/W)*({cw})*W)");
    let sy = format!("(({cy})*H+(Y/H)*({ch})*H)");
    let inside = format!("(1-lt({sx},0))*lt({sx},W)*(1-lt({sy},0))*lt({sy},H)");
    Some(format!(
        "geq=lum='p({sx},{sy})*{inside}':cb='128+(cb({sx},{sy})-128)*{inside}':cr='128+(cr({sx},{sy})-128)*{inside}'"
    ))
}

/// Builds the audio-gain-keyframe volume expression for FFmpeg's `volume` filter in
/// `eval=frame` mode, keyed off `t` (elapsed seconds) like rotation since `volume`'s per-frame
/// expression is evaluated through the same general per-option framework, not `geq`'s per-pixel
/// one. `keyframes`' values are in **dB** (matching `ClipInstance::gain_db`'s existing unit),
/// but `volume`'s expression mode evaluates to a **linear** multiplier, not dB — the `dB` suffix
/// only works on literal constants, never on an expression string (verified against FFmpeg's
/// own `volume` filter docs) — so each interpolated dB value is wrapped in `pow(10,X/20)` before
/// being emitted. Note this means the ramp is linearly interpolated in **linear-gain** space
/// between keyframe points (each dB value converted first, then lerped), not in dB space — same
/// "transform, then let `piecewise_expr` lerp the transformed values" shape `rotation_filter_
/// angle_expr` already uses for degrees->radians, chosen for consistency over re-deriving a
/// dB-space lerp inside the expression string itself. Returns `None` if there's nothing to
/// animate (0 keyframes, or every keyframe at 0 dB / unity gain).
pub fn gain_filter_db_expr(
    keyframes: &[Keyframe<f32>],
    timeline_duration_secs: f64,
) -> Option<String> {
    if keyframes.is_empty() {
        return None;
    }
    let db_to_linear = |db: f32| 10f32.powf(db / 20.0);
    if keyframes.len() == 1 {
        let linear = db_to_linear(keyframes[0].value);
        return if (linear - 1.0).abs() <= 1e-4 {
            None
        } else {
            Some(format!("{linear:.7}"))
        };
    }
    let (sorted, all_default) = sorted_and_all_default(keyframes, db_to_linear, 1.0);
    if all_default {
        return None;
    }
    Some(piecewise_expr(
        &sorted,
        timeline_duration_secs,
        "t",
        db_to_linear,
    ))
}

/// One axis of [`color_balance_filter_expr`] — `None` means "nothing animated on this axis,
/// caller falls back to its own constant field", the same shape `rotation_filter_angle_expr`/
/// `opacity_alpha_ramp_expr` use for their own single-axis fast path.
fn eq_axis_expr(
    keyframes: &[Keyframe<f32>],
    default: f32,
    timeline_duration_secs: f64,
) -> Option<String> {
    if keyframes.is_empty() {
        return None;
    }
    if keyframes.len() == 1 {
        let v = keyframes[0].value;
        return if (v - default).abs() <= 1e-4 {
            None
        } else {
            Some(format!("{v:.7}"))
        };
    }
    let identity = |v: f32| v;
    let (sorted, all_default) = sorted_and_all_default(keyframes, identity, default);
    if all_default {
        return None;
    }
    Some(piecewise_expr(
        &sorted,
        timeline_duration_secs,
        "t",
        identity,
    ))
}

/// Builds the `eq` filter's brightness/contrast/saturation stage, mixing constants with any
/// per-axis keyframe animation independently — each axis's own keyframes (when non-empty and
/// not already indistinguishable from neutral) override that axis's constant field, the same
/// "keyframes win when present" relationship [`gain_filter_db_expr`] has with the constant
/// `gain_db`. `eval=frame` is only appended when at least one axis is actually animated (a
/// plain literal-valued `eq` stage doesn't need per-frame re-evaluation). Returns `None` only
/// when there is nothing to draw at all — no keyframes on any axis and every constant is
/// already neutral (`0.0`/`1.0`/`1.0`), matching `ClipInstance::video_filter_chain`'s
/// pre-existing "only emit `eq` when something differs from neutral" guard.
pub fn color_balance_filter_expr(
    brightness_keyframes: &[Keyframe<f32>],
    contrast_keyframes: &[Keyframe<f32>],
    saturation_keyframes: &[Keyframe<f32>],
    brightness: f32,
    contrast: f32,
    saturation: f32,
    timeline_duration_secs: f64,
) -> Option<String> {
    let brightness_expr = eq_axis_expr(brightness_keyframes, 0.0, timeline_duration_secs);
    let contrast_expr = eq_axis_expr(contrast_keyframes, 1.0, timeline_duration_secs);
    let saturation_expr = eq_axis_expr(saturation_keyframes, 1.0, timeline_duration_secs);
    let animated =
        brightness_expr.is_some() || contrast_expr.is_some() || saturation_expr.is_some();

    if !animated && brightness == 0.0 && contrast == 1.0 && saturation == 1.0 {
        return None;
    }

    let b = brightness_expr.unwrap_or_else(|| format!("{brightness:.7}"));
    let c = contrast_expr.unwrap_or_else(|| format!("{contrast:.7}"));
    let s = saturation_expr.unwrap_or_else(|| format!("{saturation:.7}"));
    let eval = if animated { ":eval=frame" } else { "" };
    Some(format!(
        "eq=brightness={b}:contrast={c}:saturation={s}{eval}"
    ))
}

/// Builds one axis's value expression for `crate::shape_render::build_shape_filter_desc`'s `geq`
/// formula — either a plain constant (no keyframes, or a single keyframe) or a `T`-keyed
/// piecewise-linear expression animating a [`crate::timeline::ShapeClip`]'s position over its
/// own on-timeline duration. Unlike every other `*_filter_expr` builder in this module, `T` here
/// is the *timeline's absolute* time — a shape overlay is composited onto the already-exported
/// full video in a post-processing pass (`avbridge::apply_shape_overlays`), not evaluated inside
/// a per-clip filter chain with its own PTS reset — so keyframes are mapped through
/// `(T - start_secs)` rather than a bare `T`, keeping `time_fraction` `0.0..=1.0` mean
/// "0..`duration_secs` elapsed since this shape appeared", the same convention every other
/// keyframe field in this codebase already uses. Always returns a usable expression string
/// (never `None`) since `build_shape_filter_desc` needs *some* value for this axis regardless of
/// whether it's animated.
pub fn shape_axis_expr(
    keyframes: &[Keyframe<f32>],
    constant: f32,
    start_secs: f64,
    duration_secs: f64,
) -> String {
    if keyframes.is_empty() {
        return format!("{constant:.4}");
    }
    if keyframes.len() == 1 {
        return format!("{:.4}", keyframes[0].value);
    }
    let mut sorted = keyframes.to_vec();
    sorted.sort_by(|a, b| a.time_fraction.total_cmp(&b.time_fraction));
    let var = format!("(T-{start_secs:.6})");
    piecewise_expr(&sorted, duration_secs, &var, |v| v)
}

/// Builds the `TextClip` opacity-keyframe alpha-multiplier expression for
/// `avbridge::apply_text_overlays`'s per-segment `geq` alpha stage (`crate::render`'s
/// `text_clip_to_segments`) — a fade curve over this clip's own on-timeline duration, `T`-keyed
/// and offset by `start_secs` exactly like [`shape_axis_expr`] (both are post-pass overlay
/// stages composited onto the already-exported full video, so `T` is timeline-absolute).
///
/// Two other mechanisms were tried and ruled out by real tests against this project's linked
/// FFmpeg build before landing on this one (`avbridge/tests/text_overlay_test.rs` exercises the
/// real filtergraph, not just this string builder): `geq`'s alpha read-back function is spelled
/// `alpha(X,Y)`, not `a(X,Y)` as FFmpeg's own docs otherwise imply (`a(X,Y)` parses as "Unknown
/// function" against the real library); `colorchannelmixer`'s `aa` coefficient looked like a
/// simpler no-per-pixel-read-back alternative, but that filter has no `eval` option at all in
/// this build ("Could not set non-existent option 'eval'"), so its `t`/`n` per-frame variables
/// were never reachable. Neither of those turned out to be a version-specific fluke worth a
/// bigger workaround — `alpha(X,Y)` inside `geq` works, confirmed for real, so this stays
/// `T`-keyed like every other post-pass overlay builder in this module rather than carrying a
/// second `t`-keyed convention for no remaining reason.
///
/// Unlike `shape_axis_expr`, returns `None` when there's nothing to animate (0 keyframes, or
/// every keyframe fully opaque) — same "skip the stage entirely" convention as
/// [`opacity_alpha_ramp_expr`]/[`gain_filter_db_expr`], so an unanimated `TextClip` gets the
/// exact same filter graph it always has (no new stage, zero risk to the already-shipped
/// static-text rasterization/highlight pipeline).
pub fn text_opacity_alpha_expr(
    keyframes: &[Keyframe<f32>],
    start_secs: f64,
    duration_secs: f64,
) -> Option<String> {
    if keyframes.is_empty() {
        return None;
    }
    let clamp_opacity = |v: f32| v.clamp(0.0, 1.0);
    if keyframes.len() == 1 {
        let a = clamp_opacity(keyframes[0].value);
        return if (a - 1.0).abs() <= 1e-4 {
            None
        } else {
            Some(format!("{a:.7}"))
        };
    }
    let (sorted, all_default) = sorted_and_all_default(keyframes, clamp_opacity, 1.0);
    if all_default {
        return None;
    }
    let var = format!("(T-{start_secs:.6})");
    Some(piecewise_expr(&sorted, duration_secs, &var, clamp_opacity))
}

/// Builds the `overlay` filter's `x`/`y` pixel-offset expression for one axis of a `TextClip`'s
/// `pos_x_keyframes`/`pos_y_keyframes` — the delta, in canvas pixels, between the keyframed
/// position at time `t` and `base`, the constant position `crate::render::text_clip_to_segments`
/// bakes into the overlay PNG's own pixel layout at rasterization time (the first keyframe's
/// value when keyframes are present, `pos_x`/`pos_y` otherwise — same "keyframes win when
/// present" convention [`crate::timeline::ShapeClip`]'s own position/size/rotation keyframes
/// use). `avbridge::apply_text_overlays`'s composite stage shares the exported video's own time
/// origin, so `t` here is timeline-absolute, same clock as [`text_opacity_alpha_expr`]'s `T` —
/// just a different filter's own expression-evaluator variable name (`overlay`'s is lowercase
/// `t`, `geq`'s is uppercase `T`). Returns `None` (meaning "no offset", `overlay`'s own
/// `x=0`/`y=0` default — the exact filter graph an unanimated `TextClip` always had) with fewer
/// than 2 keyframes, since a single keyframe's value is exactly `base` by construction, or when
/// every keyframe already equals `base`.
pub fn text_position_offset_expr(
    keyframes: &[Keyframe<f32>],
    base: f32,
    start_secs: f64,
    duration_secs: f64,
    canvas_extent_px: f32,
) -> Option<String> {
    if keyframes.len() < 2 {
        return None;
    }
    let identity = |v: f32| v;
    let (sorted, all_default) = sorted_and_all_default(keyframes, identity, base);
    if all_default {
        return None;
    }
    let var = format!("(t-{start_secs:.6})");
    let to_px = |v: f32| (v - base) * canvas_extent_px;
    Some(piecewise_expr(&sorted, duration_secs, &var, to_px))
}

/// Builds the `geq` inverse-sample coordinate expressions for a `TextClip`'s `scale_keyframes`
/// — the `(X,Y)` a `geq` stage should read the raster's own pixels from instead of the output
/// pixel `(X,Y)` it's computing, to make the raster appear scaled by the (possibly time-varying)
/// factor around `(anchor_x_px, anchor_y_px)` (the raster's own baked position anchor —
/// `crate::render::text_clip_to_segments`' `base_pos_x`/`base_pos_y`, in canvas pixels). This is
/// deliberately an inverse-sample zoom (output frame size never changes, only what each pixel
/// samples) rather than a literal `scale` filter with `eval=frame`: letting `scale` renegotiate
/// its output size every frame reliably corrupted the heap in a real export elsewhere in this
/// codebase (see `avbridge::apply_text_overlays`' doc comment, `CLAUDE.md`) — the same reasoning
/// [`crate::timeline::ClipInstance::scale_keyframes`]' own `scale_filter_expr` already applies.
///
/// Scaling by factor `s` around `(ax, ay)` maps input point `(x, y)` to
/// `(ax + (x-ax)*s, ay + (y-ay)*s)`; sampling backward (what each *output* pixel should read) is
/// the inverse, `(ax + (X-ax)/s, ay + (Y-ay)/s)` — exactly what this builds, `T`-keyed like
/// [`text_opacity_alpha_expr`] (same post-pass, timeline-absolute clock). Returns `None`
/// (meaning "no remap", the raster's own unscaled pixels) for 0 keyframes, or when every
/// keyframe clamps to unity scale.
pub fn text_scale_sample_exprs(
    keyframes: &[Keyframe<f32>],
    anchor_x_px: f32,
    anchor_y_px: f32,
    start_secs: f64,
    duration_secs: f64,
) -> Option<(String, String)> {
    if keyframes.is_empty() {
        return None;
    }
    if keyframes.len() == 1 {
        let s = clamp_scale(keyframes[0].value);
        return if (s - 1.0).abs() <= 1e-4 {
            None
        } else {
            Some((
                format!("({anchor_x_px:.4}+(X-({anchor_x_px:.4}))/({s:.7}))"),
                format!("({anchor_y_px:.4}+(Y-({anchor_y_px:.4}))/({s:.7}))"),
            ))
        };
    }
    let (sorted, all_default) = sorted_and_all_default(keyframes, clamp_scale, 1.0);
    if all_default {
        return None;
    }
    let var = format!("(T-{start_secs:.6})");
    let factor_expr = piecewise_expr(&sorted, duration_secs, &var, clamp_scale);
    Some((
        format!("({anchor_x_px:.4}+(X-({anchor_x_px:.4}))/({factor_expr}))"),
        format!("({anchor_y_px:.4}+(Y-({anchor_y_px:.4}))/({factor_expr}))"),
    ))
}

/// Builds the `geq` inverse-sample coordinate expressions for a `TextClip`'s `rotation_keyframes`
/// — same "`(X,Y)` a `geq` stage should read the raster's own pixels from" contract as
/// [`text_scale_sample_exprs`], around the same raster-baked position anchor, so a keyframed
/// rotation composes with position/scale independently (no interaction term needed, same reason
/// scale and position compose independently — the geq remap never changes the raster's own
/// canvas-sized frame). Angle is clockwise degrees, same convention
/// [`crate::timeline::ShapeClip::rotation_deg`]/`shape_render`'s own rotation math uses; the
/// inverse-rotation sample point (output pixel `(X,Y)` rotated by `-angle` around the anchor) is
/// `shape_render::build_shape_filter_desc`'s `rx`/`ry` local-frame math, just re-added back onto
/// the anchor instead of compared against half-extents — FFmpeg's expression evaluator natively
/// provides `PI`/`sin()`/`cos()` (confirmed against `shape_render`'s own doc comment, not
/// assumed), `T`-keyed like [`text_scale_sample_exprs`] (same post-pass, timeline-absolute
/// clock). Returns `None` (meaning "no remap") for 0 keyframes, or when every keyframe clamps to
/// 0 degrees.
pub fn text_rotation_sample_exprs(
    keyframes: &[Keyframe<f32>],
    anchor_x_px: f32,
    anchor_y_px: f32,
    start_secs: f64,
    duration_secs: f64,
) -> Option<(String, String)> {
    if keyframes.is_empty() {
        return None;
    }
    let to_radians = |deg: f32| deg.to_radians();
    let build = |angle_rad_expr: String| {
        let sin_expr = format!("sin({angle_rad_expr})");
        let cos_expr = format!("cos({angle_rad_expr})");
        let sx = format!(
            "({anchor_x_px:.4}+(X-({anchor_x_px:.4}))*{cos_expr}+(Y-({anchor_y_px:.4}))*{sin_expr})"
        );
        let sy = format!(
            "({anchor_y_px:.4}-(X-({anchor_x_px:.4}))*{sin_expr}+(Y-({anchor_y_px:.4}))*{cos_expr})"
        );
        (sx, sy)
    };
    if keyframes.len() == 1 {
        let rad = to_radians(keyframes[0].value);
        return if rad.abs() <= 1e-4 {
            None
        } else {
            Some(build(format!("{rad:.7}")))
        };
    }
    let (sorted, all_default) = sorted_and_all_default(keyframes, to_radians, 0.0);
    if all_default {
        return None;
    }
    let var = format!("(T-{start_secs:.6})");
    let angle_expr = piecewise_expr(&sorted, duration_secs, &var, to_radians);
    Some(build(format!("({angle_expr})")))
}

/// Builds the opacity-keyframe alpha expression (a bare `0.0..=1.0` ramp, *not* yet multiplied
/// by any incoming `alpha(X,Y)` — the caller composes that, matching `mask_shape`'s existing
/// alpha-composition convention in `ClipInstance::video_filter_chain`), keyed off `N` like
/// scale since this is meant to sit inside the same `geq` stage. Returns `None` if there's
/// nothing to animate (0 keyframes, or every keyframe fully opaque).
pub fn opacity_alpha_ramp_expr(
    keyframes: &[Keyframe<f32>],
    fps_num: u32,
    fps_den: u32,
    timeline_duration_secs: f64,
) -> Option<String> {
    if keyframes.is_empty() {
        return None;
    }
    let clamp_opacity = |v: f32| v.clamp(0.0, 1.0);
    if keyframes.len() == 1 {
        let a = clamp_opacity(keyframes[0].value);
        return if (a - 1.0).abs() <= 1e-4 {
            None
        } else {
            Some(format!("{a:.7}"))
        };
    }
    let (sorted, all_default) = sorted_and_all_default(keyframes, clamp_opacity, 1.0);
    if all_default {
        return None;
    }
    let total_frames = (timeline_duration_secs * fps_num as f64 / fps_den.max(1) as f64).max(1.0);
    let n_last = (total_frames - 1.0).max(1.0);
    Some(piecewise_expr(&sorted, n_last, "N", clamp_opacity))
}

/// Builds the `overlay` filter's `x`/`y` expression fragments for this clip's position
/// keyframes, in canvas-fraction units multiplied by `overlay`'s own `main_w`/`main_h`
/// variables (its documented, standard names for the compositing base's width/height) so no
/// canvas size needs to be threaded in here — keyed off `t` like rotation. Returns `None`
/// (meaning "no offset", `overlay`'s own `x=0:y=0` default) if there's nothing to animate.
/// Only meaningful on an overlay-track clip (see module docs) — a single/background track has
/// no compositing stage to apply this to.
pub fn position_overlay_xy_expr(
    keyframes: &[Keyframe<Position>],
    timeline_duration_secs: f64,
) -> Option<(String, String)> {
    if keyframes.is_empty() {
        return None;
    }
    let xs: Vec<Keyframe<f32>> = keyframes
        .iter()
        .map(|k| Keyframe {
            time_fraction: k.time_fraction,
            value: k.value.x,
        })
        .collect();
    let ys: Vec<Keyframe<f32>> = keyframes
        .iter()
        .map(|k| Keyframe {
            time_fraction: k.time_fraction,
            value: k.value.y,
        })
        .collect();
    let identity = |v: f32| v;
    let build_axis = |axis: &[Keyframe<f32>], var: &str| -> Option<String> {
        if axis.len() == 1 {
            let v = axis[0].value;
            return if v.abs() <= 1e-4 {
                None
            } else {
                Some(format!("({v:.7})*{var}"))
            };
        }
        let (sorted, all_default) = sorted_and_all_default(axis, identity, 0.0);
        if all_default {
            return None;
        }
        Some(format!(
            "({})*{var}",
            piecewise_expr(&sorted, timeline_duration_secs, "t", identity)
        ))
    };
    let x_expr = build_axis(&xs, "main_w");
    let y_expr = build_axis(&ys, "main_h");
    if x_expr.is_none() && y_expr.is_none() {
        return None;
    }
    Some((
        x_expr.unwrap_or_else(|| "0".to_string()),
        y_expr.unwrap_or_else(|| "0".to_string()),
    ))
}

#[cfg(test)]
#[path = "keyframe/keyframe_test.rs"]
mod tests;
