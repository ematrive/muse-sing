//! Phrase rendering: pitch model (DiffSingerPitch.cs), acoustic model + vocoder
//! (DiffSingerRenderer.InvokeDiffsinger), and placement of phrases on the track.

use super::onnx::{floats, Input};
use super::phonemize::{durations_ms_to_frames, PNote, Phone, Phonemized};
use super::time::TimeAxis;
use super::voicebank::{SubModel, Voicebank};
use crate::error::{Error, Result};

pub const HEAD_FRAMES: usize = 8;
pub const TAIL_FRAMES: usize = 8;
pub const SAMPLE_RATE: u32 = 44100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PitchSource {
    /// The voicebank's pitch model (natural intonation, vibrato); notes when there is none.
    Model,
    /// Notes with a short portamento, like OpenUtau's default render.
    Notes,
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub pitch: PitchSource,
    /// Acoustic diffusion steps (OpenUtau default 20).
    pub steps: i64,
    pub pitch_steps: i64,
    /// Shallow diffusion depth, capped by the voicebank's max_depth (OpenUtau default 1.0).
    pub depth: f64,
    pub speaker: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { pitch: PitchSource::Model, steps: 20, pitch_steps: 10, depth: 1.0, speaker: None }
    }
}

pub struct RPhone {
    pub symbol: String,
    pub pos_ms: f64,
    pub end_ms: f64,
}

pub struct RNote {
    pub position: i64,
    pub end: i64,
    pub pos_ms: f64,
    pub end_ms: f64,
    pub tone: f32,
    pub lyric: String,
}

/// Contiguous phonemes rendered in one go, with the notes they sing.
pub struct RPhrase {
    pub phones: Vec<RPhone>,
    pub ticks: Vec<(i64, i64)>,
    pub notes: Vec<RNote>,
}

impl RPhrase {
    pub fn pos_ms(&self) -> f64 {
        self.phones[0].pos_ms
    }

    /// PaddedSegments: head SP, phonemes with an SP for every gap, tail SP. (symbol, ms, phone index)
    fn segments(&self, frame_ms: f64) -> Vec<(String, f64, Option<usize>)> {
        let mut out = vec![("SP".to_string(), HEAD_FRAMES as f64 * frame_ms, None)];
        for (i, p) in self.phones.iter().enumerate() {
            if i > 0 {
                let gap = p.pos_ms - self.phones[i - 1].end_ms;
                if gap > 0.0 {
                    out.push(("SP".into(), gap, None));
                }
            }
            out.push((p.symbol.clone(), p.end_ms - p.pos_ms, Some(i)));
        }
        out.push(("SP".into(), TAIL_FRAMES as f64 * frame_ms, None));
        out
    }
}

/// A phrase longer than this is cut at the next word: the models' memory grows with the square
/// of the phrase length (a 74 s phrase without rests took tens of GB and the app was killed).
pub const MAX_PHRASE_MS: f64 = 10_000.0;
/// Without a word boundary, cut anyway at this length.
const HARD_MAX_PHRASE_MS: f64 = 20_000.0;

