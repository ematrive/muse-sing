//! Sung audio inside Muse Sing: renders each part with its own singer (the `ds` engine), keeps
//! the rendered tracks (in memory and in a disk cache, one file per part) and mixes them.
//!
//! A render runs on its own thread with its own job id, apart from the audio engine's jobs, so
//! playing or recording doesn't stop it. Progress and the result arrive as `sing-progress` /
//! `sing-done` events.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::ds::{self, render::PitchSource, render::Settings, voicebank::Voicebank};
use crate::error::{Error, Result};
use crate::lyrics::Lang;
use crate::singers::SingerInfo;
use crate::{score, ustx};

/// Changes whenever the engine's output for the same input changes, to invalidate the cache.
const ENGINE_VERSION: &str = "ds-2";
/// The disk cache is trimmed (oldest files first) above this size.
const CACHE_MAX_BYTES: u64 = 2 << 30;

#[derive(Debug, Clone, Deserialize)]
pub struct PartReq {
    pub name: String,
    /// Singer id, as in `SingerInfo::id`.
    pub singer: String,
    /// Voice colour (subbank); None = the voicebank's first.
    pub color: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RenderRequest {
    pub path: String,
    pub lang: Lang,
    /// "model" (natural intonation) or "notes" (exact pitch).
    pub pitch: String,
    pub parts: Vec<PartReq>,
}

#[derive(Serialize, Clone)]
pub struct Progress {
    pub job: u64,
    pub part: String,
    /// Part being rendered (0-based) out of `parts`.
    pub index: usize,
    pub parts: usize,
    pub phrase: usize,
    pub phrases: usize,
    /// The part came from the cache.
    pub cached: bool,
}

#[derive(Serialize, Clone)]
pub struct Done {
    pub job: u64,
    pub error: Option<String>,
    pub cancelled: bool,
    pub warnings: Vec<String>,
    /// Length of the longest track.
    pub seconds: f64,
}

pub struct Track {
    pub name: String,
    pub samples: Arc<Vec<f32>>,
}

pub struct Sing {
    next_job: AtomicU64,
    /// Running render (0 = none).
    job: AtomicU64,
    /// Tracks of the last completed render.
    tracks: Mutex<Vec<Track>>,
    title: Mutex<String>,
    voicebanks: Mutex<HashMap<PathBuf, Arc<Voicebank>>>,
    cache_dir: Option<PathBuf>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// FNV-1a, stable across builds (the std hasher isn't), for cache file names.
fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

fn mtime(p: &Path) -> u64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl Sing {
    pub fn new(cache_dir: Option<PathBuf>) -> Sing {
        Sing {
            next_job: AtomicU64::new(1),
            job: AtomicU64::new(0),
            tracks: Mutex::new(vec![]),
            title: Mutex::new(String::new()),
            voicebanks: Mutex::new(HashMap::new()),
            cache_dir: cache_dir.map(|d| d.join("render")),
        }
    }

    pub fn begin(&self) -> u64 {
        let id = self.next_job.fetch_add(1, Ordering::Relaxed);
        self.job.store(id, Ordering::Relaxed);
        id
    }

    pub fn cancel(&self) {
        self.job.store(0, Ordering::Relaxed);
    }

    fn is_current(&self, job: u64) -> bool {
        self.job.load(Ordering::Relaxed) == job
    }

    fn voicebank(&self, dir: &Path) -> Result<Arc<Voicebank>> {
        if let Some(vb) = lock(&self.voicebanks).get(dir) {
            return Ok(vb.clone());
        }
        let vb = Arc::new(Voicebank::load(dir)?);
        lock(&self.voicebanks).insert(dir.to_path_buf(), vb.clone());
        Ok(vb)
    }

    pub fn title(&self) -> String {
        lock(&self.title).clone()
    }

    /// The last rendered tracks, for mixing.
    pub fn with_tracks<T>(&self, f: impl FnOnce(&[(String, &[f32])]) -> T) -> Result<T> {
        let tracks = lock(&self.tracks);
        if tracks.is_empty() {
            return Err(Error::Other("genera prima l'audio".into()));
        }
        let list: Vec<(String, &[f32])> = tracks.iter().map(|t| (t.name.clone(), t.samples.as_slice())).collect();
        Ok(f(&list))
    }

    /// Renders every requested part (the cached ones are read back). Returns the warnings, or
    /// None if the job was cancelled or replaced.
    pub fn run(
        &self,
        job: u64,
        req: &RenderRequest,
        singers: &[SingerInfo],
        progress: &dyn Fn(Progress),
    ) -> Result<Option<(Vec<String>, f64)>> {
        if req.parts.is_empty() {
            return Err(Error::Other("scegli almeno una voce".into()));
        }
        crate::log!("genera audio: {} ({} voci, lingua {}, intonazione {})", req.path, req.parts.len(), req.lang.code(), req.pitch);
        let pitch = match req.pitch.as_str() {
            "notes" => PitchSource::Notes,
            _ => PitchSource::Model,
        };
        // every singer must be usable by the engine
        let mut chosen: HashMap<&str, &SingerInfo> = HashMap::new();
        for p in &req.parts {
            let s = singers
                .iter()
                .find(|s| s.id == p.singer)
                .ok_or_else(|| Error::Other(format!("{}: cantante «{}» non trovato", p.name, p.singer)))?;
            if !s.engine {
                return Err(Error::Other(format!(
                    "{}: {} non può cantare in Muse Sing ({})",
                    p.name,
                    s.name,
                    s.engine_note.as_deref().unwrap_or("non supportato")
                )));
            }
            chosen.insert(p.name.as_str(), s);
        }

        let src = PathBuf::from(&req.path);
        let sc = score::read_mscz(&src)?;
        let names: Vec<String> = req.parts.iter().map(|p| p.name.clone()).collect();
        let hints = HashMap::new();
        let fallback = src.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let project = ustx::build(
            &sc,
            &ustx::Options { lang: req.lang, singer: "", part_singers: None, parts: Some(&names), hints: &hints, fallback_name: &fallback },
        );
        let axis = ds::project_axis(&project);
        let tempo_key: String = project.tempos.iter().map(|t| format!("{}:{};", t.position, t.bpm)).collect();
        let parts = ds::project_notes(&project);
        if parts.is_empty() {
            return Err(Error::Other("nessuna delle voci scelte ha il testo".into()));
        }

        let mut warnings = vec![];
        let mut tracks = vec![];
        let n_parts = parts.len();
        for (index, (name, notes)) in parts.iter().enumerate() {
            if !self.is_current(job) {
                return Ok(None);
            }
            let singer = chosen.get(name.as_str()).ok_or_else(|| Error::Other(format!("{name}: nessun cantante scelto")))?;
            let color = req.parts.iter().find(|p| &p.name == name).and_then(|p| p.color.clone()).filter(|c| !c.is_empty());
            let dir = PathBuf::from(&singer.path);
            let notes_key: String = notes.iter().map(|n| format!("{},{},{},{};", n.position, n.duration, n.tone, n.lyric)).collect();
            let key = fnv(&format!(
                "{ENGINE_VERSION}|{}|{}|{}|{:?}|{:?}|{}|{tempo_key}|{notes_key}",
                singer.path,
                mtime(&dir.join("dsconfig.yaml")),
                mtime(&dir.join("character.yaml")),
                color,
                pitch,
                req.lang.code(),
            ));
            let cache_file = self.cache_dir.as_ref().map(|d| d.join(format!("{key:016x}.f32")));
            let report = |phrase: usize, phrases: usize, cached: bool| {
                progress(Progress { job, part: name.clone(), index, parts: n_parts, phrase, phrases, cached });
            };
            crate::log!("{name}: cantante {} ({}), colore {:?}", singer.name, singer.path, color);
            if let Some(samples) = cache_file.as_deref().and_then(read_cache) {
                crate::log!("{name}: dalla cache");
                report(1, 1, true);
                tracks.push(Track { name: name.clone(), samples: Arc::new(samples) });
                continue;
            }
            report(0, 1, false);
            let vb = self.voicebank(&dir)?;
            let settings = Settings { pitch, speaker: color.clone(), ..Settings::default() };
            let speaker = vb.speaker_suffix(color.as_deref());
            let t0 = std::time::Instant::now();
            let ph = ds::phonemize::phonemize(&vb, req.lang, notes, &axis, &speaker).map_err(|e| Error::Other(format!("{name}: {e}")))?;
            crate::log!("{name}: {} note, {} fonemi in {:.1} s, {} avvisi", notes.len(), ph.phones.len(), t0.elapsed().as_secs_f64(), ph.warnings.len());
            warnings.extend(ph.warnings.iter().map(|w| format!("{name}: {w}")));
            let rendered = ds::render::render_track(&vb, &ph, notes, &axis, &settings, |i, n| {
                report(i, n, false);
                self.is_current(job)
            })
            .map_err(|e| Error::Other(format!("{name}: {e}")))?;
            let Some((samples, w)) = rendered else { return Ok(None) };
            warnings.extend(w.into_iter().map(|w| format!("{name}: {w}")));
            if let Some(f) = &cache_file {
                write_cache(f, &samples);
            }
            tracks.push(Track { name: name.clone(), samples: Arc::new(samples) });
        }
        if !self.is_current(job) {
            return Ok(None);
        }
        let seconds = tracks.iter().map(|t| t.samples.len()).max().unwrap_or(0) as f64 / ds::render::SAMPLE_RATE as f64;
        *lock(&self.tracks) = tracks;
        *lock(&self.title) = project.name.clone();
        let _ = self.job.compare_exchange(job, 0, Ordering::Relaxed, Ordering::Relaxed);
        if let Some(d) = &self.cache_dir {
            trim_cache(d, CACHE_MAX_BYTES);
        }
        Ok(Some((warnings, seconds)))
    }
}

/// Cache file: little-endian f32 samples at 44.1 kHz.
fn read_cache(path: &Path) -> Option<Vec<f32>> {
    let bytes = std::fs::read(path).ok()?;
    let samples = bytes.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect();
    // touched, so trimming removes the least recently used files first
    let _ = std::fs::File::options().append(true).open(path).and_then(|f| f.set_modified(std::time::SystemTime::now()));
    Some(samples)
}

fn write_cache(path: &Path, samples: &[f32]) {
    let Some(dir) = path.parent() else { return };
    let _ = std::fs::create_dir_all(dir);
    let bytes: Vec<u8> = samples.iter().flat_map(|x| x.to_le_bytes()).collect();
    let tmp = path.with_extension("part");
    if std::fs::write(&tmp, bytes).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

fn trim_cache(dir: &Path, max: u64) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            Some((m.modified().ok()?, m.len(), e.path()))
        })
        .collect();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    files.sort();
    for (_, len, p) in files {
        if total <= max {
            break;
        }
        if std::fs::remove_file(&p).is_ok() {
            total -= len;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_round_trip_and_trim() {
        let dir = std::env::temp_dir().join(format!("muse-sing-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let a = dir.join("a.f32");
        write_cache(&a, &[0.5, -0.25, 1.0]);
        assert_eq!(read_cache(&a).unwrap(), vec![0.5, -0.25, 1.0]);
        std::thread::sleep(std::time::Duration::from_millis(20));
        write_cache(&dir.join("b.f32"), &[0.0; 4]);
        trim_cache(&dir, 16);
        assert!(!a.exists(), "oldest removed first");
        assert!(dir.join("b.f32").exists());
        assert_ne!(fnv("a"), fnv("b"));
        std::fs::remove_dir_all(dir).ok();
    }
}
