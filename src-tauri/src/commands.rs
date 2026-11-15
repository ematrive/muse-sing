use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::error::{Error, Result};
use crate::lyrics::{detect_lang, word_lyrics, Lang};
use crate::score::read_mscz;
use crate::singers::{self, SingerInfo};
use crate::ustx;

#[derive(Serialize)]
pub struct PartInfo {
    pub name: String,
    pub notes: usize,
    pub syllables: usize,
    pub sung: bool,
    /// First lyrics as they will be written (word / + / +~ / hints), for a quick check.
    pub preview: String,
}

#[derive(Serialize)]
pub struct Analysis {
    pub path: String,
    pub file_name: String,
    pub title: String,
    pub lang: Lang,
    pub time_sig: String,
    pub bpm: f64,
    pub duration_sec: f64,
    pub parts: Vec<PartInfo>,
    pub warnings: Vec<String>,
    pub default_output: String,
}

fn stem(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "brano".into())
}

#[tauri::command]
pub fn analyze(path: String) -> Result<Analysis> {
    let p = PathBuf::from(&path);
    if p.extension().and_then(|e| e.to_str()).map(str::to_lowercase).as_deref() != Some("mscz") {
        return Err(Error::Format("serve un file MuseScore (.mscz)".into()));
    }
    let score = read_mscz(&p)?;
    let lang = detect_lang(&score.parts);
    let no_hints = HashMap::new();
    let parts = score
        .parts
        .iter()
        .filter(|p| !p.notes.is_empty())
        .map(|part| {
            let sung = part.is_sung();
            let preview = if sung {
                word_lyrics(&part.notes, lang, &no_hints)
                    .into_iter()
                    .filter(|l| !l.starts_with('+'))
                    .take(12)
                    .collect::<Vec<_>>()
                    .join(" ")
            } else {
                String::new()
            };
            PartInfo {
                name: part.name.clone(),
                notes: part.notes.len(),
                syllables: part.notes.iter().filter(|n| n.lyric.is_some()).count(),
                sung,
                preview,
            }
        })
        .collect();
    // duration from the tempo map
    let end = score.parts.iter().flat_map(|p| p.notes.last()).map(|n| n.start + n.dur).max().unwrap_or_default();
    let to_f = |r: crate::score::Beats| *r.numer() as f64 / *r.denom() as f64;
    let mut secs = 0.0;
    let mut last = (0.0, score.tempos[0].1);
    for (b, q) in &score.tempos {
        let b = to_f(*b);
        if b >= to_f(end) {
            break;
        }
        secs += (b - last.0) / last.1;
        last = (b, *q);
    }
    secs += (to_f(end) - last.0) / last.1;
    let q = score.tempos[0].1;
    let (n, d) = score.time_sig;
    let bpm = if d == 8 && n % 3 == 0 { q * 60.0 / 1.5 } else { q * 60.0 };
    let out = p.with_extension("ustx");
    Ok(Analysis {
        file_name: p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
        title: score.title.clone().filter(|t| !t.is_empty()).unwrap_or_else(|| stem(&p)),
        path,
        lang,
        time_sig: format!("{n}/{d}"),
        bpm: (bpm * 10.0).round() / 10.0,
        duration_sec: secs.round(),
        parts,
        warnings: score.warnings,
        default_output: out.to_string_lossy().to_string(),
    })
}

#[derive(Serialize)]
pub struct SingersList {
    pub singers: Vec<SingerInfo>,
    pub folders: Vec<String>,
    /// Muse Sing's own Singers folder (voicebanks that only Muse Sing uses).
    pub own_folder: Option<String>,
    /// Suggested singer id per language (saved choice, else a sensible default).
    pub selected: HashMap<String, String>,
    /// Saved choice per part name (lowercase), across pieces.
    pub parts: HashMap<String, PartPref>,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct PartPref {
    pub singer: String,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
struct Prefs {
    #[serde(default)]
    singer: HashMap<String, String>,
    #[serde(default)]
    parts: HashMap<String, PartPref>,
    #[serde(default)]
    recent_voices: Vec<RecentVoice>,
}

fn prefs_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("prefs.json"))
}

/// Identifier of versions up to 0.3.0: their preferences live in a sibling config folder.
const OLD_IDENTIFIER: &str = "it.sintattica.musesing";

