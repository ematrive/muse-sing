//! Reads a MuseScore 4 `.mscz` (zip + `.mscx` XML) into per-part note lists with lyrics.
//! Reference implementation: `tools/mscx_parse.py`.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Read;
use std::path::Path;

use num_rational::Ratio;
use roxmltree::{Document, Node};

use crate::error::{Error, Result};

/// Position / duration in quarter notes.
pub type Beats = Ratio<i64>;

#[derive(Debug, Clone, PartialEq)]
pub enum Syllabic {
    Single,
    Begin,
    Middle,
    End,
}

#[derive(Debug, Clone)]
pub struct Lyric {
    pub text: String,
    pub syllabic: Syllabic,
}

#[derive(Debug, Clone)]
pub struct Note {
    pub start: Beats,
    pub dur: Beats,
    pub pitch: i32,
    pub lyric: Option<Lyric>,
}

#[derive(Debug, Clone)]
pub struct Part {
    pub name: String,
    pub notes: Vec<Note>,
}

impl Part {
    pub fn is_sung(&self) -> bool {
        self.notes.iter().any(|n| n.lyric.is_some())
    }
}

#[derive(Debug, Clone)]
pub struct Score {
    pub title: Option<String>,
    /// (numerator, denominator) of the last time signature found.
    pub time_sig: (i32, i32),
    /// (position in beats, quarter notes per second), sorted.
    pub tempos: Vec<(Beats, f64)>,
    pub parts: Vec<Part>,
    /// Things in the score that the conversion ignores or flattens.
    pub warnings: Vec<String>,
}

const GRACE_TAGS: &[&str] = &[
    "acciaccatura", "appoggiatura", "grace4", "grace8", "grace16", "grace32",
    "grace8after", "grace16after", "grace32after",
];

pub fn read_mscz(path: &Path) -> Result<Score> {
    let file = std::fs::File::open(path)?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| Error::Format(format!("non è un file MuseScore valido: {e}")))?;
    let name = (0..zip.len())
        .filter_map(|i| zip.name_for_index(i).map(str::to_string))
        .find(|n| n.ends_with(".mscx") && !n.starts_with("Excerpts/"))
        .ok_or_else(|| Error::Format("nessuna partitura (.mscx) dentro il file".into()))?;
    let mut xml = String::new();
    zip.by_name(&name)
        .map_err(|e| Error::Format(e.to_string()))?
        .read_to_string(&mut xml)?;
    parse_mscx(&xml)
}

fn child<'a, 'i>(n: Node<'a, 'i>, tag: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.has_tag_name(tag))
}

fn child_text<'a>(n: Node<'a, '_>, tag: &str) -> Option<&'a str> {
    child(n, tag).map(|c| c.text().unwrap_or(""))
}

fn all_text(n: Node) -> String {
    n.descendants().filter(|d| d.is_text()).filter_map(|d| d.text()).collect()
}

fn ratio(s: &str) -> Option<Beats> {
    let s = s.trim();
    match s.split_once('/') {
        Some((a, b)) => Some(Ratio::new(a.trim().parse().ok()?, b.trim().parse().ok()?)),
        None => Some(Ratio::from_integer(s.parse().ok()?)),
    }
}

fn base_len(dt: &str) -> Option<Beats> {
    Some(match dt {
        "breve" => Ratio::from_integer(8),
        "whole" => Ratio::from_integer(4),
        "half" => Ratio::from_integer(2),
        "quarter" => Ratio::from_integer(1),
        "eighth" => Ratio::new(1, 2),
        "16th" => Ratio::new(1, 4),
        "32nd" => Ratio::new(1, 8),
        "64th" => Ratio::new(1, 16),
        "128th" => Ratio::new(1, 32),
        _ => return None,
    })
}

