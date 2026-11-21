//! Development tool: renders a score the way the app does (every sung part, accompaniment, mix)
//! and exports it, to listen to engine and mix changes outside the app.
//!
//!   cargo run --release --example sing_mix -- <score> <voicebank dir> <out.mp3>
//!       [--voices 1|2|3] [--ambience nessuno|stanza|chiesa|cattedrale] [--reverb 0..1] [--color COLOR] [--no-accomp]
//!       [--soundfont GeneralUser-GS.sf2] [--instrument organo_canne|pianoforte|archi|…] [--metronome] [--played PART]
//!       [--no-marks] [--transpose N]
use std::path::PathBuf;

use muse_sing_lib::fx::Ambience;
use muse_sing_lib::lyrics::detect_lang;
use muse_sing_lib::mix::{self, MixPart};
use muse_sing_lib::sing::{PartReq, RenderRequest, Sing};
use muse_sing_lib::singers::SingerInfo;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opt = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let pos: Vec<&String> = {
        let (mut v, mut i) = (vec![], 0);
        while i < args.len() {
            if ["--no-accomp", "--metronome", "--no-marks"].contains(&args[i].as_str()) {
                i += 1
            } else if args[i].starts_with("--") {
                i += 2
            } else {
                v.push(&args[i]);
                i += 1
            }
        }
        v
    };
    if pos.len() < 3 {
        eprintln!("usage: sing_mix <score> <voicebank dir> <out.mp3> [--voices N] [--ambience A] [--color C] [--no-accomp]");
        std::process::exit(2);
    }
    let (score, vb, out) = (PathBuf::from(pos[0]), PathBuf::from(pos[1]), PathBuf::from(pos[2]));
    let voices: u8 = opt("--voices").and_then(|v| v.parse().ok()).unwrap_or(1);
    let ambience: Ambience = opt("--ambience").map(|a| serde_json::from_str(&format!("\"{a}\"")).expect("ambience")).unwrap_or_default();
    let sc = muse_sing_lib::score::read_score(&score).expect("score");
    let lang = detect_lang(&sc.parts);
    let singer = SingerInfo {
        id: "vb".into(),
        name: "vb".into(),
        singer_type: "diffsinger".into(),
        langs: vec![],
        path: vb.to_string_lossy().into_owned(),
        openutau: false,
        engine: true,
        engine_note: None,
        colors: vec![],
    };
    let sung: Vec<String> = sc.parts.iter().filter(|p| p.is_sung()).map(|p| p.name.clone()).collect();
    let req = RenderRequest {
        path: score.to_string_lossy().into_owned(),
        lang,
        pitch: "model".into(),
        parts: sung.iter().map(|n| PartReq { name: n.clone(), singer: "vb".into(), color: opt("--color"), voices, played: opt("--played").as_deref() == Some(n.as_str()) }).collect(),
        tempo_factor: 1.0,
        accompaniment: !args.iter().any(|a| a == "--no-accomp"),
        edits: Default::default(),
        instruments: match opt("--instrument") {
            Some(i) => {
                let inst: muse_sing_lib::instruments::Instrument = serde_json::from_str(&format!("\"{i}\"")).expect("instrument");
                sc.parts.iter().filter(|p| !p.is_sung()).map(|p| (p.name.clone(), inst)).collect()
            }
            None => Default::default(),
        },
        marks: !args.iter().any(|a| a == "--no-marks"),
        transpose: opt("--transpose").and_then(|v| v.parse().ok()).unwrap_or(0),
        soundfont: opt("--soundfont").map(PathBuf::from),
    };
    let cache = std::env::temp_dir().join("muse-sing-mix-cache");
    let sing = Sing::new(Some(cache));
    let t = std::time::Instant::now();
    let job = sing.begin();
    let (warnings, seconds) = sing
        .run(job, &req, std::slice::from_ref(&singer), &|p| {
            if p.phrase == p.phrases {
                eprintln!("{}: {} frasi", p.part, p.phrases)
            }
        })
        .expect("render")
        .expect("not cancelled");
    eprintln!("{seconds:.1} s di brano in {:.1} s; avvisi: {warnings:?}", t.elapsed().as_secs_f64());
    let parts: Vec<MixPart> = sing
        .with_tracks(|t| t.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>())
        .unwrap()
        .into_iter()
        .map(|name| {
            let accomp = name == muse_sing_lib::project::ACCOMPANIMENT || name == muse_sing_lib::project::METRONOME;
            let mute = name == muse_sing_lib::project::METRONOME && !args.iter().any(|a| a == "--metronome");
            MixPart { name, volume_db: if accomp { -6.0 } else { 0.0 }, pan: 0.0, mute, solo: false }
        })
        .collect();
    let (l, r) = sing.with_tracks(|t| mix::mix_in(t, &parts, ambience, opt("--reverb").and_then(|v| v.parse().ok()).unwrap_or(muse_sing_lib::fx::DEFAULT_AMOUNT))).unwrap();
    mix::export(&out, &l, &r, muse_sing_lib::ds::render::SAMPLE_RATE, "prova").expect("export");
    eprintln!("scritto {}", out.display());
}
