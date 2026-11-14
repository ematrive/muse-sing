//! A singer's recording folder:
//!
//! ```text
//! <cartella>/profile.json   name, voice type, range
//! <cartella>/corpus.json    phrases to sing (exercises, vocalises, pieces)
//! <cartella>/takes.json     recorded phrases: file, length, peak
//! <cartella>/wavs/<id>.wav  24-bit mono, starts 0.6 s before the first note
//! ```
//!
//! The same folder is the input of the labelling step (`tools/dataset/`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::audio::{self, Recorded};
use crate::corpus::{self, base_corpus, Phrase, VoiceProfile};
use crate::error::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Take {
    pub file: String,
    pub seconds: f64,
    pub peak: f32,
    pub clipped: bool,
    pub quiet: bool,
    pub sample_rate: u32,
    /// Unix seconds.
    pub recorded_at: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct VoiceState {
    pub dir: String,
    pub profile: VoiceProfile,
    pub phrases: Vec<Phrase>,
    pub takes: BTreeMap<String, Take>,
}

fn read_json<T: for<'a> Deserialize<'a>>(p: &Path) -> Result<T> {
    let t = std::fs::read_to_string(p).map_err(|e| Error::Other(format!("{}: {e}", p.display())))?;
    serde_json::from_str(&t).map_err(|e| Error::Format(format!("{}: {e}", p.display())))
}

fn write_json<T: Serialize>(p: &Path, v: &T) -> Result<()> {
    let tmp = p.with_extension("json.part");
    std::fs::write(&tmp, serde_json::to_string_pretty(v).map_err(|e| Error::Other(e.to_string()))?)?;
    std::fs::rename(tmp, p)?;
    Ok(())
}

pub fn slug(s: &str) -> String {
    let s: String = s
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'à' | 'á' => 'a',
            'è' | 'é' => 'e',
            'ì' | 'í' => 'i',
            'ò' | 'ó' => 'o',
            'ù' | 'ú' => 'u',
            c if c.is_ascii_alphanumeric() => c,
            _ => '-',
        })
        .collect();
    s.split('-').filter(|x| !x.is_empty()).collect::<Vec<_>>().join("-")
}

/// Creates `<parent>/<slug(name)>` with the base corpus.
pub fn create(parent: &Path, profile: VoiceProfile) -> Result<VoiceState> {
    if profile.name.trim().is_empty() {
        return Err(Error::Other("scrivi il nome del cantante".into()));
    }
    if profile.high - profile.low < 12 {
        return Err(Error::Other("l'estensione deve essere almeno di un'ottava".into()));
    }
    let dir = parent.join(slug(&profile.name));
    if dir.join("profile.json").exists() {
        return Err(Error::Other(format!("esiste già un cantante in {}: aprilo invece di crearlo", dir.display())));
    }
    std::fs::create_dir_all(dir.join("wavs"))?;
    write_json(&dir.join("profile.json"), &profile)?;
    write_json(&dir.join("corpus.json"), &base_corpus(&profile))?;
    write_json(&dir.join("takes.json"), &BTreeMap::<String, Take>::new())?;
    open(&dir)
}

pub fn open(dir: &Path) -> Result<VoiceState> {
    if !dir.join("profile.json").exists() {
        return Err(Error::Other(format!("{} non è la cartella di un cantante (manca profile.json)", dir.display())));
    }
    let profile: VoiceProfile = read_json(&dir.join("profile.json"))?;
    let phrases: Vec<Phrase> = read_json(&dir.join("corpus.json"))?;
    let mut takes: BTreeMap<String, Take> =
        if dir.join("takes.json").exists() { read_json(&dir.join("takes.json"))? } else { BTreeMap::new() };
    // a take whose file was deleted by hand is no longer a take
    takes.retain(|_, t| dir.join(&t.file).exists());
    Ok(VoiceState { dir: dir.to_string_lossy().to_string(), profile, phrases, takes })
}

pub fn phrase(dir: &Path, id: &str) -> Result<Phrase> {
    let phrases: Vec<Phrase> = read_json(&dir.join("corpus.json"))?;
    phrases.into_iter().find(|p| p.id == id).ok_or_else(|| Error::Other(format!("frase {id} non trovata")))
}