fn chord_len(el: Node, tuplet: Beats) -> Beats {
    let dt = child_text(el, "durationType").unwrap_or("quarter");
    if dt == "measure" {
        let d = child_text(el, "duration").and_then(ratio).unwrap_or(Ratio::from_integer(1));
        return d * 4;
    }
    let base = base_len(dt).unwrap_or(Ratio::from_integer(1));
    let dots: i32 = child_text(el, "dots").and_then(|d| d.parse().ok()).unwrap_or(0);
    let (mut tot, mut add) = (base, base);
    for _ in 0..dots {
        add /= 2;
        tot += add;
    }
    tot * tuplet
}

pub fn parse_mscx(xml: &str) -> Result<Score> {
    let doc = Document::parse(xml).map_err(|e| Error::Format(format!("XML non valido: {e}")))?;
    let score = doc
        .root_element()
        .children()
        .find(|c| c.has_tag_name("Score"))
        .ok_or_else(|| Error::Format("manca l'elemento <Score>".into()))?;

    // staff id -> part name (Staff elements inside Part have an id, or are numbered in order)
    let mut staff_name: HashMap<String, String> = HashMap::new();
    let mut idx = 0;
    for part in score.children().filter(|c| c.has_tag_name("Part")) {
        let name = child_text(part, "trackName")
            .or_else(|| child(part, "Instrument").and_then(|i| child_text(i, "longName")))
            .unwrap_or("")
            .to_string();
        for st in part.children().filter(|c| c.has_tag_name("Staff")) {
            idx += 1;
            let id = st.attribute("id").map(str::to_string).unwrap_or_else(|| idx.to_string());
            staff_name.insert(id, name.clone());
        }
    }

    let mut title = None;
    for t in score.descendants().filter(|d| d.has_tag_name("Text")) {
        if child_text(t, "style") == Some("title") {
            title = child(t, "text").map(|x| all_text(x).trim().to_string());
        }
    }

    let staves: Vec<Node> = score.children().filter(|c| c.has_tag_name("Staff")).collect();
    let mut warnings = vec![];
    let mut tempos: BTreeMap<Beats, f64> = BTreeMap::new();
    let mut time_sigs: Vec<(i32, i32)> = vec![];
    let mut lyrics_other_voice = BTreeSet::new();

    // 1) read every staff measure by measure
    let mut staff_measures: Vec<Vec<MeasureData>> = vec![];
    for staff in &staves {
        let sid = staff.attribute("id").unwrap_or("").to_string();
        let name = staff_name.get(&sid).cloned().unwrap_or_else(|| sid.clone());
        let mut ms = vec![];
        for meas in staff.children().filter(|c| c.has_tag_name("Measure")) {
            ms.push(read_measure(meas, &name, &mut time_sigs, &mut lyrics_other_voice));
        }
        staff_measures.push(ms);
    }

    // 2) playback order (repeats, voltas, D.C./D.S.), from the structure of all staves
    let n_meas = staff_measures.iter().map(Vec::len).max().unwrap_or(0);
    let structure: Vec<Structure> = (0..n_meas)
        .map(|i| {
            let mut st = Structure::default();
            for ms in &staff_measures {
                if let Some(m) = ms.get(i) {
                    st.merge(&m.structure);
                }
            }
            st
        })
        .collect();
    let lens: Vec<Beats> = (0..n_meas)
        .map(|i| staff_measures.iter().filter_map(|ms| ms.get(i)).map(|m| m.len).max().unwrap_or_default())
        .collect();
    let (order, unsupported) = playback_order(&structure);
    warnings.extend(unsupported);

    // 3) lay the measures out in that order
    let max_verse = staff_measures.iter().flatten().flat_map(|m| &m.notes).flat_map(|n| n.lyrics.keys()).max().copied().unwrap_or(0);
    let max_pass = order.iter().map(|(_, p)| *p).max().unwrap_or(1);
    let mut parts: Vec<Part> = vec![];
    for (si, staff) in staves.iter().enumerate() {
        let sid = staff.attribute("id").unwrap_or("").to_string();
        let mut name = staff_name.get(&sid).cloned().unwrap_or_else(|| sid.clone());
        if parts.iter().any(|p| p.name == name) {
            name = format!("{name} ({sid})");
        }
        let mut notes: Vec<(Note, bool)> = vec![];
        let mut mstart: Beats = Ratio::from_integer(0);
        for &(mi, pass) in &order {
            if let Some(m) = staff_measures[si].get(mi) {
                if si == 0 {
                    for (off, t) in &m.tempos {
                        tempos.insert(mstart + off, *t);
                    }
                }
                for n in &m.notes {
                    // verse = pass through the repeat. A note with only the first verse (a refrain
                    // written once) sings it on every pass; a note that has other verses but not
                    // this one holds the previous syllable (melisma), it doesn't borrow verse 1.
                    let verse = pass as i32 - 1;
                    let shared = n.lyrics.keys().all(|&k| k == 0);
                    let lyric = n.lyrics.get(&verse).or_else(|| if shared { n.lyrics.get(&0) } else { None }).cloned();
                    notes.push((Note { start: mstart + n.offset, dur: n.dur, pitch: n.pitch, lyric }, n.tie_prev));
                }
            }
            mstart += lens[mi];
        }
        let mut merged: Vec<Note> = vec![];
        for (n, tie_prev) in notes {
            if tie_prev && merged.last().is_some_and(|m| m.pitch == n.pitch) {
                merged.last_mut().unwrap().dur += n.dur;
                continue;
            }
            merged.push(n);
        }
        parts.push(Part { name, notes: merged });
    }
    // tempo marks of the other staves (rare): keep the first staff's, add the missing ones
    if tempos.is_empty() {
        for (si, _) in staves.iter().enumerate().skip(1) {
            let mut mstart: Beats = Ratio::from_integer(0);
            for &(mi, _) in &order {
                if let Some(m) = staff_measures[si].get(mi) {
                    for (off, t) in &m.tempos {
                        tempos.insert(mstart + off, *t);
                    }
                }
                mstart += lens[mi];
            }
        }
    }

    if max_verse as u32 + 1 > max_pass {
        warnings.push(format!(
            "Ci sono {} strofe ma i ritornelli ne fanno cantare {}: le altre non vengono cantate.",
            max_verse + 1,
            max_pass
        ));
    }
    if !lyrics_other_voice.is_empty() {
        let v: Vec<String> = lyrics_other_voice.into_iter().collect();
        warnings.push(format!("Testo nella seconda voce del pentagramma ({}): viene usata solo la prima voce.", v.join(", ")));
    }
    if score.descendants().any(|d| d.has_tag_name("MeasureRepeat")) {
        warnings.push("La partitura ha battute di ripetizione (simile): restano vuote.".into());
    }
    if time_sigs.len() > 1 {
        warnings.push("Il metro cambia durante il brano: le note restano a tempo, ma la griglia delle battute in OpenUtau segue solo l'ultimo metro.".into());
    }

    let mut tempos: Vec<(Beats, f64)> = tempos.into_iter().collect();
    if tempos.is_empty() {
        tempos.push((Ratio::from_integer(0), 2.0));
    }
    Ok(Score {
        title,
        time_sig: time_sigs.last().copied().unwrap_or((4, 4)),
        tempos,
        parts,
        warnings,
    })
}