/// Splits phonemes into phrases where they stop touching (PhraseSource.FromPart), and long ones
/// at a word boundary (not in OpenUtau, which renders them whole).
pub fn phrases(ph: &Phonemized, notes: &[PNote], axis: &TimeAxis) -> Vec<RPhrase> {
    let mut out: Vec<RPhrase> = vec![];
    let mut cur: Vec<&Phone> = vec![];
    let mut flush = |cur: &mut Vec<&Phone>| {
        if cur.is_empty() {
            return;
        }
        let mut gs: Vec<usize> = cur.iter().map(|p| p.group).collect();
        gs.dedup();
        let rnotes = gs
            .iter()
            .flat_map(|&g| ph.groups[g].iter())
            .map(|&i| {
                let n = &notes[i];
                RNote {
                    position: n.position,
                    end: n.end(),
                    pos_ms: axis.tick_to_ms(n.position as f64),
                    end_ms: axis.tick_to_ms(n.end() as f64),
                    tone: n.tone as f32,
                    lyric: n.lyric.clone(),
                }
            })
            .collect();
        out.push(RPhrase {
            phones: cur
                .iter()
                .map(|p| RPhone { symbol: p.symbol.clone(), pos_ms: axis.tick_to_ms(p.position as f64), end_ms: axis.tick_to_ms(p.end as f64) })
                .collect(),
            ticks: cur.iter().map(|p| (p.position, p.end)).collect(),
            notes: rnotes,
        });
        cur.clear();
    };
    for p in &ph.phones {
        if let (Some(first), Some(last)) = (cur.first(), cur.last()) {
            let span = axis.tick_to_ms(p.position as f64) - axis.tick_to_ms(first.position as f64);
            let word_start = p.group != last.group;
            if last.end != p.position || (span >= MAX_PHRASE_MS && word_start) || span >= HARD_MAX_PHRASE_MS {
                flush(&mut cur);
            }
        }
        cur.push(p);
    }
    flush(&mut cur);
    out
}

fn midi_to_hz(m: f64) -> f64 {
    440.0 * 2f64.powf((m - 69.0) / 12.0)
}

/// FitDurationSum: makes the durations add up to `total`, taking from the end.
fn fit_sum(mut d: Vec<i32>, total: i32) -> Result<Vec<i32>> {
    if d.is_empty() {
        return Ok(d);
    }
    let delta = total - d.iter().sum::<i32>();
    let n = d.len();
    d[n - 1] += delta;
    if d[n - 1] < 0 {
        let mut deficit = -d[n - 1];
        d[n - 1] = 0;
        for i in (0..n - 1).rev() {
            let take = d[i].min(deficit);
            d[i] -= take;
            deficit -= take;
            if deficit == 0 {
                break;
            }
        }
        if deficit > 0 {
            return Err(Error::Other(format!("cannot fit durations to {total} frames")));
        }
    }
    Ok(d)
}

/// The note sounding at `ms` (or the nearest one) with a 50 ms ease-in-out portamento from an
/// adjacent previous note, as written in Muse Sing's .ustx (pitch points at -25 / +25 ms).
fn note_pitch(notes: &[RNote], ms: f64) -> f64 {
    if notes.is_empty() {
        return 60.0;
    }
    let i = notes.iter().rposition(|n| n.pos_ms <= ms).unwrap_or(0);
    let n = &notes[i];
    let tone = n.tone as f64;
    // portamento into the next note starts 25 ms before it
    if let Some(next) = notes.get(i + 1)
        && next.position == n.end
        && ms >= next.pos_ms - 25.0
    {
        let r = (ms - (next.pos_ms - 25.0)) / 50.0;
        return tone + (next.tone as f64 - tone) * (1.0 - (std::f64::consts::PI * r).cos()) / 2.0;
    }
    if i > 0 && notes[i - 1].end == n.position && ms < n.pos_ms + 25.0 {
        let prev = notes[i - 1].tone as f64;
        let r = (ms - (n.pos_ms - 25.0)) / 50.0;
        return prev + (tone - prev) * (1.0 - (std::f64::consts::PI * r).cos()) / 2.0;
    }
    tone
}

