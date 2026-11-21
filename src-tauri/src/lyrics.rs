//! From score syllables to OpenUtau note lyrics (DiffSinger phonemizers).
//! Reference implementation: `tools/mscz_to_ustx.py`.
//!
//! OpenUtau DiffSinger convention: the whole word on its first note, `+` on the notes of the following
//! syllables (phonemes are spread vowel by vowel), `+~` for melisma (same vowel held).
//! `word[ph ph ...]` gives an explicit pronunciation (phonetic hint).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::score::{Note, Part, Syllabic};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    It,
    En,
    /// Ecclesiastical Latin, sung with the Italian phonemes.
    La,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::It => "it",
            Lang::En => "en",
            Lang::La => "la",
        }
    }

    /// Language of the dictionary and G2p used to sing it (Latin uses the Italian ones).
    pub fn g2p_lang(self) -> Lang {
        match self {
            Lang::La => Lang::It,
            l => l,
        }
    }

    /// OpenUtau phonemizer class.
    pub fn phonemizer(self) -> &'static str {
        match self {
            Lang::It | Lang::La => "OpenUtau.Core.DiffSinger.DiffSingerItalianPhonemizer",
            // DIFFS EN+: uses ax / dx / q, closer to native-English voicebanks such as Tiger DS
            Lang::En => "OpenUtau.Core.DiffSinger.DiffSingerARPAPlusEnglishPhonemizer",
        }
    }
}

/// Words the dictionaries don't know (archaic forms, elisions on one note).
fn exception(lang: Lang, word: &str) -> Option<&'static str> {
    match lang {
        Lang::En => Some(match word {
            "ask'd" => "asked",
            "tatter'd" => "tattered",
            "feel'st" => "feel'st[f iy l s t]",
            "couldst" => "couldst[k uh d s t]",
            "alleating" => "alleating[ao l iy t ih ng]", // Shakespeare: "all-eating"
            "weed/of" => "weedof[w iy d ah v]",
            _ => return None,
        }),
        Lang::It | Lang::La => None,
    }
}

/// Stressed i before a vowel: full vowel, not semivowel, even when sung on one note.
const IT_HIATUS: &[&str] = &["dio", "mio", "mia", "mie", "io", "zio", "zia", "via", "pio", "pia", "sia", "spia", "fruscio"];

const STOP_IT: &[&str] = &["il", "la", "di", "che", "e", "del", "della", "nel", "dei", "a", "noi", "mi", "su"];
const STOP_EN: &[&str] = &["the", "and", "of", "thy", "to", "in", "my", "when", "be", "a", "this", "thou"];
/// Latin words that are not Italian words too.
const STOP_LA: &[&str] = &[
    "et", "est", "qui", "quae", "cum", "sunt", "nobis", "deus", "dominus", "domine", "sanctus", "agnus", "tuam",
    "eius", "mundi", "peccata", "excelsis", "kyrie", "eleison", "christe", "pacem", "spiritu", "filius", "patris",
    "tollis", "miserere", "benedictus", "nomine", "ad", "sicut", "erat", "nunc", "semper", "saecula",
    "saeculorum", "domini", "plena", "gratia", "tecum", "nostrum", "requiem", "deo", "pax", "hominibus", "laudamus", "tibi", "omnipotens",
    "cuius", "corpus", "natum", "verum", "virgine", "sanguine", "cruce", "crucem", "hoc", "ecce", "panis", "angelorum",
];
/// Words of the other lists that are also Latin: they don't count against Latin.
const ALSO_LATIN: &[&str] = &["in", "a", "e"];

/// The words of the lyrics (syllables joined, lower case, without punctuation and accents).
fn words(notes: &[Note]) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    for l in notes.iter().filter_map(|n| n.lyric.as_ref()) {
        cur.push_str(&l.text);
        if matches!(l.syllabic, Syllabic::Single | Syllabic::End) {
            out.extend(
                cur.split_whitespace()
                    .map(|w| w.chars().filter(|c| c.is_alphabetic()).map(unaccent).collect::<String>().to_lowercase())
                    .filter(|w| !w.is_empty()),
            );
            cur.clear();
        }
    }
    out
}