/// Repeat / jump information of one measure.
#[derive(Debug, Clone, Default)]
pub struct Structure {
    pub start_repeat: bool,
    /// Times the section is played (endRepeat), 0 = no end repeat.
    pub end_repeat: u32,
    /// Volta endings that start here and how many measures they cover.
    pub volta: Option<(Vec<u32>, usize)>,
    /// Marker labels in this measure (segno, coda, fine, tocoda, ...).
    pub markers: Vec<String>,
    /// (jumpTo, playUntil, continueAt)
    pub jump: Option<(String, String, String)>,
}

impl Structure {
    fn merge(&mut self, o: &Structure) {
        self.start_repeat |= o.start_repeat;
        self.end_repeat = self.end_repeat.max(o.end_repeat);
        if self.volta.is_none() {
            self.volta = o.volta.clone();
        }
        for m in &o.markers {
            if !self.markers.contains(m) {
                self.markers.push(m.clone());
            }
        }
        if self.jump.is_none() {
            self.jump = o.jump.clone();
        }
    }
}

struct NoteData {
    offset: Beats,
    dur: Beats,
    pitch: i32,
    tie_prev: bool,
    /// verse number (0-based) -> lyric
    lyrics: BTreeMap<i32, Lyric>,
}

struct MeasureData {
    len: Beats,
    notes: Vec<NoteData>,
    tempos: Vec<(Beats, f64)>,
    structure: Structure,
}

