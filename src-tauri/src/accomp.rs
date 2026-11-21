//! What is played rather than sung, on the same time axis as the sung parts: the accompaniment
//! (the parts without lyrics), the parts the user wants played as a guide, the metronome and the
//! count-in. With the SoundFont (`instruments`) they are real instruments; without it a simple
//! additive organ.

use std::collections::HashMap;
use std::sync::Arc;

use num_rational::Ratio;
use rustysynth::SoundFont;

use crate::ds::time::TimeAxis;
use crate::instruments::{self, Instrument, Line};
use crate::score::{Beats, Score};
use crate::ustx::TICKS;

/// Harmonics (multiple, amplitude) of the organ sound: a soft flue stop.
const HARMONICS: &[(f32, f32)] = &[(1.0, 1.0), (2.0, 0.45), (3.0, 0.2), (4.0, 0.12), (6.0, 0.05)];
const ATTACK_S: f32 = 0.015;
const RELEASE_S: f32 = 0.08;
/// Gain of one note: several voices sum up, the mix scales the total down if it clips.
const NOTE_GAIN: f32 = 0.08;
const ACCOMP_VELOCITY: i32 = 80;
const GUIDE_VELOCITY: i32 = 96;

fn ticks(b: Beats) -> f64 {
    *b.numer() as f64 / *b.denom() as f64 * TICKS as f64
}

fn secs(axis: &TimeAxis, b: Beats) -> f64 {
    axis.tick_to_ms(ticks(b)) / 1000.0
}

/// Every note of the parts without lyrics (all voices and chords), each part with its instrument.
/// Empty if there are none.
pub fn render(
    score: &Score,
    axis: &TimeAxis,
    sr: u32,
    chosen: &HashMap<String, Instrument>,
    sf: Option<&Arc<SoundFont>>,
) -> Vec<f32> {
    let parts: Vec<_> = score.parts.iter().filter(|p| !p.is_sung()).collect();
    let names: Vec<String> = parts.iter().map(|p| p.name.clone()).collect();
    let insts = instruments::choose(&names, chosen);
    let lines: Vec<Line> = parts
        .iter()
        .zip(insts)
        .map(|(p, instrument)| {
            let notes = if p.tones.is_empty() {
                p.notes.iter().map(|n| (secs(axis, n.start), secs(axis, n.start + n.dur), n.pitch)).collect()
            } else {
                p.tones.iter().map(|t| (secs(axis, t.start), secs(axis, t.start + t.dur), t.pitch)).collect()
            };
            Line { instrument, notes, velocity: ACCOMP_VELOCITY }
        })
        .collect();
    play(&lines, sf, sr)
}

/// The melody of a part, played instead of sung (piano with the SoundFont).
pub fn guide(notes: &[(f64, f64, i32)], sr: u32, sf: Option<&Arc<SoundFont>>) -> Vec<f32> {
    let lines = [Line { instrument: Instrument::Pianoforte, notes: notes.to_vec(), velocity: GUIDE_VELOCITY }];
    play(&lines, sf, sr)
}

fn play(lines: &[Line], sf: Option<&Arc<SoundFont>>, sr: u32) -> Vec<f32> {
    if let Some(sf) = sf {
        match instruments::render(sf, lines, sr) {
            Ok(out) => return out,
            Err(e) => crate::log!("strumenti: {e}; uso l'organo semplice"),
        }
    }
    let all: Vec<(f64, f64, i32)> = lines.iter().flat_map(|l| l.notes.iter().copied()).collect();
    organ(&all, sr)
}

/// The simple additive organ (when the SoundFont is not installed).
fn organ(notes: &[(f64, f64, i32)], sr: u32) -> Vec<f32> {
    let notes: Vec<_> = notes.iter().copied().filter(|(a, b, _)| b > a).collect();
    let end = notes.iter().map(|n| n.1).fold(0.0, f64::max);
    if notes.is_empty() {
        return vec![];
    }
    let srf = sr as f32;
    let mut out = vec![0f32; ((end + RELEASE_S as f64 + 0.1) * sr as f64) as usize];
    for (a, b, pitch) in notes {
        let freq = 440.0 * 2f32.powf((pitch as f32 - 69.0) / 12.0);
        let start = (a * sr as f64) as usize;
        let held = ((b - a) * sr as f64) as usize;
        let len = held + (RELEASE_S * srf) as usize;
        // quieter high notes, as an organ voiced for a church
        let gain = NOTE_GAIN * (1.0 - ((pitch as f32 - 60.0) / 60.0).clamp(-0.3, 0.4));
        for i in 0..len.min(out.len().saturating_sub(start)) {
            let t = i as f32 / srf;
            let env = if i < held {
                (t / ATTACK_S).min(1.0)
            } else {
                (1.0 - (i - held) as f32 / (RELEASE_S * srf)).max(0.0) * (held as f32 / srf / ATTACK_S).min(1.0)
            };
            let mut s = 0.0;
            for &(h, amp) in HARMONICS {
                let f = freq * h;
                if f < srf / 2.0 {
                    s += amp * (std::f32::consts::TAU * f * t).sin();
                }
            }
            out[start + i] += s * env * gain;
        }
    }
    out
}