pub fn detect_lang(parts: &[Part]) -> Lang {
    let (mut it, mut en, mut la) = (0, 0, 0);
    // the Italian and English votes without the words Latin has too
    let (mut it_only, mut en_only) = (0, 0);
    for w in parts.iter().flat_map(|p| words(&p.notes)) {
        let w = w.as_str();
        let shared = ALSO_LATIN.contains(&w) as i32;
        it += STOP_IT.contains(&w) as i32;
        en += STOP_EN.contains(&w) as i32;
        it_only += STOP_IT.contains(&w) as i32 * (1 - shared);
        en_only += STOP_EN.contains(&w) as i32 * (1 - shared);
        la += STOP_LA.contains(&w) as i32;
    }
    if la > it_only && la > en_only {
        Lang::La
    } else if en > it {
        Lang::En
    } else {
        Lang::It
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Lowercase, drop punctuation; Italian also drops apostrophes and joins elisions.
pub fn clean(text: &str, lang: Lang) -> String {
    let t = text.to_lowercase().replace('’', "'").replace(['‿', '_'], "/");
    let t: String = t.chars().filter(|&c| is_word_char(c) || c == '\'' || c == '/').collect();
    let t = t.trim_matches('\'').to_string();
    if let Some(e) = exception(lang, &t) {
        return e.to_string();
    }
    let t = match lang {
        Lang::It => t.replace(['\'', '/'], ""),
        Lang::La => t.replace(['\'', '/'], "").chars().map(unaccent).collect(),
        Lang::En => match t.split_once('/') {
            Some((first, _)) => first.to_string(),
            None => t,
        },
    };
    if t.is_empty() { "a".into() } else { t }
}

const IT_VOW: &str = "aeiou";

fn unaccent(c: char) -> char {
    match c {
        'à' | 'á' => 'a',
        'è' | 'é' => 'e',
        'ì' | 'í' => 'i',
        'ò' | 'ó' => 'o',
        'ù' | 'ú' => 'u',
        c => c,
    }
}

fn is_vowel(c: Option<char>) -> bool {
    c.is_some_and(|c| IT_VOW.contains(c))
}

/// Rule-based Italian transcription of one sung syllable, in the voicebank's symbols
/// (y = semivowel i, w = semivowel u, j = 'gi', ch = 'ci', sh = 'sci', gn, gl, ts, rr).
pub fn it_syllable_phones(syl: &str, next_syl: &str) -> Vec<&'static str> {
    let s: Vec<char> = syl.to_lowercase().replace(['\'', '’'], "").chars().map(unaccent).collect();
    let next_first = next_syl.chars().next().map(unaccent);
    let n = s.len();
    // character at k; at the end of the syllable, the first one of the next syllable
    let nxt = |k: usize| -> Option<char> { if k < n { Some(s[k]) } else if k == n { next_first } else { None } };
    // Python `x in "aeiou"` is also true for the empty string (end of word)
    let nxt_vowel_or_end = |k: usize| -> bool { nxt(k).is_none_or(|c| IT_VOW.contains(c)) };
    let starts = |i: usize, p: &str| -> bool {
        let p: Vec<char> = p.chars().collect();
        i + p.len() <= n && s[i..i + p.len()] == p[..]
    };
    let mut out = vec![];
    let mut i = 0;
    while i < n {
        let c = s[i];
        if c == 'h' {
            i += 1;
        } else if starts(i, "gli") && nxt_vowel_or_end(i + 3) && i + 3 < n {
            out.push("gl");
            i += 3;
        } else if starts(i, "gl") && nxt(i + 2) == Some('i') {
            out.push("gl");
            i += 2;
        } else if starts(i, "gn") {
            out.push("gn");
            i += 2;
        } else if starts(i, "sc") && matches!(nxt(i + 2), Some('e' | 'i')) {
            out.push("sh");
            i += if nxt(i + 2) == Some('i') && i + 3 < n && is_vowel(Some(s[i + 3])) { 3 } else { 2 };
        } else if (c == 'c' || c == 'g') && matches!(nxt(i + 1), Some('e' | 'i')) {
            out.push(if c == 'c' { "ch" } else { "j" });
            i += if nxt(i + 1) == Some('i') && i + 2 < n && is_vowel(Some(s[i + 2])) { 2 } else { 1 };
        } else if c == 'c' || c == 'q' {
            out.push("k");
            i += 1;
        } else if c == 'r' && nxt(i + 1) == Some('r') {
            out.push("rr");
            i += 2;
        } else if c == 'z' {
            out.push("ts");
            i += 1;
        } else if (c == 'i' || c == 'u') && i + 1 < n && is_vowel(Some(s[i + 1])) {
            out.push(if c == 'i' { "y" } else { "w" }); // diphthong inside the syllable
            i += 1;
        } else if let Some(p) = phone_of(c) {
            out.push(p);
            i += 1;
        } else {
            i += 1;
        }
    }
    out
}

fn phone_of(c: char) -> Option<&'static str> {
    Some(match c {
        'a' => "a", 'e' => "e", 'i' => "i", 'o' => "o", 'u' => "u",
        'b' => "b", 'd' => "d", 'f' => "f", 'g' => "g", 'l' => "l", 'm' => "m", 'n' => "n",
        'p' => "p", 'r' => "r", 's' => "s", 't' => "t", 'v' => "v",
        _ => return None,
    })
}

