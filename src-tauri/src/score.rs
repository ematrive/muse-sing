//! Reads a MuseScore 4 `.mscz` (zip + `.mscx` XML) into per-part note lists with lyrics; MusicXML
//! goes through `musicxml.rs` and the same `assemble`. Reference implementation: `tools/mscx_parse.py`.

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
    /// Stable id: `measure.time-it-is-played.offset-in-ticks` (measure numbered from 1 in the
    /// written score). Edits are tied to it, so they survive reloading a changed score.
    pub id: String,
    pub start: Beats,
    pub dur: Beats,
    pub pitch: i32,
    pub lyric: Option<Lyric>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tone {
    pub start: Beats,
    pub dur: Beats,
    pub pitch: i32,
}

#[derive(Debug, Clone, Default)]
pub struct Part {
    pub name: String,
    /// The melody: first voice, top note of each chord (what is sung).
    pub notes: Vec<Note>,
    /// Every note of every voice, chords included, ties merged: what an instrument plays.
    pub tones: Vec<Tone>,
    /// Dynamics, hairpins, breaths, fermatas and tempo words, as played (see `marks.rs`).
    pub marks: Marks,
}

/// Start of a hairpin, or its end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hairpin {
    Cresc,
    Dim,
    End,
}

/// A written change of tempo without a metronome mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TempoWord {
    /// rit., rall., allarg.
    Slower,
    /// accel., string.
    Faster,
    /// End of a rit./accel. line.
    End,
    /// a tempo, tempo I
    ATempo,
}

impl TempoWord {
    /// From the text written in the score ("rit.", "poco rall.", "a tempo"…). Whole words only:
    /// "ritornello" is not a ritardando.
    pub fn from_text(text: &str) -> Option<TempoWord> {
        let t = text.to_lowercase();
        let words: Vec<&str> = t.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
        let has = |keys: &[&str]| words.iter().any(|w| keys.contains(w));
        if t.contains("a tempo") || t.contains("tempo i") || t.contains("tempo primo") {
            Some(TempoWord::ATempo)
        } else if has(&["rit", "ritard", "ritardando", "riten", "ritenuto", "rall", "rallent", "rallentando", "allarg", "allargando", "slower"]) {
            Some(TempoWord::Slower)
        } else if has(&["accel", "accelerando", "string", "stringendo", "faster"]) {
            Some(TempoWord::Faster)
        } else {
            None
        }
    }
}

/// Dynamic level in dB relative to mezzo forte, from its name ("p", "mf"…).
pub fn dynamic_db(name: &str) -> Option<f32> {
    Some(match name.trim() {
        "pppp" => -22.0,
        "ppp" => -18.0,
        "pp" => -13.0,
        "p" | "fp" => -8.0,
        "mp" => -4.0,
        "mf" => 0.0,
        "f" => 3.0,
        "ff" => 5.0,
        "fff" => 7.0,
        "ffff" => 9.0,
        _ => return None,
    })
}

/// Performance markings of a part. Positions in quarter notes, in the measure while reading,
/// on the played timeline in `Part`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Marks {
    /// (position, dB relative to mf)
    pub dynamics: Vec<(Beats, f32)>,
    pub hairpins: Vec<(Beats, Hairpin)>,
    /// Where a breath is written: the end of the note before the mark.
    pub breaths: Vec<Beats>,
    /// (start, duration) of a note or rest with a fermata.
    pub fermatas: Vec<(Beats, Beats)>,
    pub tempo: Vec<(Beats, TempoWord)>,
}

impl Marks {
    fn is_empty(&self) -> bool {
        self.dynamics.is_empty() && self.hairpins.is_empty() && self.breaths.is_empty() && self.fermatas.is_empty() && self.tempo.is_empty()
    }

    /// Adds `o`, moved by `by`.
    fn extend_at(&mut self, o: &Marks, by: Beats) {
        self.dynamics.extend(o.dynamics.iter().map(|&(a, d)| (a + by, d)));
        self.hairpins.extend(o.hairpins.iter().map(|&(a, h)| (a + by, h)));
        self.breaths.extend(o.breaths.iter().map(|&a| a + by));
        self.fermatas.extend(o.fermatas.iter().map(|&(a, d)| (a + by, d)));
        self.tempo.extend(o.tempo.iter().map(|&(a, w)| (a + by, w)));
    }

