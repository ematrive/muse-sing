//! The performance markings of the score turned into sound, and transposition: applied the same
//! way to what is sung and played (`sing.rs`) and to what the roll shows (`score_roll`), so the
//! cursor stays on the music.
//!
//! - fermatas and rit./accel. change the tempo (note ids and ticks don't move, so the edits stay
//!   where they are);
//! - a breath shortens the note before it, and the singer takes a breath (AP) in the gap;
//! - dynamics and hairpins become a volume curve per part (no new render).

use std::collections::{HashMap, HashSet};

use num_rational::Ratio;

use crate::ds::phonemize::PNote;
use crate::ds::time::TimeAxis;
use crate::score::{Beats, Hairpin, Part, Score, TempoWord};

/// Tempo during a held note: 55% (the note lasts about 1.8 times).
const FERMATA: f64 = 0.55;
const RIT: f64 = 0.75;
const ACCEL: f64 = 1.2;
/// A breath takes at most an eighth, and at most a quarter of the note.
const BREATH_MAX: (i64, i64) = (1, 2);
/// A shorter gap stays silent; a longer one gets a breath only in its last part.
const BREATH_MIN_MS: f64 = 120.0;
const BREATH_MAX_MS: f64 = 450.0;
/// Lyric of the breath notes added to a sung part: the AP phoneme of DiffSinger.
pub const BREATH_LYRIC: &str = "br[AP]";
/// Hairpin without a dynamic at its end: ±6 dB.
const HAIRPIN_DB: f32 = 6.0;

pub const MAX_TRANSPOSE: i32 = 6;

/// Notes shortened for a breath, by part.
pub type Breathed = HashMap<String, HashSet<String>>;

/// Transposes, then (when `marks`) applies fermatas, tempo words and breaths. Without `marks`
/// the markings are dropped, so no volume curve either.
pub fn prepare(sc: &mut Score, marks: bool, transpose: i32) -> Breathed {
    let t = transpose.clamp(-MAX_TRANSPOSE, MAX_TRANSPOSE);
    for p in &mut sc.parts {
        p.notes.iter_mut().for_each(|n| n.pitch += t);
        p.tones.iter_mut().for_each(|n| n.pitch += t);
    }
    if !marks {
        sc.parts.iter_mut().for_each(|p| p.marks = Default::default());
        return Breathed::new();
    }
    sc.tempos = tempos(sc);
    sc.parts.iter_mut().map(|p| (p.name.clone(), breathe(p))).filter(|(_, ids)| !ids.is_empty()).collect()
}

fn end_of(sc: &Score) -> Beats {
    let notes = sc.parts.iter().flat_map(|p| p.notes.iter().map(|n| n.start + n.dur).chain(p.tones.iter().map(|n| n.start + n.dur)));
    notes.chain(sc.bars.last().map(|b| b.start)).max().unwrap_or_default()
}

