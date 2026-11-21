//! cargo run --example convert -- score.mscz out.ustx SINGER_ID [llm_hints.json]
use std::collections::HashMap;
use std::path::Path;

use muse_sing_lib::{lyrics, score, ustx};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let src = Path::new(&a[1]);
    let s = score::read_score(src).unwrap();
    let lang = lyrics::detect_lang(&s.parts);
    let mut hints = HashMap::new();
    if let Some(h) = a.get(4) {
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(h).unwrap()).unwrap();
        for x in v.as_array().unwrap() {
            if x["song"].as_str() == s.title.as_deref() {
                hints.insert(x["word"].as_str().unwrap().to_string(), x["llm"].as_str().unwrap().to_string());
            }
        }
    }
    let stem = src.file_stem().unwrap().to_string_lossy().to_string();
    let p = ustx::build(&s, &ustx::Options { lang, singer: &a[3], part_singers: None, parts: None, hints: &hints, fallback_name: &stem });
    std::fs::write(&a[2], ustx::to_yaml(&p)).unwrap();
    let notes: usize = p.voice_parts.iter().map(|v| v.notes.len()).sum();
    println!("{}: {:?}, {} tracce, {} note, avvisi {:?}", a[2], lang, p.tracks.len(), notes, s.warnings);
}
