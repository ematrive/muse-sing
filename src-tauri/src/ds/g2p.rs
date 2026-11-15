//! Grapheme-to-phoneme, ported from OpenUtau.Core/Api (G2pDictionary, G2pPack, G2pRemapper,
//! G2pFallbacks) and OpenUtau.Core/G2p (ItalianG2p, ArpabetPlusG2p). The OpenUtau packs
//! (`dict.txt` + `phones.txt` + `g2p.onnx`) are bundled under `resources/g2p` (MIT, see NOTICE).

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::sync::{Arc, Mutex, OnceLock};

use serde::Deserialize;

use super::onnx::{Input, Model};
use crate::error::{Error, Result};

pub trait G2p: Send + Sync {
    fn is_valid_symbol(&self, s: &str) -> bool;
    fn is_vowel(&self, s: &str) -> bool;
    fn is_glide(&self, s: &str) -> bool;
    fn query(&self, grapheme: &str) -> Option<Vec<String>>;
}

/// Dictionary G2p: symbol types plus word entries (G2pDictionary).
#[derive(Default)]
pub struct Dict {
    entries: HashMap<String, Vec<String>>,
    symbols: HashMap<String, bool>,
    glides: HashSet<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct DictData {
    pub symbols: Option<Vec<SymbolData>>,
    pub entries: Option<Vec<EntryData>>,
    pub replacements: Option<Vec<Replacement>>,
}

#[derive(Deserialize)]
pub struct SymbolData {
    pub symbol: String,
    #[serde(rename = "type", default)]
    pub kind: String,
}

#[derive(Deserialize)]
pub struct EntryData {
    pub grapheme: String,
    #[serde(default)]
    pub phonemes: Vec<String>,
}

#[derive(Deserialize)]
pub struct Replacement {
    pub from: String,
    pub to: String,
}

impl Dict {
    pub fn add_symbol(&mut self, symbol: &str, kind: &str) {
        self.symbols.insert(symbol.to_string(), kind == "vowel");
        if kind == "semivowel" || kind == "liquid" {
            self.glides.insert(symbol.to_string());
        } else {
            self.glides.remove(symbol);
        }
    }

    pub fn add_symbol_vowel(&mut self, symbol: &str, vowel: bool) {
        self.symbols.insert(symbol.to_string(), vowel);
    }

    /// Symbols must be added first: unknown symbols of the entry are dropped (BuildTrie).
    pub fn add_entry(&mut self, grapheme: &str, phonemes: impl IntoIterator<Item = String>) {
        let ph = phonemes.into_iter().filter(|p| self.symbols.contains_key(p)).collect();
        self.entries.insert(grapheme.to_string(), ph);
    }

    pub fn load(&mut self, data: &DictData) {
        for s in data.symbols.iter().flatten() {
            self.add_symbol(&s.symbol, &s.kind);
        }
        for e in data.entries.iter().flatten() {
            self.add_entry(&e.grapheme, e.phonemes.iter().cloned());
        }
    }

    pub fn symbols(&self) -> impl Iterator<Item = &String> {
        self.symbols.keys()
    }
}

impl G2p for Dict {
    fn is_valid_symbol(&self, s: &str) -> bool {
        self.symbols.contains_key(s)
    }
    fn is_vowel(&self, s: &str) -> bool {
        self.symbols.get(s).copied().unwrap_or(false)
    }
    fn is_glide(&self, s: &str) -> bool {
        self.glides.contains(s)
    }
    fn query(&self, grapheme: &str) -> Option<Vec<String>> {
        self.entries.get(grapheme).cloned()
    }
}

/// OpenUtau's built-in G2p: a dictionary plus a seq2seq ONNX model for unknown words (G2pPack).
pub struct Pack {
    dict: Dict,
    model: Model,
    grapheme_indexes: HashMap<char, i32>,
    phonemes: &'static [&'static str],
    cache: Mutex<HashMap<String, Vec<String>>>,
    /// Rule-based transcription for words outside the dictionary, used when the model gives
    /// nothing usable (not in OpenUtau).
    rules: Option<fn(&str) -> Vec<String>>,
    /// The model passed its self-test at load time (on some machines it only outputs zeros).
    model_ok: bool,
}

