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

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

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

/// Trampoline for whisper.cpp's `ggml_abort_callback` (`unsafe extern "C" fn(*mut c_void) ->
/// bool`), used via `FullParams`'s raw `set_abort_callback`/`set_abort_callback_user_data`
/// instead of the crate's own `set_abort_callback_safe` — see [`transcribe`]'s docs for why.
/// `user_data` must point at a `Box<dyn FnMut() -> bool>` previously leaked via
/// `Box::into_raw(Box::new(Box::new(closure) as Box<dyn FnMut() -> bool>))` (double-boxed: an
/// owned `Box<dyn Trait>` is itself a fat pointer, so it needs its own heap slot to have a
/// stable thin address to hand across the FFI boundary) — the same construction
/// `set_progress_callback_safe` correctly uses, just paired here with a trampoline that agrees
/// with it on the pointee type, which is exactly what `set_abort_callback_safe` gets wrong.
unsafe extern "C" fn abort_trampoline(user_data: *mut std::ffi::c_void) -> bool {
    // SAFETY: caller (this module's `transcribe`) guarantees user_data was produced by the
    // construction described above and remains valid (the Box isn't freed) for as long as
    // whisper.cpp might still call this — i.e. until after `state.full()` returns.
    let closure = unsafe { &mut *(user_data as *mut Box<dyn FnMut() -> bool>) };
    closure()
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
/// encode" (`GenericError(-6)`). [`abort_trampoline`] above reimplements the same
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

    let model_path_str = model_path.to_string_lossy().into_owned();
    let ctx = WhisperContext::new_with_params(&model_path_str, WhisperContextParameters::default())
        .map_err(|_| TranscribeError::ModelLoad(model_path.to_path_buf()))?;
    let mut state = ctx.create_state().map_err(|_| TranscribeError::StateInit)?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(language);
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_token_timestamps(true);
    params.set_progress_callback_safe(on_progress);

    let cancel_for_abort = Arc::clone(cancel);
    let abort_closure: Box<dyn FnMut() -> bool> =
        Box::new(move || cancel_for_abort.load(Ordering::Relaxed));
    let abort_user_data = Box::into_raw(Box::new(abort_closure));
    // SAFETY: abort_user_data points at the Box<dyn FnMut() -> bool> abort_trampoline expects,
    // built exactly the way its doc comment requires. It's reclaimed (Box::from_raw) right
    // after state.full() returns below, by which point whisper.cpp is guaranteed to have
    // stopped calling it — the call is synchronous and whisper.cpp doesn't retain the pointer
    // past it.
    unsafe {
        params.set_abort_callback(Some(abort_trampoline));
        params.set_abort_callback_user_data(abort_user_data as *mut std::ffi::c_void);
    }

    let full_result = state.full(params, &samples);
    // SAFETY: state.full() has returned, so whisper.cpp will not call abort_trampoline with
    // this pointer again — safe to reclaim and drop now.
    unsafe {
        drop(Box::from_raw(abort_user_data));
    }

    // whisper.cpp's return code doesn't distinguish "aborted via callback" from a genuine
    // encode/decode failure (both surface as e.g. WhisperError::FailedToEncode) — so once
    // cancel is set, full_result's specific Err (if any) can't be trusted to tell them apart.
    // Treat it as a cancellation unconditionally in that case, using whatever segments were
    // already committed before the abort rather than discarding them as an error.
    if cancel.load(Ordering::Relaxed) {
        return Ok(TranscribeOutcome::Cancelled(collect_segments(&state)));
    }
    full_result.map_err(|_| TranscribeError::Inference)?;

    Ok(TranscribeOutcome::Completed(collect_segments(&state)))
}

fn collect_segments(state: &whisper_rs::WhisperState) -> Vec<TranscribeSegment> {
    let num_segments = state.full_n_segments();
    let mut segments = Vec::with_capacity(num_segments.max(0) as usize);
    for i in 0..num_segments {
        let Some(segment) = state.get_segment(i) else {
            continue;
        };
        let text = segment.to_str().unwrap_or_default().trim().to_string();
        if text.is_empty() {
            continue;
        }
        // whisper.cpp timestamps are in centiseconds (units of 10ms).
        segments.push(TranscribeSegment {
            start_secs: segment.start_timestamp() as f64 * 0.01,
            end_secs: segment.end_timestamp() as f64 * 0.01,
            text,
            words: collect_words(&segment),
        });
    }
    segments
}

/// Groups `segment`'s per-token timestamps into per-word timestamps. whisper.cpp marks a new
/// word by prefixing its first token's text with a space (e.g. "Hello world" tokenizes as
/// `["Hello", " world"]` — the *second* word carries the leading space, not the first); a word
/// can span multiple tokens with no leading space of their own (e.g. "running" as `["run",
/// "ning"]`), which get appended onto the word already being built rather than starting a new
/// one. Special/control tokens (`[_BEG_]`, `[_TT_50]`, etc. — bracketed, not real transcribed
/// text) are dropped rather than becoming garbage "words".
fn collect_words(segment: &whisper_rs::WhisperSegment) -> Vec<TranscribeWord> {
    let tokens = (0..segment.n_tokens()).filter_map(|t| {
        let token = segment.get_token(t)?;
        let raw_text = token.to_str().ok()?.to_string();
        let data = token.token_data();
        Some(RawToken {
            text: raw_text,
            start_secs: data.t0 as f64 * 0.01,
            end_secs: data.t1 as f64 * 0.01,
            probability: data.p,
        })
    });
    group_tokens(tokens)
}

/// One whisper token with its time range and probability, before words are formed.
struct RawToken {
    text: String,
    start_secs: f64,
    end_secs: f64,
    probability: f32,
}

/// The word grouping of [`collect_words`], on plain tokens so it needs no whisper state.
fn group_tokens(tokens: impl IntoIterator<Item = RawToken>) -> Vec<TranscribeWord> {
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
