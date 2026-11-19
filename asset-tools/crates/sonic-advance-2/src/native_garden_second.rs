//! Cartridge-only Chao Bounce practice, reusing the established Rust task core.
//! Native source/payload evidence and host boundaries: docs/NATIVE-GARDEN-BOUNCE.md.
use crate::{GARDEN_SHA256, VerifiedRom, gba};

#[path = "reused/native_garden_bios.rs"]
pub mod bios;
pub use bios::{BiosMath, ReferenceBiosMath, RejectBiosMath};

#[path = "reused/native_bounce.rs"]
mod engine;
pub use engine::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeButtons {
    pub pressed: u16,
    pub held: u16,
}

pub const MAX_PRACTICE_FRAMES: u32 = 36_000;

fn verify_payload(payload: &[u8]) -> Result<(), crate::Error> {
    if payload.len() != 191_340 || gba::sha256(payload) != GARDEN_SHA256 {
        return Err(crate::Error(
            "Chao Bounce requires the verified A2NE garden payload".into(),
        ));
    }
    Ok(())
}

pub struct BouncePractice {
    round: NativeBounceRound,
    held: u16,
    frames: u32,
}
impl BouncePractice {
    pub fn new(rom: &VerifiedRom<'_>) -> Result<Self, Error> {
        Ok(Self {
            round: A2neBounceSource::from_verified_payload(rom.garden_bytes())?.begin_round(),
            held: 0,
            frames: 0,
        })
    }
    /// One native video-frame update. A1/B2/Start8/right16/left32/up64/down128;
    /// button edges derive solely from the preceding accepted physical word.
    pub fn tick(&mut self, held: u16) -> Result<NativeBounceView, Error> {
        if held & !0xff != 0 {
            return Err(Error::InvalidInput);
        }
        if self.round.finished().is_some() {
            return Ok(self.round.view());
        }
        if self.frames == MAX_PRACTICE_FRAMES {
            return Err(Error::InvalidState);
        }
        self.round.step(
            NativeButtons {
                pressed: held & !self.held,
                held,
            },
            &ReferenceBiosMath,
        )?;
        self.held = held;
        self.frames += 1;
        Ok(self.round.view())
    }
    pub fn view(&self) -> NativeBounceView {
        self.round.view()
    }
    pub fn frames(&self) -> u32 {
        self.frames
    }
    /// Local cartridge result only. This does not write any save or award rings
    /// to an MMO profile; persistence/transfer remains a separate adapter.
    pub fn finished(&self) -> Option<(bool, u16)> {
        self.round.finished()
    }
}
