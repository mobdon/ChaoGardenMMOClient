use crate::{city_rel::CityRel, kart_progress, Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct Guidance {
    work_254: i32,
    work_258: i32,
    work_25c: i32,
    work_260: i32,
    work_264: f32,
    work_274: i32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    guidance: Guidance,
    segment: i32,
    segment_count: i32,
    quad_segment: i32,
    quad: [[f32; 2]; 4],
    position: [f32; 3],
    previous_ground_position: [f32; 3],
    velocity_y: f32,
    work_74: i32,
    place: i32,
    player_slot: i32,
    lap: i32,
    acceleration: f32,
    deceleration: f32,
    weight: f32,
    engine: Engine,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Engine {
    angle_radians: f64,
    angle_difference: i32,
    stepped_angle: i32,
    direction_xz: [f32; 2],
    distance_return: Option<f32>,
    random_return: Option<i32>,
}
#[derive(Serialize)]
pub struct Tick {
    guidance: Guidance,
    position: [f32; 3],
    previous_ground_position: [f32; 3],
    yaw: i32,
    angle_input_zx: [f32; 2],
    target_angle: i32,
    speed_cap: f32,
    advanced: bool,
}
/// CartD 0x42c8..0x46f4. Imported angle, trig, distance and RNG results are observations.
pub fn tick(bytes: &[u8], s: Input) -> Result<Tick> {
    kart_progress::verify(bytes)?;
    let rel = CityRel::read_identified(bytes, 80)?;
    let c = rel.data(4)?;
    let data = rel.data(5)?;
    let f = |p: &[u8], i: usize| f32::from_be_bytes(p[i..i + 4].try_into().unwrap());
    let mut g = s.guidance;
    if !(0..s.segment_count).contains(&g.work_254)
        || g.work_254 != s.quad_segment
        || !(0..4).contains(&g.work_258)
    {
        return Err(Error::invalid(
            "state 7 requires the active target segment and point",
        ));
    }
    let [tx, tz] = s.quad[g.work_258 as usize];
    let angle_input_zx = [tz - s.position[2], tx - s.position[0]];
    let desired = s.engine.angle_radians * f64::from_be_bytes(c[0x208..0x210].try_into().unwrap());
    if !(-2147483648.0..2147483648.0).contains(&desired) {
        return Err(Error::invalid("unsupported target angle"));
    }
    let bonus = if s.place > 1 { f(data, 0x36c8) } else { 0. };
    let base = f(c, 0x27c) + f(c, 0x21c) * (5i32.wrapping_sub(s.player_slot) as f32);
    let base = base + f(c, 0x210) * (2i32.wrapping_sub(s.lap) as f32);
    let cap = f(data, 0x36c0) * (bonus + (base + s.place as f32 * f(data, 0x36c4)));
    g.work_264 = if s.engine.angle_difference < 364 {
        (g.work_264 + s.acceleration).min(cap)
    } else {
        (g.work_264 + s.deceleration).max(f(c, 0x274))
    };
    g.work_274 = s.engine.stepped_angle;
    let mut position = s.position;
    position[0] += g.work_264 * s.engine.direction_xz[0];
    position[2] += g.work_264 * s.engine.direction_xz[1];
    position[1] += s.velocity_y - s.weight;
    let advanced = g.work_254 == s.segment
        || s.engine
            .distance_return
            .ok_or_else(|| Error::invalid("missing pre-movement target distance return"))?
            < f(c, 0x280);
    if advanced {
        g.work_25c = g.work_254;
        g.work_260 = g.work_258;
        g.work_254 += 1;
        if g.work_254 >= s.segment_count {
            g.work_254 = 0;
        }
        let rnd = s
            .engine
            .random_return
            .ok_or_else(|| Error::invalid("missing waypoint random return"))?;
        let choice = (f(c, 0x1d4) * (f(c, 0x284) * rnd as f32)) as i32;
        g.work_258 = (g.work_258
            + match choice {
                0 => 1,
                1 => -1,
                _ => 0,
            })
        .clamp(0, 3);
    }
    Ok(Tick {
        yaw: (-0x4000i32).wrapping_sub(g.work_274),
        guidance: g,
        position,
        previous_ground_position: if s.work_74 != 0 {
            position
        } else {
            s.previous_ground_position
        },
        angle_input_zx,
        target_angle: desired as i32,
        speed_cap: cap,
        advanced,
    })
}

impl Guidance {
    pub(crate) fn at_spawn(slot: i32, yaw: i32, speed: f32) -> Self {
        Self {
            work_254: slot.wrapping_add(1),
            work_258: 1,
            work_25c: slot,
            work_260: 1,
            work_264: speed,
            work_274: yaw.wrapping_sub(0x4000),
        }
    }
}
