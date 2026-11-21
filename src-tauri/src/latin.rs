//! Ecclesiastical (Italian-style) Latin pronunciation, written with the symbols of OpenUtau's
//! Italian G2P (see `lyrics::IT_G2P_PHONES`), so Latin is sung by any Italian voicebank.
//!
//! The input is a word split into the syllables of the score. Each score syllable keeps exactly
//! one vowel: the phonemizer gives one vowel to each note, so an extra or missing vowel would
//! shift the rest of the word onto the wrong notes. Two vowels in one syllable become a vowel and
//! a glide (`lau` → l a w, `cui` → k u j, `ia` → j a).

use crate::lyrics::it_refine;

/// Words whose `h` is sung as `k` in the traditional pronunciation.
const H_AS_K: &[&str] = &["mihi", "nihil"];

fn unaccent(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ā' | 'ă' => 'a',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'ī' | 'ĭ' => 'i',
        'ò' | 'ó' | 'ô' | 'ō' | 'ŏ' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ŭ' => 'u',
        'ý' | 'ÿ' | 'y' => 'i',
        c => c,
    }
}

/// One syllable as plain lowercase letters (ae/oe ligatures spelled out, accents removed).
fn letters(syl: &str) -> String {
    let mut out = String::new();
    for c in syl.to_lowercase().chars() {
        match c {
            'æ' => out.push_str("ae"),
            'œ' => out.push_str("oe"),
            c => {
                let c = unaccent(c);
                if c.is_ascii_lowercase() {
                    out.push(c);
                }
            }
        }
    }
    out
}

fn is_vowel(c: char) -> bool {
    matches!(c, 'a' | 'e' | 'i' | 'o' | 'u')
}

/// A sung sound with the syllable it belongs to.
struct Ph {
    sym: &'static str,
    syl: usize,
}

fn is_vowel_sym(s: &str) -> bool {
    matches!(s, "a" | "e" | "i" | "o" | "u")
}

