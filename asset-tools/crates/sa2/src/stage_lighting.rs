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

/// Resolve the initialized words of the stage 20 loader's destination table.
/// A missing override leaves the last three words of rows 1..11 as prior memory.
pub fn resolve(source: Option<&[u8]>, gc: Option<&[u8]>) -> Result<serde_json::Value> {
    let derived = source.map(derive).transpose()?;
    let rows = match gc {
        Some(data) if data.len() == 576 => data
            .as_chunks::<48>()
            .0
            .iter()
            .map(|row| words(row).into_iter().map(Some).collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        Some(_) => return Err(Error::invalid("expected 12 complete GC lighting rows")),
        None => derived
            .ok_or_else(|| Error::invalid("lighting needs a base or override"))?
            .into_iter()
            .enumerate()
            .map(|(i, row)| {
                let mut result = row.into_iter().map(Some).collect::<Vec<_>>();
                result.extend([if i == 0 { Some(0) } else { None }; 3]);
                result
            })
            .collect(),
    };
    Ok(serde_json::json!({"overrideLoaded":gc.is_some(),"activeWords":rows}))
}
