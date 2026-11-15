//! Lyrics → timed phonemes with the voicebank's duration model. Port of OpenUtau's
//! MachineLearningPhonemizer, DiffSingerBasePhonemizer, DiffSingerG2pPhonemizer and the
//! Italian / English+ (ARPA+) DiffSinger phonemizers.

use std::collections::HashMap;
use std::sync::Arc;

use super::config::phoneme_language;
use super::g2p::{self, Dict, DictData, Fallbacks, G2p, Remapper};
use super::onnx::{floats, Input};
use super::time::TimeAxis;
use super::voicebank::{SubModel, Voicebank};
use crate::error::{Error, Result};
use crate::lyrics::Lang;

/// A note as the phonemizer sees it: absolute ticks, lyric with an optional `[hint]`.
#[derive(Debug, Clone)]
pub struct PNote {
    pub position: i64,
    pub duration: i64,
    pub tone: i32,
    pub lyric: String,
}

impl PNote {
    pub fn end(&self) -> i64 {
        self.position + self.duration
    }
}

/// A phoneme placed on the timeline (absolute ticks). `group` is the index of the lyric group
/// (a note plus its `+` continuation notes) it belongs to.
#[derive(Debug, Clone, PartialEq)]
pub struct Phone {
    pub symbol: String,
    pub position: i64,
    pub end: i64,
    pub group: usize,
}

/// Phonemes of each lyric group of a phrase with tick offsets from the group's first note
/// (None = lyric not recognised).
type PhraseResult = Vec<Option<Vec<(String, i64)>>>;

pub struct Phonemized {
    pub phones: Vec<Phone>,
    /// Note indices of each lyric group.
    pub groups: Vec<Vec<usize>>,
    pub warnings: Vec<String>,
}

const BASE_VOWELS_IT: &[&str] = &["a", "e", "EE", "i", "o", "OO", "u"];
const BASE_CONSONANTS_IT: &[&str] = &[
    "b", "d", "dz", "dZZ", "f", "g", "j", "JJ", "k", "l", "LL", "m", "n", "nf", "ng", "p", "r", "s", "SS", "t", "ts", "tSS",
    "v", "w", "z",
];
const BASE_VOWELS_EN: &[&str] = &["aa", "ae", "ah", "ao", "aw", "ax", "ay", "eh", "er", "ey", "ih", "iy", "ow", "oy", "uh", "uw"];
const BASE_CONSONANTS_EN: &[&str] = &[
    "b", "ch", "d", "dh", "dr", "dx", "f", "g", "hh", "jh", "k", "l", "m", "n", "ng", "p", "q", "r", "s", "sh", "t", "th",
    "tr", "v", "w", "y", "z", "zh",
];

/// The language-specific phonemizer: G2p chain plus the symbols it knows (`HasPhoneme`).
pub struct LangG2p {
    pub lang: Lang,
    pub g2p: Fallbacks,
    symbols: HashMap<String, bool>,
    /// G2p symbol → voicebank symbol, also applied to the symbols of a phonetic hint.
    replacements: HashMap<String, String>,
}

/// Italian G2p symbols that voicebanks made for other languages often lack, with a close
/// substitute tried when the dsdict has no replacement for them (not in OpenUtau).
const FALLBACK_IT: &[(&str, &str)] = &[("JJ", "ny"), ("j", "y"), ("LL", "gl"), ("dz", "z"), ("ts", "s")];

