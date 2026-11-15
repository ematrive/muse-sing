//! `dsconfig.yaml` and `vocoder.yaml` (OpenUtau.Core/DiffSinger/DiffSingerConfig.cs, DiffSingerVocoder.cs).
//! OpenUtau reads them with YamlDotNet's underscored naming, so `hiddenSize` is `hidden_size` and so on.

use std::path::Path;

use serde::Deserialize;

use crate::error::{Error, Result};

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct RandomPitchShifting {
    pub range: Vec<f32>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct AugmentationArgs {
    pub random_pitch_shifting: Option<RandomPitchShifting>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DsConfig {
    pub phonemes: String,
    pub languages: Option<String>,
    pub acoustic: Option<String>,
    pub vocoder: Option<String>,
    pub speakers: Option<Vec<String>>,
    pub hidden_size: usize,
    pub use_key_shift_embed: bool,
    pub use_speed_embed: bool,
    pub use_energy_embed: bool,
    pub use_breathiness_embed: bool,
    pub use_voicing_embed: bool,
    pub use_tension_embed: bool,
    pub augmentation_args: Option<AugmentationArgs>,
    pub use_continuous_acceleration: bool,
    pub use_lang_id: bool,
    pub use_shallow_diffusion: Option<bool>,
    pub use_variable_depth: Option<bool>,
    #[serde(rename = "max_depth")]
    pub max_depth_raw: f64,
    pub dur: Option<String>,
    pub dur_type: Option<String>,
    pub linguistic: Option<String>,
    pub pitch: Option<String>,
    pub variance: Option<String>,
    pub predict_dur: bool,
    pub predict_energy: bool,
    pub predict_breathiness: bool,
    pub predict_voicing: bool,
    pub predict_tension: bool,
    pub use_expr: bool,
    pub use_note_rest: bool,
    pub sample_rate: u32,
    pub hop_size: u32,
    pub win_size: u32,
    pub fft_size: u32,
    pub num_mel_bins: u32,
    pub mel_fmin: f64,
    pub mel_fmax: f64,
    pub mel_base: Scalar,
    pub mel_scale: String,
}

impl Default for DsConfig {
    fn default() -> Self {
        DsConfig {
            phonemes: "phonemes.txt".into(),
            languages: None,
            acoustic: None,
            vocoder: None,
            speakers: None,
            hidden_size: 256,
            use_key_shift_embed: false,
            use_speed_embed: false,
            use_energy_embed: false,
            use_breathiness_embed: false,
            use_voicing_embed: false,
            use_tension_embed: false,
            augmentation_args: None,
            use_continuous_acceleration: false,
            use_lang_id: false,
            use_shallow_diffusion: None,
            use_variable_depth: None,
            max_depth_raw: 0.0,
            dur: None,
            dur_type: None,
            linguistic: None,
            pitch: None,
            variance: None,
            predict_dur: true,
            predict_energy: true,
            predict_breathiness: true,
            predict_voicing: false,
            predict_tension: false,
            use_expr: false,
            use_note_rest: false,
            sample_rate: 44100,
            hop_size: 512,
            win_size: 2048,
            fft_size: 2048,
            num_mel_bins: 128,
            mel_fmin: 40.0,
            mel_fmax: 16000.0,
            mel_base: Scalar("10".into()),
            mel_scale: "slaney".into(),
        }
    }
}

impl DsConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).map_err(|e| Error::Other(format!("{}: {e}", path.display())))?;
        serde_yaml::from_str(&text).map_err(|e| Error::Format(format!("{}: {e}", path.display())))
    }

    pub fn frame_ms(&self) -> f64 {
        1000.0 * self.hop_size as f64 / self.sample_rate as f64
    }

    pub fn use_variable_depth(&self) -> bool {
        self.use_variable_depth.or(self.use_shallow_diffusion).unwrap_or(false)
    }

    /// Largest shallow-diffusion depth, from 0 to 1.
    pub fn max_depth(&self) -> f64 {
        if self.use_continuous_acceleration { self.max_depth_raw } else { self.max_depth_raw / 1000.0 }
    }

    pub fn key_shift_range(&self) -> Option<(f32, f32)> {
        let r = &self.augmentation_args.as_ref()?.random_pitch_shifting.as_ref()?.range;
        (r.len() == 2).then(|| (r[0], r[1]))
    }
}

