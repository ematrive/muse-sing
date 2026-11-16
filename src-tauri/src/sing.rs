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

use crate::ds::{self, phonemize::split_hint, render::PitchSeg, render::PitchSource, render::Settings, voicebank::Voicebank};
use crate::edits::PartEdits;
use crate::error::{Error, Result};
use crate::lyrics::Lang;
use crate::singers::SingerInfo;
use crate::{score, ustx};

/// Changes whenever the engine's output for the same input changes, to invalidate the cache.
const ENGINE_VERSION: &str = "ds-3";
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
    /// 1.0 = written tempo, 0.8 = 80% (practice).
    #[serde(default = "one")]
    pub tempo_factor: f64,
    /// Add the instrumental parts, played by an organ sound.
    #[serde(default)]
    pub accompaniment: bool,
    /// Pronunciation and curve edits by part name.
    #[serde(default)]
    pub edits: HashMap<String, PartEdits>,
}

/// A phoneme of a sung word, for the pronunciation editor (seconds on the track).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhoneInfo {
    pub symbol: String,
    pub start: f64,
    pub end: f64,
    /// Start before the timing edits.
    pub base: f64,
}

/// A sung word: id of its first note, the word as the pronunciation table names it, its notes'
/// span and its phonemes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordInfo {
    pub id: String,
    pub word: String,
    pub start: f64,
    pub end: f64,
    pub phones: Vec<PhoneInfo>,
}

/// What the editor shows of a rendered part.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrackInfo {
    pub words: Vec<WordInfo>,
    pub pitch: Vec<PitchSeg>,
    /// Phonemes the singer knows (symbol, vowel).
    pub phonemes: Vec<(String, bool)>,
    /// The voicebank follows the timbre / consonant speed curves.
    pub gender: bool,
    pub velocity: bool,
}

