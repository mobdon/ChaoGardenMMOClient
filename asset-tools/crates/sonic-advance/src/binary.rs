//! Checked raw-cartridge reads and BIOS type-0x10/type-0x30 decompression.
use crate::{Error, Result};
use sha1::{Digest, Sha1};
use sha2::Sha256;
use std::io::Read;
use std::path::Path;

pub const ROM_LEN: usize = 0x800000;
pub const ROM_SHA1: &str = "d842afa7dd1e84de08addb94a51506f1bcafd551";
pub const ROM_SHA256: &str = "afc29e3325baf68a1822e88b25fcb59c8bdc29b8cdc48fa21c56defbd60ca020";
pub const MAX_DECODED: usize = 32 * 1024 * 1024;

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn sha1(bytes: &[u8]) -> String {
    format!("{:x}", Sha1::digest(bytes))
}

pub fn range(input: &[u8], offset: usize, length: usize) -> Result<&[u8]> {
    let end = offset
        .checked_add(length)
        .ok_or_else(|| Error("range overflow".into()))?;
    input
        .get(offset..end)
        .ok_or_else(|| Error(format!("truncated range {offset:#x}..{end:#x}")))
}
pub fn u16_at(input: &[u8], offset: usize) -> Result<u16> {
    let b = range(input, offset, 2)?;
    Ok(u16::from_le_bytes([b[0], b[1]]))
}
pub fn u32_at(input: &[u8], offset: usize) -> Result<u32> {
    let b = range(input, offset, 4)?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// Only the canonical 0x08000000 ROM window is accepted. No pointer masking,
/// RAM dereference, or unverified mirror normalization takes place.
pub fn rom_offset(pointer: u32, rom_length: usize, alignment: usize) -> Result<usize> {
    if alignment == 0 || !alignment.is_power_of_two() {
        return Err(Error("invalid alignment".into()));
    }
    let offset = pointer
        .checked_sub(0x08000000)
        .ok_or_else(|| Error("not a ROM pointer".into()))? as usize;
    if offset >= rom_length || offset >= 0x2000000 || !offset.is_multiple_of(alignment) {
        return Err(Error(format!(
            "out-of-range or unaligned ROM pointer {pointer:#x}"
        )));
    }
    Ok(offset)
}

/// A bounded read even when metadata is stale or a special file is supplied.
pub fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let cap = limit
        .checked_add(1)
        .ok_or_else(|| Error("input cap overflow".into()))?;
    let mut input = std::fs::File::open(path)
        .map_err(|e| Error(e.to_string()))?
        .take(cap as u64);
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let n = input.read(&mut chunk).map_err(|e| Error(e.to_string()))?;
        if n == 0 {
            break;
        }
        if n > limit.saturating_sub(bytes.len()) {
            return Err(Error("input limit exceeded".into()));
        }
        bytes
            .try_reserve(n)
            .map_err(|_| Error("input allocation failed".into()))?;
        bytes.extend_from_slice(&chunk[..n]);
    }
    Ok(bytes)
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Header {
    pub title: String,
    pub game_code: String,
    pub maker_code: String,
    pub revision: u8,
    pub fixed_value_valid: bool,
    pub checksum_valid: bool,
}
pub fn header(input: &[u8]) -> Result<Header> {
    let h = range(input, 0, 0xc0)?;
    let complement = h[0xa0..0xbd]
        .iter()
        .fold(0_u8, |v, b| v.wrapping_sub(*b))
        .wrapping_sub(0x19);
    let text = |b: &[u8]| {
        String::from_utf8_lossy(b)
            .trim_end_matches(['\0', ' '])
            .to_owned()
    };
    Ok(Header {
        title: text(&h[0xa0..0xac]),
        game_code: text(&h[0xac..0xb0]),
        maker_code: text(&h[0xb0..0xb2]),
        revision: h[0xbc],
        fixed_value_valid: h[0xb2] == 0x96,
        checksum_valid: h[0xbd] == complement,
    })
}
pub fn verify_asoe(input: &[u8]) -> Result<Header> {
    if input.len() != ROM_LEN {
        return Err(Error("expected 8 MiB ASOE ROM".into()));
    }
    let h = header(input)?;
    if h.title != "SONICADVANCE"
        || h.game_code != "ASOE"
        || h.maker_code != "78"
        || h.revision != 0
        || !h.fixed_value_valid
        || !h.checksum_valid
        || sha256(input) != ROM_SHA256
    {
        return Err(Error(
            "unsupported source: exact ASOE revision-0 fingerprint required".into(),
        ));
    }
    Ok(h)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    pub bytes: Vec<u8>,
    pub consumed: usize,
}