/// DsPitch.Process: predicted pitch (MIDI, fractional) per frame of `pitch.frame_ms()`.
fn predict_pitch(pm: &SubModel, phrase: &RPhrase, speaker: &str, steps: i64) -> Result<Vec<f32>> {
    let frame_ms = pm.frame_ms();
    let start_ms = phrase.pos_ms() - HEAD_FRAMES as f64 * frame_ms;
    let segs = phrase.segments(frame_ms);
    let tokens = segs.iter().map(|s| pm.token(&s.0)).collect::<Result<Vec<i64>>>()?;
    let ph_dur = durations_ms_to_frames(&segs.iter().map(|s| s.1).collect::<Vec<_>>(), frame_ms)?;
    let total: i32 = ph_dur.iter().sum();

    // vowel types come from dspitch/dsdict.yaml
    let dict_path = pm.root.join("dsdict.yaml");
    let dict: super::g2p::DictData = serde_yaml::from_str(&std::fs::read_to_string(&dict_path)?)
        .map_err(|e| Error::Format(format!("{}: {e}", dict_path.display())))?;
    let mut types = super::g2p::Dict::default();
    types.load(&dict);
    types.add_symbol_vowel("SP", true);
    types.add_symbol_vowel("AP", true);
    use super::g2p::G2p;
    for p in &phrase.phones {
        if !types.is_valid_symbol(&p.symbol) {
            return Err(Error::Other(format!("symbol \"{}\" not defined in {}", p.symbol, dict_path.display())));
        }
    }

    let mut ling = vec![("tokens", Input::I64(tokens))];
    if pm.config.predict_dur {
        let (wd, wdur) = word_div_dur(&segs, &ph_dur, |s| types.is_vowel(s))?;
        ling.push(("word_div", Input::I64(wd)));
        ling.push(("word_dur", Input::I64(wdur)));
    } else {
        ling.push(("ph_dur", Input::I64(ph_dur.iter().map(|&x| x as i64).collect())));
    }
    if pm.config.use_lang_id {
        let ids = segs
            .iter()
            .map(|s| if s.2.is_some() { pm.language_ids.get(super::config::phoneme_language(&s.0)).copied().unwrap_or(0) } else { 0 })
            .collect();
        ling.push(("languages", Input::I64(ids)));
    }
    let mut out = pm.linguistic.run(ling)?;
    let encoder_out = out.take("encoder_out")?;

    // notes with rests for gaps and padding
    let notes = &phrase.notes;
    let mut dur_ms = vec![(notes[0].pos_ms - start_ms).max(0.0)];
    let mut midi = vec![notes[0].tone];
    let mut rest = vec![true];
    let mut prev_end = notes[0].pos_ms;
    for n in notes {
        let gap = n.pos_ms - prev_end;
        if gap > 0.0 {
            dur_ms.push(gap);
            midi.push(n.tone);
            rest.push(true);
        }
        dur_ms.push(n.end_ms - n.pos_ms);
        midi.push(n.tone);
        if n.lyric.starts_with('+') {
            rest.push(*rest.last().unwrap());
        } else {
            let sung = phrase
                .phones
                .iter()
                .zip(&phrase.ticks)
                .filter(|(_, (pos, end))| *end > n.position + 1 && *pos < n.end - 1)
                .any(|(p, _)| p.symbol != "AP" && p.symbol != "SP" && types.is_vowel(&p.symbol));
            rest.push(!sung);
        }
        prev_end = n.end_ms;
    }
    dur_ms.push(TAIL_FRAMES as f64 * frame_ms);
    midi.push(notes.last().unwrap().tone);
    rest.push(true);
    if rest.iter().all(|r| *r) {
        midi.iter_mut().for_each(|m| *m = 60.0);
    } else {
        let mut i = 0;
        while i < rest.len() {
            if !rest[i] {
                i += 1;
                continue;
            }
            let mut j = i + 1;
            while j < rest.len() && rest[j] {
                j += 1;
            }
            if i == 0 {
                let v = midi[j];
                midi[..j].iter_mut().for_each(|m| *m = v);
            } else if j == rest.len() {
                let v = midi[i - 1];
                midi[i..].iter_mut().for_each(|m| *m = v);
            } else {
                let mid = (i + j).div_ceil(2);
                let (a, b) = (midi[i - 1], midi[j]);
                midi[i..mid].iter_mut().for_each(|m| *m = a);
                midi[mid..j].iter_mut().for_each(|m| *m = b);
            }
            i = j + 1;
        }
    }
    let note_dur = fit_sum(durations_ms_to_frames(&dur_ms, frame_ms)?, total)?;

    let n = total as usize;
    let mut inputs = vec![
        ("encoder_out", Input::Raw(encoder_out)),
        ("note_midi", Input::F32(midi)),
        ("note_dur", Input::I64(note_dur.iter().map(|&x| x as i64).collect())),
        ("ph_dur", Input::I64(ph_dur.iter().map(|&x| x as i64).collect())),
        ("pitch", Input::F32(vec![60.0; n])),
        ("retake", Input::Bool(vec![true; n])),
    ];
    if pm.config.use_continuous_acceleration {
        inputs.push(("steps", Input::I64(vec![steps.max(1)])));
    } else {
        let mut speedup = (1000.0 / steps.max(1) as f64).round().max(1.0) as i64;
        while 1000 % speedup != 0 && speedup > 1 {
            speedup -= 1;
        }
        inputs.push(("speedup", Input::I64(vec![speedup])));
    }
    if pm.config.use_expr {
        inputs.push(("expr", Input::F32(vec![1.0; n])));
    }
    if let Some(sp) = &pm.speakers {
        inputs.push(("spk_embed", Input::F32x2(sp.repeat(speaker, n), sp.hidden)));
    }
    if pm.config.use_note_rest {
        inputs.push(("note_rest", Input::Bool(rest)));
    }
    let mut out = pm.main.run(inputs)?;
    let (_, pitch) = floats(&out.first()?)?;
    Ok(pitch)
}

