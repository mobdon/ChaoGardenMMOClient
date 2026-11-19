//! Installed SA2 stage fog files. Preserve float bit patterns, including the
//! halfword-swapped samples of the long form.
use crate::{Error, Result};
use serde_json::{json, Value};
pub fn read(data: &[u8]) -> Result<Value> {
    if !matches!(data.len(), 16 | 528) {
        return Err(Error::invalid("expected 16- or 528-byte stage fog file"));
    }
    let word = |at: usize| u32::from_be_bytes(data[at..at + 4].try_into().unwrap());
    let table = (data.len() == 528).then(|| {
        data[16..]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|v| u32::from_be_bytes(*v).rotate_left(16))
            .collect::<Vec<_>>()
    });
    Ok(
        json!({"type":i16::from_be_bytes([data[0],data[1]]),"unknown":i16::from_be_bytes([data[2],data[3]]),
        "argb":&data[4..8],"maxDistanceBits":word(8),"minDistanceBits":word(12),"tableBits":table}),
    )
}