/// Reads `prefs.json`; if missing, copies the one of the old identifier next to it (recent singers,
/// voicebank per language), so updating from 0.2/0.3 keeps them.
fn read_prefs_text(path: &Path) -> Option<String> {
    if let Ok(t) = std::fs::read_to_string(path) {
        return Some(t);
    }
    let old = path.parent()?.parent()?.join(OLD_IDENTIFIER).join("prefs.json");
    let t = std::fs::read_to_string(old).ok()?;
    let _ = std::fs::create_dir_all(path.parent()?);
    let _ = std::fs::write(path, &t);
    Some(t)
}

fn load_prefs(app: &tauri::AppHandle) -> Prefs {
    prefs_path(app)
        .and_then(|p| read_prefs_text(&p))
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_prefs(app: &tauri::AppHandle, prefs: &Prefs) {
    if let Some(p) = prefs_path(app) {
        let _ = std::fs::create_dir_all(p.parent().unwrap());
        let _ = std::fs::write(p, serde_json::to_string_pretty(prefs).unwrap_or_default());
    }
}

/// `<app data>/Singers`, created on first use.
fn own_singers_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    let d = app.path().app_data_dir().ok()?.join("Singers");
    std::fs::create_dir_all(&d).ok()?;
    Some(d)
}

fn all_singers(app: &tauri::AppHandle) -> Vec<SingerInfo> {
    singers::list_singers(own_singers_dir(app).as_deref())
}

#[tauri::command]
pub async fn list_singers(app: tauri::AppHandle) -> Result<SingersList> {
    // reading the voicebanks' configuration takes a moment: off the main thread
    tauri::async_runtime::spawn_blocking(move || {
        let singers = all_singers(&app);
        let prefs = load_prefs(&app);
        let mut selected = HashMap::new();
        for lang in ["it", "en"] {
            let saved = prefs.singer.get(lang).filter(|id| singers.iter().any(|s| &s.id == *id)).cloned();
            if let Some(id) = saved.or_else(|| singers::suggest(&singers, lang, true)).or_else(|| singers::suggest(&singers, lang, false)) {
                selected.insert(lang.to_string(), id);
            }
        }
        let parts = prefs.parts.into_iter().filter(|(_, p)| singers.iter().any(|s| s.id == p.singer)).collect();
        SingersList {
            folders: singers::singer_roots().iter().map(|p| p.to_string_lossy().to_string()).collect(),
            own_folder: own_singers_dir(&app).map(|p| p.to_string_lossy().to_string()),
            singers,
            selected,
            parts,
        }
    })
    .await
    .map_err(|e| Error::Other(format!("cantanti: {e}")))
}

/// Remembers the singer of each part (by name) and the language's default singer.
fn remember_singers(app: &tauri::AppHandle, lang: Lang, parts: &[(String, PartPref)]) {
    let mut prefs = load_prefs(app);
    if let Some((_, first)) = parts.first() {
        prefs.singer.insert(lang.code().to_string(), first.singer.clone());
    }
    for (name, p) in parts {
        prefs.parts.insert(name.to_lowercase(), p.clone());
    }
    save_prefs(app, &prefs);
}

#[derive(Deserialize)]
pub struct GenerateRequest {
    pub path: String,
    pub output: String,
    pub lang: Lang,
    pub singer: String,
    pub parts: Vec<String>,
    /// Singer id per part, overriding `singer`.
    #[serde(default)]
    pub part_singers: HashMap<String, String>,
}

#[derive(Serialize)]
pub struct GenerateResult {
    pub output: String,
    pub tracks: usize,
    pub notes: usize,
}

