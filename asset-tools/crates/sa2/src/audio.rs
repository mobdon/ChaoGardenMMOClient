//! Read-only AFS indexing and CRI ADX type-3 to PCM conversion.
//!
//! Format evidence: vgmstream `src/meta/adx.c` and `src/coding/adx_decoder.c`
//! (upstream inspected 2026-11-18). The mathematical decoder is implemented
//! here independently. AHX, encrypted ADX and CSB are deliberately unsupported.

use crate::{Error, Result};
use serde::Serialize;

fn range(input: &[u8], offset: usize, length: usize) -> Result<&[u8]> {
    input
        .get(
            offset
                ..offset
                    .checked_add(length)
                    .ok_or_else(|| Error::invalid("audio range overflow"))?,
        )
        .ok_or_else(|| Error::invalid("truncated audio range"))
}

fn be16(input: &[u8], offset: usize) -> Result<u16> {
    let b = range(input, offset, 2)?;
    Ok(u16::from_be_bytes([b[0], b[1]]))
}

fn be32(input: &[u8], offset: usize) -> Result<u32> {
    let b = range(input, offset, 4)?;
    Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

fn le32(input: &[u8], offset: usize) -> Result<u32> {
    let b = range(input, offset, 4)?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AfsEntry {
    pub index: usize,
    pub offset: usize,
    pub length: usize,
}

impl AfsEntry {
    pub fn bytes<'a>(&self, input: &'a [u8]) -> Result<&'a [u8]> {
        range(input, self.offset, self.length)
    }
}