fn read_measure(meas: Node, staff_name: &str, time_sigs: &mut Vec<(i32, i32)>, lyrics_other_voice: &mut BTreeSet<String>) -> MeasureData {
    let mut st = Structure::default();
    if child(meas, "startRepeat").is_some() {
        st.start_repeat = true;
    }
    if let Some(e) = child(meas, "endRepeat") {
        st.end_repeat = e.text().and_then(|t| t.trim().parse().ok()).unwrap_or(2);
    }
    let mut notes = vec![];
    let mut tempos = vec![];
    let mut mlen: Option<Beats> = None;
    let zero: Beats = Ratio::from_integer(0);
    for (vi, voice) in meas.children().filter(|c| c.has_tag_name("voice")).enumerate() {
        let mut pos = zero;
        // <location> can move back (e.g. chord symbols): the measure ends at the furthest point
        let mut max_pos = zero;
        let mut tuplet: Beats = Ratio::from_integer(1);
        for el in voice.children().filter(|c| c.is_element()) {
            match el.tag_name().name() {
                "TimeSig" => {
                    let n = child_text(el, "sigN").and_then(|s| s.parse().ok()).unwrap_or(4);
                    let d = child_text(el, "sigD").and_then(|s| s.parse().ok()).unwrap_or(4);
                    if time_sigs.last() != Some(&(n, d)) {
                        time_sigs.push((n, d));
                    }
                }
                "Tempo" => {
                    if let Some(t) = child_text(el, "tempo").and_then(|s| s.parse::<f64>().ok()) {
                        tempos.push((pos, t));
                    }
                }
                "Marker" => {
                    if let Some(l) = child_text(el, "label") {
                        st.markers.push(l.trim().to_string());
                    }
                }
                "Jump" => {
                    let g = |k| child_text(el, k).unwrap_or("").trim().to_string();
                    st.jump = Some((g("jumpTo"), g("playUntil"), g("continueAt")));
                }
                "Spanner" if el.attribute("type") == Some("Volta") => {
                    if let Some(v) = child(el, "Volta") {
                        let endings: Vec<u32> = child_text(v, "endings")
                            .unwrap_or("")
                            .split([',', ' '])
                            .filter_map(|x| x.trim().parse().ok())
                            .collect();
                        let span = child(el, "next")
                            .and_then(|n| child(n, "location"))
                            .and_then(|l| child_text(l, "measures"))
                            .and_then(|m| m.trim().parse().ok())
                            .unwrap_or(1);
                        if !endings.is_empty() {
                            st.volta = Some((endings, span));
                        }
                    }
                }
                "Tuplet" => {
                    let nn = child_text(el, "normalNotes").and_then(|s| s.parse().ok()).unwrap_or(1);
                    let an = child_text(el, "actualNotes").and_then(|s| s.parse().ok()).unwrap_or(1);
                    tuplet = Ratio::new(nn, an);
                }
                "endTuplet" => tuplet = Ratio::from_integer(1),
                "location" => {
                    if let Some(f) = child_text(el, "fractions").and_then(ratio) {
                        pos += f * 4;
                    }
                }
                "Chord" | "Rest" => {
                    if GRACE_TAGS.iter().any(|g| child(el, g).is_some()) {
                        continue; // grace notes take no time
                    }
                    let ln = chord_len(el, tuplet);
                    if el.has_tag_name("Chord") {
                        let note_els: Vec<_> = el.children().filter(|c| c.has_tag_name("Note")).collect();
                        let silent = !note_els.is_empty() && note_els.iter().all(|n| child_text(*n, "play") == Some("0"));
                        let mut lyrics = BTreeMap::new();
                        for l in el.children().filter(|c| c.has_tag_name("Lyrics")) {
                            let no: i32 = child_text(l, "no").and_then(|n| n.trim().parse().ok()).unwrap_or(0);
                            if vi > 0 {
                                lyrics_other_voice.insert(staff_name.to_string());
                            }
                            let syllabic = match child_text(l, "syllabic") {
                                Some("begin") => Syllabic::Begin,
                                Some("middle") => Syllabic::Middle,
                                Some("end") => Syllabic::End,
                                _ => Syllabic::Single,
                            };
                            lyrics.insert(no, Lyric { text: child(l, "text").map(all_text).unwrap_or_default(), syllabic });
                        }
                        if vi == 0 && !silent && !note_els.is_empty() {
                            let pitch = note_els
                                .iter()
                                .filter_map(|n| child_text(*n, "pitch").and_then(|p| p.parse::<i32>().ok()))
                                .max()
                                .unwrap_or(60);
                            let tie_prev = note_els.iter().any(|n| {
                                n.children().any(|s| s.has_tag_name("Spanner") && s.attribute("type") == Some("Tie") && child(s, "prev").is_some())
                            });
                            notes.push(NoteData { offset: pos, dur: ln, pitch, tie_prev, lyrics });
                        }
                    }
                    pos += ln;
                    max_pos = max_pos.max(pos);
                }
                _ => {}
            }
        }
        if mlen.is_none_or(|m| max_pos > m) {
            mlen = Some(max_pos);
        }
    }
    if let Some(l) = meas.attribute("len").and_then(ratio) {
        mlen = Some(l * 4);
    }
    MeasureData { len: mlen.unwrap_or(zero), notes, tempos, structure: st }
}

