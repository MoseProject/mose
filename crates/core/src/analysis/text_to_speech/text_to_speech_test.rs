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

use super::*;

fn fixture_config() -> PiperVoiceConfig {
    let mut phoneme_id_map = HashMap::new();
    phoneme_id_map.insert("_".to_string(), vec![0]);
    phoneme_id_map.insert("^".to_string(), vec![1]);
    phoneme_id_map.insert("$".to_string(), vec![2]);
    phoneme_id_map.insert("a".to_string(), vec![5]);
    phoneme_id_map.insert("t".to_string(), vec![10]);
    // A syllabic consonant — 'n' + U+0329 COMBINING VERTICAL LINE BELOW, one grapheme cluster
    // (unlike a spacing modifier like the stress mark 'ˈ', which is its own separate cluster).
    phoneme_id_map.insert("n\u{0329}".to_string(), vec![20]);

    let mut phoneme_map = HashMap::new();
    phoneme_map.insert("k".to_string(), vec!["t".to_string()]);

    PiperVoiceConfig {
        audio: PiperAudioConfig { sample_rate: 22050 },
        espeak: PiperEspeakConfig {
            voice: "pt-br".to_string(),
        },
        inference: PiperInferenceConfig::default(),
        num_speakers: 1,
        phoneme_map,
        phoneme_id_map,
    }
}

#[test]
fn parses_a_real_piper_voice_config() {
    // Trimmed from a real downloaded pt_BR-faber-medium.onnx.json, verified against Hugging
    // Face before depending on this schema.
    let json = r#"{
        "audio": {"sample_rate": 22050, "quality": "medium"},
        "espeak": {"voice": "pt-br"},
        "inference": {"noise_scale": 0.667, "length_scale": 1, "noise_w": 0.8},
        "phoneme_type": "espeak",
        "phoneme_map": {"c": ["k"]},
        "phoneme_id_map": {"_": [0], "^": [1], "$": [2], " ": [3]},
        "num_speakers": 1
    }"#;
    let config: PiperVoiceConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.audio.sample_rate, 22050);
    assert_eq!(config.espeak.voice, "pt-br");
    assert_eq!(config.num_speakers, 1);
    assert_eq!(config.phoneme_id_map.get("_"), Some(&vec![0]));
    assert_eq!(config.phoneme_map.get("c"), Some(&vec!["k".to_string()]));
}

#[test]
fn parses_config_missing_optional_fields() {
    // No "inference"/"phoneme_map"/"num_speakers" — must still parse via #[serde(default)].
    let json = r#"{
        "audio": {"sample_rate": 16000},
        "espeak": {"voice": "en-us"},
        "phoneme_id_map": {"_": [0]}
    }"#;
    let config: PiperVoiceConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.num_speakers, 0);
    assert!((config.inference.noise_scale - 0.667).abs() < 1e-6);
    assert!((config.inference.length_scale - 1.0).abs() < 1e-6);
}

#[test]
fn phonemes_to_ids_wraps_with_bos_and_eos() {
    let config = fixture_config();
    let ids = phonemes_to_ids("a", &config);
    assert_eq!(ids, vec![1, 5, 0, 2]); // BOS, 'a', PAD, EOS
}

#[test]
fn phonemes_to_ids_intersperses_pad_between_symbols() {
    let config = fixture_config();
    let ids = phonemes_to_ids("at", &config);
    assert_eq!(ids, vec![1, 5, 0, 10, 0, 2]); // BOS, 'a', PAD, 't', PAD, EOS
}

#[test]
fn phonemes_to_ids_applies_phoneme_map_substitution() {
    let config = fixture_config();
    // 'k' isn't in phoneme_id_map directly, but phoneme_map remaps it to 't'.
    let ids = phonemes_to_ids("k", &config);
    assert_eq!(ids, vec![1, 10, 0, 2]); // BOS, 't' (substituted), PAD, EOS
}

#[test]
fn phonemes_to_ids_skips_unknown_symbols() {
    let config = fixture_config();
    // 'z' has no entry and no substitution — skipped, not an error.
    let ids = phonemes_to_ids("az", &config);
    assert_eq!(ids, vec![1, 5, 0, 2]); // BOS, 'a', PAD, EOS ('z' silently dropped)
}

