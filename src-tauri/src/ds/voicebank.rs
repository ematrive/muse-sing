//! A DiffSinger voicebank folder as OpenUtau lays it out (DiffSingerSinger.cs): `character.yaml`,
//! root `dsconfig.yaml` (acoustic model), `dsdur/` (duration), `dspitch/` (pitch), `dsvariance/`,
//! a bundled `dsvocoder/` or a vocoder installed in OpenUtau's Dependencies folder.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::Deserialize;

use super::config::{load_language_ids, load_phonemes, DsConfig, VocoderConfig, MAX_MEL_BINS};
use super::onnx::Model;
use crate::error::{Error, Result};

#[derive(Debug, Clone, Deserialize)]
pub struct Subbank {
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub suffix: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Character {
    name: Option<String>,
    subbanks: Vec<Subbank>,
}

/// Speaker embeddings (`<speaker>.emb`, `hidden_size` little-endian floats) of one model folder.
pub struct Speakers {
    names: Vec<String>,
    embeds: Vec<Vec<f32>>,
    pub hidden: usize,
}

impl Speakers {
    pub fn load(root: &Path, cfg: &DsConfig) -> Result<Option<Speakers>> {
        let Some(names) = &cfg.speakers else { return Ok(None) };
        let mut embeds = vec![];
        for n in names {
            let p = root.join(format!("{n}.emb"));
            let bytes = std::fs::read(&p).map_err(|e| Error::Other(format!("{}: {e}", p.display())))?;
            if bytes.len() < cfg.hidden_size * 4 {
                return Err(Error::Format(format!("{}: too short for hidden_size {}", p.display(), cfg.hidden_size)));
            }
            embeds.push(bytes.as_chunks::<4>().0.iter().take(cfg.hidden_size).map(|b| f32::from_le_bytes(*b)).collect());
        }
        Ok(Some(Speakers { names: names.clone(), embeds, hidden: cfg.hidden_size }))
    }

    /// DiffSingerSpeakerEmbedManager.getSpeakerIndexBySuffix: exact name, then matching trailing path
    /// segments, then the first speaker.
    pub fn index_of(&self, suffix: &str) -> usize {
        if let Some(i) = self.names.iter().position(|n| n == suffix) {
            return i;
        }
        let seg = |s: &str| s.split(['/', '\\']).map(str::to_string).collect::<Vec<_>>();
        let sf = seg(suffix);
        self.names
            .iter()
            .position(|n| {
                let sp = seg(n);
                sf.len() <= sp.len() && sp[sp.len() - sf.len()..] == sf[..]
            })
            .unwrap_or(0)
    }

    /// [rows, hidden] with the speaker's embedding on every row.
    pub fn repeat(&self, suffix: &str, rows: usize) -> Vec<f32> {
        let e = &self.embeds[self.index_of(suffix)];
        let mut out = Vec::with_capacity(rows * self.hidden);
        for _ in 0..rows {
            out.extend_from_slice(e);
        }
        out
    }
}

/// One of the variance-side model folders (`dsdur`, `dspitch`, `dsvariance`).
pub struct SubModel {
    pub root: PathBuf,
    pub config: DsConfig,
    pub tokens: HashMap<String, i64>,
    pub language_ids: HashMap<String, i64>,
    pub linguistic: Model,
    pub main: Model,
    pub speakers: Option<Speakers>,
}

impl SubModel {
    fn load(root: &Path, main_key: fn(&DsConfig) -> Option<&String>) -> Result<SubModel> {
        let config = DsConfig::load(&root.join("dsconfig.yaml"))?;
        let tokens = load_phonemes(&root.join(&config.phonemes))?;
        let language_ids = match (&config.languages, config.use_lang_id) {
            (Some(l), true) => load_language_ids(&root.join(l))?,
            (None, true) => return Err(Error::Format(format!("{}: \"languages\" missing in dsconfig.yaml", root.display()))),
            _ => HashMap::new(),
        };
        let key = |k: Option<&String>, what: &str| {
            k.cloned().ok_or_else(|| Error::Format(format!("{}: \"{what}\" missing in dsconfig.yaml", root.display())))
        };
        let linguistic = Model::load(&root.join(key(config.linguistic.as_ref(), "linguistic")?))?;
        let main = Model::load(&root.join(key(main_key(&config), "model")?))?;
        let speakers = Speakers::load(root, &config)?;
        Ok(SubModel { root: root.to_path_buf(), config, tokens, language_ids, linguistic, main, speakers })
    }

    pub fn frame_ms(&self) -> f64 {
        self.config.frame_ms()
    }