/// Adds (or replaces) the phrases of one part of a score, placed before the low/high register exercises.
pub fn add_piece(dir: &Path, mscz: &Path, part: &str) -> Result<VoiceState> {
    let st = open(dir)?;
    let (_, new) = corpus::piece_phrases(mscz, part, &st.profile)?;
    if new.is_empty() {
        return Err(Error::Other("nessuna frase con testo in questa voce".into()));
    }
    let group = new[0].group.clone();
    let mut phrases: Vec<Phrase> = st.phrases.into_iter().filter(|p| p.group != group).collect();
    let at = phrases
        .iter()
        .position(|p| p.group == corpus::GROUP_LOW || p.group == corpus::GROUP_HIGH)
        .unwrap_or(phrases.len());
    phrases.splice(at..at, new);
    write_json(&dir.join("corpus.json"), &phrases)?;
    open(dir)
}

/// Removes a piece's phrases (recordings are kept on disk but no longer listed).
pub fn remove_group(dir: &Path, group: &str) -> Result<VoiceState> {
    if !group.starts_with("Brano") {
        return Err(Error::Other("si possono togliere solo i brani".into()));
    }
    let st = open(dir)?;
    let phrases: Vec<Phrase> = st.phrases.into_iter().filter(|p| p.group != group).collect();
    write_json(&dir.join("corpus.json"), &phrases)?;
    open(dir)
}

/// Writes the WAV of a take and records it in takes.json (a new take replaces the old one).
pub fn save_take(dir: &Path, id: &str, rec: &Recorded) -> Result<Take> {
    std::fs::create_dir_all(dir.join("wavs"))?;
    let file = format!("wavs/{id}.wav");
    audio::write_wav(&dir.join(&file), &rec.samples, rec.sample_rate)?;
    let peak = rec.samples.iter().fold(0f32, |m, x| m.max(x.abs()));
    let take = Take {
        file,
        seconds: (rec.samples.len() as f64 / rec.sample_rate as f64 * 10.0).round() / 10.0,
        peak,
        clipped: peak >= 0.99,
        quiet: peak < 0.05,
        sample_rate: rec.sample_rate,
        recorded_at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
    };
    let path = dir.join("takes.json");
    let mut takes: BTreeMap<String, Take> = if path.exists() { read_json(&path)? } else { BTreeMap::new() };
    takes.insert(id.to_string(), take.clone());
    write_json(&path, &takes)?;
    Ok(take)
}

pub fn take_path(dir: &Path, id: &str) -> Result<PathBuf> {
    let st = open(dir)?;
    let t = st.takes.get(id).ok_or_else(|| Error::Other("questa frase non è ancora registrata".into()))?;
    Ok(dir.join(&t.file))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_add_piece_save_take() {
        let parent = std::env::temp_dir().join(format!("muse-sing-voice-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&parent);
        let prof = VoiceProfile { name: "Maria Rossi".into(), voice: "soprano".into(), low: 60, high: 79 };
        let st = create(&parent, prof.clone()).unwrap();
        assert!(st.dir.ends_with("maria-rossi"));
        assert!(create(&parent, prof).is_err(), "no overwrite");
        let dir = PathBuf::from(&st.dir);
        let n0 = st.phrases.len();

        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        // the choir scores are local only (not in the public repository)
        if !fixtures.join("santo.mscz").exists() {
            eprintln!("tests/fixtures mancante: test saltato");
            std::fs::remove_dir_all(parent).ok();
            return;
        }
        let st = add_piece(&dir, &fixtures.join("santo.mscz"), "Soprano").unwrap();
        let added = st.phrases.len() - n0;
        assert!(added >= 3);
        // pieces come before the low register
        let first_piece = st.phrases.iter().position(|p| p.group.starts_with("Brano")).unwrap();
        let first_low = st.phrases.iter().position(|p| p.group == corpus::GROUP_LOW).unwrap();
        assert!(first_piece < first_low);
        // adding again replaces
        let st = add_piece(&dir, &fixtures.join("santo.mscz"), "Soprano").unwrap();
        assert_eq!(st.phrases.len() - n0, added);

        let id = st.phrases[0].id.clone();
        let rec = Recorded { samples: vec![0.2; 44_100], sample_rate: 44_100 };
        let t = save_take(&dir, &id, &rec).unwrap();
        assert_eq!(t.seconds, 1.0);
        assert!(!t.clipped && !t.quiet);
        assert!(open(&dir).unwrap().takes.contains_key(&id));
        std::fs::remove_file(take_path(&dir, &id).unwrap()).unwrap();
        assert!(open(&dir).unwrap().takes.is_empty(), "deleted file = no take");

        let group = st.phrases[first_piece].group.clone();
        assert_eq!(remove_group(&dir, &group).unwrap().phrases.len(), n0);
        std::fs::remove_dir_all(parent).ok();
    }
}
