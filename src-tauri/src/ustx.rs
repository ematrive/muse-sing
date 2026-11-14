//! OpenUtau project (`.ustx`, YAML, version 0.10) for a DiffSinger voicebank.
//! Field names follow OpenUtau's YamlDotNet underscored naming (OpenUtau.Core/Ustx/*.cs).

use std::collections::HashMap;

use num_rational::Ratio;
use serde::Serialize;

use crate::lyrics::{word_lyrics, Lang};
use crate::score::{Beats, Score};

pub const TICKS: i64 = 480; // OpenUtau ticks per quarter note
const COLORS: &[&str] = &["Pink", "Blue", "Green", "Orange", "Purple", "Yellow"];

#[derive(Serialize)]
pub struct Project {
    pub name: String,
    pub comment: String,
    pub output_dir: String,
    pub cache_dir: String,
    pub ustx_version: String,
    pub key: i32,
    pub time_signatures: Vec<TimeSignature>,
    pub tempos: Vec<Tempo>,
    pub tracks: Vec<Track>,
    pub voice_parts: Vec<VoicePart>,
    pub wave_parts: Vec<serde_yaml::Value>,
}

#[derive(Serialize)]
pub struct TimeSignature {
    pub bar_position: i32,
    pub beat_per_bar: i32,
    pub beat_unit: i32,
}

#[derive(Serialize)]
pub struct Tempo {
    pub position: i64,
    pub bpm: f64,
}

#[derive(Serialize)]
pub struct RendererSettings {
    pub renderer: String,
}

#[derive(Serialize)]
pub struct Track {
    pub singer: String,
    pub phonemizer: String,
    pub renderer_settings: RendererSettings,
    pub track_name: String,
    pub track_color: String,
    pub mute: bool,
    pub solo: bool,
    pub volume: f64,
    pub pan: f64,
    pub voice_color_names: Vec<String>,
}

#[derive(Serialize)]
pub struct VoicePart {
    pub name: String,
    pub comment: String,
    pub track_no: usize,
    pub position: i64,
    pub duration: i64,
    pub notes: Vec<UNote>,
    pub curves: Vec<serde_yaml::Value>,
}

#[derive(Serialize)]
pub struct PitchPoint {
    pub x: f64,
    pub y: f64,
    pub shape: String,
}

#[derive(Serialize)]
pub struct Pitch {
    pub data: Vec<PitchPoint>,
    pub snap_first: bool,
}

#[derive(Serialize)]
pub struct Vibrato {
    pub length: f64,
    pub period: f64,
    pub depth: f64,
    #[serde(rename = "in")]
    pub fade_in: f64,
    #[serde(rename = "out")]
    pub fade_out: f64,
    pub shift: f64,
    pub drift: f64,
    pub vol_link: f64,
}

#[derive(Serialize)]
pub struct UNote {
    pub position: i64,
    pub duration: i64,
    pub tone: i32,
    pub lyric: String,
    pub pitch: Pitch,
    pub vibrato: Vibrato,
    pub phoneme_expressions: Vec<serde_yaml::Value>,
    pub phoneme_overrides: Vec<serde_yaml::Value>,
}

fn ticks(b: Beats) -> i64 {
    (b * Ratio::from_integer(TICKS)).round().to_integer()
}

pub struct Options<'a> {
    pub lang: Lang,
    pub singer: &'a str,
    /// Part names to include (None = every part with lyrics).
    pub parts: Option<&'a [String]>,
    pub hints: &'a HashMap<String, String>,
    pub fallback_name: &'a str,
}

pub fn build(score: &Score, opt: &Options) -> Project {
    let mut tracks = vec![];
    let mut voice_parts = vec![];
    let sung = score
        .parts
        .iter()
        .filter(|p| p.is_sung())
        .filter(|p| opt.parts.is_none_or(|sel| sel.contains(&p.name)));
    for (i, part) in sung.enumerate() {
        let lyrics = word_lyrics(&part.notes, opt.lang, opt.hints);
        let mut notes: Vec<UNote> = part
            .notes
            .iter()
            .zip(lyrics)
            .map(|(n, lyric)| UNote {
                position: ticks(n.start),
                duration: ticks(n.dur),
                tone: n.pitch,
                lyric,
                pitch: Pitch {
                    data: vec![
                        PitchPoint { x: -25.0, y: 0.0, shape: "io".into() },
                        PitchPoint { x: 25.0, y: 0.0, shape: "io".into() },
                    ],
                    snap_first: true,
                },
                vibrato: Vibrato {
                    length: 0.0, period: 175.0, depth: 25.0, fade_in: 10.0, fade_out: 10.0,
                    shift: 0.0, drift: 0.0, vol_link: 0.0,
                },
                phoneme_expressions: vec![],
                phoneme_overrides: vec![],
            })
            .collect();
        // overlapping notes (rounding) would confuse the phonemizer
        for k in 1..notes.len() {
            let next_pos = notes[k].position;
            let a = &mut notes[k - 1];
            if a.position + a.duration > next_pos {
                a.duration = next_pos - a.position;
            }
        }
        let end = notes.iter().map(|n| n.position + n.duration).max().unwrap_or(0);
        tracks.push(Track {
            singer: opt.singer.to_string(),
            phonemizer: opt.lang.phonemizer().to_string(),
            renderer_settings: RendererSettings { renderer: "DIFFSINGER".into() },
            track_name: part.name.clone(),
            track_color: COLORS[i % COLORS.len()].into(),
            mute: false,
            solo: false,
            volume: 0.0,
            pan: 0.0,
            voice_color_names: vec![String::new()],
        });
        voice_parts.push(VoicePart {
            name: part.name.clone(),
            comment: String::new(),
            track_no: i,
            position: 0,
            duration: end,
            notes,
            curves: vec![],
        });
    }
    Project {
        name: score.title.clone().filter(|t| !t.is_empty()).unwrap_or_else(|| opt.fallback_name.to_string()),
        comment: "Generato da Muse Sing".into(),
        output_dir: "Vocal".into(),
        cache_dir: "UCache".into(),
        ustx_version: "0.10".into(),
        key: 0,
        time_signatures: vec![TimeSignature { bar_position: 0, beat_per_bar: score.time_sig.0, beat_unit: score.time_sig.1 }],
        tempos: score
            .tempos
            .iter()
            .map(|(b, q)| Tempo { position: ticks(*b), bpm: (q * 60.0 * 1000.0).round() / 1000.0 })
            .collect(),
        tracks,
        voice_parts,
        wave_parts: vec![],
    }
}

pub fn to_yaml(p: &Project) -> String {
    serde_yaml::to_string(p).expect("ustx serialization")
}