    fn sort(&mut self) {
        self.dynamics.sort_by_key(|a| a.0);
        self.hairpins.sort_by_key(|a| a.0);
        self.breaths.sort();
        self.breaths.dedup();
        self.fermatas.sort();
        self.fermatas.dedup();
        self.tempo.sort_by_key(|a| a.0);
    }
}

impl Part {
    pub fn is_sung(&self) -> bool {
        self.notes.iter().any(|n| n.lyric.is_some())
    }
}

#[derive(Debug, Clone)]
pub struct Score {
    pub title: Option<String>,
    /// (numerator, denominator) of the first time signature.
    pub time_sig: (i32, i32),
    /// The time signatures used, each once, in the order they first appear.
    pub time_sigs: Vec<(i32, i32)>,
    /// The measures as they are played (repeats unrolled).
    pub bars: Vec<Bar>,
    /// (position in beats, quarter notes per second), sorted.
    pub tempos: Vec<(Beats, f64)>,
    pub parts: Vec<Part>,
    /// Things in the score that the conversion ignores or flattens.
    pub warnings: Vec<String>,
}

/// One measure as played.
#[derive(Debug, Clone, PartialEq)]
pub struct Bar {
    pub start: Beats,
    /// Number printed in the score (`None`: a measure left out of the count, like a pickup).
    pub number: Option<String>,
    /// The time signature, on the first measure and where it changes.
    pub time_sig: Option<(i32, i32)>,
}

const GRACE_TAGS: &[&str] = &[
    "acciaccatura", "appoggiatura", "grace4", "grace8", "grace16", "grace32",
    "grace8after", "grace16after", "grace32after",
];

/// Score file formats, recognised from the content (not the file name).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Mscz,
    /// Compressed MusicXML.
    Mxl,
    MusicXml,
}

impl Format {
    /// File extension for a copy of the score.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Mscz => "mscz",
            Format::Mxl => "mxl",
            Format::MusicXml => "musicxml",
        }
    }

    pub fn detect(bytes: &[u8]) -> Option<Format> {
        if bytes.starts_with(b"PK") {
            let zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).ok()?;
            let names: Vec<&str> = zip.file_names().collect();
            if names.iter().any(|n| n.ends_with(".mscx")) {
                return Some(Format::Mscz);
            }
            if names.iter().any(|n| *n == "META-INF/container.xml" || n.ends_with(".musicxml") || n.ends_with(".xml")) {
                return Some(Format::Mxl);
            }
            return None;
        }
        // skip a byte-order mark, the XML declaration, comments and the DOCTYPE
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]);
        (head.contains("<score-partwise") || head.contains("<score-timewise")).then_some(Format::MusicXml)
    }
}

/// Reads a MuseScore file or a MusicXML file (plain or compressed).
pub fn read_score(path: &Path) -> Result<Score> {
    let bytes = std::fs::read(path)?;
    match Format::detect(&bytes) {
        Some(Format::Mscz) => read_mscz(path),
        Some(Format::Mxl) => crate::musicxml::parse_musicxml(&crate::musicxml::mxl_root(&bytes)?),
        Some(Format::MusicXml) => {
            let text = String::from_utf8(bytes)
                .or_else(|e| utf16(e.as_bytes()).ok_or(e))
                .map_err(|_| Error::Format("MusicXML: codifica del testo non riconosciuta".into()))?;
            crate::musicxml::parse_musicxml(text.trim_start_matches('\u{feff}'))
        }
        None => Err(Error::Format("serve una partitura MuseScore (.mscz) o MusicXML (.musicxml, .mxl)".into())),
    }
}

