//! Room ambience for the mix: a stereo Freeverb (Jezar's public-domain design: 8 damped comb
//! filters and 4 allpass filters per channel), with a short pre-delay.

use serde::{Deserialize, Serialize};

/// Ambience presets, as chosen in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Ambience {
    /// Dry, as rendered.
    #[default]
    Nessuno,
    Stanza,
    Chiesa,
    Cattedrale,
}

struct Preset {
    room: f32,
    damp: f32,
    wet: f32,
    predelay_s: f32,
    /// Silence added at the end for the reverb to ring out.
    tail_s: f32,
}

impl Ambience {
    fn preset(self) -> Option<Preset> {
        match self {
            Ambience::Nessuno => None,
            Ambience::Stanza => Some(Preset { room: 0.5, damp: 0.5, wet: 0.16, predelay_s: 0.008, tail_s: 1.5 }),
            Ambience::Chiesa => Some(Preset { room: 0.8, damp: 0.35, wet: 0.24, predelay_s: 0.02, tail_s: 3.0 }),
            Ambience::Cattedrale => Some(Preset { room: 0.93, damp: 0.25, wet: 0.3, predelay_s: 0.035, tail_s: 5.0 }),
        }
    }
}

// tunings for 44.1 kHz, scaled for other rates
const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASSES: [usize; 4] = [556, 441, 341, 225];
const SPREAD: usize = 23;
const FIXED_GAIN: f32 = 0.015;

struct Comb {
    buf: Vec<f32>,
    i: usize,
    store: f32,
}

impl Comb {
    fn new(len: usize) -> Comb {
        Comb { buf: vec![0.0; len.max(1)], i: 0, store: 0.0 }
    }
    #[inline]
    fn process(&mut self, x: f32, feedback: f32, damp: f32) -> f32 {
        let out = self.buf[self.i];
        self.store = out * (1.0 - damp) + self.store * damp;
        self.buf[self.i] = x + self.store * feedback;
        self.i = (self.i + 1) % self.buf.len();
        out
    }
}

struct Allpass {
    buf: Vec<f32>,
    i: usize,
}

impl Allpass {
    fn new(len: usize) -> Allpass {
        Allpass { buf: vec![0.0; len.max(1)], i: 0 }
    }
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let b = self.buf[self.i];
        self.buf[self.i] = x + b * 0.5;
        self.i = (self.i + 1) % self.buf.len();
        b - x
    }
}

/// Adds the ambience to a stereo mix in place, lengthening it by the reverb tail.
pub fn apply(l: &mut Vec<f32>, r: &mut Vec<f32>, sr: u32, ambience: Ambience) {
    let Some(p) = ambience.preset() else { return };
    let scale = sr as f32 / 44100.0;
    let sz = |n: usize| ((n as f32) * scale).round() as usize;
    let tail = (p.tail_s * sr as f32) as usize;
    let len = l.len().max(r.len()) + tail;
    l.resize(len, 0.0);
    r.resize(len, 0.0);

    let feedback = p.room * 0.28 + 0.7;
    let damp = p.damp * 0.4;
    let mut combs: [Vec<Comb>; 2] = [COMBS.iter().map(|&n| Comb::new(sz(n))).collect(), COMBS.iter().map(|&n| Comb::new(sz(n + SPREAD))).collect()];
    let mut aps: [Vec<Allpass>; 2] =
        [ALLPASSES.iter().map(|&n| Allpass::new(sz(n))).collect(), ALLPASSES.iter().map(|&n| Allpass::new(sz(n + SPREAD))).collect()];
    let pre = ((p.predelay_s * sr as f32) as usize).max(1);
    let mut pre_buf = vec![0f32; pre];
    let mut pi = 0;
    for i in 0..len {
        let input = pre_buf[pi];
        pre_buf[pi] = (l[i] + r[i]) * 0.5 * FIXED_GAIN;
        pi = (pi + 1) % pre;
        for (ch, out) in [&mut *l, &mut *r].into_iter().enumerate() {
            let mut acc = 0.0;
            for c in combs[ch].iter_mut() {
                acc += c.process(input, feedback, damp);
            }
            for a in aps[ch].iter_mut() {
                acc = a.process(acc);
            }
            out[i] += acc * p.wet * 3.0;
        }
    }
    // drop the silent end of the tail
    let floor = 1e-4;
    let last = l.iter().zip(r.iter()).rposition(|(a, b)| a.abs() > floor || b.abs() > floor).map_or(0, |i| i + 1);
    l.truncate(last);
    r.truncate(last);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_is_untouched() {
        let (mut l, mut r) = (vec![0.5f32, 0.0, -0.5], vec![0.1f32, 0.2, 0.3]);
        apply(&mut l, &mut r, 44100, Ambience::Nessuno);
        assert_eq!((l, r), (vec![0.5, 0.0, -0.5], vec![0.1, 0.2, 0.3]));
    }

    #[test]
    fn an_impulse_rings_out_and_decays() {
        for amb in [Ambience::Stanza, Ambience::Chiesa, Ambience::Cattedrale] {
            let mut l = vec![0f32; 4410];
            let mut r = vec![0f32; 4410];
            l[0] = 1.0;
            r[0] = 1.0;
            apply(&mut l, &mut r, 44100, amb);
            assert!(l.len() > 4410 + 22050, "{amb:?}: a tail of at least half a second");
            assert!(l.iter().chain(&r).all(|x| x.is_finite() && x.abs() <= 1.0), "{amb:?}");
            let energy = |s: &[f32]| s.iter().map(|x| x * x).sum::<f32>();
            let n = l.len();
            let (early, late) = (energy(&l[100..n / 4]), energy(&l[3 * n / 4..]));
            assert!(early > 0.0 && late < early / 10.0, "{amb:?}: decays ({early} → {late})");
            // the two sides differ (stereo spread)
            assert!(l[5000..6000] != r[5000..6000], "{amb:?}");
        }
    }
}