/// Preserve archive ordering, including empty slots; never interpret filename
/// metadata as a trusted output path. Entries may alias the same source bytes.
pub fn read_afs(input: &[u8]) -> Result<Vec<AfsEntry>> {
    if range(input, 0, 4)? != b"AFS\0" {
        return Err(Error::invalid("not an AFS archive"));
    }
    let count = le32(input, 4)? as usize;
    if count > 1_000_000 {
        return Err(Error::invalid("AFS entry limit exceeded"));
    }
    let table_end = 8usize
        .checked_add(
            count
                .checked_mul(8)
                .ok_or_else(|| Error::invalid("AFS count overflow"))?,
        )
        .ok_or_else(|| Error::invalid("AFS table overflow"))?;
    range(input, 0, table_end)?;
    let mut entries = Vec::with_capacity(count);
    for index in 0..count {
        let offset = le32(input, 8 + index * 8)? as usize;
        let length = le32(input, 12 + index * 8)? as usize;
        if length != 0 && offset < table_end {
            return Err(Error::invalid("AFS entry overlaps its index"));
        }
        range(input, offset, length)?;
        entries.push(AfsEntry {
            index,
            offset,
            length,
        });
    }
    Ok(entries)
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AdxLoop {
    pub start_sample: u32,
    pub end_sample: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AdxHeader {
    pub encoding_type: u8,
    pub version: u16,
    pub channels: u8,
    pub sample_rate: u32,
    /// Frames of PCM per channel, excluding optional ADX end padding.
    pub sample_count: u32,
    pub highpass_cutoff: u16,
    pub data_offset: usize,
    pub loop_points: Option<AdxLoop>,
    pub initial_history: Vec<[i16; 2]>,
}

/// Parse the supported standard ADX framing. Rejected encodings remain opaque;
/// AHX shares an ADX-like header but requires a different codec.
pub fn read_adx_header(input: &[u8]) -> Result<AdxHeader> {
    if be16(input, 0)? != 0x8000 {
        return Err(Error::invalid("not a CRI ADX stream"));
    }
    range(input, 0, 20)?;
    let data_offset = usize::from(be16(input, 2)?) + 4;
    let encoding_type = input[4];
    let version = be16(input, 18)?;
    if encoding_type != 3 || !matches!(version, 0x0300 | 0x0400 | 0x0500) {
        return Err(Error::invalid(format!("unsupported ADX encoding {encoding_type:#x}/version {version:#x}; AHX/encryption remain opaque")));
    }
    if input[5] != 18 || input[6] != 4 {
        return Err(Error::invalid("unsupported ADX block size or bit depth"));
    }
    let channels = input[7];
    let sample_rate = be32(input, 8)?;
    let sample_count = be32(input, 12)?;
    let highpass_cutoff = be16(input, 16)?;
    if !(1..=8).contains(&channels)
        || !(1..=384_000).contains(&sample_rate)
        || sample_count > i32::MAX as u32
        || u32::from(highpass_cutoff) >= sample_rate / 2
    {
        return Err(Error::invalid(
            "invalid ADX channels, sample count or frequency",
        ));
    }
    if data_offset < 26 || range(input, data_offset - 6, 6)? != b"(c)CRI" {
        return Err(Error::invalid("ADX CRI marker missing"));
    }
    range(input, 0, data_offset)?;
    let mut initial_history = vec![[0, 0]; usize::from(channels)];
    let loop_base = match version {
        0x0300 => Some(20),
        0x0400 => {
            let history_size = (usize::from(channels) * 4).max(8);
            if 24 + history_size > data_offset - 6 {
                return Err(Error::invalid("ADX v4 history overlaps payload"));
            }
            for (channel, history) in initial_history.iter_mut().enumerate() {
                *history = [
                    be16(input, 24 + channel * 4)? as i16,
                    be16(input, 26 + channel * 4)? as i16,
                ];
            }
            Some(24 + history_size)
        }
        _ => None,
    };
    let mut loop_points = None;
    if let Some(base) = loop_base {
        if base + 24 <= data_offset - 6 && be32(input, base + 4)? != 0 {
            let start_sample = be32(input, base + 8)?;
            let end_sample = be32(input, base + 16)?;
            if start_sample >= end_sample || end_sample > sample_count {
                return Err(Error::invalid("ADX loop points exceed source samples"));
            }
            loop_points = Some(AdxLoop {
                start_sample,
                end_sample,
            });
        }
    }
    let blocks = (sample_count as usize).div_ceil(32);
    let encoded_length = blocks
        .checked_mul(18 * usize::from(channels))
        .ok_or_else(|| Error::invalid("ADX block count overflow"))?;
    range(input, data_offset, encoded_length)?;
    Ok(AdxHeader {
        encoding_type,
        version,
        channels,
        sample_rate,
        sample_count,
        highpass_cutoff,
        data_offset,
        loop_points,
        initial_history,
    })
}

#[derive(Debug, Clone)]
pub struct PcmAudio {
    pub header: AdxHeader,
    /// Signed 16-bit channel-interleaved PCM, source sample count exactly once.
    pub samples: Vec<i16>,
}

/// Decode once without repeating loops. `max_samples` bounds the aggregate
/// channel-interleaved allocation, rather than the smaller per-channel count.
pub fn decode_adx(input: &[u8], max_samples: usize) -> Result<PcmAudio> {
    let header = read_adx_header(input)?;
    let channels = usize::from(header.channels);
    let count = (header.sample_count as usize)
        .checked_mul(channels)
        .ok_or_else(|| Error::invalid("ADX PCM sample count overflow"))?;
    if count > max_samples {
        return Err(Error::invalid("ADX PCM sample limit exceeded"));
    }
    // ADX's high-pass predictor coefficients, quantized toward zero to Q12.
    let angle = (std::f64::consts::TAU * f64::from(header.highpass_cutoff)
        / f64::from(header.sample_rate)) as f32;
    let z = angle.cos();
    let a = (std::f64::consts::SQRT_2 - f64::from(z)) as f32;
    let b = (std::f64::consts::SQRT_2 - 1.0) as f32;
    let c = (a - ((a + b) * (a - b)).sqrt()) / b;
    let coef1 = (c * 8192.0) as i32;
    let coef2 = (c * c * -4096.0) as i32;
    let mut histories = header.initial_history.clone();
    let mut samples = vec![0; count];
    for block in 0..(header.sample_count as usize).div_ceil(32) {
        for (channel, history) in histories.iter_mut().enumerate() {
            let frame = range(
                input,
                header.data_offset + (block * channels + channel) * 18,
                18,
            )?;
            let raw_scale = i16::from_be_bytes([frame[0], frame[1]]);
            if raw_scale < 0 {
                return Err(Error::invalid(
                    "ADX end marker or invalid scale before declared sample end",
                ));
            }
            let scale = i32::from(raw_scale) + 1;
            for local in 0..32 {
                let sample_index = block * 32 + local;
                if sample_index >= header.sample_count as usize {
                    break;
                }
                let byte = frame[2 + local / 2];
                let nibble = if local % 2 == 0 { byte >> 4 } else { byte & 15 };
                let signed = if nibble >= 8 {
                    i32::from(nibble) - 16
                } else {
                    i32::from(nibble)
                };
                let h1 = i32::from(history[0]);
                let h2 = i32::from(history[1]);
                let prediction = if header.version == 0x0300 {
                    ((coef1 * h1) >> 12) + ((coef2 * h2) >> 12)
                } else {
                    (coef1 * h1 + coef2 * h2) >> 12
                };
                let decoded = (signed * scale + prediction)
                    .clamp(i32::from(i16::MIN), i32::from(i16::MAX))
                    as i16;
                samples[sample_index * channels + channel] = decoded;
                history[1] = history[0];
                history[0] = decoded;
            }
        }
    }
    Ok(PcmAudio { header, samples })
}

impl PcmAudio {
    /// Write PCM once and preserve a declared forward loop in a WAV sampler chunk.
    pub fn to_wav(&self) -> Result<Vec<u8>> {
        let expected_samples = (self.header.sample_count as usize)
            .checked_mul(usize::from(self.header.channels))
            .ok_or_else(|| Error::invalid("WAV PCM sample count overflow"))?;
        if !(1..=8).contains(&self.header.channels)
            || self.header.sample_rate == 0
            || self.samples.len() != expected_samples
        {
            return Err(Error::invalid("inconsistent WAV channel/sample metadata"));
        }
        let byte_rate = self
            .header
            .sample_rate
            .checked_mul(u32::from(self.header.channels) * 2)
            .ok_or_else(|| Error::invalid("WAV byte rate overflow"))?;
        let data_size = self
            .samples
            .len()
            .checked_mul(2)
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| Error::invalid("WAV exceeds RIFF size limit"))?;
        let sampler_size = if let Some(points) = &self.header.loop_points {
            if points.start_sample >= points.end_sample
                || points.end_sample > self.header.sample_count
            {
                return Err(Error::invalid("invalid WAV loop range"));
            }
            68
        } else {
            0
        };
        let riff_size = data_size
            .checked_add(36 + sampler_size)
            .ok_or_else(|| Error::invalid("WAV RIFF size overflow"))?;
        let mut output = Vec::with_capacity(riff_size as usize + 8);
        output.extend_from_slice(b"RIFF");
        output.extend_from_slice(&riff_size.to_le_bytes());
        output.extend_from_slice(b"WAVEfmt ");
        output.extend_from_slice(&16u32.to_le_bytes());
        output.extend_from_slice(&1u16.to_le_bytes());
        output.extend_from_slice(&u16::from(self.header.channels).to_le_bytes());
        output.extend_from_slice(&self.header.sample_rate.to_le_bytes());
        output.extend_from_slice(&byte_rate.to_le_bytes());
        output.extend_from_slice(&(u16::from(self.header.channels) * 2).to_le_bytes());
        output.extend_from_slice(&16u16.to_le_bytes());
        if let Some(points) = &self.header.loop_points {
            output.extend_from_slice(b"smpl");
            output.extend_from_slice(&60u32.to_le_bytes());
            let period_ns = 1_000_000_000 / self.header.sample_rate;
            let sampler_header = [0, 0, period_ns, 60, 0, 0, 0, 1, 0];
            // Forward loop, inclusive WAV endpoint, repeat indefinitely.
            let sampler_loop = [0, 0, points.start_sample, points.end_sample - 1, 0, 0];
            for word in sampler_header.into_iter().chain(sampler_loop) {
                output.extend_from_slice(&word.to_le_bytes());
            }
        }
        output.extend_from_slice(b"data");
        output.extend_from_slice(&data_size.to_le_bytes());
        for sample in &self.samples {
            output.extend_from_slice(&sample.to_le_bytes());
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn adx() -> Vec<u8> {
        let mut b = vec![0; 50];
        b[..4].copy_from_slice(&[0x80, 0, 0, 28]);
        b[4..8].copy_from_slice(&[3, 18, 4, 1]);
        b[8..12].copy_from_slice(&32000u32.to_be_bytes());
        b[12..16].copy_from_slice(&3u32.to_be_bytes());
        b[16..18].copy_from_slice(&500u16.to_be_bytes());
        b[18..20].copy_from_slice(&0x0300u16.to_be_bytes());
        b[26..32].copy_from_slice(b"(c)CRI");
        b[34] = 0x12;
        b[35] = 0x30;
        b
    }
    #[test]
    fn afs_bounds_empty_and_aliases() {
        let mut b = vec![0; 33];
        b[..4].copy_from_slice(b"AFS\0");
        b[4..8].copy_from_slice(&3u32.to_le_bytes());
        for at in [8, 16] {
            b[at..at + 4].copy_from_slice(&32u32.to_le_bytes());
            b[at + 4..at + 8].copy_from_slice(&1u32.to_le_bytes());
        }
        let entries = read_afs(&b).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].bytes(&b).unwrap(), &[0]);
        b[12..16].copy_from_slice(&2u32.to_le_bytes());
        assert!(read_afs(&b).is_err());
    }
    #[test]
    fn adx_decodes_high_nibble_first_and_exact_samples() {
        let pcm = decode_adx(&adx(), 3).unwrap();
        assert_eq!(pcm.samples, [1, 3, 7]);
        let wav = pcm.to_wav().unwrap();
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(wav.len(), 50);
        assert!(decode_adx(&adx(), 2).is_err());
    }
    #[test]
    fn adx_rejects_truncation_ahx_and_encryption() {
        let b = adx();
        for end in 0..b.len() {
            assert!(read_adx_header(&b[..end]).is_err());
        }
        let mut b = adx();
        b[4] = 0x11;
        assert!(read_adx_header(&b).is_err());
        b[4] = 3;
        b[18..20].copy_from_slice(&0x0408u16.to_be_bytes());
        assert!(read_adx_header(&b).is_err());
    }

    #[test]
    #[ignore = "requires SA2_INSTALL and SA2_REFERENCE_ROOT with owner-local vgmstream WAV/AHX outputs"]
    fn local_audio_parity() {
        use std::{fs, path::PathBuf};
        let game = PathBuf::from(std::env::var_os("SA2_INSTALL").expect("SA2_INSTALL"));
        let reference =
            PathBuf::from(std::env::var_os("SA2_REFERENCE_ROOT").expect("SA2_REFERENCE_ROOT"));
        fn wave_pcm(wave: &[u8]) -> &[u8] {
            assert_eq!(&wave[..4], b"RIFF");
            assert_eq!(&wave[8..12], b"WAVE");
            let mut at = 12;
            while at + 8 <= wave.len() {
                let size = u32::from_le_bytes(wave[at + 4..at + 8].try_into().unwrap()) as usize;
                if &wave[at..at + 4] == b"data" {
                    return &wave[at + 8..at + 8 + size];
                }
                at += 8 + size + size % 2;
            }
            panic!("reference WAV data chunk missing");
        }
        for (id, file) in [
            ("music-hub", "chao_hall"),
            ("music-garden", "chao_g_bgm_n"),
            ("music-hero", "chao_g_bgm_h"),
            ("music-dark", "chao_g_bgm_d"),
        ] {
            let path = game.join(format!("resource/gd_PC/ADX/{file}.adx"));
            let source = fs::read(&path).unwrap();
            let decoded = decode_adx(&source, 20_000_000).unwrap().to_wav().unwrap();
            let expected = fs::read(reference.join(format!("audio-work/{id}.wav"))).unwrap();
            assert_eq!(wave_pcm(&decoded), wave_pcm(&expected), "{id}");
            assert_eq!(source, fs::read(path).unwrap(), "source unchanged");
        }
        let path = game.join("resource/gd_PC/event_adx_e.afs");
        let source = fs::read(&path).unwrap();
        let entries = read_afs(&source).unwrap();
        for (id, index) in [
            ("voice-knuckles", 2654),
            ("voice-tails", 2662),
            ("voice-amy", 2673),
        ] {
            let expected = fs::read(reference.join(format!("audio-work/{id}.ahx"))).unwrap();
            assert_eq!(entries[index].bytes(&source).unwrap(), expected, "{id}");
        }
        assert_eq!(source, fs::read(path).unwrap(), "source unchanged");
    }
}
