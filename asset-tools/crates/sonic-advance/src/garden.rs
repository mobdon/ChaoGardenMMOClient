//! Verified ASOE garden evidence without inferred native save semantics.
//!
//! The polynomial table is established byte for byte. Its presence does not
//! establish the ASOE save span, initialization, final inversion, sector layout,
//! or transfer protocol. The separately named X-25 routine is a mathematical
//! reference helper, not an ASOE save validator.

use crate::{binary, catalog, Error, Result};

pub const CRC_TABLE_OFFSET: usize = 0x18374;
pub const CRC_TABLE_LEN: usize = 512;
pub const CRC_TABLE_SHA256: &str =
    "638dbc5f735cced09b3d0d767a72a8800baeb1b3bb399a514af61b3ea7be8a01";
pub const REFLECTED_POLYNOMIAL: u16 = 0x8408;

fn table_entry(mut value: u16) -> u16 {
    for _ in 0..8 {
        value = if value & 1 != 0 {
            (value >> 1) ^ REFLECTED_POLYNOMIAL
        } else {
            value >> 1
        };
    }
    value
}

/// Validate all 256 little-endian words, with no allocation or input mutation.
/// This checks table structure only; use `GardenEvidence::parse` to require the
/// exact ASOE embedded program before assigning its version-specific offsets.
pub fn verify_crc_table(payload: &[u8]) -> Result<()> {
    let table = binary::range(payload, CRC_TABLE_OFFSET, CRC_TABLE_LEN)?;
    for (index, pair) in table.as_chunks::<2>().0.iter().enumerate() {
        let actual = u16::from_le_bytes([pair[0], pair[1]]);
        if actual != table_entry(index as u16) {
            return Err(Error(format!("garden CRC table differs at word {index}")));
        }
    }
    Ok(())
}

/// Standard CRC-16/X-25 reference calculation: polynomial 0x1021 reflected as
/// 0x8408, initial value 0xffff, and final complement. These seed/finalization
/// parameters remain unverified for ASOE's native save reader.
pub fn crc16_x25_reference(bytes: &[u8]) -> u16 {
    let mut value = 0xffff;
    for byte in bytes {
        value = table_entry((value ^ u16::from(*byte)) & 0xff) ^ (value >> 8);
    }
    !value
}

/// Borrow the exact garden program, preserving every unresolved byte.
#[derive(Debug)]
pub struct GardenEvidence<'a> {
    raw: &'a [u8],
}

impl<'a> GardenEvidence<'a> {
    pub fn parse(payload: &'a [u8]) -> Result<Self> {
        if payload.len() != catalog::GARDEN_LEN || binary::sha256(payload) != catalog::GARDEN_SHA256
        {
            return Err(Error("unsupported garden program fingerprint".into()));
        }
        verify_crc_table(payload)?;
        Ok(Self { raw: payload })
    }

    pub fn opaque_program(&self) -> &'a [u8] {
        self.raw
    }

    pub fn crc_table(&self) -> &'a [u8] {
        &self.raw[CRC_TABLE_OFFSET..CRC_TABLE_OFFSET + CRC_TABLE_LEN]
    }
}

/// Prepare the decompressed EWRAM program immediately before the retail soft reset.
/// The three stores are ASOE ROM offsets 0xe08e, 0xe094 and 0xe0a4.
pub fn prepare_launch(rom: &[u8], loaded_save: &[u8], frame: u32) -> Result<Vec<u8>> {
    let word = |offset| -> Result<u32> {
        let bytes = binary::range(loaded_save, offset, 4)?;
        Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
    };
    let score = word(0x428)?;
    let checksum = word(0x42c)?;
    let language = u32::from(loaded_save[0x19]);
    let mut program = catalog::garden(rom)?.bytes;
    for (slot, value) in
        program[8..20]
            .chunks_exact_mut(4)
            .zip([score, language, frame.wrapping_add(checksum)])
    {
        slot.copy_from_slice(&value.to_le_bytes());
    }
    Ok(program)
}
