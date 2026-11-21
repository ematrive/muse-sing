//! Real instruments for the accompaniment and the played guide parts: a General MIDI SoundFont
//! (GeneralUser GS by S. Christian Collins) played by `rustysynth`.
//!
//! The SoundFont is not shipped with the app: the user downloads it on request from a copy kept
//! with Muse Sing's releases (its author asks not to link his own download files).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

pub const SOUNDFONT_URL: &str = "https://github.com/ematrive/muse-sing/releases/download/soundfont-1/GeneralUser-GS.sf2";
pub const SOUNDFONT_FILE: &str = "GeneralUser-GS.sf2";
/// The synthesizer plays about 12 dB below the simple organ it replaces: brought to the same level.
const GAIN: f32 = 4.0;

/// The instruments offered in the UI, with their General MIDI program.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Instrument {
    OrganoCanne,
    Armonium,
    Pianoforte,
    Archi,
    Flauto,
    Chitarra,
    Arpa,
}

impl Instrument {
    pub fn program(self) -> i32 {
        match self {
            Instrument::OrganoCanne => 19, // Church Organ
            Instrument::Armonium => 20,    // Reed Organ
            Instrument::Pianoforte => 0,   // Acoustic Grand Piano
            Instrument::Archi => 48,       // String Ensemble 1
            Instrument::Flauto => 73,      // Flute
            Instrument::Chitarra => 24,    // Nylon Guitar
            Instrument::Arpa => 46,        // Orchestral Harp
        }
    }

    /// A sensible instrument for a part, from its name (organ when nothing matches).
    pub fn for_part(name: &str) -> Instrument {
        let n = name.to_lowercase();
        let has = |keys: &[&str]| keys.iter().any(|k| n.contains(k));
        if has(&["armonium", "harmonium"]) {
            Instrument::Armonium
        } else if has(&["piano", "pf.", "tastier", "keyboard"]) {
            Instrument::Pianoforte
        } else if has(&["chitarr", "guitar"]) {
            Instrument::Chitarra
        } else if has(&["flaut", "flute"]) {
            Instrument::Flauto
        } else if has(&["viol", "archi", "string", "cello", "contrabb", "bass"]) && !has(&["basso", "bassi"]) {
            Instrument::Archi
        } else if has(&["arpa", "harp"]) {
            Instrument::Arpa
        } else {
            Instrument::OrganoCanne
        }
    }
}

/// The SoundFont, loaded once (it is about 30 MB).
pub fn load(path: &Path) -> Result<Arc<SoundFont>> {
    static CACHE: Mutex<Option<(PathBuf, Arc<SoundFont>)>> = Mutex::new(None);
    let mut c = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((p, sf)) = c.as_ref()
        && p == path
    {
        return Ok(sf.clone());
    }
    let mut f = std::io::BufReader::new(std::fs::File::open(path)?);
    let sf = Arc::new(SoundFont::new(&mut f).map_err(|e| Error::Format(format!("strumenti non validi: {e:?}")))?);
    *c = Some((path.to_path_buf(), sf.clone()));
    Ok(sf)
}

/// What one instrument plays: (start s, end s, MIDI pitch) and how hard.
pub struct Line {
    pub instrument: Instrument,
    pub notes: Vec<(f64, f64, i32)>,
    pub velocity: i32,
}

