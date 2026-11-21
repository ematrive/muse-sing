//! Downloads TIGER DS from its author's GitHub release and installs it in Muse Sing's Singers
//! folder. Tiger's licence does not allow redistributing it, so the app never ships it: the user
//! downloads it from the official release.
//!
//! The release is a "pack" zip with OpenUtau plugins and, stored inside, the voicebank zip
//! (`Voice Library/TIGER_DS_v106.zip`); only the voicebank is extracted.

use crate::error::{Error, Result};
use serde::Serialize;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub const PACK_URL: &str = "https://github.com/spicytigermeat/tiger_diffsinger/releases/download/v106/TIGER_DS_v106_PACK.zip";
pub const PACK_FILE: &str = "TIGER_DS_v106_PACK.zip";
/// Name of the installed folder inside the Singers folder.
pub const FOLDER: &str = "Tiger";

#[derive(Clone, Copy, Debug, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Download,
    Extract,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Progress {
    pub phase: Phase,
    pub done: u64,
    /// 0 when unknown
    pub total: u64,
}

/// Downloads `url` to `dest` (through `dest.part`), reporting the bytes received.
pub async fn download(url: &str, dest: &Path, mut on: impl FnMut(Progress)) -> Result<()> {
    // the updater plugin does the same before using reqwest with rustls
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    let net = |e: reqwest::Error| Error::Other(format!("download di Tiger non riuscito: {e}"));
    let client = reqwest::Client::builder().user_agent(concat!("MuseSing/", env!("CARGO_PKG_VERSION"))).build().map_err(net)?;
    let mut resp = client.get(url).send().await.map_err(net)?.error_for_status().map_err(net)?;
    let total = resp.content_length().unwrap_or(0);
    let part = dest.with_extension("zip.part");
    let mut file = std::io::BufWriter::new(File::create(&part)?);
    let (mut done, mut shown) = (0u64, 0u64);
    on(Progress { phase: Phase::Download, done, total });
    while let Some(chunk) = resp.chunk().await.map_err(net)? {
        file.write_all(&chunk)?;
        done += chunk.len() as u64;
        if done - shown >= 1 << 20 {
            shown = done;
            on(Progress { phase: Phase::Download, done, total });
        }
    }
    file.flush()?;
    drop(file);
    if total > 0 && done != total {
        let _ = std::fs::remove_file(&part);
        return Err(Error::Other("download di Tiger interrotto: riprova".into()));
    }
    std::fs::rename(&part, dest)?;
    on(Progress { phase: Phase::Download, done, total: done });
    Ok(())
}

/// A byte range of a file, read as if it were a whole file (a zip stored inside a zip).
struct Window<R> {
    inner: R,
    start: u64,
    len: u64,
    pos: u64,
}

impl<R: Read + Seek> Read for Window<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let left = self.len.saturating_sub(self.pos);
        let n = (buf.len() as u64).min(left) as usize;
        if n == 0 {
            return Ok(0);
        }
        self.inner.seek(SeekFrom::Start(self.start + self.pos))?;
        let n = self.inner.read(&mut buf[..n])?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl<R: Read + Seek> Seek for Window<R> {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        let pos = match to {
            SeekFrom::Start(p) => p as i64,
            SeekFrom::End(d) => self.len as i64 + d,
            SeekFrom::Current(d) => self.pos as i64 + d,
        };
        if pos < 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "seek before start"));
        }
        self.pos = pos as u64;
        Ok(self.pos)
    }
}

fn bad(e: zip::result::ZipError) -> Error {
    Error::Format(format!("pacchetto di Tiger non valido: {e}"))
}

/// The voicebank zip inside the pack: `Voice Library/<name>.zip`.
fn inner_name<R: Read + Seek>(pack: &zip::ZipArchive<R>) -> Option<String> {
    pack.file_names().find(|n| n.contains("Voice Library/") && n.to_lowercase().ends_with(".zip")).map(str::to_string)
}

/// A free folder name in `singers`: `Tiger`, then `Tiger 2`, …
fn free_dir(singers: &Path) -> PathBuf {
    (1..)
        .map(|k| singers.join(if k == 1 { FOLDER.to_string() } else { format!("{FOLDER} {k}") }))
        .find(|p| !p.exists())
        .unwrap()
}

/// Extracts the voicebank of the pack at `pack_path` into a new folder of `singers` and returns it.
pub fn install_pack(pack_path: &Path, singers: &Path, scratch: &Path, mut on: impl FnMut(Progress)) -> Result<PathBuf> {
    let mut pack = zip::ZipArchive::new(BufReader::new(File::open(pack_path)?)).map_err(bad)?;
    let name = inner_name(&pack).ok_or_else(|| Error::Format("il pacchetto di Tiger non contiene il voicebank".into()))?;
    let (stored, start, len) = {
        let f = pack.by_name(&name).map_err(bad)?;
        (f.compression() == zip::CompressionMethod::Stored, f.data_start(), f.size())
    };
    let mut copy = None;
    let reader: Box<dyn ReadSeek> = if stored {
        Box::new(Window { inner: BufReader::new(File::open(pack_path)?), start, len, pos: 0 })
    } else {
        // compressed inside the pack: decompress it to a file first
        let tmp = scratch.join("tiger_voicebank.zip");
        let mut out = File::create(&tmp)?;
        std::io::copy(&mut pack.by_name(&name).map_err(bad)?, &mut out)?;
        copy = Some(tmp.clone());
        Box::new(BufReader::new(File::open(&tmp)?))
    };
    let result = extract(reader, singers, &mut on);
    if let Some(tmp) = copy {
        let _ = std::fs::remove_file(tmp);
    }
    result
}

trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}

fn extract(reader: Box<dyn ReadSeek>, singers: &Path, on: &mut impl FnMut(Progress)) -> Result<PathBuf> {
    let mut vb = zip::ZipArchive::new(reader).map_err(bad)?;
    let total: u64 = (0..vb.len()).filter_map(|i| vb.by_index_raw(i).ok().map(|f| f.size())).sum();
    let part = singers.join(format!("{FOLDER}.part"));
    if part.exists() {
        std::fs::remove_dir_all(&part)?;
    }
    std::fs::create_dir_all(&part)?;
    let mut done = 0u64;
    let mut run = || -> Result<()> {
        for i in 0..vb.len() {
            let mut f = vb.by_index(i).map_err(bad)?;
            // never write outside the folder (`..`, absolute paths)
            let Some(rel) = f.enclosed_name() else { continue };
            let out = part.join(rel);
            if f.is_dir() {
                std::fs::create_dir_all(&out)?;
                continue;
            }
            if let Some(dir) = out.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let mut w = std::io::BufWriter::new(File::create(&out)?);
            let mut buf = vec![0u8; 1 << 20];
            loop {
                let n = f.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                w.write_all(&buf[..n])?;
                done += n as u64;
                on(Progress { phase: Phase::Extract, done, total });
            }
            w.flush()?;
        }
        // skipped entries count in the total too
        on(Progress { phase: Phase::Extract, done: total, total });
        Ok(())
    };
    if let Err(e) = run() {
        let _ = std::fs::remove_dir_all(&part);
        return Err(e);
    }
    if !has_character(&part, 0) {
        let _ = std::fs::remove_dir_all(&part);
        return Err(Error::Format("il pacchetto di Tiger non contiene character.txt".into()));
    }
    let dest = free_dir(singers);
    std::fs::rename(&part, &dest)?;
    Ok(dest)
}

fn has_character(dir: &Path, depth: usize) -> bool {
    if dir.join("character.txt").is_file() {
        return true;
    }
    depth < 3
        && std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .any(|e| e.path().is_dir() && has_character(&e.path(), depth + 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::write::SimpleFileOptions;

    fn zip_bytes(files: &[(&str, &[u8])], method: zip::CompressionMethod) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, data) in files {
            w.start_file(*name, SimpleFileOptions::default().compression_method(method)).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    fn pack(method: zip::CompressionMethod) -> Vec<u8> {
        let vb = zip_bytes(
            &[("character.txt", b""), ("dsconfig.yaml", b"phonemes: x"), ("dsacoustic/acoustic.onnx", &[7u8; 5000]), ("../evil.txt", b"no")],
            zip::CompressionMethod::Deflated,
        );
        zip_bytes(&[("PACK/readme.txt", b"hi"), ("PACK/Voice Library/TIGER_DS_v106.zip", &vb)], method)
    }

    #[test]
    fn installs_the_voicebank_from_the_pack() {
        for (k, method) in [zip::CompressionMethod::Stored, zip::CompressionMethod::Deflated].into_iter().enumerate() {
            let dir = std::env::temp_dir().join(format!("musesing-tiger-test-{}-{k}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            let singers = dir.join("Singers");
            std::fs::create_dir_all(&singers).unwrap();
            let p = dir.join(PACK_FILE);
            std::fs::write(&p, pack(method)).unwrap();
            let mut last = None;
            let got = install_pack(&p, &singers, &dir, |pr| last = Some(pr)).unwrap();
            assert_eq!(got, singers.join("Tiger"));
            assert!(got.join("character.txt").is_file());
            assert_eq!(std::fs::read(got.join("dsacoustic/acoustic.onnx")).unwrap().len(), 5000);
            assert!(!dir.join("evil.txt").exists() && !singers.join("evil.txt").exists());
            let last = last.unwrap();
            assert_eq!((last.phase, last.done), (Phase::Extract, last.total));
            // a second install never overwrites the first
            assert_eq!(install_pack(&p, &singers, &dir, |_| {}).unwrap(), singers.join("Tiger 2"));
            assert!(!dir.join("tiger_voicebank.zip").exists());
            std::fs::remove_dir_all(&dir).unwrap();
        }
    }

    #[test]
    fn rejects_a_pack_without_voicebank() {
        let dir = std::env::temp_dir().join(format!("musesing-tiger-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(PACK_FILE);
        std::fs::write(&p, zip_bytes(&[("readme.txt", b"hi")], zip::CompressionMethod::Stored)).unwrap();
        assert!(install_pack(&p, &dir, &dir, |_| {}).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