fn one() -> f64 {
    1.0
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
    info: Mutex<HashMap<String, Arc<TrackInfo>>>,
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
            info: Mutex::new(HashMap::new()),
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

    /// Words, phonemes and pitch of a part of the last render.
    pub fn track_info(&self, part: &str) -> Option<Arc<TrackInfo>> {
        lock(&self.info).get(part).cloned()
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
        if req.parts.is_empty() && !req.accompaniment {
            return Err(Error::Other("scegli almeno una voce".into()));
        }
        if !(0.25..=2.0).contains(&req.tempo_factor) {
            return Err(Error::Other(format!("velocità {:.0}% non valida", req.tempo_factor * 100.0)));
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
        crate::log!("velocità {:.0}%, accompagnamento {}", req.tempo_factor * 100.0, if req.accompaniment { "sì" } else { "no" });
        let no_hints = HashMap::new();
        let fallback = src.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        fn opts<'a>(lang: Lang, hints: &'a HashMap<String, String>, parts: &'a [String], fallback: &'a str) -> ustx::Options<'a> {
            ustx::Options { lang, singer: "", part_singers: None, parts: Some(parts), hints, fallback_name: fallback }
        }
        let mut project = ustx::build(&sc, &opts(req.lang, &no_hints, &names, &fallback));
        for t in &mut project.tempos {
            t.bpm *= req.tempo_factor;
        }
        let axis = ds::project_axis(&project);
        // ticks of the written score (curves are stored in them) at the practice tempo
        let tempo_key: String = project.tempos.iter().map(|t| format!("{}:{};", t.position, t.bpm)).collect();
        let no_edits = PartEdits::default();
        let mut req_edits = req.edits.clone();
        for e in req_edits.values_mut() {
            for c in [&mut e.pitch, &mut e.dynamics, &mut e.gender, &mut e.velocity] {
                *c = std::mem::take(c).normalized(0.0);
            }
        }
        let req_edits = &req_edits;
        let mut parts = ds::project_notes(&project);
        for (name, notes) in &mut parts {
            let e = req_edits.get(name.as_str()).unwrap_or(&no_edits);
            if !e.words.is_empty() {
                let hints: HashMap<String, String> = e.words.iter().filter(|(_, v)| !v.trim().is_empty()).map(|(k, v)| (k.clone(), v.clone())).collect();
                let one = [name.clone()];
                if let Some((_, n)) = ds::project_notes(&ustx::build(&sc, &opts(req.lang, &hints, &one, &fallback))).into_iter().next() {
                    *notes = n;
                }
            }
            for n in notes.iter_mut().filter(|n| !n.lyric.starts_with('+')) {
                let Some(w) = e.notes.get(&n.id) else { continue };
                let word = split_hint(&n.lyric).0;
                if !w.word.is_empty() && w.word != word {
                    continue;
                }
                if let Some(ph) = w.phonemes.as_deref().filter(|p| !p.trim().is_empty()) {
                    n.lyric = format!("{word}[{}]", ph.trim());
                }
            }
        }
        if parts.is_empty() && !req.accompaniment {
            return Err(Error::Other("nessuna delle voci scelte ha il testo".into()));
        }

        let mut warnings = vec![];
        let mut tracks = vec![];
        let mut infos = HashMap::new();
        let n_parts = parts.len();
        for (index, (name, notes)) in parts.iter().enumerate() {
            if !self.is_current(job) {
                return Ok(None);
            }
            let singer = chosen.get(name.as_str()).ok_or_else(|| Error::Other(format!("{name}: nessun cantante scelto")))?;
            let color = req.parts.iter().find(|p| &p.name == name).and_then(|p| p.color.clone()).filter(|c| !c.is_empty());
            let dir = PathBuf::from(&singer.path);
            let edits = req_edits.get(name.as_str()).unwrap_or(&no_edits);
            let notes_key: String = notes.iter().map(|n| format!("{},{},{},{},{};", n.id, n.position, n.duration, n.tone, n.lyric)).collect();
            let salt = format!(
                "{ENGINE_VERSION}|{}|{}|{}|{:?}|{:?}|{}",
                singer.path,
                mtime(&dir.join("dsconfig.yaml")),
                mtime(&dir.join("character.yaml")),
                color,
                pitch,
                req.lang.code(),
            );
            let key = fnv(&format!("{salt}|{tempo_key}|{notes_key}|{}", edits.render_key()));
            let cache_file = self.cache_dir.as_ref().map(|d| d.join(format!("{key:016x}.f32")));
            let info_file = cache_file.as_ref().map(|f| f.with_extension("json"));
            let report = |phrase: usize, phrases: usize, cached: bool| {
                progress(Progress { job, part: name.clone(), index, parts: n_parts, phrase, phrases, cached });
            };
            crate::log!("{name}: cantante {} ({}), colore {:?}", singer.name, singer.path, color);
            let hit = cache_file.as_deref().and_then(read_cache).zip(info_file.as_deref().and_then(read_info));
            if let Some((mut samples, info)) = hit {
                crate::log!("{name}: dalla cache");
                report(1, 1, true);
                apply_dynamics(&mut samples, edits, &axis);
                tracks.push(Track { name: name.clone(), samples: Arc::new(samples) });
                infos.insert(name.clone(), Arc::new(info));
                continue;
            }
            report(0, 1, false);
            let vb = self.voicebank(&dir)?;
            let settings = Settings { pitch, speaker: color.clone(), ..Settings::default() };
            let speaker = vb.speaker_suffix(color.as_deref());
            let t0 = std::time::Instant::now();
            let mut ph = ds::phonemize::phonemize(&vb, req.lang, notes, &axis, &speaker).map_err(|e| Error::Other(format!("{name}: {e}")))?;
            crate::log!("{name}: {} note, {} fonemi in {:.1} s, {} avvisi", notes.len(), ph.phones.len(), t0.elapsed().as_secs_f64(), ph.warnings.len());
            warnings.extend(ph.warnings.iter().map(|w| format!("{name}: {w}")));
            let base: Vec<i64> = ph.phones.iter().map(|p| p.position).collect();
            let word_of: HashMap<&str, String> =
                notes.iter().filter(|n| !n.lyric.starts_with('+')).map(|n| (n.id.as_str(), split_hint(&n.lyric).0)).collect();
            let matches = |id: &str, w: &crate::edits::WordEdit| word_of.get(id).is_some_and(|x| w.word.is_empty() || *x == w.word);
            for (id, w) in edits.notes.iter().filter(|(id, w)| !w.is_empty() && !matches(id, w)) {
                let bar = id.split('.').next().unwrap_or("?");
                warnings.push(format!("{name}: ritocco di «{}» (battuta {bar}) non applicato: lì la partitura ora ha un'altra parola", w.word));
            }
            let offsets: HashMap<String, Vec<f64>> = edits
                .notes
                .iter()
                .filter(|(id, w)| w.offsets.iter().any(|o| *o != 0.0) && matches(id, w))
                .map(|(k, w)| (k.clone(), w.offsets.clone()))
                .collect();
            ds::phonemize::shift_phonemes(&mut ph, notes, &axis, &offsets);
            let curve = |ms: f64| {
                let tick = axis.ms_to_tick(ms) as f64;
                [edits.pitch.at(tick, 0.0), edits.gender.at(tick, 0.0), edits.velocity.at(tick, 0.0)]
            };
            let phrase_cache = self.cache_dir.as_ref().map(|d| DiskPhrases(d.clone()));
            let rendered = ds::render::render_track(
                &vb,
                &ph,
                notes,
                &axis,
                &settings,
                &curve,
                phrase_cache.as_ref().map(|c| (c as &dyn ds::render::PhraseCache, salt.as_str())),
                |i, n| {
                    report(i, n, false);
                    self.is_current(job)
                },
            )
            .map_err(|e| Error::Other(format!("{name}: {e}")))?;
            let Some(out) = rendered else { return Ok(None) };
            crate::log!("{name}: {} frasi sintetizzate ora, le altre dalla cache", out.rendered);
            warnings.extend(out.warnings.into_iter().map(|w| format!("{name}: {w}")));
            let info = TrackInfo {
                words: word_info(&ph, notes, &base, &axis),
                pitch: out.pitch,
                phonemes: ds::phonemize::singer_phonemes(&vb, req.lang).unwrap_or_default(),
                gender: vb.config.use_key_shift_embed,
                velocity: vb.config.use_speed_embed,
            };
            let mut samples = out.samples;
            if let (Some(f), Some(i)) = (&cache_file, &info_file) {
                write_cache(f, &samples);
                write_info(i, &info);
            }
            apply_dynamics(&mut samples, edits, &axis);
            tracks.push(Track { name: name.clone(), samples: Arc::new(samples) });
            infos.insert(name.clone(), Arc::new(info));
        }
        if !self.is_current(job) {
            return Ok(None);
        }
        if req.accompaniment {
            let samples = crate::accomp::render(&sc, &axis, ds::render::SAMPLE_RATE);
            if !samples.is_empty() {
                crate::log!("accompagnamento: {:.1} s", samples.len() as f64 / ds::render::SAMPLE_RATE as f64);
                tracks.push(Track { name: crate::project::ACCOMPANIMENT.into(), samples: Arc::new(samples) });
            }
        }
        if tracks.is_empty() {
            return Err(Error::Other("niente da suonare: la partitura non ha né voci scelte né parti strumentali".into()));
        }
        let seconds = tracks.iter().map(|t| t.samples.len()).max().unwrap_or(0) as f64 / ds::render::SAMPLE_RATE as f64;
        *lock(&self.tracks) = tracks;
        *lock(&self.info) = infos;
        *lock(&self.title) = project.name.clone();
        let _ = self.job.compare_exchange(job, 0, Ordering::Relaxed, Ordering::Relaxed);
        if let Some(d) = &self.cache_dir {
            trim_cache(d, CACHE_MAX_BYTES);
        }
        Ok(Some((warnings, seconds)))
    }
}

