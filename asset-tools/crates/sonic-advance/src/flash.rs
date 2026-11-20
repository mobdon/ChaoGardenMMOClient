//! Read-only inspection of raw 64 KiB Sonic Advance US flash images.
//! The game and its embedded Tiny Chao Garden use separate record formats.
use crate::{Error, Result};
use serde::Serialize;

pub const SECTOR_BYTES: usize = 0x1000;
pub const FLASH_BYTES: usize = 16 * SECTOR_BYTES;
const GAME_CHECKSUM: usize = 0x42c;
const TINY_BYTES: usize = 0x858;
const GAME_MAGIC: u32 = 0x4f524950;
const TINY_MAGIC: u16 = 0x0817;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Validity {
    Erased,
    WrongMarker,
    BadChecksum,
    Valid,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Slot {
    pub sector: usize,
    pub validity: Validity,
    /// Present only for a validated Tiny record. Game selection uses scan order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation: Option<u32>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlashInspection {
    pub game_slots: Vec<Slot>,
    pub selected_game_sector: Option<usize>,
    /// Prospective destinations of a *changed* native game save.
    pub next_game_sector: usize,
    pub game_erase_ahead_sector: usize,
    pub tiny_slots: Vec<Slot>,
    pub selected_tiny_sector: Option<usize>,
    pub next_tiny_sector: usize,
    pub next_tiny_generation: u32,
}
fn word(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}
fn half(bytes: &[u8]) -> u16 {
    u16::from_le_bytes([bytes[0], bytes[1]])
}

/// Complemented reflected CRC-16 used by the embedded ASOE garden.
pub fn tiny_checksum(data: &[u8]) -> u16 {
    let mut crc = u16::MAX;
    for &b in data {
        crc ^= u16::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0x8408
            } else {
                crc >> 1
            };
        }
    }
    !crc
}
fn game_status(record: &[u8]) -> Validity {
    if word(record) != GAME_MAGIC {
        return missing_marker(record);
    }
    let sum = record[..GAME_CHECKSUM]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|v| u32::from_le_bytes(*v))
        .fold(0u32, u32::wrapping_add);
    if sum == word(&record[GAME_CHECKSUM..]) {
        Validity::Valid
    } else {
        Validity::BadChecksum
    }
}
fn missing_marker(record: &[u8]) -> Validity {
    if record.iter().all(|&v| v == 0xff) {
        Validity::Erased
    } else {
        Validity::WrongMarker
    }
}
pub fn inspect(flash: &[u8]) -> Result<FlashInspection> {
    if flash.len() != FLASH_BYTES {
        return Err(Error("expected a raw 65536-byte ASOE flash image".into()));
    }
    let mut game_slots = Vec::with_capacity(10);
    let mut tiny_slots = Vec::with_capacity(6);
    let mut selected_tiny_sector = None;
    let mut tiny_generation = 0;
    for (sector, bytes) in flash.as_chunks::<SECTOR_BYTES>().0.iter().enumerate() {
        if sector < 10 {
            game_slots.push(Slot {
                sector,
                validity: game_status(&bytes[..GAME_CHECKSUM + 4]),
                generation: None,
            });
        } else {
            let record = &bytes[..TINY_BYTES];
            let validity = if half(&record[2..]) != TINY_MAGIC {
                missing_marker(record)
            } else if half(record) != tiny_checksum(&record[2..]) {
                Validity::BadChecksum
            } else {
                Validity::Valid
            };
            let generation = (validity == Validity::Valid).then(|| word(&record[4..]));
            if let Some(version) = generation {
                if selected_tiny_sector.is_none() || version > tiny_generation {
                    selected_tiny_sector = Some(sector);
                    tiny_generation = version;
                }
            }
            tiny_slots.push(Slot {
                sector,
                validity,
                generation,
            });
        }
    }
    // Sector zero is probed separately. With a missing marker there, the
    // game still scans from one. Later missing markers terminate that scan.
    let start = usize::from(matches!(
        game_slots[0].validity,
        Validity::Erased | Validity::WrongMarker
    ));
    let selected_game_sector = game_slots[start..]
        .iter()
        .take_while(|slot| matches!(slot.validity, Validity::Valid | Validity::BadChecksum))
        .filter(|slot| slot.validity == Validity::Valid)
        .map(|slot| slot.sector)
        .last();
    let next_game_sector = selected_game_sector.map_or(0, |n| (n + 1) % 10);
    Ok(FlashInspection {
        game_slots,
        selected_game_sector,
        next_game_sector,
        game_erase_ahead_sector: (next_game_sector + 1) % 10,
        tiny_slots,
        selected_tiny_sector,
        next_tiny_sector: selected_tiny_sector.map_or(10, |s| 10 + (s - 10 + 1) % 6),
        next_tiny_generation: tiny_generation.wrapping_add(1),
    })
}
