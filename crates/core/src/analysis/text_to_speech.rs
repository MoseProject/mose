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

//! Text-to-speech — per `request.md`'s Fase 4 "Texto-pra-fala": synthesizes narration from
//! typed text. Runs a Piper VITS ONNX voice model (`rhasspy/piper-voices` on Hugging Face, MIT)
//! against phonemes produced by [`EspeakPhonemizer`] (statically-linked espeak-ng, no runtime
//! DLL — see `build.rs`'s doc comment for how its `espeak-ng-data` directory gets discovered at
//! runtime).
//!
//! Pipeline: [`Phonemizer::phonemize`] (text -> one IPA string per sentence) ->
//! [`phonemes_to_ids`] (IPA string -> phoneme IDs, via the voice's own `phoneme_id_map`) ->
//! [`synthesize`] (ONNX VITS inference -> raw `f32` samples) -> [`write_wav`].
//!
//! **Phoneme ID scheme:** BOS (`"^"`) at the start, EOS (`"$"`) at the end, and PAD (`"_"`)
//! interspersed after every phoneme symbol — this exact scheme (not guessed: confirmed against
//! a real downloaded voice's `.onnx.json`) is what every published Piper voice was trained on,
//! matching the reference Python implementation's `phonemes_to_ids`.
//!
//! **Model I/O contract:** `input` (`int64[1,N]` phoneme ids), `input_lengths` (`int64[1]`),
//! `scales` (`float32[3]` = `[noise_scale, length_scale, noise_w]`), optionally `sid`
//! (`int64[1]`, only for multi-speaker voices) -> `output` (`float32[1,1,T]` samples in
//! `-1.0..=1.0` at the voice's `sample_rate`). This is Piper's well-established, publicly
//! documented ONNX export contract, not guessed — and confirmed empirically end-to-end on this
//! dev machine (see the module's smoke-tested history in `CLAUDE.md`).

use std::collections::HashMap;
use std::path::Path;

use unicode_segmentation::UnicodeSegmentation;

mod phonemizer;

pub use phonemizer::{EspeakPhonemizer, PhonemizationError, Phonemizer};

const PAD_SYMBOL: &str = "_";
const BOS_SYMBOL: &str = "^";
const EOS_SYMBOL: &str = "$";

/// A Piper voice's `.onnx.json` sidecar — only the fields `text_to_speech` actually needs.
/// `#[serde(default)]` throughout since Piper's config schema has grown fields over the years
/// (e.g. `phoneme_map`) that not every voice's config includes.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PiperVoiceConfig {
    pub audio: PiperAudioConfig,
    pub espeak: PiperEspeakConfig,
    #[serde(default)]
    pub inference: PiperInferenceConfig,
    #[serde(default)]
    pub num_speakers: u32,
    /// Character substitutions applied before phoneme-ID lookup (e.g. some voices map `"c"` to
    /// `["k"]`) — rare in practice but silently wrong output (falls through to "unknown
    /// phoneme, skip") if ignored on a voice that has one.
    #[serde(default)]
    pub phoneme_map: HashMap<String, Vec<String>>,
    pub phoneme_id_map: HashMap<String, Vec<i64>>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct PiperAudioConfig {
    pub sample_rate: u32,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct PiperEspeakConfig {
    /// espeak-ng voice/language code (e.g. `"pt-br"`) — passed straight to
    /// [`espeak_rs::text_to_phonemes`].
    pub voice: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct PiperInferenceConfig {
    #[serde(default = "default_noise_scale")]
    pub noise_scale: f32,
    #[serde(default = "default_length_scale")]
    pub length_scale: f32,
    #[serde(default = "default_noise_w")]
    pub noise_w: f32,
}

impl Default for PiperInferenceConfig {
    fn default() -> Self {
        Self {
            noise_scale: default_noise_scale(),
            length_scale: default_length_scale(),
            noise_w: default_noise_w(),
        }
    }
}

fn default_noise_scale() -> f32 {
    0.667
}
fn default_length_scale() -> f32 {
    1.0
}
fn default_noise_w() -> f32 {
    0.8
}

#[derive(Debug)]
pub enum TtsError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Phonemization(PhonemizationError),
    Onnx(String),
    /// `synthesize` produced no audio at all — e.g. `text` phonemized to nothing (all
    /// whitespace/punctuation with no speakable content).
    NoAudio,
}

impl std::fmt::Display for TtsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TtsError::Io(e) => write!(f, "I/O error: {e}"),
            TtsError::Json(e) => write!(f, "voice config parse error: {e}"),
            TtsError::Phonemization(e) => write!(f, "{e}"),
            TtsError::Onnx(e) => write!(f, "ONNX inference failed: {e}"),
            TtsError::NoAudio => write!(f, "no speakable content in the given text"),
        }
    }
}

impl std::error::Error for TtsError {}

/// Loads and parses a Piper voice's `.onnx.json` sidecar file.
pub fn load_voice_config(config_path: &Path) -> Result<PiperVoiceConfig, TtsError> {
    let bytes = std::fs::read(config_path).map_err(TtsError::Io)?;
    serde_json::from_slice(&bytes).map_err(TtsError::Json)
}

