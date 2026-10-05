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

//! A Brazilian Portuguese grapheme-to-phoneme converter written for this project, so the text to
//! speech path needs no GPL phonemizer.
//!
//! The Piper voices were trained on IPA strings in the style of eSpeak NG (for instance `ʊ` for a
//! final unstressed `o`, `æ` for a final `a`, `dʒ` for `d` before `i`, `ˈ` before the stressed
//! vowel), so this module writes the same symbol set: the voice's own `phoneme_id_map` is what
//! turns the symbols into model input. The rules come from Portuguese phonology (syllable
//! nuclei, the written accent and ending rules for stress, nasal vowels, coda `l`/`r`/`s`,
//! palatalization of `t`/`d`). The development oracle was eSpeak's *output* on Portuguese text;
//! none of its rule files or source was read or copied.
//!
//! Pipeline: text -> tokens (words, numbers, sentence breaks) -> numbers spelled out -> one
//! phoneme string per sentence, words separated by a space. Punctuation is dropped, as the
//! previous backend did.

use super::phonemizer::{PhonemizationError, Phonemizer};

const BACKEND: &str = "pt-BR rules";
/// Consonants that voice a preceding `s` (and a word-final `s` before the next word).
const VOICING_CONSONANTS: &str = "bdgvzjlmnr";

/// Rule-based Brazilian Portuguese phonemizer. Accepts `pt`, `pt-br`, `pt_BR` (any case).
#[derive(Debug, Default, Clone, Copy)]
pub struct PortuguesePhonemizer;

impl Phonemizer for PortuguesePhonemizer {
    fn phonemize(&self, text: &str, language: &str) -> Result<Vec<String>, PhonemizationError> {
        if !is_portuguese(language) {
            return Err(PhonemizationError::new(
                BACKEND,
                format!("unsupported language {language:?}; only Portuguese is available"),
            ));
        }
        Ok(phonemize_text(text))
    }
}

fn is_portuguese(language: &str) -> bool {
    let lower = language.to_lowercase().replace('_', "-");
    lower == "pt" || lower.starts_with("pt-")
}

// ---------------------------------------------------------------------------------------------
// Tokens, numbers and sentences
// ---------------------------------------------------------------------------------------------

enum Token {
    Word(String),
    Break,
}

fn tokenize(text: &str) -> Vec<Token> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_alphabetic() {
            let start = i;
            while i < chars.len() && chars[i].is_alphabetic() {
                i += 1;
            }
            tokens.push(Token::Word(
                chars[start..i].iter().collect::<String>().to_lowercase(),
            ));
        } else if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            let integer: String = chars[start..i].iter().collect();
            let mut decimals = String::new();
            let decimal_mark = i + 1 < chars.len()
                && (chars[i] == ',' || chars[i] == '.')
                && chars[i + 1].is_ascii_digit();
            if decimal_mark {
                i += 1;
                let from = i;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                decimals = chars[from..i].iter().collect();
            }
            for word in spell_number(&integer, &decimals).split_whitespace() {
                tokens.push(Token::Word(word.to_string()));
            }
        } else {
            if matches!(c, '.' | '!' | '?' | ';' | ':' | '\n' | '…') {
                tokens.push(Token::Break);
            }
            i += 1;
        }
    }
    tokens
}

const UNITS: [&str; 20] = [
    "zero",
    "um",
    "dois",
    "três",
    "quatro",
    "cinco",
    "seis",
    "sete",
    "oito",
    "nove",
    "dez",
    "onze",
    "doze",
    "treze",
    "quatorze",
    "quinze",
    "dezesseis",
    "dezessete",
    "dezoito",
    "dezenove",
];
const TENS: [&str; 10] = [
    "",
    "",
    "vinte",
    "trinta",
    "quarenta",
    "cinquenta",
    "sessenta",
    "setenta",
    "oitenta",
    "noventa",
];
const HUNDREDS: [&str; 10] = [
    "",
    "cento",
    "duzentos",
    "trezentos",
    "quatrocentos",
    "quinhentos",
    "seiscentos",
    "setecentos",
    "oitocentos",
    "novecentos",
];
const MAX_SPELLED_DIGITS: usize = 12;