/// A YAML scalar read as text whether it is written quoted or not (`mel_base: 10` or `'10'`).
#[derive(Debug, Clone, PartialEq)]
pub struct Scalar(pub String);

impl<'de> Deserialize<'de> for Scalar {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let v = serde_yaml::Value::deserialize(d)?;
        Ok(Scalar(match v {
            serde_yaml::Value::String(s) => s,
            serde_yaml::Value::Number(n) => n.to_string(),
            serde_yaml::Value::Bool(b) => b.to_string(),
            _ => String::new(),
        }))
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct VocoderConfig {
    pub name: String,
    pub model: String,
    pub sample_rate: u32,
    pub hop_size: u32,
    pub win_size: u32,
    pub fft_size: u32,
    pub num_mel_bins: u32,
    pub mel_fmin: f64,
    pub mel_fmax: f64,
    pub mel_base: Scalar,
    pub mel_scale: String,
    pub pitch_controllable: bool,
}

impl Default for VocoderConfig {
    fn default() -> Self {
        VocoderConfig {
            name: "vocoder".into(),
            model: "model.onnx".into(),
            sample_rate: 44100,
            hop_size: 512,
            win_size: 2048,
            fft_size: 2048,
            num_mel_bins: 128,
            mel_fmin: 40.0,
            mel_fmax: 16000.0,
            mel_base: Scalar("10".into()),
            mel_scale: "slaney".into(),
            pitch_controllable: false,
        }
    }
}

/// Mel bins above this are not a vocoder (OpenUtau's DsVocoderConfig.MaxMelBins).
pub const MAX_MEL_BINS: u32 = 255;

impl VocoderConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).map_err(|e| Error::Other(format!("{}: {e}", path.display())))?;
        serde_yaml::from_str(&text).map_err(|e| Error::Format(format!("{}: {e}", path.display())))
    }

    pub fn frame_ms(&self) -> f64 {
        1000.0 * self.hop_size as f64 / self.sample_rate as f64
    }
}

/// Phoneme list: `phonemes.txt` (one per line, the line number is the token) or a JSON map.
pub fn load_phonemes(path: &Path) -> Result<std::collections::HashMap<String, i64>> {
    let text = std::fs::read_to_string(path).map_err(|e| Error::Other(format!("{}: {e}", path.display())))?;
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("json")) {
        return serde_json::from_str(&text).map_err(|e| Error::Format(format!("{}: {e}", path.display())));
    }
    // File.ReadAllLines: split on \n, drop the \r of CRLF files, no trailing empty line
    let mut lines: Vec<&str> = text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    Ok(lines.into_iter().enumerate().map(|(i, l)| (l.to_string(), i as i64)).collect())
}

pub fn load_language_ids(path: &Path) -> Result<std::collections::HashMap<String, i64>> {
    let text = std::fs::read_to_string(path).map_err(|e| Error::Other(format!("{}: {e}", path.display())))?;
    serde_json::from_str(&text).map_err(|e| Error::Format(format!("{}: {e}", path.display())))
}

/// `en/aa` → `en`; phonemes without a prefix have no language.
pub fn phoneme_language(ph: &str) -> &str {
    ph.split_once('/').map(|(l, _)| l).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_underscored_keys_and_defaults() {
        let c: DsConfig = serde_yaml::from_str(
            "phonemes: dsacoustic/phonemes.txt\nacoustic: a.onnx\nhidden_size: 256\nuse_key_shift_embed: true\n\
             use_continuous_acceleration: true\nuse_variable_depth: true\nmax_depth: 0.6\nmel_base: '10'\n\
             speakers:\n  - dsacoustic/tiger_fresh\naugmentation_args:\n  random_pitch_shifting:\n    enabled: true\n    range: [-5., 5.]\n",
        )
        .unwrap();
        assert!(c.use_key_shift_embed && c.use_variable_depth());
        assert_eq!(c.max_depth(), 0.6);
        assert_eq!(c.key_shift_range(), Some((-5.0, 5.0)));
        assert_eq!(c.mel_base, Scalar("10".into()));
        assert_eq!(c.sample_rate, 44100);
        assert!(c.predict_dur);
        let c: DsConfig = serde_yaml::from_str("max_depth: 600\nmel_base: 10\n").unwrap();
        assert_eq!(c.max_depth(), 0.6);
        assert_eq!(c.mel_base.0, "10");
    }
}