/// Splits an espeak IPA phoneme string into individual symbols by Unicode grapheme cluster
/// (keeps a base letter and any combining stress/length diacritic together as one symbol,
/// matching how Piper voices were trained — a plain `char` split would separate them).
fn split_phoneme_symbols(ipa: &str) -> Vec<String> {
    ipa.graphemes(true).map(str::to_string).collect()
}

/// Converts one sentence's IPA phoneme symbols into a Piper phoneme-ID sequence: BOS, then for
/// each symbol its ID(s) followed by PAD, then EOS — the exact scheme every published Piper
/// voice was trained on (see module docs). Applies `config.phoneme_map` substitutions first.
/// A symbol with no entry in `phoneme_id_map` (after substitution) is silently skipped — same
/// behavior as the reference implementation (unknown/untrainable symbols, e.g. some punctuation).
pub fn phonemes_to_ids(ipa: &str, config: &PiperVoiceConfig) -> Vec<i64> {
    let bos = config
        .phoneme_id_map
        .get(BOS_SYMBOL)
        .cloned()
        .unwrap_or_default();
    let eos = config
        .phoneme_id_map
        .get(EOS_SYMBOL)
        .cloned()
        .unwrap_or_default();
    let pad = config
        .phoneme_id_map
        .get(PAD_SYMBOL)
        .cloned()
        .unwrap_or_default();

    let mut ids = bos;
    for symbol in split_phoneme_symbols(ipa) {
        let mapped: Vec<String> = config
            .phoneme_map
            .get(&symbol)
            .cloned()
            .unwrap_or_else(|| vec![symbol]);
        for m in mapped {
            if let Some(symbol_ids) = config.phoneme_id_map.get(&m) {
                ids.extend(symbol_ids);
                ids.extend(&pad);
            }
        }
    }
    ids.extend(eos);
    ids
}

/// Phonemizes `text` with the default [`EspeakPhonemizer`] and runs Piper VITS inference.
pub fn synthesize(
    model_path: &Path,
    config: &PiperVoiceConfig,
    text: &str,
) -> Result<Vec<f32>, TtsError> {
    synthesize_with_phonemizer(model_path, config, text, &EspeakPhonemizer)
}

/// Phonemizes `text` with an explicit backend and runs Piper VITS inference sentence-by-sentence,
/// concatenating the results with a short silence gap between sentences. Returns raw `f32` samples
/// in `-1.0..=1.0` at `config.audio.sample_rate`.
///
/// `phonemizer` is invoked once per synthesis request. The dynamic dispatch therefore does not sit
/// inside the per-sentence ONNX inference loop.
pub fn synthesize_with_phonemizer(
    model_path: &Path,
    config: &PiperVoiceConfig,
    text: &str,
    phonemizer: &dyn Phonemizer,
) -> Result<Vec<f32>, TtsError> {
    let sentences = phonemizer
        .phonemize(text, &config.espeak.voice)
        .map_err(TtsError::Phonemization)?;
    let run = crate::onnx_models::piper_runner(model_path, config)?;
    assemble_sentences(config, &sentences, run)
}

/// Turns each phonemized sentence into ids, runs the model on them through `run` (ids in, raw
/// samples out) and joins the results with a short silence between sentences. Split from
/// [`synthesize_with_phonemizer`] so the sentence handling is testable without a Piper model.
pub(crate) fn assemble_sentences(
    config: &PiperVoiceConfig,
    sentences: &[String],
    mut run: impl FnMut(Vec<i64>) -> Result<Vec<f32>, TtsError>,
) -> Result<Vec<f32>, TtsError> {
    // A quarter-second of silence between sentences reads more naturally than butting them
    // together with no pause at all.
    let gap = vec![0.0f32; (config.audio.sample_rate as usize) / 4];
    let mut samples = Vec::new();

    for sentence in sentences {
        let ids = phonemes_to_ids(sentence, config);
        if ids.is_empty() {
            continue;
        }
        let spoken = run(ids)?;
        if !samples.is_empty() {
            samples.extend_from_slice(&gap);
        }
        samples.extend_from_slice(&spoken);
    }

    if samples.is_empty() {
        return Err(TtsError::NoAudio);
    }
    Ok(samples)
}

/// Writes `samples` (mono, `-1.0..=1.0`) to `out_path` as a 16-bit PCM WAV file at `sample_rate`.
pub fn write_wav(out_path: &Path, sample_rate: u32, samples: &[f32]) -> Result<(), TtsError> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer =
        hound::WavWriter::create(out_path, spec).map_err(|e| TtsError::Io(io_from_hound(e)))?;
    for &s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        writer
            .write_sample((clamped * i16::MAX as f32) as i16)
            .map_err(|e| TtsError::Io(io_from_hound(e)))?;
    }
    writer
        .finalize()
        .map_err(|e| TtsError::Io(io_from_hound(e)))?;
    Ok(())
}

fn io_from_hound(e: hound::Error) -> std::io::Error {
    match e {
        hound::Error::IoError(io) => io,
        other => std::io::Error::other(other.to_string()),
    }
}

#[cfg(test)]
#[path = "text_to_speech/text_to_speech_test.rs"]
mod tests;