#[tauri::command]
pub fn generate(app: tauri::AppHandle, req: GenerateRequest) -> Result<GenerateResult> {
    let src = PathBuf::from(&req.path);
    let score = read_mscz(&src)?;
    if req.singer.trim().is_empty() && req.part_singers.is_empty() {
        return Err(Error::Other("scegli il cantante".into()));
    }
    if req.parts.is_empty() {
        return Err(Error::Other("scegli almeno una voce".into()));
    }
    let own: Vec<&String> = req.part_singers.values().chain([&req.singer]).filter(|s| s.starts_with(singers::OWN_PREFIX)).collect();
    if let Some(s) = own.first() {
        return Err(Error::Other(format!(
            "«{}» è nella cartella cantanti di Muse Sing, OpenUtau non lo vede: per il .ustx scegli un cantante installato in OpenUtau",
            s.trim_start_matches(singers::OWN_PREFIX)
        )));
    }
    let hints = HashMap::new();
    let project = ustx::build(
        &score,
        &ustx::Options {
            lang: req.lang,
            singer: &req.singer,
            part_singers: Some(&req.part_singers),
            parts: Some(&req.parts),
            hints: &hints,
            fallback_name: &stem(&src),
        },
    );
    let mut out = PathBuf::from(&req.output);
    if out.extension().is_none() {
        out.set_extension("ustx");
    }
    std::fs::write(&out, ustx::to_yaml(&project))?;
    let chosen: Vec<(String, PartPref)> = req
        .parts
        .iter()
        .map(|p| (p.clone(), PartPref { singer: req.part_singers.get(p).unwrap_or(&req.singer).clone(), color: None }))
        .filter(|(_, p)| !p.singer.is_empty())
        .collect();
    remember_singers(&app, req.lang, &chosen);
    Ok(GenerateResult {
        output: out.to_string_lossy().to_string(),
        tracks: project.tracks.len(),
        notes: project.voice_parts.iter().map(|p| p.notes.len()).sum(),
    })
}

// ---------------------------------------------------------------------------------------------
// Recording a voicebank

use std::sync::{Arc, Mutex};
use tauri::{Emitter, State};

use crate::audio::{self, Engine};
use crate::corpus::VoiceProfile;
use crate::voice::{self, Take, VoiceState};

#[derive(Serialize, Deserialize, Clone)]
pub struct RecentVoice {
    pub dir: String,
    pub name: String,
}

fn remember_voice(app: &tauri::AppHandle, st: &VoiceState) {
    let mut prefs = load_prefs(app);
    prefs.recent_voices.retain(|v| v.dir != st.dir);
    prefs.recent_voices.insert(0, RecentVoice { dir: st.dir.clone(), name: st.profile.name.clone() });
    prefs.recent_voices.truncate(8);
    save_prefs(app, &prefs);
}

#[tauri::command]
pub fn voice_recent(app: tauri::AppHandle) -> Vec<RecentVoice> {
    load_prefs(&app).recent_voices.into_iter().filter(|v| Path::new(&v.dir).join("profile.json").exists()).collect()
}

#[tauri::command]
pub fn voice_create(app: tauri::AppHandle, parent: String, profile: VoiceProfile) -> Result<VoiceState> {
    let st = voice::create(Path::new(&parent), profile)?;
    remember_voice(&app, &st);
    Ok(st)
}

#[tauri::command]
pub fn voice_open(app: tauri::AppHandle, dir: String) -> Result<VoiceState> {
    let st = voice::open(Path::new(&dir))?;
    remember_voice(&app, &st);
    Ok(st)
}

#[tauri::command]
pub fn voice_add_piece(dir: String, path: String, part: String) -> Result<VoiceState> {
    voice::add_piece(Path::new(&dir), Path::new(&path), &part)
}

#[tauri::command]
pub fn voice_remove_group(dir: String, group: String) -> Result<VoiceState> {
    voice::remove_group(Path::new(&dir), &group)
}

/// The audio engine, shared by the commands.
pub struct AudioState(pub Arc<Engine>);

impl AudioState {
    pub fn new(app: tauri::AppHandle) -> Self {
        let notify: audio::Notify = Arc::new(move |ev| {
            let _ = match ev {
                audio::Event::Level(l) => app.emit("audio-level", l),
                audio::Event::Device(msg) => app.emit("audio-device", msg),
            };
        });
        AudioState(Engine::new(notify))
    }
}

#[derive(Serialize, Clone)]
pub struct AudioDone {
    pub job: u64,
    pub kind: &'static str,
    pub id: String,
    pub take: Option<Take>,
    pub error: Option<String>,
    pub cancelled: bool,
}

/// Runs blocking audio work off the main thread.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| Error::Other(format!("audio: {e}")))?
}

