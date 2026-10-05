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

use std::sync::Mutex;

/// Converts text into one IPA string per sentence for a requested language.
///
/// Implementations are synchronous. Callers should keep phonemization off the UI thread when the
/// selected backend may block or perform substantial work.
pub trait Phonemizer {
    fn phonemize(&self, text: &str, language: &str) -> Result<Vec<String>, PhonemizationError>;
}

/// A backend-independent phonemization failure suitable for propagation through the TTS boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhonemizationError {
    backend: &'static str,
    message: String,
}

impl PhonemizationError {
    pub fn new(backend: &'static str, message: impl Into<String>) -> Self {
        Self {
            backend,
            message: message.into(),
        }
    }

    pub fn backend(&self) -> &'static str {
        self.backend
    }
}

impl std::fmt::Display for PhonemizationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} phonemization failed: {}", self.backend, self.message)
    }
}

impl std::error::Error for PhonemizationError {}

/// Lightweight, rule-based multilingual phonemizer backed by the bundled eSpeak NG engine.
#[derive(Debug, Default, Clone, Copy)]
pub struct EspeakPhonemizer;

// eSpeak keeps the selected voice and phonemization buffers in process-global native state.
// Serialize the complete voice-selection and conversion operation so concurrent TTS jobs cannot
// change each other's language or read a buffer while another call replaces it.
static ESPEAK_LOCK: Mutex<()> = Mutex::new(());

impl Phonemizer for EspeakPhonemizer {
    fn phonemize(&self, text: &str, language: &str) -> Result<Vec<String>, PhonemizationError> {
        let _guard = ESPEAK_LOCK.lock().map_err(|_| {
            PhonemizationError::new("eSpeak NG", "global phonemizer lock is poisoned")
        })?;

        espeak_rs::text_to_phonemes(text, language, None)
            .map_err(|error| PhonemizationError::new("eSpeak NG", error.0))
    }
}