/// Measures in playback order, each with its pass number through the current repeat (1-based,
/// used to pick the verse). Follows MuseScore's playback: repeats are not taken again after a
/// D.C./D.S. jump, Fine and To Coda only apply after the jump.
pub fn playback_order(st: &[Structure]) -> (Vec<(usize, u32)>, Vec<String>) {
    let mut warnings = vec![];
    let n = st.len();
    // measures covered by voltas: index -> endings
    let mut volta_of: Vec<Option<Vec<u32>>> = vec![None; n];
    for (i, s) in st.iter().enumerate() {
        if let Some((endings, span)) = &s.volta {
            for slot in volta_of.iter_mut().skip(i).take((*span).max(1)) {
                *slot = Some(endings.clone());
            }
        }
    }
    let find_marker = |label: &str| -> Option<usize> {
        match label {
            "start" | "" => Some(0),
            "end" => Some(n),
            l => st.iter().position(|s| s.markers.iter().any(|m| m == l)),
        }
    };
    let mut order = vec![];
    let mut i = 0;
    let mut repeat_start = 0; // where the current repeat section begins
    let mut pass: u32 = 1; // pass through the current repeat section
    let mut going_back = false; // true right after jumping back for a repeat
    let mut jumped = false; // a D.C./D.S. was taken
    let mut play_until = String::new();
    let mut continue_at = String::new();
    let mut guard = 0;
    while i < n {
        guard += 1;
        if guard > 10_000 {
            warnings.push("Struttura dei ritornelli non riconosciuta: il brano è stato troncato.".into());
            break;
        }
        let s = &st[i];
        if s.start_repeat && !going_back {
            repeat_start = i;
            pass = 1;
        }
        going_back = false;
        // volta: play only on the matching pass (after a jump: the last ending)
        if let Some(endings) = &volta_of[i] {
            let want = if jumped { last_pass_of(st, i) } else { pass };
            if !endings.contains(&want) {
                i += 1;
                continue;
            }
        }
        order.push((i, if jumped { 1 } else { pass }));
        // after a jump: stop at Fine, or go to the Coda at "To Coda"
        if jumped && !play_until.is_empty() && s.markers.contains(&play_until) {
            if continue_at.is_empty() {
                break;
            }
            match find_marker(&continue_at) {
                Some(t) => {
                    i = t;
                    play_until.clear();
                    continue;
                }
                None => {
                    warnings.push(format!("Salto a «{continue_at}» non trovato."));
                    break;
                }
            }
        }
        if s.end_repeat > 0 && !jumped {
            if pass < s.end_repeat {
                pass += 1;
                i = repeat_start;
                going_back = true;
                continue;
            }
            pass = 1;
            repeat_start = i + 1;
        }
        if let Some((to, until, cont)) = &s.jump
            && !jumped {
                match find_marker(to) {
                    Some(t) => {
                        jumped = true;
                        play_until = until.clone();
                        continue_at = cont.clone();
                        pass = 1;
                        i = t;
                        continue;
                    }
                    None => warnings.push(format!("Salto a «{to}» non trovato: ignorato.")),
                }
            }
        i += 1;
    }
    (order, warnings)
}