/// Opens the devices it needs, stops the current job and runs `body` on an audio thread; progress
/// and the result arrive as `audio-progress` / `audio-done` events carrying the returned job id.
async fn run_job<F>(app: tauri::AppHandle, engine: Arc<Engine>, kind: &'static str, id: String, mic: bool, body: F) -> Result<u64>
where
    F: FnOnce(&Engine, u64, &dyn Fn(audio::Progress)) -> Result<Option<Take>> + Send + 'static,
{
    blocking(move || {
        engine.ensure(mic)?;
        let job = engine.begin_job(kind == "take");
        std::thread::spawn(move || {
            let emit = |p: audio::Progress| {
                let _ = app.emit("audio-progress", p);
            };
            let res = body(&engine, job, &emit);
            let cancelled = !engine.is_current(job);
            engine.end_job(job);
            let done = match res {
                Ok(take) => AudioDone { job, kind, id, take, error: None, cancelled },
                Err(e) => AudioDone { job, kind, id, take: None, error: Some(e.to_string()), cancelled },
            };
            let _ = app.emit("audio-done", done);
        });
        Ok(job)
    })
    .await
}

#[tauri::command]
pub async fn audio_devices() -> Result<audio::Devices> {
    blocking(audio::list_devices).await
}

/// Opens microphone and output for a recording session (None = automatic choice).
#[tauri::command]
pub async fn audio_open(state: State<'_, AudioState>, input: Option<String>, output: Option<String>) -> Result<audio::Status> {
    let engine = state.0.clone();
    blocking(move || engine.open(input, output)).await
}

/// Closes the devices (leaving the session or the tab).
#[tauri::command]
pub async fn audio_close(state: State<'_, AudioState>) -> Result<()> {
    let engine = state.0.clone();
    blocking(move || {
        engine.close();
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn audio_guide(
    app: tauri::AppHandle,
    state: State<'_, AudioState>,
    dir: String,
    id: String,
    from_beats: f64,
    speed: f64,
    looping: bool,
) -> Result<u64> {
    let ph = voice::phrase(Path::new(&dir), &id)?;
    run_job(app, state.0.clone(), "guide", id, false, move |e, job, emit| {
        e.play_guide(job, &ph, from_beats, speed, looping, emit)?;
        Ok(None)
    })
    .await
}

#[tauri::command]
pub async fn audio_record(
    app: tauri::AppHandle,
    state: State<'_, AudioState>,
    dir: String,
    id: String,
    guide: bool,
    start_note: bool,
) -> Result<u64> {
    let ph = voice::phrase(Path::new(&dir), &id)?;
    let dir = PathBuf::from(dir);
    let take_id = id.clone();
    run_job(app, state.0.clone(), "take", id, true, move |e, job, emit| match e.record_take(job, &ph, guide, start_note, emit)? {
        Some(rec) => Ok(Some(voice::save_take(&dir, &take_id, &rec)?)),
        None => Ok(None),
    })
    .await
}

#[tauri::command]
pub async fn audio_play_take(app: tauri::AppHandle, state: State<'_, AudioState>, dir: String, id: String, from_sec: f64) -> Result<u64> {
    let path = voice::take_path(Path::new(&dir), &id)?;
    let bpm = voice::phrase(Path::new(&dir), &id)?.bpm;
    run_job(app, state.0.clone(), "play", id, false, move |e, job, emit| {
        e.play_file(job, &path, from_sec, bpm, emit)?;
        Ok(None)
    })
    .await
}

/// One note of the piano roll (not while recording).
#[tauri::command]
pub async fn audio_note(state: State<'_, AudioState>, pitch: i32, seconds: f64) -> Result<()> {
    let engine = state.0.clone();
    blocking(move || {
        engine.ensure(false)?;
        engine.play_note(pitch, seconds)
    })
    .await
}

#[tauri::command]
pub fn audio_stop(state: State<AudioState>) {
    state.0.stop();
}

// ---------------------------------------------------------------------------------------------
// Sung audio (Muse Sing's DiffSinger engine)

use crate::mix::{self, MixPart};
use crate::sing::{self, RenderRequest, Sing};

pub struct SingState(pub Arc<Sing>, pub Mutex<u64>);

impl SingState {
    pub fn new(app: &tauri::AppHandle) -> Self {
        SingState(Arc::new(Sing::new(app.path().app_cache_dir().ok())), Mutex::new(0))
    }
}

/// Starts rendering the parts; returns the job id carried by `sing-progress` / `sing-done`.
#[tauri::command]
pub fn sing_start(app: tauri::AppHandle, state: State<SingState>, req: RenderRequest) -> Result<u64> {
    let sing = state.0.clone();
    let job = sing.begin();
    std::thread::spawn(move || {
        let singers = all_singers(&app);
        let emit = |p: sing::Progress| {
            let _ = app.emit("sing-progress", p);
        };
        let t0 = std::time::Instant::now();
        let res = sing.run(job, &req, &singers, &emit);
        match &res {
            Ok(Some((w, secs))) => crate::log!("genera audio: finito, {secs:.1} s di audio in {:.1} s, {} avvisi", t0.elapsed().as_secs_f64(), w.len()),
            Ok(None) => crate::log!("genera audio: annullato"),
            Err(e) => crate::log!("genera audio: ERRORE {e}"),
        }
        let done = match res {
            Ok(Some((warnings, seconds))) => {
                let chosen: Vec<(String, PartPref)> =
                    req.parts.iter().map(|p| (p.name.clone(), PartPref { singer: p.singer.clone(), color: p.color.clone() })).collect();
                remember_singers(&app, req.lang, &chosen);
                sing::Done { job, error: None, cancelled: false, warnings, seconds }
            }
            Ok(None) => sing::Done { job, error: None, cancelled: true, warnings: vec![], seconds: 0.0 },
            Err(e) => sing::Done { job, error: Some(e.to_string()), cancelled: false, warnings: vec![], seconds: 0.0 },
        };
        let _ = app.emit("sing-done", done);
    });
    Ok(job)
}

#[tauri::command]
pub fn sing_cancel(state: State<SingState>) {
    state.0.cancel();
}

#[derive(Serialize, Clone)]
pub struct PlayProgress {
    pub job: u64,
    pub t: f64,
    pub total: f64,
}

#[derive(Serialize, Clone)]
pub struct PlayDone {
    pub job: u64,
    pub error: Option<String>,
}

/// Plays the mix of the rendered parts from `from_sec` (`sing-play` / `sing-play-done` events).
#[tauri::command]
pub async fn sing_play(
    app: tauri::AppHandle,
    audio: State<'_, AudioState>,
    state: State<'_, SingState>,
    mix: Vec<MixPart>,
    from_sec: f64,
) -> Result<u64> {
    let (sing, engine) = (state.0.clone(), audio.0.clone());
    let (l, r) = sing.with_tracks(|t| mix::mix(t, &mix))?;
    let job = blocking({
        let engine = engine.clone();
        move || {
            engine.ensure(false)?;
            Ok(engine.begin_job(false))
        }
    })
    .await?;
    *state.1.lock().unwrap_or_else(|e| e.into_inner()) = job;
    std::thread::spawn(move || {
        let progress = |t: f64, total: f64| {
            let _ = app.emit("sing-play", PlayProgress { job, t, total });
        };
        let res = engine.play_stereo(job, &l, &r, crate::ds::render::SAMPLE_RATE, from_sec, &progress);
        engine.end_job(job);
        let _ = app.emit("sing-play-done", PlayDone { job, error: res.err().map(|e| e.to_string()) });
    });
    Ok(job)
}

/// Stops the mix playback (not a recording started meanwhile).
#[tauri::command]
pub fn sing_stop(audio: State<AudioState>, state: State<SingState>) {
    let job = *state.1.lock().unwrap_or_else(|e| e.into_inner());
    if audio.0.is_current(job) {
        audio.0.stop();
    }
}

/// Exports the mix; the format follows the extension (.mp3, .flac, .wav).
#[tauri::command]
pub async fn sing_export(state: State<'_, SingState>, mix: Vec<MixPart>, path: String) -> Result<String> {
    let sing = state.0.clone();
    blocking(move || {
        let (l, r) = sing.with_tracks(|t| mix::mix(t, &mix))?;
        let title = sing.title();
        mix::export(Path::new(&path), &l, &r, crate::ds::render::SAMPLE_RATE, &title)?;
        Ok(path)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefs_recovered_from_old_identifier() {
        let base = std::env::temp_dir().join(format!("muse-sing-prefs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let new = base.join("app.musesing.desktop").join("prefs.json");
        assert_eq!(read_prefs_text(&new), None, "nothing anywhere");

        std::fs::create_dir_all(base.join(OLD_IDENTIFIER)).unwrap();
        std::fs::write(base.join(OLD_IDENTIFIER).join("prefs.json"), r#"{"singer":{"it":"x"}}"#).unwrap();
        let t = read_prefs_text(&new).unwrap();
        assert!(t.contains("\"it\""));
        assert!(new.exists(), "copied to the new folder");

        // the new file wins from now on
        std::fs::write(&new, "{}").unwrap();
        assert_eq!(read_prefs_text(&new).unwrap(), "{}");
        std::fs::remove_dir_all(base).ok();
    }
}