/// Ecclesiastical Latin transcription of a word from its score syllables.
pub fn word_phones(syllables: &[String]) -> Vec<&'static str> {
    let mut syls: Vec<String> = syllables.iter().map(|s| letters(s)).collect();
    let word: String = syls.concat();
    if H_AS_K.contains(&word.as_str()) {
        for s in &mut syls {
            *s = s.replace('h', "k");
        }
    }
    // the letters of the whole word, each with its syllable
    let mut ch: Vec<char> = vec![];
    let mut sy: Vec<usize> = vec![];
    for (k, s) in syls.iter().enumerate() {
        for c in s.chars() {
            ch.push(c);
            sy.push(k);
        }
    }
    let n = ch.len();
    let at = |i: usize| -> Option<char> { ch.get(i).copied() };
    // e, i, or ae/oe follow: c and g are soft
    let soft = |i: usize| -> bool {
        match at(i) {
            Some('e' | 'i') => true,
            Some('a' | 'o') => at(i + 1) == Some('e') && sy.get(i + 1) == sy.get(i),
            _ => false,
        }
    };
    let vowel_at = |i: usize| at(i).is_some_and(is_vowel);

    let mut out: Vec<Ph> = vec![];
    let mut i = 0;
    while i < n {
        let c = ch[i];
        let s = sy[i];
        let mut push = |sym: &'static str| out.push(Ph { sym, syl: s });
        let prev = if i > 0 { Some(ch[i - 1]) } else { None };
        match c {
            // ae / oe inside one syllable: e
            'a' | 'o' if at(i + 1) == Some('e') && sy[i + 1] == s => {
                push("e");
                i += 2;
                continue;
            }
            'a' | 'e' | 'i' | 'o' => push(match c {
                'a' => "a",
                'e' => "e",
                'i' => "i",
                _ => "o",
            }),
            'u' => {
                // qu, and ngu before a vowel: the u is a glide
                let glide = prev == Some('q') || (prev == Some('g') && i >= 2 && ch[i - 2] == 'n' && vowel_at(i + 1));
                push(if glide { "w" } else { "u" });
            }
            'j' => push("j"),
            'h' => {} // silent
            'c' => {
                if at(i + 1) == Some('h') {
                    push("k");
                    i += 2;
                    continue;
                } else if at(i + 1) == Some('c') && soft(i + 2) {
                    push("t"); // ecce: t tSS
                } else if soft(i + 1) {
                    push(if matches!(prev, Some('s' | 'x')) { "SS" } else { "tSS" });
                } else {
                    push("k");
                }
            }
            's' => {
                if at(i + 1) == Some('c') && soft(i + 2) {
                    push("SS"); // descendit: sc before e/i is one sound
                    i += 2;
                    continue;
                }
                push("s");
            }
            'g' => {
                if at(i + 1) == Some('n') {
                    push("JJ");
                    i += 2;
                    continue;
                } else if at(i + 1) == Some('g') && soft(i + 2) {
                    push("d");
                } else {
                    push(if soft(i + 1) { "dZZ" } else { "g" });
                }
            }
            'q' | 'k' => push("k"),
            'p' if at(i + 1) == Some('h') => {
                push("f");
                i += 2;
                continue;
            }
            't' => {
                if at(i + 1) == Some('h') {
                    push("t");
                    i += 2;
                    continue;
                }
                // ti before a vowel (not after s, t, x): tsi
                let tsi = at(i + 1) == Some('i') && vowel_at(i + 2) && !matches!(prev, Some('s' | 't' | 'x'));
                push(if tsi { "ts" } else { "t" });
            }
            'x' => {
                if i == 1 && ch[0] == 'e' && (vowel_at(2) || (at(2) == Some('h') && vowel_at(3))) {
                    push("g"); // exaudi: egz-
                    push("z");
                } else if at(i + 1) == Some('c') && soft(i + 2) {
                    push("k"); // excelsis: the c gives SS
                } else {
                    push("k");
                    push("s");
                }
            }
            'z' => push("dz"),
            'b' => push("b"),
            'd' => push("d"),
            'f' => push("f"),
            'l' => push("l"),
            'm' => push("m"),
            'n' => push("n"),
            'p' => push("p"),
            'r' if at(i + 1) == Some('r') => {
                push("rr"); // double r: trilled, as in Italian
                i += 2;
                continue;
            }
            'r' => push("r"),
            'v' | 'w' => push("v"),
            _ => {}
        }
        i += 1;
    }

    // one vowel per syllable
    for k in 0..syls.len() {
        let idx: Vec<usize> = (0..out.len()).filter(|&j| out[j].syl == k).collect();
        // i at the start of a syllable, before a vowel: a glide (Ie-sus, e-ius, al-le-lu-ia)
        if idx.len() >= 2 && out[idx[0]].sym == "i" && is_vowel_sym(out[idx[1]].sym) {
            out[idx[0]].sym = "j";
        }
        loop {
            let v: Vec<usize> = idx.iter().copied().filter(|&j| is_vowel_sym(out[j].sym)).collect();
            if v.len() < 2 {
                break;
            }
            let glide = |s: &str| match s {
                "i" => Some("j"),
                "u" => Some("w"),
                _ => None,
            };
            // falling (au, eu, ui) or rising (ia, ie, uo): the i/u becomes the glide
            if let Some(g) = glide(out[v[1]].sym) {
                out[v[1]].sym = g;
            } else if let Some(g) = glide(out[v[0]].sym) {
                out[v[0]].sym = g;
            } else {
                break; // two full vowels in one score syllable: sung as written
            }
        }
    }
    it_refine(out.into_iter().map(|p| p.sym).collect())
}

