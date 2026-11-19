//! In-memory ASOE cartridge Garden session. Never overwrites the input image.
use crate::{flash, Error, Result};
const LEN: usize = 0x858;
fn word(r: &[u8], p: usize) -> u32 {
    u32::from_le_bytes(r[p..p + 4].try_into().unwrap())
}
fn put(r: &mut [u8], p: usize, v: u32) {
    r[p..p + 4].copy_from_slice(&v.to_le_bytes());
}

pub struct CartridgeGarden {
    record: [u8; LEN],
    flash: Vec<u8>,
    next_sector: usize,
    generation: u32,
    pub rings: u32,
    pub rng: u32,
}
impl CartridgeGarden {
    /// Open an existing structurally valid record, as opposed to manufacturing a new Chao.
    pub fn load(image: &[u8], launch_score: u32) -> Result<Self> {
        let scan = flash::inspect(image)?;
        let sector = scan
            .selected_tiny_sector
            .ok_or_else(|| Error("no valid Tiny Garden record in this flash".into()))?;
        let start = sector * flash::SECTOR_BYTES;
        let mut record: [u8; LEN] = image[start..start + LEN].try_into().unwrap();
        let old = word(&record, 8);
        let mut rings = word(&record, 12);
        if launch_score > old {
            rings = rings.wrapping_add(launch_score - old).min(99999);
            put(&mut record, 8, launch_score);
        }
        record[0x64..0x68].copy_from_slice(&rings.to_be_bytes());
        record[0x68] = 0;
        let rng = word(&record, 0x18);
        Ok(Self {
            record,
            flash: image.to_vec(),
            next_sector: scan.next_tiny_sector,
            generation: scan.next_tiny_generation,
            rings,
            rng,
        })
    }
    /// Model a successful first flash-program attempt. Remaining sector bytes are erased.
    pub fn save_copy(&self) -> Vec<u8> {
        let mut out = self.flash.clone();
        let s = &mut out
            [self.next_sector * flash::SECTOR_BYTES..(self.next_sector + 1) * flash::SECTOR_BYTES];
        s.fill(0xff);
        let r = &mut s[..LEN];
        r.copy_from_slice(&self.record);
        r[2..4].copy_from_slice(&0x817u16.to_le_bytes());
        put(r, 4, self.generation);
        put(r, 12, self.rings);
        put(r, 0x18, self.rng);
        let crc = flash::tiny_checksum(&r[2..]);
        r[..2].copy_from_slice(&crc.to_le_bytes());
        out
    }
}
