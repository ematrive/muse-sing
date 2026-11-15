//! Diagnostics: what the Italian G2p model predicts for some words, on this machine.
//!
//!   cargo run --release --example g2p_check -- bimbo salutano scaldano
use muse_sing_lib::ds::g2p;

fn main() {
    let words: Vec<String> = std::env::args().skip(1).collect();
    let words = if words.is_empty() { ["bimbo", "salutano", "vegliano", "scaldano", "mensi", "cristoil", "gliangeli"].map(String::from).to_vec() } else { words };
    let p = g2p::italian().expect("g2p");
    let table = p.phoneme_table();
    for w in words {
        match p.predict_indices(&w) {
            Ok(ix) => {
                let syms: Vec<String> = ix.iter().map(|&i| table.get(i as usize).map(|s| if s.is_empty() { format!("<{i}>") } else { s.to_string() }).unwrap_or(format!("?{i}"))).collect();
                println!("{w}: {} | {ix:?}", syms.join(" "));
            }
            Err(e) => println!("{w}: ERRORE {e}"),
        }
    }
}