/// Whether every score syllable got exactly one vowel.
pub fn one_vowel_per_syllable(syllables: &[String], phones: &[&str]) -> bool {
    let syllables = syllables.iter().filter(|s| !letters(s).is_empty()).count();
    phones.iter().filter(|p| is_vowel_sym(p)).count() == syllables
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(word: &str) -> String {
        let syls: Vec<String> = word.split('-').map(str::to_string).collect();
        word_phones(&syls).join(" ")
    }

    #[test]
    fn ecclesiastical_rules() {
        let cases = [
            ("glo-ri-a", "g l o r i a"),
            ("ex-cel-sis", "e k SS e l s i s"),
            ("a-gnus", "a JJ u s"),
            ("De-i", "d e i"),
            ("gra-ti-a", "g r a ts i a"),
            ("e-ti-am", "e ts i a m"),
            ("hos-ti-a", "o s t i a"),
            ("Ie-su", "j e s u"),
            ("Je-su", "j e s u"),
            ("san-gui-nis", "s a ng g w i n i s"),
            ("qui", "k w i"),
            ("mi-hi", "m i k i"),
            ("ni-hil", "n i k i l"),
            ("lau-da-mus", "l a w d a m u s"),
            ("cae-li", "tSS e l i"),
            ("coe-li", "tSS e l i"),
            ("cæ-li", "tSS e l i"),
            ("ec-ce", "e t tSS e"),
            ("de-scen-dit", "d e SS e n d i t"),
            ("sus-ci-pe", "s u SS i p e"),
            ("ex-au-di", "e g z a w d i"),
            ("e-le-i-son", "e l e i s o n"),
            ("Ky-ri-e", "k i r i e"),
            ("Chri-ste", "k r i s t e"),
            ("pa-cem", "p a tSS e m"),
            ("ho-san-na", "o s a n n a"),
            ("Sa-ba-oth", "s a b a o t"),
            ("re-gi-na", "r e dZZ i n a"),
            ("cu-i", "k u i"),
            ("cui", "k u j"),
            ("huic", "u j k"),
            ("al-le-lu-ia", "a l l e l u j a"),
            ("e-ius", "e j u s"),
            ("cu-ius", "k u j u s"),
            ("Is-ra-el", "i z r a e l"),
            ("Mi-cha-el", "m i k a e l"),
            ("e-quus", "e k w u s"),
            ("fi-li-i", "f i l i i"),
            ("ter-ra", "t e rr a"),
            ("in-fer-ni", "i nf f e r n i"),
            ("tu-am", "t u a m"),
            ("Dó-mi-ne", "d o m i n e"),
            ("Ze-cha-ri-as", "dz e k a r i a s"),
        ];
        for (word, expected) in cases {
            assert_eq!(w(word), expected, "{word}");
        }
    }

    #[test]
    fn every_syllable_has_one_vowel() {
        let words = [
            "glo-ri-a", "ex-cel-sis", "a-gnus", "De-i", "gra-ti-a", "Ie-su", "san-gui-nis", "qui", "lau-da-mus",
            "cae-li", "ec-ce", "ex-au-di", "e-le-i-son", "Ky-ri-e", "cui", "huic", "al-le-lu-ia", "e-ius", "Is-ra-el",
            "e-quus", "fi-li-i", "quo-ni-am", "tol-lis", "pec-ca-ta", "mun-di", "mi-se-re-re", "no-bis", "Pa-tris",
            "Spi-ri-tu", "San-cto", "be-ne-dic-tus", "ve-nit", "no-mi-ne", "Do-mi-ni", "a-do-ro", "de-vo-te",
            "la-tens", "De-i-tas", "quae", "Ma-ri-a", "Ae-ter-ne", "lin-gua", "ius-ti-ti-a", "in-ae-sti-ma-bi-lis",
        ];
        for word in words {
            let syls: Vec<String> = word.split('-').map(str::to_string).collect();
            let ph = word_phones(&syls);
            assert!(one_vowel_per_syllable(&syls, &ph), "{word}: {}", ph.join(" "));
        }
    }
}