/// The written tempos with the fermatas and rit./accel. of every part (each counted once).
fn tempos(sc: &Score) -> Vec<(Beats, f64)> {
    let zero: Beats = Ratio::from_integer(0);
    let base = |at: Beats| sc.tempos.iter().take_while(|(p, _)| *p <= at).last().or(sc.tempos.first()).map_or(2.0, |t| t.1);

    // fermatas: the union of the held spans of every part
    let mut spans: Vec<(Beats, Beats)> = sc.parts.iter().flat_map(|p| p.marks.fermatas.iter().map(|&(a, d)| (a, a + d))).filter(|(a, b)| b > a).collect();
    spans.sort();
    let mut held: Vec<(Beats, Beats)> = vec![];
    for (a, b) in spans {
        match held.last_mut() {
            Some(h) if a <= h.1 => h.1 = h.1.max(b),
            _ => held.push((a, b)),
        }
    }

    // tempo words: (from, factor) steps
    let mut words: Vec<(Beats, TempoWord)> = sc.parts.iter().flat_map(|p| p.marks.tempo.iter().copied()).collect();
    words.sort_by_key(|a| a.0);
    words.dedup_by(|b, a| a.0 == b.0);
    let end = end_of(sc);
    let changes: Vec<Beats> = sc.tempos.iter().map(|t| t.0).collect();
    let next_after = |at: Beats| -> Beats {
        let w = words.iter().map(|w| w.0).find(|p| *p > at);
        let c = changes.iter().copied().find(|p| *p > at);
        [w, c, Some(end)].into_iter().flatten().min().unwrap_or(end)
    };
    let two_bars_after = |at: Beats| -> Beats {
        let i = sc.bars.iter().rposition(|b| b.start <= at).unwrap_or(0);
        sc.bars.get(i + 2).map_or(end, |b| b.start)
    };
    let mut steps: Vec<(Beats, f64)> = vec![];
    let mut factor = 1.0;
    for &(at, w) in &words {
        match w {
            TempoWord::Slower | TempoWord::Faster => {
                let target = if w == TempoWord::Slower { RIT } else { ACCEL };
                let stop = next_after(at).min(two_bars_after(at));
                // one step per quarter note
                let n = ((stop - at).to_integer().max(1)) as usize;
                for k in 0..n {
                    let f = factor + (target - factor) * (k + 1) as f64 / n as f64;
                    steps.push((at + Ratio::from_integer(k as i64), f));
                }
                factor = target;
                // until a tempo, a new tempo or the end
                let reset = words.iter().find(|x| x.0 > at && x.1 == TempoWord::ATempo).map(|x| x.0);
                let change = changes.iter().copied().find(|p| *p > at);
                if let Some(r) = [reset, change].into_iter().flatten().min() {
                    steps.push((r, 1.0));
                }
            }
            TempoWord::ATempo => {
                factor = 1.0;
                steps.push((at, 1.0));
            }
            TempoWord::End => {}
        }
    }
    steps.sort_by_key(|a| a.0);
    // after a reset, a later ramp starts again from 1
    let word_factor = |at: Beats| steps.iter().take_while(|s| s.0 <= at).last().map_or(1.0, |s| s.1);
    let in_fermata = |at: Beats| held.iter().any(|&(a, b)| a <= at && at < b);

    let mut points: Vec<Beats> = changes.clone();
    points.extend(held.iter().flat_map(|&(a, b)| [a, b]));
    points.extend(steps.iter().map(|s| s.0));
    points.push(zero);
    points.sort();
    points.dedup();
    let mut out: Vec<(Beats, f64)> = vec![];
    for p in points {
        let q = base(p) * word_factor(p) * if in_fermata(p) { FERMATA } else { 1.0 };
        if out.last().is_none_or(|l| (l.1 - q).abs() > 1e-9) {
            out.push((p, q));
        }
    }
    out
}

/// Shortens the sung notes that a breath follows; returns their ids.
fn breathe(p: &mut Part) -> HashSet<String> {
    let mut ids = HashSet::new();
    let (num, den) = BREATH_MAX;
    for &b in &p.marks.breaths {
        let Some(i) = p.notes.iter().position(|n| n.start + n.dur == b) else { continue };
        let n = &p.notes[i];
        // only a note followed straight away by another one: before a rest there is time already
        if !p.notes.get(i + 1).is_some_and(|m| m.start == b) {
            continue;
        }
        let cut = Ratio::new(num, den).min(n.dur / 4);
        p.notes[i].dur -= cut;
        ids.insert(p.notes[i].id.clone());
    }
    ids
}

/// Adds a breath (AP) in the gap after each shortened note, before the next one.
pub fn add_breaths(notes: &mut Vec<PNote>, breathed: &HashSet<String>, axis: &TimeAxis) {
    if breathed.is_empty() {
        return;
    }
    let ms = |t: i64| axis.tick_to_ms(t as f64);
    let mut out = Vec::with_capacity(notes.len() + breathed.len());
    for (i, n) in notes.iter().enumerate() {
        out.push(n.clone());
        if !breathed.contains(&n.id) || n.lyric.starts_with('+') {
            continue;
        }
        let Some(next) = notes.get(i + 1) else { continue };
        let gap = ms(next.position) - ms(n.end());
        if gap < BREATH_MIN_MS {
            continue;
        }
        let start = n.end().max(axis.ms_to_tick(ms(next.position) - BREATH_MAX_MS));
        out.push(PNote { id: format!("{}.respiro", n.id), position: start, duration: next.position - start, tone: n.tone, lyric: BREATH_LYRIC.into() });
    }
    *notes = out;
}