fn zip_entry(zip: &[u8], name: &str) -> Result<Vec<u8>> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).map_err(|e| Error::Format(format!("g2p pack: {e}")))?;
    let mut f = archive.by_name(name).map_err(|e| Error::Format(format!("g2p pack {name}: {e}")))?;
    let mut out = vec![];
    f.read_to_end(&mut out)?;
    Ok(out)
}

fn remove_tail_digits(s: &str) -> String {
    s.trim_end_matches(|c: char| c.is_ascii_digit()).to_string()
}

impl Pack {
    fn load(
        zip: &[u8],
        graphemes: &[&str],
        phonemes: &'static [&'static str],
        lowercase: bool,
        rules: Option<fn(&str) -> Vec<String>>,
        probe: &str,
    ) -> Result<Pack> {
        let prep_g = |s: &str| if lowercase { s.to_lowercase() } else { s.to_string() };
        let prep_p = |s: &str| remove_tail_digits(&if lowercase { s.to_lowercase() } else { s.to_string() });
        let dict_txt = String::from_utf8_lossy(&zip_entry(zip, "dict.txt")?).into_owned();
        let phones_txt = String::from_utf8_lossy(&zip_entry(zip, "phones.txt")?).into_owned();
        let model_bytes = zip_entry(zip, "g2p.onnx")?;
        let mut dict = Dict::default();
        for line in phones_txt.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() == 2 {
                dict.add_symbol(&prep_p(parts[0]), parts[1]);
            }
        }
        for line in dict_txt.lines().filter(|l| !l.starts_with(";;;")) {
            let parts: Vec<&str> = line.trim().split("  ").collect();
            if parts.len() == 2 {
                dict.add_entry(&prep_g(parts[0]), parts[1].split_whitespace().map(prep_p));
            }
        }
        let model = Model::load_bytes_with("g2p.onnx", &model_bytes, false)?;
        let grapheme_indexes = graphemes
            .iter()
            .enumerate()
            .skip(4)
            .filter_map(|(i, g)| g.chars().next().map(|c| (c, i as i32)))
            .collect();
        let mut pack = Pack { dict, model, grapheme_indexes, phonemes, cache: Mutex::new(HashMap::new()), rules, model_ok: true };
        // a word the model must know: if it doesn't come out right, don't trust the model
        let got = pack.predict(probe).unwrap_or_default();
        pack.model_ok = !got.is_empty() && got.iter().all(|s| pack.dict.is_valid_symbol(s));
        Ok(pack)
    }

    pub fn model_ok(&self) -> bool {
        self.model_ok
    }

    fn predict(&self, grapheme: &str) -> Result<Vec<String>> {
        // indices 0-3 are the model's special tokens (no phoneme): skipped, unlike OpenUtau
        Ok(self.predict_indices(grapheme)?.iter().filter_map(|&i| self.phonemes.get(i as usize)).filter(|s| !s.is_empty()).map(|s| s.to_string()).collect())
    }

    /// Raw output of the G2p model: phoneme indices (for diagnostics).
    pub fn predict_indices(&self, grapheme: &str) -> Result<Vec<i32>> {
        let src: Vec<i32> = grapheme.to_lowercase().chars().filter_map(|c| self.grapheme_indexes.get(&c).copied()).collect();
        if src.is_empty() {
            return Ok(vec![]);
        }
        let mut tgt: Vec<i32> = vec![2];
        let mut t: i32 = 0;
        while (t as usize) < src.len() && tgt.len() < 48 {
            let mut out = self.model.run(vec![
                ("src", Input::I32(src.clone())),
                ("tgt", Input::I32(tgt.clone())),
                ("t", Input::I32(vec![t])),
            ])?;
            let v = out.first()?;
            let (_, data) = v.try_extract_tensor::<i32>().map_err(|e| Error::Other(format!("g2p: {e}")))?;
            let pred = data.first().copied().unwrap_or(2);
            if pred != 2 {
                tgt.push(pred);
            } else {
                t += 1;
            }
        }
        Ok(tgt[1..].to_vec())
    }

    /// The model's phoneme table (index → symbol; 0-3 are special tokens).
    pub fn phoneme_table(&self) -> &[&str] {
        self.phonemes
    }
}

