//! «Canta e controlla»: the singer's recording against the notes of their part.
//!
//! Each note is judged on the middle of its length (after the attack), with the octave nearest
//! to the written one (a man singing the soprano line an octave down is not out of tune).
//! Only the notes fully inside the recorded stretch are judged.

use serde::Serialize;

use crate::pitch::{Track, HOP};

/// A note of the part as rendered: seconds at the practice tempo, pitch after transposition.
#[derive(Debug, Clone, PartialEq)]
pub struct RefNote {
    pub id: String,
    pub start: f64,
    pub end: f64,
    pub pitch: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// within 25 cents
    InTune,
    /// 25 to 50 cents
    Almost,
    /// more than 50 cents
    Off,
    /// no voice where the note is
    Missed,
    /// too short to tell
    NotJudged,
}

#[derive(Debug, Clone, Serialize)]
pub struct NoteCheck {
    pub id: String,
    pub verdict: Verdict,
    /// Sung minus written, octave folded (negative: flat).
    pub cents: Option<f32>,
}

const IN_TUNE: f32 = 25.0;
const ALMOST: f32 = 50.0;
/// The attack is not judged: at least this, or 20% of the note.
const ATTACK: f64 = 0.08;
/// Fewer voiced estimates than this in the judged part: the note can't be judged.
const MIN_FRAMES: usize = 3;
/// Search range for the delay between the recording and the music; never more than this share of
/// a typical note, or a phrase sung one note late would look in tune.
const MAX_LAG: f64 = 0.25;
const MAX_LAG_NOTE: f64 = 0.4;
/// A take that ends this soon after the count-in has nothing to judge.
const MIN_SUNG: f64 = 0.5;

/// The stretch actually sung (score seconds): from `from` until the recording stopped, at most
/// `to`. The recording started `lead` seconds (the count-in) before `from` and lasts `heard`.
/// None when it stopped during the count-in or right after.
pub fn sung_range(from: f64, to: f64, lead: f64, heard: f64) -> Option<(f64, f64)> {
    let end = (from - lead + heard).min(to);
    (end - from >= MIN_SUNG).then_some((from, end))
}

/// Cents from `target`, folded to the nearest octave (−600..600).
fn folded_cents(midi: f32, target: i32) -> f32 {
    let c = (midi - target as f32) * 100.0;
    c - 1200.0 * (c / 1200.0).round()
}

fn judged_span(n: &RefNote) -> (f64, f64) {
    let d = n.end - n.start;
    (n.start + ATTACK.max(0.2 * d), n.end - 0.2 * d)
}

/// The notes inside `range` (score seconds), judged on `track`, whose time 0 is score time `offset`.
pub fn compare(notes: &[RefNote], track: &Track, offset: f64, range: (f64, f64)) -> Vec<NoteCheck> {
    notes
        .iter()
        .filter(|n| n.start >= range.0 - 1e-6 && n.end <= range.1 + 1e-6)
        .map(|n| {
            let (a, b) = judged_span(n);
            let frames: Vec<Option<f32>> = if b > a {
                let (i, j) = (((a - offset) / HOP).ceil().max(0.0) as usize, ((b - offset) / HOP).floor().max(0.0) as usize);
                (i..=j).map(|k| track.midi.get(k).copied().flatten()).collect()
            } else {
                vec![]
            };
            let mut voiced: Vec<f32> = frames.iter().flatten().copied().collect();
            let verdict_of = |c: f32| match c.abs() {
                x if x < IN_TUNE => Verdict::InTune,
                x if x < ALMOST => Verdict::Almost,
                _ => Verdict::Off,
            };
            let (verdict, cents) = if frames.len() < MIN_FRAMES {
                (Verdict::NotJudged, None)
            } else if voiced.len() < MIN_FRAMES.max(frames.len() * 2 / 5) {
                (Verdict::Missed, None)
            } else {
                voiced.sort_by(|x, y| x.total_cmp(y));
                let c = folded_cents(voiced[voiced.len() / 2], n.pitch);
                (verdict_of(c), Some(c))
            };
            NoteCheck { id: n.id.clone(), verdict, cents }
        })
        .collect()
}

