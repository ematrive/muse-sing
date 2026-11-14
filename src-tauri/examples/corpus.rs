//! Prints (and optionally writes) the recording corpus for a voice.
//! cargo run --example corpus -- soprano 60 79 [out.json] [score.mscz:Part ...]
use std::path::Path;

use muse_sing_lib::corpus::{base_corpus, coverage, piece_phrases, VoiceProfile};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let p = VoiceProfile {
        name: "Prova".into(),
        voice: a.first().cloned().unwrap_or_else(|| "soprano".into()),
        low: a.get(1).and_then(|x| x.parse().ok()).unwrap_or(60),
        high: a.get(2).and_then(|x| x.parse().ok()).unwrap_or(79),
    };
    let mut c = base_corpus(&p);
    for spec in a.iter().skip(4) {
        let (file, part) = spec.rsplit_once(':').expect("score.mscz:Part");
        let (title, ph) = piece_phrases(Path::new(file), part, &p).unwrap();
        let secs: f64 = ph.iter().map(|x| x.seconds).sum();
        println!("{title} / {part}: {} frasi, {:.1} min", ph.len(), secs / 60.0);
        for x in ph.iter().take(3) {
            println!("   {} [{:.1}s] {}", x.id, x.seconds, x.text);
        }
        c.extend(ph);
    }
    let secs: f64 = c.iter().map(|x| x.seconds).sum();
    println!("{} frasi, {:.1} min di canto", c.len(), secs / 60.0);
    let cov = coverage(&c, &p);
    println!("fonemi: {:?}", cov.phonemes);
    println!("note mai cantate: {:?}", cov.missing_pitches);
    if let Some(out) = a.get(3).filter(|x| !x.is_empty() && *x != "-") {
        std::fs::write(out, serde_json::to_string_pretty(&c).unwrap()).unwrap();
    }
}
