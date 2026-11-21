//! Muse Sing projects (`.msng`, formerly `.musesing`): a zip with `project.json` (choices, mix,
//! edits) and a copy of the score (`score.mscz`, `score.mxl` or `score.musicxml`), so a project can
//! be moved or sent on its own. Rendered audio is not stored: it lives in the render cache and can
//! always be made again.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::edits::PartEdits;
use crate::error::{Error, Result};
use crate::lyrics::Lang;
use crate::score::Format;

pub const EXTENSION: &str = "msng";
/// Extensions of projects saved by older versions (still opened; saved again as `.msng`).
pub const LEGACY_EXTENSIONS: &[&str] = &["musesing"];
const FORMAT_VERSION: u32 = 1;
const PROJECT_JSON: &str = "project.json";
/// The score copy inside the project, by format.
const SCORE_NAMES: &[&str] = &["score.mscz", "score.mxl", "score.musicxml"];

/// Whether a path has a project extension (current or legacy).
pub fn is_project_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .is_some_and(|e| e == EXTENSION || LEGACY_EXTENSIONS.contains(&e.as_str()))
}

fn score_format(score: &[u8]) -> Result<Format> {
    Format::detect(score).ok_or_else(|| Error::Format("la partitura non è un file MuseScore o MusicXML".into()))
}
/// Name of the accompaniment track in the mix.
pub const ACCOMPANIMENT: &str = "Accompagnamento";
/// Name of the metronome track in the mix.
pub const METRONOME: &str = "Metronomo";

fn default_true() -> bool {
    true
}

fn default_tempo() -> f64 {
    1.0
}

fn default_pitch() -> String {
    "model".into()
}

/// One part of the score: whether it is sung, by whom, and its place in the mix.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PartCfg {
    pub name: String,
    #[serde(default = "default_true")]
    pub sing: bool,
    #[serde(default)]
    pub singer: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub volume_db: f32,
    #[serde(default)]
    pub pan: f32,
    #[serde(default)]
    pub mute: bool,
    #[serde(default)]
    pub solo: bool,
    /// Singers in the section (1 = a soloist).
    #[serde(default = "default_voices")]
    pub voices: u8,
    /// Played on the piano instead of sung.
    #[serde(default)]
    pub played: bool,
}

fn default_voices() -> u8 {
    1
}

/// A played track (the accompaniment, the metronome): on or off and its place in the mix.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AccompCfg {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_accomp_db")]
    pub volume_db: f32,
    #[serde(default)]
    pub pan: f32,
    #[serde(default)]
    pub mute: bool,
    #[serde(default)]
    pub solo: bool,
}

fn default_accomp_db() -> f32 {
    -6.0
}