impl LangG2p {
    fn code(&self) -> &'static str {
        self.lang.code()
    }

    fn has_phoneme(&self, s: &str) -> bool {
        self.symbols.contains_key(s)
    }

    /// DiffSingerG2pPhonemizer.LoadG2p for the duration model folder.
    pub fn load(dur: &SubModel, lang: Lang) -> Result<LangG2p> {
        let dict_name = format!("dsdict-{}.yaml", lang.code());
        let path = [dict_name.as_str(), "dsdict.yaml"]
            .iter()
            .map(|n| dur.root.join(n))
            .find(|p| p.is_file())
            .ok_or_else(|| Error::Other(format!("no {dict_name} or dsdict.yaml in {}", dur.root.display())))?;
        let text = std::fs::read_to_string(&path)?;
        let data: DictData = serde_yaml::from_str(&text).map_err(|e| Error::Format(format!("{}: {e}", path.display())))?;
        let mut dict = Dict::default();
        dict.load(&data);
        let mut replacements: HashMap<String, String> =
            data.replacements.iter().flatten().map(|r| (r.from.clone(), r.to.clone())).collect();
        let mut symbols: HashMap<String, bool> = HashMap::new();
        for s in data.symbols.iter().flatten() {
            symbols.insert(s.symbol.trim().to_string(), true);
        }
        dict.add_symbol_vowel("SP", true);
        dict.add_symbol_vowel("AP", true);

        let (base, vowels, consonants): (Arc<dyn G2p>, &[&str], &[&str]) = match lang {
            Lang::It => (g2p::italian()?, BASE_VOWELS_IT, BASE_CONSONANTS_IT),
            Lang::En => (g2p::arpabet_plus()?, BASE_VOWELS_EN, BASE_CONSONANTS_EN),
        };
        for v in vowels {
            symbols.insert(v.to_string(), true);
        }
        for c in consonants {
            symbols.insert(c.to_string(), false);
        }
        let code = lang.code();
        let use_lang_id = dur.config.use_lang_id;
        for ph in vowels.iter().chain(consonants) {
            if replacements.contains_key(*ph) {
                continue;
            }
            let prefixed = format!("{code}/{ph}");
            let fallback = match lang {
                Lang::It => FALLBACK_IT.iter().find(|(f, _)| f == ph).map(|(_, t)| t.to_string()),
                Lang::En => None,
            }
            .filter(|t| dur.tokens.contains_key(t));
            let to = if dur.tokens.contains_key(&prefixed) {
                prefixed
            } else if dur.tokens.contains_key(*ph) {
                ph.to_string()
            } else if let Some(t) = fallback {
                t
            } else if !use_lang_id {
                ph.to_string()
            } else {
                prefixed
            };
            replacements.insert(ph.to_string(), to);
        }
        for (from, to) in &replacements {
            if base.is_valid_symbol(to) {
                symbols.insert(from.clone(), base.is_vowel(to));
            }
        }
        let remapper = Remapper { mapped: base, symbols: symbols.clone(), replacements: replacements.clone() };
        Ok(LangG2p { lang, g2p: Fallbacks(vec![Arc::new(dict), Arc::new(remapper)]), symbols, replacements })
    }
}

/// `word[ph ph]` → (`word`, Some(`ph ph`)), as UNote.ToPhonemizerNote (`\[(.*)\]`).
pub fn split_hint(lyric: &str) -> (String, Option<String>) {
    if let (Some(a), Some(b)) = (lyric.find('['), lyric.rfind(']'))
        && a < b
    {
        let hint = lyric[a + 1..b].trim().to_string();
        let rest = format!("{}{}", &lyric[..a], &lyric[b + 1..]);
        return (rest.trim().to_string(), Some(hint));
    }
    (lyric.trim().to_string(), None)
}

fn is_extension(lyric: &str) -> bool {
    lyric.starts_with("+~") || lyric.starts_with("+*")
}

/// Lyric groups: a note and the following `+…` notes.
pub fn groups(notes: &[PNote]) -> Vec<Vec<usize>> {
    let mut out: Vec<Vec<usize>> = vec![];
    for (i, n) in notes.iter().enumerate() {
        match out.last_mut() {
            Some(g) if n.lyric.starts_with('+') && notes[*g.last().unwrap()].end() == n.position => g.push(i),
            _ => out.push(vec![i]),
        }
    }
    out
}

struct Ctx<'a> {
    dur: &'a SubModel,
    lg: &'a LangG2p,
    axis: &'a TimeAxis,
    speaker: &'a str,
}

struct PerNote {
    position: i64,
    tone: i32,
    phonemes: Vec<String>,
}