/// One beat of the metronome: where (in quarter notes of the played score) and whether it is the
/// first of the measure.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Beat {
    pub at: Beats,
    pub accent: bool,
}

/// The beats of the played measures. Compound meters (6/8, 9/8, 12/8) beat dotted quarters; a
/// measure shorter than its time signature (a pickup) gets the last beats, unaccented.
pub fn beats(score: &Score) -> Vec<Beat> {
    let end = score.parts.iter().flat_map(|p| p.notes.last()).map(|n| n.start + n.dur).max().unwrap_or_default();
    let mut out = vec![];
    let mut sig = score.time_sig;
    for (i, bar) in score.bars.iter().enumerate() {
        sig = bar.time_sig.unwrap_or(sig);
        let (n, d) = (sig.0.max(1) as i64, sig.1.max(1) as i64);
        let nominal = Ratio::new(n * 4, d);
        let bar_end = score.bars.get(i + 1).map_or(bar.start + nominal, |b| b.start);
        let len = bar_end - bar.start;
        if len <= Ratio::from_integer(0) || bar.start >= end {
            continue;
        }
        let unit = if d == 8 && n % 3 == 0 && n > 3 { Ratio::new(3, 2) } else { Ratio::new(4, d) };
        let count = (len / unit).floor().to_integer().max(1);
        if len < nominal {
            for k in (1..=count).rev() {
                out.push(Beat { at: bar_end - unit * k, accent: false });
            }
        } else {
            for k in 0..count {
                out.push(Beat { at: bar.start + unit * k, accent: k == 0 });
            }
        }
    }
    out
}

/// The metronome track: a click on every beat, louder on the first of the measure.
pub fn metronome(beats_s: &[(f64, bool)], sr: u32) -> Vec<f32> {
    let Some(last) = beats_s.last() else { return vec![] };
    let mut out = vec![0f32; ((last.0 + 0.1) * sr as f64) as usize];
    for &(t, accent) in beats_s {
        crate::audio::add_click(&mut out, sr as f64, t, accent);
    }
    out
}

