//! Original embedded Tiny Garden ring credit, return and memory save lifecycle.
//! Native byte arithmetic is preserved; these are local compatibility routines,
//! never accepted MMO reward claims or an authenticated persistence service.
use crate::{
    Error,
    native_save::{self, FIRST_GARDEN_SECTOR, GARDEN_SECTOR_COUNT, SAVE_RECORD_LEN, SECTOR_STRIDE},
};
use serde::Serialize;

pub const MAX_RINGS: u32 = 99_999;
pub const GARDEN_CALLBACK: u32 = 0x0200_1bf9;
pub const BOUNCE_RETURN_CALLBACK: u32 = 0x0200_f621;

pub fn credit_signed_byte(rings: u32, reward: u8) -> u32 {
    rings
        .wrapping_add(i32::from(reward as i8) as u32)
        .min(MAX_RINGS)
}
pub fn credit_unsigned_halfword(rings: u32, reward: u16) -> u32 {
    rings.wrapping_add(u32::from(reward)).min(MAX_RINGS)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SignedGameExit {
    pub active_byte: u8,
    pub phase: u32,
    pub main_callback: u32,
}
/// Shared Memory-mode credit/return block (0xBDB4 and0xD174). Readiness comes
/// from the original results UI service0x9DF4, which is outside this routine.
pub fn signed_game_exit_step(
    state: &mut SignedGameExit,
    rings: &mut u32,
    reward: u8,
    ready: bool,
) -> bool {
    if !ready {
        return false;
    }
    *rings = credit_signed_byte(*rings, reward);
    state.active_byte = 0;
    state.phase = 0;
    state.main_callback = GARDEN_CALLBACK;
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct BounceReturnState {
    pub main_callback: u32,
    pub completion_callback: u32,
    pub fade_counter: i16,
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct BounceReturnStep {
    pub brightness_register: u16,
    pub restore_garden_tasks: bool,
    pub returned: bool,
    pub reward_applied: bool,
}
/// Native0xF620: after three explicit UI/frame services, increment the signed
/// halfword counter and choose the stored callback or credit+garden restore.
/// The native callback dispatcher must stop calling this after `returned`.
pub fn bounce_return_step(
    state: &mut BounceReturnState,
    rings: &mut u32,
    reward: u16,
) -> BounceReturnStep {
    state.fade_counter = state.fade_counter.wrapping_add(1);
    let mut out = BounceReturnStep {
        brightness_register: state.fade_counter as u16,
        ..Default::default()
    };
    if state.fade_counter > 15 {
        out.returned = true;
        if state.completion_callback != 0 {
            state.main_callback = state.completion_callback;
        } else {
            *rings = credit_unsigned_halfword(*rings, reward);
            state.main_callback = GARDEN_CALLBACK;
            out.restore_garden_tasks = true;
            out.reward_applied = true;
        }
    }
    out
}

fn read32(raw: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(raw[at..at + 4].try_into().expect("bounded record field"))
}
fn put32(raw: &mut [u8], at: usize, value: u32) {
    raw[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GardenLoadServices {
    /// Original GameConfig.unk0: external native ring-generation input, not a
    /// trusted clock or a count that a browser may submit to an MMO server.
    pub config_counter: u32,
    /// Result of native0x8810 when no valid record exists. `from_seed` performs
    /// the verified draw and retains its resulting seed.
    pub initial_word: u32,
    /// Seed entering load after the initializer service. `from_seed` supplies
    /// the post-draw seed; valid records instead restore their saved seed.
    pub rng_seed_before: u32,
}
impl GardenLoadServices {
    /// Original0x8810 draws once from native0x1554 with bound30, then computes
    /// (draw+60)*60. Preserve the post-draw seed for the new-save load path.
    pub fn from_seed(config_counter: u32, seed: u32) -> Self {
        let mut rng = crate::native_growth::NativeRng::new(seed);
        let draw = rng.bounded(std::num::NonZeroU16::new(30).expect("nonzero native bound"));
        Self {
            config_counter,
            initial_word: (u32::from(draw) + 60) * 60,
            rng_seed_before: rng.state(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GardenSaveSession {
    record: [u8; SAVE_RECORD_LEN],
    flash: Vec<u8>,
    pub local_sector: u8,
    pub version: u32,
    pub rings: u32,
    pub rng_seed: u32,
    pub initialized: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GardenWriteReport {
    pub attempted_physical_sectors: Vec<u8>,
    pub succeeded: bool,
}
impl GardenSaveSession {
    /// LoadGameState with the existing checksum/sector reader. All opaque bytes
    /// survive, including the native no-valid-save path's untouched last-read
    /// Chao block. No source file or real device is opened or changed.
    pub fn load(flash: &[u8], services: GardenLoadServices) -> Result<Self, Error> {
        let inspection = native_save::inspect_flash(flash).map_err(|e| Error(e.to_string()))?;
        let initialized = inspection.selected.is_none();
        let mut record = [0; SAVE_RECORD_LEN];
        let (local_sector, version, rng_seed);
        if let Some(selected) = inspection.selected {
            record.copy_from_slice(selected.raw_bytes());
            local_sector = (inspection
                .selected_physical_sector
                .expect("selected sector")
                - FIRST_GARDEN_SECTOR) as u8;
            version = selected.save_counter();
            rng_seed = read32(&record, 0x18);
        } else {
            let at = (FIRST_GARDEN_SECTOR + GARDEN_SECTOR_COUNT - 1) * SECTOR_STRIDE;
            record.copy_from_slice(&flash[at..at + SAVE_RECORD_LEN]);
            record[8..0x58].fill(0);
            record[0x58..0x6a].fill(255);
            put32(&mut record, 0x10, services.initial_word);
            record[0x1c] = 255;
            record[0x59] = 0;
            record[0x1f] = 0x58;
            record[0x20] = 0x68;
            local_sector = 6;
            version = 0;
            rng_seed = services.rng_seed_before;
        }
        let previous = read32(&record, 8);
        let saved_rings = read32(&record, 0xc);
        let rings = if previous >= services.config_counter {
            saved_rings
        } else {
            put32(&mut record, 8, services.config_counter);
            saved_rings
                .wrapping_add(services.config_counter.wrapping_sub(previous))
                .min(MAX_RINGS)
        };
        put32(&mut record, 0x64, rings.swap_bytes());
        record[0x68] = 0;
        Ok(Self {
            record,
            flash: flash.to_vec(),
            local_sector,
            version,
            rings,
            rng_seed,
            initialized,
        })
    }
    pub fn record_bytes(&self) -> &[u8; SAVE_RECORD_LEN] {
        &self.record
    }
    /// Crate-local native gameplay may update only independently traced fields.
    /// Checksum/header are regenerated by write_memory; no input file is edited.
    pub(crate) fn record_mut(&mut self) -> &mut [u8; SAVE_RECORD_LEN] {
        &mut self.record
    }
    pub fn flash_bytes(&self) -> &[u8] {
        &self.flash
    }
    /// Original SaveGameState packing and five attempts across six sectors.
    /// Failed bits0..5 leave that physical sector unchanged. Successful memory
    /// programming fills one sectorFF and copies0x858bytes; this is an explicit
    /// adapter, not physical flash programming/error/timing or durable storage.
    pub fn write_memory(&mut self, failed_local_sectors: u8) -> Result<GardenWriteReport, Error> {
        if self.local_sector > 6 {
            return Err(Error("Invalid local garden save sector".into()));
        }
        self.record[2..4].copy_from_slice(&native_save::SAVE_MARKER.to_le_bytes());
        self.version = self.version.wrapping_add(1);
        put32(&mut self.record, 4, self.version);
        put32(&mut self.record, 0xc, self.rings);
        put32(&mut self.record, 0x18, self.rng_seed);
        let checksum = native_save::checksum(&self.record[2..]);
        self.record[..2].copy_from_slice(&checksum.to_le_bytes());
        let mut report = GardenWriteReport {
            attempted_physical_sectors: Vec::with_capacity(5),
            succeeded: false,
        };
        for _ in 0..5 {
            self.local_sector = self.local_sector.wrapping_add(1);
            if self.local_sector >= 6 {
                self.local_sector = 0;
            }
            let physical = self.local_sector + 10;
            report.attempted_physical_sectors.push(physical);
            if failed_local_sectors & (1 << self.local_sector) == 0 {
                let at = usize::from(physical) * SECTOR_STRIDE;
                self.flash[at..at + SECTOR_STRIDE].fill(255);
                self.flash[at..at + SAVE_RECORD_LEN].copy_from_slice(&self.record);
                report.succeeded = true;
                break;
            }
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_credit_retains_unsigned_wrap_and_signed_byte_semantics() {
        assert_eq!(credit_signed_byte(0, 255), 99999);
        assert_eq!(credit_signed_byte(200, 255), 199);
        assert_eq!(credit_unsigned_halfword(u32::MAX, 2), 1);
        assert_eq!(credit_unsigned_halfword(99990, 20), 99999);
    }
    #[test]
    fn native_return_is_owned_by_callback_dispatch() {
        let mut s = BounceReturnState {
            main_callback: BOUNCE_RETURN_CALLBACK,
            completion_callback: 0,
            fade_counter: 14,
        };
        let mut rings = 10;
        assert!(!bounce_return_step(&mut s, &mut rings, 20).returned);
        assert!(bounce_return_step(&mut s, &mut rings, 20).restore_garden_tasks);
        assert_eq!((s.main_callback, rings), (GARDEN_CALLBACK, 30));
        if s.main_callback == BOUNCE_RETURN_CALLBACK {
            bounce_return_step(&mut s, &mut rings, 20);
        }
        assert_eq!(rings, 30);
    }
}