impl Ctx<'_> {
    /// A hint symbol as the G2p writes it (`t`, `i`, `EE`, `JJ`…), remapped like the G2p output
    /// so hinted and plain words sound alike, or else as written (a voicebank symbol). OpenUtau
    /// only accepts the latter.
    fn validate_hint(&self, ph: &str) -> Option<String> {
        self.lg.replacements.get(ph).and_then(|r| self.validate(r)).or_else(|| self.validate(ph))
    }

    fn validate(&self, ph: &str) -> Option<String> {
        if self.lg.g2p.is_valid_symbol(ph) && self.dur.tokens.contains_key(ph) {
            return Some(ph.to_string());
        }
        let p = format!("{}/{ph}", self.lg.code());
        (self.lg.g2p.is_valid_symbol(&p) && self.dur.tokens.contains_key(&p)).then_some(p)
    }

    /// GetSymbols: phonetic hint, dictionary / G2p, the lyric as a hint, nothing.
    fn symbols(&self, lyric: &str) -> (Vec<String>, Vec<String>) {
        let (word, hint) = split_hint(lyric);
        let parse = |h: &str| -> (Vec<String>, Vec<String>) {
            let ok = h.split_whitespace().filter_map(|s| self.validate_hint(s)).collect();
            let rejected = h.split_whitespace().filter(|s| self.validate_hint(s).is_none()).map(str::to_string).collect();
            (ok, rejected)
        };
        if let Some(h) = hint.filter(|h| !h.is_empty()) {
            return parse(&h);
        }
        if let Some(r) = self.lg.g2p.query(&word).or_else(|| self.lg.g2p.query(&word.to_lowercase())) {
            return (r, vec![]);
        }
        parse(&word)
    }

    /// ProcessWord: spreads the word's phonemes over its notes, one vowel per note.
    fn process_word(&self, notes: &[&PNote], symbols: &[String]) -> Result<Vec<PerNote>> {
        for s in symbols {
            if !self.lg.g2p.is_valid_symbol(s) {
                return Err(Error::Other(format!("type of symbol \"{s}\" not defined in the voicebank's dsdict")));
            }
        }
        let mut word = vec![PerNote { position: -1, tone: notes[0].tone, phonemes: vec![] }];
        let is_vowel: Vec<bool> = symbols.iter().map(|s| self.lg.g2p.is_vowel(s)).collect();
        let is_glide: Vec<bool> = symbols.iter().map(|s| self.lg.g2p.is_glide(s)).collect();
        let non_ext: Vec<&&PNote> = notes.iter().filter(|n| !is_extension(&n.lyric)).collect();
        let mut is_start = vec![false; symbols.len()];
        if is_vowel.iter().all(|v| !v) && !is_start.is_empty() {
            is_start[0] = true;
        }
        for i in 0..symbols.len() {
            if is_vowel[i] {
                if i >= 2 && is_glide[i - 1] && !is_vowel[i - 2] {
                    is_start[i - 1] = true;
                } else {
                    is_start[i] = true;
                }
            }
        }
        let mut k = 0;
        for i in 0..symbols.len() {
            if is_start[i] && k < non_ext.len() {
                word.push(PerNote { position: non_ext[k].position, tone: non_ext[k].tone, phonemes: vec![] });
                k += 1;
            }
            word.last_mut().unwrap().phonemes.push(symbols[i].clone());
        }
        Ok(word)
    }

    /// ProcessPart: duration model over one phrase of contiguous groups. Returns, for each group,
    /// its phonemes with tick offsets from the group's first note (None = lyric not recognised).
    fn process_phrase(&self, notes: &[PNote], phrase: &[Vec<usize>], warnings: &mut Vec<String>) -> Result<PhraseResult> {
        const PADDING_MS: f64 = 500.0;
        let frame_ms = self.dur.frame_ms();
        let first = &notes[phrase[0][0]];
        let last = &notes[*phrase.last().unwrap().last().unwrap()];
        let mut pp = vec![PerNote { position: -1, tone: first.tone, phonemes: vec!["SP".into()] }];
        let mut note_ph_index = vec![1usize];
        let mut found = vec![false; phrase.len()];
        for (w, group) in phrase.iter().enumerate() {
            let head = &notes[group[0]];
            let (raw, mut rejected) = self.symbols(&head.lyric);
            let mut symbols: Vec<String> = raw.iter().filter(|s| self.dur.tokens.contains_key(*s)).cloned().collect();
            rejected.extend(raw.iter().filter(|s| !self.dur.tokens.contains_key(*s)).cloned());
            if !rejected.is_empty() {
                let shown: Vec<String> = rejected.iter().map(|s| if s.is_empty() || s.contains(char::is_whitespace) { format!("{s:?}") } else { s.clone() }).collect();
                warnings.push(format!("\"{}\": fonemi non riconosciuti {}", head.lyric, shown.join(" ")));
            } else if symbols.is_empty() {
                warnings.push(format!("\"{}\": parola non riconosciuta", head.lyric));
            }
            found[w] = !symbols.is_empty();
            if symbols.is_empty() {
                symbols = vec!["SP".into()];
            }
            let group_notes: Vec<&PNote> = group.iter().map(|&i| &notes[i]).collect();
            let mut word = self.process_word(&group_notes, &symbols)?;
            let count: usize = word.iter().map(|n| n.phonemes.len()).sum();
            let lead = word.remove(0);
            pp.last_mut().unwrap().phonemes.extend(lead.phonemes);
            pp.extend(word);
            note_ph_index.push(note_ph_index.last().unwrap() + count);
        }
        pp.push(PerNote { position: last.end(), tone: last.tone, phonemes: vec![] });
        pp[0].position = self.axis.ms_to_tick(self.axis.tick_to_ms(pp[1].position as f64) - PADDING_MS);

        let phs: Vec<&String> = pp.iter().flat_map(|n| n.phonemes.iter()).collect();
        let tokens = phs.iter().map(|p| self.dur.token(p)).collect::<Result<Vec<i64>>>()?;
        let word_div: Vec<i64> = pp[..pp.len() - 1].iter().map(|n| n.phonemes.len() as i64).collect();
        let word_ms: Vec<f64> = pp
            .windows(2)
            .map(|w| self.axis.tick_to_ms(w[1].position as f64) - self.axis.tick_to_ms(w[0].position as f64))
            .collect();
        let word_dur: Vec<i64> = durations_ms_to_frames(&word_ms, frame_ms)?.into_iter().map(|x| x as i64).collect();

        let mut ling = vec![
            ("tokens", Input::I64(tokens.clone())),
            ("word_div", Input::I64(word_div.clone())),
            ("word_dur", Input::I64(word_dur.clone())),
        ];
        if self.dur.config.use_lang_id {
            let ids = phs.iter().map(|p| self.dur.language_ids.get(phoneme_language(p)).copied().unwrap_or(0)).collect();
            ling.push(("languages", Input::I64(ids)));
        }
        let mut out = self.dur.linguistic.run(ling)?;
        let encoder_out = out.take("encoder_out")?;
        let x_masks = out.take("x_masks")?;
        let ph_midi: Vec<i64> = pp.iter().flat_map(|n| std::iter::repeat_n(n.tone as i64, n.phonemes.len())).collect();
        let mut dur_in = vec![("encoder_out", Input::Raw(encoder_out)), ("x_masks", Input::Raw(x_masks)), ("ph_midi", Input::I64(ph_midi))];
        let word_inputs = match self.dur.config.dur_type.as_deref() {
            Some("rel") => true,
            None | Some("") => self.dur.main.has_input("word_div"),
            _ => false,
        };
        if word_inputs {
            dur_in.push(("word_div", Input::I64(word_div)));
            dur_in.push(("word_dur", Input::I64(word_dur)));
        }
        if let Some(sp) = &self.dur.speakers {
            dur_in.push(("spk_embed", Input::F32x2(sp.repeat(self.speaker, tokens.len()), sp.hidden)));
        }
        let mut out = self.dur.main.run(dur_in)?;
        let (_, frames) = floats(&out.first()?)?;
        let frames: Vec<f64> = frames.into_iter().map(|x| x as f64).collect();
        if frames.len() != tokens.len() {
            return Err(Error::Other(format!("duration model returned {} values for {} phonemes", frames.len(), tokens.len())));
        }

        // align: phonemes of each note are stretched to end exactly where the next note starts
        let mut align: Vec<(usize, f64)> = vec![];
        let mut cum = 0;
        for (i, n) in pp.iter().enumerate().take(pp.len() - 1) {
            cum += n.phonemes.len();
            align.push((cum, self.axis.tick_to_ms(pp[i + 1].position as f64)));
        }
        let mut positions: Vec<f64> = vec![];
        positions.extend(stretch(&frames[1..align[0].0], frame_ms, align[0].1));
        for w in align.windows(2) {
            let group = &frames[w[0].0..w[1].0];
            let sum: f64 = group.iter().sum();
            let span = w[1].1 - w[0].1;
            if sum > 0.0 {
                positions.extend(stretch(group, span / sum, w[1].1));
            } else {
                let even = vec![1.0; group.len()];
                positions.extend(stretch(&even, span / group.len().max(1) as f64, w[1].1));
            }
        }

        let mut result = vec![];
        for (w, group) in phrase.iter().enumerate() {
            let head = &notes[group[0]];
            if !found[w] {
                result.push(None);
                continue;
            }
            let note_ms = self.axis.tick_to_ms(head.position as f64);
            let mut r = vec![];
            for i in note_ph_index[w]..note_ph_index[w + 1] {
                if !phs[i].is_empty() {
                    r.push((phs[i].clone(), self.axis.ticks_between_ms(note_ms, positions[i - 1])));
                }
            }
            result.push(Some(r));
        }
        Ok(result)
    }
}