#[test]
fn phonemes_to_ids_keeps_a_combining_diacritic_with_its_base_char() {
    let config = fixture_config();
    // "n" + U+0329 is one grapheme cluster, one phoneme symbol — must NOT be split into two
    // lookups ('n' alone and the bare combining mark) that would both miss the map.
    let ids = phonemes_to_ids("n\u{0329}", &config);
    assert_eq!(ids, vec![1, 20, 0, 2]);
}

#[test]
fn phonemes_to_ids_empty_input_is_just_bos_and_eos() {
    let config = fixture_config();
    let ids = phonemes_to_ids("", &config);
    assert_eq!(ids, vec![1, 2]);
}

struct FailingPhonemizer;

impl Phonemizer for FailingPhonemizer {
    fn phonemize(&self, text: &str, language: &str) -> Result<Vec<String>, PhonemizationError> {
        assert_eq!(text, "Hello");
        assert_eq!(language, "pt-br");
        Err(PhonemizationError::new("test backend", "expected failure"))
    }
}

#[test]
fn synthesis_uses_the_injected_phonemizer_before_loading_the_model() {
    let error = synthesize_with_phonemizer(
        Path::new("model-must-not-be-opened.onnx"),
        &fixture_config(),
        "Hello",
        &FailingPhonemizer,
    )
    .unwrap_err();

    assert!(matches!(
        error,
        TtsError::Phonemization(ref source)
            if source.backend() == "test backend"
                && source.to_string() == "test backend phonemization failed: expected failure"
    ));
}

#[test]
fn write_wav_produces_a_readable_file() {
    let dir = std::env::temp_dir().join(format!("oca_tts_wav_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("out.wav");
    let samples = vec![0.0f32, 0.5, -0.5, 1.0, -1.0];
    write_wav(&path, 22050, &samples).unwrap();

    let reader = hound::WavReader::open(&path).unwrap();
    let spec = reader.spec();
    assert_eq!(spec.sample_rate, 22050);
    assert_eq!(spec.channels, 1);
    assert_eq!(reader.len(), samples.len() as u32);

    std::fs::remove_file(&path).unwrap();
    std::fs::remove_dir(&dir).unwrap();
}

struct FixedPhonemizer;

impl Phonemizer for FixedPhonemizer {
    fn phonemize(&self, _text: &str, _language: &str) -> Result<Vec<String>, PhonemizationError> {
        Ok(vec!["a".to_string()])
    }
}

#[test]
fn every_tts_error_has_a_readable_message() {
    assert!(TtsError::Io(std::io::Error::other("disk"))
        .to_string()
        .contains("disk"));
    let json = serde_json::from_str::<PiperVoiceConfig>("{").unwrap_err();
    assert!(TtsError::Json(json).to_string().contains("voice config"));
    let phonemization = TtsError::Phonemization(PhonemizationError::new("test", "no voice"));
    assert!(phonemization.to_string().contains("no voice"));
    assert!(TtsError::Onnx("bad model".into())
        .to_string()
        .contains("bad model"));
    assert!(!TtsError::NoAudio.to_string().is_empty());
}

#[test]
fn a_voice_config_loads_from_a_file_and_bad_files_are_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("voice.json");
    std::fs::write(
        &path,
        r#"{"audio":{"sample_rate":16000},"espeak":{"voice":"pt-br"},"phoneme_id_map":{"_":[0]}}"#,
    )
    .unwrap();
    assert_eq!(load_voice_config(&path).unwrap().audio.sample_rate, 16000);
    assert!(matches!(
        load_voice_config(&dir.path().join("missing.json")),
        Err(TtsError::Io(_))
    ));
    std::fs::write(&path, "{").unwrap();
    assert!(matches!(load_voice_config(&path), Err(TtsError::Json(_))));
}

#[test]
fn synthesis_reports_phonemization_and_model_failures() {
    let config = fixture_config();
    let model = Path::new("missing-model.onnx");
    assert!(matches!(
        synthesize_with_phonemizer(model, &config, "Hello", &FailingPhonemizer),
        Err(TtsError::Phonemization(_))
    ));
    assert!(matches!(
        synthesize_with_phonemizer(model, &config, "Hello", &FixedPhonemizer),
        Err(TtsError::Onnx(_))
    ));
    assert!(synthesize(model, &config, "Hello").is_err());
}

#[test]
fn the_default_phonemizer_speaks_portuguese_and_refuses_other_languages() {
    let sentences = PortuguesePhonemizer.phonemize("ola", "pt-br").unwrap();
    assert!(!sentences.is_empty());
    let error = PortuguesePhonemizer
        .phonemize("hello", "en-us")
        .unwrap_err();
    assert_eq!(error.backend(), "pt-BR rules");
}

#[test]
fn a_nasal_vowel_is_looked_up_as_its_base_letter_and_combining_tilde() {
    // Published Piper voices have the base letter and U+0303 as separate symbols and no
    // entry for the cluster.
    let mut config = fixture_config();
    config.phoneme_id_map.clear();
    for (symbol, id) in [("_", 0), ("^", 1), ("$", 2), ("ɐ", 7), ("\u{303}", 8)] {
        config.phoneme_id_map.insert(symbol.to_string(), vec![id]);
    }
    assert_eq!(phonemes_to_ids("ɐ\u{303}", &config), vec![1, 7, 0, 8, 0, 2]);
}

#[test]
fn wav_files_are_written_as_sixteen_bit_mono_and_failures_are_io_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("voice.wav");
    // Samples beyond the range are clamped.
    write_wav(&path, 8000, &[0.0, 0.5, 2.0, -2.0]).unwrap();
    let mut reader = hound::WavReader::open(&path).unwrap();
    assert_eq!(reader.spec().sample_rate, 8000);
    let samples: Vec<i16> = reader.samples::<i16>().map(Result::unwrap).collect();
    assert_eq!(samples, vec![0, 16383, i16::MAX, -i16::MAX]);

    let missing_dir = dir.path().join("no").join("such").join("dir.wav");
    assert!(matches!(
        write_wav(&missing_dir, 8000, &[0.0]),
        Err(TtsError::Io(_))
    ));
}