/// DiffSingerUtils.WordDivAndDur over padded segments.
fn word_div_dur(segs: &[(String, f64, Option<usize>)], ph_dur: &[i32], is_vowel: impl Fn(&str) -> bool) -> Result<(Vec<i64>, Vec<i64>)> {
    let mut vowels: Vec<usize> = (0..segs.len()).filter(|&i| segs[i].2.is_some() && is_vowel(&segs[i].0)).collect();
    if vowels.is_empty() {
        vowels = vec![segs.len() - 2];
    }
    let tail = segs.len() - 1;
    let mut div = vec![vowels[0] as i64];
    div.extend(vowels.windows(2).map(|w| (w[1] - w[0]) as i64));
    div.push((tail - vowels.last().unwrap()) as i64);
    div.push(1);
    if div.iter().any(|&d| d <= 0) || div.iter().sum::<i64>() as usize != ph_dur.len() {
        return Err(Error::Other("word division mismatch".into()));
    }
    let mut dur = vec![];
    let mut off = 0usize;
    for &d in &div {
        dur.push(ph_dur[off..off + d as usize].iter().map(|&x| x as i64).sum());
        off += d as usize;
    }
    Ok((div, dur))
}

/// Linear resampling of a per-frame curve between frame rates (ResampleCurve).
fn resample(curve: &[f32], len: usize, src_ms: f64, dst_ms: f64) -> Vec<f32> {
    if curve.len() == len && (src_ms - dst_ms).abs() < 1e-3 {
        return curve.to_vec();
    }
    (0..len)
        .map(|i| {
            let x = i as f64 * dst_ms / src_ms;
            if x <= 0.0 {
                curve[0]
            } else if x >= (curve.len() - 1) as f64 {
                curve[curve.len() - 1]
            } else {
                let x0 = x.floor() as usize;
                let f = (x - x0 as f64) as f32;
                curve[x0] + (curve[x0 + 1] - curve[x0]) * f
            }
        })
        .collect()
}

