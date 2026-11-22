//! Guide playback and microphone recording (cpal), WAV files (hound).
//!
//! The [`Engine`] keeps one output stream and (during a recording session) one input stream open,
//! instead of opening fresh streams for every take. Opening a microphone can reconfigure the
//! audio hardware: Bluetooth headphones switch profile and change sample rate, and macOS pauses
//! any output stream whose device changes rate. Doing it once, before anything has to be heard,
//! is what keeps the count-in clicks from getting lost.
//!
//! The output is a small mixer: buffers are scheduled at an absolute output frame, and progress
//! (the red cursor, the count-in number) is computed from the frames actually played, minus the
//! latency the device reports. A take: four count-in clicks (optionally with the starting note),
//! then the phrase (optionally with the guide melody), then a short tail. The saved WAV starts
//! `LEAD_IN` s before the first note; the first click is located in the input through the stream
//! timestamps, so the file stays aligned with Bluetooth latency too.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{ErrorKind, FromSample, InterfaceType, SampleFormat, SizedSample, Stream, StreamConfig, StreamInstant};
use serde::Serialize;

use crate::corpus::{CNote, Phrase};
use crate::error::{Error, Result};

pub const COUNT_IN_BEATS: usize = 4;
/// Seconds kept before the first note and after the last one.
pub const LEAD_IN: f64 = 0.6;
pub const TAIL: f64 = 1.0;
/// A buffer is scheduled this far after the current output frame, so it never starts mid-buffer.
const SCHEDULE_AHEAD: f64 = 0.15;
/// Silence between two repetitions of the guide in loop mode, in beats.
const LOOP_GAP_BEATS: f64 = 2.0;
const GUIDE_GAIN: f32 = 0.3;
const TAKE_GUIDE_GAIN: f32 = 0.18;
const START_NOTE_GAIN: f32 = 0.14;
const NOTE_GAIN: f32 = 0.3;

fn midi_hz(p: i32) -> f64 {
    440.0 * 2f64.powf((p as f64 - 69.0) / 12.0)
}

pub fn count_in_secs(ph: &Phrase) -> f64 {
    COUNT_IN_BEATS as f64 * 60.0 / ph.bpm
}

pub fn phrase_secs(ph: &Phrase) -> f64 {
    ph.notes.iter().map(|n| n.beats).sum::<f64>() * 60.0 / ph.bpm
}

// ------------------------------------------------------------------------------------------------
// Synthesis

/// Adds a soft piano-like tone of `dur` seconds at `t0`.
pub(crate) fn add_tone(buf: &mut [f32], sr: f64, t0: f64, dur: f64, pitch: i32, gain: f32) {
    let f = midi_hz(pitch);
    let i0 = (t0 * sr) as usize;
    let len = ((dur + 0.06) * sr) as usize;
    for i in 0..len {
        let t = i as f64 / sr;
        let attack = (t / 0.008).min(1.0);
        let release = if t > dur { (1.0 - (t - dur) / 0.06).max(0.0) } else { 1.0 };
        let env = attack * release * (0.35 + 0.65 * (-t * 2.5).exp());
        let w = 2.0 * std::f64::consts::PI * f * t;
        let v = w.sin() + 0.4 * (2.0 * w).sin() + 0.15 * (3.0 * w).sin() + 0.05 * (4.0 * w).sin();
        match buf.get_mut(i0 + i) {
            Some(s) => *s += (v * env) as f32 * gain,
            None => break,
        }
    }
}

pub(crate) fn add_click(buf: &mut [f32], sr: f64, t0: f64, accent: bool) {
    let f = if accent { 1600.0 } else { 1000.0 };
    let i0 = (t0 * sr) as usize;
    for i in 0..(0.04 * sr) as usize {
        let t = i as f64 / sr;
        if let Some(s) = buf.get_mut(i0 + i) {
            *s += (0.45 * (2.0 * std::f64::consts::PI * f * t).sin() * (-t * 90.0).exp()) as f32;
        }
    }
}

/// The melody from its first note at `bpm`, followed by `tail` seconds of silence.
pub fn synth_melody(notes: &[CNote], bpm: f64, sr: u32, gain: f32, tail: f64) -> Vec<f32> {
    let sr_f = sr as f64;
    let beat = 60.0 / bpm;
    let total = notes.iter().map(|n| n.beats).sum::<f64>() * beat + tail;
    let mut buf = vec![0f32; (total * sr_f) as usize + 1];
    let mut pos = 0.0;
    for n in notes {
        add_tone(&mut buf, sr_f, pos, n.beats * beat, n.pitch, gain);
        pos += n.beats * beat;
    }
    buf
}

/// What plays during a take: the count-in clicks, the starting note softly on beats 3-4 (if
/// `start_note`), then the melody (if `melody`). The first click is at sample 0, the first note at
/// `count_in_secs`.
pub fn synth_take(ph: &Phrase, sr: u32, melody: bool, start_note: bool) -> Vec<f32> {
    let sr_f = sr as f64;
    let beat = 60.0 / ph.bpm;
    let start = count_in_secs(ph);
    let mut buf = vec![0f32; ((start + phrase_secs(ph) + 0.3) * sr_f) as usize + 1];
    for b in 0..COUNT_IN_BEATS {
        add_click(&mut buf, sr_f, b as f64 * beat, b == 0);
    }
    if start_note && let Some(first) = ph.notes.first() {
        let from = (COUNT_IN_BEATS as f64 - 2.0).max(0.0) * beat;
        add_tone(&mut buf, sr_f, from, start - from - 0.12 * beat, first.pitch, START_NOTE_GAIN);
    }
    if melody {
        let m = synth_melody(&ph.notes, ph.bpm, sr, TAKE_GUIDE_GAIN, 0.0);
        let i0 = (start * sr_f) as usize;
        for (s, v) in buf.iter_mut().skip(i0).zip(m) {
            *s += v;
        }
    }
    buf
}

/// One note with the guide's timbre.
pub fn synth_note(pitch: i32, secs: f64, sr: u32) -> Vec<f32> {
    let mut buf = vec![0f32; ((secs + 0.1) * sr as f64) as usize];
    add_tone(&mut buf, sr as f64, 0.0, secs, pitch, NOTE_GAIN);
    buf
}

// ------------------------------------------------------------------------------------------------
// Alignment (pure, tested)

/// Seconds of a stream instant, None for the "no timestamp" value some backends report.
fn instant_secs(i: StreamInstant) -> Option<f64> {
    let n = i.as_nanos();
    (n > 0).then(|| n as f64 / 1e9)
}

/// The input frame captured at the moment output frame `out_frame` is heard, from the latest
/// timestamp of each stream: `out_ref` = (output frame, playback time), `in_ref` = (input frame,
/// capture time), in seconds of the same clock.
pub fn align_input_frame(out_frame: u64, out_ref: (u64, f64), out_sr: u32, in_ref: (u64, f64), in_sr: u32) -> Option<u64> {
    let t = out_ref.1 + (out_frame as f64 - out_ref.0 as f64) / out_sr as f64;
    let f = in_ref.0 as f64 + (t - in_ref.1) * in_sr as f64;
    (f.is_finite() && f >= 0.0).then(|| f.round() as u64)
}

