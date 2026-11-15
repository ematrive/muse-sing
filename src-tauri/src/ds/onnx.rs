//! Thin layer over ONNX Runtime (`ort`): one shared environment, sessions behind a mutex
//! (`Session::run` takes `&mut self`), inputs built by name with the rank the model declares.

use std::path::Path;
use std::sync::{Mutex, OnceLock};

use ort::session::Session;
use ort::value::{DynValue, Tensor};

use crate::error::{Error, Result};

/// The process-wide ONNX Runtime environment (only one is allowed).
pub fn env() -> Result<&'static ort::environment::Environment> {
    static ENV: OnceLock<std::result::Result<ort::environment::Environment, String>> = OnceLock::new();
    ENV.get_or_init(|| ort::init().with_name("muse-sing").build().map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|e| Error::Other(format!("ONNX Runtime: {e}")))
}

fn ort_err(what: &str) -> impl Fn(ort::Error) -> Error + '_ {
    move |e| Error::Other(format!("{what}: {e}"))
}

pub struct Model {
    name: String,
    session: Mutex<Session>,
    /// (input name, declared rank)
    inputs: Vec<(String, usize)>,
}

/// An input value before it is turned into a tensor; the shape is completed by `Model::run`.
pub enum Input {
    I64(Vec<i64>),
    I32(Vec<i32>),
    F32(Vec<f32>),
    Bool(Vec<bool>),
    /// Float matrix [rows, cols] (encoder output, speaker embeddings).
    F32x2(Vec<f32>, usize),
    /// Encoder output passed through unchanged, with its full shape.
    Raw(DynValue),
}

pub struct Outputs(Vec<(String, DynValue)>);

impl Outputs {
    pub fn take(&mut self, name: &str) -> Result<DynValue> {
        let i = self.0.iter().position(|(n, _)| n == name).ok_or_else(|| Error::Other(format!("output \"{name}\" missing")))?;
        Ok(self.0.remove(i).1)
    }

    pub fn first(&mut self) -> Result<DynValue> {
        if self.0.is_empty() {
            return Err(Error::Other("model returned no output".into()));
        }
        Ok(self.0.remove(0).1)
    }
}

pub fn floats(v: &DynValue) -> Result<(Vec<i64>, Vec<f32>)> {
    let (shape, data) = v.try_extract_tensor::<f32>().map_err(ort_err("float output"))?;
    Ok((shape.to_vec(), data.to_vec()))
}

impl Model {
    pub fn load(path: &Path) -> Result<Model> {
        let mut builder = Session::builder(env()?).map_err(ort_err("ONNX session"))?;
        let session = builder
            .commit_from_file(path)
            .map_err(|e| Error::Other(format!("{}: {e}", path.display())))?;
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        Ok(Self::new(name, session))
    }

    pub fn load_bytes(name: &str, bytes: &[u8]) -> Result<Model> {
        let mut builder = Session::builder(env()?).map_err(ort_err("ONNX session"))?;
        let session = builder.commit_from_memory(bytes).map_err(|e| Error::Other(format!("{name}: {e}")))?;
        Ok(Self::new(name.to_string(), session))
    }

    fn new(name: String, session: Session) -> Model {
        let inputs = session
            .inputs()
            .iter()
            .map(|i| (i.name().to_string(), i.dtype().tensor_shape().map(|s| s.len()).unwrap_or(0)))
            .collect();
        Model { name, session: Mutex::new(session), inputs }
    }

    pub fn has_input(&self, name: &str) -> bool {
        self.inputs.iter().any(|(n, _)| n == name)
    }

    /// Runs the model. Inputs are given as 1-D data: the batch axis is added from the declared rank
    /// (`[n]` → `[1, n]`, scalars stay `[]` or become `[1]`). Inputs the model doesn't declare are an
    /// error, like OpenUtau's `Onnx.VerifyInputNames`, and so are declared inputs that are missing.
    pub fn run(&self, inputs: Vec<(&str, Input)>) -> Result<Outputs> {
        for (n, _) in &self.inputs {
            if !inputs.iter().any(|(k, _)| k == n) {
                return Err(Error::Other(format!("{}: input \"{n}\" not provided", self.name)));
            }
        }
        let mut values: Vec<(String, DynValue)> = vec![];
        for (name, input) in inputs {
            let rank = self
                .inputs
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, r)| *r)
                .ok_or_else(|| Error::Other(format!("{}: unexpected input \"{name}\"", self.name)))?;
            let shape = |len: usize| -> Vec<i64> {
                match rank {
                    0 => vec![],
                    1 => vec![len as i64],
                    _ => vec![1, len as i64],
                }
            };
            let err = ort_err("input tensor");
            let v: DynValue = match input {
                Input::I64(d) => Tensor::from_array((shape(d.len()), d)).map_err(err)?.into_dyn(),
                Input::I32(d) => Tensor::from_array((shape(d.len()), d)).map_err(err)?.into_dyn(),
                Input::F32(d) => Tensor::from_array((shape(d.len()), d)).map_err(err)?.into_dyn(),
                Input::Bool(d) => Tensor::from_array((shape(d.len()), d)).map_err(err)?.into_dyn(),
                Input::F32x2(d, cols) => {
                    let rows = d.len().checked_div(cols).unwrap_or(0);
                    Tensor::from_array((vec![1, rows as i64, cols as i64], d)).map_err(err)?.into_dyn()
                }
                Input::Raw(v) => v,
            };
            values.push((name.to_string(), v));
        }
        let mut session = self.session.lock().map_err(|_| Error::Other("ONNX session poisoned".into()))?;
        let outputs = session.run(values).map_err(|e| Error::Other(format!("{}: {e}", self.name)))?;
        let mut out = vec![];
        for (name, value) in outputs {
            out.push((name.to_string(), value));
        }
        Ok(Outputs(out))
    }
}
