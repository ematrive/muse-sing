//! Muse Sing projects (`.msng`, formerly `.musesing`): a zip with `project.json` (choices, mix,
//! edits) and a copy of the score (`score.mscz`, `score.mxl` or `score.musicxml`), so a project can
//! be moved or sent on its own. The generated voices can be kept too (`audio/`: one FLAC per
//! finished sung part, with its TrackInfo), so reopening it, even on another Mac, needs no new
//! render; without them they are made again from the render cache or from scratch.

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
    /// The part «Io canto» (None: none chosen, or a project saved before it was kept here).
    #[serde(default)]
    pub mine: Option<String>,
    /// dB of the other voices in the study tracks.
    #[serde(default = "default_guide_db")]
    pub guide_db: f32,
    /// Your own part under «Canta e controlla»: "muta", "piano" or "normale".
    #[serde(default = "default_own_voice")]
    pub own_voice: String,
    /// Save the generated voices in the file.
    #[serde(default = "yes")]
    pub keep_audio: bool,
}

fn default_guide_db() -> f32 {
    -10.0
}

fn default_own_voice() -> String {
    "muta".into()
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
            mine: None,
            guide_db: default_guide_db(),
            own_voice: default_own_voice(),
            keep_audio: true,
        }
    }
}

const AUDIO_DIR: &str = "audio/";
const AUDIO_INDEX: &str = "audio/index.json";

/// A finished sung part kept in a project: `audio/<key>.flac` and `audio/<key>.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioPart {
    pub part: String,
    /// The render cache key of the part (`sing::part_file_name`), in hex.
    pub key: String,
    /// Multiply the decoded samples by this (`mix::flac_mono`).
    pub scale: f32,
}

/// A part to write: its description, the FLAC and the TrackInfo JSON.
pub struct AudioEntry {
    pub meta: AudioPart,
    pub flac: Vec<u8>,
    pub info: Vec<u8>,
}

/// The voices to save with a project.
pub enum Audio<'a> {
    None,
    Parts(Vec<AudioEntry>),
    /// The ones already in this project file, copied as they are (when the voices in memory are
    /// not those of the current choices, or were never loaded).
    KeepFrom(&'a Path),
}

/// Bytes of a project file and of its pieces (compressed, as stored).
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct Sizes {
    pub total: u64,
    pub score: u64,
    pub settings: u64,
    pub audio: u64,
    pub audio_parts: Vec<String>,
}

fn zip_err(e: impl std::fmt::Display) -> Error {
    Error::Format(format!("progetto: {e}"))
}

/// Writes the project and the score into `path` (through a temporary file, so a failed save
/// leaves the previous version intact).
pub fn save(path: &Path, project: &Project, score: &[u8], audio: Audio) -> Result<()> {
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
        match audio {
            Audio::None => {}
            Audio::Parts(parts) => {
                let index: Vec<&AudioPart> = parts.iter().map(|p| &p.meta).collect();
                zip.start_file(AUDIO_INDEX, opts).map_err(zip_err)?;
                zip.write_all(&serde_json::to_vec_pretty(&index).map_err(zip_err)?)?;
                for p in &parts {
                    zip.start_file(format!("{AUDIO_DIR}{}.flac", p.meta.key), opts.compression_method(zip::CompressionMethod::Stored)).map_err(zip_err)?;
                    zip.write_all(&p.flac)?;
                    zip.start_file(format!("{AUDIO_DIR}{}.json", p.meta.key), opts).map_err(zip_err)?;
                    zip.write_all(&p.info)?;
                }
            }
            Audio::KeepFrom(old) => {
                if let Ok(mut src) = std::fs::File::open(old).map_err(Error::from).and_then(|f| zip::ZipArchive::new(f).map_err(zip_err)) {
                    for i in 0..src.len() {
                        let entry = src.by_index_raw(i).map_err(zip_err)?;
                        if entry.name().starts_with(AUDIO_DIR) {
                            zip.raw_copy_file(entry).map_err(zip_err)?;
                        }
                    }
                }
            }
        }
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

/// The voices kept in a project file (empty when there are none).
pub fn audio(path: &Path) -> Result<Vec<AudioEntry>> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path)?).map_err(zip_err)?;
    let Ok(mut f) = zip.by_name(AUDIO_INDEX) else { return Ok(vec![]) };
    let mut text = String::new();
    f.read_to_string(&mut text)?;
    drop(f);
    let index: Vec<AudioPart> = serde_json::from_str(&text).map_err(|e| Error::Format(format!("{AUDIO_INDEX}: {e}")))?;
    let mut out = vec![];
    for p in index {
        let mut read = |ext: &str| -> Result<Vec<u8>> {
            let mut b = vec![];
            zip.by_name(&format!("{AUDIO_DIR}{}.{ext}", p.key)).map_err(zip_err)?.read_to_end(&mut b)?;
            Ok(b)
        };
        let (flac, info) = (read("flac")?, read("json")?);
        out.push(AudioEntry { meta: p, flac, info });
    }
    Ok(out)
}

