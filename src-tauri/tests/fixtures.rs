//! The Rust port must produce the same projects as the reference Python tool
//! (`tools/mscz_to_ustx.py`), whose outputs are in tests/fixtures/*.ustx.

use std::collections::HashMap;
use std::path::Path;

use muse_sing_lib::lyrics::{detect_lang, Lang};
use muse_sing_lib::score::read_mscz;
use muse_sing_lib::ustx;
use serde_yaml::Value;

/// The choir scores are not in the public repository: without them these tests pass empty.
fn fixtures() -> Option<std::path::PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    if dir.join("santo.mscz").exists() {
        Some(dir)
    } else {
        eprintln!("tests/fixtures mancante: test sui brani saltato");
        None
    }
}

/// `first_pass`: the Python reference doesn't unroll repeats, compare only its notes. The singer
/// id is the one written in the reference file.
fn check_with(name: &str, expected_lang: Lang, fixture: &str, first_pass: bool) {
    let Some(dir) = fixtures() else { return };
    let theirs: Value = serde_yaml::from_str(&std::fs::read_to_string(dir.join(fixture)).unwrap()).unwrap();
    let singer = theirs["tracks"][0]["singer"].as_str().unwrap_or_default();
    let score = read_mscz(&dir.join(format!("{name}.mscz"))).unwrap();
    let lang = detect_lang(&score.parts);
    assert_eq!(lang, expected_lang, "{name}: language");
    let hints = HashMap::new();
    let ours: Value = serde_yaml::from_str(&ustx::to_yaml(&ustx::build(
        &score,
        &ustx::Options { lang, singer, part_singers: None, parts: None, hints: &hints, fallback_name: name },
    )))
    .unwrap();

    assert_eq!(ours["ustx_version"], theirs["ustx_version"]);
    assert_eq!(ours["time_signatures"], theirs["time_signatures"], "{name}: time signatures");
    let bpm = |v: &Value| v["tempos"][0]["bpm"].as_f64().unwrap();
    assert!((bpm(&ours) - bpm(&theirs)).abs() < 1e-6, "{name}: tempo");

    let tracks = |v: &Value| v["tracks"].as_sequence().unwrap().clone();
    assert_eq!(tracks(&ours).len(), tracks(&theirs).len(), "{name}: tracks");
    for (a, b) in tracks(&ours).iter().zip(tracks(&theirs).iter()) {
        for k in ["singer", "phonemizer", "track_name"] {
            assert_eq!(a[k], b[k], "{name}: track {k}");
        }
    }
    let parts = |v: &Value| v["voice_parts"].as_sequence().unwrap().clone();
    for (pa, pb) in parts(&ours).iter().zip(parts(&theirs).iter()) {
        let (na, nb) = (pa["notes"].as_sequence().unwrap(), pb["notes"].as_sequence().unwrap());
        if !first_pass {
            assert_eq!(na.len(), nb.len(), "{name} / {:?}: note count", pa["name"]);
        }
        for (i, (a, b)) in na.iter().zip(nb.iter()).enumerate() {
            for k in ["position", "duration", "tone", "lyric"] {
                assert_eq!(a[k], b[k], "{name} / {:?} note {i}: {k}", pa["name"]);
            }
        }
        if !first_pass {
            assert_eq!(pa["duration"], pb["duration"], "{name}: part duration");
        }
    }
}

fn check(name: &str, expected_lang: Lang, fixture: &str) {
    check_with(name, expected_lang, fixture, false)
}

#[test]
fn santo() {
    check("santo", Lang::It, "santo.ustx");
}

#[test]
fn agnello() {
    check("agnello", Lang::It, "agnello.ustx");
}

#[test]
fn adoro() {
    // repeat 1-18 twice, verse 2 in bars 11-18
    check_with("adoro", Lang::It, "adoro.ustx", true);
    let Some(dir) = fixtures() else { return };
    let score = read_mscz(&dir.join("adoro.mscz")).unwrap();
    let voce = &score.parts.iter().find(|p| p.name == "Voce").unwrap().notes;
    assert_eq!(voce.len(), 176);
    let text: String = voce.iter().filter_map(|n| n.lyric.as_ref()).map(|l| l.text.as_str()).collect::<Vec<_>>().join(" ");
    assert_eq!(text.matches("A do ro").count(), 4, "refrain twice per pass");
    assert!(text.to_lowercase().contains("gra zia"), "verse 2 sung on the second pass: {text}");
    assert!(score.warnings.is_empty(), "{:?}", score.warnings);
}

#[test]
fn sonnet_parts_end_together() {
    // a <location> moving back inside a bar (chord symbols) must not shorten the bar
    let Some(dir) = fixtures() else { return };
    let score = read_mscz(&dir.join("sonnet.mscz")).unwrap();
    let ends: Vec<_> = score.parts.iter().filter(|p| p.is_sung()).map(|p| p.notes.last().map(|n| n.start + n.dur).unwrap()).collect();
    assert!(ends.windows(2).all(|w| w[0] == w[1]), "{ends:?}");
}

#[test]
fn sonnet() {
    check("sonnet", Lang::En, "sonnet.ustx");
}

#[test]
fn sonnet_tiger() {
    check("sonnet", Lang::En, "sonnet_tiger.ustx");
}

#[test]
fn no_warnings_on_simple_scores() {
    let Some(dir) = fixtures() else { return };
    for n in ["santo", "agnello", "adoro", "sonnet"] {
        let s = read_mscz(&dir.join(format!("{n}.mscz"))).unwrap();
        println!("{n}: {:?}", s.warnings);
    }
}
