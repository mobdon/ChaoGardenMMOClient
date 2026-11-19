//! The 12-row base table used by the installed stage lighting loader.
use crate::{Error, Result};
pub(crate) fn words<const N: usize>(row: &[u8; N]) -> Vec<u32> {
    row.as_chunks::<4>()
        .0
        .iter()
        .map(|v| u32::from_be_bytes(*v))
        .collect()
}
pub fn derive(source: &[u8]) -> Result<Vec<[u32; 9]>> {
    if source.len() != 384 {
        return Err(Error::invalid("expected 12 complete stage lighting rows"));
    }
    let mut derived = Vec::new();
    for row in source.as_chunks::<32>().0 {
        let w = words(row);
        let scaled = |i: usize| (f32::from_bits(w[3]) * f32::from_bits(w[i])).to_bits();
        derived.push([
            w[0],
            w[1],
            w[2],
            scaled(5),
            scaled(6),
            scaled(7),
            w[4],
            w[4],
            w[4],
        ]);
    }
    Ok(derived)
}
