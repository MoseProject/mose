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

//! Automatic speech transcription — Fase 4's "Legendas automáticas". Runs a local Whisper
//! model (whisper.cpp via the `whisper-rs` crate) fully offline: audio is decoded/resampled to
//! the 16kHz mono float PCM whisper.cpp requires via `avbridge::extract_pcm_16k_mono` (no
//! subprocess, same FFI-not-shell-out approach as every other `avbridge` call), then run
//! through Whisper's inference to produce timestamped segments.
//!
//! Release bundles include Whisper Base under `resources/models/`; callers still pass the path
//! explicitly so tests and advanced users can supply another compatible GGML model.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// One transcribed segment: a `[start_secs, end_secs)` time range plus its recognized text.
/// `words` breaks that same text down further, one entry per word with its own tighter time
/// range — see [`TranscribeWord`] — for word-highlight subtitles (Fase 4's "Legenda com
/// destaque de palavra"). Empty if word-level timestamps couldn't be extracted for this segment
/// (shouldn't normally happen — [`transcribe`] always requests token timestamps — but a
/// segment with only special/filtered tokens has nothing to group into words).
#[derive(Debug, Clone, PartialEq)]
pub struct TranscribeSegment {
    pub start_secs: f64,
    pub end_secs: f64,
    pub text: String,
    pub words: Vec<TranscribeWord>,
}

/// One word within a [`TranscribeSegment`], with its own `[start_secs, end_secs)` — whisper.cpp
/// gives timestamps per *token*, not per word (a word can be multiple tokens, e.g. "running" as
/// "run" + "ning"); [`collect_segments`] groups tokens into words by whitespace boundaries and
/// takes the first token's start / last token's end as the word's own range.
#[derive(Debug, Clone, PartialEq)]
pub struct TranscribeWord {
    pub text: String,
    pub start_secs: f64,
    pub end_secs: f64,
    /// This word's own confidence, `0.0..=1.0` — the mean of its constituent tokens' own
    /// probability (`whisper_token_data::p`, confirmed against `whisper-rs-sys`'s bindgen
    /// output, not assumed). A multi-token word (e.g. "running" as `["run", "ning"]`) averages
    /// across every token that got merged into it, same grouping [`collect_words`] already does
    /// for the word's own text/time range.
    pub confidence: f32,
}

#[derive(Debug)]
pub enum TranscribeError {
    /// Failed to decode/resample `source_path`'s audio — see [`avbridge::PcmError`].
    Audio(avbridge::PcmError),
    /// The source decoded successfully but produced zero PCM samples — nothing to transcribe.
    NoAudio,
    /// `model_path` doesn't exist, isn't a valid GGML Whisper model, or otherwise failed to
    /// load.
    ModelLoad(std::path::PathBuf),
    /// Failed to allocate a whisper.cpp inference state for the loaded model.
    StateInit,
    /// Whisper inference itself failed (not a cancellation — see [`TranscribeOutcome::Cancelled`]
    /// for that).
    Inference,
}

impl std::fmt::Display for TranscribeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TranscribeError::Audio(e) => write!(f, "failed to extract audio: {e}"),
            TranscribeError::NoAudio => write!(f, "source has no audio to transcribe"),
            TranscribeError::ModelLoad(p) => {
                write!(f, "failed to load whisper model at {}", p.display())
            }
            TranscribeError::StateInit => write!(f, "failed to create whisper inference state"),
            TranscribeError::Inference => write!(f, "whisper inference failed"),
        }
    }
}

impl std::error::Error for TranscribeError {}

impl From<avbridge::PcmError> for TranscribeError {
    fn from(e: avbridge::PcmError) -> Self {
        TranscribeError::Audio(e)
    }
}

/// How a transcription ended. A cancelled transcription still carries whatever segments
/// whisper.cpp had already emitted before the abort callback fired — there's no "invalid
/// truncated file" concern here (nothing is written to disk), so partial results are still
/// useful to the caller rather than being discarded.
#[derive(Debug, Clone, PartialEq)]
pub enum TranscribeOutcome {
    Completed(Vec<TranscribeSegment>),
    Cancelled(Vec<TranscribeSegment>),
}

/// Transcribes `source_path`'s audio using the Whisper model at `model_path`.
///
/// `language` is a Whisper language code (e.g. `Some("pt")`); `None` lets Whisper
/// auto-detect the spoken language from the first ~30 seconds of audio.
///
/// Calls `on_progress(percent)` (0-100) as inference proceeds, and checks `cancel` between
/// whisper.cpp's internal steps — if another thread sets it, inference stops early and this
/// returns `Ok(TranscribeOutcome::Cancelled(partial_segments))` rather than treating the
/// cancellation as a failure (same convention as [`crate::render::render_export`]).
///
/// Cancellation is wired through `FullParams`'s raw, `unsafe` `set_abort_callback` API rather
/// than the crate's own safe `set_abort_callback_safe`, which has a real upstream bug in
/// `whisper-rs` 0.16.0: its trampoline casts the callback's double-boxed
/// `Box<dyn FnMut() -> bool>` pointer back to the *original* concrete closure type instead of
/// `Box<dyn FnMut() -> bool>` (compare its `trampoline::<F>` to `set_progress_callback_safe`'s
/// correct `trampoline::<Box<dyn FnMut(i32)>>`) — undefined behavior, confirmed empirically:
/// any closure capturing state made whisper.cpp abort on the very first internal check
/// regardless of what the closure actually returned, immediately failing with "failed to
/// encode" (`GenericError(-6)`). `whisper_model`'s abort trampoline reimplements the same
/// double-box construction with the matching pointee type instead.
///
/// `cancel` is an `Arc<AtomicBool>` rather than a plain reference (unlike the `render`/`encode`
/// functions' `&AtomicBool`) because `set_progress_callback_safe` (which IS used as-is, and IS
/// correct) requires a `'static` closure.
pub fn transcribe(
    source_path: &Path,
    model_path: &Path,
    language: Option<&str>,
    cancel: &Arc<AtomicBool>,
    on_progress: impl FnMut(i32) + 'static,
) -> Result<TranscribeOutcome, TranscribeError> {
    if cancel.load(Ordering::Relaxed) {
        return Ok(TranscribeOutcome::Cancelled(Vec::new()));
    }

    let samples = avbridge::extract_pcm_16k_mono(source_path)?;
    if samples.is_empty() {
        return Err(TranscribeError::NoAudio);
    }

    crate::whisper_model::run(&samples, model_path, language, cancel, on_progress)
}

