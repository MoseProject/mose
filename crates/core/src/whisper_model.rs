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

//! The part of [`crate::transcribe`] that runs a Whisper GGML model. A model is a
//! multi-hundred-megabyte download that is not in the repository, so this file is excluded from
//! coverage; the audio decoding before it, the cancellation checks, the word grouping and every
//! caller are tested without one.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::transcribe::{
    group_tokens, RawToken, TranscribeError, TranscribeOutcome, TranscribeSegment, TranscribeWord,
};

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

/// Runs the model over 16 kHz mono `samples`; see [`crate::transcribe::transcribe`].
pub(crate) fn run(
    samples: &[f32],
    model_path: &Path,
    language: Option<&str>,
    cancel: &Arc<AtomicBool>,
    on_progress: impl FnMut(i32) + 'static,
) -> Result<TranscribeOutcome, TranscribeError> {
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

    let full_result = state.full(params, samples);
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
