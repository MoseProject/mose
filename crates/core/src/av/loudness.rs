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

//! Measures perceptual loudness (LUFS), true peak and loudness range via a single-pass
//! `loudnorm` filter run through `oca-avbridge`'s FFI — no subprocess.
//!
//! Like [`crate::probe`], the FFI call ([`measure_loudness`]) is a thin wrapper around a pure,
//! unit-testable parser ([`parse_loudnorm_stderr`]) — the parser is unchanged from when it
//! read a subprocess's stderr: `loudnorm` still only exposes its final report via `av_log`
//! (captured on the C side, see `oca-avbridge/csrc/bridge.c`), so the text shape is identical.

use std::path::Path;

use serde::Deserialize;

use crate::media::LoudnessMetrics;

#[derive(Debug)]
pub enum LoudnessError {
    /// `oca-avbridge` failed before or during the decode/filter pipeline.
    Bridge(avbridge::LoudnessError),
    /// The captured report didn't contain a `{...}` JSON block — usually means the input had
    /// no audio stream, or the pipeline failed before the filter ran.
    NoReportFound,
    /// Found a `{...}` block but it wasn't a valid `loudnorm` report.
    Json(serde_json::Error),
}

impl std::fmt::Display for LoudnessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoudnessError::Bridge(e) => write!(f, "failed to measure loudness: {e}"),
            LoudnessError::NoReportFound => {
                write!(f, "no loudnorm report found in the captured output")
            }
            LoudnessError::Json(e) => write!(f, "failed to parse loudnorm report: {e}"),
        }
    }
}

impl std::error::Error for LoudnessError {}

/// Runs a single-pass `loudnorm` measurement against `path` and returns the result.
///
/// This is analysis only — nothing is written or re-encoded. The default targets
/// (`I=-16:TP=-1.5:LRA=11`) match `Watch-Gameplay.ps1`'s original technique; the two-pass
/// normalization described in Fase 2 of the execution plan (measure, then apply a target like
/// -14 LUFS for YouTube) builds on top of this measurement.
pub fn measure_loudness(path: &Path) -> Result<LoudnessMetrics, LoudnessError> {
    let report = avbridge::measure_loudness_json(path).map_err(LoudnessError::Bridge)?;
    parse_loudnorm_stderr(&report)
}

#[derive(Debug, Deserialize)]
struct LoudnormReport {
    input_i: String,
    input_tp: String,
    input_lra: String,
}

/// Extracts and parses the `loudnorm` JSON report from captured `av_log` text, ignoring the
/// log lines before and after it.
pub fn parse_loudnorm_stderr(stderr: &str) -> Result<LoudnessMetrics, LoudnessError> {
    let json_block = extract_first_json_object(stderr).ok_or(LoudnessError::NoReportFound)?;
    let report: LoudnormReport = serde_json::from_str(json_block).map_err(LoudnessError::Json)?;

    Ok(LoudnessMetrics {
        integrated_lufs: report.input_i.parse().unwrap_or(0.0),
        true_peak_dbtp: report.input_tp.parse().unwrap_or(0.0),
        loudness_range_lu: report.input_lra.parse().unwrap_or(0.0),
    })
}

/// Finds the first balanced `{...}` substring, scanning byte-by-byte and tracking brace
/// depth so it isn't confused by any earlier unbalanced braces in the log preamble.
fn extract_first_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let mut depth = 0i32;
    for (offset, ch) in text[start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let end = start + offset + 1;
                    return Some(&text[start..end]);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
#[path = "loudness/loudness_test.rs"]
mod tests;