/// One whisper token with its time range and probability, before words are formed.
pub(crate) struct RawToken {
    pub(crate) text: String,
    pub(crate) start_secs: f64,
    pub(crate) end_secs: f64,
    pub(crate) probability: f32,
}

/// The word grouping of [`collect_words`], on plain tokens so it needs no whisper state.
pub(crate) fn group_tokens(tokens: impl IntoIterator<Item = RawToken>) -> Vec<TranscribeWord> {
    // Parallel to `current`: the running (sum, count) of token probabilities merged into the
    // word being built, so its final confidence is their mean — tracked alongside rather than
    // inside `TranscribeWord` itself, which only ever stores the finished per-word average.
    let mut words = Vec::new();
    let mut current: Option<TranscribeWord> = None;
    let mut current_prob_sum = 0.0_f32;
    let mut current_prob_count = 0u32;

    for token in tokens {
        let raw_text = token.text.as_str();
        if raw_text.trim().starts_with('[') {
            continue;
        }
        if raw_text.starts_with(' ') || current.is_none() {
            if let Some(mut word) = current.take() {
                if !word.text.is_empty() {
                    word.confidence = current_prob_sum / current_prob_count.max(1) as f32;
                    words.push(word);
                }
            }
            current = Some(TranscribeWord {
                text: raw_text.trim_start().to_string(),
                start_secs: token.start_secs,
                end_secs: token.end_secs,
                confidence: 0.0,
            });
            current_prob_sum = token.probability;
            current_prob_count = 1;
        } else if let Some(word) = current.as_mut() {
            word.text.push_str(raw_text);
            word.end_secs = token.end_secs;
            current_prob_sum += token.probability;
            current_prob_count += 1;
        }
    }
    if let Some(mut word) = current.take() {
        if !word.text.is_empty() {
            word.confidence = current_prob_sum / current_prob_count.max(1) as f32;
            words.push(word);
        }
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(text: &str, start_secs: f64, end_secs: f64, probability: f32) -> RawToken {
        RawToken {
            text: text.to_string(),
            start_secs,
            end_secs,
            probability,
        }
    }

    #[test]
    fn a_leading_space_starts_a_word_and_other_tokens_extend_it() {
        let words = group_tokens([
            token("[_BEG_]", 0.0, 0.0, 1.0),
            token("Hello", 0.0, 0.4, 0.5),
            token(" run", 0.5, 0.7, 0.9),
            token("ning", 0.7, 1.0, 0.7),
            token("[_TT_50]", 1.0, 1.0, 1.0),
        ]);
        assert_eq!(words.len(), 3 - 1);
        assert_eq!(words[0].text, "Hello");
        assert_eq!((words[0].start_secs, words[0].end_secs), (0.0, 0.4));
        assert_eq!(words[0].confidence, 0.5);
        // The two tokens of "running" merge: text joined, range extended, mean probability.
        assert_eq!(words[1].text, "running");
        assert_eq!((words[1].start_secs, words[1].end_secs), (0.5, 1.0));
        assert!((words[1].confidence - 0.8).abs() < 1e-6);
    }

    #[test]
    fn empty_words_are_dropped_and_no_tokens_give_no_words() {
        assert!(group_tokens([]).is_empty());
        // A lone space token is an empty word at the start and at the end.
        let words = group_tokens([
            token(" ", 0.0, 0.1, 0.5),
            token(" hi", 0.1, 0.2, 0.5),
            token(" ", 0.2, 0.3, 0.5),
        ]);
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].text, "hi");
    }

    #[test]
    fn errors_have_readable_messages_and_wrap_pcm_failures() {
        let model = TranscribeError::ModelLoad(std::path::PathBuf::from("model.bin"));
        assert!(model.to_string().contains("model.bin"));
        assert!(!TranscribeError::NoAudio.to_string().is_empty());
        assert!(!TranscribeError::StateInit.to_string().is_empty());
        assert!(!TranscribeError::Inference.to_string().is_empty());
        let wrapped = TranscribeError::from(avbridge::PcmError::NoAudioStream);
        assert!(matches!(wrapped, TranscribeError::Audio(_)));
        assert!(wrapped.to_string().contains("extract audio"));
    }
}
