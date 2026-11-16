//! DiffSinger engine: a Rust port of OpenUtau's DiffSinger renderer (OpenUtau.Core/DiffSinger,
//! MIT, https://github.com/openutau/OpenUtau). See NOTICE.

/// Progress lines on stderr (visible in the terminal of `pnpm tauri dev`), with the time.
#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {{
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
        let s = (t % 86400.0) as u64;
        eprintln!("[muse-sing {:02}:{:02}:{:02}] {}", s / 3600, s / 60 % 60, s % 60, format!($($arg)*));
    }};
}

pub mod config;
pub mod g2p;
pub mod onnx;
pub mod phonemize;
pub mod render;
pub mod time;
pub mod voicebank;

use crate::ustx::{Project, TICKS};
use phonemize::PNote;
use time::TimeAxis;

/// Notes of each voice part of a Muse Sing project, as the phonemizer takes them.
pub fn project_notes(p: &Project) -> Vec<(String, Vec<PNote>)> {
    p.voice_parts
        .iter()
        .map(|vp| {
            let notes = vp
                .notes
                .iter()
                .map(|n| PNote { id: n.id.clone(), position: vp.position + n.position, duration: n.duration, tone: n.tone, lyric: n.lyric.clone() })
                .collect();
            (vp.name.clone(), notes)
        })
        .collect()
}

pub fn project_axis(p: &Project) -> TimeAxis {
    let tempos: Vec<(i64, f64)> = p.tempos.iter().map(|t| (t.position, t.bpm)).collect();
    TimeAxis::new(&tempos, TICKS)
}
