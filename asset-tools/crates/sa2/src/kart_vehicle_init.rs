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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileInput {
    profile: usize,
    alternate: bool,
    velocity: [f32; 3],
    normalize: Option<Normalize>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Normalize {
    magnitude: f32,
    vector: [f32; 3],
}
#[derive(Serialize)]
pub struct Profile {
    section5_offset: usize,
    acceleration: f32,
    brake_power: f32,
    deceleration: f32,
    speed_cap: f32,
    weight: f32,
    field_14: f32,
    drift_handling: f32,
    speed_threshold: f32,
    field_20: f32,
    velocity_limit: f32,
    velocity: [f32; 3],
    shared_velocity_write: Option<[f32; 3]>,
}
/// Resolve the physics row copied by 0x2284 and bound the incoming 3D velocity.
pub fn profile(bytes: &[u8], s: ProfileInput) -> Result<Profile> {
    kart_progress::verify(bytes)?;
    if s.profile > 8 || (s.alternate && !(2..=7).contains(&s.profile)) {
        return Err(crate::Error::invalid(
            "unsupported base/alternate kart profile",
        ));
    }
    let rel = CityRel::read_identified(bytes, 80)?;
    let data = rel.data(5)?;
    let offset = if s.alternate {
        0x3834 + (s.profile - 2) * 40
    } else {
        0x36cc + s.profile * 40
    };
    let row: [f32; 10] = std::array::from_fn(|j| {
        f32::from_be_bytes(data[offset + 4 * j..offset + 4 * j + 4].try_into().unwrap())
    });
    let mut velocity = s.velocity;
    let mut shared_velocity_write = None;
    if velocity != [0.; 3] {
        let n = s.normalize.ok_or_else(|| {
            crate::Error::invalid("moving kart requires observed normalization result")
        })?;
        if n.magnitude > row[9] {
            velocity = n.vector.map(|v| v * row[9]);
            shared_velocity_write = Some(velocity);
        }
    }
    Ok(Profile {
        section5_offset: offset,
        acceleration: row[0],
        brake_power: row[1],
        deceleration: row[2],
        speed_cap: row[3],
        weight: row[4],
        field_14: row[5],
        drift_handling: row[6],
        speed_threshold: row[7],
        field_20: row[8],
        velocity_limit: row[9],
        velocity,
        shared_velocity_write,
    })
}