/// The delay of the recording (seconds, within ±0.25) where the voice best matches the notes:
/// what the latency compensation of the devices may have missed.
pub fn best_lag(notes: &[RefNote], track: &Track, offset: f64, range: (f64, f64)) -> f64 {
    let inside: Vec<&RefNote> = notes.iter().filter(|n| n.start >= range.0 && n.end <= range.1).collect();
    let score = |lag: f64| -> usize {
        inside
            .iter()
            .map(|n| {
                let (a, b) = (n.start + 0.05, n.end - 0.05);
                let mut t = a;
                let mut hits = 0;
                while t < b {
                    if track.at(t - offset + lag).is_some_and(|m| folded_cents(m, n.pitch).abs() < ALMOST) {
                        hits += 1;
                    }
                    t += HOP;
                }
                hits
            })
            .sum()
    };
    let mut lens: Vec<f64> = inside.iter().map(|n| n.end - n.start).collect();
    lens.sort_by(|a, b| a.total_cmp(b));
    let typical = lens.get(lens.len() / 2).copied().unwrap_or(1.0);
    let steps = (MAX_LAG.min(MAX_LAG_NOTE * typical) / HOP).round() as i64;
    let zero = score(0.0);
    let (mut best, mut best_score) = (0.0, zero);
    for k in -steps..=steps {
        let lag = k as f64 * HOP;
        let s = score(lag);
        // a clear improvement only: otherwise trust the devices
        if s > best_score && s as f64 > zero as f64 * 1.05 + 2.0 {
            (best, best_score) = (lag, s);
        }
    }
    best
}

