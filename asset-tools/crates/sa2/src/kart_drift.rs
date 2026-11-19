//! CartD state 2 drift windup. Engine math is a projection; task arithmetic is f32.
use crate::{kart_progress, Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    heading: i32,
    work_68: i32,
    work_a4: usize,
    work_a8: i32,
    position: [f32; 3],
    velocity: [f32; 3],
    accelerate: bool,
    work_74: i32,
    drift_handling: f32,
    weight: f32,
}
#[derive(Serialize)]
pub struct Update {
    heading: i32,
    work_68: i32,
    work_a4: usize,
    vehicle_state: i32,
    local_velocity: [f32; 3],
    position: [f32; 3],
    math_source: &'static str,
}
fn rotate([x, y, z]: [f32; 3], angle: i32) -> [f32; 3] {
    let (s, c) = (f64::from(angle as u16) * (std::f64::consts::TAU / 65536.)).sin_cos();
    [
        (c * f64::from(x) + s * f64::from(z)) as f32,
        y,
        (-s * f64::from(x) + c * f64::from(z)) as f32,
    ]
}
pub fn tick(bytes: &[u8], s: Input) -> Result<Update> {
    kart_progress::verify(bytes)?;
    if s.work_a4 >= 5 {
        return Err(Error::invalid("state 2 requires windup index 0 through 4"));
    }
    let rel = crate::city_rel::CityRel::read_identified(bytes, 80)?;
    let data = rel.data(5)?;
    let constants = rel.data(4)?;
    let f = |i: usize| f32::from_be_bytes(constants[i..i + 4].try_into().unwrap());
    let [x, y, z] = s.velocity;
    let heading = if x.abs() >= f(0x21c) || z.abs() >= f(0x21c) {
        let scale = f64::from_be_bytes(constants[0x208..0x210].try_into().unwrap());
        (-((f64::from(z).atan2(f64::from(x)) * scale) as i32 + 0x4000)) & 0xffff
    } else {
        s.heading
    };
    let at = 0x3be4 + s.work_a4 * 4;
    let amount = i32::from_be_bytes(data[at..at + 4].try_into().unwrap());
    let offset = if s.work_a8 < 0 {
        s.work_68.wrapping_add(amount)
    } else {
        s.work_68.wrapping_sub(amount)
    };
    let angle = heading.wrapping_add(offset);
    let mut v = rotate([x, 0., z], angle.wrapping_neg());
    let mut speed = f64::from(v[0]).hypot(f64::from(v[2])) as f32;
    v[2] *= s.drift_handling;
    if !s.accelerate {
        speed *= f(0x25c);
    }
    let norm = f64::from(v[0]).hypot(f64::from(v[2]));
    if norm != 0. {
        v[0] = (f64::from(v[0]) / norm) as f32 * speed;
        v[2] = (f64::from(v[2]) / norm) as f32 * speed;
    }
    let delta = rotate(v, angle);
    let mut position = std::array::from_fn(|i| s.position[i] + delta[i]);
    position[1] += y - if y > 0. && s.work_74 != 0 {
        s.weight * f(0x234)
    } else {
        s.weight
    };
    if !position.iter().all(|x| x.is_finite()) {
        return Err(Error::invalid("nonfinite kart drift projection"));
    }
    let done = s.work_a4 == 4;
    Ok(Update {
        heading,
        work_68: offset,
        work_a4: if done { 0 } else { s.work_a4 + 1 },
        vehicle_state: if done { 3 } else { 2 },
        local_velocity: v,
        position,
        math_source: "mathematical_projection",
    })
}
