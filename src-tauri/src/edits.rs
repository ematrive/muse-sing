//! Edits of a sung part: pronunciation (per word, per note, phoneme timing) and curves
//! (intonation, dynamics, timbre, consonant speed). Stored in the project; the curves are in
//! score ticks (480 per quarter), so they stay on their notes at any practice tempo.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Curves are stored one value every this many ticks (as OpenUtau's curves).
pub const CURVE_STEP: i64 = 5;

/// A curve as its non-default points: sorted (tick multiple of `CURVE_STEP`, value). Between
/// two points it is linear; next to a missing point it goes back to the default.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Curve(pub Vec<(i64, f32)>);

impl Curve {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn get(&self, tick: i64, default: f32) -> f32 {
        match self.0.binary_search_by_key(&tick, |p| p.0) {
            Ok(i) => self.0[i].1,
            Err(_) => default,
        }
    }

    /// Value at a tick (fractional), `default` where nothing was drawn.
    pub fn at(&self, tick: f64, default: f32) -> f32 {
        if self.0.is_empty() {
            return default;
        }
        let a = (tick / CURVE_STEP as f64).floor() as i64 * CURVE_STEP;
        let f = ((tick - a as f64) / CURVE_STEP as f64) as f32;
        let va = self.get(a, default);
        let vb = self.get(a + CURVE_STEP, default);
        va + (vb - va) * f
    }

    /// Keeps the points sorted, on the grid, without duplicates or default values.
    pub fn normalized(mut self, default: f32) -> Curve {
        for p in &mut self.0 {
            p.0 = (p.0 as f64 / CURVE_STEP as f64).round() as i64 * CURVE_STEP;
        }
        self.0.sort_by_key(|p| p.0);
        self.0.dedup_by_key(|p| p.0);
        self.0.retain(|p| p.1.is_finite() && (p.1 - default).abs() > 1e-4);
        self
    }
}

/// Edits of one word (stored on its first note).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct WordEdit {
    /// The word when it was edited: if the score is reloaded and the note with this id now sings
    /// another word (bars added or removed before it), the edit is not applied.
    pub word: String,
    /// Phonemes of the singer, separated by spaces; None = as the dictionary says.
    pub phonemes: Option<String>,
    /// Shift of each phoneme's start in ms (negative = earlier). Ignored when the word no
    /// longer has as many phonemes.
    pub offsets: Vec<f64>,
}

impl WordEdit {
    pub fn is_empty(&self) -> bool {
        self.phonemes.as_deref().is_none_or(|p| p.trim().is_empty()) && self.offsets.iter().all(|o| *o == 0.0)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PartEdits {
    /// Pronunciation of a word every time it is sung (word as the score is read, lower case).
    pub words: BTreeMap<String, String>,
    /// Edits of single words, by the stable id of their first note.
    pub notes: BTreeMap<String, WordEdit>,
    /// Intonation shift in cents.
    pub pitch: Curve,
    /// Volume in dB (applied after singing, no new render needed).
    pub dynamics: Curve,
    /// Timbre: -1 darker … +1 brighter (voicebanks with a key-shift embedding).
    pub gender: Curve,
    /// Consonants: -0.5 slower … +0.5 faster, log2 of the speed (voicebanks with a speed embedding).
    pub velocity: Curve,
}

impl PartEdits {
    /// The edits that change the sung audio (not the dynamics), as text for a cache key.
    pub fn render_key(&self) -> String {
        let curves = [&self.pitch, &self.gender, &self.velocity];
        format!("{:?}|{:?}|{curves:?}", self.words, self.notes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curve_interpolates_and_falls_back_to_default() {
        let c = Curve(vec![(100, 50.0), (105, 100.0)]);
        assert_eq!(c.at(100.0, 0.0), 50.0);
        assert_eq!(c.at(102.5, 0.0), 75.0);
        assert_eq!(c.at(107.5, 0.0), 50.0, "half way back to the default");
        assert_eq!(c.at(500.0, 0.0), 0.0);
        assert_eq!(Curve::default().at(3.0, 1.0), 1.0);
        let n = Curve(vec![(12, 1.0), (3, 0.0), (11, 2.0)]).normalized(0.0);
        assert_eq!(n.0, vec![(10, 1.0)]);
    }

    #[test]
    fn old_and_partial_edits_load() {
        let e: PartEdits = serde_json::from_str(r#"{"words":{"pace":"p a tS e"},"notes":{"3.1.0":{"offsets":[-20,0]}}}"#).unwrap();
        assert_eq!(e.words["pace"], "p a tS e");
        assert_eq!(e.notes["3.1.0"].offsets, vec![-20.0, 0.0]);
        assert!(e.pitch.is_empty());
    }
}
