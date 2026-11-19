//! Embedded A2NE Tiny Chao Garden random stream and the initial Memory deal.
//! The unflipped board and RNG are private round inputs, never a public snapshot.
use std::num::NonZeroU16;

pub use crate::native_growth::NativeRng as GardenRng;

impl GardenRng {
    pub fn below(&mut self, limit: NonZeroU16) -> u16 {
        self.bounded(limit)
    }
}

/// Narrow trusted-board inspection for the source fixture; the playable
/// round calls the same placement primitive and exposes a sanitized view.
pub struct MemoryDeal(crate::native_garden_memory::NativeMemoryBoard);
impl MemoryDeal {
    pub fn new(rng: &mut GardenRng) -> Result<Self, crate::native_garden_memory::Error> {
        crate::native_garden_memory::create_board_with_limit(
            rng,
            crate::native_garden_memory::MAX_PLACEMENT_RANDOM_CALLS,
        )
        .map(Self)
    }
    pub fn card(&self, row: u8, column: u8) -> Option<u8> {
        if row >= 5 || column >= 6 {
            return None;
        }
        u8::try_from(self.0.native_cells()[usize::from(row) * 6 + usize::from(column)]).ok()
    }
    pub fn face(&self, row: u8, column: u8) -> Option<u8> {
        self.card(row, column).map(|id| id / 2)
    }
}