/// Plays the lines (one MIDI channel each) into a mono buffer at `sr`.
pub fn render(sf: &Arc<SoundFont>, lines: &[Line], sr: u32) -> Result<Vec<f32>> {
    let mut settings = SynthesizerSettings::new(sr as i32);
    settings.enable_reverb_and_chorus = false; // the mix adds the room
    settings.maximum_polyphony = 128;
    let mut synth = Synthesizer::new(sf, &settings).map_err(|e| Error::Other(format!("sintetizzatore: {e:?}")))?;
    // (sample, off before on, channel, key, velocity)
    let mut events: Vec<(usize, u8, i32, i32, i32)> = vec![];
    let channels = (0..16).filter(|c| *c != 9); // 9 is the drum channel
    for (line, ch) in lines.iter().zip(channels) {
        synth.process_midi_message(ch, 0xC0, line.instrument.program(), 0);
        for &(a, b, pitch) in &line.notes {
            let (a, b) = ((a * sr as f64) as usize, (b * sr as f64) as usize);
            if b <= a || !(0..128).contains(&pitch) {
                continue;
            }
            events.push((a, 1, ch, pitch, line.velocity));
            events.push((b, 0, ch, pitch, 0));
        }
    }
    if events.is_empty() {
        return Ok(vec![]);
    }
    events.sort();
    let tail = sr as usize * 2; // releases ring out
    let len = events.last().map_or(0, |e| e.0) + tail;
    let (mut l, mut r) = (vec![0f32; len], vec![0f32; len]);
    let mut at = 0;
    for (sample, on, ch, key, vel) in events {
        if sample > at {
            synth.render(&mut l[at..sample], &mut r[at..sample]);
            at = sample;
        }
        if on == 1 {
            synth.note_on(ch, key, vel);
        } else {
            synth.note_off(ch, key);
        }
    }
    synth.render(&mut l[at..], &mut r[at..]);
    let mut mono: Vec<f32> = l.iter().zip(&r).map(|(a, b)| (a + b) * 0.5 * GAIN).collect();
    let floor = 1e-4;
    let last = mono.iter().rposition(|x| x.abs() > floor).map_or(0, |i| i + 1);
    mono.truncate(last);
    Ok(mono)
}

/// Instrument of each instrumental part: the chosen one, or one guessed from the name.
pub fn choose(parts: &[String], chosen: &HashMap<String, Instrument>) -> Vec<Instrument> {
    parts.iter().map(|p| chosen.get(p).copied().unwrap_or_else(|| Instrument::for_part(p))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instruments_from_part_names() {
        assert_eq!(Instrument::for_part("Organo"), Instrument::OrganoCanne);
        assert_eq!(Instrument::for_part("Pianoforte"), Instrument::Pianoforte);
        assert_eq!(Instrument::for_part("Piano (2)"), Instrument::Pianoforte);
        assert_eq!(Instrument::for_part("Chitarra"), Instrument::Chitarra);
        assert_eq!(Instrument::for_part("Violino I"), Instrument::Archi);
        assert_eq!(Instrument::for_part("Basso"), Instrument::OrganoCanne, "a sung bass is not a double bass");
        assert_eq!(Instrument::for_part("Flauto"), Instrument::Flauto);
        assert_eq!(Instrument::for_part("Harmonium"), Instrument::Armonium);
        let names = vec!["Organo".to_string(), "Flauto".to_string()];
        let chosen = HashMap::from([("Organo".to_string(), Instrument::Pianoforte)]);
        assert_eq!(choose(&names, &chosen), vec![Instrument::Pianoforte, Instrument::Flauto]);
    }

    /// Needs the SoundFont: `MUSESING_TEST_SOUNDFONT=/path/GeneralUser-GS.sf2`.
    #[test]
    fn plays_notes_with_the_soundfont() {
        let Some(path) = std::env::var_os("MUSESING_TEST_SOUNDFONT") else { return };
        let sf = load(Path::new(&path)).unwrap();
        for inst in [Instrument::OrganoCanne, Instrument::Pianoforte, Instrument::Archi] {
            let lines = [Line { instrument: inst, notes: vec![(0.1, 0.6, 60), (0.6, 1.1, 64), (0.6, 1.1, 67)], velocity: 80 }];
            let out = render(&sf, &lines, 44100).unwrap();
            let rms = |a: f64, b: f64| {
                let s = &out[(a * 44100.0) as usize..(b * 44100.0) as usize];
                (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt()
            };
            assert!(rms(0.0, 0.09) < 1e-3, "{inst:?}: silent before the first note");
            assert!(rms(0.2, 0.5) > 0.005 && rms(0.7, 1.0) > 0.005, "{inst:?}: sounding");
            assert!(out.iter().all(|x| x.is_finite() && x.abs() < 2.0));
        }
    }
}