#[test]
fn hound_errors_become_io_errors() {
    let io = io_from_hound(hound::Error::IoError(std::io::Error::other("raw")));
    assert_eq!(io.to_string(), "raw");
    let other = io_from_hound(hound::Error::FormatError("odd format"));
    assert!(other.to_string().contains("odd format"));
}

#[test]
fn sentences_are_run_through_the_model_and_joined_with_a_quarter_second_of_silence() {
    let config = fixture_config();
    let mut calls: Vec<Vec<i64>> = Vec::new();
    let samples = assemble_sentences(&config, &["a".to_string(), "t".to_string()], |ids| {
        calls.push(ids);
        Ok(vec![0.5; 4])
    })
    .unwrap();
    assert_eq!(calls.len(), 2);
    // Two 4-sample sentences plus a quarter second (22050 / 4 samples) of silence between.
    assert_eq!(samples.len(), 4 + 22050 / 4 + 4);
    assert_eq!(samples[0], 0.5);
    assert_eq!(samples[4], 0.0);
}

#[test]
fn sentences_without_speakable_symbols_are_skipped_and_nothing_at_all_is_an_error() {
    // A voice that knows no symbol at all turns every sentence into zero ids.
    let mut config = fixture_config();
    config.phoneme_id_map.clear();
    let mut calls = 0;
    let result = assemble_sentences(&config, &["???".to_string(), String::new()], |_| {
        calls += 1;
        Ok(vec![0.1])
    });
    assert!(matches!(result, Err(TtsError::NoAudio)));
    assert_eq!(calls, 0);
    assert!(matches!(
        assemble_sentences(&config, &[], |_| Ok(vec![0.1])),
        Err(TtsError::NoAudio)
    ));
}

#[test]
fn a_model_failure_stops_the_synthesis() {
    let config = fixture_config();
    let result = assemble_sentences(&config, &["a".to_string()], |_| {
        Err(TtsError::Onnx("boom".into()))
    });
    assert!(matches!(result, Err(TtsError::Onnx(message)) if message == "boom"));
}

struct Words(Vec<String>);

impl Phonemizer for Words {
    fn phonemize(&self, _text: &str, _language: &str) -> Result<Vec<String>, PhonemizationError> {
        Ok(self.0.clone())
    }
}

#[test]
fn a_missing_model_file_is_an_onnx_error_after_phonemizing() {
    let config = fixture_config();
    let result = synthesize_with_phonemizer(
        Path::new("definitely-not-a-voice.onnx"),
        &config,
        "hello",
        &Words(vec!["a".into()]),
    );
    assert!(matches!(result, Err(TtsError::Onnx(_))));
}
