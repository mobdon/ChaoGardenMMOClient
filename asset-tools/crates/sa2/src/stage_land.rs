//! Static collision tables mapped in the Steam-delivered stage and Chao modules.
use crate::{archive, city_land, city_rel::CityRel, manifest, stage, Error, Result};
use serde::Serialize;
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LandSource {
    pub filename: &'static str,
    pub symbol: &'static str,
    pub mapping: &'static str,
    pub geometry_path: &'static str,
    pub file_offset: u32,
    pub mapped_offset: u32,
    pub sha256: &'static str,
    pub module: u32,
    pub size: usize,
    pub section: u8,
    pub count: u16,
    pub visual: i16,
    pub format: &'static str,
    pub supported: bool,
}
pub fn catalog() -> &'static [LandSource] {
    include!("stage_land_catalog.rs")
}
fn identify(bytes: &[u8], symbol: Option<&str>) -> Result<&'static LandSource> {
    if !catalog().iter().any(|p| p.size == bytes.len()) {
        return Err(Error::invalid("unrecognized installed land module size"));
    }
    let sha = manifest::sha256(bytes);
    catalog()
        .iter()
        .find(|p| p.sha256 == sha && symbol.is_none_or(|s| s == p.symbol))
        .ok_or_else(|| Error::invalid("unrecognized installed module or land symbol"))
}
fn decode(bytes: &[u8], row: &LandSource) -> Result<stage::Stage> {
    if !row.supported {
        return Err(Error::invalid(format!(
            "unsupported static land table {}",
            row.symbol
        )));
    }
    let unpacked;
    let bytes = if row.filename.ends_with(".prs") {
        unpacked = archive::decompress_prs(bytes, 16 * 1024 * 1024)?;
        &unpacked[..]
    } else {
        bytes
    };
    let rel = CityRel::read_identified(bytes, row.module)?;
    let start = rel
        .sections()
        .get(row.section as usize)
        .ok_or_else(|| Error::invalid("stage land section missing"))?
        .file_offset;
    let h = row
        .file_offset
        .checked_sub(start)
        .ok_or_else(|| Error::invalid("land header outside section"))?;
    city_land::read_rel_table(
        &rel,
        row.section,
        h as usize,
        row.format == "SA2B",
        row.count,
        row.visual,
    )
}
/// Read the first landtable listed in the source's original mapping.
pub fn read(bytes: &[u8]) -> Result<stage::Stage> {
    decode(bytes, identify(bytes, None)?)
}
pub fn read_named(bytes: &[u8], symbol: &str) -> Result<stage::Stage> {
    decode(bytes, identify(bytes, Some(symbol))?)
}
