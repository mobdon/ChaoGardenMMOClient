//! Verified record regions and opaque default-table inspection.
//!
//! The layout is specific to the fingerprinted A2NE multiboot garden, not the
//! main game's record or another supplied garden. Native field labels remain
//! unknown unless supported by the pinned reference.

use crate::native_save::{SAVE_RECORD_LEN, SaveRecord};
use serde::Serialize;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::fmt;

pub const DEFAULT_CHAO_TEMPLATE_OFFSET: usize = 0x2071c;
pub const DEFAULT_CHAO_TEMPLATE_LEN: usize = 0x780;
pub const INITIAL_NAME_TABLE_OFFSET: usize = 0x202b6;
pub const INITIAL_NAME_TABLE_LEN: usize = 2 * 10 * 7;
pub const GARDEN_PAYLOAD_SHA1: &str = "2c7da8e1c9af85986eecf4effae39197cdac9abf";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutError {
    UnsupportedPayload,
    InvalidNameIndex { mode: usize, row: usize },
}
impl fmt::Display for LayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for LayoutError {}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegionReport {
    pub id: &'static str,
    pub offset: usize,
    pub bytes: usize,
    pub sha256: String,
    pub status: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveLayoutReport {
    pub bytes: usize,
    pub stored_checksum: u16,
    pub save_counter: u32,
    pub raw_sha256: String,
    pub regions: Vec<RegionReport>,
    pub authority: &'static str,
}

fn region(id: &'static str, offset: usize, raw: &[u8], status: &'static str) -> RegionReport {
    RegionReport {
        id,
        offset,
        bytes: raw.len(),
        sha256: format!("{:x}", Sha256::digest(raw)),
        status,
    }
}

/// Only a checksum-validated record can produce this report. Regions cover
/// every body byte without assigning guessed stat, form or genetic meanings.
pub fn inspect_record(record: &SaveRecord<'_>) -> SaveLayoutReport {
    let raw = record.raw_bytes();
    SaveLayoutReport {
        bytes: SAVE_RECORD_LEN,
        stored_checksum: record.stored_checksum(),
        save_counter: record.save_counter(),
        raw_sha256: format!("{:x}", Sha256::digest(raw)),
        regions: vec![
            region(
                "garden-header",
                0,
                &raw[..8],
                "decoded checksum, marker and unsigned generation",
            ),
            region(
                "garden-local-state",
                8,
                &raw[8..0x58],
                "opaque; original field names mostly unknown",
            ),
            region(
                "transfer-prefix",
                0x58,
                &raw[0x58..0x6a],
                "opaque; native transfer begins here",
            ),
            region(
                "chao-prefix",
                0x6a,
                &raw[0x6a..0xd8],
                "opaque; initializer clears from this offset",
            ),
            region(
                "chao-default-block",
                0xd8,
                &raw[0xd8..],
                "opaque; initializer copies 0x780-byte native template here",
            ),
        ],
        authority: "local read-only compatibility evidence; no MMO progression authorization",
    }
}

/// A borrowed view keeps every default byte intact. No external source code
/// or native template byte is embedded in the tool or its tests.
#[derive(Debug)]
pub struct SaveTemplates<'a> {
    payload: &'a [u8],
}

impl<'a> SaveTemplates<'a> {
    /// Exact fingerprint gating prevents these offsets being applied to a
    /// different game, region, revision or arbitrary synthetic buffer.
    pub fn parse(payload: &'a [u8]) -> Result<Self, LayoutError> {
        if payload.len() != 191_340 || format!("{:x}", Sha1::digest(payload)) != GARDEN_PAYLOAD_SHA1
        {
            return Err(LayoutError::UnsupportedPayload);
        }
        Ok(Self { payload })
    }

    pub fn default_chao_bytes(&self) -> &'a [u8] {
        &self.payload
            [DEFAULT_CHAO_TEMPLATE_OFFSET..DEFAULT_CHAO_TEMPLATE_OFFSET + DEFAULT_CHAO_TEMPLATE_LEN]
    }

    /// Two native mode groups, ten seven-byte candidate names each. These
    /// bytes are native glyph IDs, not assumed ASCII/Unicode text.
    pub fn name_bytes(&self, mode: usize, row: usize) -> Result<&'a [u8], LayoutError> {
        if mode >= 2 || row >= 10 {
            return Err(LayoutError::InvalidNameIndex { mode, row });
        }
        let offset = INITIAL_NAME_TABLE_OFFSET + (mode * 10 + row) * 7;
        Ok(&self.payload[offset..offset + 7])
    }

    pub fn reports(&self) -> [RegionReport; 2] {
        [
            region(
                "initial-name-table",
                INITIAL_NAME_TABLE_OFFSET,
                &self.payload
                    [INITIAL_NAME_TABLE_OFFSET..INITIAL_NAME_TABLE_OFFSET + INITIAL_NAME_TABLE_LEN],
                "decoded 2x10x7 glyph-ID layout; text encoding unknown",
            ),
            region(
                "default-chao-template",
                DEFAULT_CHAO_TEMPLATE_OFFSET,
                self.default_chao_bytes(),
                "opaque 0x780-byte initializer copied to record+0xD8",
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_save::{SAVE_MARKER, checksum};

    #[test]
    fn record_regions_cover_all_bytes_and_preserve_unknown_type_values() {
        let mut raw = [0xa7; SAVE_RECORD_LEN];
        raw[2..4].copy_from_slice(&SAVE_MARKER.to_le_bytes());
        raw[4..8].copy_from_slice(&37_u32.to_le_bytes());
        raw[0x80] = 0x1c;
        raw[0x529] = 0xfd;
        let crc = checksum(&raw[2..]);
        raw[..2].copy_from_slice(&crc.to_le_bytes());
        let record = SaveRecord::parse(&raw).unwrap();
        let report = inspect_record(&record);
        assert_eq!(report.save_counter, 37);
        assert_eq!(
            report.regions.iter().map(|r| r.bytes).sum::<usize>(),
            SAVE_RECORD_LEN
        );
        for pair in report.regions.windows(2) {
            assert_eq!(pair[0].offset + pair[0].bytes, pair[1].offset);
        }
        assert_eq!(record.raw_bytes(), &raw);
        assert_eq!(report.regions[4].bytes, DEFAULT_CHAO_TEMPLATE_LEN);
    }

    #[test]
    fn unsupported_payload_cannot_activate_native_offsets() {
        assert!(matches!(
            SaveTemplates::parse(&[]),
            Err(LayoutError::UnsupportedPayload)
        ));
        assert!(matches!(
            SaveTemplates::parse(&vec![0; 191_340]),
            Err(LayoutError::UnsupportedPayload)
        ));
    }
}
