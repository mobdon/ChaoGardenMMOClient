use crate::{city_rel::CityRel, kart_progress, Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Deserialize, Serialize)]
pub struct Guidance {
    pub(crate) work_254: i32,
    pub(crate) work_258: i32,
    pub(crate) work_25c: i32,
    pub(crate) work_260: i32,
    pub(crate) work_264: f32,
    pub(crate) work_274: i32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    tuning: Option<[f32; 3]>,
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
    pub(crate) guidance: Guidance,
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
    tick_verified(&rel, s)
}
fn tick_verified(rel: &CityRel, s: Input) -> Result<Tick> {
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
    let [scale, place_factor, extra] = s
        .tuning
        .unwrap_or_else(|| std::array::from_fn(|j| f(data, 0x36c0 + 4 * j)));
    let bonus = if s.place > 1 { extra } else { 0. };
    let base = f(c, 0x27c) + f(c, 0x21c) * (5i32.wrapping_sub(s.player_slot) as f32);
    let base = base + f(c, 0x210) * (2i32.wrapping_sub(s.lap) as f32);
    let cap = scale * (bonus + (base + s.place as f32 * place_factor));
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoryDrive {
    position: Option<[f32; 3]>,
    velocity_y: Option<f32>,
    previous_ground_position: [f32; 3],
    work_74: i32,
    place: i32,
    engine: Engine,
}
impl StoryDrive {
    pub(crate) fn run(
        self,
        rel: &CityRel,
        guidance: Guidance,
        route: (i32, i32),
        lap: i32,
        quads: &[[[f32; 2]; 4]],
        pose: Option<([f32; 3], [f32; 3], [i32; 3])>,
        mode: usize,
    ) -> Result<Tick> {
        if quads.len() != route.1 as usize {
            return Err(Error::invalid(
                "guided segment count conflicts with installed course",
            ));
        }
        let quad = *quads
            .get(guidance.work_254 as usize)
            .ok_or_else(|| Error::invalid("missing guided target quad"))?;
        let (mut position, mut vy) = (self.position, self.velocity_y);
        if let Some((p, v, _)) = pose {
            if position.is_some_and(|q| q.map(f32::to_bits) != p.map(f32::to_bits)) {
                return Err(Error::invalid(
                    "guided position conflicts with preceding frame",
                ));
            }
            position = Some(p);
            // A nonzero incoming vector can be clamped before dispatch; its vertical
            // component must then be observed after the profile speed limit.
            if v == [0.; 3] {
                if vy.is_some_and(|y| y != 0.) {
                    return Err(Error::invalid(
                        "guided vertical velocity conflicts with stationary frame",
                    ));
                }
                vy = Some(v[1]);
            }
        }
        let row = crate::kart_vehicle_init::base_row(rel.data(5)?, mode);
        tick_verified(
            rel,
            Input {
                tuning: None,
                guidance,
                segment: route.0,
                segment_count: route.1,
                quad_segment: guidance.work_254,
                quad,
                position: position.ok_or_else(|| Error::invalid("missing guided task position"))?,
                velocity_y: vy.ok_or_else(|| Error::invalid("missing guided dispatch velocity"))?,
                previous_ground_position: self.previous_ground_position,
                work_74: self.work_74,
                place: self.place,
                player_slot: 0,
                lap,
                acceleration: row[0],
                // The guided turn branch reads section 6 +0x21444 (profile word 1).
                deceleration: row[1],
                weight: row[4],
                engine: self.engine,
            },
        )
    }
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupInput {
    mode: i32,
    previous_tuning: Option<[f32; 3]>,
    mode5: Option<Mode5>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Mode5 {
    engine_word_b4: i32,
    profile_words: [u32; 10],
    tuning_28: [f32; 3],
    tuning_34: [f32; 3],
}
#[derive(Serialize)]
pub struct Setup {
    tuning: [f32; 3],
    profile8_write: Option<[u32; 10]>,
}
/// Module-wide writes made by race setup at 0x5ce8..0x5e74.
pub fn setup(bytes: &[u8], s: SetupInput) -> Result<Setup> {
    kart_progress::verify(bytes)?;
    let rel = CityRel::read_identified(bytes, 80)?;
    let c = rel.data(4)?;
    let d = rel.data(5)?;
    let f = |p: &[u8], i: usize| f32::from_be_bytes(p[i..i + 4].try_into().unwrap());
    let mut tuning = s
        .previous_tuning
        .unwrap_or_else(|| std::array::from_fn(|j| f(d, 0x36c0 + j * 4)));
    let mut profile8_write = None;
    match s.mode {
        2 => tuning[0] = f(c, 0x270),
        3 => tuning[0] = f(c, 0x2a4),
        4 => {
            tuning[0] = f(c, 0x214);
            tuning[2] = f(c, 0x210);
        }
        5 => {
            let row = s
                .mode5
                .ok_or_else(|| Error::invalid("mode 5 requires the selected live setup record"))?;
            profile8_write = Some(row.profile_words);
            tuning = if row.engine_word_b4 == 1 {
                row.tuning_34
            } else {
                row.tuning_28
            };
        }
        _ => {}
    }
    Ok(Setup {
        tuning,
        profile8_write,
    })
}

/// Read the pre-fixup engine buffer using relative offsets (CartD text 0..0x13c).
pub fn buffer_tuning(
    bytes: &[u8],
    buffer: &[u8],
    engine_word_b4: i32,
) -> Result<serde_json::Value> {
    let header = buffer
        .get(..24)
        .ok_or_else(|| Error::invalid("truncated kart setup header"))?;
    let word = |s: &[u8], i: usize| u32::from_be_bytes(s[i..i + 4].try_into().unwrap());
    let offset = word(header, 0) as usize;
    let record = buffer
        .get(offset..)
        .and_then(|s| s.get(..64))
        .ok_or_else(|| Error::invalid("kart profile outside setup buffer"))?;
    if offset < 24 {
        return Err(Error::invalid(
            "overlapping kart profile and relocation header unsupported",
        ));
    }
    let row = Mode5 {
        engine_word_b4,
        profile_words: std::array::from_fn(|i| word(record, i * 4)),
        tuning_28: std::array::from_fn(|i| f32::from_bits(word(record, 40 + i * 4))),
        tuning_34: std::array::from_fn(|i| f32::from_bits(word(record, 52 + i * 4))),
    };
    let result = setup(
        bytes,
        SetupInput {
            mode: 5,
            previous_tuning: None,
            mode5: Some(row),
        },
    )?;
    if !result.tuning.iter().all(|v| v.is_finite()) {
        return Err(Error::invalid(
            "selected tuning cannot be represented as JSON",
        ));
    }
    Ok(serde_json::json!({
        "profile_offset": offset,
        "transfer_arguments": [[word(header,4),word(header,8)], [word(header,12),word(header,16)]],
        "resource_offset": word(header,20),
        "setup": result,
    }))
}
