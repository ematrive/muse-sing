//! Mixing the sung parts (volume, pan, mute) and exporting the mix as WAV, FLAC or MP3.

use std::path::Path;

use serde::Deserialize;

use crate::error::{Error, Result};

/// Mix settings of one part, from the UI.
#[derive(Debug, Clone, Deserialize)]
pub struct MixPart {
    pub name: String,
    /// Gain in dB (0 = as rendered).
    pub volume_db: f32,
    /// -1 = left, 0 = centre, 1 = right.
    pub pan: f32,
    pub mute: bool,
    /// When any part is soloed, only the soloed ones play.
    #[serde(default)]
    pub solo: bool,
}

/// Highest peak of the exported mix: below full scale, so MP3 decoding doesn't clip.
const PEAK: f32 = 0.95;

/// Sums the tracks into a stereo pair with an equal-power pan law (centre = unity gain on both
/// sides), then scales the whole mix down if it would clip. Tracks missing from `parts` are left
/// out.
pub fn mix(tracks: &[(String, &[f32])], parts: &[MixPart]) -> (Vec<f32>, Vec<f32>) {
    let len = tracks.iter().map(|(_, t)| t.len()).max().unwrap_or(0);
    let (mut l, mut r) = (vec![0f32; len], vec![0f32; len]);
    let any_solo = parts.iter().any(|p| p.solo);
    for (name, samples) in tracks {
        let Some(p) = parts.iter().find(|p| &p.name == name) else { continue };
        if if any_solo { !p.solo } else { p.mute } {
            continue;
        }
        let gain = 10f32.powf(p.volume_db / 20.0);
        let angle = (p.pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
        let (gl, gr) = (gain * angle.cos() * std::f32::consts::SQRT_2, gain * angle.sin() * std::f32::consts::SQRT_2);
        for (i, &x) in samples.iter().enumerate() {
            l[i] += x * gl;
            r[i] += x * gr;
        }
    }
    let peak = l.iter().chain(&r).fold(0f32, |a, &b| a.max(b.abs()));
    if peak > PEAK {
        let k = PEAK / peak;
        l.iter_mut().chain(r.iter_mut()).for_each(|x| *x *= k);
    }
    (l, r)
}

fn to_i16(x: f32) -> i16 {
    (x.clamp(-1.0, 1.0) * 32767.0).round() as i16
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Wav,
    Flac,
    Mp3,
}

impl Format {
    pub fn from_path(path: &Path) -> Result<Format> {
        match path.extension().and_then(|e| e.to_str()).map(str::to_lowercase).as_deref() {
            Some("wav") => Ok(Format::Wav),
            Some("flac") => Ok(Format::Flac),
            Some("mp3") => Ok(Format::Mp3),
            _ => Err(Error::Format("formato non supportato: scegli .mp3, .flac o .wav".into())),
        }
    }
}

/// Writes a stereo mix; the format comes from the file extension. Written to a temporary file
/// first, so a failed export doesn't leave half a file behind.
pub fn export(path: &Path, left: &[f32], right: &[f32], sample_rate: u32, title: &str) -> Result<()> {
    let format = Format::from_path(path)?;
    let bytes = match format {
        Format::Wav => wav_bytes(left, right, sample_rate)?,
        Format::Flac => flac_bytes(left, right, sample_rate)?,
        Format::Mp3 => mp3_bytes(left, right, sample_rate, title)?,
    };
    let tmp = path.with_extension("part");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn wav_bytes(left: &[f32], right: &[f32], sample_rate: u32) -> Result<Vec<u8>> {
    let spec = hound::WavSpec { channels: 2, sample_rate, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let err = |e: hound::Error| Error::Other(format!("WAV: {e}"));
    let mut cur = std::io::Cursor::new(Vec::with_capacity(left.len() * 4 + 44));
    {
        let mut w = hound::WavWriter::new(&mut cur, spec).map_err(err)?;
        for (&a, &b) in left.iter().zip(right) {
            w.write_sample(to_i16(a)).map_err(err)?;
            w.write_sample(to_i16(b)).map_err(err)?;
        }
        w.finalize().map_err(err)?;
    }
    Ok(cur.into_inner())
}

fn flac_bytes(left: &[f32], right: &[f32], sample_rate: u32) -> Result<Vec<u8>> {
    use flacenc::component::BitRepr;
    use flacenc::error::Verify;
    let samples: Vec<i32> = left.iter().zip(right).flat_map(|(&a, &b)| [to_i16(a) as i32, to_i16(b) as i32]).collect();
    let config = flacenc::config::Encoder::default().into_verified().map_err(|e| Error::Other(format!("FLAC: {e:?}")))?;
    let source = flacenc::source::MemSource::from_samples(&samples, 2, 16, sample_rate as usize);
    let stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
        .map_err(|e| Error::Other(format!("FLAC: {e:?}")))?;
    let mut sink = flacenc::bitsink::ByteSink::new();
    stream.write(&mut sink).map_err(|e| Error::Other(format!("FLAC: {e:?}")))?;
    Ok(sink.as_slice().to_vec())
}

fn mp3_bytes(left: &[f32], right: &[f32], sample_rate: u32, title: &str) -> Result<Vec<u8>> {
    use mp3lame_encoder::{Bitrate, Builder, DualPcm, FlushNoGap, Id3Tag, Quality};
    let err = |what: &str| {
        let what = what.to_string();
        move |e: &dyn std::fmt::Debug| Error::Other(format!("MP3 ({what}): {e:?}"))
    };
    let mut b = Builder::new().ok_or_else(|| Error::Other("MP3: encoder LAME non disponibile".into()))?;
    b.set_num_channels(2).map_err(|e| err("canali")(&e))?;
    b.set_sample_rate(sample_rate).map_err(|e| err("frequenza")(&e))?;
    b.set_brate(Bitrate::Kbps192).map_err(|e| err("bitrate")(&e))?;
    b.set_quality(Quality::Good).map_err(|e| err("qualità")(&e))?;
    let title: Vec<u8> = title.bytes().take(250).collect();
    b.set_id3_tag(Id3Tag { title: &title, artist: b"Muse Sing", album: &[], album_art: &[], year: &[], comment: &[] })
        .map_err(|e| err("tag")(&e))?;
    let mut enc = b.build().map_err(|e| err("avvio")(&e))?;
    let l: Vec<i16> = left.iter().map(|&x| to_i16(x)).collect();
    let r: Vec<i16> = right.iter().map(|&x| to_i16(x)).collect();
    let mut out = Vec::with_capacity(l.len() / 4);
    for (a, b) in l.chunks(1152 * 64).zip(r.chunks(1152 * 64)) {
        out.reserve(mp3lame_encoder::max_required_buffer_size(a.len()));
        enc.encode_to_vec(DualPcm { left: a, right: b }, &mut out).map_err(|e| err("codifica")(&e))?;
    }
    out.reserve(7200);
    enc.flush_to_vec::<FlushNoGap>(&mut out).map_err(|e| err("chiusura")(&e))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(name: &str, volume_db: f32, pan: f32, mute: bool) -> MixPart {
        MixPart { name: name.into(), volume_db, pan, mute, solo: false }
    }

    #[test]
    fn solo_wins_over_mute() {
        let a = vec![0.1f32; 2];
        let b = vec![0.2f32; 2];
        let tracks = [("A".to_string(), a.as_slice()), ("B".to_string(), b.as_slice())];
        let mut pa = part("A", 0.0, 0.0, true);
        pa.solo = true;
        let (l, _) = mix(&tracks, &[pa, part("B", 0.0, 0.0, false)]);
        assert!((l[0] - 0.1).abs() < 1e-6, "only the soloed (even if muted) part plays");
    }

    #[test]
    fn pan_volume_and_mute() {
        let a = vec![0.1f32; 4];
        let b = vec![0.2f32; 2];
        let tracks = [("A".to_string(), a.as_slice()), ("B".to_string(), b.as_slice())];
        let (l, r) = mix(&tracks, &[part("A", 0.0, 0.0, false), part("B", 0.0, 0.0, true)]);
        assert_eq!(l.len(), 4);
        assert!((l[0] - 0.1).abs() < 1e-6 && (r[0] - 0.1).abs() < 1e-6, "centre = unity on both sides");
        let (l, r) = mix(&tracks, &[part("A", 6.0206, -1.0, false), part("B", 0.0, 1.0, false)]);
        assert!(r[3].abs() < 1e-6 && (l[3] - 0.2 * std::f32::consts::SQRT_2).abs() < 1e-4, "hard left, +6 dB");
        assert!((r[0] - 0.2 * std::f32::consts::SQRT_2).abs() < 1e-4);
        // loud: scaled down to the peak
        let loud = vec![0.9f32; 3];
        let (l, _) = mix(&[("A".into(), loud.as_slice()), ("B".into(), loud.as_slice())], &[part("A", 0.0, 0.0, false), part("B", 0.0, 0.0, false)]);
        assert!((l[0] - PEAK).abs() < 1e-6);
    }

    #[test]
    fn exports_every_format() {
        let n = 44100 / 4;
        let l: Vec<f32> = (0..n).map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / 44100.0).sin() * 0.5).collect();
        let r: Vec<f32> = l.iter().map(|x| -x).collect();
        let dir = std::env::temp_dir().join(format!("muse-sing-mix-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for ext in ["wav", "flac", "mp3"] {
            let p = dir.join(format!("t.{ext}"));
            export(&p, &l, &r, 44100, "Prova").unwrap();
            let bytes = std::fs::read(&p).unwrap();
            assert!(bytes.len() > 1000, "{ext}: {} bytes", bytes.len());
            match ext {
                "wav" => assert_eq!(&bytes[..4], b"RIFF"),
                "flac" => assert_eq!(&bytes[..4], b"fLaC"),
                _ => assert!(&bytes[..3] == b"ID3" || bytes[0] == 0xFF),
            }
        }
        let (back, _) = crate::audio::read_wav_mono(&dir.join("t.wav")).unwrap();
        assert_eq!(back.len(), n);
        assert!(export(&dir.join("t.ogg"), &l, &r, 44100, "x").is_err());
        std::fs::remove_dir_all(dir).ok();
    }
}