/// Explicit hint for Italian words that OpenUtau's G2P gets wrong with these voicebanks:
/// semivowel i (the G2P 'j' is mapped to the voicebank's 'gi': pie-tà -> "gie-tà") and
/// double r (the G2P has no trilled rr: terra -> "tera"). Other words go to the G2P.
pub fn it_hint(syllables: &[String], word: &str) -> String {
    let mut phones: Vec<&str> = vec![];
    for (k, syl) in syllables.iter().enumerate() {
        let next = syllables.get(k + 1).map(String::as_str).unwrap_or("");
        phones.extend(it_syllable_phones(syl, next));
    }
    let phones: Vec<&str> = phones
        .iter()
        .enumerate()
        .filter(|&(k, p)| !(*p == "r" && k > 0 && phones[k - 1] == "rr"))
        .map(|(_, p)| *p)
        .collect();
    if !phones.contains(&"y") && !phones.contains(&"rr") {
        return word.to_string();
    }
    let phones: Vec<&str> = if IT_HIATUS.contains(&word) {
        phones.into_iter().map(|p| if p == "y" { "i" } else { p }).collect()
    } else {
        phones
    };
    format!("{word}[{}]", phones.join(" "))
}

/// All symbols of OpenUtau's Italian G2P (ItalianG2p.cs), plus `rr` (double trilled r) which we add.
/// Own voicebanks use exactly this set, so the G2P output needs no translation.
pub const IT_G2P_PHONES: &[&str] = &[
    "a", "e", "EE", "i", "o", "OO", "u", "b", "d", "dz", "dZZ", "f", "g", "j", "JJ", "k", "l", "LL", "m", "n",
    "nf", "ng", "p", "r", "rr", "s", "SS", "t", "ts", "tSS", "v", "w", "z",
];

/// Rule-based transcription of one Italian word in the G2P symbol set (open/closed e and o are not
/// distinguished: the score doesn't say, the G2P dictionary decides at synthesis time).
pub fn it_g2p_word(syllables: &[String]) -> Vec<&'static str> {
    let mut ph: Vec<&'static str> = vec![];
    for (k, syl) in syllables.iter().enumerate() {
        let next = syllables.get(k + 1).map(String::as_str).unwrap_or("");
        ph.extend(it_syllable_phones(syl, next).into_iter().map(|p| match p {
            "y" => "j",
            "j" => "dZZ",
            "ch" => "tSS",
            "sh" => "SS",
            "gn" => "JJ",
            "gl" => "LL",
            p => p,
        }));
    }
    it_refine(ph)
}