/// Picks where the first click is in the input: the timestamp estimate when it is plausible (within
/// half a second of the frame-counter estimate, which ignores latencies), the counter otherwise.
pub fn choose_click_frame(by_timestamp: Option<u64>, by_counter: Option<u64>, in_sr: u32) -> Option<u64> {
    match (by_timestamp, by_counter) {
        (Some(a), Some(b)) if (a as i64 - b as i64).unsigned_abs() <= in_sr as u64 / 2 => Some(a),
        (_, Some(b)) => Some(b),
        (a, None) => a,
    }
}

// ------------------------------------------------------------------------------------------------
// Devices

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DevInfo {
    pub name: String,
    pub bluetooth: bool,
    pub default: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Devices {
    pub inputs: Vec<DevInfo>,
    pub outputs: Vec<DevInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub input: Option<DevInfo>,
    pub output: DevInfo,
    /// The system microphone is Bluetooth and another one was picked instead.
    pub input_switched: bool,
    pub input_rate: u32,
    pub output_rate: u32,
}

/// Bluetooth headsets: by interface type where the backend reports it, by name otherwise
/// (CoreAudio in cpal 0.18 does not report Bluetooth).
pub fn looks_bluetooth(name: &str, iface: Option<InterfaceType>) -> bool {
    if iface == Some(InterfaceType::Bluetooth) {
        return true;
    }
    let n = name.to_lowercase();
    ["airpods", "bluetooth", "beats", "buds", "headset", "hands-free", "handsfree", "wh-1000", "bose", "jabra"]
        .iter()
        .any(|k| n.contains(k))
}

fn dev_name(d: &cpal::Device) -> (String, Option<InterfaceType>) {
    match d.description() {
        Ok(desc) => (desc.name().to_string(), Some(desc.interface_type())),
        Err(_) => ("?".into(), None),
    }
}

fn dev_info(d: &cpal::Device, default: &Option<String>) -> DevInfo {
    let (name, iface) = dev_name(d);
    DevInfo { bluetooth: looks_bluetooth(&name, iface), default: default.as_deref() == Some(name.as_str()), name }
}

pub fn list_devices() -> Result<Devices> {
    let host = cpal::default_host();
    let def_in = host.default_input_device().map(|d| dev_name(&d).0);
    let def_out = host.default_output_device().map(|d| dev_name(&d).0);
    let inputs = host.input_devices().map(|it| it.map(|d| dev_info(&d, &def_in)).collect()).unwrap_or_default();
    let outputs = host.output_devices().map(|it| it.map(|d| dev_info(&d, &def_out)).collect()).unwrap_or_default();
    Ok(Devices { inputs, outputs })
}

/// The microphone to use: the one asked for if present, else the system one, else (when the system
/// one is Bluetooth) a wired/built-in one, so the headphones stay in high-quality mode.
fn pick_input(want: Option<&str>) -> Option<(cpal::Device, DevInfo, bool)> {
    let host = cpal::default_host();
    let def = host.default_input_device();
    let def_name = def.as_ref().map(|d| dev_name(d).0);
    let all: Vec<cpal::Device> = host.input_devices().map(|it| it.collect()).unwrap_or_default();
    if let Some(w) = want
        && let Some(d) = all.iter().find(|d| dev_name(d).0 == w)
    {
        let info = dev_info(d, &def_name);
        return Some((d.clone(), info, false));
    }
    let def = def?;
    let info = dev_info(&def, &def_name);
    if info.bluetooth {
        let mut others: Vec<(cpal::Device, DevInfo)> =
            all.iter().map(|d| (d.clone(), dev_info(d, &def_name))).filter(|(_, i)| !i.bluetooth).collect();
        let builtin = |i: &DevInfo| {
            let n = i.name.to_lowercase();
            ["macbook", "built-in", "integrat", "internal", "imac"].iter().any(|k| n.contains(k))
        };
        others.sort_by_key(|(_, i)| !builtin(i));
        if let Some((d, i)) = others.into_iter().next() {
            return Some((d, i, true));
        }
    }
    Some((def, info, false))
}

fn pick_output(want: Option<&str>) -> Option<(cpal::Device, DevInfo)> {
    let host = cpal::default_host();
    let def = host.default_output_device();
    let def_name = def.as_ref().map(|d| dev_name(d).0);
    if let Some(w) = want
        && let Some(d) = host.output_devices().ok()?.find(|d| dev_name(d).0 == w)
    {
        let info = dev_info(&d, &def_name);
        return Some((d, info));
    }
    let def = def?;
    let info = dev_info(&def, &def_name);
    Some((def, info))
}

/// An audio error in Italian, with cpal's kind and message kept for diagnosis.
fn audio_err(what: &str, e: &cpal::Error) -> Error {
    let owner;
    let hint = match e.kind() {
        ErrorKind::PermissionDenied => {
            let app = crate::mic_access::owner();
            owner = format!("«{app}» non ha il permesso: apri Impostazioni di Sistema → Privacy e sicurezza → Microfono e attiva «{app}»");
            owner.as_str()
        }
        ErrorKind::DeviceBusy => "il dispositivo è occupato o non si è ancora liberato, riprova tra un attimo",
        ErrorKind::DeviceNotAvailable => "il dispositivo non è disponibile (scollegato o spento?)",
        ErrorKind::StreamInvalidated => {
            "il dispositivo ha cambiato configurazione (succede con le cuffie Bluetooth quando si attiva il loro microfono)"
        }
        ErrorKind::UnsupportedConfig => "il dispositivo non accetta il formato audio richiesto",
        _ => "errore del sistema audio",
    };
    Error::Other(format!("{what}: {hint} [{:?}: {e}]", e.kind()))
}

fn retryable(e: &cpal::Error) -> bool {
    matches!(
        e.kind(),
        ErrorKind::DeviceBusy | ErrorKind::DeviceNotAvailable | ErrorKind::StreamInvalidated | ErrorKind::BackendError
    )
}

/// Up to three attempts, waiting 300 then 600 ms, for errors that tend to go away by themselves.
fn with_retry<T>(mut f: impl FnMut() -> std::result::Result<T, cpal::Error>) -> std::result::Result<T, cpal::Error> {
    let mut wait = 300;
    for _ in 0..2 {
        match f() {
            Err(e) if retryable(&e) => {
                eprintln!("audio: {e:?}, nuovo tentativo tra {wait} ms");
                std::thread::sleep(Duration::from_millis(wait));
                wait *= 2;
            }
            r => return r,
        }
    }
    f()
}

// ------------------------------------------------------------------------------------------------
// Engine

/// Events for the UI.
pub enum Event {
    /// Input peak (0..1) about 20 times a second while the microphone is open.
    Level(f32),
    /// A device problem while idle or playing (unplugged, reconfigured…).
    Device(String),
}

pub type Notify = Arc<dyn Fn(Event) + Send + Sync>;

#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub job: u64,
    /// guide | take | play
    pub kind: &'static str,
    /// wait | countin | sing | tail
    pub phase: &'static str,
    /// Seconds from the start of what is playing (guide/file position; take: since the first click).
    pub t: f64,
    pub total: f64,
    /// Cursor on the piano roll, in beats from the first note.
    pub beats: f64,
    /// Latest input peak (0..1), recording only.
    pub level: f32,
}