    pub fn token(&self, ph: &str) -> Result<i64> {
        self.tokens.get(ph).copied().ok_or_else(|| {
            Error::Other(format!("phoneme \"{ph}\" isn't supported by {}", self.root.join(&self.config.phonemes).display()))
        })
    }
}

pub struct Vocoder {
    pub config: VocoderConfig,
    pub model: Model,
}

pub struct Voicebank {
    pub dir: PathBuf,
    pub name: String,
    pub config: DsConfig,
    pub subbanks: Vec<Subbank>,
    pub tokens: HashMap<String, i64>,
    pub language_ids: HashMap<String, i64>,
    pub speakers: Option<Speakers>,
    acoustic: OnceLock<std::result::Result<Model, String>>,
    vocoder: OnceLock<std::result::Result<Vocoder, String>>,
    dur: OnceLock<std::result::Result<SubModel, String>>,
    pitch: OnceLock<std::result::Result<SubModel, String>>,
}

fn lazy<T>(cell: &OnceLock<std::result::Result<T, String>>, f: impl FnOnce() -> Result<T>) -> Result<&T> {
    cell.get_or_init(|| f().map_err(|e| e.to_string())).as_ref().map_err(|e| Error::Other(e.clone()))
}

impl Voicebank {
    pub fn load(dir: &Path) -> Result<Voicebank> {
        let character: Character = std::fs::read_to_string(dir.join("character.yaml"))
            .ok()
            .and_then(|t| serde_yaml::from_str(&t).ok())
            .unwrap_or_default();
        let config = DsConfig::load(&dir.join("dsconfig.yaml"))?;
        let tokens = load_phonemes(&dir.join(&config.phonemes))?;
        let language_ids = match (&config.languages, config.use_lang_id) {
            (Some(l), true) => load_language_ids(&dir.join(l))?,
            _ => HashMap::new(),
        };
        let speakers = Speakers::load(dir, &config)?;
        let name = character.name.unwrap_or_else(|| dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
        Ok(Voicebank {
            dir: dir.to_path_buf(),
            name,
            config,
            subbanks: character.subbanks,
            tokens,
            language_ids,
            speakers,
            acoustic: OnceLock::new(),
            vocoder: OnceLock::new(),
            dur: OnceLock::new(),
            pitch: OnceLock::new(),
        })
    }

    /// Speaker suffix of a subbank (voice colour); the first subbank by default.
    pub fn speaker_suffix(&self, color: Option<&str>) -> String {
        color
            .and_then(|c| self.subbanks.iter().find(|s| s.color == c))
            .or_else(|| self.subbanks.first())
            .map(|s| s.suffix.clone())
            .unwrap_or_default()
    }

    pub fn has_pitch_model(&self) -> bool {
        self.dir.join("dspitch").join("dsconfig.yaml").is_file()
    }

    pub fn needs_variance(&self) -> bool {
        let c = &self.config;
        c.use_energy_embed || c.use_breathiness_embed || c.use_voicing_embed || c.use_tension_embed
    }

    pub fn acoustic(&self) -> Result<&Model> {
        lazy(&self.acoustic, || {
            let a = self.config.acoustic.as_ref().ok_or_else(|| Error::Format("dsconfig.yaml has no \"acoustic\"".into()))?;
            Model::load(&self.dir.join(a))
        })
    }

    pub fn duration(&self) -> Result<&SubModel> {
        lazy(&self.dur, || SubModel::load(&self.dir.join("dsdur"), |c| c.dur.as_ref()))
    }

    pub fn pitch(&self) -> Result<&SubModel> {
        lazy(&self.pitch, || SubModel::load(&self.dir.join("dspitch"), |c| c.pitch.as_ref()))
    }

    /// The bundled `dsvocoder` folder, or the named vocoder installed in OpenUtau.
    fn vocoder_dir(&self) -> Result<PathBuf> {
        let bundled = self.dir.join("dsvocoder");
        if bundled.join("vocoder.yaml").is_file() {
            return Ok(bundled);
        }
        let name = self.config.vocoder.clone().unwrap_or_default();
        let found = vocoder_dirs().into_iter().map(|d| d.join(&name)).find(|d| !name.is_empty() && d.join("vocoder.yaml").is_file());
        found.ok_or_else(|| Error::Other(format!("il cantante è incompleto: manca il vocoder «{name}» (cartella dsvocoder)")))
    }

    fn vocoder_config(&self) -> Result<(PathBuf, VocoderConfig)> {
        let dir = self.vocoder_dir()?;
        let config = VocoderConfig::load(&dir.join("vocoder.yaml"))?;
        if config.num_mel_bins < 1 || config.num_mel_bins > MAX_MEL_BINS {
            return Err(Error::Format(format!("vocoder num_mel_bins must be between 1 and {MAX_MEL_BINS}")));
        }
        Ok((dir, config))
    }

    pub fn vocoder(&self) -> Result<&Vocoder> {
        lazy(&self.vocoder, || {
            let (dir, config) = self.vocoder_config()?;
            let model = Model::load(&dir.join(&config.model))?;
            Ok(Vocoder { config, model })
        })
    }

    /// Why Muse Sing's engine can't sing with this voicebank, if it can't (checked from the
    /// configuration files only, without loading the models).
    pub fn engine_problem(&self) -> Option<String> {
        if self.config.acoustic.is_none() {
            return Some("manca il modello acustico".into());
        }
        if !self.dir.join("dsdur").join("dsconfig.yaml").is_file() {
            return Some("manca il modello delle durate (dsdur)".into());
        }
        if self.needs_variance() {
            return Some("richiede il modello variance, non ancora supportato".into());
        }
        let (_, voc) = match self.vocoder_config() {
            Ok(v) => v,
            Err(e) => return Some(e.to_string()),
        };
        if voc.sample_rate != super::render::SAMPLE_RATE {
            return Some(format!("vocoder a {} Hz, non ancora supportato", voc.sample_rate));
        }
        if voc.sample_rate != self.config.sample_rate || voc.hop_size != self.config.hop_size || voc.num_mel_bins != self.config.num_mel_bins {
            return Some("vocoder e modello acustico non compatibili".into());
        }
        None
    }

    /// Voice colours (subbanks), in order.
    pub fn colors(&self) -> Vec<String> {
        self.subbanks.iter().map(|s| s.color.clone()).filter(|c| !c.is_empty()).collect()
    }
}

/// Folders where named vocoders are installed: OpenUtau's Dependencies folder.
fn vocoder_dirs() -> Vec<PathBuf> {
    crate::singers::openutau_data_dir().map(|d| vec![d.join("Dependencies")]).unwrap_or_default()
}
