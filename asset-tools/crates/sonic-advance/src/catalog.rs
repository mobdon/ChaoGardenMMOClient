//! Hash-only, version-specific catalog. No embedded art, audio, ROM or save bytes.
use crate::{binary, Error, Result};
use serde::{Deserialize, Serialize};

pub const GARDEN_OFFSET: usize = 0x9be78;
pub const GARDEN_COMPRESSED_LEN: usize = 114900;
pub const GARDEN_LEN: usize = 138752;
pub const GARDEN_SHA256: &str = "989acfa4a2522cb78d4dc55cb75232528ebc8b3f5852afdb28410d276339b9d0";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Record {
    pub id: String,
    pub domain: String,
    pub address_space: String,
    pub offset: usize,
    pub length: usize,
    pub sha256: String,
    pub status: String,
    pub evidence: String,
    pub codec: String,
    pub decoded_length: Option<usize>,
    pub decoded_sha256: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedCatalog {
    pub schema_version: u32,
    pub schema_status: &'static str,
    pub game: &'static str,
    pub source_sha256: &'static str,
    pub header: binary::Header,
    pub records: Vec<Record>,
    pub remaining_unknowns: Vec<String>,
}

pub fn records() -> Result<Vec<Record>> {
    serde_json::from_str(include_str!("../catalog.json")).map_err(|e| Error(e.to_string()))
}

pub fn garden(rom: &[u8]) -> Result<binary::Decoded> {
    binary::verify_asoe(rom)?;
    let source = binary::range(rom, GARDEN_OFFSET, GARDEN_COMPRESSED_LEN)?;
    let decoded = binary::decompress(source, GARDEN_LEN)?;
    if decoded.consumed != GARDEN_COMPRESSED_LEN
        || decoded.bytes.len() != GARDEN_LEN
        || binary::sha256(&decoded.bytes) != GARDEN_SHA256
    {
        return Err(Error("ASOE embedded garden fingerprint mismatch".into()));
    }
    Ok(decoded)
}

pub fn verify(rom: &[u8]) -> Result<VerifiedCatalog> {
    let header = binary::verify_asoe(rom)?;
    let garden = garden(rom)?;
    let records = records()?;
    for record in &records {
        let source = match record.address_space.as_str() {
            "rom" => rom,
            "garden-decoded" => &garden.bytes,
            _ => return Err(Error(format!("unknown address space for {}", record.id))),
        };
        let span = binary::range(source, record.offset, record.length)?;
        if binary::sha256(span) != record.sha256 {
            return Err(Error(format!(
                "source span hash mismatch for {}",
                record.id
            )));
        }
        match record.codec.as_str() {
            "raw" => {
                if record.decoded_length.is_some() || record.decoded_sha256.is_some() {
                    return Err(Error(format!(
                        "raw record with decoded metadata: {}",
                        record.id
                    )));
                }
            }
            "lz77" | "rle" => {
                let expected_type = if record.codec == "lz77" { 0x10 } else { 0x30 };
                if span.first() != Some(&expected_type) {
                    return Err(Error(format!("codec mismatch: {}", record.id)));
                }
                let length = record
                    .decoded_length
                    .ok_or_else(|| Error("missing decoded length".into()))?;
                let decoded = binary::decompress(span, length)?;
                if decoded.consumed != record.length
                    || decoded.bytes.len() != length
                    || Some(binary::sha256(&decoded.bytes)) != record.decoded_sha256
                {
                    return Err(Error(format!("decoded parity mismatch: {}", record.id)));
                }
            }
            _ => return Err(Error(format!("unknown codec for {}", record.id))),
        }
    }
    Ok(VerifiedCatalog {
        schema_version: 1,
        schema_status: "proposed-sonic-advance-only",
        game: "sonic-advance.asoe.r0",
        source_sha256: binary::ROM_SHA256,
        header,
        records,
        remaining_unknowns: unknowns(),
    })
}

pub fn unknowns() -> Vec<String> {
    [
        "Full runtime reachability and unused-resource classification; structural candidates alone do not establish use.",
        "Complete sprite/animation/OAM type-to-art lookup, especially Chaos types and fallback behavior.",
        "Exact UI/font glyph/text encoding and resource-to-screen assignment.",
        "Complete audio voice/sample/sequence graph and playback parity.",
        "ASOE native save sector layout, generation/checksum spans, field meanings and real-save roundtrip.",
        "GameCube JoyBus payload/handshake, success/failure/save ordering and transfer field meanings.",
        "Native minigame rules/physics/rewards and complete care/shop/item behavior.",
        "Main game mechanics/stage/collision behavior and other-region/revision layouts.",
    ].into_iter().map(String::from).collect()
}
