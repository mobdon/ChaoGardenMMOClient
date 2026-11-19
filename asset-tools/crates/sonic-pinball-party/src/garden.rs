//! A3VE's embedded garden checksum and opaque flash records.
//!
//! Only the marker, generation, checksum, record length and sector selection
//! have been traced. Chao fields and transfer packets remain uninterpreted.
use crate::{binary, sha256};
use std::fmt;

pub const PAYLOAD_SHA256: &str = "33dcbab013c9007ad838d8cbff693eef140768ca4110c9617c4055aa75659efb";
pub const PAYLOAD_LENGTH: usize = 0x2e12c;
pub const CRC_TABLE_OFFSET: usize = 0x1f4e0;
pub const SAVE_RECORD_LENGTH: usize = 0x858;
pub const SAVE_MARKER: u16 = 0x1965;
pub const FLASH_LENGTH: usize = 0x10000;
pub const SECTOR_LENGTH: usize = 0x1000;
pub const FIRST_GARDEN_SECTOR: usize = 10;
pub const GARDEN_SECTOR_COUNT: usize = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Length { actual: usize, expected: usize },
    PayloadIdentity,
    Layout(usize),
    Marker(u16),
    Checksum { stored: u16, calculated: u16 },
    NoValidSector,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

fn word(bytes: &[u8], offset: usize) -> Result<u32, Error> {
    binary::u32_le(bytes, offset).map_err(|_| Error::Layout(offset))
}

/// Verify this version before interpreting its embedded-garden layout.
/// Offsets are relative to the decoded payload, not to the cartridge ROM.
pub fn verify_payload_layout(payload: &[u8]) -> Result<(), Error> {
    if payload.len() != PAYLOAD_LENGTH || sha256(payload) != PAYLOAD_SHA256 {
        return Err(Error::PayloadIdentity);
    }
    // A3VE LoadGameState's literal pool and CRC routine's table pointer.
    for (offset, expected) in [
        (0x2b0, u32::from(SAVE_MARKER)),
        (0x2b4, SAVE_RECORD_LENGTH as u32),
        (0x2b8, (SAVE_RECORD_LENGTH - 2) as u32),
        (0x189c, 0x02000000 + CRC_TABLE_OFFSET as u32),
    ] {
        if word(payload, offset)? != expected {
            return Err(Error::Layout(offset));
        }
    }
    for index in 0..256 {
        let offset = CRC_TABLE_OFFSET + index * 2;
        let stored = binary::u16_le(payload, offset).map_err(|_| Error::Layout(offset))?;
        if stored != crc_table_entry(index as u16) {
            return Err(Error::Layout(offset));
        }
    }
    Ok(())
}

fn crc_table_entry(mut value: u16) -> u16 {
    for _ in 0..8 {
        value = if value & 1 == 0 {
            value >> 1
        } else {
            (value >> 1) ^ 0x8408
        };
    }
    value
}

/// Reflected 0x8408 polynomial, initial 0xffff, final inversion. The native
/// routine at payload 0x185c..0x189c and all 256 table words are verified.
pub fn crc16_x25(bytes: &[u8]) -> u16 {
    let mut crc = 0xffff;
    for byte in bytes {
        crc = crc_table_entry((crc ^ u16::from(*byte)) & 0xff) ^ (crc >> 8);
    }
    !crc
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Record<'a> {
    pub stored_checksum: u16,
    pub generation: u32,
    /// The whole validated record. No unknown bytes are discarded or rewritten.
    pub bytes: &'a [u8],
}

