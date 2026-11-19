//! CartD vehicle-state movement callbacks 8 and 9, before the common contact pass.
use crate::{city_rel::CityRel, kart_progress, Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    vehicle_state: i32,
    position: [f32; 3],
    angles: [i32; 3],
    work_68: i32,
    work_dc: i32,
    global_21760: i32,
    global_22494: i32,
}

pub fn step(bytes: &[u8], mut s: State) -> Result<State> {
    kart_progress::verify(bytes)?;
    let rel = CityRel::read_identified(bytes, 80)?;
    let constants = rel.data(4)?;
    let f = |i: usize| f32::from_be_bytes(constants[i..i + 4].try_into().unwrap());
    match s.vehicle_state {
        8 => {
            s.position[0] += f(0x278);
            s.angles = [0, -0x4000, 0];
        }
        9 => {
            s.position[2] -= f(0x27c);
            let previous = s.global_21760;
            s.global_21760 = previous.wrapping_add(1);
            if previous > 120 {
                s.global_22494 = 0;
                s.vehicle_state = 0;
            }
        }
        _ => return Err(Error::invalid("expected kart movement state 8 or 9")),
    }
    s.work_68 = 0;
    s.work_dc = 0;
    Ok(s)
}