/// Positions (ms) of phonemes with the given frame durations scaled by `ratio`, ending at `end`.
fn stretch(source: &[f64], ratio: f64, end: f64) -> Vec<f64> {
    let total: f64 = source.iter().sum::<f64>() * ratio;
    let mut pos = end - total;
    let mut out = Vec::with_capacity(source.len());
    for d in source {
        out.push(pos);
        pos += d * ratio;
    }
    out
}

/// DiffSingerUtils.DurationsMsToFrames: cumulative rounding as in the training data.
pub fn durations_ms_to_frames(ms: &[f64], frame_ms: f64) -> Result<Vec<i32>> {
    let mut out = Vec::with_capacity(ms.len());
    let mut acc = 0.0;
    let mut prev = 0i32;
    for &d in ms {
        if d < 0.0 {
            return Err(Error::Other(format!("negative DiffSinger duration: {d} ms")));
        }
        acc += d;
        let frame = round_half_even(acc / frame_ms + 0.5) as i32;
        out.push(frame - prev);
        prev = frame;
    }
    Ok(out)
}

fn round_half_even(x: f64) -> f64 {
    let r = x.round();
    if (x - x.trunc()).abs() == 0.5 { 2.0 * (x / 2.0).round() } else { r }
}

/// English+ post-processing (DiffSingerARPAPlusEnglishPhonemizer.Process).
fn arpa_plus_fix(lg: &LangG2p, phs: Vec<(String, i64)>, has_prev_neighbour: bool) -> Vec<(String, i64)> {
    let lc = format!("{}/", lg.code());
    let has = |s: &str| lg.has_phoneme(s);
    let mut out = vec![];
    let mut i = 0;
    while i < phs.len() {
        let (p, pos) = &phs[i];
        let next = phs.get(i + 1).map(|x| x.0.as_str());
        if next == Some(&format!("{lc}dx")) && *p == format!("{lc}n") && has(&format!("{lc}n")) && has(&format!("{lc}dx")) {
            out.push((p.clone(), *pos));
            i += 2;
            continue;
        }
        if next == Some("dx") && p == "n" && has("n") && has("dx") && !has(&format!("{lc}n")) && !has(&format!("{lc}dx")) {
            out.push((p.clone(), *pos));
            i += 2;
            continue;
        }
        let q = *p == format!("{lc}q") || p == "q";
        let rep = if (*p == format!("{lc}cl") || !has("q") || q) && has("vf") && !has_prev_neighbour {
            Some("vf".to_string())
        } else if q && has("cl") {
            Some("cl".to_string())
        } else if *p == format!("{lc}q") && !has("cl") {
            Some("q".to_string())
        } else if p == "ax" && !has("ax") {
            Some(p.clone())
        } else if *p == format!("{lc}ax") && !has(&format!("{lc}ax")) {
            Some(format!("{lc}ah"))
        } else {
            None
        };
        out.push((rep.unwrap_or_else(|| p.clone()), *pos));
        i += 1;
    }
    out
}

