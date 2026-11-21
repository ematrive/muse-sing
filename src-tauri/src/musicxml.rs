//! Reads MusicXML (`score-partwise`, plain `.musicxml`/`.xml` or compressed `.mxl`) into the same
//! written measures as the MuseScore reader, then lays them out with `score::assemble`.
//! MusicXML is what other notation programs and optical music recognition (PDF scans) export.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Read;

use num_rational::Ratio;
use roxmltree::{Document, Node};

use crate::error::{Error, Result};
use crate::score::{assemble, dynamic_db, Beats, Hairpin, Lyric, Marks, MeasureData, NoteData, Score, StaffData, Structure, Syllabic, TempoWord, ToneData};

fn child<'a, 'i>(n: Node<'a, 'i>, tag: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.has_tag_name(tag))
}

fn child_text<'a>(n: Node<'a, '_>, tag: &str) -> Option<&'a str> {
    child(n, tag).and_then(|c| c.text()).map(str::trim)
}

/// The score file inside a compressed MusicXML (`.mxl`): the rootfile named by
/// META-INF/container.xml, else the first .musicxml/.xml that isn't in META-INF.
pub fn mxl_root(bytes: &[u8]) -> Result<String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| Error::Format(format!("MusicXML compresso non valido: {e}")))?;
    let mut name = None;
    if let Ok(mut f) = zip.by_name("META-INF/container.xml") {
        let mut text = String::new();
        f.read_to_string(&mut text)?;
        if let Ok(doc) = Document::parse(&text) {
            name = doc.descendants().find(|d| d.has_tag_name("rootfile")).and_then(|r| r.attribute("full-path")).map(str::to_string);
        }
    }
    let name = name
        .or_else(|| {
            (0..zip.len())
                .filter_map(|i| zip.name_for_index(i).map(str::to_string))
                .find(|n| !n.starts_with("META-INF/") && (n.ends_with(".musicxml") || n.ends_with(".xml")))
        })
        .ok_or_else(|| Error::Format("nessuna partitura dentro il file .mxl".into()))?;
    let mut xml = String::new();
    zip.by_name(&name).map_err(|e| Error::Format(e.to_string()))?.read_to_string(&mut xml)?;
    Ok(xml)
}

fn step_semitone(step: &str) -> Option<i32> {
    Some(match step {
        "C" => 0,
        "D" => 2,
        "E" => 4,
        "F" => 5,
        "G" => 7,
        "A" => 9,
        "B" => 11,
        _ => return None,
    })
}

fn note_type_len(t: &str) -> Option<Beats> {
    Some(match t {
        "breve" => Ratio::from_integer(8),
        "whole" => Ratio::from_integer(4),
        "half" => Ratio::from_integer(2),
        "quarter" => Ratio::from_integer(1),
        "eighth" => Ratio::new(1, 2),
        "16th" => Ratio::new(1, 4),
        "32nd" => Ratio::new(1, 8),
        _ => return None,
    })
}

/// Quarter notes per second of a `<metronome>`.
fn metronome(m: Node) -> Option<f64> {
    let unit = note_type_len(child_text(m, "beat-unit")?)?;
    let dots = m.children().filter(|c| c.has_tag_name("beat-unit-dot")).count();
    let (mut len, mut add) = (unit, unit);
    for _ in 0..dots {
        add /= 2;
        len += add;
    }
    let per_min: f64 = child_text(m, "per-minute")?.parse().ok()?;
    Some(per_min * (*len.numer() as f64 / *len.denom() as f64) / 60.0)
}

/// Lyric number → verse index (0-based): numbers as written, other names in order of appearance.
#[derive(Default)]
struct Verses(HashMap<String, i32>);

impl Verses {
    fn index(&mut self, number: &str) -> i32 {
        if let Ok(n) = number.trim().parse::<i32>() {
            return (n - 1).max(0);
        }
        let next = 100 + self.0.len() as i32;
        *self.0.entry(number.to_string()).or_insert(next)
    }
}

fn lyric_of(l: Node) -> Option<Lyric> {
    // several <text> joined by <elision>: one note sings two syllables (elision)
    let texts: Vec<&str> = l.children().filter(|c| c.has_tag_name("text")).map(|t| t.text().unwrap_or("")).collect();
    if texts.is_empty() {
        return None; // only <extend/>: the melisma line of the previous syllable
    }
    let syllabic = match child_text(l, "syllabic") {
        Some("begin") => Syllabic::Begin,
        Some("middle") => Syllabic::Middle,
        Some("end") => Syllabic::End,
        _ => Syllabic::Single,
    };
    Some(Lyric { text: texts.join("‿"), syllabic })
}

