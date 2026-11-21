//! Accompaniment: the parts without lyrics (organ, piano…) played by a simple additive organ,
//! on the same time axis as the sung parts.

use crate::ds::time::TimeAxis;
use crate::score::{Beats, Score};
use crate::ustx::TICKS;

/// Harmonics (multiple, amplitude) of the organ sound: a soft flue stop.
const HARMONICS: &[(f32, f32)] = &[(1.0, 1.0), (2.0, 0.45), (3.0, 0.2), (4.0, 0.12), (6.0, 0.05)];
const ATTACK_S: f32 = 0.015;
const RELEASE_S: f32 = 0.08;
/// Gain of one note: several voices sum up, the mix scales the total down if it clips.
const NOTE_GAIN: f32 = 0.08;

fn ticks(b: Beats) -> f64 {
    *b.numer() as f64 / *b.denom() as f64 * TICKS as f64
}

/// Mono buffer at `sr` with every note of the parts without lyrics (empty if there are none).
pub fn render(score: &Score, axis: &TimeAxis, sr: u32) -> Vec<f32> {
    let notes: Vec<(f64, f64, i32)> = score
        .parts
        .iter()
        .filter(|p| !p.is_sung())
        .flat_map(|p| p.notes.iter())
        .map(|n| {
            let a = axis.tick_to_ms(ticks(n.start)) / 1000.0;
            let b = axis.tick_to_ms(ticks(n.start + n.dur)) / 1000.0;
            (a, b, n.pitch)
        })
        .filter(|(a, b, _)| b > a)
        .collect();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::{Note, Part};
    use num_rational::Ratio;

    #[test]
    fn plays_only_the_instrumental_parts() {
        let note = |start: i64, pitch: i32, lyric: bool| Note {
            id: format!("{start}"),
            start: Ratio::from_integer(start),
            dur: Ratio::from_integer(1),
            pitch,
            lyric: lyric.then(|| crate::score::Lyric { text: "la".into(), syllabic: crate::score::Syllabic::Single }),
        };
        let score = Score {
            title: None,
            time_sig: (4, 4),
            time_sigs: vec![(4, 4)],
            bars: vec![],
            tempos: vec![(Ratio::from_integer(0), 1.0)],
            warnings: vec![],
            parts: vec![
                Part { name: "Organo".into(), notes: vec![note(0, 60, false), note(1, 64, false)] },
                Part { name: "Soprano".into(), notes: vec![note(4, 72, true)] },
            ],
        };
        let axis = TimeAxis::new(&[(0, 60.0)], TICKS);
        let buf = render(&score, &axis, 8000);
        // two quarter notes at 60 bpm = 2 s, plus the release; the sung note at 4 s is not there
        assert!((buf.len() as f64 / 8000.0) < 2.5, "{}", buf.len());
        let rms = |x: &[f32]| (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt();
        assert!(rms(&buf[..8000]) > 0.01);
    }
}