fn is_all_punct(s: &str) -> bool {
    let mut it = s.chars();
    matches!((it.next(), it.next()), (Some(c), None) if c.is_ascii_punctuation() || "«»“”‘’…".contains(c))
}

impl G2p for Pack {
    fn is_valid_symbol(&self, s: &str) -> bool {
        self.dict.is_valid_symbol(s)
    }
    fn is_vowel(&self, s: &str) -> bool {
        self.dict.is_vowel(s)
    }
    fn is_glide(&self, s: &str) -> bool {
        self.dict.is_glide(s)
    }
    fn query(&self, grapheme: &str) -> Option<Vec<String>> {
        if grapheme.is_empty() || is_all_punct(grapheme) {
            return None;
        }
        if let Some(p) = self.dict.query(grapheme) {
            return Some(p);
        }
        if let Some(p) = self.cache.lock().ok()?.get(grapheme) {
            return Some(p.clone());
        }
        let predicted = if self.model_ok { self.predict(grapheme).unwrap_or_default() } else { vec![] };
        let usable = !predicted.is_empty() && predicted.iter().all(|s| self.dict.is_valid_symbol(s));
        let p = if usable { predicted } else { self.rules.map(|f| f(grapheme)).unwrap_or_default() };
        if p.is_empty() {
            return None;
        }
        self.cache.lock().ok()?.insert(grapheme.to_string(), p.clone());
        Some(p)
    }
}

/// Maps the base G2p's phonemes to the voicebank's (G2pRemapper).
pub struct Remapper {
    pub mapped: Arc<dyn G2p>,
    pub symbols: HashMap<String, bool>,
    pub replacements: HashMap<String, String>,
}

impl G2p for Remapper {
    fn is_valid_symbol(&self, s: &str) -> bool {
        self.symbols.contains_key(s)
    }
    fn is_vowel(&self, s: &str) -> bool {
        self.symbols.get(s).copied().unwrap_or(false)
    }
    fn is_glide(&self, _s: &str) -> bool {
        false
    }
    fn query(&self, grapheme: &str) -> Option<Vec<String>> {
        let p = self.mapped.query(grapheme)?;
        Some(p.into_iter().map(|ph| self.replacements.get(&ph).cloned().unwrap_or(ph)).collect())
    }
}

/// First G2p that knows the symbol / word wins (G2pFallbacks).
pub struct Fallbacks(pub Vec<Arc<dyn G2p>>);

impl G2p for Fallbacks {
    fn is_valid_symbol(&self, s: &str) -> bool {
        self.0.iter().any(|g| g.is_valid_symbol(s))
    }
    fn is_vowel(&self, s: &str) -> bool {
        self.0.iter().find(|g| g.is_valid_symbol(s)).is_some_and(|g| g.is_vowel(s))
    }
    fn is_glide(&self, s: &str) -> bool {
        self.0.iter().find(|g| g.is_valid_symbol(s)).is_some_and(|g| g.is_glide(s))
    }
    fn query(&self, grapheme: &str) -> Option<Vec<String>> {
        self.0.iter().find_map(|g| g.query(grapheme))
    }
}

// ---- built-in packs ----

static IT_ZIP: &[u8] = include_bytes!("../../resources/g2p/g2p-it.zip");
static ARPA_PLUS_ZIP: &[u8] = include_bytes!("../../resources/g2p/g2p-arpabet-plus.zip");