/// Spells a number in Portuguese: the integer part as a cardinal (up to 12 digits, else digit by
/// digit), then `vírgula` and the decimals digit by digit.
fn spell_number(integer: &str, decimals: &str) -> String {
    let mut out = if integer.len() <= MAX_SPELLED_DIGITS {
        cardinal(integer.parse::<u64>().unwrap_or(0))
    } else {
        spell_digits(integer)
    };
    if !decimals.is_empty() {
        out.push_str(" vírgula ");
        out.push_str(&spell_digits(decimals));
    }
    out
}

fn spell_digits(digits: &str) -> String {
    digits
        .chars()
        .filter_map(|c| c.to_digit(10))
        .map(|d| UNITS[d as usize])
        .collect::<Vec<_>>()
        .join(" ")
}

fn below_thousand(n: u64) -> String {
    match n {
        0..=19 => UNITS[n as usize].to_string(),
        20..=99 => {
            let tens = TENS[(n / 10) as usize];
            match n % 10 {
                0 => tens.to_string(),
                unit => format!("{tens} e {}", UNITS[unit as usize]),
            }
        }
        100 => "cem".to_string(),
        _ => {
            let hundreds = HUNDREDS[(n / 100) as usize];
            match n % 100 {
                0 => hundreds.to_string(),
                rest => format!("{hundreds} e {}", below_thousand(rest)),
            }
        }
    }
}

fn cardinal(n: u64) -> String {
    if n < 1000 {
        return below_thousand(n);
    }
    const SCALES: [(u64, &str, &str); 3] = [
        (1_000_000_000, "bilhão", "bilhões"),
        (1_000_000, "milhão", "milhões"),
        (1_000, "mil", "mil"),
    ];
    // Each part keeps the number it spells: "e" joins the last part when that number is below
    // 100 or a round hundred (um milhão e quinhentos mil, but um milhão duzentos e trinta mil).
    let mut parts: Vec<(String, u64)> = Vec::new();
    let mut rest = n;
    for (size, singular, plural) in SCALES {
        let count = rest / size;
        rest %= size;
        if count == 0 {
            continue;
        }
        let text = if size == 1_000 && count == 1 {
            "mil".to_string()
        } else {
            let name = if count == 1 { singular } else { plural };
            format!("{} {name}", below_thousand(count))
        };
        parts.push((text, count));
    }
    if rest > 0 {
        parts.push((below_thousand(rest), rest));
    }
    let Some((last, last_value)) = parts.pop() else {
        return String::new();
    };
    if parts.is_empty() {
        return last;
    }
    let head = parts
        .into_iter()
        .map(|(text, _)| text)
        .collect::<Vec<_>>()
        .join(" ");
    let joiner = if last_value < 100 || last_value % 100 == 0 {
        " e "
    } else {
        " "
    };
    format!("{head}{joiner}{last}")
}

/// The spoken name of a letter, used for words with no vowel (`srt`, `x`).
fn letter_name(c: char) -> &'static str {
    match c {
        'a' => "á",
        'b' => "bê",
        'c' => "cê",
        'd' => "dê",
        'e' => "é",
        'f' => "efe",
        'g' => "gê",
        'h' => "agá",
        'i' => "i",
        'j' => "jota",
        'k' => "cá",
        'l' => "ele",
        'm' => "eme",
        'n' => "ene",
        'o' => "ó",
        'p' => "pê",
        'q' => "quê",
        'r' => "erre",
        's' => "esse",
        't' => "tê",
        'u' => "u",
        'v' => "vê",
        'w' => "dáblio",
        'x' => "xis",
        'y' => "ípsilon",
        'z' => "zê",
        _ => "",
    }
}

fn phonemize_text(text: &str) -> Vec<String> {
    let mut sentences: Vec<Vec<String>> = vec![Vec::new()];
    for token in tokenize(text) {
        match token {
            Token::Break => {
                if sentences.last().is_some_and(|s| !s.is_empty()) {
                    sentences.push(Vec::new());
                }
            }
            Token::Word(word) => {
                if let Some(sentence) = sentences.last_mut() {
                    sentence.push(word);
                }
            }
        }
    }
    sentences
        .into_iter()
        .filter(|words| !words.is_empty())
        .map(|words| phonemize_sentence(&words))
        .collect()
}