/// Rendered phrases in the cache folder: `p<key>.f32` = frames of pitch, pitch, samples.
struct DiskPhrases(PathBuf);

impl ds::render::PhraseCache for DiskPhrases {
    fn get(&self, key: u64) -> Option<(Vec<f32>, Vec<f32>)> {
        let all = read_cache(&self.0.join(format!("p{key:016x}.f32")))?;
        let n = *all.first()? as usize;
        (all.len() > n).then(|| (all[1 + n..].to_vec(), all[1..1 + n].to_vec()))
    }

    fn put(&self, key: u64, samples: &[f32], midi: &[f32]) {
        let mut all = Vec::with_capacity(1 + midi.len() + samples.len());
        all.push(midi.len() as f32);
        all.extend_from_slice(midi);
        all.extend_from_slice(samples);
        write_cache(&self.0.join(format!("p{key:016x}.f32")), &all);
    }
}

/// Words and phonemes of a part in seconds (`base`: phoneme starts before the timing edits).
fn word_info(ph: &ds::phonemize::Phonemized, notes: &[ds::phonemize::PNote], base: &[i64], axis: &ds::time::TimeAxis) -> Vec<WordInfo> {
    let sec = |t: i64| axis.tick_to_ms(t as f64) / 1000.0;
    let mut words: Vec<WordInfo> = vec![];
    for (k, p) in ph.phones.iter().enumerate() {
        let g = &ph.groups[p.group];
        let head = &notes[g[0]];
        if words.last().is_none_or(|w| w.id != head.id) {
            let last = &notes[*g.last().unwrap()];
            words.push(WordInfo {
                id: head.id.clone(),
                word: split_hint(&head.lyric).0,
                start: sec(head.position),
                end: sec(last.end()),
                phones: vec![],
            });
        }
        words.last_mut().unwrap().phones.push(PhoneInfo { symbol: p.symbol.clone(), start: sec(p.position), end: sec(p.end), base: sec(base[k]) });
    }
    words
}

/// Volume curve of the edits (dB), applied to a rendered part in 10 ms steps.
fn apply_dynamics(samples: &mut [f32], edits: &PartEdits, axis: &ds::time::TimeAxis) {
    if edits.dynamics.is_empty() {
        return;
    }
    let sr = ds::render::SAMPLE_RATE as usize;
    let block = sr / 100;
    let gain_at = |i: usize| {
        let tick = axis.ms_to_tick(i as f64 * 1000.0 / sr as f64) as f64;
        10f32.powf(edits.dynamics.at(tick, 0.0).clamp(-60.0, 24.0) / 20.0)
    };
    let mut g0 = gain_at(0);
    for (b, chunk) in samples.chunks_mut(block).enumerate() {
        let g1 = gain_at((b + 1) * block);
        let n = chunk.len() as f32;
        for (k, x) in chunk.iter_mut().enumerate() {
            *x *= g0 + (g1 - g0) * k as f32 / n;
        }
        g0 = g1;
    }
}

fn read_info(path: &Path) -> Option<TrackInfo> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

fn write_info(path: &Path, info: &TrackInfo) {
    if let Ok(text) = serde_json::to_vec(info) {
        let _ = std::fs::write(path, text);
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
