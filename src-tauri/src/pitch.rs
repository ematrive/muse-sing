//! Pitch of a sung recording (YIN, de Cheveigné & Kawahara 2002), for «Canta e controlla».
//! Pure Rust, no model: one estimate every 10 ms, between 65 and 1100 Hz.

/// Seconds between two estimates.
pub const HOP: f64 = 0.01;
const WINDOW: f64 = 0.03;
const F_MIN: f64 = 65.0;
const F_MAX: f64 = 1100.0;
/// YIN threshold on the cumulative mean normalized difference.
const THRESHOLD: f32 = 0.15;
/// Analysis rate: the input is low-passed and decimated to about this.
const TARGET_SR: u32 = 12_000;

/// Estimates every `HOP` seconds: frame `k` is centred at `k * HOP` seconds of the input.
#[derive(Debug, Clone, Default)]
pub struct Track {
    /// MIDI pitch (fractional), None where nobody sings.
    pub midi: Vec<Option<f32>>,
}

impl Track {
    pub fn at(&self, t: f64) -> Option<f32> {
        if t < 0.0 {
            return None;
        }
        self.midi.get((t / HOP).round() as usize).copied().flatten()
    }
}

/// Low-pass (windowed sinc) and keep one sample in `d`.
fn decimate(x: &[f32], d: usize) -> Vec<f32> {
    if d <= 1 {
        return x.to_vec();
    }
    let half = 8 * d;
    let fc = 0.45 / d as f64; // cycles per input sample
    let taps: Vec<f32> = (0..=2 * half)
        .map(|i| {
            let n = i as f64 - half as f64;
            let sinc = if n == 0.0 { 2.0 * fc } else { (2.0 * std::f64::consts::PI * fc * n).sin() / (std::f64::consts::PI * n) };
            let w = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / (2 * half) as f64).cos();
            (sinc * w) as f32
        })
        .collect();
    (0..x.len() / d)
        .map(|k| {
            let c = k * d;
            let mut s = 0f32;
            for (i, t) in taps.iter().enumerate() {
                let j = c as isize + i as isize - half as isize;
                if j >= 0 && (j as usize) < x.len() {
                    s += x[j as usize] * t;
                }
            }
            s
        })
        .collect()
}

fn hz_midi(f: f64) -> f32 {
    (69.0 + 12.0 * (f / 440.0).log2()) as f32
}

