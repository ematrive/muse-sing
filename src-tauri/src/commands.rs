use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::error::{Error, Result};
use crate::fx::Ambience;
use crate::lyrics::{detect_lang, word_lyrics, Lang};
use crate::score::read_score;
use crate::singers::{self, SingerInfo};

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
    /// Original file path without extension, to propose names for exports and projects.
    pub default_output: String,
}

fn stem(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "brano".into())
}

#[tauri::command]
pub fn analyze(path: String) -> Result<Analysis> {
    let p = PathBuf::from(&path);
    analysis_of(&p, &p)
}

/// Analysis of the score at `p`, shown with the name of `display` (the original file, when `p`
/// is a project's working copy).
fn analysis_of(p: &Path, display: &Path) -> Result<Analysis> {
    let path = p.to_string_lossy().into_owned();
    let score = read_score(p)?;
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
    // the tempo mark refers to the opening time signature (dotted quarter in 6/8, 9/8, 12/8)
    let (n, d) = score.time_sig;
    let bpm = if d == 8 && n % 3 == 0 { q * 60.0 / 1.5 } else { q * 60.0 };
    let mut sigs: Vec<String> = score.time_sigs.iter().take(3).map(|(n, d)| format!("{n}/{d}")).collect();
    if sigs.is_empty() {
        sigs.push(format!("{n}/{d}"));
    }
    if score.time_sigs.len() > 3 {
        sigs.push("…".into());
    }
    let out = display.with_extension("");
    Ok(Analysis {
        file_name: display.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
        title: score.title.clone().filter(|t| !t.is_empty()).unwrap_or_else(|| stem(display)),
        path,
        lang,
        time_sig: sigs.join(", "),
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
    #[serde(default)]
    recent_projects: Vec<RecentProject>,
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

/// Event with the progress of `install_tiger` (`tiger::Progress`).
pub const TIGER_EVENT: &str = "tiger-progress";

static TIGER_BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Downloads TIGER DS from its official release and installs it in Muse Sing's Singers folder;
/// returns the new folder.
#[tauri::command]
pub async fn install_tiger(app: tauri::AppHandle) -> Result<String> {
    use std::sync::atomic::Ordering;
    if TIGER_BUSY.swap(true, Ordering::SeqCst) {
        return Err(Error::Other("Tiger si sta già scaricando".into()));
    }
    let res = async {
        let singers = own_singers_dir(&app).ok_or_else(|| Error::Other("cartella dei cantanti non disponibile".into()))?;
        let cache = app.path().app_cache_dir().map_err(|e| Error::Other(e.to_string()))?;
        std::fs::create_dir_all(&cache)?;
        let pack = cache.join(crate::tiger::PACK_FILE);
        let emit = {
            let app = app.clone();
            move |p: crate::tiger::Progress| {
                let _ = app.emit(TIGER_EVENT, p);
            }
        };
        if !pack.is_file() {
            crate::tiger::download(crate::tiger::PACK_URL, &pack, emit.clone()).await?;
        }
        let dest = tauri::async_runtime::spawn_blocking(move || {
            let r = crate::tiger::install_pack(&pack, &singers, &cache, emit);
            // a good pack is no longer needed; a broken one must be downloaded again
            let _ = std::fs::remove_file(&pack);
            r
        })
        .await
        .map_err(|e| Error::Other(format!("installazione di Tiger: {e}")))??;
        Ok(dest.to_string_lossy().to_string())
    }
    .await;
    TIGER_BUSY.store(false, Ordering::SeqCst);
    res
}

/// Where the SoundFont of the instruments is installed.
fn soundfont_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    Some(app.path().app_data_dir().ok()?.join("SoundFonts").join(crate::instruments::SOUNDFONT_FILE))
}

/// Whether the instruments (SoundFont) are installed.
#[tauri::command]
pub fn soundfont_installed(app: tauri::AppHandle) -> bool {
    soundfont_path(&app).is_some_and(|p| p.is_file())
}

/// Event with the progress of `install_soundfont` (`tiger::Progress`).
pub const SOUNDFONT_EVENT: &str = "soundfont-progress";

static SOUNDFONT_BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Downloads the instruments (GeneralUser GS, from Muse Sing's own copy) into the app data.
#[tauri::command]
pub async fn install_soundfont(app: tauri::AppHandle) -> Result<String> {
    use std::sync::atomic::Ordering;
    if SOUNDFONT_BUSY.swap(true, Ordering::SeqCst) {
        return Err(Error::Other("gli strumenti si stanno già scaricando".into()));
    }
    let res = async {
        let dest = soundfont_path(&app).ok_or_else(|| Error::Other("cartella dell'app non disponibile".into()))?;
        if let Some(dir) = dest.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let emit = {
            let app = app.clone();
            move |p: crate::tiger::Progress| {
                let _ = app.emit(SOUNDFONT_EVENT, p);
            }
        };
        crate::tiger::download(crate::instruments::SOUNDFONT_URL, &dest, emit).await?;
        // a file that doesn't load is no use: remove it so the download can be retried
        let check = dest.clone();
        let ok = tauri::async_runtime::spawn_blocking(move || crate::instruments::load(&check).map(|_| ()))
            .await
            .map_err(|e| Error::Other(format!("strumenti: {e}")))?;
        if let Err(e) = ok {
            let _ = std::fs::remove_file(&dest);
            return Err(e);
        }
        Ok(dest.to_string_lossy().to_string())
    }
    .await;
    SOUNDFONT_BUSY.store(false, Ordering::SeqCst);
    res
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
        for lang in ["it", "en", "la"] {
            // Latin falls back to the Italian choice
            let saved = [lang, if lang == "la" { "it" } else { lang }]
                .iter()
                .find_map(|l| prefs.singer.get(*l).filter(|id| singers.iter().any(|s| &s.id == *id)).cloned());
            if let Some(id) = saved.or_else(|| singers::suggest(&singers, lang, true)).or_else(|| singers::suggest(&singers, lang, false)) {
                selected.insert(lang.to_string(), id);
            }
        }
        let parts = prefs.parts.into_iter().filter(|(_, p)| singers.iter().any(|s| s.id == p.singer)).collect();
        SingersList {
            folders: singers::singer_roots().iter().filter(|p| p.is_dir()).map(|p| p.to_string_lossy().to_string()).collect(),
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
pub fn sing_start(app: tauri::AppHandle, state: State<SingState>, mut req: RenderRequest) -> Result<u64> {
    req.soundfont = soundfont_path(&app).filter(|p| p.is_file());
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

/// Words, phonemes and sung pitch of a part of the last render (None before a render).
#[tauri::command]
pub fn sing_track_info(state: State<SingState>, part: String) -> Option<sing::TrackInfo> {
    state.0.track_info(&part).map(|i| (*i).clone())
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
#[allow(clippy::too_many_arguments)] // the arguments of a Tauri command
pub async fn sing_play(
    app: tauri::AppHandle,
    audio: State<'_, AudioState>,
    state: State<'_, SingState>,
    mix: Vec<MixPart>,
    from_sec: f64,
    loop_range: Option<(f64, f64)>,
    ambience: Option<Ambience>,
    reverb: Option<f32>,
    count_in: Option<bool>,
) -> Result<u64> {
    let (sing, engine) = (state.0.clone(), audio.0.clone());
    let (mut l, mut r) = sing.with_tracks(|t| mix::mix_in(t, &mix, ambience.unwrap_or_default(), reverb.unwrap_or(crate::fx::DEFAULT_AMOUNT)))?;
    // a measure of clicks before the start (not when looping a range): the buffer then starts
    // with the clicks, and the reported position is moved back to the score's time
    let sr = crate::ds::render::SAMPLE_RATE;
    let (mut play_from, mut shift, mut counting) = (from_sec, 0.0, false);
    if count_in == Some(true) && loop_range.is_none() {
        let pre = sing.count_in(from_sec);
        let start = ((from_sec * sr as f64) as usize).min(l.len());
        let lead = pre.len() as f64 / sr as f64;
        l = pre.iter().copied().chain(l[start..].iter().copied()).collect();
        r = pre.iter().copied().chain(r[start..].iter().copied()).collect();
        (play_from, shift, counting) = (0.0, from_sec - lead, true);
    }
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
            // during the count-in the cursor waits at the start
            let t = if counting { (t + shift).max(from_sec) } else { t };
            let _ = app.emit("sing-play", PlayProgress { job, t, total: total + shift });
        };
        let res = engine.play_stereo(job, &l, &r, sr, play_from, loop_range, &progress);
        engine.end_job(job);
        let _ = app.emit("sing-play-done", PlayDone { job, error: res.err().map(|e| e.to_string()) });
    });
    Ok(job)
}

// ---------------------------------------------------------------------------------------------
// «Canta e controlla»

/// The last check's recording, to listen to it over the music (in memory only).
pub struct CheckTake {
    /// At `SAMPLE_RATE`.
    samples: Vec<f32>,
    /// Score seconds of `samples[0]`.
    offset: f64,
    range: (f64, f64),
}

#[derive(Default)]
pub struct CheckState(pub Arc<Mutex<Option<CheckTake>>>);

#[derive(Serialize, Clone)]
pub struct CheckProgress {
    pub job: u64,
    /// Score seconds (stays at the start during the count-in).
    pub t: f64,
    pub counting: bool,
    /// The take is over and the voice being analysed.
    pub analyzing: bool,
    /// Microphone peak, 0..1.
    pub level: f32,
}

#[derive(Serialize, Clone)]
pub struct CheckDone {
    pub job: u64,
    pub part: String,
    pub notes: Vec<crate::check::NoteCheck>,
    /// Sung pitch (score seconds, MIDI), moved by octaves next to the notes.
    pub line: Vec<(f64, f32)>,
    /// The stretch actually sung (score seconds).
    pub range: (f64, f64),
    /// Delay of the voice found and compensated, seconds.
    pub lag: f64,
    pub error: Option<String>,
}

/// Plays the mix from `from_sec` (to `to_sec` or the end) after a count-in and records the
/// microphone; then compares the voice with the notes of `part` (`sing-check` / `sing-check-done`).
#[tauri::command]
#[allow(clippy::too_many_arguments)] // the arguments of a Tauri command
pub async fn sing_check(
    app: tauri::AppHandle,
    audio: State<'_, AudioState>,
    state: State<'_, SingState>,
    takes: State<'_, CheckState>,
    mix: Vec<MixPart>,
    part: String,
    from_sec: f64,
    to_sec: Option<f64>,
    ambience: Option<Ambience>,
    reverb: Option<f32>,
) -> Result<u64> {
    let (sing, engine, takes) = (state.0.clone(), audio.0.clone(), takes.0.clone());
    let notes = sing.melody(&part).ok_or_else(|| Error::Other(format!("{part}: genera prima l'audio")))?;
    let (l, r) = sing.with_tracks(|t| mix::mix_in(t, &mix, ambience.unwrap_or_default(), reverb.unwrap_or(crate::fx::DEFAULT_AMOUNT)))?;
    let sr = crate::ds::render::SAMPLE_RATE;
    let total = l.len() as f64 / sr as f64;
    let from = from_sec.clamp(0.0, total);
    let to = to_sec.unwrap_or(total).clamp(from, total);
    if to - from < 1.0 {
        return Err(Error::Other("il tratto da cantare è troppo corto".into()));
    }
    let pre = sing.count_in(from);
    let lead = pre.len() as f64 / sr as f64;
    let (i, j) = ((from * sr as f64) as usize, ((to * sr as f64) as usize).min(l.len()));
    let l: Vec<f32> = pre.iter().chain(&l[i..j]).copied().collect();
    let r: Vec<f32> = pre.iter().chain(&r[i..j]).copied().collect();
    let job = blocking({
        let engine = engine.clone();
        move || {
            engine.ensure(true)?;
            Ok(engine.begin_job(true))
        }
    })
    .await?;
    *state.1.lock().unwrap_or_else(|e| e.into_inner()) = job;
    std::thread::spawn(move || {
        let progress = |t: f64, _total: f64, level: f32| {
            let _ = app.emit("sing-check", CheckProgress { job, t: (from - lead + t).max(from), counting: t < lead, analyzing: false, level });
        };
        let res = engine.play_and_record(job, &l, &r, sr, &progress);
        engine.end_job(job);
        let mut done = CheckDone { job, part, notes: vec![], line: vec![], range: (from, from), lag: 0.0, error: None };
        match res {
            Ok(None) => done.error = Some(String::new()), // replaced by something else: nothing to show
            Err(e) => done.error = Some(e.to_string()),
            Ok(Some(rec)) => 'judge: {
                let offset = from - lead;
                let heard = rec.samples.len() as f64 / rec.sample_rate as f64;
                // stopped during the count-in: as if cancelled
                let Some(range) = crate::check::sung_range(from, to, lead, heard) else {
                    done.error = Some(String::new());
                    break 'judge;
                };
                let _ = app.emit("sing-check", CheckProgress { job, t: range.1, counting: false, analyzing: true, level: 0.0 });
                // the last take on disk, to look at it if something is wrong
                let peak = rec.samples.iter().fold(0f32, |m, x| m.max(x.abs()));
                let rms = (rec.samples.iter().map(|x| x * x).sum::<f32>() / rec.samples.len().max(1) as f32).sqrt();
                if let Ok(dir) = app.path().app_cache_dir() {
                    let file = dir.join("ultima-prova.wav");
                    if std::fs::create_dir_all(&dir).is_ok() && audio::write_wav(&file, &rec.samples, rec.sample_rate).is_ok() {
                        crate::log!("canta e controlla: registrazione in {}", file.display());
                    }
                }
                if peak < 1e-3 {
                    let mic = engine.status().ok().and_then(|s| s.input).map(|d| d.name).unwrap_or_else(|| "?".into());
                    crate::log!("canta e controlla: silenzio (picco {peak:.5}) dal microfono «{mic}»");
                    done.error = Some(format!(
                        "Il microfono «{mic}» non ha mandato suono. Su macOS controlla Impostazioni di Sistema → Privacy e sicurezza → Microfono: Muse Sing (o il Terminale, se avvii l'app da lì) deve essere attivo. Oppure scegli un altro microfono con «Attiva microfono»."
                    ));
                    break 'judge;
                }
                let track = crate::pitch::analyze(&rec.samples, rec.sample_rate);
                let voiced = track.midi.iter().filter(|m| m.is_some()).count() as f64 / track.midi.len().max(1) as f64;
                crate::log!("canta e controlla: picco {peak:.3}, rms {rms:.4}, voce nel {:.0}% dei frame", voiced * 100.0);
                let lag = crate::check::best_lag(&notes, &track, offset, range);
                let at = offset - lag;
                done.notes = crate::check::compare(&notes, &track, at, range);
                done.line = crate::check::sung_line(&notes, &track, at, range);
                done.range = range;
                done.lag = lag;
                crate::log!("canta e controlla: {} note in {:.1}-{:.1} s, ritardo {:.0} ms", done.notes.len(), range.0, range.1, lag * 1000.0);
                let samples = audio::resample(&rec.samples, rec.sample_rate, sr);
                *takes.lock().unwrap_or_else(|e| e.into_inner()) = Some(CheckTake { samples, offset: at, range });
            }
        }
        let _ = app.emit("sing-check-done", done);
    });
    Ok(job)
}

/// Plays the last check's stretch with the recorded voice over the mix (`sing-play` events).
#[tauri::command]
pub async fn sing_check_replay(
    app: tauri::AppHandle,
    audio: State<'_, AudioState>,
    state: State<'_, SingState>,
    takes: State<'_, CheckState>,
    mix: Vec<MixPart>,
    ambience: Option<Ambience>,
    reverb: Option<f32>,
) -> Result<u64> {
    let (sing, engine) = (state.0.clone(), audio.0.clone());
    let (l, r, from, total) = {
        let take = takes.0.lock().unwrap_or_else(|e| e.into_inner());
        let take = take.as_ref().ok_or_else(|| Error::Other("canta prima un tratto".into()))?;
        let (mut l, mut r) = sing.with_tracks(|t| mix::mix_in(t, &mix, ambience.unwrap_or_default(), reverb.unwrap_or(crate::fx::DEFAULT_AMOUNT)))?;
        let sr = crate::ds::render::SAMPLE_RATE as f64;
        let total = l.len() as f64 / sr;
        // the voice brought to a clear level, centred
        let peak = take.samples.iter().fold(0f32, |m, x| m.max(x.abs()));
        let gain = if peak > 1e-4 { 0.6 / peak } else { 1.0 };
        for (k, x) in take.samples.iter().enumerate() {
            let at = take.offset * sr + k as f64;
            if at < 0.0 {
                continue;
            }
            let at = at as usize;
            if at >= l.len() {
                l.resize(at + 1, 0.0);
                r.resize(at + 1, 0.0);
            }
            l[at] += x * gain;
            r[at] += x * gain;
        }
        let j = ((take.range.1 * sr) as usize).min(l.len());
        let i = ((take.range.0 * sr) as usize).min(j);
        (l[i..j].to_vec(), r[i..j].to_vec(), take.range.0, total)
    };
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
        let progress = |t: f64, _: f64| {
            let _ = app.emit("sing-play", PlayProgress { job, t: from + t, total });
        };
        let res = engine.play_stereo(job, &l, &r, crate::ds::render::SAMPLE_RATE, 0.0, None, &progress);
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
pub async fn sing_export(state: State<'_, SingState>, mix: Vec<MixPart>, path: String, ambience: Option<Ambience>, reverb: Option<f32>) -> Result<String> {
    let sing = state.0.clone();
    blocking(move || {
        let (l, r) = sing.with_tracks(|t| mix::mix_in(t, &mix, ambience.unwrap_or_default(), reverb.unwrap_or(crate::fx::DEFAULT_AMOUNT)))?;
        let title = sing.title();
        mix::export(Path::new(&path), &l, &r, crate::ds::render::SAMPLE_RATE, &title)?;
        Ok(path)
    })
    .await
}

// ---------------------------------------------------------------------------------------------
// Projects (.msng)

use crate::project::{self, PartCfg, Project};

#[derive(Serialize, Deserialize, Clone)]
pub struct RecentProject {
    pub path: String,
    pub title: String,
}

/// A project as the UI holds it: the project, where it is saved (None = not yet), its score's
/// working copy and the analysis of that score.
#[derive(Serialize)]
pub struct OpenedProject {
    pub project: Project,
    pub path: Option<String>,
    pub score_path: String,
    pub analysis: Analysis,
    /// Folder to propose for saving and exporting.
    pub default_dir: String,
}

fn cache_dir(app: &tauri::AppHandle) -> Result<PathBuf> {
    app.path().app_cache_dir().map_err(|e| Error::Other(format!("cartella cache: {e}")))
}

fn remember_project(app: &tauri::AppHandle, path: &str, title: &str) {
    let mut prefs = load_prefs(app);
    // an old .musesing saved again as .msng: the old file leaves the list, so it isn't reopened
    // and saved over the newer one by mistake
    let legacy: Vec<String> = project::LEGACY_EXTENSIONS.iter().map(|e| Path::new(path).with_extension(e).to_string_lossy().into_owned()).collect();
    prefs.recent_projects.retain(|p| p.path != path && !legacy.contains(&p.path));
    prefs.recent_projects.insert(0, RecentProject { path: path.to_string(), title: title.to_string() });
    prefs.recent_projects.truncate(10);
    save_prefs(app, &prefs);
}

fn fresh_parts(a: &Analysis) -> Vec<PartCfg> {
    a.parts
        .iter()
        .filter(|p| p.sung)
        .map(|p| PartCfg { name: p.name.clone(), sing: true, singer: String::new(), color: None, volume_db: 0.0, pan: 0.0, mute: false, solo: false, voices: 1, played: false })
        .collect()
}

/// What the project file weighs, piece by piece.
#[tauri::command]
pub async fn project_sizes(path: String) -> Result<project::Sizes> {
    blocking(move || project::sizes(Path::new(&path))).await
}

fn dir_size(dir: &Path) -> u64 {
    std::fs::read_dir(dir).map_or(0, |entries| {
        entries.flatten().map(|e| e.metadata().map_or(0, |m| if m.is_dir() { dir_size(&e.path()) } else { m.len() })).sum()
    })
}

/// Bytes of the generated voices kept in the render cache.
#[tauri::command]
pub async fn cache_size(sing: State<'_, SingState>) -> Result<u64> {
    let dir = sing.0.cache_dir().map(Path::to_path_buf);
    blocking(move || Ok(dir.map_or(0, |d| dir_size(&d)))).await
}

/// Empties the render cache (the voices in memory stay until the next render).
#[tauri::command]
pub async fn cache_clear(sing: State<'_, SingState>) -> Result<()> {
    let dir = sing.0.cache_dir().map(Path::to_path_buf);
    blocking(move || {
        if let Some(d) = dir.filter(|d| d.is_dir()) {
            std::fs::remove_dir_all(&d)?;
            crate::log!("cache delle voci svuotata");
        }
        Ok(())
    })
    .await
}

fn parent_dir(p: &Path) -> String {
    p.parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default()
}

/// New project from a MuseScore or MusicXML file (singers are filled in by the UI from the saved choices).
#[tauri::command]
pub async fn project_new(app: tauri::AppHandle, score: String) -> Result<OpenedProject> {
    blocking(move || {
        let src = PathBuf::from(&score);
        let bytes = std::fs::read(&src)?;
        let wc = project::working_copy(&cache_dir(&app)?, &bytes)?;
        let analysis = analysis_of(&wc, &src)?;
        let instruments = analysis.parts.iter().any(|p| !p.sung);
        let project = Project::new(&analysis.title, &analysis.file_name, analysis.lang, fresh_parts(&analysis), instruments);
        Ok(OpenedProject { project, path: None, score_path: wc.to_string_lossy().into_owned(), analysis, default_dir: parent_dir(&src) })
    })
    .await
}

#[tauri::command]
pub async fn project_open(app: tauri::AppHandle, sing: State<'_, SingState>, path: String) -> Result<OpenedProject> {
    let sing = sing.0.clone();
    blocking(move || {
        let p = PathBuf::from(&path);
        let (project, bytes) = project::open(&p)?;
        let wc = project::working_copy(&cache_dir(&app)?, &bytes)?;
        restore_audio(&sing, &p);
        let display = p.with_file_name(if project.score_name.is_empty() { "partitura.mscz" } else { &project.score_name });
        let analysis = analysis_of(&wc, &display)?;
        remember_project(&app, &path, &project.title);
        Ok(OpenedProject { project, path: Some(path), score_path: wc.to_string_lossy().into_owned(), analysis, default_dir: parent_dir(&p) })
    })
    .await
}

/// The voices kept in a project file back into the render cache (errors only logged: the
/// project opens anyway, and the voices can be generated again).
fn restore_audio(sing: &Sing, path: &Path) {
    let t0 = std::time::Instant::now();
    match project::audio(path) {
        Ok(parts) if !parts.is_empty() => {
            for p in &parts {
                let res = u64::from_str_radix(&p.meta.key, 16)
                    .map_err(|e| Error::Format(e.to_string()))
                    .and_then(|k| sing.store_part_audio(k, &p.flac, p.meta.scale, &p.info));
                if let Err(e) = res {
                    crate::log!("progetto: voce {} non ripresa dal file: {e}", p.meta.part);
                }
            }
            crate::log!("progetto: {} voci riprese dal file in {:.1} s", parts.len(), t0.elapsed().as_secs_f64());
        }
        Ok(_) => {}
        Err(e) => crate::log!("progetto: audio non letto: {e}"),
    }
}

/// The voices of the last render, ready to be written in a project (None if some is no longer in
/// the cache).
fn current_audio(sing: &Sing) -> Option<Vec<project::AudioEntry>> {
    sing.last_parts()
        .into_iter()
        .map(|last| {
            let (flac, scale, info) = sing.part_audio(&last)?;
            Some(project::AudioEntry { meta: project::AudioPart { part: last.part, key: format!("{:016x}", last.portable), scale }, flac, info })
        })
        .collect()
}

/// Saves the project with its score; returns the path written (always with the .msng extension).
/// `audio`: "current" (the voices in memory, which match the project's choices), "keep" (the ones
/// already in the file `from`, the project as it was opened) or "none".
#[tauri::command]
pub async fn project_save(
    app: tauri::AppHandle,
    sing: State<'_, SingState>,
    path: String,
    project: Project,
    score_path: String,
    audio: Option<String>,
    from: Option<String>,
) -> Result<String> {
    let sing = sing.0.clone();
    blocking(move || {
        let mut p = PathBuf::from(&path);
        if p.extension().and_then(|e| e.to_str()) != Some(project::EXTENSION) {
            p.set_extension(project::EXTENSION);
        }
        let score = std::fs::read(&score_path)?;
        // the voices already in the file the project was opened from (none for a new project)
        let from = from.map(PathBuf::from);
        let keep = || from.as_deref().map_or(project::Audio::None, project::Audio::KeepFrom);
        let t0 = std::time::Instant::now();
        let audio = match audio.as_deref() {
            Some("none") => project::Audio::None,
            Some("current") => match current_audio(&sing) {
                Some(parts) => project::Audio::Parts(parts),
                None => {
                    crate::log!("progetto: voci generate non disponibili, tengo quelle del file");
                    keep()
                }
            },
            _ => keep(),
        };
        project::save(&p, &project, &score, audio)?;
        crate::log!("progetto salvato in {:.1} s", t0.elapsed().as_secs_f64());
        let out = p.to_string_lossy().into_owned();
        remember_project(&app, &out, &project.title);
        Ok(out)
    })
    .await
}

/// Replaces the project's score with a newer version (MuseScore or MusicXML), keeping the settings
/// of the parts that are still there.
#[tauri::command]
pub async fn project_reload_score(app: tauri::AppHandle, project: Project, score: String) -> Result<OpenedProject> {
    blocking(move || {
        let src = PathBuf::from(&score);
        let bytes = std::fs::read(&src)?;
        let wc = project::working_copy(&cache_dir(&app)?, &bytes)?;
        let analysis = analysis_of(&wc, &src)?;
        let mut project = project;
        project.parts = project::merge_parts(&project.parts, fresh_parts(&analysis));
        project.score_name = analysis.file_name.clone();
        Ok(OpenedProject { project, path: None, score_path: wc.to_string_lossy().into_owned(), analysis, default_dir: parent_dir(&src) })
    })
    .await
}

/// Files the system asked the app to open (double click in the Finder) that the window has not
/// taken yet: at launch the request arrives before the page is listening.
#[derive(Default)]
pub struct OpenedFiles(pub Mutex<Vec<String>>);

/// Event sent to the window when the system asks to open files while the app is running.
pub const OPEN_FILES_EVENT: &str = "open-files";

/// Remembers files to open and tells the window (which takes them with `take_opened_files`).
pub fn files_opened(app: &tauri::AppHandle, paths: Vec<String>) {
    if paths.is_empty() {
        return;
    }
    if let Some(state) = app.try_state::<OpenedFiles>() {
        state.0.lock().unwrap().extend(paths);
    }
    let _ = app.emit(OPEN_FILES_EVENT, ());
}

#[tauri::command]
pub fn take_opened_files(state: State<'_, OpenedFiles>) -> Vec<String> {
    std::mem::take(&mut *state.0.lock().unwrap())
}

#[tauri::command]
pub fn project_recent(app: tauri::AppHandle) -> Vec<RecentProject> {
    load_prefs(&app).recent_projects.into_iter().filter(|p| Path::new(&p.path).is_file()).collect()
}

#[derive(Serialize)]
pub struct RollNote {
    /// Stable id of the note (`score::Note::id`).
    pub id: String,
    /// Position and end in ticks of the written score (480 per quarter).
    pub tick: i64,
    pub end_tick: i64,
    pub start: f64,
    pub end: f64,
    pub pitch: i32,
    pub lyric: Option<String>,
}

#[derive(Serialize)]
pub struct RollPart {
    pub name: String,
    pub sung: bool,
    pub notes: Vec<RollNote>,
}

#[derive(Serialize)]
pub struct RollBar {
    /// Position in ticks of the written score, and in seconds.
    pub tick: i64,
    pub start: f64,
    /// Number printed in the score (`None`: left out of the count, like a pickup).
    pub number: Option<String>,
    /// "3/4" on the first measure and where the time signature changes.
    pub time_sig: Option<String>,
}

#[derive(Serialize)]
pub struct Roll {
    pub parts: Vec<RollPart>,
    /// The measures as played (repeats unrolled).
    pub bars: Vec<RollBar>,
    /// (tick, bpm at the practice tempo), to turn ticks into seconds.
    pub tempos: Vec<(i64, f64)>,
}

/// Every part's notes in seconds (repeats unrolled, at `tempo_factor`), for the score roll.
#[tauri::command]
pub async fn score_roll(score_path: String, tempo_factor: f64, marks: Option<bool>, transpose: Option<i32>) -> Result<Roll> {
    blocking(move || {
        let mut score = read_score(Path::new(&score_path))?;
        // the same tempo, breaths and pitches as the render, so the cursor follows the audio
        crate::marks::prepare(&mut score, marks.unwrap_or(true), transpose.unwrap_or(0));
        let ticks = |b: crate::score::Beats| *b.numer() as f64 / *b.denom() as f64 * crate::ustx::TICKS as f64;
        let tempos: Vec<(i64, f64)> =
            score.tempos.iter().map(|(b, q)| (ticks(*b).round() as i64, q * 60.0 * tempo_factor.clamp(0.25, 2.0))).collect();
        let axis = crate::ds::time::TimeAxis::new(&tempos, crate::ustx::TICKS);
        let sec = |b: crate::score::Beats| axis.tick_to_ms(ticks(b)) / 1000.0;
        let parts = score
            .parts
            .iter()
            .filter(|p| !p.notes.is_empty())
            .map(|p| RollPart {
                name: p.name.clone(),
                sung: p.is_sung(),
                notes: p
                    .notes
                    .iter()
                    .map(|n| RollNote {
                        id: n.id.clone(),
                        tick: ticks(n.start).round() as i64,
                        end_tick: ticks(n.start + n.dur).round() as i64,
                        start: sec(n.start),
                        end: sec(n.start + n.dur),
                        pitch: n.pitch,
                        lyric: n.lyric.as_ref().map(|l| l.text.clone()),
                    })
                    .collect(),
            })
            .collect();
        let bars = score
            .bars
            .iter()
            .map(|b| RollBar {
                tick: ticks(b.start).round() as i64,
                start: sec(b.start),
                number: b.number.clone(),
                time_sig: b.time_sig.map(|(n, d)| format!("{n}/{d}")),
            })
            .collect();
        Ok(Roll { parts, bars, tempos })
    })
    .await
}

fn file_safe(s: &str) -> String {
    s.chars().map(|c| if "/\\:*?\"<>|".contains(c) { '-' } else { c }).collect::<String>().trim().to_string()
}

/// Practice tracks: one file per sung part, with that part as mixed and the others lowered by
/// `others_db` (the accompaniment stays as mixed). Returns the files written.
#[tauri::command]
#[allow(clippy::too_many_arguments)] // the arguments of a Tauri command
pub async fn sing_export_guides(
    state: State<'_, SingState>,
    mix: Vec<MixPart>,
    dir: String,
    ext: String,
    others_db: f32,
    ambience: Option<Ambience>,
    reverb: Option<f32>,
    count_in: Option<bool>,
    kind: Option<String>,
) -> Result<Vec<String>> {
    let sing = state.0.clone();
    blocking(move || {
        let title = file_safe(&sing.title());
        let names: Vec<String> = sing.with_tracks(|t| t.iter().map(|(n, _)| n.clone()).collect())?;
        let mut written = vec![];
        // "evidenza": the part louder than the others; "senza": the others without it
        let kinds: &[&str] = match kind.as_deref() {
            Some("senza") => &["senza"],
            Some("entrambe") => &["evidenza", "senza"],
            _ => &["evidenza"],
        };
        for name in names.iter().filter(|n| n.as_str() != project::ACCOMPANIMENT && n.as_str() != project::METRONOME) {
            for &k in kinds {
                let without = k == "senza";
                let parts: Vec<MixPart> = mix
                    .iter()
                    .map(|p| {
                        let mut p = p.clone();
                        p.solo = false;
                        if &p.name == name {
                            p.mute = without;
                        } else if !without && p.name != project::ACCOMPANIMENT && p.name != project::METRONOME {
                            p.volume_db += others_db;
                        }
                        p
                    })
                    .collect();
                let (mut l, mut r) = sing.with_tracks(|t| mix::mix_in(t, &parts, ambience.unwrap_or_default(), reverb.unwrap_or(crate::fx::DEFAULT_AMOUNT)))?;
                if count_in == Some(true) {
                    let pre = sing.count_in(0.0);
                    l.splice(0..0, pre.iter().copied());
                    r.splice(0..0, pre.iter().copied());
                }
                let label = if without { format!("senza {name}") } else { name.clone() };
                let path = Path::new(&dir).join(format!("{title} - {}.{ext}", file_safe(&label)));
                mix::export(&path, &l, &r, crate::ds::render::SAMPLE_RATE, &format!("{} ({label})", sing.title()))?;
                written.push(path.to_string_lossy().into_owned());
            }
        }
        Ok(written)
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