fn phonemize_sentence(words: &[String]) -> String {
    let spoken = expand_spelled_words(words);
    let mut out = Vec::with_capacity(spoken.len());
    for (i, word) in spoken.iter().enumerate() {
        let next_first = spoken.get(i + 1).and_then(|w| w.chars().next());
        out.push(phonemize_word(word, next_first));
    }
    out.join(" ")
}

/// Replaces a vowel-less word of up to five letters, and any single letter, by the names of its
/// letters.
fn expand_spelled_words(words: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(words.len());
    for word in words {
        let letters = word.chars().count();
        let spelled =
            (!word.chars().any(is_vowel) && letters <= 5) || (letters == 1 && word != "a");
        if spelled && word != "e" && word != "o" {
            out.extend(
                word.chars()
                    .map(letter_name)
                    .filter(|name| !name.is_empty())
                    .map(String::from),
            );
        } else {
            out.push(word.clone());
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Word to phonemes
// ---------------------------------------------------------------------------------------------

fn is_vowel(c: char) -> bool {
    "aeiouáàâãéêíóôõúü".contains(c)
}

fn is_open_vowel(c: char) -> bool {
    "aeoáéóâêôãõ".contains(c)
}

fn is_front(c: char) -> bool {
    "eéêií".contains(c)
}

fn is_accented(c: char) -> bool {
    "áéíóúâêôãõ".contains(c)
}

fn plain(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ã' => 'a',
        'é' | 'ê' => 'e',
        'í' => 'i',
        'ó' | 'ô' | 'õ' => 'o',
        'ú' | 'ü' => 'u',
        other => other,
    }
}

/// Words spoken without their own stress, with a fixed pronunciation (the caller adds an `s`
/// ending).
fn function_word(word: &str) -> Option<&'static str> {
    Some(match word {
        "a" | "à" => "a",
        "o" => "ʊ",
        "e" => "i",
        "é" => "ɛ",
        "ao" => "aʊ",
        "de" => "dʒy",
        "que" => "ky",
        "se" => "sy",
        "do" => "dʊ",
        "da" => "dæ",
        "no" => "nʊ",
        "na" => "na",
        "um" => "ũŋ",
        "uma" => "umæ",
        "com" => "koŋ",
        "em" => "ˈeɪŋ",
        "por" => "poɾ",
        "ou" => "ow",
        "mas" => "ma",
        "me" => "my",
        "te" => "tʃy",
        _ => return None,
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Consonant,
    Nucleus,
}

/// One consonant grapheme or one vowel nucleus (a vowel or a diphthong) of a word.
struct Item {
    kind: Kind,
    text: String,
}

impl Item {
    fn consonant(text: impl Into<String>) -> Self {
        Self {
            kind: Kind::Consonant,
            text: text.into(),
        }
    }

    fn nucleus(text: impl Into<String>) -> Self {
        Self {
            kind: Kind::Nucleus,
            text: text.into(),
        }
    }

    fn is_nucleus(&self) -> bool {
        self.kind == Kind::Nucleus
    }
}

/// Graphemic units: single vowels and consonants, with the digraphs `ch lh nh rr ss sç`, and the
/// silent `u` of `que qui gue gui` and `sc`/`xc` before a front vowel folded into one unit.
fn units_of(word: &[char]) -> Vec<(bool, String)> {
    let mut units = Vec::new();
    let mut i = 0;
    while i < word.len() {
        let c = word[i];
        let two: String = word[i..(i + 2).min(word.len())].iter().collect();
        let before_front = word.get(i + 2).is_some_and(|&n| is_front(n));
        if is_vowel(c) {
            units.push((true, c.to_string()));
            i += 1;
        } else if matches!(two.as_str(), "ch" | "lh" | "nh" | "rr" | "ss" | "sç")
            || (matches!(two.as_str(), "qu" | "gu" | "sc" | "xc") && before_front)
        {
            units.push((false, two));
            i += 2;
        } else {
            units.push((false, c.to_string()));
            i += 1;
        }
    }
    units
}

/// Splits a run of vowel letters into nuclei: falling diphthongs (`ai ei oi ui au eu ou iu`),
/// nasal ones (`ão ãe õe`), a final rising pair (`ua`, `eo`, and `ia` after an accented vowel),
/// and hiatus otherwise.
fn nuclei(run: &[char], prev_consonant: &str, tail: &str, accented: bool) -> Vec<String> {
    let mut run = run;
    let mut glide = None;
    if run.len() > 1 && run[0] == 'u' && (prev_consonant == "q" || prev_consonant == "g") {
        glide = Some('u');
        run = &run[1..];
    }
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < run.len() {
        let c = run[i];
        if let Some(&next) = run.get(i + 1) {
            let at_run_end = i + 2 == run.len();
            let pair: String = [c, next].iter().collect();
            if matches!(pair.as_str(), "ão" | "ãe" | "õe") {
                out.push(pair);
                i += 2;
                continue;
            }
            if c == 'u' && next == 'i' && at_run_end && tail == "r" {
                out.push(c.to_string());
                i += 1;
                continue;
            }
            if is_open_vowel(c) && (next == 'i' || next == 'u') {
                if run.get(i + 2).is_some_and(|&n| is_open_vowel(n)) {
                    out.push(c.to_string());
                    i += 1;
                } else {
                    out.push(pair);
                    i += 2;
                }
                continue;
            }
            if (c == 'i' || c == 'u') && (next == 'i' || next == 'u') && c != next {
                out.push(pair);
                i += 2;
                continue;
            }
            let rising =
                c == 'u' || (c == 'e' && (next == 'a' || next == 'o')) || (c == 'i' && accented);
            if rising && "aeoãõ".contains(next) && at_run_end && (tail.is_empty() || tail == "s")
            {
                out.push(pair);
                i += 2;
                continue;
            }
        }
        out.push(c.to_string());
        i += 1;
    }
    if let Some(g) = glide {
        match out.first_mut() {
            Some(first) => first.insert(0, g),
            None => out.push(g.to_string()),
        }
    }
    out
}

fn parse(word: &str) -> Vec<Item> {
    let chars: Vec<char> = word.chars().collect();
    let accented = chars.iter().any(|&c| "áéíóúâêô".contains(c));
    let units = units_of(&chars);
    let mut items: Vec<Item> = Vec::new();
    let mut i = 0;
    while i < units.len() {
        if !units[i].0 {
            items.push(Item::consonant(units[i].1.clone()));
            i += 1;
            continue;
        }
        let mut j = i;
        while j < units.len() && units[j].0 {
            j += 1;
        }
        let run: Vec<char> = units[i..j].iter().flat_map(|u| u.1.chars()).collect();
        let prev = items
            .last()
            .filter(|it| !it.is_nucleus())
            .map_or(String::new(), |it| it.text.clone());
        let tail: String = units[j..].iter().map(|u| u.1.as_str()).collect();
        for text in nuclei(&run, &prev, &tail, accented) {
            items.push(Item::nucleus(text));
        }
        i = j;
    }
    items
}

fn nucleus_positions(items: &[Item]) -> Vec<usize> {
    items
        .iter()
        .enumerate()
        .filter(|(_, it)| it.is_nucleus())
        .map(|(k, _)| k)
        .collect()
}

/// The stressed nucleus: the one with a written accent; else by ending (`r l z x`, a final
/// `i`/`u`/`im`/`um` are oxytone), else the second to last.
fn find_stress(items: &[Item]) -> Option<usize> {
    let nuclei = nucleus_positions(items);
    let &last = nuclei.last()?;
    if let Some(&k) = nuclei
        .iter()
        .find(|&&k| items[k].text.chars().any(is_accented))
    {
        return Some(k);
    }
    if nuclei.len() == 1 {
        return Some(last);
    }
    let coda: String = items[last + 1..]
        .iter()
        .map(|it| it.text.as_str())
        .collect();
    let last_text = items[last].text.as_str();
    if matches!(coda.as_str(), "r" | "l" | "z" | "x") {
        return Some(last);
    }
    if (last_text == "i" || last_text == "u") && matches!(coda.as_str(), "" | "s" | "m" | "ns") {
        return Some(last);
    }
    Some(nuclei[nuclei.len() - 2])
}

const SINGLE_SYLLABLE_UNSTRESSED: [&str; 8] = ["é", "ao", "aos", "à", "a", "o", "as", "os"];

fn phonemize_word(word: &str, next_first: Option<char>) -> String {
    if let Some(base) = function_word(word) {
        return base.to_string();
    }
    if let Some(base) = word
        .strip_suffix('s')
        .filter(|_| word != "mas")
        .and_then(function_word)
    {
        let voiced = next_first.is_some_and(|c| is_vowel(c) || VOICING_CONSONANTS.contains(c));
        return format!("{base}{}", if voiced { 'z' } else { 's' });
    }
    let items = parse(word);
    let positions = nucleus_positions(&items);
    let stress = find_stress(&items)
        .filter(|_| !(positions.len() == 1 && SINGLE_SYLLABLE_UNSTRESSED.contains(&word)));
    let secondary = secondary_stress(&positions, stress);
    let ctx = Context {
        items: &items,
        next_first,
    };
    let mut out = String::new();
    for (k, item) in items.iter().enumerate() {
        if !item.is_nucleus() {
            out.push_str(&ctx.consonant(k));
            continue;
        }
        let mark = if Some(k) == stress {
            "ˈ"
        } else if secondary.contains(&k) {
            "ˌ"
        } else {
            ""
        };
        let vowel = ctx.vowel(k, Some(k) == stress);
        if vowel.contains(GLIDE_MARK_SLOT) {
            out.push_str(&vowel.replace(GLIDE_MARK_SLOT, mark));
        } else {
            out.push_str(mark);
            out.push_str(&vowel);
        }
    }
    out
}

/// Secondary stress: the first syllable, then every second one, never next to the stressed
/// syllable (the first excepted), and only in words of three or more syllables.
fn secondary_stress(positions: &[usize], stress: Option<usize>) -> Vec<usize> {
    let Some(stress_item) = stress.filter(|_| positions.len() >= 3) else {
        return Vec::new();
    };
    let stress_pos = positions
        .iter()
        .position(|&k| k == stress_item)
        .unwrap_or(0);
    positions
        .iter()
        .enumerate()
        .take(stress_pos)
        .filter(|(p, _)| p % 2 == 0 && (*p == 0 || stress_pos - p >= 2))
        .map(|(_, &k)| k)
        .collect()
}

/// Placeholder inside a vowel string for where the stress mark goes when a glide comes first
/// (`kwˈa`, not `kˈwa`).
const GLIDE_MARK_SLOT: char = '§';

struct Context<'a> {
    items: &'a [Item],
    next_first: Option<char>,
}

impl Context<'_> {
    fn nucleus_text(&self, k: usize) -> &str {
        self.items
            .get(k)
            .filter(|it| it.is_nucleus())
            .map_or("", |it| it.text.as_str())
    }

    fn is_last(&self, k: usize) -> bool {
        k + 1 >= self.items.len()
    }

    fn consonant(&self, k: usize) -> String {
        let text = self.items[k].text.as_str();
        let next = self.items.get(k + 1);
        let prev = k.checked_sub(1).and_then(|p| self.items.get(p));
        let next_nucleus = self.nucleus_text(k + 1);
        let next_starts = next_nucleus.chars().next();
        let front_next = next_starts.is_some_and(is_front);
        let onset = next.is_some_and(Item::is_nucleus);
        let prev_is_nucleus = prev.is_some_and(Item::is_nucleus);
        // A nasal consonant closing a syllable is absorbed by the nasal vowel before it.
        if matches!(text, "m" | "n") && prev_is_nucleus && !onset {
            return String::new();
        }
        match text {
            "b" | "f" | "p" | "v" | "k" | "w" | "m" | "n" => text.to_string(),
            "z" => (if onset { "z" } else { "s" }).to_string(),
            "d" | "t" => {
                let final_e = next_nucleus == "e"
                    && self
                        .items
                        .get(k + 2)
                        .is_none_or(|n| n.text == "s" && self.is_last(k + 2));
                let palatal = matches!(next_starts, Some('i' | 'í')) || final_e;
                let affricate = if text == "d" { "dʒ" } else { "tʃ" };
                (if palatal { affricate } else { text }).to_string()
            }
            "c" => (if front_next { "s" } else { "k" }).to_string(),
            "ç" | "sç" | "sc" | "xc" | "ss" => "s".to_string(),
            "g" => (if front_next { "ʒ" } else { "ɡ" }).to_string(),
            "gu" => "ɡ".to_string(),
            "qu" | "q" => "k".to_string(),
            "h" => String::new(),
            "j" => "ʒ".to_string(),
            "l" => self.l(k, onset, prev),
            "lh" => "lj".to_string(),
            "nh" => "ɲ".to_string(),
            "ch" => "ʃ".to_string(),
            "rr" => "x".to_string(),
            "r" => self.r(k, onset, prev),
            "s" => self.s(k, onset, prev),
            "x" => self.x(k, prev, next_nucleus),
            other => other.to_string(),
        }
    }

    /// `l` before a vowel is `l`; closing a syllable it is `ʊ` after `a`/final `e`, `l` after
    /// `u`, `w` otherwise.
    fn l(&self, k: usize, onset: bool, prev: Option<&Item>) -> String {
        if onset {
            return "l".to_string();
        }
        let before = prev
            .filter(|p| p.is_nucleus())
            .and_then(|p| p.text.chars().last());
        match before {
            Some('u') => "l",
            Some('a' | 'á') => "ʊ",
            Some('e' | 'é') if self.is_last(k) => "ʊ",
            _ => "w",
        }
        .to_string()
    }

    /// Word-initial and after `n l s`: `x`; between vowels `ɾ`, after a consonant in a cluster
    /// `r`; closing a syllable `ɾə`, and word-final `r` (`ɾ` before a vowel-initial word).
    fn r(&self, k: usize, onset: bool, prev: Option<&Item>) -> String {
        let after_consonant = prev.is_some_and(|p| !p.is_nucleus());
        let strong =
            prev.is_none_or(|p| !p.is_nucleus() && matches!(p.text.as_str(), "n" | "l" | "s"));
        if strong {
            return "x".to_string();
        }
        if onset {
            return (if after_consonant { "r" } else { "ɾ" }).to_string();
        }
        if self.is_last(k) {
            return (if self.next_first.is_some_and(is_vowel) {
                "ɾ"
            } else {
                "r"
            })
            .to_string();
        }
        "ɾə".to_string()
    }

    /// `s` between vowels is `z`; closing a syllable or ending a word it is `z` before a voiced
    /// consonant or a vowel (also across words) and `s` otherwise.
    fn s(&self, k: usize, onset: bool, prev: Option<&Item>) -> String {
        if onset {
            return (if prev.is_some_and(Item::is_nucleus) {
                "z"
            } else {
                "s"
            })
            .to_string();
        }
        let voiced = match self.items.get(k + 1) {
            None => {
                let nasal_plural = prev.is_some_and(|p| matches!(p.text.as_str(), "õe" | "ãe"));
                nasal_plural
                    || self
                        .next_first
                        .is_some_and(|c| is_vowel(c) || VOICING_CONSONANTS.contains(c))
            }
            Some(next) => next
                .text
                .chars()
                .next()
                .is_some_and(|c| VOICING_CONSONANTS.contains(c)),
        };
        (if voiced { "z" } else { "s" }).to_string()
    }

    /// `x`: `ʃ`, except `ex` + vowel (`z`) and `ex` + consonant (`s`) at the start of a word.
    fn x(&self, k: usize, prev: Option<&Item>, next_nucleus: &str) -> String {
        if k == 1 && prev.is_some_and(|p| p.text == "e") {
            return (if next_nucleus.is_empty() { "s" } else { "z" }).to_string();
        }
        "ʃ".to_string()
    }

    fn vowel(&self, k: usize, stressed: bool) -> String {
        let text = self.items[k].text.as_str();
        let next = self.items.get(k + 1);
        let next_next = self.items.get(k + 2);
        let final_syllable = self.items[k + 1..].iter().all(|it| !it.is_nucleus());
        let coda: String = self.items[k + 1..]
            .iter()
            .map(|it| it.text.as_str())
            .collect();
        let chars: Vec<char> = text.chars().collect();
        let base: Vec<char> = chars.iter().map(|&c| plain(c)).collect();
        let plural = coda == "s";
        match text {
            "ão" => return "ɐ̃ʊ̃".to_string(),
            "ãe" => return (if plural { "ɐ̃j" } else { "ɐ̃ɪ̃" }).to_string(),
            "õe" => return (if plural { "õj" } else { "õɪ̃" }).to_string(),
            "ã" => return "ɐ̃".to_string(),
            "õ" => return "õ".to_string(),
            _ => {}
        }
        if base == ['a'] && !stressed && coda == "m" && final_syllable {
            return "ɐ̃ʊ̃".to_string();
        }
        let next_is = |names: &[&str]| {
            next.is_some_and(|n| !n.is_nucleus() && names.contains(&n.text.as_str()))
        };
        let consonant_then_nucleus = next_next.is_some_and(Item::is_nucleus);
        let nasal_onset_next = next_is(&["n", "m", "nh"]) && consonant_then_nucleus;
        if stressed && base == ['a'] && matches!(chars[0], 'a' | 'á' | 'â') && nasal_onset_next {
            return "ɐ̃".to_string();
        }
        if next_is(&["m", "n"]) && !consonant_then_nucleus && base.len() == 1 {
            let closing = if next_next.is_some_and(|n| matches!(n.text.as_str(), "p" | "b")) {
                "m"
            } else {
                "ŋ"
            };
            let nasal = match base[0] {
                'a' => "ɐ̃",
                'e' => "eɪ",
                'o' => "o",
                'u' => "ũ",
                _ => "i",
            };
            return format!("{nasal}{closing}");
        }
        if base.len() == 2 {
            return self.two_letter_nucleus(text, &chars, &base, stressed, final_syllable, &coda);
        }
        if base.len() >= 3 {
            return text.to_string();
        }
        let oral = self.oral(base[0], chars[0], stressed, final_syllable, &coda);
        if oral == "a" && !stressed && nasal_onset_next {
            return "æ".to_string();
        }
        let before_plain_consonant = |names: &[&str]| next_is(names) && consonant_then_nucleus;
        if stressed && chars[0] == 'e' && before_plain_consonant(&["t", "c"]) {
            return "ɛ".to_string();
        }
        if stressed && chars[0] == 'o' && before_plain_consonant(&["l"]) {
            return "ɔ".to_string();
        }
        oral
    }

    fn two_letter_nucleus(
        &self,
        text: &str,
        chars: &[char],
        base: &[char],
        stressed: bool,
        final_syllable: bool,
        coda: &str,
    ) -> String {
        if text == "ou" {
            return "ow".to_string();
        }
        if text == "ui" {
            return "ui".to_string();
        }
        if (base[1] == 'i' || base[1] == 'u') && is_open_vowel(chars[0]) {
            let glide = if base[1] == 'i' { "ɪ" } else { "ʊ" };
            return format!(
                "{}{glide}",
                self.oral(base[0], chars[0], stressed, false, "")
            );
        }
        let glide = if base[0] == 'u' { 'w' } else { 'j' };
        let vowel = self.oral(base[1], chars[1], stressed, final_syllable, coda);
        format!("{glide}{GLIDE_MARK_SLOT}{vowel}")
    }

    fn oral(
        &self,
        base: char,
        original: char,
        stressed: bool,
        final_syllable: bool,
        coda: &str,
    ) -> String {
        let open_end = final_syllable && (coda.is_empty() || coda == "s");
        let before_vowel_word = self.next_first.is_some_and(is_vowel);
        if !stressed && final_syllable && coda.is_empty() && before_vowel_word {
            match base {
                'o' => return "w".to_string(),
                'e' => return "i".to_string(),
                _ => {}
            }
        }
        match base {
            'a' => (if stressed || !open_end { "a" } else { "æ" }).to_string(),
            'e' if stressed => (if original == 'é' { "ɛ" } else { "e" }).to_string(),
            'e' => (if open_end { "y" } else { "e" }).to_string(),
            'o' if stressed => (if original == 'ó' { "ɔ" } else { "o" }).to_string(),
            'o' => (if open_end { "ʊ" } else { "o" }).to_string(),
            other => other.to_string(),
        }
    }
}

#[cfg(test)]
#[path = "pt_br_test.rs"]
mod pt_br_test;