/// Volume of a part from its dynamics and hairpins: (position, dB relative to mf), linear in
/// between; empty when the part has none.
pub fn gain_curve(p: &Part) -> Vec<(Beats, f32)> {
    let m = &p.marks;
    let mut out: Vec<(Beats, f32)> = vec![];
    let mut level = 0.0f32;
    // dynamics and hairpin starts, in order
    let mut events: Vec<(Beats, Option<f32>, Option<Hairpin>)> = m.dynamics.iter().map(|&(a, d)| (a, Some(d), None)).collect();
    events.extend(m.hairpins.iter().filter(|h| h.1 != Hairpin::End).map(|&(a, h)| (a, None, Some(h))));
    events.sort_by(|a, b| a.0.cmp(&b.0).then(a.2.is_some().cmp(&b.2.is_some())));
    for &(at, dynamic, hairpin) in &events {
        if let Some(db) = dynamic {
            if !out.is_empty() {
                out.push((at, level));
            }
            out.push((at, db));
            level = db;
        } else if let Some(h) = hairpin {
            // ends at its end mark, or at the next dynamic or hairpin
            let end_mark = m.hairpins.iter().find(|x| x.0 > at).map(|x| x.0);
            let next_dyn = m.dynamics.iter().find(|x| x.0 > at);
            let end = [end_mark, next_dyn.map(|x| x.0)].into_iter().flatten().min().unwrap_or(at + Ratio::from_integer(4));
            // the dynamic it leads to: at its end, or just after
            let target = next_dyn.filter(|x| x.0 <= end + Ratio::from_integer(1)).map(|x| x.1).unwrap_or(
                level + if h == Hairpin::Cresc { HAIRPIN_DB } else { -HAIRPIN_DB },
            );
            out.push((at, level));
            out.push((end, target.clamp(-22.0, 9.0)));
            level = target.clamp(-22.0, 9.0);
        }
    }
    out
}