/// Renders one phrase. Returns (start in ms on the track, mono samples at 44.1 kHz).
pub fn render_phrase(vb: &Voicebank, phrase: &RPhrase, s: &Settings) -> Result<(f64, Vec<f32>)> {
    if vb.needs_variance() {
        return Err(Error::Other("voicebank needs a variance model (not supported yet)".into()));
    }
    let voc = vb.vocoder()?;
    let cfg = &vb.config;
    if voc.config.sample_rate != cfg.sample_rate || voc.config.hop_size != cfg.hop_size || voc.config.num_mel_bins != cfg.num_mel_bins {
        return Err(Error::Format("vocoder and acoustic model don't match (sample rate, hop size or mel bins)".into()));
    }
    if voc.config.sample_rate != SAMPLE_RATE {
        return Err(Error::Format(format!("vocoder sample rate {} not supported yet", voc.config.sample_rate)));
    }
    let speaker = vb.speaker_suffix(s.speaker.as_deref());
    let frame_ms = voc.config.frame_ms();
    let start_ms = phrase.pos_ms() - HEAD_FRAMES as f64 * frame_ms;
    let segs = phrase.segments(frame_ms);
    let tokens = segs
        .iter()
        .map(|x| vb.tokens.get(&x.0).copied().ok_or_else(|| Error::Other(format!("phoneme \"{}\" not in the acoustic model", x.0))))
        .collect::<Result<Vec<i64>>>()?;
    let durations = durations_ms_to_frames(&segs.iter().map(|x| x.1).collect::<Vec<_>>(), frame_ms)?;
    let total = durations.iter().sum::<i32>() as usize;

    let midi: Vec<f32> = match s.pitch {
        PitchSource::Model if vb.has_pitch_model() => {
            let pm = vb.pitch()?;
            let p = predict_pitch(pm, phrase, &speaker, s.pitch_steps)?;
            resample(&p, total, pm.frame_ms(), frame_ms)
        }
        _ => (0..total).map(|i| note_pitch(&phrase.notes, start_ms + i as f64 * frame_ms) as f32).collect(),
    };
    let f0: Vec<f32> = midi.iter().map(|&m| midi_to_hz(m as f64) as f32).collect();

    let acoustic = vb.acoustic()?;
    let mut inputs = vec![
        ("tokens", Input::I64(tokens.clone())),
        ("durations", Input::I64(durations.iter().map(|&d| d as i64).collect())),
        ("f0", Input::F32(f0.clone())),
    ];
    let depth = if cfg.use_variable_depth() { s.depth.min(cfg.max_depth()) } else { 1.0 };
    if cfg.use_continuous_acceleration {
        if cfg.use_variable_depth() {
            inputs.push(("depth", Input::F32(vec![depth as f32])));
        }
        inputs.push(("steps", Input::I64(vec![s.steps])));
    } else {
        let speedup;
        if cfg.use_variable_depth() {
            let mut d = (depth * 1000.0).round() as i64;
            speedup = (d / s.steps).max(1);
            d = d / speedup * speedup;
            inputs.push(("depth", Input::I64(vec![d])));
        } else {
            let mut sp = (1000 / s.steps).max(1);
            while 1000 % sp != 0 && sp > 1 {
                sp -= 1;
            }
            speedup = sp;
        }
        inputs.push(("speedup", Input::I64(vec![speedup])));
    }
    if cfg.use_lang_id {
        let ids = segs
            .iter()
            .map(|x| if x.2.is_some() { vb.language_ids.get(super::config::phoneme_language(&x.0)).copied().unwrap_or(0) } else { 0 })
            .collect();
        inputs.push(("languages", Input::I64(ids)));
    }
    if let Some(sp) = &vb.speakers {
        inputs.push(("spk_embed", Input::F32x2(sp.repeat(&speaker, total), sp.hidden)));
    }
    if cfg.use_key_shift_embed {
        inputs.push(("gender", Input::F32(vec![0.0; total])));
    }
    if cfg.use_speed_embed {
        inputs.push(("velocity", Input::F32(vec![1.0; total])));
    }
    let mut out = acoustic.run(inputs)?;
    let mel_v = out.first()?;
    let (shape, mut mel) = floats(&mel_v)?;
    if voc.config.mel_base != cfg.mel_base {
        let k = if voc.config.mel_base.0 == "e" { std::f32::consts::LN_10 } else { std::f32::consts::LOG10_E };
        mel.iter_mut().for_each(|x| *x *= k);
    }
    let bins = *shape.last().unwrap_or(&0) as usize;
    let mut out = voc.model.run(vec![("mel", Input::F32x2(mel, bins)), ("f0", Input::F32(f0))])?;
    let (_, samples) = floats(&out.first()?)?;
    Ok((start_ms, samples))
}

