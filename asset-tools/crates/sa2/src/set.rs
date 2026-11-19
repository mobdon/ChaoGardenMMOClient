//! Numbered SA2 PC stage SET files. Object IDs index the current stage's object list.
use crate::{Error, Result};
use serde::Serialize;

/// Defensive limit; the largest installed numbered PC set contains 944 records.
pub const MAX_PLACEMENTS: usize = 65_536;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Placement {
    pub object_id: u16,
    pub clip_level: u8,
    /// Unsigned source words. Individual objects may use them as parameters.
    pub rotation: [u16; 3],
    pub position: [f32; 3],
    /// The native Scale vector; interpretation depends on the object's update function.
    pub parameters: [f32; 3],
}

/// Parses the installed PC stage format: a big-endian count in a 32-byte header,
/// followed by exactly that many big-endian 32-byte entries in file order.
pub fn parse_set(bytes: &[u8]) -> Result<Vec<Placement>> {
    let (header, body) = bytes
        .split_at_checked(32)
        .ok_or_else(|| Error::invalid("truncated stage SET header"))?;
    let count = u32::from_be_bytes(header[..4].try_into().unwrap());
    if !body.len().is_multiple_of(32) || body.len() / 32 != count as usize {
        return Err(Error::invalid("stage SET count does not match file length"));
    }
    if count as usize > MAX_PLACEMENTS {
        return Err(Error::invalid("stage SET exceeds placement limit"));
    }
    body.as_chunks::<32>()
        .0
        .iter()
        .enumerate()
        .map(|(index, record)| {
            let word = |i| u16::from_be_bytes([record[i], record[i + 1]]);
            let scalar = |i: usize| f32::from_be_bytes(record[i..i + 4].try_into().unwrap());
            let packed = word(0);
            let position = [scalar(8), scalar(12), scalar(16)];
            let parameters = [scalar(20), scalar(24), scalar(28)];
            if position.iter().chain(&parameters).any(|v| !v.is_finite()) {
                return Err(Error::invalid(format!(
                    "nonfinite value in stage SET placement {index}"
                )));
            }
            Ok(Placement {
                object_id: packed & 0x0fff,
                clip_level: (packed >> 12) as u8,
                rotation: [word(2), word(4), word(6)],
                position,
                parameters,
            })
        })
        .collect()
}