/// A measure of clicks before `from` (seconds), at the beat found there, with the chord of the
/// first sung notes softly under the last two clicks. The buffer ends where playback starts.
pub fn count_in(beats_s: &[(f64, bool)], sung: &[(f64, f64, i32)], from: f64, sr: u32) -> Vec<f32> {
    if beats_s.is_empty() {
        return vec![];
    }
    // the beat where playback starts, its length and the beats of its measure
    let k = beats_s.iter().position(|b| b.0 >= from - 0.05).unwrap_or(beats_s.len() - 1);
    let beat = match (beats_s.get(k + 1), k.checked_sub(1).and_then(|j| beats_s.get(j))) {
        (Some(b), _) => b.0 - beats_s[k].0,
        (None, Some(p)) => beats_s[k].0 - p.0,
        _ => 0.6,
    }
    .clamp(0.2, 2.0);
    let bar_start = beats_s[..=k].iter().rposition(|b| b.1).unwrap_or(0);
    let per_bar = beats_s.iter().skip(bar_start + 1).position(|b| b.1).map_or(4, |n| n + 1);
    // at least three clicks (a 6/8 beats only two a measure)
    let n = if per_bar < 3 { per_bar * 2 } else { per_bar };
    let len = n as f64 * beat;
    let mut out = vec![0f32; (len * sr as f64) as usize];
    for i in 0..n {
        crate::audio::add_click(&mut out, sr as f64, i as f64 * beat, i % per_bar == 0);
    }
    // the first chord: what the voices sing at `from`, or their next notes
    let next = sung.iter().filter(|s| s.1 > from + 0.02).map(|s| s.0.max(from)).fold(f64::INFINITY, f64::min);
    let mut chord: Vec<i32> = sung.iter().filter(|s| s.0 <= next + 0.05 && s.1 > next).map(|s| s.2).collect();
    chord.sort();
    chord.dedup();
    for p in chord {
        crate::audio::add_tone(&mut out, sr as f64, len - 2.0 * beat, 2.0 * beat - 0.07, p, 0.07);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::{Bar, Note, Part, Tone};

    fn note(start: i64, dur: i64, pitch: i32, lyric: bool) -> Note {
        Note {
            id: format!("{start}"),
            start: Ratio::from_integer(start),
            dur: Ratio::from_integer(dur),
            pitch,
            lyric: lyric.then(|| crate::score::Lyric { text: "la".into(), syllabic: crate::score::Syllabic::Single }),
        }
    }

    fn score(bars: Vec<Bar>, parts: Vec<Part>) -> Score {
        Score { title: None, time_sig: (4, 4), time_sigs: vec![(4, 4)], bars, tempos: vec![(Ratio::from_integer(0), 1.0)], warnings: vec![], parts }
    }

    #[test]
    fn plays_only_the_instrumental_parts_with_their_chords() {
        let organ = Part {
            name: "Organo".into(),
            notes: vec![note(0, 1, 64, false), note(1, 1, 67, false)],
            tones: vec![
                Tone { start: Ratio::from_integer(0), dur: Ratio::from_integer(1), pitch: 60 },
                Tone { start: Ratio::from_integer(0), dur: Ratio::from_integer(1), pitch: 64 },
                Tone { start: Ratio::from_integer(1), dur: Ratio::from_integer(1), pitch: 67 },
            ],
            ..Default::default()
        };
        let sc = score(vec![], vec![organ, Part { name: "Soprano".into(), notes: vec![note(4, 1, 72, true)], ..Default::default() }]);
        let axis = TimeAxis::new(&[(0, 60.0)], TICKS);
        let buf = render(&sc, &axis, 8000, &HashMap::new(), None);
        // two quarter notes at 60 bpm = 2 s, plus the release; the sung note at 4 s is not there
        assert!((buf.len() as f64 / 8000.0) < 2.5, "{}", buf.len());
        let rms = |x: &[f32]| (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt();
        assert!(rms(&buf[..8000]) > rms(&buf[8000..16000]), "the chord is louder than the single note");
    }

    #[test]
    fn beats_follow_meters_and_pickups() {
        let bar = |start: i64, number: Option<&str>, sig: Option<(i32, i32)>| Bar {
            start: Ratio::from_integer(start),
            number: number.map(str::to_string),
            time_sig: sig,
        };
        let sung = Part { name: "S".into(), notes: vec![note(0, 8, 60, true)], ..Default::default() };
        // pickup of one quarter in 4/4, a 4/4 measure, then a 6/8 measure (3 quarters)
        let sc = score(vec![bar(0, None, Some((4, 4))), bar(1, Some("1"), None), bar(5, Some("2"), Some((6, 8)))], vec![sung]);
        let b: Vec<(String, bool)> = beats(&sc).iter().map(|b| (format!("{}", b.at), b.accent)).collect();
        let e = |a: &str, acc: bool| (a.to_string(), acc);
        assert_eq!(b, vec![e("0", false), e("1", true), e("2", false), e("3", false), e("4", false), e("5", true), e("13/2", false)]);
    }

    #[test]
    fn count_in_is_a_measure_with_the_first_chord() {
        // 4/4 at one beat a second
        let beats_s: Vec<(f64, bool)> = (0..12).map(|i| (i as f64, i % 4 == 0)).collect();
        let sung = vec![(4.0, 5.0, 60), (4.0, 5.0, 67), (8.0, 9.0, 62)];
        let buf = count_in(&beats_s, &sung, 4.0, 8000);
        assert_eq!(buf.len(), 4 * 8000, "four beats of one second");
        let rms = |a: f64, b: f64| {
            let s = &buf[(a * 8000.0) as usize..(b * 8000.0) as usize];
            (s.iter().map(|v| v * v).sum::<f32>() / s.len() as f32).sqrt()
        };
        assert!(rms(2.2, 2.9) > 0.01 && rms(0.2, 0.9) < 1e-3, "the chord sounds only under the last two clicks");
        // 6/8: two beats a measure, so two measures of clicks
        let compound: Vec<(f64, bool)> = (0..8).map(|i| (i as f64 * 0.9, i % 2 == 0)).collect();
        assert_eq!(count_in(&compound, &sung, 1.8, 8000).len(), (4.0 * 0.9 * 8000.0) as usize);
    }
}
