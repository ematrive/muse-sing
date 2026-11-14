//! Records the first corpus phrase with the default devices and writes it to the given path.
//! Used to check the take pipeline (devices, count-in, timing, alignment) without the UI.
use std::sync::Arc;

use muse_sing_lib::audio::{count_in_secs, phrase_secs, write_wav, Engine, Event, LEAD_IN, TAIL};
use muse_sing_lib::corpus::{base_corpus, VoiceProfile};

fn main() {
    let out = std::env::args().nth(1).expect("out.wav");
    let ph = base_corpus(&VoiceProfile { name: "x".into(), voice: "soprano".into(), low: 60, high: 79 }).remove(0);
    let engine = Engine::new(Arc::new(|ev| {
        if let Event::Device(msg) = ev {
            eprintln!("dispositivo: {msg}");
        }
    }));
    let st = engine.open(None, None).unwrap();
    println!(
        "ingresso: {:?} ({} Hz){}, uscita: {} ({} Hz)",
        st.input.as_ref().map(|i| &i.name),
        st.input_rate,
        if st.input_switched { " [scelto al posto del microfono Bluetooth]" } else { "" },
        st.output.name,
        st.output_rate
    );
    let job = engine.begin_job(true);
    let ticks = std::cell::Cell::new(0);
    let first_countin = std::cell::Cell::new(None);
    let rec = engine
        .record_take(job, &ph, true, true, &|p| {
            ticks.set(ticks.get() + 1);
            if p.phase == "countin" && first_countin.get().is_none() {
                first_countin.set(Some(p.t));
            }
        })
        .unwrap()
        .unwrap();
    let secs = rec.samples.len() as f64 / rec.sample_rate as f64;
    println!(
        "count-in {:.2}s (primo tick a {:.3}s), frase {:.2}s, file {secs:.2}s (atteso {:.2}s), {} Hz, {} tick",
        count_in_secs(&ph),
        first_countin.get().unwrap_or(-1.0),
        phrase_secs(&ph),
        LEAD_IN + phrase_secs(&ph) + TAIL,
        rec.sample_rate,
        ticks.get()
    );
    write_wav(std::path::Path::new(&out), &rec.samples, rec.sample_rate).unwrap();
}