/// Strict WRAM byte-output interpretation. A malformed final token that
/// overruns the declared length is rejected, rather than emulating corruption.
pub fn decompress(input: &[u8], max_output: usize) -> Result<Decoded> {
    let h = range(input, 0, 4)?;
    if h[0] != 0x10 && h[0] != 0x30 {
        return Err(Error("unsupported compression type".into()));
    }
    let length = usize::from(h[1]) | usize::from(h[2]) << 8 | usize::from(h[3]) << 16;
    if length == 0 || length > max_output || length > MAX_DECODED {
        return Err(Error("decoded output limit exceeded or zero length".into()));
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|_| Error("output allocation failed".into()))?;
    let mut cursor = 4;
    while bytes.len() < length {
        let control = range(input, cursor, 1)?[0];
        cursor += 1;
        if h[0] == 0x30 {
            let count = usize::from(control & 0x7f) + if control & 0x80 != 0 { 3 } else { 1 };
            if count > length - bytes.len() {
                return Err(Error("RLE token exceeds declared output".into()));
            }
            if control & 0x80 != 0 {
                let value = range(input, cursor, 1)?[0];
                cursor += 1;
                bytes.resize(bytes.len() + count, value);
            } else {
                bytes.extend_from_slice(range(input, cursor, count)?);
                cursor += count;
            }
        } else {
            for bit in (0..8).rev() {
                if bytes.len() == length {
                    break;
                }
                if control & (1 << bit) == 0 {
                    bytes.push(range(input, cursor, 1)?[0]);
                    cursor += 1;
                } else {
                    let pair = range(input, cursor, 2)?;
                    cursor += 2;
                    let count = usize::from(pair[0] >> 4) + 3;
                    let distance = ((usize::from(pair[0]) & 15) << 8 | usize::from(pair[1])) + 1;
                    if distance > bytes.len() || count > length - bytes.len() {
                        return Err(Error("invalid LZ77 back-reference".into()));
                    }
                    for _ in 0..count {
                        bytes.push(bytes[bytes.len() - distance]);
                    }
                }
            }
        }
    }
    Ok(Decoded {
        bytes,
        consumed: cursor,
    })
}

/// A caller-specified, bounded table. Unknown pointer targets remain offsets;
/// this does not infer a table terminator or asset sizes from adjacent pointers.
pub fn pointer_table(
    input: &[u8],
    offset: usize,
    count: usize,
    max_entries: usize,
) -> Result<Vec<usize>> {
    if count > max_entries || count > 65536 {
        return Err(Error("pointer table entry limit exceeded".into()));
    }
    let raw = range(
        input,
        offset,
        count
            .checked_mul(4)
            .ok_or_else(|| Error("table overflow".into()))?,
    )?;
    raw.as_chunks::<4>()
        .0
        .iter()
        .map(|b| rom_offset(u32::from_le_bytes(*b), input.len(), 1))
        .collect()
}

/// Explicit nullable variant for native sparse tables. A missing entry stays
/// None; it is not redirected to ROM zero or labeled unused/playable.
pub fn nullable_pointer_table(
    input: &[u8],
    offset: usize,
    count: usize,
    max_entries: usize,
) -> Result<Vec<Option<usize>>> {
    if count > max_entries || count > 65536 {
        return Err(Error("pointer table entry limit exceeded".into()));
    }
    let raw = range(
        input,
        offset,
        count
            .checked_mul(4)
            .ok_or_else(|| Error("table overflow".into()))?,
    )?;
    raw.as_chunks::<4>()
        .0
        .iter()
        .map(|b| {
            let pointer = u32::from_le_bytes(*b);
            if pointer == 0 {
                Ok(None)
            } else {
                rom_offset(pointer, input.len(), 1).map(Some)
            }
        })
        .collect()
}