/// Phonemizes a whole track: groups, phrases split at rests, duration model per phrase, then
/// phoneme ends as UPhoneme.ValidateDuration computes them.
pub fn phonemize(vb: &Voicebank, lang: Lang, notes: &[PNote], axis: &TimeAxis, speaker: &str) -> Result<Phonemized> {
    let dur = vb.duration()?;
    let lg = LangG2p::load(dur, lang)?;
    let ctx = Ctx { dur, lg: &lg, axis, speaker };
    let groups = groups(notes);
    let mut warnings = vec![];
    let group_end = |g: &Vec<usize>| notes[*g.last().unwrap()].end();

    // phrases: runs of groups that touch
    let mut phrases: Vec<std::ops::Range<usize>> = vec![];
    for (i, g) in groups.iter().enumerate() {
        match phrases.last_mut() {
            Some(r) if group_end(&groups[i - 1]) == notes[g[0]].position => r.end = i + 1,
            _ => phrases.push(i..i + 1),
        }
    }
    let mut per_group: Vec<Option<Vec<(String, i64)>>> = vec![None; groups.len()];
    for r in phrases {
        let phrase = &groups[r.clone()];
        match ctx.process_phrase(notes, phrase, &mut warnings) {
            Ok(res) => {
                for (k, v) in res.into_iter().enumerate() {
                    per_group[r.start + k] = v;
                }
            }
            Err(e) => {
                let words: Vec<&str> = phrase.iter().map(|g| notes[g[0]].lyric.as_str()).collect();
                warnings.push(format!("frase \"{}\" saltata: {e}", words.join(" ")));
            }
        }
    }

    let mut phones: Vec<Phone> = vec![];
    for (gi, g) in groups.iter().enumerate() {
        let head = &notes[g[0]];
        let (word, _) = split_hint(&head.lyric);
        let list = if word == "-" {
            Some(vec![("SP".to_string(), 0)])
        } else if word == "br" && lang == Lang::En {
            Some(vec![("AP".to_string(), 0)])
        } else {
            per_group[gi].take()
        };
        let Some(mut list) = list else { continue };
        if lang == Lang::En {
            let prev_neighbour = gi > 0 && group_end(&groups[gi - 1]) == head.position && !notes[groups[gi - 1][0]].lyric.trim().is_empty();
            list = arpa_plus_fix(&lg, list, prev_neighbour);
        }
        for (symbol, offset) in list {
            phones.push(Phone { symbol, position: head.position + offset, end: group_end(g), group: gi });
        }
    }
    // a phoneme lasts until the next one starts, at most until the end of its group
    for i in 0..phones.len() {
        if let Some(next) = phones.get(i + 1).map(|p| p.position) {
            phones[i].end = phones[i].end.min(next);
        }
    }
    let before = phones.len();
    phones.retain(|p| p.end > p.position);
    if phones.len() < before {
        warnings.push(format!("{} fonemi di durata nulla ignorati", before - phones.len()));
    }
    Ok(Phonemized { phones, groups, warnings })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hints_and_groups() {
        assert_eq!(split_hint("pietà[p y e t a]"), ("pietà".into(), Some("p y e t a".into())));
        assert_eq!(split_hint("forty"), ("forty".into(), None));
        let n = |p, d, l: &str| PNote { position: p, duration: d, tone: 60, lyric: l.into() };
        let notes = vec![n(0, 480, "forty"), n(480, 480, "+"), n(960, 480, "+~"), n(1920, 480, "win"), n(2400, 480, "+")];
        assert_eq!(groups(&notes), vec![vec![0, 1, 2], vec![3, 4]]);
    }

    #[test]
    fn frames_round_cumulatively() {
        assert_eq!(durations_ms_to_frames(&[11.6, 11.6, 11.6], 11.61).unwrap(), vec![1, 1, 1]);
        let f = durations_ms_to_frames(&[100.0, 250.0, 33.0], 512.0 * 1000.0 / 44100.0).unwrap();
        assert_eq!(f.iter().sum::<i32>(), ((383.0 / (512.0 * 1000.0 / 44100.0)) + 0.5_f64).round() as i32);
    }

    #[test]
    fn stretch_ends_at_target() {
        let p = stretch(&[1.0, 2.0, 1.0], 10.0, 100.0);
        assert_eq!(p, vec![60.0, 70.0, 90.0]);
    }
}