const IT_GRAPHEMES: &[&str] = &[
    "", "", "", "", "'", "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s",
    "t", "u", "v", "w", "x", "y", "z", "à", "è", "é", "ì", "í", "ò", "ù", "ú",
];
const IT_PHONEMES: &[&str] = &[
    "", "", "", "", "a", "b", "d", "dz", "dZZ", "e", "EE", "f", "g", "i", "j", "JJ", "k", "l", "LL", "m", "n", "nf", "ng",
    "o", "OO", "p", "r", "s", "SS", "t", "ts", "tSS", "u", "v", "w", "z",
];
const ARPA_PLUS_GRAPHEMES: &[&str] = &[
    "", "", "", "", "'", "-", "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r",
    "s", "t", "u", "v", "w", "x", "y", "z", "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O",
    "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z",
];
const ARPA_PLUS_PHONEMES: &[&str] = &[
    "", "", "", "", "aa", "ae", "ah", "ao", "aw", "ax", "ay", "b", "ch", "d", "dh", "dr", "dx", "eh", "er", "ey", "f",
    "g", "hh", "ih", "iy", "jh", "k", "l", "m", "n", "ng", "ow", "oy", "p", "q", "r", "s", "sh", "t", "th", "tr", "uh",
    "uw", "v", "w", "y", "z", "zh",
];

fn cached(cell: &'static OnceLock<std::result::Result<Arc<Pack>, String>>, f: impl FnOnce() -> Result<Pack>) -> Result<Arc<Pack>> {
    cell.get_or_init(|| f().map(Arc::new).map_err(|e| e.to_string())).clone().map_err(Error::Other)
}

pub fn italian() -> Result<Arc<Pack>> {
    static P: OnceLock<std::result::Result<Arc<Pack>, String>> = OnceLock::new();
    cached(&P, || Pack::load(IT_ZIP, IT_GRAPHEMES, IT_PHONEMES, false, Some(italian_rules), "salutano"))
}

/// Italian spelling is regular enough for rules (lyrics.rs) to stand in for the model.
fn italian_rules(word: &str) -> Vec<String> {
    crate::lyrics::it_g2p_word(&[word.to_string()]).into_iter().map(str::to_string).collect()
}

/// The Italian model file, for diagnostics.
pub fn italian_model_bytes() -> Result<Vec<u8>> {
    zip_entry(IT_ZIP, "g2p.onnx")
}

pub fn arpabet_plus() -> Result<Arc<Pack>> {
    static P: OnceLock<std::result::Result<Arc<Pack>, String>> = OnceLock::new();
    cached(&P, || Pack::load(ARPA_PLUS_ZIP, ARPA_PLUS_GRAPHEMES, ARPA_PLUS_PHONEMES, true, None, "singing"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn italian_pack_dictionary_and_model() {
        let p = italian().unwrap();
        assert_eq!(p.query("medico").unwrap(), ["m", "EE", "d", "i", "k", "o"]);
        assert!(p.is_vowel("EE") && !p.is_vowel("k"));
        // not in the dictionary: predicted by the ONNX model
        assert!(p.model_ok());
        let w = p.query("prostrerò").unwrap();
        assert!(w.starts_with(&["p".to_string(), "r".to_string()]), "{w:?}");
    }

    #[test]
    fn italian_rules_stand_in_for_the_model() {
        let r = |w: &str| italian_rules(w).join(" ");
        assert_eq!(r("salutano"), "s a l u t a n o");
        assert_eq!(r("bimbo"), "b i m b o");
        assert_eq!(r("vegliano"), "v e LL a n o");
        assert_eq!(r("gliangeli"), "LL a n dZZ e l i");
        assert_eq!(r("scaldano"), "s k a l d a n o");
        assert_eq!(r("pastorie"), "p a s t o r j e");
        // a pack whose model failed its self-test answers with the rules
        let mut p = Pack::load(IT_ZIP, IT_GRAPHEMES, IT_PHONEMES, false, Some(italian_rules), "salutano").unwrap();
        p.model_ok = false;
        assert_eq!(p.query("mensi").unwrap().join(" "), "m e n s i");
        assert_eq!(p.query("medico").unwrap()[1], "EE", "the dictionary still comes first");
    }

    #[test]
    fn arpabet_plus_pack() {
        let p = arpabet_plus().unwrap();
        assert_eq!(p.query("chandelier").unwrap(), ["sh", "ae", "n", "d", "ax", "l", "ih", "r"]);
        assert!(p.query("zorbleflux").is_some_and(|w| !w.is_empty()));
    }
}