/// What a project file weighs, piece by piece.
pub fn sizes(path: &Path) -> Result<Sizes> {
    let mut sizes = Sizes { total: std::fs::metadata(path)?.len(), ..Sizes::default() };
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path)?).map_err(zip_err)?;
    for i in 0..zip.len() {
        let f = zip.by_index_raw(i).map_err(zip_err)?;
        let n = f.compressed_size();
        match f.name() {
            PROJECT_JSON => sizes.settings += n,
            name if SCORE_NAMES.contains(&name) => sizes.score += n,
            name if name.starts_with(AUDIO_DIR) => sizes.audio += n,
            _ => {}
        }
    }
    if let Ok(mut f) = zip.by_name(AUDIO_INDEX) {
        let mut text = String::new();
        f.read_to_string(&mut text)?;
        let index: Vec<AudioPart> = serde_json::from_str(&text).unwrap_or_default();
        sizes.audio_parts = index.into_iter().map(|p| p.part).collect();
    }
    Ok(sizes)
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
        // every choice of the two tabs comes back as it was
        p.parts[0].singer = "musesing:Tiger".into();
        p.parts[0].color = Some("soft".into());
        p.parts[0].voices = 3;
        p.parts[1].played = true;
        p.pitch = "notes".into();
        p.transpose = -2;
        p.marks = false;
        p.count_in = true;
        p.metronome.enabled = true;
        p.mine = Some("Soprano".into());
        p.guide_db = -6.0;
        p.own_voice = "piano".into();
        let path = dir.join("t.msng");
        let xml = br#"<?xml version="1.0"?><score-partwise version="4.0"></score-partwise>"#;
        save(&path, &p, xml, Audio::None).unwrap();
        let (back, score) = open(&path).unwrap();
        assert_eq!(back, p);
        assert_eq!(score, xml);
        let wc = working_copy(&dir, &score).unwrap();
        assert_eq!(wc.extension().unwrap(), "musicxml");
        assert_eq!(std::fs::read(&wc).unwrap(), xml);
        assert!(open(&wc).is_err(), "a score is not a project");
        assert!(save(&path, &p, b"not a score", Audio::None).is_err());
        assert!(audio(&path).unwrap().is_empty(), "no voices saved");

        // with the voices: they come back; saved again with KeepFrom they are copied as they are
        let entry = |part: &str, key: &str| AudioEntry {
            meta: AudioPart { part: part.into(), key: key.into(), scale: 1.25 },
            flac: format!("flac of {part}").into_bytes(),
            info: br#"{"words":[]}"#.to_vec(),
        };
        save(&path, &p, xml, Audio::Parts(vec![entry("Soprano", "00000000000000aa"), entry("Basso", "00000000000000bb")])).unwrap();
        let back = audio(&path).unwrap();
        assert_eq!(back.iter().map(|e| (e.meta.clone(), e.flac.clone(), e.info.clone())).collect::<Vec<_>>(), [
            (entry("Soprano", "00000000000000aa").meta, b"flac of Soprano".to_vec(), br#"{"words":[]}"#.to_vec()),
            (entry("Basso", "00000000000000bb").meta, b"flac of Basso".to_vec(), br#"{"words":[]}"#.to_vec()),
        ]);
        let sz = sizes(&path).unwrap();
        assert!(sz.audio > 0 && sz.score > 0 && sz.settings > 0 && sz.total >= sz.audio + sz.score + sz.settings);
        assert_eq!(sz.audio_parts, ["Soprano", "Basso"]);
        let copy = dir.join("copia.msng");
        save(&copy, &p, xml, Audio::KeepFrom(&path)).unwrap();
        assert_eq!(audio(&copy).unwrap().len(), 2, "Salva con nome keeps the voices");
        save(&path, &p, xml, Audio::KeepFrom(&path)).unwrap();
        assert_eq!(audio(&path).unwrap().len(), 2, "saved over itself");
        save(&path, &p, xml, Audio::None).unwrap();
        assert!(audio(&path).unwrap().is_empty() && sizes(&path).unwrap().audio == 0, "removed");
        save(&dir.join("nuovo.msng"), &p, xml, Audio::KeepFrom(&dir.join("non-esiste.msng"))).unwrap();
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
        assert!(p.mine.is_none() && p.guide_db == -10.0 && p.own_voice == "muta");
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
