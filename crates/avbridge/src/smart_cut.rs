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

use std::ffi::{c_void, CString};
use std::os::raw::c_longlong;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use crate::ffi::{avbridge_smart_cut_video, progress_trampoline};
use crate::timeline_export::Canvas;

/// What [`smart_cut_video`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmartCutOutcome {
    /// The video between the cut points was written to the output path.
    Done,
    /// The clip or the cut does not suit a copy; nothing was written.
    NotApplicable,
    Cancelled,
}

#[derive(Debug, thiserror::Error)]
pub enum SmartCutError {
    #[error("path is not a valid C string: {0}")]
    InvalidPath(std::ffi::NulError),
    /// Writing started and failed: the output file is not usable.
    #[error("the lossless cut failed part-way")]
    Failed,
    #[error("unknown smart-cut status code: {0}")]
    Unknown(i32),
}

/// Writes the video of `source` between `in_secs` and `out_secs` to `out_path` (no audio), frame
/// accurate, without encoding what lies between the keyframes next to the cut points: that part is
/// copied packet for packet, and only the pictures at the two ends are decoded and encoded again.
/// The source must be H.264 4:2:0 at the canvas's size with a constant frame rate equal to the
/// canvas's, and both neighbouring keyframes must be IDR pictures; otherwise this returns
/// [`SmartCutOutcome::NotApplicable`] without writing anything.
pub fn smart_cut_video<F: FnMut(f64)>(
    source: &Path,
    in_secs: f64,
    out_secs: f64,
    canvas: Canvas,
    out_path: &Path,
    cancel: &AtomicBool,
    mut on_progress: F,
) -> Result<SmartCutOutcome, SmartCutError> {
    let c_in =
        CString::new(source.to_string_lossy().as_bytes()).map_err(SmartCutError::InvalidPath)?;
    let c_out =
        CString::new(out_path.to_string_lossy().as_bytes()).map_err(SmartCutError::InvalidPath)?;
    // SAFETY: the paths are valid C strings held for the call, `on_progress` and `cancel` outlive
    // it, and the trampoline only ever runs inside it.
    let status = unsafe {
        avbridge_smart_cut_video(
            c_in.as_ptr(),
            in_secs,
            out_secs,
            canvas.width as i32,
            canvas.height as i32,
            canvas.fps_num as i32,
            canvas.fps_den as i32,
            canvas.bit_rate_bps as c_longlong,
            c_out.as_ptr(),
            Some(progress_trampoline::<F>),
            &mut on_progress as *mut _ as *mut c_void,
            cancel.as_ptr() as *const u8,
        )
    };
    match status {
        0 => Ok(SmartCutOutcome::Done),
        1 => Ok(SmartCutOutcome::NotApplicable),
        2 => Ok(SmartCutOutcome::Cancelled),
        3 => Err(SmartCutError::Failed),
        other => Err(SmartCutError::Unknown(other)),
    }
}