/// Italian assimilations on a G2p transcription: n before f/v and k/g, voiced s, single r after rr.
pub fn it_refine(ph: Vec<&'static str>) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = vec![];
    for (k, &p) in ph.iter().enumerate() {
        let next = ph.get(k + 1).copied();
        out.push(match (p, next) {
            ("r", _) if out.last() == Some(&"rr") => continue,
            ("n", Some("f" | "v")) => "nf",
            ("n", Some("k" | "g")) => "ng",
            ("s", Some("b" | "d" | "g" | "l" | "m" | "n" | "r" | "v" | "dZZ")) => "z",
            (p, _) => p,
        });
    }
    out
}

fn syllable_clean(s: &str) -> String {
    s.chars().filter(|&c| is_word_char(c) || c == '\'' || c == '’').collect()
}

/// One lyric per note. `hints`: optional word -> phonemes (e.g. from the AI step), wins over the rules.
pub fn word_lyrics(notes: &[Note], lang: Lang, hints: &HashMap<String, String>) -> Vec<String> {
    let mut lyrics = vec![String::new(); notes.len()];
    let mut i = 0;
    while i < notes.len() {
        if notes[i].lyric.is_none() {
            lyrics[i] = if i > 0 { "+~".into() } else { "a".into() };
            i += 1;
            continue;
        }
        let start = i;
        let mut word = String::new();
        let mut syls: Vec<String> = vec![];
        while i < notes.len() {
            match &notes[i].lyric {
                Some(l) => {
                    word.push_str(&l.text);
                    syls.extend(l.text.replace('‿', "/").split('/').map(syllable_clean));
                    lyrics[i] = "+".into();
                    if matches!(l.syllabic, Syllabic::Single | Syllabic::End) {
                        i += 1;
                        break;
                    }
                }
                None => lyrics[i] = "+~".into(),
            }
            i += 1;
        }
        let mut lyric = clean(&word, lang);
        let bare = lyric.split('[').next().unwrap_or("").to_string();
        if let Some(h) = hints.get(&bare) {
            lyric = format!("{bare}[{h}]");
        } else if lang == Lang::It && !lyric.contains('[') {
            let syls: Vec<String> = syls.into_iter().filter(|s| !s.is_empty()).collect();
            lyric = it_hint(&syls, &lyric);
        } else if lang == Lang::La && !lyric.contains('[') {
            let syls: Vec<String> = syls.into_iter().filter(|s| !s.is_empty()).collect();
            let ph = crate::latin::word_phones(&syls);
            if !ph.is_empty() {
                lyric = format!("{lyric}[{}]", ph.join(" "));
            }
        }
        lyrics[start] = lyric;
    }
    lyrics
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn italian_hints() {
        assert_eq!(it_hint(&s(&["pie", "tà"]), "pietà"), "pietà[p y e t a]");
        assert_eq!(it_hint(&s(&["ter", "ra"]), "terra"), "terra[t e rr a]");
        assert_eq!(it_hint(&s(&["dio"]), "dio"), "dio[d i o]");
        assert_eq!(it_hint(&s(&["glo", "ria"]), "gloria"), "gloria[g l o r y a]");
        assert_eq!(it_hint(&s(&["cie", "li"]), "cieli"), "cieli");
        assert_eq!(it_hint(&s(&["san", "to"]), "santo"), "santo");
    }

    #[test]
    fn g2p_symbols() {
        let w = |v: &[&str]| it_g2p_word(&s(v)).join(" ");
        assert_eq!(w(&["pie", "tà"]), "p j e t a");
        assert_eq!(w(&["ter", "ra"]), "t e rr a");
        assert_eq!(w(&["a", "gnel", "lo"]), "a JJ e l l o");
        assert_eq!(w(&["gi", "gli"]), "dZZ i LL i");
        assert_eq!(w(&["scen", "de"]), "SS e n d e");
        assert_eq!(w(&["in", "fer", "no"]), "i nf f e r n o");
        assert_eq!(w(&["an", "che"]), "a ng k e");
        assert_eq!(w(&["sme", "ral", "do"]), "z m e r a l d o");
        assert_eq!(w(&["quan", "do"]), "k w a n d o");
    }

    /// A part whose lyrics are written with hyphens between syllables, as in a score.
    fn sung(text: &str) -> Vec<Part> {
        use crate::score::Lyric;
        let mut notes = vec![];
        for w in text.split_whitespace() {
            let syls: Vec<&str> = w.split('-').collect();
            for (k, sy) in syls.iter().enumerate() {
                let syllabic = match (k, syls.len()) {
                    (_, 1) => Syllabic::Single,
                    (0, _) => Syllabic::Begin,
                    (k, n) if k == n - 1 => Syllabic::End,
                    _ => Syllabic::Middle,
                };
                notes.push(Note { id: String::new(), start: 0.into(), dur: 1.into(), pitch: 60, lyric: Some(Lyric { text: sy.to_string(), syllabic }) });
            }
        }
        vec![Part { name: "S".into(), notes }]
    }

    #[test]
    fn detects_latin_by_whole_words() {
        assert_eq!(detect_lang(&sung("Ky-ri-e e-le-i-son Chri-ste e-le-i-son")), Lang::La);
        assert_eq!(detect_lang(&sung("A-gnus De-i qui tol-lis pec-ca-ta mun-di mi-se-re-re no-bis")), Lang::La);
        assert_eq!(detect_lang(&sung("Glo-ri-a in ex-cel-sis De-o et in ter-ra pax ho-mi-ni-bus")), Lang::La);
        assert_eq!(detect_lang(&sung("San-ctus San-ctus Do-mi-nus De-us Sa-ba-oth")), Lang::La);
        // "in" is English and Latin: it doesn't outvote the Latin words
        let ave = "A-ve ve-rum cor-pus na-tum de Ma-ri-a vir-gi-ne ve-re pas-sum im-mo-la-tum in cru-ce pro ho-mi-ne \
                   cu-ius la-tus per-fo-ra-tum flu-xit a-qua et san-gui-ne es-to no-bis prae-gus-ta-tum in mor-tis ex-a-mi-ne";
        assert_eq!(detect_lang(&sung(ave)), Lang::La);
        // Italian liturgy: Latin-looking syllables (mi, su, di) don't count as Latin
        assert_eq!(detect_lang(&sung("A-gnel-lo di Di-o che to-gli i pec-ca-ti del mon-do ab-bi pie-tà di noi")), Lang::It);
        assert_eq!(detect_lang(&sung("Glo-ri-a a Di-o nel-l'al-to dei cie-li e pa-ce in ter-ra")), Lang::It);
        assert_eq!(detect_lang(&sung("When for-ty win-ters shall be-siege thy brow")), Lang::En);
    }

    #[test]
    fn latin_words_get_hints() {
        let p = sung("A-gnus De-i qui tol-lis");
        let l = word_lyrics(&p[0].notes, Lang::La, &HashMap::new());
        assert_eq!(l, ["agnus[a JJ u s]", "+", "dei[d e i]", "+", "qui[k w i]", "tollis[t o l l i s]", "+"]);
        assert_eq!(clean("Dómine", Lang::La), "domine");
    }

    #[test]
    fn cleaning() {
        assert_eq!(clean("dell’u", Lang::It), "dellu");
        assert_eq!(clean("gli/i", Lang::It), "glii");
        assert_eq!(clean("beauty's", Lang::En), "beauty's");
        assert_eq!(clean("\"This", Lang::En), "this");
        assert_eq!(clean("feel'st", Lang::En), "feel'st[f iy l s t]");
    }
}