/// dB of the curve at `at`: 0 before its first point, the last value after its end.
pub fn gain_at(curve: &[(f64, f32)], at: f64) -> f32 {
    match curve.iter().position(|p| p.0 > at) {
        None => curve.last().map_or(0.0, |p| p.1),
        Some(0) => 0.0,
        Some(i) => {
            let (a, b) = (curve[i - 1], curve[i]);
            if b.0 - a.0 <= 0.0 { b.1 } else { a.1 + (b.1 - a.1) * ((at - a.0) / (b.0 - a.0)) as f32 }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::{Bar, Marks, Note, Tone};

    fn b(n: i64) -> Beats {
        Ratio::from_integer(n)
    }

    fn part(name: &str, notes: &[(i64, i64, i32)], marks: Marks) -> Part {
        Part {
            name: name.into(),
            notes: notes.iter().map(|&(s, d, p)| Note { id: format!("{s}"), start: b(s), dur: b(d), pitch: p, lyric: None }).collect(),
            tones: notes.iter().map(|&(s, d, p)| Tone { start: b(s), dur: b(d), pitch: p }).collect(),
            marks,
        }
    }

    fn score(parts: Vec<Part>, tempos: Vec<(i64, f64)>) -> Score {
        Score {
            title: None,
            time_sig: (4, 4),
            time_sigs: vec![(4, 4)],
            bars: (0..8).map(|i| Bar { start: b(i * 4), number: Some((i + 1).to_string()), time_sig: None }).collect(),
            tempos: tempos.into_iter().map(|(p, q)| (b(p), q)).collect(),
            parts,
            warnings: vec![],
        }
    }

    #[test]
    fn transposes_and_drops_marks_when_off() {
        let m = Marks { breaths: vec![b(2)], fermatas: vec![(b(2), b(2))], ..Default::default() };
        let mut sc = score(vec![part("S", &[(0, 2, 60), (2, 2, 62)], m)], vec![(0, 2.0)]);
        let breathed = prepare(&mut sc, false, 3);
        assert!(breathed.is_empty());
        assert_eq!(sc.parts[0].notes[0].pitch, 63);
        assert_eq!(sc.parts[0].tones[1].pitch, 65);
        assert_eq!(sc.parts[0].notes[0].dur, b(2), "no breath");
        assert_eq!(sc.tempos, vec![(b(0), 2.0)], "no fermata");
        assert_eq!(sc.parts[0].marks, Marks::default());
    }

    #[test]
    fn fermatas_of_all_parts_slow_down_once() {
        // the same fermata on two parts, the base tempo changes inside it
        let m = Marks { fermatas: vec![(b(4), b(2))], ..Default::default() };
        let mut sc = score(vec![part("S", &[(0, 8, 60)], m.clone()), part("A", &[(0, 8, 57)], m)], vec![(0, 2.0), (5, 1.0)]);
        prepare(&mut sc, true, 0);
        assert_eq!(sc.tempos, vec![(b(0), 2.0), (b(4), 2.0 * FERMATA), (b(5), FERMATA), (b(6), 1.0)], "restores the tempo in force at its end");
    }

    #[test]
    fn rit_ramps_until_a_tempo() {
        let m = Marks { tempo: vec![(b(4), TempoWord::Slower), (b(16), TempoWord::ATempo)], ..Default::default() };
        let mut sc = score(vec![part("S", &[(0, 20, 60)], m)], vec![(0, 2.0)]);
        prepare(&mut sc, true, 0);
        let at = |x: i64| sc.tempos.iter().take_while(|t| t.0 <= b(x)).last().unwrap().1;
        assert_eq!(at(3), 2.0);
        assert!(at(4) < 2.0 && at(4) > at(8), "slows down step by step");
        assert!((at(11) - 2.0 * RIT).abs() < 1e-9, "reaches the target in two bars");
        assert!((at(14) - 2.0 * RIT).abs() < 1e-9, "and stays there");
        assert_eq!(at(16), 2.0, "a tempo");
        assert_eq!(TempoWord::from_text("poco rit."), Some(TempoWord::Slower));
        assert_eq!(TempoWord::from_text("Rall. molto"), Some(TempoWord::Slower));
        assert_eq!(TempoWord::from_text("ritornello"), None);
        assert_eq!(TempoWord::from_text("a tempo"), Some(TempoWord::ATempo));
        assert_eq!(TempoWord::from_text("accel."), Some(TempoWord::Faster));
    }

    #[test]
    fn breath_shortens_the_note_and_adds_ap() {
        // breath after the first note; the second note ends before a rest: nothing to do there
        let m = Marks { breaths: vec![b(2), b(6)], ..Default::default() };
        let mut sc = score(vec![part("S", &[(0, 2, 60), (2, 4, 62), (8, 1, 64)], m)], vec![(0, 1.0)]);
        let breathed = prepare(&mut sc, true, 0);
        let ids = &breathed["S"];
        assert_eq!(ids.len(), 1);
        assert_eq!(sc.parts[0].notes[0].dur, Ratio::new(3, 2), "an eighth less");
        assert_eq!(sc.parts[0].notes[1].dur, b(4));

        let axis = TimeAxis::new(&[(0, 60.0)], 480);
        let mut notes: Vec<PNote> = sc.parts[0]
            .notes
            .iter()
            .map(|n| PNote { id: n.id.clone(), position: (n.start * 480).to_integer(), duration: (n.dur * 480).to_integer(), tone: n.pitch, lyric: "la".into() })
            .collect();
        add_breaths(&mut notes, ids, &axis);
        assert_eq!(notes.len(), 4);
        let ap = &notes[1];
        assert_eq!(ap.lyric, BREATH_LYRIC);
        // the gap is 500 ms at 60 bpm: the breath takes its last 450 ms
        assert_eq!((ap.position, ap.position + ap.duration), (744, 960));
    }

    #[test]
    fn dynamics_and_hairpins_make_a_curve() {
        // p, crescendo to f, then a diminuendo with no dynamic after it
        let m = Marks {
            dynamics: vec![(b(0), -8.0), (b(8), 3.0)],
            hairpins: vec![(b(4), Hairpin::Cresc), (b(8), Hairpin::End), (b(12), Hairpin::Dim), (b(14), Hairpin::End)],
            ..Default::default()
        };
        let curve: Vec<(f64, f32)> = gain_curve(&part("S", &[(0, 16, 60)], m)).into_iter().map(|(a, d)| (*a.numer() as f64 / *a.denom() as f64, d)).collect();
        assert_eq!(gain_at(&curve, 2.0), -8.0);
        assert!((gain_at(&curve, 6.0) - (-2.5)).abs() < 1e-4, "halfway through the crescendo");
        assert_eq!(gain_at(&curve, 10.0), 3.0);
        assert_eq!(gain_at(&curve, 14.0), -3.0, "6 dB down");
        assert_eq!(gain_at(&curve, 20.0), -3.0);
        assert!(gain_curve(&part("S", &[(0, 4, 60)], Marks::default())).is_empty());
    }
}
