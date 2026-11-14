//! Recording corpus for building a DiffSinger voicebank: short phrases (5–12 s) whose notes and words are
//! known in advance, so the recordings only need automatic alignment, not hand labelling.
//!
//! Three sources:
//! - exercises: Italian lines covering every phoneme, diphthong and double consonant, on simple melodies,
//!   in three registers of the singer's range;
//! - vocalises: one vowel on a five-note scale, stepping through the whole range (pitch coverage);
//! - pieces: the choir's own scores, the singer's part cut at rests.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::lyrics::it_g2p_word;
use crate::score::{read_mscz, Beats, Syllabic};
use crate::voice::slug;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CNote {
    /// MIDI pitch.
    pub pitch: i32,
    /// Duration in quarter notes.
    pub beats: f64,
    /// Syllable sung on this note; `None` = the previous vowel goes on (melisma).
    pub syl: Option<String>,
    /// Last syllable of a word.
    #[serde(default)]
    pub word_end: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Phrase {
    pub id: String,
    pub group: String,
    pub text: String,
    /// Quarter notes per minute.
    pub bpm: f64,
    pub notes: Vec<CNote>,
    /// Expected phonemes (G2P symbols), for coverage checks.
    pub phonemes: Vec<String>,
    pub seconds: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceProfile {
    pub name: String,
    /// soprano / contralto / tenore / baritono / basso
    pub voice: String,
    /// Comfortable range, MIDI.
    pub low: i32,
    pub high: i32,
}

pub const GROUP_MID: &str = "Esercizi · registro medio";
pub const GROUP_VOC: &str = "Vocalizzi";
pub const GROUP_LOW: &str = "Esercizi · registro grave";
pub const GROUP_HIGH: &str = "Esercizi · registro acuto";

/// Italian lines, syllables split by '-'. Together they cover all the G2P phonemes, the common
/// diphthongs, the double consonants and the clusters (str, pr, fl, sb, sm…). No apostrophes on purpose.
pub const LINES: &[&str] = &[
    "Can-to la ter-ra e il cie-lo",
    "Pie-tà di noi, Si-gno-re",
    "Nel-la not-te scu-ra e fred-da",
    "Il so-le bril-la sul ma-re",
    "Gio-ia e pa-ce in o-gni cuo-re",
    "Lu-ce dol-ce del mat-ti-no",
    "Fug-ge il ven-to tra gli al-be-ri",
    "Scen-de la ne-ve sui cam-pi",
    "Bel-la ro-sa del giar-di-no",
    "Quan-do tor-na la pri-ma-ve-ra",
    "Ra-gaz-zi gio-ca-no in piaz-za",
    "Zuc-che-ro dol-ce nel caf-fè",
    "Ma-gni-fi-ca è la mon-ta-gna",
    "Fi-glio mi-o, a-scol-ta be-ne",
    "Pro-fu-mo di pa-ne cal-do",
    "Guer-ra e pa-ce nel mon-do",
    "Stel-la che bril-li las-sù",
    "Spe-ran-za ve-ra nel cuo-re",
    "Glo-ria a Di-o nel-le al-tez-ze",
    "Ac-qua fre-sca del-la fon-te",
    "Ab-bi pie-tà di me",
    "Leg-ge-ra vo-la la piu-ma",
    "Ver-de pra-to, cie-lo az-zur-ro",
    "Bab-bo por-ta il pa-ne a ca-sa",
    "In-fer-no e pa-ra-di-so",
    "An-che og-gi can-to con te",
    "Lun-go il fiu-me gran-de e len-to",
    "Con-for-to dol-ce e sin-ce-ro",
    "Sbat-te la por-ta di ca-sa",
    "Sme-ral-do ver-de e lu-cen-te",
    "Ro-se ros-se e gi-gli bian-chi",
    "Il tre-no cor-re ve-lo-ce",
    "Strin-gi for-te la mia ma-no",
    "Pre-ghie-ra u-mi-le e san-ta",
    "O-san-na nel più al-to dei cie-li",
    "A-gnel-lo di Di-o",
    "Che to-gli i pec-ca-ti del mon-do",
    "Do-na a noi la pa-ce",
    "Mez-zo-gior-no sul-la piaz-za",
    "Piog-gia fi-ne sui tet-ti",
    "Uo-mi-ni e don-ne in cam-mi-no",
    "Au-ro-ra e tra-mon-to",
    "Sei tu la mi-a vi-ta",
    "Noi can-tia-mo in-sie-me",
    "Fiam-ma vi-va nel ca-mi-no",
    "Flau-to e vio-li-no in co-ro",
    "Bru-cia la le-gna nel fuo-co",
    "Gri-da di gio-ia nel-la val-le",
    "Cre-do in u-na ve-ri-tà",
    "Tut-to pas-sa, re-sta a-mo-re",
    "Bat-ti il tem-po con le ma-ni",
    "Vec-chio ca-stel-lo sul col-le",
    "Si scio-glie la ne-ve sul mon-te",
    "Ghir-lan-de di fio-ri ros-si",
    "Chi-tar-ra e tam-bu-ro",
    "Cor-re il car-ro sul-la stra-da",
    "Ar-ri-va il tre-no in ri-tar-do",
    "Sot-to il pon-te scor-re il fiu-me",
    "Puoi ve-de-re le stel-le",
    "Ec-co il gior-no del Si-gno-re",
    "Pas-sa-no le nu-vo-le gri-gie",
    "Sal-ve Re-gi-na, ma-dre",
    "Fra-tel-lo so-le, so-rel-la lu-na",
    "Bel-lez-za an-ti-ca e nuo-va",
    "Zio Pao-lo leg-ge il gior-na-le",
    "Mam-ma pre-pa-ra la ce-na",
    "Il gat-to dor-me sul let-to",
    "Ven-go do-ma-ni a tro-var-ti",
    "Si-gno-re pie-tà, Cri-sto pie-tà",
    "Be-ne-det-to co-lui che vie-ne",
    "Pa-dre no-stro che sei nei cie-li",
    "San-ti-fi-ca-to sia il tuo no-me",
    "Ven-ga il tuo re-gno di pa-ce",
    "Da-cci og-gi il no-stro pa-ne",
    "Al-le-lu-ia, can-ta-te al Si-gno-re",
    "Ri-ma-ni con noi, Si-gno-re",
    "Ac-co-gli la no-stra pre-ghie-ra",
    "Lo-da-te Di-o con can-ti di fe-sta",
    "Ge-sù, pa-sto-re buo-no",
    "Ma-ri-a, ma-dre di gra-zia",
    "Spi-ri-to San-to, scen-di su noi",
    "Glo-ria al Pa-dre e al Fi-glio",
    "La tua pa-ro-la è lam-pa-da",
    "Nel-la tua gra-zia tro-vo la gio-ia",
    "Il pa-ne del cie-lo per noi",
    "Chie-sa di Di-o, po-po-lo san-to",
    "Ec-co lo spo-so che vie-ne",
    "Tut-ta la ter-ra can-ti a Di-o",
    "Ser-vi-re il fra-tel-lo con a-mo-re",
    "Sor-gi, ri-splen-di, Ge-ru-sa-lem-me",
    "Scen-da la ru-gia-da dal cie-lo",
    "Il mio cuo-re e-sul-ta nel Si-gno-re",
    "Bea-to chi a-scol-ta la pa-ro-la",
    "Ac-cen-di la lam-pa-da del-la fe-de",
    "O-gni crea-tu-ra lo-di il Si-gno-re",
    "Fuo-co, ven-to, ac-qua e ter-ra",
    "Gior-no di fe-sta, gior-no di lu-ce",
    "Sciol-to dal-le ca-te-ne",
    "Nel bu-io del-la not-te ve-glia-te",
    "Gui-da i miei pas-si",
    "Zam-pil-la la fon-te nel bo-sco",
    "Rag-gi di so-le sui gi-ra-so-li",
];

const MAJOR: [i32; 10] = [0, 2, 4, 5, 7, 9, 11, 12, 14, 16];
const TEMPOS: [f64; 4] = [72.0, 84.0, 66.0, 96.0];

/// Scale degrees (indices into MAJOR) for `n` notes, five melodic shapes.
fn melody(shape: usize, n: usize) -> Vec<usize> {
    let wave = |seq: &[usize]| (0..n).map(|i| seq[i % seq.len()]).collect::<Vec<_>>();
    let mut d = match shape % 5 {
        // up then down
        0 => {
            let top = (n / 2).min(5);
            (0..n).map(|i| if i <= top { i } else { top.saturating_sub(i - top) }).collect()
        }
        // arpeggio
        1 => wave(&[0, 2, 4, 7, 4, 2]),
        // descending steps from the fifth
        2 => (0..n).map(|i| 4 - (i % 5)).collect(),
        // reciting tone, cadence at the end
        3 => (0..n).map(|i| if i + 3 >= n { [3, 1, 0][i + 3 - n] } else { 2 }).collect(),
        // leap up, steps down
        _ => wave(&[0, 4, 3, 2, 1]),
    };
    // always land on the tonic
    if let Some(last) = d.last_mut() {
        *last = 0;
    }
    d
}

/// Syllables of a line: (syllable, word_end).
fn line_syllables(line: &str) -> Vec<(String, bool)> {
    let mut out = vec![];
    for word in line.split_whitespace() {
        let syls: Vec<&str> = word.split('-').collect();
        for (k, s) in syls.iter().enumerate() {
            out.push((s.to_string(), k + 1 == syls.len()));
        }
    }
    out
}

fn word_clean(s: &str) -> String {
    s.chars().filter(|c| c.is_alphabetic() || *c == '\'').collect::<String>().to_lowercase()
}

/// Expected phonemes of a sequence of notes, word by word.
fn phonemes_of(notes: &[CNote]) -> Vec<String> {
    let mut out = vec![];
    let mut word: Vec<String> = vec![];
    for n in notes {
        if let Some(s) = &n.syl {
            let s = word_clean(s);
            if !s.is_empty() {
                word.push(s);
            }
            if n.word_end && !word.is_empty() {
                out.extend(it_g2p_word(&word).into_iter().map(str::to_string));
                word.clear();
            }
        }
    }
    if !word.is_empty() {
        out.extend(it_g2p_word(&word).into_iter().map(str::to_string));
    }
    out
}

/// Display text: "San-to, San-to".
fn text_of(notes: &[CNote]) -> String {
    let mut t = String::new();
    for n in notes {
        if let Some(s) = &n.syl {
            t.push_str(s.trim());
            t.push_str(if n.word_end { " " } else { "-" });
        }
    }
    t.trim_end_matches([' ', '-']).to_string()
}

fn phrase(id: String, group: &str, bpm: f64, notes: Vec<CNote>) -> Phrase {
    let beats: f64 = notes.iter().map(|n| n.beats).sum();
    Phrase {
        id,
        group: group.into(),
        text: text_of(&notes),
        bpm,
        phonemes: phonemes_of(&notes),
        seconds: (beats * 60.0 / bpm * 10.0).round() / 10.0,
        notes,
    }
}

/// Base pitch so that a melody spanning `span` semitones sits at `frac` (0 = bottom, 1 = top) of the range.
fn base_pitch(p: &VoiceProfile, span: i32, frac: f64) -> i32 {
    let room = (p.high - p.low - span).max(0);
    p.low + (room as f64 * frac).round() as i32
}

fn exercise(p: &VoiceProfile, idx: usize, line: &str, band: (&str, &str, f64)) -> Phrase {
    let syls = line_syllables(line);
    let n = syls.len();
    let degrees = melody(idx, n);
    let rel: Vec<i32> = degrees.iter().map(|&d| MAJOR[d]).collect();
    let span = rel.iter().max().unwrap() - rel.iter().min().unwrap();
    let base = base_pitch(p, span, band.2);
    let fast = n >= 10;
    let notes = syls
        .into_iter()
        .zip(rel)
        .enumerate()
        .map(|(i, ((s, word_end), r))| CNote {
            pitch: base + r,
            beats: if i + 1 == n {
                2.0
            } else if fast {
                0.5
            } else {
                match idx % 3 {
                    // long notes at word ends, like sustained choral writing
                    1 if word_end => 2.0,
                    // dotted pairs
                    2 => if i % 2 == 0 { 1.5 } else { 0.5 },
                    _ => 1.0,
                }
            },
            syl: Some(s),
            word_end,
        })
        .collect();
    phrase(format!("l{:02}-{}", idx + 1, band.0), band.1, TEMPOS[idx % TEMPOS.len()], notes)
}

const VOC_SYL: [&str; 5] = ["la", "ne", "mi", "do", "tu"];

/// One syllable on 1-2-3-4-5-4-3-2-1, starting every two semitones through the range.
fn vocalises(p: &VoiceProfile) -> Vec<Phrase> {
    let scale = [0, 2, 4, 5, 7, 5, 4, 2, 0];
    let mut out = vec![];
    let mut k = 0;
    let mut start = p.low;
    while start + 7 <= p.high {
        let syl = VOC_SYL[k % VOC_SYL.len()];
        let notes = scale
            .iter()
            .enumerate()
            .map(|(i, r)| CNote {
                pitch: start + r,
                beats: if i + 1 == scale.len() { 1.5 } else { 0.5 },
                syl: (i == 0).then(|| syl.to_string()),
                word_end: i == 0,
            })
            .collect();
        out.push(phrase(format!("v{:02}-{syl}", k + 1), GROUP_VOC, 80.0, notes));
        start += 2;
        k += 1;
    }
    out
}

/// Exercises and vocalises for a singer, in recording order: middle register and vocalises first (together
/// they are enough for a pilot), then the low and high registers. Pieces are inserted before the low register.
pub fn base_corpus(p: &VoiceProfile) -> Vec<Phrase> {
    let bands = [("m", GROUP_MID, 0.5), ("g", GROUP_LOW, 0.1), ("a", GROUP_HIGH, 0.9)];
    let mut out = vec![];
    for (b, band) in bands.iter().enumerate() {
        out.extend(LINES.iter().enumerate().map(|(i, line)| exercise(p, i, line, *band)));
        if b == 0 {
            out.extend(vocalises(p));
        }
    }
    out
}

fn to_f(b: Beats) -> f64 {
    *b.numer() as f64 / *b.denom() as f64
}

/// Phrases of one sung part of a score: cut at rests (≥ an eighth) or when longer than 12 s; transposed by
/// octaves into the singer's range; repeated passages with the same words only once.
pub fn piece_phrases(path: &Path, part_name: &str, p: &VoiceProfile) -> Result<(String, Vec<Phrase>)> {
    let score = read_mscz(path)?;
    let title = score
        .title
        .clone()
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
    let part = score
        .parts
        .iter()
        .find(|x| x.name == part_name)
        .ok_or_else(|| Error::Other(format!("voce «{part_name}» non trovata")))?;
    if !part.is_sung() {
        return Err(Error::Other(format!("la voce «{part_name}» non ha testo")));
    }
    // octave shift that keeps most notes inside the range
    let shift = (-3..=3)
        .map(|k| k * 12)
        .min_by_key(|s| {
            let out = part.notes.iter().filter(|n| n.pitch + s < p.low || n.pitch + s > p.high).count();
            (out, s.abs())
        })
        .unwrap_or(0);
    let bpm_at = |b: Beats| {
        score.tempos.iter().take_while(|(pos, _)| *pos <= b).last().map(|(_, q)| q * 60.0).unwrap_or(score.tempos[0].1 * 60.0)
    };

    let mut groups: Vec<Vec<&crate::score::Note>> = vec![];
    let mut cur: Vec<&crate::score::Note> = vec![];
    for n in &part.notes {
        if let Some(prev) = cur.last() {
            let gap = to_f(n.start - (prev.start + prev.dur));
            let len = to_f(prev.start + prev.dur - cur[0].start) * 60.0 / bpm_at(cur[0].start);
            let at_word_end = prev.lyric.as_ref().is_none_or(|l| matches!(l.syllabic, Syllabic::Single | Syllabic::End));
            if gap >= 0.5 || (len > 12.0 && at_word_end && n.lyric.is_some()) {
                groups.push(std::mem::take(&mut cur));
            }
        }
        cur.push(n);
    }
    groups.push(cur);

    let base = slug(&title);
    let mut seen = HashSet::new();
    let mut out = vec![];
    for g in groups {
        if g.len() < 3 || g.iter().all(|n| n.lyric.is_none()) {
            continue;
        }
        let bpm = bpm_at(g[0].start);
        let notes: Vec<CNote> = g
            .iter()
            .map(|n| CNote {
                pitch: n.pitch + shift,
                beats: to_f(n.dur),
                syl: n.lyric.as_ref().map(|l| l.text.replace(['‿', '_'], " ")),
                word_end: n.lyric.as_ref().is_some_and(|l| matches!(l.syllabic, Syllabic::Single | Syllabic::End)),
            })
            .collect();
        let key = serde_json::to_string(&notes).unwrap_or_default();
        if !seen.insert(key) {
            continue;
        }
        let id = format!("b-{base}-{}-{:02}", slug(part_name), out.len() + 1);
        out.push(phrase(id, &format!("Brano · {title}"), (bpm * 10.0).round() / 10.0, notes));
    }
    Ok((title, out))
}

/// Phoneme -> occurrences, and pitches not sung in any phrase.
pub struct Coverage {
    pub phonemes: BTreeMap<String, usize>,
    pub missing_pitches: Vec<i32>,
}

pub fn coverage(phrases: &[Phrase], p: &VoiceProfile) -> Coverage {
    let mut phonemes = BTreeMap::new();
    let mut pitches = HashSet::new();
    for ph in phrases {
        for x in &ph.phonemes {
            *phonemes.entry(x.clone()).or_insert(0) += 1;
        }
        pitches.extend(ph.notes.iter().map(|n| n.pitch));
    }
    Coverage { phonemes, missing_pitches: (p.low..=p.high).filter(|x| !pitches.contains(x)).collect() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyrics::IT_G2P_PHONES;

    fn soprano() -> VoiceProfile {
        VoiceProfile { name: "Prova".into(), voice: "soprano".into(), low: 60, high: 79 }
    }

    #[test]
    fn covers_phonemes_and_range() {
        let p = soprano();
        let c = base_corpus(&p);
        let cov = coverage(&c, &p);
        // EE / OO (open e, o) and dz (voiced z: mezzo, azzurro) come from the G2P dictionary, not the spelling
        for ph in IT_G2P_PHONES.iter().filter(|x| !["EE", "OO", "dz"].contains(x)) {
            let n = cov.phonemes.get(*ph).copied().unwrap_or(0);
            assert!(n >= 6, "phoneme {ph}: {n} occurrences (×3 registers)");
        }
        assert!(cov.missing_pitches.is_empty(), "pitches never sung: {:?}", cov.missing_pitches);
        let ids: HashSet<_> = c.iter().map(|x| &x.id).collect();
        assert_eq!(ids.len(), c.len(), "unique ids");
        assert!(c.iter().all(|x| x.notes.iter().all(|n| n.pitch >= p.low && n.pitch <= p.high)));
        let minutes: f64 = c.iter().map(|x| x.seconds).sum::<f64>() / 60.0;
        assert!((20.0..45.0).contains(&minutes), "{minutes} minutes");
        assert!(c.iter().all(|x| (3.0..14.0).contains(&x.seconds)), "phrase lengths");
    }

    #[test]
    fn text_and_phonemes() {
        let c = base_corpus(&soprano());
        let first = &c[0];
        assert_eq!(first.text, "Can-to la ter-ra e il cie-lo");
        assert_eq!(first.phonemes.join(" "), "k a n t o l a t e rr a e i l tSS e l o");
        assert_eq!(first.group, GROUP_MID);
    }

    #[test]
    fn deterministic() {
        assert_eq!(base_corpus(&soprano()), base_corpus(&soprano()));
    }
}
