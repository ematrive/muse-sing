//! End-to-end render with a real voicebank: set MUSESING_TEST_VOICEBANK to a DiffSinger voicebank
//! folder (e.g. TIGER DS). Skipped otherwise, and when the score fixtures are missing.
use std::path::PathBuf;

use muse_sing_lib::lyrics::Lang;
use muse_sing_lib::mix::{self, MixPart};
use muse_sing_lib::sing::{PartReq, RenderRequest, Sing};
use muse_sing_lib::singers::SingerInfo;

#[test]
fn renders_mixes_and_reuses_the_cache() {
    let Some(vb) = std::env::var_os("MUSESING_TEST_VOICEBANK").map(PathBuf::from) else {
        eprintln!("MUSESING_TEST_VOICEBANK not set: skipped");
        return;
    };
    let score = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/agnello.mscz");
    if !score.exists() {
        eprintln!("fixture missing: skipped");
        return;
    }
    let singer = SingerInfo {
        id: "test".into(),
        name: "test".into(),
        singer_type: "diffsinger".into(),
        langs: vec![],
        path: vb.to_string_lossy().into_owned(),
        openutau: false,
        engine: true,
        engine_note: None,
        colors: vec![],
    };
    let cache = std::env::temp_dir().join(format!("muse-sing-sing-{}", std::process::id()));
    let sing = Sing::new(Some(cache.clone()));
    let req = RenderRequest {
        path: score.to_string_lossy().into_owned(),
        lang: Lang::It,
        pitch: "notes".into(),
        parts: vec![PartReq { name: "Soprano".into(), singer: "test".into(), color: None }],
    };
    let t = std::time::Instant::now();
    let job = sing.begin();
    let (warnings, secs) = sing.run(job, &req, std::slice::from_ref(&singer), &|_| {}).unwrap().expect("not cancelled");
    eprintln!("rendered {secs:.1}s in {:.1}s, warnings: {warnings:?}", t.elapsed().as_secs_f64());
    assert!(secs > 10.0);
    assert!(warnings.iter().all(|w| !w.contains("non riconosciuti")), "{warnings:?}");

    let part = MixPart { name: "Soprano".into(), volume_db: 0.0, pan: -0.5, mute: false };
    let (l, r) = sing.with_tracks(|t| mix::mix(t, std::slice::from_ref(&part))).unwrap();
    let rms = |x: &[f32]| (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt();
    assert!(rms(&l) > 0.01, "not silent");
    assert!(rms(&l) > rms(&r), "panned left");

    // second time from the cache, and a cancelled job leaves the tracks alone
    let t = std::time::Instant::now();
    let job = sing.begin();
    sing.run(job, &req, std::slice::from_ref(&singer), &|p| assert!(p.cached)).unwrap().unwrap();
    assert!(t.elapsed().as_secs_f64() < 5.0);
    let job = sing.begin();
    sing.cancel();
    assert!(sing.run(job, &req, std::slice::from_ref(&singer), &|_| {}).unwrap().is_none());
    std::fs::remove_dir_all(cache).ok();
}

/// The voicebank is found in a Singers folder, usable by the engine, and Italian hints written in
/// G2p symbols come out as the same phonemes as plain words.
#[test]
fn finds_the_singer_and_remaps_italian_hints() {
    let Some(vb) = std::env::var_os("MUSESING_TEST_VOICEBANK").map(PathBuf::from) else {
        eprintln!("MUSESING_TEST_VOICEBANK not set: skipped");
        return;
    };
    let root = vb.parent().unwrap();
    let all = muse_sing_lib::singers::list_singers(Some(root));
    let s = all.iter().find(|s| std::path::Path::new(&s.path) == vb).expect("voicebank listed");
    eprintln!("{} engine={} note={:?} colors={:?} langs={:?}", s.name, s.engine, s.engine_note, s.colors, s.langs);
    assert!(s.engine, "{:?}", s.engine_note);
    assert!(s.id.starts_with(muse_sing_lib::singers::OWN_PREFIX) && !s.openutau);
    assert!(s.langs.iter().any(|l| l == "it"));

    use muse_sing_lib::ds::{phonemize::{phonemize, PNote}, time::TimeAxis, voicebank::Voicebank};
    let v = Voicebank::load(&vb).unwrap();
    let axis = TimeAxis::new(&[(0, 120.0)], 480);
    let note = |pos: i64, lyric: &str| PNote { position: pos, duration: 480, tone: 64, lyric: lyric.into() };
    let words = |notes: &[PNote]| -> Vec<String> {
        let ph = phonemize(&v, Lang::It, notes, &axis, &v.speaker_suffix(None)).unwrap();
        assert!(ph.warnings.is_empty(), "{:?}", ph.warnings);
        ph.phones.iter().map(|p| p.symbol.clone()).collect()
    };
    let hinted = words(&[note(0, "terra[t e rr a]"), note(480, "+"), note(1440, "gloria[g l o r y a]"), note(1920, "+"), note(2400, "+")]);
    eprintln!("hinted: {hinted:?}");
    for s in ["tx", "lg", "aa", "ee", "oo"] {
        assert!(hinted.iter().any(|p| p == s), "{s} missing in {hinted:?}");
    }
    for s in ["t", "l", "a", "e", "o"] {
        assert!(!hinted.iter().any(|p| p == s), "{s} not remapped in {hinted:?}");
    }
    let plain = words(&[note(0, "signore"), note(480, "+"), note(960, "+")]);
    assert!(plain.iter().any(|p| p == "ny"), "{plain:?}");
}