/// Highest volta ending at measure i (the last pass): after a D.C./D.S. MuseScore plays the last ending.
fn last_pass_of(st: &[Structure], i: usize) -> u32 {
    let mut best = 0;
    for (k, s) in st.iter().enumerate().take(i + 1) {
        if let Some((endings, span)) = &s.volta
            && k + span.max(&1) > i {
                best = best.max(*endings.iter().max().unwrap_or(&0));
            }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bars(n: usize) -> Vec<Structure> {
        vec![Structure::default(); n]
    }
    fn idx(o: &[(usize, u32)]) -> Vec<usize> {
        o.iter().map(|(i, _)| i + 1).collect() // 1-based bar numbers
    }

    #[test]
    fn simple_repeat_with_passes() {
        let mut s = bars(4);
        s[0].start_repeat = true;
        s[2].end_repeat = 2;
        let (o, w) = playback_order(&s);
        assert!(w.is_empty());
        assert_eq!(idx(&o), vec![1, 2, 3, 1, 2, 3, 4]);
        assert_eq!(o.iter().map(|x| x.1).collect::<Vec<_>>(), vec![1, 1, 1, 2, 2, 2, 1]);
    }

    #[test]
    fn voltas() {
        // |: 1 2 [1. 3 :|] [2. 4] 5
        let mut s = bars(5);
        s[0].start_repeat = true;
        s[2].volta = Some((vec![1], 1));
        s[2].end_repeat = 2;
        s[3].volta = Some((vec![2], 1));
        let (o, _) = playback_order(&s);
        assert_eq!(idx(&o), vec![1, 2, 3, 1, 2, 4, 5]);
    }

    #[test]
    fn da_capo_al_fine() {
        // 1 2(Fine) 3 4(D.C. al Fine)
        let mut s = bars(4);
        s[1].markers.push("fine".into());
        s[3].jump = Some(("start".into(), "fine".into(), String::new()));
        let (o, _) = playback_order(&s);
        assert_eq!(idx(&o), vec![1, 2, 3, 4, 1, 2]);
    }

    #[test]
    fn dal_segno_al_coda() {
        // 1 2(Segno) 3(To Coda) 4(D.S. al Coda) 5(Coda)
        let mut s = bars(5);
        s[1].markers.push("segno".into());
        s[2].markers.push("coda".into()); // "To Coda" label in MuseScore 4 is the playUntil target
        s[3].jump = Some(("segno".into(), "coda".into(), "codab".into()));
        s[4].markers.push("codab".into());
        let (o, _) = playback_order(&s);
        assert_eq!(idx(&o), vec![1, 2, 3, 4, 2, 3, 5]);
    }

    #[test]
    fn repeats_not_taken_after_jump() {
        // |: 1 :| 2(D.C.)
        let mut s = bars(2);
        s[0].start_repeat = true;
        s[0].end_repeat = 2;
        s[1].jump = Some(("start".into(), String::new(), String::new()));
        let (o, _) = playback_order(&s);
        assert_eq!(idx(&o), vec![1, 1, 2, 1, 2]);
    }
}