/// Renders a phonemized track to a mono buffer at 44.1 kHz starting at 0 ms. Phrases that fail
/// are skipped and reported in the returned warnings; if all of them fail, the first error is
/// returned. `progress(done, total)` returns false to stop (the result is then `Ok(None)`).
pub fn render_track(
    vb: &Voicebank,
    ph: &Phonemized,
    notes: &[PNote],
    axis: &TimeAxis,
    s: &Settings,
    mut progress: impl FnMut(usize, usize) -> bool,
) -> Result<Option<(Vec<f32>, Vec<String>)>> {
    let list = phrases(ph, notes, axis);
    let mut buf: Vec<f32> = vec![];
    let mut warnings = vec![];
    let mut first_error = None;
    let mut ok = 0;
    for (i, phrase) in list.iter().enumerate() {
        if !progress(i, list.len()) {
            return Ok(None);
        }
        let t0 = std::time::Instant::now();
        let len_s = (phrase.phones.last().map(|p| p.end_ms).unwrap_or(0.0) - phrase.pos_ms()) / 1000.0;
        crate::log!("frase {}/{}: da {:.1} s, lunga {:.1} s, {} fonemi", i + 1, list.len(), phrase.pos_ms() / 1000.0, len_s, phrase.phones.len());
        let result = render_phrase(vb, phrase, s);
        crate::log!("frase {}/{}: {} in {:.1} s", i + 1, list.len(), if result.is_ok() { "fatta" } else { "ERRORE" }, t0.elapsed().as_secs_f64());
        match result {
            Ok((start_ms, samples)) => {
                ok += 1;
                let start = (start_ms * SAMPLE_RATE as f64 / 1000.0).round() as i64;
                let skip = (-start).max(0) as usize;
                let start = start.max(0) as usize;
                let need = start + samples.len().saturating_sub(skip);
                if buf.len() < need {
                    buf.resize(need, 0.0);
                }
                for (k, v) in samples.iter().skip(skip).enumerate() {
                    buf[start + k] += v;
                }
            }
            Err(e) => {
                let text: Vec<&str> = phrase.phones.iter().map(|p| p.symbol.as_str()).collect();
                warnings.push(format!("frase [{}] non sintetizzata: {e}", text.join(" ")));
                first_error.get_or_insert(e);
            }
        }
    }
    if ok == 0 && let Some(e) = first_error {
        return Err(e);
    }
    progress(list.len(), list.len());
    Ok(Some((buf, warnings)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_sum_takes_from_the_end() {
        assert_eq!(fit_sum(vec![3, 4, 5], 14).unwrap(), vec![3, 4, 7]);
        assert_eq!(fit_sum(vec![3, 4, 2], 6).unwrap(), vec![3, 3, 0]);
        assert!(fit_sum(vec![1, 1], -1).is_err());
    }

    #[test]
    fn word_division() {
        let segs: Vec<(String, f64, Option<usize>)> = ["SP", "k", "a", "t", "o", "SP"]
            .iter()
            .enumerate()
            .map(|(i, s)| (s.to_string(), 1.0, if i == 0 || i == 5 { None } else { Some(i - 1) }))
            .collect();
        let (div, dur) = word_div_dur(&segs, &[8, 2, 5, 3, 6, 8], |s| s == "a" || s == "o").unwrap();
        assert_eq!(div, vec![2, 2, 1, 1]);
        assert_eq!(dur, vec![10, 8, 6, 8]);
    }

    #[test]
    fn portamento_between_adjacent_notes() {
        let n = |p: i64, e: i64, t: f32| RNote { position: p, end: e, pos_ms: p as f64, end_ms: e as f64, tone: t, lyric: "a".into() };
        let notes = vec![n(0, 500, 60.0), n(500, 1000, 62.0)];
        assert_eq!(note_pitch(&notes, 100.0), 60.0);
        assert!((note_pitch(&notes, 500.0) - 61.0).abs() < 1e-9);
        assert_eq!(note_pitch(&notes, 600.0), 62.0);
    }
}