/// UTF-16 text with a byte-order mark (some Windows exporters).
fn utf16(b: &[u8]) -> Option<String> {
    let le = match b.get(..2)? {
        [0xFF, 0xFE] => true,
        [0xFE, 0xFF] => false,
        _ => return None,
    };
    let units: Vec<u16> = b[2..].as_chunks::<2>().0.iter().map(|c| if le { u16::from_le_bytes(*c) } else { u16::from_be_bytes(*c) }).collect();
    String::from_utf16(&units).ok()
}

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
    let mut lyrics_other_voice = BTreeSet::new();

    // printed measure numbers, as MuseScore counts them: measures marked "exclude from measure
    // count" (irregular) get none, "add to measure number" (noOffset) moves the count
    let mut numbers: Vec<Option<String>> = vec![];
    if let Some(first) = staves.first() {
        let mut no: i64 = 1;
        for meas in first.children().filter(|c| c.has_tag_name("Measure")) {
            no += child_text(meas, "noOffset").and_then(|t| t.trim().parse::<i64>().ok()).unwrap_or(0);
            if child_text(meas, "irregular").is_some_and(|t| t.trim() != "0") {
                numbers.push(None);
            } else {
                numbers.push(Some(no.to_string()));
                no += 1;
            }
        }
    }

    // 1) read every staff measure by measure
    let mut staff_data: Vec<StaffData> = vec![];
    for staff in &staves {
        let sid = staff.attribute("id").unwrap_or("").to_string();
        let name = staff_name.get(&sid).cloned().unwrap_or_else(|| sid.clone());
        let mut ms = vec![];
        for (mi, meas) in staff.children().filter(|c| c.has_tag_name("Measure")).enumerate() {
            let mut m = read_measure(meas, &name, &mut lyrics_other_voice);
            m.number = numbers.get(mi).cloned().unwrap_or_else(|| Some((mi + 1).to_string()));
            ms.push(m);
        }
        staff_data.push(StaffData { name, id: sid, measures: ms });
    }
    if score.descendants().any(|d| d.has_tag_name("MeasureRepeat")) {
        warnings.push("La partitura ha battute di ripetizione (simile): restano vuote.".into());
    }
    Ok(assemble(title, staff_data, lyrics_other_voice, warnings))
}

/// The measures of one staff as written, before repeats are unrolled.
pub(crate) struct StaffData {
    pub name: String,
    /// Shown after the name when two staves have the same name.
    pub id: String,
    pub measures: Vec<MeasureData>,
}