fn divs(n: Node, tag: &str, divisions: i64) -> Beats {
    let d: f64 = child_text(n, tag).and_then(|t| t.parse().ok()).unwrap_or(0.0);
    // durations are integers in practice; round odd exporters to 1/1000 of a division
    Ratio::new((d * 1000.0).round() as i64, divisions.max(1) * 1000)
}

pub fn parse_musicxml(xml: &str) -> Result<Score> {
    // MusicXML files start with a DOCTYPE (external, never fetched)
    let opts = roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
    let doc = Document::parse_with_options(xml, opts).map_err(|e| Error::Format(format!("XML non valido: {e}")))?;
    let root = doc.root_element();
    match root.tag_name().name() {
        "score-partwise" => {}
        "score-timewise" => return Err(Error::Format("MusicXML «timewise» non supportato: esporta la partitura come «partwise»".into())),
        _ => return Err(Error::Format("non è un file MusicXML".into())),
    }
    let title = child(root, "work")
        .and_then(|w| child_text(w, "work-title"))
        .or_else(|| child_text(root, "movement-title"))
        .or_else(|| {
            root.children()
                .filter(|c| c.has_tag_name("credit"))
                .find(|c| child_text(*c, "credit-type") == Some("title"))
                .and_then(|c| child_text(c, "credit-words"))
        })
        .filter(|t| !t.is_empty())
        .map(str::to_string);

    let mut part_names: HashMap<String, String> = HashMap::new();
    if let Some(pl) = child(root, "part-list") {
        for sp in pl.children().filter(|c| c.has_tag_name("score-part")) {
            let id = sp.attribute("id").unwrap_or("").to_string();
            let name = child_text(sp, "part-name")
                .filter(|n| !n.is_empty() && !n.starts_with("MusicXML Part"))
                .or_else(|| child_text(sp, "part-abbreviation").filter(|n| !n.is_empty()))
                .unwrap_or(&id)
                .to_string();
            part_names.insert(id, name);
        }
    }

    let mut lyrics_other_voice = BTreeSet::new();
    let mut warnings = vec![];
    let mut staves: Vec<StaffData> = vec![];
    for part in root.children().filter(|c| c.has_tag_name("part")) {
        let pid = part.attribute("id").unwrap_or("").to_string();
        let name = part_names.get(&pid).cloned().unwrap_or_else(|| pid.clone());
        let (measures, n_staves) = read_part(part, &name, &mut lyrics_other_voice, &mut warnings);
        for (s, ms) in measures.into_iter().enumerate().take(n_staves) {
            let staff_name = if n_staves > 1 && s > 0 { format!("{name} ({})", s + 1) } else { name.clone() };
            staves.push(StaffData { name: staff_name, id: format!("{pid}.{}", s + 1), measures: ms });
        }
    }
    link_jumps(&mut staves);
    Ok(assemble(title, staves, lyrics_other_voice, warnings))
}

/// A jump whose target label doesn't exist (exporters name segni and codas freely) goes to the
/// only segno / coda of that kind, if there is one.
fn link_jumps(staves: &mut [StaffData]) {
    let labels: BTreeSet<String> = staves.iter().flat_map(|s| &s.measures).flat_map(|m| m.structure.markers.iter().cloned()).collect();
    let fix = |l: &mut String| {
        if l.is_empty() || l == "start" || l == "end" || labels.contains(l.as_str()) {
            return;
        }
        let kind = l.split(':').next().unwrap_or("").to_string();
        let same: Vec<&String> = labels.iter().filter(|m| m.split(':').next() == Some(kind.as_str())).collect();
        if let [only] = same[..] {
            *l = only.clone();
        }
    };
    for m in staves.iter_mut().flat_map(|s| s.measures.iter_mut()) {
        if let Some((to, until, cont)) = &mut m.structure.jump {
            fix(to);
            fix(until);
            fix(cont);
        }
    }
}

