//! Diagnostics: what the Italian G2p model predicts on this machine, with ONNX Runtime's default
//! settings and with graph optimisations / threads changed.
//!
//!   cargo run --release --example g2p_check -- [words…]
use muse_sing_lib::ds::g2p::{self, G2p};
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;

fn predict(s: &mut Session, src: &[i32]) -> Result<Vec<i32>, String> {
    let mut tgt = vec![2i32];
    let mut t = 0i32;
    while (t as usize) < src.len() && tgt.len() < 48 {
        let inputs = ort::inputs![
            "src" => Tensor::from_array((vec![1i64, src.len() as i64], src.to_vec())).map_err(|e| e.to_string())?,
            "tgt" => Tensor::from_array((vec![1i64, tgt.len() as i64], tgt.clone())).map_err(|e| e.to_string())?,
            "t" => Tensor::from_array((vec![1i64], vec![t])).map_err(|e| e.to_string())?,
        ];
        let out = s.run(inputs).map_err(|e| e.to_string())?;
        let (_, d) = out[0].try_extract_tensor::<i32>().map_err(|e| e.to_string())?;
        let p = d[0];
        if p != 2 { tgt.push(p) } else { t += 1 }
    }
    Ok(tgt[1..].to_vec())
}

fn main() {
    let words: Vec<String> = std::env::args().skip(1).collect();
    let words = if words.is_empty() { ["bimbo", "salutano", "mensi"].map(String::from).to_vec() } else { words };
    println!("{} {}", std::env::consts::OS, std::env::consts::ARCH);
    let p = g2p::italian().expect("g2p");
    println!("modello G2P affidabile in Muse Sing: {}", p.model_ok());
    for w in &words {
        println!("  app  {w}: {:?}", p.query(w));
    }
    // graphemes as in g2p.rs: "'" = 4, a = 5, … (IT_GRAPHEMES)
    let graphemes = "'abcdefghijklmnopqrstuvwxyzàèéìíòùú";
    let enc = |w: &str| -> Vec<i32> { w.chars().filter_map(|c| graphemes.chars().position(|g| g == c)).map(|i| i as i32 + 4).collect() };
    let bytes = g2p::italian_model_bytes().expect("model");
    let env = muse_sing_lib::ds::onnx::env().expect("ort");
    let table = p.phoneme_table();
    let configs: [(&str, GraphOptimizationLevel, usize); 5] = [
        ("predefinito (Level3)", GraphOptimizationLevel::Level3, 0),
        ("Level2", GraphOptimizationLevel::Level2, 0),
        ("Level1", GraphOptimizationLevel::Level1, 0),
        ("senza ottimizzazioni", GraphOptimizationLevel::Disable, 0),
        ("senza ottimizzazioni, 1 thread", GraphOptimizationLevel::Disable, 1),
    ];
    for (name, level, threads) in configs {
        let mut b = Session::builder(env).expect("builder").with_optimization_level(level).expect("level");
        if threads > 0 {
            b = b.with_intra_threads(threads).expect("threads");
        }
        let mut s = b.commit_from_memory(&bytes).expect("session");
        for w in &words {
            match predict(&mut s, &enc(w)) {
                Ok(ix) => {
                    let syms: Vec<&str> = ix.iter().map(|&i| table.get(i as usize).copied().filter(|s| !s.is_empty()).unwrap_or("·")).collect();
                    println!("  {name}  {w}: {} {ix:?}", syms.join(" "));
                }
                Err(e) => println!("  {name}  {w}: ERRORE {e}"),
            }
        }
    }
}