struct Voice {
    id: u64,
    buf: Arc<Vec<f32>>,
    /// Right channel of a stereo sound (`buf` is then the left one).
    right: Option<Arc<Vec<f32>>>,
    /// Absolute output frame of `buf[offset]`.
    start: u64,
    offset: usize,
    looping: bool,
    gain: f32,
    /// Fade-in length in frames (when starting in the middle of a sound).
    fade: u64,
    /// Note previews are not jobs: a new one replaces the previous one.
    note: bool,
}

#[derive(Default)]
struct Mixer {
    voices: Vec<Voice>,
    /// Frames rendered since the output stream opened.
    frame: u64,
    /// Seconds between rendering a frame and hearing it, as reported by the device.
    latency: f64,
    /// (frame, playback seconds) of the latest callback.
    ts: Option<(u64, f64)>,
    /// (voice id, input frame counter when the voice started), for alignment without timestamps.
    marks: Vec<(u64, u64)>,
}

impl Mixer {
    /// Calls `out(frame, left, right)` for `n` frames.
    fn render(&mut self, n: usize, mut out: impl FnMut(usize, f32, f32), in_now: u64, in_ratio: f64) {
        let f0 = self.frame;
        for v in &self.voices {
            if v.start >= f0 && v.start < f0 + n as u64 && !self.marks.iter().any(|(id, _)| *id == v.id) {
                self.marks.push((v.id, in_now + ((v.start - f0) as f64 * in_ratio) as u64));
            }
        }
        for i in 0..n {
            let f = f0 + i as u64;
            let (mut s, mut r) = (0f32, 0f32);
            for v in &self.voices {
                if f < v.start {
                    continue;
                }
                let rel = f - v.start;
                let len = v.buf.len();
                let mut p = v.offset + rel as usize;
                if v.looping && len > 0 {
                    p %= len;
                }
                if let Some(x) = v.buf.get(p) {
                    let fade = if v.fade > 0 { (rel as f32 / v.fade as f32).min(1.0) } else { 1.0 };
                    let g = v.gain * fade;
                    s += x * g;
                    r += v.right.as_ref().and_then(|b| b.get(p)).unwrap_or(x) * g;
                }
            }
            out(i, s.clamp(-1.0, 1.0), r.clamp(-1.0, 1.0));
        }
        self.frame = f0 + n as u64;
        let now = self.frame;
        self.voices.retain(|v| v.looping || now < v.start || v.offset + ((now - v.start) as usize) < v.buf.len());
    }
}

#[derive(Default)]
struct Capture {
    recording: bool,
    /// Job that started the recording: a late cleanup of a cancelled take must not stop the next one.
    owner: u64,
    /// Input frame of `samples[0]`.
    start_frame: u64,
    /// Interleaved, `channels` per frame.
    samples: Vec<f32>,
    /// (frame, capture seconds) of the latest callback.
    ts: Option<(u64, f64)>,
}

struct Output {
    _stream: Stream,
    info: DevInfo,
    rate: u32,
}

struct Input {
    _stream: Stream,
    info: DevInfo,
    rate: u32,
    channels: usize,
    switched: bool,
}

#[derive(Default)]
struct Io {
    input: Option<Input>,
    output: Option<Output>,
}

const KIND_NONE: u8 = 0;
const KIND_TAKE: u8 = 2;

pub struct Engine {
    notify: Notify,
    io: Mutex<Io>,
    /// Devices asked for by the UI (None = system default).
    prefs: Mutex<(Option<String>, Option<String>)>,
    mixer: Arc<Mutex<Mixer>>,
    capture: Arc<Mutex<Capture>>,
    in_frames: Arc<AtomicU64>,
    in_rate: Arc<AtomicU32>,
    peak: Arc<AtomicU32>,
    level: AtomicU32,
    has_input: AtomicBool,
    out_broken: Arc<AtomicBool>,
    in_broken: Arc<AtomicBool>,
    /// Bumped whenever the output stream is rebuilt: scheduled frames from before are meaningless.
    generation: AtomicU64,
    job: AtomicU64,
    job_kind: AtomicU8,
    next_id: AtomicU64,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// Silences a job's sounds when it ends, however it ends (an error must not leave them playing).
struct DropVoices<'a>(&'a Mutex<Mixer>, u64);
impl Drop for DropVoices<'_> {
    fn drop(&mut self) {
        lock(self.0).voices.retain(|v| v.id != self.1);
    }
}

/// Clears the recording flag when the take ends, however it ends (if it is still this take's).
struct StopRecording<'a>(&'a Mutex<Capture>, u64);
impl Drop for StopRecording<'_> {
    fn drop(&mut self) {
        let mut c = lock(self.0);
        if c.owner == self.1 {
            c.recording = false;
        }
    }
}

impl Engine {
    pub fn new(notify: Notify) -> Arc<Self> {
        let e = Arc::new(Engine {
            notify,
            io: Mutex::new(Io::default()),
            prefs: Mutex::new((None, None)),
            mixer: Arc::new(Mutex::new(Mixer::default())),
            capture: Arc::new(Mutex::new(Capture::default())),
            in_frames: Arc::new(AtomicU64::new(0)),
            in_rate: Arc::new(AtomicU32::new(48_000)),
            peak: Arc::new(AtomicU32::new(0)),
            level: AtomicU32::new(0),
            has_input: AtomicBool::new(false),
            out_broken: Arc::new(AtomicBool::new(false)),
            in_broken: Arc::new(AtomicBool::new(false)),
            generation: AtomicU64::new(0),
            job: AtomicU64::new(0),
            job_kind: AtomicU8::new(KIND_NONE),
            next_id: AtomicU64::new(1),
        });
        let weak: Weak<Engine> = Arc::downgrade(&e);
        std::thread::spawn(move || {
            while let Some(e) = weak.upgrade() {
                if e.has_input.load(Ordering::Relaxed) {
                    let l = f32::from_bits(e.peak.swap(0, Ordering::Relaxed));
                    e.level.store(l.to_bits(), Ordering::Relaxed);
                    (e.notify)(Event::Level(l));
                }
                drop(e);
                std::thread::sleep(Duration::from_millis(50));
            }
        });
        e
    }