/// Measures of each staff of a part (index = staff - 1) and the number of staves.
fn read_part(
    part: Node,
    name: &str,
    lyrics_other_voice: &mut BTreeSet<String>,
    warnings: &mut Vec<String>,
) -> (Vec<Vec<MeasureData>>, usize) {
    let zero: Beats = Ratio::from_integer(0);
    // the first voice of each staff (the one whose notes are sung): the lowest voice number used there
    let mut first_voice: BTreeMap<usize, i32> = BTreeMap::new();
    let mut n_staves = 1usize;
    for n in part.descendants().filter(|d| d.has_tag_name("note")) {
        let staff: usize = child_text(n, "staff").and_then(|s| s.parse().ok()).unwrap_or(1);
        let voice: i32 = child_text(n, "voice").and_then(|s| s.parse().ok()).unwrap_or(1);
        let e = first_voice.entry(staff).or_insert(voice);
        *e = (*e).min(voice);
        n_staves = n_staves.max(staff);
    }
    for s in part.descendants().filter(|d| d.has_tag_name("staves")) {
        n_staves = n_staves.max(s.text().and_then(|t| t.trim().parse().ok()).unwrap_or(1));
    }

    let mut out: Vec<Vec<MeasureData>> = (0..n_staves).map(|_| vec![]).collect();
    let mut divisions: i64 = 1;
    let mut sig: (i32, i32) = (4, 4);
    let mut verses = Verses::default();
    let mut open_ending: Option<(usize, Vec<u32>)> = None;
    let mut has_fine = false;
    let mut has_tocoda = false;
    let mut pending_jumps: Vec<(usize, String)> = vec![]; // (measure, text of the jump) to complete

    for (mi, meas) in part.children().filter(|c| c.has_tag_name("measure")).enumerate() {
        let mut st = Structure::default();
        let mut notes: Vec<Vec<NoteData>> = (0..n_staves).map(|_| vec![]).collect();
        let mut tones: Vec<Vec<ToneData>> = (0..n_staves).map(|_| vec![]).collect();
        let mut marks: Vec<Marks> = (0..n_staves).map(|_| Marks::default()).collect();
        let mut tempos: Vec<(Beats, f64)> = vec![];
        let mut time_sig = None;
        // a pickup or a split measure (implicit) is left out of the count
        let number = meas.attribute("number").filter(|_| meas.attribute("implicit") != Some("yes")).map(|n| n.trim().to_string());
        let mut pos = zero;
        let mut max_pos = zero;
        // last note of each staff in this measure, for <chord/> notes
        let mut last: Vec<Option<usize>> = vec![None; n_staves];
        for el in meas.children().filter(|c| c.is_element()) {
            match el.tag_name().name() {
                "attributes" => {
                    if let Some(d) = child_text(el, "divisions").and_then(|d| d.parse::<f64>().ok()) {
                        divisions = d.round().max(1.0) as i64;
                    }
                    if let Some(t) = child(el, "time") {
                        let n: i32 = child_text(t, "beats").map(|b| b.split('+').filter_map(|x| x.trim().parse::<i32>().ok()).sum()).unwrap_or(4);
                        let d: i32 = child_text(t, "beat-type").and_then(|b| b.parse().ok()).unwrap_or(4);
                        sig = (n, d);
                        time_sig.get_or_insert(sig);
                    }
                }
                "backup" => pos -= divs(el, "duration", divisions),
                "forward" => {
                    pos += divs(el, "duration", divisions);
                    max_pos = max_pos.max(pos);
                }
                "direction" | "sound" => {
                    let sound = if el.has_tag_name("sound") { Some(el) } else { child(el, "sound") };
                    let types: Vec<Node> = el.children().filter(|c| c.has_tag_name("direction-type")).flat_map(|d| d.children()).filter(|c| c.is_element()).collect();
                    let words: String = types.iter().filter(|t| t.has_tag_name("words")).filter_map(|t| t.text()).collect::<Vec<_>>().join(" ").to_lowercase();
                    let tempo = sound
                        .and_then(|s| s.attribute("tempo"))
                        .and_then(|t| t.parse::<f64>().ok())
                        .map(|t| t / 60.0)
                        .or_else(|| types.iter().find(|t| t.has_tag_name("metronome")).and_then(|m| metronome(*m)));
                    if let Some(t) = tempo.filter(|t| *t > 0.0) {
                        tempos.push((pos, t));
                    }
                    // performance markings, on the staff the direction belongs to
                    let staff = child_text(el, "staff").and_then(|s| s.parse::<usize>().ok()).unwrap_or(1).clamp(1, n_staves) - 1;
                    let mk = &mut marks[staff];
                    for t in &types {
                        if t.has_tag_name("dynamics") {
                            if let Some(db) = t.children().filter(|c| c.is_element()).find_map(|c| dynamic_db(c.tag_name().name())) {
                                mk.dynamics.push((pos, db));
                            }
                        } else if t.has_tag_name("wedge") {
                            match t.attribute("type") {
                                Some("crescendo") => mk.hairpins.push((pos, Hairpin::Cresc)),
                                Some("diminuendo") => mk.hairpins.push((pos, Hairpin::Dim)),
                                Some("stop") => mk.hairpins.push((pos, Hairpin::End)),
                                _ => {}
                            }
                        }
                    }
                    if tempo.is_none()
                        && let Some(w) = TempoWord::from_text(&words)
                    {
                        mk.tempo.push((pos, w));
                    } else if tempo.is_some() && TempoWord::from_text(&words) == Some(TempoWord::ATempo) {
                        mk.tempo.push((pos, TempoWord::ATempo));
                    }
                    let attr = |k: &str| sound.and_then(|s| s.attribute(k)).map(str::to_string);
                    if let Some(v) = attr("segno") {
                        st.markers.push(format!("segno:{v}"));
                    } else if types.iter().any(|t| t.has_tag_name("segno")) {
                        st.markers.push("segno:".into());
                    }
                    if let Some(v) = attr("coda") {
                        st.markers.push(format!("coda:{v}"));
                    } else if types.iter().any(|t| t.has_tag_name("coda")) && attr("tocoda").is_none() {
                        st.markers.push("coda:".into());
                    }
                    if let Some(v) = attr("tocoda") {
                        st.markers.push(format!("tocoda:{v}"));
                        has_tocoda = true;
                    }
                    if attr("fine").is_some() || (words.trim() == "fine" && sound.is_none()) {
                        st.markers.push("fine".into());
                        has_fine = true;
                    }
                    if attr("dacapo").as_deref() == Some("yes") {
                        st.jump = Some(("start".into(), String::new(), String::new()));
                        pending_jumps.push((mi, words.clone()));
                    } else if let Some(v) = attr("dalsegno") {
                        st.jump = Some((format!("segno:{v}"), String::new(), String::new()));
                        pending_jumps.push((mi, words.clone()));
                    }
                }
                "barline" => {
                    if let Some(r) = child(el, "repeat") {
                        match r.attribute("direction") {
                            Some("forward") => st.start_repeat = true,
                            Some("backward") => st.end_repeat = r.attribute("times").and_then(|t| t.parse().ok()).unwrap_or(2),
                            _ => {}
                        }
                    }
                    if let Some(e) = child(el, "ending") {
                        let numbers: Vec<u32> = e.attribute("number").unwrap_or("").split([',', ' ']).filter_map(|x| x.trim().parse().ok()).collect();
                        match e.attribute("type") {
                            Some("start") => open_ending = Some((mi, numbers)),
                            Some("stop" | "discontinue") => {
                                if let Some((start, nums)) = open_ending.take() {
                                    let span = mi - start + 1;
                                    let nums = if nums.is_empty() { numbers } else { nums };
                                    if start == mi {
                                        st.volta = Some((nums, span));
                                    } else if let Some(m) = out[0].get_mut(start) {
                                        m.structure.volta = Some((nums, span));
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                "note" => {
                    let staff = child_text(el, "staff").and_then(|s| s.parse::<usize>().ok()).unwrap_or(1).clamp(1, n_staves) - 1;
                    let voice: i32 = child_text(el, "voice").and_then(|s| s.parse().ok()).unwrap_or(1);
                    let first = first_voice.get(&(staff + 1)).copied().unwrap_or(voice) == voice;
                    if child(el, "grace").is_some() {
                        continue; // grace notes take no time
                    }
                    let is_chord = child(el, "chord").is_some();
                    let dur = divs(el, "duration", divisions);
                    let start = if is_chord { (pos - dur).max(zero) } else { pos };
                    let lyrics: BTreeMap<i32, Lyric> = el
                        .children()
                        .filter(|c| c.has_tag_name("lyric"))
                        .filter_map(|l| lyric_of(l).map(|ly| (verses.index(l.attribute("number").unwrap_or("1")), ly)))
                        .collect();
                    if !first && !lyrics.is_empty() {
                        lyrics_other_voice.insert(name.to_string());
                    }
                    let pitch = child(el, "pitch").and_then(|p| {
                        let step = step_semitone(child_text(p, "step")?)?;
                        let alter: f64 = child_text(p, "alter").and_then(|a| a.parse().ok()).unwrap_or(0.0);
                        let octave: i32 = child_text(p, "octave")?.parse().ok()?;
                        Some((octave + 1) * 12 + step + alter.round() as i32)
                    });
                    let cue = child(el, "cue").is_some();
                    let tie_prev = el.children().any(|c| c.has_tag_name("tie") && c.attribute("type") == Some("stop"))
                        || child(el, "notations").is_some_and(|n| n.children().any(|c| c.has_tag_name("tied") && c.attribute("type") == Some("stop")));
                    if !cue && let Some(pitch) = pitch {
                        tones[staff].push(ToneData { offset: start, dur, pitch, tie_prev });
                    }
                    if !cue && let Some(nt) = child(el, "notations") {
                        if child(nt, "fermata").is_some() {
                            marks[staff].fermatas.push((start, dur));
                        }
                        if child(nt, "articulations").is_some_and(|a| child(a, "breath-mark").is_some()) {
                            marks[staff].breaths.push(start + dur);
                        }
                    }
                    if first && !cue && let Some(pitch) = pitch {
                        match (is_chord, last[staff]) {
                            // another note of the same chord: the top one is sung, as in MuseScore
                            (true, Some(k)) => {
                                let prev = &mut notes[staff][k];
                                prev.pitch = prev.pitch.max(pitch);
                                prev.tie_prev |= tie_prev;
                                for (v, l) in lyrics {
                                    prev.lyrics.entry(v).or_insert(l);
                                }
                            }
                            _ => {
                                notes[staff].push(NoteData { offset: start, dur, pitch, tie_prev, lyrics });
                                last[staff] = Some(notes[staff].len() - 1);
                            }
                        }
                    }
                    if !is_chord {
                        pos += dur;
                        max_pos = max_pos.max(pos);
                    }
                }
                _ => {}
            }
        }
        let nominal: Beats = Ratio::new(sig.0 as i64 * 4, sig.1.max(1) as i64);
        let len = if max_pos > zero { max_pos } else { nominal };
        for (s, ((staff_notes, staff_tones), staff_marks)) in notes.into_iter().zip(tones).zip(marks).enumerate() {
            out[s].push(MeasureData {
                len,
                notes: staff_notes,
                tones: staff_tones,
                tempos: tempos.clone(),
                structure: st.clone(),
                time_sig,
                number: number.clone(),
                marks: staff_marks,
            });
        }
    }
    if open_ending.is_some() {
        warnings.push(format!("{name}: una volta (finale 1., 2.…) non è chiusa: considerata lunga una battuta."));
        if let Some((start, nums)) = open_ending
            && let Some(m) = out[0].get_mut(start)
        {
            m.structure.volta = Some((nums, 1));
        }
    }
    // D.C. / D.S. "al Fine" or "al Coda": from the words, else from what the score has
    for (mi, words) in pending_jumps {
        let (until, cont) = if words.contains("fine") || (has_fine && !words.contains("coda")) {
            ("fine".to_string(), String::new())
        } else if words.contains("coda") || has_tocoda {
            ("tocoda:".to_string(), "coda:".to_string())
        } else {
            (String::new(), String::new())
        };
        for staff in out.iter_mut() {
            if let Some(Some((_, u, c))) = staff.get_mut(mi).map(|m| &mut m.structure.jump) {
                *u = until.clone();
                *c = cont.clone();
            }
        }
    }
    (out, n_staves)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (start in quarters, printed number, time signature) of a played measure
    type BarRow<'a> = (i64, Option<&'a str>, Option<(i32, i32)>);

    fn note(step: &str, octave: i32, dur: i32, extra: &str) -> String {
        format!("<note><pitch><step>{step}</step><octave>{octave}</octave></pitch><duration>{dur}</duration><voice>1</voice>{extra}</note>")
    }

    fn lyric(n: u32, syl: &str, text: &str) -> String {
        format!(r#"<lyric number="{n}"><syllabic>{syl}</syllabic><text>{text}</text></lyric>"#)
    }

    /// Pickup bar, |: two verses :| with first and second endings, a tie, a chord, a rest.
    fn sample() -> String {
        // the pickup is "implicit": left out of the measure count
        let m = |n: u32, body: &str| {
            let implicit = if n == 0 { r#" implicit="yes""# } else { "" };
            format!(r#"<measure number="{n}"{implicit}>{body}</measure>"#)
        };
        let measures = [
            m(0, &format!(
                r#"<attributes><divisions>2</divisions><time><beats>3</beats><beat-type>4</beat-type></time></attributes>
                <direction><direction-type><metronome><beat-unit>quarter</beat-unit><per-minute>90</per-minute></metronome></direction-type><sound tempo="90"/></direction>
                {}"#,
                note("C", 4, 2, &lyric(1, "single", "O"))
            )),
            m(1, &format!(
                r#"<barline location="left"><repeat direction="forward"/></barline>{}{}{}"#,
                note("D", 4, 2, &(lyric(1, "begin", "san") + &lyric(2, "begin", "glo"))),
                note("E", 4, 2, &(lyric(1, "end", "cta") + &lyric(2, "end", "ri"))),
                note("F", 4, 2, r#"<tie type="start"/>"#),
            )),
            m(2, &format!(
                r#"<barline location="left"><ending number="1" type="start"/></barline>{}{}<note><rest/><duration>2</duration><voice>1</voice></note>
                <barline location="right"><ending number="1" type="stop"/><repeat direction="backward"/></barline>"#,
                note("F", 4, 2, r#"<tie type="stop"/>"#),
                note("G", 4, 2, &lyric(1, "single", "a")),
            )),
            m(3, &format!(
                r#"<barline location="left"><ending number="2" type="start"/></barline>{}{}<barline location="right"><ending number="2" type="discontinue"/></barline>"#,
                note("A", 4, 4, &lyric(1, "begin", "a")),
                note("C", 5, 4, "<chord/>"),
            )),
        ];
        format!(
            r#"<?xml version="1.0"?><!DOCTYPE score-partwise PUBLIC "-//Recordare//DTD MusicXML 4.0 Partwise//EN" "http://www.musicxml.org/dtds/partwise.dtd"><score-partwise version="4.0"><work><work-title>Prova</work-title></work>
            <part-list><score-part id="P1"><part-name>Soprano</part-name></score-part></part-list>
            <part id="P1">{}</part></score-partwise>"#,
            measures.concat()
        )
    }

    #[test]
    fn performance_marks() {
        let dir = |t: &str| format!("<direction><direction-type>{t}</direction-type></direction>");
        let body = [
            r#"<attributes><divisions>1</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>"#.to_string(),
            dir("<dynamics><p/></dynamics>"),
            dir(r#"<wedge type="crescendo"/>"#),
            note("C", 4, 1, ""),
            note("D", 4, 1, ""),
            dir(r#"<wedge type="stop"/>"#),
            dir("<dynamics><f/></dynamics>"),
            dir("<words>poco rit.</words>"),
            note("E", 4, 1, "<notations><articulations><breath-mark/></articulations></notations>"),
            note("F", 4, 1, "<notations><fermata/></notations>"),
        ]
        .concat();
        let xml = format!(
            r#"<score-partwise version="4.0"><part-list><score-part id="P1"><part-name>S</part-name></score-part></part-list><part id="P1"><measure number="1">{body}</measure></part></score-partwise>"#
        );
        let s = parse_musicxml(&xml).unwrap();
        let m = &s.parts[0].marks;
        let b = |n: i64| Ratio::from_integer(n);
        assert_eq!(m.dynamics, vec![(b(0), -8.0), (b(2), 3.0)]);
        assert_eq!(m.hairpins, vec![(b(0), Hairpin::Cresc), (b(2), Hairpin::End)]);
        assert_eq!(m.tempo, vec![(b(2), TempoWord::Slower)]);
        assert_eq!(m.breaths, vec![b(3)]);
        assert_eq!(m.fermatas, vec![(b(3), b(1))]);
    }

    #[test]
    fn reads_notes_repeats_verses_ties() {
        let s = parse_musicxml(&sample()).unwrap();
        assert_eq!(s.title.as_deref(), Some("Prova"));
        assert_eq!(s.time_sig, (3, 4));
        assert_eq!(s.tempos, vec![(Ratio::from_integer(0), 1.5)]);
        let p = &s.parts[0];
        assert_eq!(p.name, "Soprano");
        let got: Vec<(i32, String, String)> = p
            .notes
            .iter()
            .map(|n| (n.pitch, format!("{}", n.start), n.lyric.as_ref().map(|l| l.text.clone()).unwrap_or_default()))
            .collect();
        let e = |p: i32, s: &str, l: &str| (p, s.to_string(), l.to_string());
        assert_eq!(
            got,
            vec![
                e(60, "0", "O"),
                // first pass: verse 1, F tied across the bar into the first ending
                e(62, "1", "san"),
                e(64, "2", "cta"),
                e(65, "3", ""),
                e(67, "5", "a"),
                // second pass: verse 2, then the second ending (top note of the chord)
                e(62, "7", "glo"),
                e(64, "8", "ri"),
                e(65, "9", ""),
                e(72, "10", "a"),
            ]
        );
        assert_eq!(p.notes[3].dur, Ratio::from_integer(2), "tie merged");
        // what an instrument plays: both notes of the chord, ties merged too
        let tones: Vec<(String, i32, String)> = p.tones.iter().map(|t| (format!("{}", t.start), t.pitch, format!("{}", t.dur))).collect();
        let t = |s: &str, p: i32, d: &str| (s.to_string(), p, d.to_string());
        assert!(tones.contains(&t("3", 65, "2")) && tones.contains(&t("9", 65, "1")), "{tones:?}");
        assert!(tones.contains(&t("10", 69, "2")) && tones.contains(&t("10", 72, "2")), "{tones:?}");
        assert_eq!(tones.len(), 10, "{tones:?}");
        assert_eq!(p.notes[0].id, "1.1.0");
        assert_eq!(p.notes[5].id, "2.2.0");
        let bars: Vec<BarRow> =
            s.bars.iter().map(|b| (b.start.to_integer(), b.number.as_deref(), b.time_sig)).collect();
        assert_eq!(bars, vec![(0, None, Some((3, 4))), (1, Some("1"), None), (4, Some("2"), None), (7, Some("1"), None), (10, Some("3"), None)]);
    }

    #[test]
    fn da_capo_al_fine_and_elision() {
        let xml = format!(
            r#"<score-partwise><part-list><score-part id="P1"><part-name>A</part-name></score-part></part-list><part id="P1">
            <measure number="1"><attributes><divisions>1</divisions></attributes>{}</measure>
            <measure number="2">{}<direction><direction-type><words>Fine</words></direction-type><sound fine="yes"/></direction></measure>
            <measure number="3">{}<direction><direction-type><words>D.C. al Fine</words></direction-type><sound dacapo="yes"/></direction></measure>
            </part></score-partwise>"#,
            note("C", 4, 4, r#"<lyric><syllabic>single</syllabic><text>ma</text><elision/><text>e</text></lyric>"#),
            note("D", 4, 4, ""),
            note("E", 4, 4, ""),
        );
        let s = parse_musicxml(&xml).unwrap();
        let pitches: Vec<i32> = s.parts[0].notes.iter().map(|n| n.pitch).collect();
        assert_eq!(pitches, vec![60, 62, 64, 60, 62]);
        assert_eq!(s.parts[0].notes[0].lyric.as_ref().unwrap().text, "ma‿e");
    }

    #[test]
    fn piano_staves_become_parts_and_timewise_is_refused() {
        let xml = r#"<score-partwise><part-list><score-part id="P1"><part-name>Pianoforte</part-name></score-part></part-list><part id="P1">
            <measure number="1"><attributes><divisions>1</divisions><staves>2</staves></attributes>
            <note><pitch><step>E</step><octave>5</octave></pitch><duration>4</duration><voice>1</voice><staff>1</staff></note>
            <backup><duration>4</duration></backup>
            <note><pitch><step>C</step><octave>3</octave></pitch><duration>4</duration><voice>5</voice><staff>2</staff></note>
            </measure></part></score-partwise>"#;
        let s = parse_musicxml(xml).unwrap();
        let names: Vec<&str> = s.parts.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Pianoforte", "Pianoforte (2)"]);
        assert_eq!(s.parts[1].notes[0].pitch, 48);
        assert!(parse_musicxml("<score-timewise/>").is_err());
    }
}
