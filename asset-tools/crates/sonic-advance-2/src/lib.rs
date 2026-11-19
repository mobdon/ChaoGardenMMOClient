//! A2NE asset readers and standalone cartridge Garden practice.
#![forbid(unsafe_code)]

pub mod catalog;
#[path = "reused/gba.rs"]
pub mod gba;
pub mod graphics;
pub mod native_garden_art;
pub mod native_garden_background;
pub mod native_garden_behavior;
pub mod native_garden_home_art;
pub mod native_garden_memory;
pub mod native_garden_progress;
pub mod native_garden_second;
pub mod native_garden_session;
#[path = "reused/native_growth.rs"]
pub mod native_growth;
#[path = "reused/native_hatching.rs"]
pub mod native_hatching;
#[path = "reused/native_save.rs"]
pub mod native_save;
#[path = "reused/native_shop_state.rs"]
pub mod native_shop_state;
#[path = "reused/native_weeds.rs"]
pub mod native_weeds;
pub mod save_layout;
#[path = "reused/shop.rs"]
pub mod shop;
pub mod tables;
pub mod transfer;

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}

pub const ROM_LEN: usize = 0x1000000;
pub const ROM_SHA256: &str = "f2b7a3329401a02706d87560f61da3e6da2ca42f2b9ae98a86d78e0529cb7260";
pub const GARDEN_SHA256: &str = "84d4df3f397e25885768d212a46287db706857651102b9145595cfda30cee7ca";
pub const REFERENCE_COMMIT: &str = "2dd3f17d30cd3c156e508aac0017620ae485b9ba";

/// Only this exact cartridge is permitted to use the catalog's offsets.
pub struct VerifiedRom<'a> {
    pub(crate) rom: &'a [u8],
    pub(crate) garden: Vec<u8>,
    pub(crate) compressed_len: usize,
}
impl<'a> VerifiedRom<'a> {
    pub fn parse(rom: &'a [u8]) -> Result<Self, Error> {
        if rom.len() != ROM_LEN || gba::sha256(rom) != ROM_SHA256 {
            return Err(Error(
                "Expected verified A2NE revision 0 ROM (size/SHA-256 mismatch)".into(),
            ));
        }
        let header = gba::GbaHeader::parse(rom)
            .map_err(|e| Error(e.to_string()))?
            .report();
        if header.game_code != "A2NE"
            || header.revision != 0
            || !header.checksum_valid
            || !header.fixed_value_valid
        {
            return Err(Error("A2NE header mismatch".into()));
        }
        let payload = gba::extract_reference_garden(rom).map_err(|e| Error(e.to_string()))?;
        if payload.consumed != 155_273 || gba::sha256(&payload.bytes) != GARDEN_SHA256 {
            return Err(Error("Embedded Tiny Garden fingerprint mismatch".into()));
        }
        native_save::verify_reference_crc_table(&payload.bytes)
            .map_err(|e| Error(e.to_string()))?;
        Ok(Self {
            rom,
            garden: payload.bytes,
            compressed_len: payload.consumed,
        })
    }

    pub fn rom_bytes(&self) -> &[u8] {
        self.rom
    }
    pub fn garden_bytes(&self) -> &[u8] {
        &self.garden
    }

    pub fn source_bytes(&self, source: &str) -> Result<&[u8], Error> {
        match source {
            "rom" => Ok(self.rom),
            "garden" => Ok(&self.garden),
            _ => Err(Error(format!("Unknown address space: {source}"))),
        }
    }

    pub fn range(&self, source: &str, offset: usize, length: usize) -> Result<&[u8], Error> {
        bounded_range(self.source_bytes(source)?, offset, length)
    }
}

pub fn bounded_range(bytes: &[u8], offset: usize, length: usize) -> Result<&[u8], Error> {
    let end = offset
        .checked_add(length)
        .ok_or_else(|| Error("Range overflow".into()))?;
    bytes.get(offset..end).ok_or_else(|| {
        Error(format!(
            "Out of bounds range {offset:#x}+{length:#x} in {:#x} bytes",
            bytes.len()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_wrong_sources_and_overflow() {
        assert!(VerifiedRom::parse(&[0; 192]).is_err());
        assert!(bounded_range(&[0; 4], usize::MAX, 2).is_err());
        assert!(bounded_range(&[0; 4], 2, 3).is_err());
        assert!(bounded_range(&[1, 2], 2, 0).unwrap().is_empty());
    }
}
