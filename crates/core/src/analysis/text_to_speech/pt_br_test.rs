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

fn one(text: &str) -> String {
    let sentences = PortuguesePhonemizer.phonemize(text, "pt-br").unwrap();
    assert_eq!(sentences.len(), 1, "{text:?} -> {sentences:?}");
    sentences.into_iter().next().unwrap()
}

#[test]
fn stress_marks_the_syllable_the_spelling_calls_for() {
    // Penultimate by default, last for -r/-l/-z, the written accent wins.
    assert_eq!(one("casa"), "kˈazæ");
    assert_eq!(one("ajustar"), "ˌaʒustˈar");
    assert_eq!(one("música"), "mˈuzikæ");
    assert_eq!(one("exportação"), "ˌespoɾətasˈɐ̃ʊ̃");
}

#[test]
fn final_unstressed_vowels_are_reduced() {
    assert_eq!(one("rato"), "xˈatʊ");
    assert_eq!(one("fonte"), "fˈoŋtʃy");
    assert_eq!(one("setas"), "sˈɛtæs");
}

#[test]
fn nasal_vowels_and_digraphs() {
    assert_eq!(one("comando"), "kˌomˈɐ̃ŋdʊ");
    assert_eq!(one("chuva"), "ʃˈuvæ");
    assert_eq!(one("nenhum"), "neɲˈu\u{303}ŋ");
    assert_eq!(one("não"), "nˈɐ̃ʊ̃");
}

#[test]
fn nasal_vowels_are_written_with_a_combining_tilde() {
    // The voice's symbol table has the tilde as its own symbol and no precomposed õ or ũ.
    for text in ["nenhum", "padrões", "não"] {
        assert!(
            one(text).chars().all(|c| c != '\u{f5}' && c != '\u{169}'),
            "{text}"
        );
    }
}

#[test]
fn t_and_d_become_affricates_before_i_and_a_final_e() {
    assert!(one("tia").starts_with("tʃ"));
    assert!(one("dia").starts_with("dʒ"));
    assert!(one("ajuste").ends_with("tʃy"));
}

#[test]
fn a_final_s_is_voiced_before_a_voiced_sound_across_words() {
    assert!(one("dos amigos").starts_with("dʊz"));
    assert!(one("dos pais").starts_with("dʊs"));
}

#[test]
fn punctuation_is_dropped_and_sentences_are_split() {
    let sentences = PortuguesePhonemizer
        .phonemize("Olá, mundo! Tudo bem? Sim.", "pt")
        .unwrap();
    assert_eq!(sentences.len(), 3);
    assert!(sentences
        .iter()
        .all(|s| !s.contains(',') && !s.contains('!')));
    assert!(sentences[0].contains(' '));
}

#[test]
fn only_portuguese_is_accepted() {
    for language in ["pt", "pt-br", "pt_BR", "PT-BR"] {
        assert!(PortuguesePhonemizer.phonemize("oi", language).is_ok());
    }
    let error = PortuguesePhonemizer
        .phonemize("hello", "en-us")
        .unwrap_err();
    assert_eq!(error.backend(), BACKEND);
}

#[test]
fn empty_or_symbol_only_text_yields_no_sentences() {
    assert!(PortuguesePhonemizer
        .phonemize("", "pt-br")
        .unwrap()
        .is_empty());
    assert!(PortuguesePhonemizer
        .phonemize("... !?", "pt-br")
        .unwrap()
        .is_empty());
}

#[test]
fn cardinals_follow_portuguese_usage() {
    assert_eq!(cardinal(0), "zero");
    assert_eq!(cardinal(21), "vinte e um");
    assert_eq!(cardinal(100), "cem");
    assert_eq!(cardinal(101), "cento e um");
    assert_eq!(cardinal(1000), "mil");
    assert_eq!(cardinal(2025), "dois mil e vinte e cinco");
    assert_eq!(
        cardinal(1_234_000),
        "um milhão duzentos e trinta e quatro mil"
    );
    assert_eq!(cardinal(1_500_000), "um milhão e quinhentos mil");
    assert_eq!(cardinal(2_000_000_000), "dois bilhões");
}

#[test]
fn numbers_in_the_text_are_spoken() {
    assert_eq!(one("21"), one("vinte e um"));
    assert_eq!(one("0,5"), one("zero vírgula cinco"));
    assert_eq!(
        spell_number("1234567890123", ""),
        spell_digits("1234567890123")
    );
}

#[test]
fn words_without_vowels_are_spelled_by_letter_name() {
    assert_eq!(one("srt"), one("esse erre tê"));
}

#[test]
fn every_symbol_written_exists_in_the_portuguese_voice() {
    // The pt_BR Faber voice's `phoneme_id_map`: a symbol outside it is dropped by
    // `phonemes_to_ids` without a trace.
    let voice = "abcdefhijklmnopqrstuvwxyzæçðøħŋœɐɑɒɔɕəɛɡɪɲɾʃʊʒɹʁˈˌː\u{303} ";
    let text = "O rato roeu a roupa do rei de Roma. A criança brincava na chuva com seu \
                cachorro. Exportação concluída com sucesso em quinze segundos, às 14h30. \
                Os padrões de áudio e vídeo não estão disponíveis.";
    for sentence in PortuguesePhonemizer.phonemize(text, "pt-br").unwrap() {
        for symbol in sentence.chars() {
            assert!(voice.contains(symbol), "{symbol:?} in {sentence:?}");
        }
    }
}