/// The sung pitch to draw over the notes: (score seconds, MIDI), every 20 ms, each point moved by
/// octaves next to the note it is sung on (or the nearest one).
pub fn sung_line(notes: &[RefNote], track: &Track, offset: f64, range: (f64, f64)) -> Vec<(f64, f32)> {
    let mut out = vec![];
    let mut k = ((range.0 - offset) / HOP).max(0.0) as usize;
    while k < track.midi.len() {
        let t = offset + k as f64 * HOP;
        if t > range.1 {
            break;
        }
        if let Some(m) = track.midi[k] {
            let near = notes
                .iter()
                .min_by(|a, b| {
                    let d = |n: &RefNote| if t < n.start { n.start - t } else if t > n.end { t - n.end } else { 0.0 };
                    d(a).total_cmp(&d(b))
                })
                .map(|n| n.pitch);
            let m = near.map_or(m, |p| p as f32 + folded_cents(m, p) / 100.0);
            out.push((t, m));
        }
        k += 2;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pitch::{analyze, tests::voice};

    const SR: u32 = 44_100;

    /// Notes of 0.6 s; the singer sings them with `cents` off each (None: silent), starting `delay`
    /// seconds late, an octave down when `octave`.
    fn sing(notes: &[RefNote], cents: &[Option<f32>], delay: f64, octave: bool) -> Vec<f32> {
        let end = notes.last().unwrap().end + 1.0;
        let mut x = vec![0f32; (end * SR as f64) as usize];
        for (n, c) in notes.iter().zip(cents) {
            let Some(c) = c else { continue };
            let m = n.pitch as f32 + c / 100.0 - if octave { 12.0 } else { 0.0 };
            let v = voice(m, n.end - n.start, SR, 15.0);
            let at = ((n.start + delay) * SR as f64) as usize;
            for (i, s) in v.iter().enumerate() {
                if let Some(o) = x.get_mut(at + i) {
                    *o += s;
                }
            }
        }
        x
    }

    fn notes() -> Vec<RefNote> {
        [60, 62, 64, 65, 67, 69]
            .iter()
            .enumerate()
            .map(|(i, &p)| RefNote { id: format!("n{i}"), start: 0.5 + i as f64 * 0.6, end: 1.1 + i as f64 * 0.6, pitch: p })
            .collect()
    }

    fn verdicts(cents: &[Option<f32>], delay: f64, octave: bool) -> Vec<Verdict> {
        let ns = notes();
        let track = analyze(&sing(&ns, cents, delay, octave), SR);
        let range = (0.0, 10.0);
        let lag = best_lag(&ns, &track, 0.0, range);
        assert!((lag - delay).abs() <= 0.03 || delay == 0.0 && lag.abs() <= 0.03, "lag {lag} for a delay of {delay}");
        compare(&ns, &track, -lag, range).iter().map(|c| c.verdict).collect()
    }

    #[test]
    fn judges_each_note() {
        let cents = [Some(0.0), Some(-10.0), Some(40.0), Some(-80.0), None, Some(15.0)];
        let expected = vec![Verdict::InTune, Verdict::InTune, Verdict::Almost, Verdict::Off, Verdict::Missed, Verdict::InTune];
        assert_eq!(verdicts(&cents, 0.0, false), expected);
        assert_eq!(verdicts(&cents, 0.0, true), expected, "an octave down is the same note");
        // the devices' latency not fully compensated
        for delay in [0.1, 0.2, -0.1, -0.2] {
            assert_eq!(verdicts(&cents, delay, false), expected, "delay {delay}");
        }
    }

    #[test]
    fn a_phrase_sung_one_note_late_is_not_in_tune() {
        // quick notes of 0.25 s, each sung with the pitch of the one before
        let ns: Vec<RefNote> = [60, 62, 64, 65, 67, 69, 71, 72, 71, 69, 67, 65]
            .iter()
            .enumerate()
            .map(|(i, &p)| RefNote { id: format!("n{i}"), start: 0.5 + i as f64 * 0.25, end: 0.75 + i as f64 * 0.25, pitch: p })
            .collect();
        let late: Vec<RefNote> = ns.iter().enumerate().map(|(i, n)| RefNote { pitch: if i == 0 { 55 } else { ns[i - 1].pitch }, ..n.clone() }).collect();
        let track = analyze(&sing(&late, &[Some(0.0); 12], 0.0, false), SR);
        let lag = best_lag(&ns, &track, 0.0, (0.0, 10.0));
        let got = compare(&ns, &track, -lag, (0.0, 10.0));
        let in_tune = got.iter().filter(|c| c.verdict == Verdict::InTune).count();
        assert!(in_tune <= 2, "lag {lag}: {in_tune} of 12 in tune");
    }

    #[test]
    fn stretch_sung() {
        // count-in of 2 s before 10 s; stopped after 5 s, during the count-in, past the end
        assert_eq!(sung_range(10.0, 30.0, 2.0, 5.0), Some((10.0, 13.0)));
        assert_eq!(sung_range(10.0, 30.0, 2.0, 1.5), None);
        assert_eq!(sung_range(10.0, 30.0, 2.0, 2.2), None);
        assert_eq!(sung_range(10.0, 12.0, 2.0, 9.0), Some((10.0, 12.0)));
    }

    #[test]
    fn only_notes_inside_the_stretch_and_short_ones_not_judged() {
        let mut ns = notes();
        ns.push(RefNote { id: "short".into(), start: 4.2, end: 4.28, pitch: 60 });
        let track = analyze(&sing(&ns, &[Some(0.0); 7], 0.0, false), SR);
        let got = compare(&ns, &track, 0.0, (1.0, 4.5));
        let ids: Vec<&str> = got.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["n1", "n2", "n3", "n4", "n5", "short"], "n0 starts before the stretch");
        assert_eq!(got.last().unwrap().verdict, Verdict::NotJudged);
        let line = sung_line(&ns, &track, 0.0, (0.0, 10.0));
        assert!(line.iter().all(|(t, m)| ns.iter().any(|n| (m - n.pitch as f32).abs() < 0.5 && *t > n.start - 0.1 && *t < n.end + 0.1)));
    }
}
