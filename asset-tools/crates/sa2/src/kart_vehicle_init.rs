//! CartD's vehicle initialization callback, text 0x2898 through 0x2a28.
use crate::{city_rel::CityRel, kart_progress, Result};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    work_44: i32,
    player_slot: i32,
    position: [f32; 3],
    angles: [i32; 3],
    /// The four words from the selected live profile at +0x14,+0x18,+0x1c,+0x28.
    profile_words: [u32; 4],
}
#[derive(Serialize)]
pub struct Init {
    position: [f32; 3],
    angles: [i32; 3],
    previous_position: [f32; 3],
    velocity: [f32; 3],
    vehicle_state: i32,
    work_70: i32,
    player_slot: i32,
    profile_words_34_to_40: [u32; 4],
    cleared_work_offsets: &'static [u16],
    guidance: Option<crate::kart_guided_drive::Guidance>,
}
pub fn init(bytes: &[u8], s: Input) -> Result<Init> {
    kart_progress::verify(bytes)?;
    let rel = CityRel::read_identified(bytes, 80)?;
    let c = rel.data(4)?;
    let zero = f32::from_be_bytes(c[0x1c8..0x1cc].try_into().unwrap());
    let guided = s.work_44 == 0;
    let guidance = guided
        .then(|| crate::kart_guided_drive::Guidance::at_spawn(s.player_slot, s.angles[1], zero));
    Ok(Init {
        position: s.position,
        angles: s.angles,
        previous_position: s.position,
        velocity: [zero; 3],
        vehicle_state: if guided { 7 } else { 0 },
        work_70: if guided { -1 } else { s.player_slot },
        player_slot: s.player_slot,
        profile_words_34_to_40: s.profile_words,
        cleared_work_offsets: &[
            0x68, 0xf0, 0xf4, 0xf8, 0x108, 0xbc, 0xc4, 0xc8, 0xd8, 0xdc, 0xec, 0x290, 0x28c, 0xcc,
            0x294,
        ],
        guidance,
    })
}
