//! Development tool: renders the sung parts of a MuseScore file with a DiffSinger voicebank.
//!
//!   cargo run --release --example ds_render -- <score.mscz> <voicebank dir> <out.wav>
//!       [--part NAME] [--lang it|en] [--pitch model|notes] [--phrases N] [--steps N] [--speaker COLOR]
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use muse_sing_lib::ds::{self, render::{PitchSource, Settings}, voicebank::Voicebank};
use muse_sing_lib::lyrics::{detect_lang, Lang};
use muse_sing_lib::{score, ustx};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let pos: Vec<&String> = {
        let mut v = vec![];
        let mut i = 0;
        while i < args.len() {
            if args[i].starts_with("--") { i += 2 } else { v.push(&args[i]); i += 1 }
        }
        v
    };
    let opt = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    if pos.len() < 3 {
        eprintln!("usage: ds_render <score.mscz> <voicebank dir> <out.wav> [--part NAME] [--lang it|en] [--pitch model|notes] [--phrases N] [--steps N] [--speaker COLOR]");
        std::process::exit(2);
    }
    let (score_path, vb_dir, out) = (PathBuf::from(pos[0]), PathBuf::from(pos[1]), PathBuf::from(pos[2]));
    let sc = score::read_mscz(&score_path).expect("score");
    let lang = match opt("--lang").as_deref() { Some("it") => Lang::It, Some("en") => Lang::En, _ => detect_lang(&sc.parts) };
    let parts = opt("--part").map(|p| vec![p]);
    let hints = HashMap::new();
    let project = ustx::build(&sc, &ustx::Options { lang, singer: "", part_singers: None, parts: parts.as_deref(), hints: &hints, fallback_name: "score" });
    let axis = ds::project_axis(&project);
    let t0 = Instant::now();
    let vb = Voicebank::load(&vb_dir).expect("voicebank");
    let settings = Settings {
        pitch: if opt("--pitch").as_deref() == Some("notes") { PitchSource::Notes } else { PitchSource::Model },
        steps: opt("--steps").and_then(|s| s.parse().ok()).unwrap_or(20),
        speaker: opt("--speaker"),
        ..Settings::default()
    };
    let max_phrases: Option<usize> = opt("--phrases").and_then(|s| s.parse().ok());
    let mut mix: Vec<f32> = vec![];
    for (name, notes) in ds::project_notes(&project) {
        let mut ph = ds::phonemize::phonemize(&vb, lang, &notes, &axis, &vb.speaker_suffix(settings.speaker.as_deref())).expect("phonemize");
        for w in &ph.warnings { eprintln!("[{name}] {w}"); }
        if let Some(n) = max_phrases {
            let list = ds::render::phrases(&ph, &notes, &axis);
            if let Some(cut) = list.get(n) {
                let end = cut.ticks[0].0;
                ph.phones.retain(|p| p.position < end);
            }
        }
        if std::env::var_os("DS_NOTES").is_some() {
            for n in &notes { eprintln!("note {:7.2}s {:7.2}s {:3} {}", axis.tick_to_ms(n.position as f64) / 1000.0, axis.tick_to_ms(n.end() as f64) / 1000.0, n.tone, n.lyric); }
        }
        let shown: Vec<String> = ph.phones.iter().take(40).map(|p| format!("{}@{}-{}", p.symbol, p.position, p.end)).collect();
        eprintln!("[{name}] {} phonemes: {} ...", ph.phones.len(), shown.join(" "));
        let t = Instant::now();
        let (buf, warnings) = ds::render::render_track(&vb, &ph, &notes, &axis, &settings, |i, n| { eprint!("\r[{name}] phrase {i}/{n}"); true })
            .expect("render").expect("not cancelled");
        eprintln!("\n[{name}] rendered {:.1}s of audio in {:.1}s", buf.len() as f64 / 44100.0, t.elapsed().as_secs_f64());
        for w in warnings { eprintln!("[{name}] {w}"); }
        if mix.len() < buf.len() { mix.resize(buf.len(), 0.0); }
        for (m, v) in mix.iter_mut().zip(buf) { *m += v; }
    }
    let peak = mix.iter().fold(0f32, |a, &b| a.max(b.abs()));
    let gain = if peak > 0.98 { 0.98 / peak } else { 1.0 };
    let spec = hound::WavSpec { channels: 1, sample_rate: 44100, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut w = hound::WavWriter::create(&out, spec).expect("wav");
    for v in &mix { w.write_sample((v * gain * 32767.0).round().clamp(-32768.0, 32767.0) as i16).unwrap(); }
    w.finalize().unwrap();
    eprintln!("peak {peak:.3}, total {:.1}s -> {}", t0.elapsed().as_secs_f64(), out.display());
}