/// Read an exact native record without assigning meanings to its opaque body.
/// A valid native checksum grants no authority to import MMO progression.
pub fn read_record(bytes: &[u8]) -> Result<Record<'_>, Error> {
    if bytes.len() != SAVE_RECORD_LENGTH {
        return Err(Error::Length {
            actual: bytes.len(),
            expected: SAVE_RECORD_LENGTH,
        });
    }
    let marker = binary::u16_le(bytes, 2).map_err(|_| Error::Layout(2))?;
    if marker != SAVE_MARKER {
        return Err(Error::Marker(marker));
    }
    let stored = binary::u16_le(bytes, 0).map_err(|_| Error::Layout(0))?;
    let calculated = crc16_x25(&bytes[2..]);
    if stored != calculated {
        return Err(Error::Checksum { stored, calculated });
    }
    Ok(Record {
        stored_checksum: stored,
        generation: word(bytes, 4)?,
        bytes,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectedRecord<'a> {
    /// Physical flash sector, 10 through 15 inclusive.
    pub sector: usize,
    pub record: Record<'a>,
}

/// A3VE scans six 4 KiB sectors beginning at sector 10 and chooses the greatest
/// unsigned generation among valid records. Equal generations retain the first
/// sector; generation wraparound is not reinterpreted as a newer timestamp.
pub fn select_flash_record(flash: &[u8]) -> Result<SelectedRecord<'_>, Error> {
    if flash.len() != FLASH_LENGTH {
        return Err(Error::Length {
            actual: flash.len(),
            expected: FLASH_LENGTH,
        });
    }
    let mut selected: Option<SelectedRecord<'_>> = None;
    for sector in FIRST_GARDEN_SECTOR..FIRST_GARDEN_SECTOR + GARDEN_SECTOR_COUNT {
        let start = sector * SECTOR_LENGTH;
        if let Ok(record) = read_record(&flash[start..start + SAVE_RECORD_LENGTH]) {
            if selected.is_none_or(|prior| record.generation > prior.record.generation) {
                selected = Some(SelectedRecord { sector, record });
            }
        }
    }
    selected.ok_or(Error::NoValidSector)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_record(generation: u32) -> Vec<u8> {
        let mut bytes: Vec<_> = (0..SAVE_RECORD_LENGTH).map(|i| (i % 251) as u8).collect();
        bytes[2..4].copy_from_slice(&SAVE_MARKER.to_le_bytes());
        bytes[4..8].copy_from_slice(&generation.to_le_bytes());
        let checksum = crc16_x25(&bytes[2..]);
        bytes[..2].copy_from_slice(&checksum.to_le_bytes());
        bytes
    }

    #[test]
    fn standard_crc_check_and_empty() {
        assert_eq!(crc16_x25(b"123456789"), 0x906e);
        assert_eq!(crc16_x25(b""), 0);
    }

    #[test]
    fn opaque_bytes_survive_and_corruption_is_rejected() {
        let bytes = synthetic_record(0xfedcba98);
        let record = read_record(&bytes).unwrap();
        assert_eq!(record.generation, 0xfedcba98);
        assert_eq!(record.bytes, bytes);
        let mut corrupt = bytes.clone();
        corrupt[0x857] ^= 0x80;
        assert!(matches!(read_record(&corrupt), Err(Error::Checksum { .. })));
        corrupt[2] ^= 1;
        assert!(matches!(read_record(&corrupt), Err(Error::Marker(_))));
        assert!(matches!(
            read_record(&bytes[..0x857]),
            Err(Error::Length { .. })
        ));
        let mut oversized = bytes;
        oversized.push(0);
        assert!(matches!(read_record(&oversized), Err(Error::Length { .. })));
    }

    #[test]
    fn bounded_flash_selection_preserves_first_tie_and_rejects_bad_crc() {
        let mut flash = vec![0xff; FLASH_LENGTH];
        assert_eq!(select_flash_record(&flash), Err(Error::NoValidSector));
        for (sector, generation) in [(10, 4), (11, 9), (12, 9), (13, u32::MAX)] {
            let start = sector * SECTOR_LENGTH;
            flash[start..start + SAVE_RECORD_LENGTH].copy_from_slice(&synthetic_record(generation));
        }
        let selected = select_flash_record(&flash).unwrap();
        assert_eq!(
            (selected.sector, selected.record.generation),
            (13, u32::MAX)
        );
        flash[13 * SECTOR_LENGTH + 0x857] ^= 1;
        let selected = select_flash_record(&flash).unwrap();
        assert_eq!((selected.sector, selected.record.generation), (11, 9));
        assert!(matches!(
            select_flash_record(&flash[..FLASH_LENGTH - 1]),
            Err(Error::Length { .. })
        ));
        flash.push(0);
        assert!(matches!(
            select_flash_record(&flash),
            Err(Error::Length { .. })
        ));
    }

    #[test]
    fn first_valid_zero_generation_is_selected() {
        let mut flash = vec![0xff; FLASH_LENGTH];
        let start = 12 * SECTOR_LENGTH;
        flash[start..start + SAVE_RECORD_LENGTH].copy_from_slice(&synthetic_record(0));
        let selected = select_flash_record(&flash).unwrap();
        assert_eq!((selected.sector, selected.record.generation), (12, 0));
    }

    #[test]
    fn wrong_payload_identity_fails_closed() {
        assert_eq!(verify_payload_layout(&[]), Err(Error::PayloadIdentity));
        assert_eq!(
            verify_payload_layout(&vec![0; PAYLOAD_LENGTH]),
            Err(Error::PayloadIdentity)
        );
    }
}