/// From the written measures of every staff to the parts as they are played: repeats, voltas and
/// jumps unrolled, verses picked by pass, ties merged. Shared by MuseScore and MusicXML.
pub(crate) fn assemble(
    title: Option<String>,
    staff_data: Vec<StaffData>,
    lyrics_other_voice: BTreeSet<String>,
    mut warnings: Vec<String>,
) -> Score {
    let mut tempos: BTreeMap<Beats, f64> = BTreeMap::new();
    let staff_measures: Vec<&Vec<MeasureData>> = staff_data.iter().map(|s| &s.measures).collect();

    // 2) playback order (repeats, voltas, D.C./D.S.), from the structure of all staves
    let n_meas = staff_measures.iter().map(|m| m.len()).max().unwrap_or(0);
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

    // time signatures and printed numbers of the written measures (first staff that has them)
    let sig_at: Vec<Option<(i32, i32)>> =
        (0..n_meas).map(|i| staff_measures.iter().filter_map(|ms| ms.get(i)).find_map(|m| m.time_sig)).collect();
    let mut time_sigs: Vec<(i32, i32)> = vec![];
    for s in sig_at.iter().flatten() {
        if !time_sigs.contains(s) {
            time_sigs.push(*s);
        }
    }
    // the signature in force in each written measure
    let mut cur = time_sigs.first().copied().unwrap_or((4, 4));
    let in_force: Vec<(i32, i32)> = sig_at
        .iter()
        .map(|s| {
            cur = s.unwrap_or(cur);
            cur
        })
        .collect();
    let number_at: Vec<Option<String>> =
        (0..n_meas).map(|i| staff_measures.iter().find_map(|ms| ms.get(i)).and_then(|m| m.number.clone())).collect();
    let mut bars: Vec<Bar> = vec![];
    let mut bstart: Beats = Ratio::from_integer(0);
    for &(mi, _) in &order {
        let sig = in_force[mi];
        let changed = bars.is_empty() || in_force[order[bars.len() - 1].0] != sig;
        bars.push(Bar { start: bstart, number: number_at[mi].clone(), time_sig: changed.then_some(sig) });
        bstart += lens[mi];
    }

    // 3) lay the measures out in that order
    let max_verse = staff_measures.iter().copied().flatten().flat_map(|m| &m.notes).flat_map(|n| n.lyrics.keys()).max().copied().unwrap_or(0);
    let max_pass = order.iter().map(|(_, p)| *p).max().unwrap_or(1);
    let mut parts: Vec<Part> = vec![];
    for (si, staff) in staff_data.iter().enumerate() {
        let mut name = staff.name.clone();
        if parts.iter().any(|p| p.name == name) {
            name = format!("{name} ({})", staff.id);
        }
        let mut notes: Vec<(Note, bool)> = vec![];
        let mut tones: Vec<Tone> = vec![];
        let mut marks = Marks::default();
        // last tone of each pitch, to merge ties
        let mut last_tone: HashMap<i32, usize> = HashMap::new();
        let mut mstart: Beats = Ratio::from_integer(0);
        let mut played: HashMap<usize, u32> = HashMap::new();
        for &(mi, pass) in &order {
            let occurrence = {
                let c = played.entry(mi).or_insert(0);
                *c += 1;
                *c
            };
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
                    let off = (n.offset * Ratio::from_integer(480)).round().to_integer();
                    let id = format!("{}.{occurrence}.{off}", mi + 1);
                    notes.push((Note { id, start: mstart + n.offset, dur: n.dur, pitch: n.pitch, lyric }, n.tie_prev));
                }
                if !m.marks.is_empty() {
                    marks.extend_at(&m.marks, mstart);
                }
                for t in &m.tones {
                    let start = mstart + t.offset;
                    if t.tie_prev
                        && let Some(&k) = last_tone.get(&t.pitch)
                        && tones[k].start + tones[k].dur == start
                    {
                        tones[k].dur += t.dur;
                        continue;
                    }
                    last_tone.insert(t.pitch, tones.len());
                    tones.push(Tone { start, dur: t.dur, pitch: t.pitch });
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
        // the same pitch struck twice at once (two voices sharing a note): one, the longer
        tones.sort_by(|a, b| a.start.cmp(&b.start).then(a.pitch.cmp(&b.pitch)).then(b.dur.cmp(&a.dur)));
        tones.dedup_by(|b, a| a.start == b.start && a.pitch == b.pitch);
        marks.sort();
        parts.push(Part { name, notes: merged, tones, marks });
    }
    // tempo marks of the other staves (rare): keep the first staff's, add the missing ones
    if tempos.is_empty() {
        for ms in staff_measures.iter().skip(1) {
            let mut mstart: Beats = Ratio::from_integer(0);
            for &(mi, _) in &order {
                if let Some(m) = ms.get(mi) {
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

    let mut tempos: Vec<(Beats, f64)> = tempos.into_iter().collect();
    if tempos.is_empty() {
        tempos.push((Ratio::from_integer(0), 2.0));
    }
    Score {
        title,
        time_sig: time_sigs.first().copied().unwrap_or((4, 4)),
        time_sigs,
        bars,
        tempos,
        parts,
        warnings,
    }
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

pub(crate) struct NoteData {
    pub offset: Beats,
    pub dur: Beats,
    pub pitch: i32,
    pub tie_prev: bool,
    /// verse number (0-based) -> lyric
    pub lyrics: BTreeMap<i32, Lyric>,
}

/// One sounding pitch of a measure (any voice, any note of a chord).
pub(crate) struct ToneData {
    pub offset: Beats,
    pub dur: Beats,
    pub pitch: i32,
    pub tie_prev: bool,
}

pub(crate) struct MeasureData {
    pub len: Beats,
    pub notes: Vec<NoteData>,
    pub tones: Vec<ToneData>,
    /// (offset in the measure, quarter notes per second)
    pub tempos: Vec<(Beats, f64)>,
    pub structure: Structure,
    /// Time signature written in this measure.
    pub time_sig: Option<(i32, i32)>,
    /// Printed number (`None`: left out of the count).
    pub number: Option<String>,
    pub marks: Marks,
}

fn read_measure(meas: Node, staff_name: &str, lyrics_other_voice: &mut BTreeSet<String>) -> MeasureData {
    let mut st = Structure::default();
    if child(meas, "startRepeat").is_some() {
        st.start_repeat = true;
    }
    if let Some(e) = child(meas, "endRepeat") {
        st.end_repeat = e.text().and_then(|t| t.trim().parse().ok()).unwrap_or(2);
    }
    let mut notes = vec![];
    let mut tempos = vec![];
    let mut time_sig = None;
    let mut tones: Vec<ToneData> = vec![];
    let mut marks = Marks::default();
    let mut mlen: Option<Beats> = None;
    let zero: Beats = Ratio::from_integer(0);
    for (vi, voice) in meas.children().filter(|c| c.has_tag_name("voice")).enumerate() {
        let mut pos = zero;
        // <location> can move back (e.g. chord symbols): the measure ends at the furthest point
        let mut max_pos = zero;
        let mut tuplet: Beats = Ratio::from_integer(1);
        // a <Fermata> comes just before the chord or rest it holds
        let mut fermata = false;
        for el in voice.children().filter(|c| c.is_element()) {
            match el.tag_name().name() {
                // written after the chord it follows, where the next one starts
                "Breath" => marks.breaths.push(pos),
                "Fermata" => fermata = true,
                "Dynamic" => {
                    if let Some(db) = child_text(el, "subtype").and_then(dynamic_db) {
                        marks.dynamics.push((pos, db));
                    }
                }
                // subtype 0 / 2: crescendo (hairpin / "cresc." line), 1 / 3: diminuendo
                "Spanner" if el.attribute("type") == Some("HairPin") => {
                    if let Some(h) = child(el, "HairPin") {
                        let dim = matches!(child_text(h, "subtype").map(str::trim), Some("1" | "3"));
                        marks.hairpins.push((pos, if dim { Hairpin::Dim } else { Hairpin::Cresc }));
                    } else if child(el, "prev").is_some() {
                        marks.hairpins.push((pos, Hairpin::End));
                    }
                }
                // rit./accel. line: the start names the change, the end comes back with <prev>
                "Spanner" if el.attribute("type") == Some("GradualTempoChange") => {
                    if let Some(g) = child(el, "GradualTempoChange") {
                        let kind = child_text(g, "tempoChangeType").unwrap_or("");
                        let faster = ["accelerando", "stringendo", "precipitando"].iter().any(|k| kind.contains(k));
                        marks.tempo.push((pos, if faster { TempoWord::Faster } else { TempoWord::Slower }));
                    } else if child(el, "prev").is_some() {
                        marks.tempo.push((pos, TempoWord::End));
                    }
                }
                "StaffText" | "SystemText" | "Expression" => {
                    if let Some(w) = child(el, "text").map(all_text).as_deref().and_then(TempoWord::from_text) {
                        marks.tempo.push((pos, w));
                    }
                }
                "TimeSig" => {
                    let n = child_text(el, "sigN").and_then(|s| s.parse().ok()).unwrap_or(4);
                    let d = child_text(el, "sigD").and_then(|s| s.parse().ok()).unwrap_or(4);
                    time_sig.get_or_insert((n, d));
                }
                "Tempo" => {
                    if let Some(t) = child_text(el, "tempo").and_then(|s| s.parse::<f64>().ok()) {
                        tempos.push((pos, t));
                    }
                    if child(el, "text").map(all_text).as_deref().and_then(TempoWord::from_text) == Some(TempoWord::ATempo) {
                        marks.tempo.push((pos, TempoWord::ATempo));
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
                    if std::mem::take(&mut fermata) {
                        marks.fermatas.push((pos, ln));
                    }
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
                        for n in note_els.iter().filter(|n| child_text(**n, "play") != Some("0")) {
                            if let Some(pitch) = child_text(*n, "pitch").and_then(|p| p.parse::<i32>().ok()) {
                                let tie_prev = n.children().any(|s| s.has_tag_name("Spanner") && s.attribute("type") == Some("Tie") && child(s, "prev").is_some());
                                tones.push(ToneData { offset: pos, dur: ln, pitch, tie_prev });
                            }
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
    MeasureData { len: mlen.unwrap_or(zero), notes, tones, tempos, structure: st, time_sig, number: None, marks }
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

    /// (start in quarters, printed number, time signature) of a played measure
    type BarRow<'a> = (i64, Option<&'a str>, Option<(i32, i32)>);

    fn bars(n: usize) -> Vec<Structure> {
        vec![Structure::default(); n]
    }
    fn idx(o: &[(usize, u32)]) -> Vec<usize> {
        o.iter().map(|(i, _)| i + 1).collect() // 1-based bar numbers
    }

    #[test]
    fn bars_follow_repeats_and_time_signature_changes() {
        let q = "<Rest><durationType>quarter</durationType></Rest>";
        let xml = format!(
            r#"<museScore><Score><Staff id="1">
            <Measure len="1/4"><irregular>1</irregular><voice><TimeSig><sigN>2</sigN><sigD>4</sigD></TimeSig>{q}</voice></Measure>
            <Measure><startRepeat/><voice>{q}{q}</voice></Measure>
            <Measure><endRepeat>2</endRepeat><voice>{q}{q}</voice></Measure>
            <Measure><voice><TimeSig><sigN>4</sigN><sigD>4</sigD></TimeSig>{q}{q}{q}{q}</voice></Measure>
            </Staff></Score></museScore>"#
        );
        let s = parse_mscx(&xml).unwrap();
        assert!(s.warnings.is_empty(), "{:?}", s.warnings);
        assert_eq!(s.time_sig, (2, 4));
        assert_eq!(s.time_sigs, vec![(2, 4), (4, 4)]);
        let s2 = parse_mscx(&xml.replace("<sigN>4</sigN><sigD>4</sigD>", "<sigN>2</sigN><sigD>4</sigD>")).unwrap();
        assert_eq!(s2.time_sigs, vec![(2, 4)], "each signature once");
        let got: Vec<BarRow> =
            s.bars.iter().map(|b| (b.start.to_integer(), b.number.as_deref(), b.time_sig)).collect();
        assert_eq!(
            got,
            vec![(0, None, Some((2, 4))), (1, Some("1"), None), (3, Some("2"), None), (5, Some("1"), None), (7, Some("2"), None), (9, Some("3"), Some((4, 4)))]
        );
    }

    #[test]
    fn performance_marks() {
        // shapes from MuseScore 4.7: dynamic and fermata before their chord, hairpin and
        // rit. lines with their end where they stop, breath after its chord
        let c = |p: i32| format!("<Chord><durationType>quarter</durationType><Note><pitch>{p}</pitch></Note></Chord>");
        let xml = format!(
            r#"<museScore><Score><Staff id="1">
            <Measure><voice><TimeSig><sigN>4</sigN><sigD>4</sigD></TimeSig><Dynamic><subtype>p</subtype><velocity>49</velocity></Dynamic>
              <Spanner type="HairPin"><HairPin><subtype>0</subtype></HairPin><next><location><measures>1</measures></location></next></Spanner>
              {}{}{}{}<Breath><symbol>breathMarkComma</symbol></Breath></voice></Measure>
            <Measure><voice><Spanner type="HairPin"><prev><location><measures>-1</measures></location></prev></Spanner>
              <Dynamic><subtype>f</subtype></Dynamic><Spanner type="GradualTempoChange"><GradualTempoChange><tempoChangeType>ritardando</tempoChangeType><beginText>rit.</beginText></GradualTempoChange><next><location><fractions>1/2</fractions></location></next></Spanner>
              {}{}<Spanner type="GradualTempoChange"><prev><location><fractions>-1/2</fractions></location></prev></Spanner>
              <Spanner type="HairPin"><HairPin><subtype>3</subtype></HairPin><next><location><fractions>1/4</fractions></location></next></Spanner>{}
              <Spanner type="HairPin"><prev><location><fractions>-1/4</fractions></location></prev></Spanner><Fermata><subtype>fermataAbove</subtype></Fermata>{}</voice></Measure>
            </Staff></Score></museScore>"#,
            c(60), c(62), c(64), c(65), c(67), c(65), c(64), c(62)
        );
        let s = parse_mscx(&xml).unwrap();
        let m = &s.parts[0].marks;
        let b = |n: i64| Ratio::from_integer(n);
        assert_eq!(m.dynamics, vec![(b(0), -8.0), (b(4), 3.0)]);
        assert_eq!(m.hairpins, vec![(b(0), Hairpin::Cresc), (b(4), Hairpin::End), (b(6), Hairpin::Dim), (b(7), Hairpin::End)]);
        assert_eq!(m.breaths, vec![b(4)]);
        assert_eq!(m.fermatas, vec![(b(7), b(1))]);
        assert_eq!(m.tempo, vec![(b(4), TempoWord::Slower), (b(6), TempoWord::End)]);
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