    fn error_handler(&self, input: bool) -> impl FnMut(cpal::Error) + Send + 'static {
        let broken = if input { self.in_broken.clone() } else { self.out_broken.clone() };
        let notify = self.notify.clone();
        move |e: cpal::Error| {
            eprintln!("audio ({}): {e:?}", if input { "ingresso" } else { "uscita" });
            match e.kind() {
                ErrorKind::Xrun | ErrorKind::RealtimeDenied | ErrorKind::DeviceChanged => {}
                _ => {
                    if !broken.swap(true, Ordering::Relaxed) {
                        let what = if input { "Microfono" } else { "Uscita audio" };
                        notify(Event::Device(audio_err(what, &e).to_string()));
                    }
                }
            }
        }
    }

    fn open_output(&self, want: Option<&str>) -> Result<Output> {
        let (dev, info) = with_retry(|| {
            pick_output(want).ok_or_else(|| cpal::Error::with_message(ErrorKind::DeviceNotAvailable, "nessuna uscita"))
        })
        .map_err(|e| audio_err("Uscita audio", &e))?;
        let stream = with_retry(|| {
            let cfg = dev.default_output_config()?;
            let rate = cfg.sample_rate();
            let fmt = cfg.sample_format();
            let config: StreamConfig = cfg.into();
            *lock(&self.mixer) = Mixer::default();
            let s = match fmt {
                SampleFormat::F32 => self.build_output::<f32>(&dev, config),
                SampleFormat::I16 => self.build_output::<i16>(&dev, config),
                SampleFormat::I32 => self.build_output::<i32>(&dev, config),
                SampleFormat::U16 => self.build_output::<u16>(&dev, config),
                SampleFormat::F64 => self.build_output::<f64>(&dev, config),
                f => Err(cpal::Error::with_message(ErrorKind::UnsupportedConfig, format!("formato {f}"))),
            }?;
            s.play()?;
            Ok((s, rate))
        })
        .map_err(|e| audio_err("Uscita audio", &e))?;
        self.out_broken.store(false, Ordering::Relaxed);
        self.generation.fetch_add(1, Ordering::Relaxed);
        Ok(Output { _stream: stream.0, info, rate: stream.1 })
    }

    fn build_output<T: SizedSample + FromSample<f32>>(&self, dev: &cpal::Device, config: StreamConfig) -> std::result::Result<Stream, cpal::Error> {
        let ch = config.channels as usize;
        let rate = config.sample_rate as f64;
        let mixer = self.mixer.clone();
        let in_frames = self.in_frames.clone();
        let in_rate = self.in_rate.clone();
        dev.build_output_stream(
            config,
            move |data: &mut [T], info: &cpal::OutputCallbackInfo| {
                let mut m = lock(&mixer);
                let ts = info.timestamp();
                m.latency = ts.playback.checked_duration_since(ts.callback).map(|d| d.as_secs_f64()).unwrap_or(0.0);
                let f0 = m.frame;
                m.ts = instant_secs(ts.playback).map(|s| (f0, s));
                let ratio = in_rate.load(Ordering::Relaxed) as f64 / rate;
                let n = data.len() / ch.max(1);
                m.render(
                    n,
                    |i, l, r| {
                        let frame = &mut data[i * ch..(i + 1) * ch];
                        match frame {
                            [a, b, rest @ ..] => {
                                *a = T::from_sample(l);
                                *b = T::from_sample(r);
                                let m = T::from_sample((l + r) * 0.5);
                                rest.iter_mut().for_each(|s| *s = m);
                            }
                            [a] => *a = T::from_sample((l + r) * 0.5),
                            [] => {}
                        }
                    },
                    in_frames.load(Ordering::Relaxed),
                    ratio,
                );
            },
            self.error_handler(false),
            None,
        )
    }

    fn open_input(&self, want: Option<&str>) -> Result<Input> {
        // without the permission CoreAudio records silence and doesn't always show the macOS dialog
        crate::mic_access::ensure()?;
        let (dev, info, switched) = with_retry(|| {
            pick_input(want).ok_or_else(|| cpal::Error::with_message(ErrorKind::DeviceNotAvailable, "nessun microfono"))
        })
        .map_err(|e| audio_err("Microfono", &e))?;
        let (stream, rate, channels) = with_retry(|| {
            let cfg = dev.default_input_config()?;
            let rate = cfg.sample_rate();
            let ch = cfg.channels() as usize;
            let fmt = cfg.sample_format();
            let config: StreamConfig = cfg.into();
            self.in_rate.store(rate, Ordering::Relaxed);
            let s = match fmt {
                SampleFormat::F32 => self.build_input::<f32>(&dev, config),
                SampleFormat::I16 => self.build_input::<i16>(&dev, config),
                SampleFormat::I32 => self.build_input::<i32>(&dev, config),
                SampleFormat::U16 => self.build_input::<u16>(&dev, config),
                SampleFormat::F64 => self.build_input::<f64>(&dev, config),
                f => Err(cpal::Error::with_message(ErrorKind::UnsupportedConfig, format!("formato {f}"))),
            }?;
            s.play()?;
            Ok((s, rate, ch))
        })
        .map_err(|e| audio_err("Microfono", &e))?;
        self.in_broken.store(false, Ordering::Relaxed);
        Ok(Input { _stream: stream, info, rate, channels, switched })
    }

    fn build_input<T: SizedSample>(&self, dev: &cpal::Device, config: StreamConfig) -> std::result::Result<Stream, cpal::Error>
    where
        f32: FromSample<T>,
    {
        let ch = (config.channels as usize).max(1);
        let capture = self.capture.clone();
        let frames = self.in_frames.clone();
        let peak = self.peak.clone();
        dev.build_input_stream(
            config,
            move |data: &[T], info: &cpal::InputCallbackInfo| {
                let f0 = frames.load(Ordering::Relaxed);
                let mut m = 0f32;
                {
                    let mut c = lock(&capture);
                    c.ts = instant_secs(info.timestamp().capture).map(|s| (f0, s));
                    if c.recording && c.samples.is_empty() {
                        c.start_frame = f0;
                    }
                    let rec = c.recording;
                    for &x in data {
                        let v: f32 = x.to_sample();
                        m = m.max(v.abs());
                        if rec {
                            c.samples.push(v);
                        }
                    }
                }
                frames.store(f0 + (data.len() / ch) as u64, Ordering::Relaxed);
                // positive floats keep their order as bit patterns
                peak.fetch_max(m.to_bits(), Ordering::Relaxed);
            },
            self.error_handler(true),
            None,
        )
    }

    /// Opens (or reopens with other devices) the streams for a recording session: microphone first,
    /// then the output, then waits until both run steadily.
    pub fn open(&self, input: Option<String>, output: Option<String>) -> Result<Status> {
        let changed = {
            let mut p = lock(&self.prefs);
            let changed = *p != (input.clone(), output.clone());
            *p = (input, output);
            changed
        };
        if changed {
            self.stop();
            let mut io = lock(&self.io);
            io.input = None;
            io.output = None;
            self.has_input.store(false, Ordering::Relaxed);
        }
        self.ensure(true)?;
        self.status()
    }

    pub fn status(&self) -> Result<Status> {
        let io = lock(&self.io);
        let out = io.output.as_ref().ok_or_else(|| Error::Other("uscita audio chiusa".into()))?;
        Ok(Status {
            input: io.input.as_ref().map(|i| i.info.clone()),
            output: out.info.clone(),
            input_switched: io.input.as_ref().is_some_and(|i| i.switched),
            input_rate: io.input.as_ref().map(|i| i.rate).unwrap_or(0),
            output_rate: out.rate,
        })
    }

    /// Closes everything (leaving the session or the tab): the system microphone indicator goes off
    /// and Bluetooth headphones go back to high-quality mode.
    pub fn close(&self) {
        self.stop();
        let mut io = lock(&self.io);
        self.has_input.store(false, Ordering::Relaxed);
        io.input = None;
        io.output = None;
    }

    /// Makes sure the output (and the microphone, if `mic`) is open and running.
    pub fn ensure(&self, mic: bool) -> Result<()> {
        let mut io = lock(&self.io);
        let (want_in, want_out) = lock(&self.prefs).clone();
        let mut new_input = false;
        if mic && (io.input.is_none() || self.in_broken.load(Ordering::Relaxed)) {
            io.input = None;
            self.has_input.store(false, Ordering::Relaxed);
            io.input = Some(self.open_input(want_in.as_deref())?);
            self.has_input.store(true, Ordering::Relaxed);
            new_input = true;
        }
        for attempt in 0..3 {
            if io.output.is_none() || self.out_broken.load(Ordering::Relaxed) || (new_input && attempt == 0) {
                // a new microphone can reconfigure the headphones: reopen the output after it
                io.output = None;
                io.output = Some(self.open_output(want_out.as_deref())?);
            }
            // must run steadily for a moment (longer right after opening a microphone)
            let rate = io.output.as_ref().map(|o| o.rate).unwrap_or(48_000) as u64;
            let need = if new_input { rate / 2 } else { rate / 20 };
            let t0 = Instant::now();
            while lock(&self.mixer).frame < need && !self.out_broken.load(Ordering::Relaxed) && t0.elapsed() < Duration::from_secs(3) {
                std::thread::sleep(Duration::from_millis(20));
            }
            if !self.out_broken.load(Ordering::Relaxed) && lock(&self.mixer).frame >= need {
                break;
            }
            if attempt == 2 {
                return Err(Error::Other("l'uscita audio non parte: controlla le cuffie e riprova".into()));
            }
            self.out_broken.store(true, Ordering::Relaxed);
        }
        if new_input {
            let t0 = Instant::now();
            while self.in_frames.load(Ordering::Relaxed) == 0 && t0.elapsed() < Duration::from_millis(1500) {
                std::thread::sleep(Duration::from_millis(20));
            }
            if self.in_frames.load(Ordering::Relaxed) == 0 {
                io.input = None;
                self.has_input.store(false, Ordering::Relaxed);
                return Err(Error::Other(
                    "il microfono non manda audio: controlla in Impostazioni di Sistema → Privacy e sicurezza → Microfono che Muse Sing sia attivo".into(),
                ));
            }
        }
        Ok(())
    }

    // --- jobs ------------------------------------------------------------------------------------

    /// Starts a new job (stopping the previous one) and returns its id. Events carry it, so the UI
    /// can ignore the late events of a job it already replaced.
    pub fn begin_job(&self, take: bool) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.job.store(id, Ordering::Relaxed);
        self.job_kind.store(if take { KIND_TAKE } else { 1 }, Ordering::Relaxed);
        let mut m = lock(&self.mixer);
        m.voices.clear();
        m.marks.clear();
        id
    }

    pub fn end_job(&self, id: u64) {
        if self.job.compare_exchange(id, 0, Ordering::Relaxed, Ordering::Relaxed).is_ok() {
            self.job_kind.store(KIND_NONE, Ordering::Relaxed);
        }
    }

    pub fn is_current(&self, id: u64) -> bool {
        self.job.load(Ordering::Relaxed) == id
    }

    /// Stops whatever is playing or recording.
    pub fn stop(&self) {
        self.job.store(0, Ordering::Relaxed);
        self.job_kind.store(KIND_NONE, Ordering::Relaxed);
        lock(&self.capture).recording = false;
        lock(&self.mixer).voices.clear();
    }

    fn out_rate(&self) -> Result<u32> {
        lock(&self.io).output.as_ref().map(|o| o.rate).ok_or_else(|| Error::Other("uscita audio chiusa".into()))
    }

    /// Schedules a buffer just ahead of the current output frame. Returns its start frame.
    fn schedule(&self, id: u64, buf: Vec<f32>, offset: usize, looping: bool, gain: f32, note: bool, rate: u32) -> u64 {
        self.schedule_stereo(id, buf, None, offset, looping, gain, note, rate)
    }

    #[allow(clippy::too_many_arguments)]
    fn schedule_stereo(&self, id: u64, buf: Vec<f32>, right: Option<Vec<f32>>, offset: usize, looping: bool, gain: f32, note: bool, rate: u32) -> u64 {
        let mut m = lock(&self.mixer);
        let start = m.frame + (SCHEDULE_AHEAD * rate as f64) as u64;
        let fade = if offset > 0 { (0.008 * rate as f64) as u64 } else { 0 };
        if note {
            m.voices.retain(|v| !v.note);
        }
        m.voices.push(Voice { id, buf: Arc::new(buf), right: right.map(Arc::new), start, offset, looping, gain, fade, note });
        start
    }

    /// Seconds heard since output frame `start` (negative before it).
    fn heard(&self, start: u64, rate: u32) -> f64 {
        let m = lock(&self.mixer);
        (m.frame as f64 - start as f64) / rate as f64 - m.latency
    }

    /// Ok(true) while the job should go on, Ok(false) if it was replaced or stopped, Err if the
    /// output broke under it.
    fn alive(&self, id: u64, generation: u64) -> Result<bool> {
        if !self.is_current(id) {
            return Ok(false);
        }
        if self.out_broken.load(Ordering::Relaxed) || self.generation.load(Ordering::Relaxed) != generation {
            return Err(Error::Other("l'audio si è interrotto (cuffie scollegate o riconfigurate?): riprova".into()));
        }
        Ok(true)
    }

    /// Plays the guide melody from `from_beats`, at `speed` (1 = written tempo), optionally in a loop.
    pub fn play_guide(&self, id: u64, ph: &Phrase, from_beats: f64, speed: f64, looping: bool, progress: &dyn Fn(Progress)) -> Result<()> {
        let rate = self.out_rate()?;
        let generation = self.generation.load(Ordering::Relaxed);
        let bpm = ph.bpm * speed.clamp(0.25, 2.0);
        let beat = 60.0 / bpm;
        let phrase_beats: f64 = ph.notes.iter().map(|n| n.beats).sum();
        let gap = if looping { LOOP_GAP_BEATS * beat } else { 0.3 };
        let buf = synth_melody(&ph.notes, bpm, rate, GUIDE_GAIN, gap);
        let len_s = buf.len() as f64 / rate as f64;
        let from = from_beats.clamp(0.0, phrase_beats);
        let offset_s = if from >= phrase_beats && !looping { 0.0 } else { from * beat };
        let start = self.schedule(id, buf, (offset_s * rate as f64) as usize, looping, 1.0, false, rate);
        while self.alive(id, generation)? {
            let mut pos = offset_s + self.heard(start, rate).max(0.0);
            if looping {
                pos %= len_s;
            } else if pos >= len_s {
                break;
            }
            progress(Progress { job: id, kind: "guide", phase: "sing", t: pos, total: phrase_beats * beat, beats: pos / beat, level: 0.0 });
            std::thread::sleep(Duration::from_millis(30));
        }
        Ok(())
    }

    /// Plays a WAV file (any rate, first channel) from `from_sec`. `bpm` maps the position to beats
    /// (the file has `LEAD_IN` seconds before the first note).
    pub fn play_file(&self, id: u64, path: &Path, from_sec: f64, bpm: f64, progress: &dyn Fn(Progress)) -> Result<()> {
        let rate = self.out_rate()?;
        let generation = self.generation.load(Ordering::Relaxed);
        let (mono, file_sr) = read_wav_mono(path)?;
        let buf = resample(&mono, file_sr, rate);
        let len_s = buf.len() as f64 / rate as f64;
        let from = if from_sec >= len_s - 0.05 { 0.0 } else { from_sec.max(0.0) };
        let start = self.schedule(id, buf, (from * rate as f64) as usize, false, 1.0, false, rate);
        let beat = 60.0 / bpm;
        while self.alive(id, generation)? {
            let pos = from + self.heard(start, rate).max(0.0);
            if pos >= len_s {
                break;
            }
            progress(Progress { job: id, kind: "play", phase: "sing", t: pos, total: len_s, beats: (pos - LEAD_IN) / beat, level: 0.0 });
            std::thread::sleep(Duration::from_millis(30));
        }
        Ok(())
    }

    /// Plays a stereo buffer (sung mix) from `from_sec`; `progress(position, total)` in seconds.
    /// With `loop_range` (from, to in seconds) it repeats that stretch until stopped.
    #[allow(clippy::too_many_arguments)]
    pub fn play_stereo(
        &self,
        id: u64,
        left: &[f32],
        right: &[f32],
        sr: u32,
        from_sec: f64,
        loop_range: Option<(f64, f64)>,
        progress: &dyn Fn(f64, f64),
    ) -> Result<()> {
        let rate = self.out_rate()?;
        let generation = self.generation.load(Ordering::Relaxed);
        let total_s = left.len() as f64 / sr as f64;
        let range = loop_range
            .map(|(a, b)| (a.clamp(0.0, total_s), b.clamp(0.0, total_s)))
            .filter(|(a, b)| b - a >= 0.5);
        let (base, l, r) = match range {
            Some((a, b)) => {
                let (i, j) = ((a * sr as f64) as usize, (b * sr as f64) as usize);
                (a, resample(&left[i..j], sr, rate), resample(&right[i..j], sr, rate))
            }
            None => (0.0, resample(left, sr, rate), resample(right, sr, rate)),
        };
        let len_s = l.len() as f64 / rate as f64;
        let rel = from_sec - base;
        let from = if rel < 0.0 || rel >= len_s - 0.05 { 0.0 } else { rel };
        let start = self.schedule_stereo(id, l, Some(r), (from * rate as f64) as usize, range.is_some(), 1.0, false, rate);
        while self.alive(id, generation)? {
            let mut pos = from + self.heard(start, rate).max(0.0);
            if range.is_some() {
                pos %= len_s;
            } else if pos >= len_s {
                break;
            }
            progress(base + pos, total_s);
            std::thread::sleep(Duration::from_millis(50));
        }
        Ok(())
    }

    /// Plays a stereo buffer once and records the microphone over it («Canta e controlla»).
    /// `stop` ends it early and keeps what was sung; another job replacing it gives None. Sample 0
    /// of the result is the moment frame 0 of the buffer is heard. `progress(t, total, level)`.
    pub fn play_and_record(&self, id: u64, left: &[f32], right: &[f32], sr: u32, progress: &dyn Fn(f64, f64, f32)) -> Result<Option<Recorded>> {
        let (rate, in_rate, ch) = {
            let io = lock(&self.io);
            let out = io.output.as_ref().ok_or_else(|| Error::Other("uscita audio chiusa".into()))?;
            let inp = io.input.as_ref().ok_or_else(|| Error::Other("il microfono non è aperto".into()))?;
            (out.rate, inp.rate, inp.channels.max(1))
        };
        let generation = self.generation.load(Ordering::Relaxed);
        let (l, r) = (resample(left, sr, rate), resample(right, sr, rate));
        let total = l.len() as f64 / rate as f64;
        {
            let mut c = lock(&self.capture);
            c.samples = Vec::with_capacity(((total + 2.0) * in_rate as f64) as usize * ch);
            c.owner = id;
            c.recording = true;
        }
        let _rec = StopRecording(&self.capture, id);
        let start = self.schedule_stereo(id, l, Some(r), 0, false, 1.0, false, rate);
        let _voices = DropVoices(&self.mixer, id);
        let stopped = loop {
            if !self.is_current(id) {
                if self.job.load(Ordering::Relaxed) != 0 {
                    return Ok(None); // another job took over
                }
                break true;
            }
            if self.out_broken.load(Ordering::Relaxed) || self.generation.load(Ordering::Relaxed) != generation {
                return Err(Error::Other("l'audio si è interrotto (cuffie scollegate o riconfigurate?): riprova".into()));
            }
            if self.in_broken.load(Ordering::Relaxed) {
                return Err(Error::Other("il microfono si è interrotto durante la registrazione: riprova".into()));
            }
            let t = self.heard(start, rate);
            if t >= total {
                break false;
            }
            progress(t.max(0.0), total, f32::from_bits(self.level.load(Ordering::Relaxed)));
            std::thread::sleep(Duration::from_millis(30));
        };
        let heard = self.heard(start, rate).clamp(0.0, total);

        // where frame 0 of the buffer is in the input
        let (out_ts, mark) = {
            let m = lock(&self.mixer);
            (m.ts, m.marks.iter().find(|(v, _)| *v == id).map(|(_, f)| *f))
        };
        let in_ts = lock(&self.capture).ts;
        let by_ts = match (out_ts, in_ts) {
            (Some(o), Some(i)) => align_input_frame(start, o, rate, i, in_rate),
            _ => None,
        };
        let Some(click) = choose_click_frame(by_ts, mark, in_rate) else {
            // stopped before anything was heard: nothing to judge
            if stopped {
                return Ok(None);
            }
            return Err(Error::Other("registrazione non allineabile: riprova".into()));
        };
        crate::log!(
            "canta e controlla: allineato con {} (timestamp {:?}, contatore {:?}), {:.1} s ascoltati{}",
            if by_ts.is_some() && Some(click) == by_ts { "timestamp" } else { "contatore" },
            by_ts,
            mark,
            heard,
            if stopped { ", fermato" } else { "" }
        );
        let to = click + (heard * in_rate as f64) as u64;
        if !stopped {
            // wait for the input to deliver the last frames
            let t0 = Instant::now();
            while t0.elapsed() < Duration::from_millis(1500) {
                let c = lock(&self.capture);
                if c.start_frame + (c.samples.len() / ch) as u64 >= to {
                    break;
                }
                drop(c);
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        let (all, first) = {
            let mut c = lock(&self.capture);
            if c.owner != id {
                return Ok(None);
            }
            c.recording = false;
            (std::mem::take(&mut c.samples), c.start_frame)
        };
        let samples = trim_take(&all, first, ch, click, LEAD_IN, heard, in_rate);
        Ok(Some(Recorded { samples, sample_rate: in_rate }))
    }

    /// One note, to check a pitch (refused while recording). Replaces the previous note.
    pub fn play_note(&self, pitch: i32, secs: f64) -> Result<()> {
        if self.job_kind.load(Ordering::Relaxed) == KIND_TAKE {
            return Err(Error::Other("durante la registrazione le note non suonano".into()));
        }
        let rate = self.out_rate()?;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.schedule(id, synth_note(pitch, secs.clamp(0.15, 3.0), rate), 0, false, 1.0, true, rate);
        Ok(())
    }

    /// Count-in + phrase + tail. Returns None if stopped by the user. The result is trimmed to
    /// LEAD_IN seconds before the first note, with the loudest input channel kept.
    pub fn record_take(&self, id: u64, ph: &Phrase, guide: bool, start_note: bool, progress: &dyn Fn(Progress)) -> Result<Option<Recorded>> {
        let (rate, in_rate, ch) = {
            let io = lock(&self.io);
            let out = io.output.as_ref().ok_or_else(|| Error::Other("uscita audio chiusa".into()))?;
            let inp = io.input.as_ref().ok_or_else(|| Error::Other("il microfono non è aperto".into()))?;
            (out.rate, inp.rate, inp.channels.max(1))
        };
        let generation = self.generation.load(Ordering::Relaxed);
        let count_in = count_in_secs(ph);
        let sing = phrase_secs(ph);
        let total = count_in + sing + TAIL;
        let beat = 60.0 / ph.bpm;
        {
            let mut c = lock(&self.capture);
            c.samples = Vec::with_capacity(((total + 2.0) * in_rate as f64) as usize * ch);
            c.owner = id;
            c.recording = true;
        }
        let _rec = StopRecording(&self.capture, id);
        let start = self.schedule(id, synth_take(ph, rate, guide, start_note), 0, false, 1.0, false, rate);
        loop {
            if !self.alive(id, generation)? {
                return Ok(None);
            }
            if self.in_broken.load(Ordering::Relaxed) {
                return Err(Error::Other("il microfono si è interrotto durante la registrazione: riprova".into()));
            }
            let t = self.heard(start, rate);
            if t >= total {
                break;
            }
            let phase = if t < 0.0 {
                "wait"
            } else if t < count_in {
                "countin"
            } else if t < count_in + sing {
                "sing"
            } else {
                "tail"
            };
            let level = f32::from_bits(self.level.load(Ordering::Relaxed));
            progress(Progress { job: id, kind: "take", phase, t, total, beats: (t - count_in) / beat, level });
            std::thread::sleep(Duration::from_millis(30));
        }

        // where the first click is in the input
        let (out_ts, mark) = {
            let m = lock(&self.mixer);
            (m.ts, m.marks.iter().find(|(v, _)| *v == id).map(|(_, f)| *f))
        };
        let in_ts = lock(&self.capture).ts;
        let by_ts = match (out_ts, in_ts) {
            (Some(o), Some(i)) => align_input_frame(start, o, rate, i, in_rate),
            _ => None,
        };
        let click = choose_click_frame(by_ts, mark, in_rate).ok_or_else(|| Error::Other("registrazione non allineabile: riprova".into()))?;
        let to = click + (total * in_rate as f64) as u64;
        // wait for the input to deliver the last frames
        let t0 = Instant::now();
        loop {
            {
                let c = lock(&self.capture);
                if c.start_frame + (c.samples.len() / ch) as u64 >= to || t0.elapsed() > Duration::from_millis(1500) {
                    break;
                }
            }
            if !self.alive(id, generation)? {
                return Ok(None);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let (all, first) = {
            let mut c = lock(&self.capture);
            if c.owner != id {
                return Ok(None);
            }
            c.recording = false;
            (std::mem::take(&mut c.samples), c.start_frame)
        };
        let samples = trim_take(&all, first, ch, click, count_in, total, in_rate);
        if samples.is_empty() {
            return Err(Error::Other("il microfono non ha registrato niente: riprova".into()));
        }
        Ok(Some(Recorded { samples, sample_rate: in_rate }))
    }
}

/// The saved part of a take: from `LEAD_IN` s before the first note to the end of the tail, on the
/// loudest channel. `all` is the interleaved input from frame `first`, `click` the input frame of
/// the first count-in click.
pub fn trim_take(all: &[f32], first: u64, ch: usize, click: u64, count_in: f64, total: f64, in_rate: u32) -> Vec<f32> {
    let from = click + ((count_in - LEAD_IN).max(0.0) * in_rate as f64) as u64;
    let to = click + (total * in_rate as f64) as u64;
    let mono = loudest_channel(all, ch);
    let a = (from.saturating_sub(first) as usize).min(mono.len());
    let b = (to.saturating_sub(first) as usize).min(mono.len());
    if a < b { mono[a..b].to_vec() } else { vec![] }
}

pub struct Recorded {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

/// Interleaved -> the channel with the most energy (a mic on input 1 of a 2-input interface).
pub fn loudest_channel(inter: &[f32], ch: usize) -> Vec<f32> {
    if ch <= 1 {
        return inter.to_vec();
    }
    let energy = |c: usize| inter.iter().skip(c).step_by(ch).map(|x| (x * x) as f64).sum::<f64>();
    let best = (0..ch).max_by(|a, b| energy(*a).total_cmp(&energy(*b))).unwrap_or(0);
    inter.iter().skip(best).step_by(ch).copied().collect()
}

pub fn resample(x: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || x.is_empty() {
        return x.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let n = (x.len() as u64 * to as u64 / from as u64) as usize;
    (0..n)
        .map(|i| {
            let p = i as f64 * ratio;
            let k = p as usize;
            let f = (p - k as f64) as f32;
            let a = x[k.min(x.len() - 1)];
            let b = x[(k + 1).min(x.len() - 1)];
            a + (b - a) * f
        })
        .collect()
}

/// 24-bit mono WAV.
pub fn write_wav(path: &Path, samples: &[f32], sr: u32) -> Result<()> {
    let spec = hound::WavSpec { channels: 1, sample_rate: sr, bits_per_sample: 24, sample_format: hound::SampleFormat::Int };
    let tmp = path.with_extension("wav.part");
    let mut w = hound::WavWriter::create(&tmp, spec).map_err(|e| Error::Other(format!("scrittura WAV: {e}")))?;
    for &s in samples {
        w.write_sample((s.clamp(-1.0, 1.0) * 8_388_607.0) as i32).map_err(|e| Error::Other(format!("scrittura WAV: {e}")))?;
    }
    w.finalize().map_err(|e| Error::Other(format!("scrittura WAV: {e}")))?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

pub fn read_wav_mono(path: &Path) -> Result<(Vec<f32>, u32)> {
    let mut r = hound::WavReader::open(path).map_err(|e| Error::Other(format!("lettura WAV: {e}")))?;
    let spec = r.spec();
    let ch = spec.channels as usize;
    let inter: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().filter_map(|s| s.ok()).collect(),
        hound::SampleFormat::Int => {
            let scale = (1i64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>().filter_map(|s| s.ok()).map(|s| s as f32 / scale).collect()
        }
    };
    Ok((inter.iter().step_by(ch.max(1)).copied().collect(), spec.sample_rate))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::{base_corpus, VoiceProfile};

    fn phrase() -> Phrase {
        base_corpus(&VoiceProfile { name: "x".into(), voice: "soprano".into(), low: 60, high: 79 }).remove(0)
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    #[test]
    fn take_guide_length_and_level() {
        let ph = phrase();
        let g = synth_take(&ph, 48_000, true, true);
        let expect = count_in_secs(&ph) + phrase_secs(&ph) + 0.3;
        assert!((g.len() as f64 / 48_000.0 - expect).abs() < 0.01);
        let peak = g.iter().fold(0f32, |m, x| m.max(x.abs()));
        assert!(peak > 0.1 && peak < 1.0, "peak {peak}");
    }

    #[test]
    fn first_click_at_zero_and_start_note_only_when_asked() {
        let ph = phrase();
        let sr = 48_000;
        let beat = (60.0 / ph.bpm * sr as f64) as usize;
        let with = synth_take(&ph, sr, false, true);
        let without = synth_take(&ph, sr, false, false);
        // the accented click is in the first 40 ms
        assert!(rms(&without[..sr as usize / 25]) > 0.05);
        // between clicks 3 and 4 (after the click decays) only the starting note sounds
        let mid = &with[2 * beat + beat / 2..3 * beat - 10];
        let mid0 = &without[2 * beat + beat / 2..3 * beat - 10];
        assert!(rms(mid) > 0.01, "starting note missing");
        assert!(rms(mid0) < 1e-4);
        // and the melody is not there before the count-in ends
        assert!(rms(&with[..4 * beat - beat / 4]) < 0.2);
    }

    #[test]
    fn melody_tempo_scales_length() {
        let ph = phrase();
        let a = synth_melody(&ph.notes, ph.bpm, 48_000, 0.3, 0.0);
        let b = synth_melody(&ph.notes, ph.bpm * 0.5, 48_000, 0.3, 0.0);
        let ra = b.len() as f64 / a.len() as f64;
        assert!((ra - 2.0).abs() < 0.01, "{ra}");
    }

    #[test]
    fn mixer_schedules_loops_and_marks() {
        let mut m = Mixer::default();
        m.voices.push(Voice { id: 7, buf: Arc::new(vec![1.0; 4]), right: None, start: 2, offset: 0, looping: false, gain: 0.5, fade: 0, note: false });
        m.voices.push(Voice { id: 8, buf: Arc::new(vec![0.25, 0.0]), right: None, start: 0, offset: 1, looping: true, gain: 1.0, fade: 0, note: false });
        let mut out = vec![0f32; 8];
        m.render(8, |i, v, _| out[i] = v, 1000, 1.0);
        assert_eq!(out, vec![0.0, 0.25, 0.5, 0.75, 0.5, 0.75, 0.0, 0.25]);
        assert_eq!(m.frame, 8);
        // the one-shot voice is gone, the loop stays; marks record the input counter at the start
        assert_eq!(m.voices.len(), 1);
        assert!(m.marks.contains(&(7, 1002)) && m.marks.contains(&(8, 1000)));
    }

    #[test]
    fn aligns_with_latency_and_falls_back() {
        // output frame 48000 is heard at 10.5 s (playback ref: frame 0 at 9.5 s);
        // input frame 1000 was captured at 10.0 s -> 10.5 s is input frame 1000 + 0.5 * 44100
        let f = align_input_frame(48_000, (0, 9.5), 48_000, (1000, 10.0), 44_100).unwrap();
        assert_eq!(f, 1000 + 22_050);
        // timestamps close to the counter estimate -> use them
        assert_eq!(choose_click_frame(Some(f), Some(f - 8000), 44_100), Some(f));
        // implausible (e.g. clocks from different bases) -> the counter
        assert_eq!(choose_click_frame(Some(f + 90_000), Some(f), 44_100), Some(f));
        assert_eq!(choose_click_frame(None, Some(5), 44_100), Some(5));
        assert_eq!(choose_click_frame(Some(9), None, 44_100), Some(9));
        // before the input started
        assert_eq!(align_input_frame(0, (0, 0.0), 48_000, (0, 5.0), 48_000), None);
        assert_eq!(instant_secs(StreamInstant::ZERO), None);
    }

    /// A take through a simulated loopback: the output plays with 200 ms of (Bluetooth) latency at
    /// 48 kHz, the microphone hears it and records at 44.1 kHz on two channels, starting at another
    /// moment. Alignment by timestamps + trimming must put the first note at LEAD_IN.
    #[test]
    fn simulated_take_puts_first_note_at_lead_in() {
        let ph = phrase();
        let (out_sr, in_sr, out_lat, in_start) = (48_000u32, 44_100u32, 0.2, 0.037);
        // render through the mixer in 10 ms chunks, the take scheduled like Engine::schedule
        let mut m = Mixer::default();
        m.render(4800, |_, _, _| {}, 0, 0.0);
        let start = m.frame + (SCHEDULE_AHEAD * out_sr as f64) as u64;
        let buf = synth_take(&ph, out_sr, true, false);
        let mut out = vec![0f32; start as usize + buf.len() + out_sr as usize * 2];
        m.voices.push(Voice { id: 1, buf: Arc::new(buf), right: None, start, offset: 0, looping: false, gain: 1.0, fade: 0, note: false });
        while (m.frame as usize) < out.len() - 480 {
            let f0 = m.frame as usize;
            m.render(480, |i, v, _| out[f0 + i] = v, 0, 0.0);
        }
        // the microphone: input frame j is captured at in_start + j / in_sr and hears the output
        // frame played at that moment (played at frame / out_sr + out_lat)
        let n_in = ((out.len() as f64 / out_sr as f64) * in_sr as f64) as usize;
        let mut inter = Vec::with_capacity(n_in * 2);
        for j in 0..n_in {
            let t = in_start + j as f64 / in_sr as f64;
            let f = ((t - out_lat) * out_sr as f64).round();
            let v = if f >= 0.0 { out.get(f as usize).copied().unwrap_or(0.0) } else { 0.0 };
            inter.extend([v * 0.01, v]); // the mic on input 2
        }
        let out_ref = (m.frame, m.frame as f64 / out_sr as f64 + out_lat);
        let in_ref = (n_in as u64 - 441, in_start + (n_in - 441) as f64 / in_sr as f64);
        let click = align_input_frame(start, out_ref, out_sr, in_ref, in_sr).unwrap();
        let total = count_in_secs(&ph) + phrase_secs(&ph) + TAIL;
        let take = trim_take(&inter, 0, 2, click, count_in_secs(&ph), total, in_sr);
        assert!((take.len() as f64 / in_sr as f64 - (LEAD_IN + phrase_secs(&ph) + TAIL)).abs() < 0.01, "length {}", take.len());
        // silence before the first note (the count-in is cut away), then the note
        let onset = take.iter().position(|v| v.abs() > 0.02).unwrap() as f64 / in_sr as f64;
        assert!((onset - LEAD_IN).abs() < 0.005, "first note at {onset:.4} s");
    }

    #[test]
    fn bluetooth_names() {
        assert!(looks_bluetooth("AirPods Pro di Emanuele", None));
        assert!(looks_bluetooth("Cuffie", Some(InterfaceType::Bluetooth)));
        assert!(!looks_bluetooth("Microfono MacBook Air", Some(InterfaceType::BuiltIn)));
        assert!(!looks_bluetooth("Scarlett 2i2 USB", None));
    }

    #[test]
    fn wav_roundtrip_24bit() {
        let dir = std::env::temp_dir().join(format!("muse-sing-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.wav");
        let x: Vec<f32> = (0..4410).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
        write_wav(&p, &x, 44_100).unwrap();
        let (y, sr) = read_wav_mono(&p).unwrap();
        assert_eq!(sr, 44_100);
        assert_eq!(y.len(), x.len());
        assert!(x.iter().zip(&y).all(|(a, b)| (a - b).abs() < 1e-5));
        let spec = hound::WavReader::open(&p).unwrap().spec();
        assert_eq!((spec.channels, spec.bits_per_sample), (1, 24));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn picks_loudest_channel_and_resamples() {
        let inter = [0.0, 0.5, 0.0, -0.5, 0.0, 0.5];
        assert_eq!(loudest_channel(&inter, 2), vec![0.5, -0.5, 0.5]);
        let x: Vec<f32> = (0..48_000).map(|i| i as f32).collect();
        assert_eq!(resample(&x, 48_000, 44_100).len(), 44_100);
    }
}