/// YIN over the whole recording.
pub fn analyze(samples: &[f32], sr: u32) -> Track {
    let d = (sr / TARGET_SR).max(1) as usize;
    let x = decimate(samples, d);
    let sr = sr as f64 / d as f64;
    let w = (WINDOW * sr) as usize;
    let tau_min = (sr / F_MAX).floor().max(2.0) as usize;
    let tau_max = (sr / F_MIN).ceil() as usize;
    let hop = HOP * sr;
    let frames = (x.len() as f64 / hop) as usize;
    // loudness of each frame, to tell singing from silence relative to this recording
    let rms: Vec<f32> = (0..frames)
        .map(|k| {
            let c = (k as f64 * hop) as usize;
            let a = c.saturating_sub(w / 2);
            let b = (a + w).min(x.len());
            if b <= a { 0.0 } else { (x[a..b].iter().map(|v| v * v).sum::<f32>() / (b - a) as f32).sqrt() }
        })
        .collect();
    let mut sorted: Vec<f32> = rms.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let loud = sorted.get(sorted.len() * 95 / 100).copied().unwrap_or(0.0);
    let gate = (loud * 0.1).max(1e-4);

    let mut raw: Vec<Option<f32>> = vec![None; frames];
    let mut diff = vec![0f32; tau_max + 1];
    for (k, r) in raw.iter_mut().enumerate() {
        if rms[k] < gate {
            continue;
        }
        let a = ((k as f64 * hop) as usize).saturating_sub(w / 2);
        if a + w + tau_max > x.len() {
            continue;
        }
        let seg = &x[a..a + w + tau_max];
        // difference function and its cumulative mean normalisation
        let mut sum = 0f32;
        diff[0] = 1.0;
        for tau in 1..=tau_max {
            let mut s = 0f32;
            for j in 0..w {
                let dv = seg[j] - seg[j + tau];
                s += dv * dv;
            }
            sum += s;
            diff[tau] = if sum > 0.0 { s * tau as f32 / sum } else { 1.0 };
        }
        // first dip under the threshold, down to its minimum
        let mut tau = tau_min;
        let mut found = None;
        while tau < tau_max {
            if diff[tau] < THRESHOLD {
                while tau + 1 < tau_max && diff[tau + 1] < diff[tau] {
                    tau += 1;
                }
                found = Some(tau);
                break;
            }
            tau += 1;
        }
        let Some(t) = found else { continue };
        // parabolic interpolation around the minimum
        let (y0, y1, y2) = (diff[t - 1], diff[t], diff[t + 1]);
        let den = y0 - 2.0 * y1 + y2;
        let shift = if den.abs() > 1e-9 { (0.5 * (y0 - y2) / den).clamp(-1.0, 1.0) } else { 0.0 };
        *r = Some(hz_midi(sr / (t as f64 + shift as f64)));
    }
    // median of 5 (only where most of them are voiced): removes octave jumps of a single frame
    let midi = (0..frames)
        .map(|k| {
            raw[k]?;
            let mut v: Vec<f32> = raw[k.saturating_sub(2)..(k + 3).min(frames)].iter().flatten().copied().collect();
            if v.len() < 3 {
                return raw[k];
            }
            v.sort_by(|a, b| a.total_cmp(b));
            Some(v[v.len() / 2])
        })
        .collect();
    Track { midi }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A sung-like tone: harmonics and a 5.5 Hz vibrato of ±`vib` cents around `midi`.
    pub fn voice(midi: f32, secs: f64, sr: u32, vib: f32) -> Vec<f32> {
        let n = (secs * sr as f64) as usize;
        let mut phase = 0f64;
        (0..n)
            .map(|i| {
                let t = i as f64 / sr as f64;
                let m = midi as f64 + vib as f64 / 100.0 * (2.0 * std::f64::consts::PI * 5.5 * t).sin();
                let f = 440.0 * 2f64.powf((m - 69.0) / 12.0);
                phase += 2.0 * std::f64::consts::PI * f / sr as f64;
                (0.3 * phase.sin() + 0.2 * (2.0 * phase).sin() + 0.15 * (3.0 * phase).sin() + 0.05 * (4.0 * phase).sin()) as f32 * 0.5
            })
            .collect()
    }

    /// `cargo test --release pitch::tests::speed -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn speed() {
        let x = voice(60.0, 240.0, 48_000, 20.0);
        let t = std::time::Instant::now();
        let tr = analyze(&x, 48_000);
        eprintln!("4 minutes at 48 kHz in {:.2} s ({} frames)", t.elapsed().as_secs_f64(), tr.midi.len());
    }

    #[test]
    fn finds_the_pitch_of_voices_without_octave_errors() {
        for sr in [44_100, 48_000, 16_000] {
            for midi in [43.0f32, 55.0, 62.3, 69.0, 79.0] {
                let mut x = vec![0f32; sr as usize / 2];
                x.extend(voice(midi, 1.0, sr, 30.0));
                x.extend(vec![0f32; sr as usize / 2]);
                let tr = analyze(&x, sr);
                assert_eq!(tr.at(0.2), None, "silence at {sr} Hz");
                let got: Vec<f32> = (60..140).filter_map(|k| tr.at(k as f64 * HOP)).collect();
                assert!(got.len() > 70, "{midi} at {sr}: {} voiced frames", got.len());
                let mean = got.iter().sum::<f32>() / got.len() as f32;
                assert!((mean - midi).abs() < 0.1, "{midi} at {sr} Hz: {mean}");
                assert!(got.iter().all(|m| (m - midi).abs() < 0.6), "{midi} at {sr} Hz: no octave jumps");
                assert_eq!(tr.at(1.8), None);
            }
        }
    }
}