impl Default for AccompCfg {
    fn default() -> Self {
        AccompCfg { enabled: false, volume_db: default_accomp_db(), pan: 0.0, mute: false, solo: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Project {
    pub version: u32,
    pub title: String,
    /// File name of the score it was made from (for display; the score itself is inside).
    #[serde(default)]
    pub score_name: String,
    pub lang: Lang,
    /// "model" or "notes".
    #[serde(default = "default_pitch")]
    pub pitch: String,
    /// 1.0 = written tempo; 0.8 = 80% (slower, for practice).
    #[serde(default = "default_tempo")]
    pub tempo_factor: f64,
    #[serde(default)]
    pub parts: Vec<PartCfg>,
    #[serde(default)]
    pub accompaniment: AccompCfg,
    /// Pronunciation and curve edits per part.
    #[serde(default)]
    pub edits: HashMap<String, PartEdits>,
    /// Room ambience of the mix (dry in projects saved before it existed).
    #[serde(default)]
    pub ambience: crate::fx::Ambience,
    /// Amount of reverb, 0..1 (`fx::DEFAULT_AMOUNT` = the preset as designed).
    #[serde(default = "default_reverb")]
    pub reverb: f32,
    /// Instrument of each instrumental part (missing: guessed from the part name).
    #[serde(default)]
    pub instruments: HashMap<String, crate::instruments::Instrument>,
    /// Click on every beat, in the mix.
    #[serde(default)]
    pub metronome: AccompCfg,
    /// A measure of clicks before playback starts.
    #[serde(default)]
    pub count_in: bool,
    /// Follow the dynamics, breaths, fermatas and rit./accel. of the score.
    #[serde(default = "yes")]
    pub marks: bool,
    /// Semitones up or down (±6).
    #[serde(default)]
    pub transpose: i32,
    /// Study tracks: "evidenza" (the part louder), "senza" (without the part) or "entrambe".
    #[serde(default = "default_guides")]
    pub guides: String,
}

fn yes() -> bool {
    true
}

fn default_guides() -> String {
    "evidenza".into()
}

fn default_reverb() -> f32 {
    crate::fx::DEFAULT_AMOUNT
}

impl Project {
    pub fn new(title: &str, score_name: &str, lang: Lang, parts: Vec<PartCfg>, has_instruments: bool) -> Project {
        Project {
            version: FORMAT_VERSION,
            title: title.to_string(),
            score_name: score_name.to_string(),
            lang,
            pitch: default_pitch(),
            tempo_factor: 1.0,
            parts,
            accompaniment: AccompCfg { enabled: has_instruments, ..AccompCfg::default() },
            edits: HashMap::new(),
            ambience: crate::fx::Ambience::Chiesa,
            reverb: crate::fx::DEFAULT_AMOUNT,
            instruments: HashMap::new(),
            metronome: AccompCfg::default(),
            count_in: false,
            marks: true,
            transpose: 0,
            guides: default_guides(),
        }
    }
}

fn zip_err(e: impl std::fmt::Display) -> Error {
    Error::Format(format!("progetto: {e}"))
}

/// Writes the project and the score into `path` (through a temporary file, so a failed save
/// leaves the previous version intact).
pub fn save(path: &Path, project: &Project, score: &[u8]) -> Result<()> {
    let name = format!("score.{}", score_format(score)?.extension());
    let tmp = path.with_extension(format!("{EXTENSION}.part"));
    {
        let file = std::fs::File::create(&tmp)?;
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file(PROJECT_JSON, opts).map_err(zip_err)?;
        zip.write_all(serde_json::to_string_pretty(project).map_err(zip_err)?.as_bytes())?;
        // .mscz and .mxl are already compressed
        let method = if name.ends_with(".musicxml") { zip::CompressionMethod::Deflated } else { zip::CompressionMethod::Stored };
        zip.start_file(name, opts.compression_method(method)).map_err(zip_err)?;
        zip.write_all(score)?;
        zip.finish().map_err(zip_err)?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Reads a project: (project, score bytes).
pub fn open(path: &Path) -> Result<(Project, Vec<u8>)> {
    let file = std::fs::File::open(path)?;
    let mut zip = zip::ZipArchive::new(file).map_err(|_| Error::Format("non è un progetto Muse Sing".into()))?;
    let mut text = String::new();
    zip.by_name(PROJECT_JSON).map_err(|_| Error::Format("progetto senza project.json".into()))?.read_to_string(&mut text)?;
    let project: Project = serde_json::from_str(&text).map_err(|e| Error::Format(format!("project.json: {e}")))?;
    if project.version > FORMAT_VERSION {
        return Err(Error::Format("progetto creato da una versione più nuova di Muse Sing: aggiorna l'app".into()));
    }
    let name = SCORE_NAMES
        .iter()
        .find(|n| zip.index_for_name(n).is_some())
        .ok_or_else(|| Error::Format("progetto senza partitura".into()))?;
    let mut score = vec![];
    zip.by_name(name).map_err(zip_err)?.read_to_end(&mut score)?;
    Ok((project, score))
}

/// FNV-1a of the score bytes, to name its working copy.
pub fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325u64, |h, b| (h ^ *b as u64).wrapping_mul(0x100000001b3))
}

/// Puts the score in the cache folder (where the rest of the app reads it) and returns its path.
pub fn working_copy(cache_dir: &Path, score: &[u8]) -> Result<PathBuf> {
    let ext = score_format(score)?.extension();
    let dir = cache_dir.join("projects");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{:016x}.{ext}", hash(score)));
    if !path.is_file() {
        std::fs::write(&path, score)?;
    }
    Ok(path)
}

/// Keeps the settings of the parts that are still in the score (by name), adds the new ones
/// with `fresh`, and drops the ones that are gone.
pub fn merge_parts(old: &[PartCfg], fresh: Vec<PartCfg>) -> Vec<PartCfg> {
    fresh.into_iter().map(|f| old.iter().find(|o| o.name == f.name).cloned().unwrap_or(f)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(name: &str) -> PartCfg {
        PartCfg { name: name.into(), sing: true, singer: "s".into(), color: None, volume_db: 0.0, pan: 0.0, mute: false, solo: false, voices: 1, played: false }
    }

    #[test]
    fn round_trip() {
        let dir = std::env::temp_dir().join(format!("muse-sing-project-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut p = Project::new("Santo", "santo.mscz", Lang::It, vec![part("Soprano"), part("Basso")], true);
        p.tempo_factor = 0.8;
        p.parts[1].volume_db = -3.0;
        p.edits.entry("Soprano".into()).or_default().words.insert("x".into(), "y".into());
        let path = dir.join("t.msng");
        let xml = br#"<?xml version="1.0"?><score-partwise version="4.0"></score-partwise>"#;
        save(&path, &p, xml).unwrap();
        let (back, score) = open(&path).unwrap();
        assert_eq!(back, p);
        assert_eq!(score, xml);
        let wc = working_copy(&dir, &score).unwrap();
        assert_eq!(wc.extension().unwrap(), "musicxml");
        assert_eq!(std::fs::read(&wc).unwrap(), xml);
        assert!(open(&wc).is_err(), "a score is not a project");
        assert!(save(&path, &p, b"not a score").is_err());
        assert!(is_project_path(Path::new("/a/b.MSNG")) && is_project_path(Path::new("x.musesing")));
        assert!(!is_project_path(Path::new("x.mscz")));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn old_files_get_defaults() {
        let p: Project = serde_json::from_str(r#"{"version":1,"title":"t","lang":"it","parts":[{"name":"A"}]}"#).unwrap();
        assert_eq!(p.tempo_factor, 1.0);
        assert_eq!(p.pitch, "model");
        assert!(p.parts[0].sing && !p.accompaniment.enabled);
        assert_eq!(p.parts[0].voices, 1);
        assert_eq!(p.reverb, crate::fx::DEFAULT_AMOUNT);
        assert_eq!(p.ambience, crate::fx::Ambience::Nessuno, "saved projects keep their dry sound");
        assert!(p.marks && p.transpose == 0 && p.guides == "evidenza");
        let n = Project::new("t", "s.mscz", Lang::It, vec![], false);
        assert_eq!(n.ambience, crate::fx::Ambience::Chiesa);
        let back: Project = serde_json::from_str(&serde_json::to_string(&n).unwrap()).unwrap();
        assert_eq!(back, n);
    }

    #[test]
    fn reload_keeps_known_parts() {
        let mut a = part("A");
        a.volume_db = 4.0;
        let merged = merge_parts(&[a, part("Gone")], vec![part("A"), part("New")]);
        assert_eq!(merged.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["A", "New"]);
        assert_eq!(merged[0].volume_db, 4.0);
    }
}
