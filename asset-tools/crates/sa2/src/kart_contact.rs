//! Native kart movement query and response writes: CartD 0x2ca4..0x3020.
use crate::{kart_progress, Result};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    previous_position: [f32; 3],
    proposed_position: [f32; 3],
    velocity: [f32; 3],
    work_28c: i32,
    work_44: i32,
    player_slot: i32,
    result: Option<Contact>,
    wall: Option<WallInput>,
    tail: Option<TailInput>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Contact {
    return_word: i32,
    resolved_position: [f32; 3],
    returned_from: Option<[f32; 3]>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WallInput {
    normal: [f32; 3],
    impulse: [f32; 3],
    work_70: i32,
    cooldown: i32,
    mode: i32,
    vehicle_state: i32,
    engine_byte_23: i8,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TailInput {
    observed_local_velocity: Option<[f32; 3]>,
    observed_raw_magnitude: f32,
    post_alignment_angles: [i32; 3],
    work_d4: i32,
    ring: Option<Ring>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Ring {
    slot: i32,
    cursor: usize,
    velocities: Vec<[f32; 3]>,
    rotations: Vec<[i32; 3]>,
}
impl Ring {
    pub(crate) fn validate(&self) -> Result<()> {
        if !(0..=1).contains(&self.slot)
            || self.cursor >= 60
            || self.velocities.len() != 60
            || self.rotations.len() != 60
        {
            return Err(crate::Error::invalid("invalid kart velocity/rotation ring"));
        }
        Ok(())
    }
    pub(crate) fn motion_history(&self) -> Result<[[f32; 3]; 30]> {
        self.validate()?;
        Ok(std::array::from_fn(|i| {
            self.velocities[(self.cursor + 59 - i) % 60]
        }))
    }
}
#[derive(Serialize)]
struct TailUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    observed_local_velocity: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    projected_local_velocity: Option<[f32; 3]>,
    work_d4: i32,
    ring: Option<Ring>,
    motion_history: Option<[[f32; 3]; 30]>,
}
impl TailInput {
    fn finish(self, slot: i32, raw: [f32; 3], scale: f32, units: f32) -> Result<TailUpdate> {
        let delta = (scale * self.observed_raw_magnitude) * units;
        if self.observed_raw_magnitude < 0.0 || !(0.0..2147483648.0).contains(&delta) {
            return Err(crate::Error::invalid(
                "raw magnitude outside supported fctiwz range",
            ));
        }
        let mut ring = self.ring;
        let motion_history = match (&mut ring, slot) {
            (None, i) if i < 0 => None,
            (Some(r), 0..=1) if r.slot == slot => {
                r.validate()?;
                r.velocities[r.cursor] = raw;
                r.rotations[r.cursor] = self.post_alignment_angles;
                r.cursor = (r.cursor + 1) % 60;
                Some(r.motion_history()?)
            }
            _ => return Err(crate::Error::invalid(
                "expected matching 60-entry history for slot 0/1, or no history for negative slot",
            )),
        };
        Ok(TailUpdate {
            projected_local_velocity: if self.observed_local_velocity.is_none() {
                Some(inverse_vehicle_rotation(raw, self.post_alignment_angles)?)
            } else {
                None
            },
            observed_local_velocity: self.observed_local_velocity,
            work_d4: self.work_d4.wrapping_add(delta as i32),
            ring,
            motion_history,
        })
    }
}
#[derive(Serialize)]
struct WallUpdate {
    wall_hit: bool,
    impulse: [f32; 3],
    cooldown: i32,
    vehicle_state: i32,
    align_to_normal: bool,
}
#[derive(Serialize)]
struct Request {
    from: [f32; 3],
    to: [f32; 3],
    word_18: u32,
    float_1c: f32,
}
#[derive(Serialize)]
enum Call {
    Effect10860 {
        position: [f32; 3],
        velocity: [f32; 3],
        scale: f32,
    },
    Engine8006b710([i32; 4]),
    Engine8002f8c4([i32; 4]),
    Engine80011714(i32),
    Cart9884,
}
#[derive(Serialize)]
struct Response {
    #[serde(skip_serializing_if = "Option::is_none")]
    tail: Option<TailUpdate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    wall: Option<WallUpdate>,
    work_74: i32,
    work_28c: i32,
    position: [f32; 3],
    velocity: [f32; 3],
    calls: Vec<Call>,
    next: &'static str,
}
#[derive(Serialize)]
pub struct Update {
    request: Request,
    response: Option<Response>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoryContact {
    previous_position: [f32; 3],
    result: Option<Contact>,
    wall: Option<WallInput>,
    tail: Option<TailInput>,
}
impl StoryContact {
    pub(crate) fn apply(
        self,
        data: &[u8],
        proposed_position: [f32; 3],
        velocity: [f32; 3],
        work_28c: i32,
        work_44: i32,
        mode: i32,
    ) -> Result<Update> {
        if self
            .wall
            .as_ref()
            .is_some_and(|w| w.mode != mode || w.work_70 != 0)
        {
            return Err(crate::Error::invalid(
                "wall context conflicts with primary story kart",
            ));
        }
        step_with_constants(
            data,
            Input {
                previous_position: self.previous_position,
                proposed_position,
                velocity,
                work_28c,
                work_44,
                player_slot: 0,
                result: self.result,
                wall: self.wall,
                tail: self.tail,
            },
        )
    }
}
pub fn step(bytes: &[u8], s: Input) -> Result<Update> {
    kart_progress::verify(bytes)?;
    let rel = crate::city_rel::CityRel::read_identified(bytes, 80)?;
    step_with_constants(rel.data(4)?, s)
}
fn step_with_constants(data: &[u8], s: Input) -> Result<Update> {
    let f = |i: usize| f32::from_be_bytes(data[i..i + 4].try_into().unwrap());
    let request = Request {
        from: s.previous_position,
        to: s.proposed_position,
        word_18: 0,
        float_1c: f(0x1f4),
    };
    if s.tail.is_some() && (s.wall.is_none() || s.result.is_none()) {
        return Err(crate::Error::invalid(
            "tail requires contact result and wall context",
        ));
    }
    let slot = s.wall.as_ref().map(|w| w.work_70);
    let response = s
        .result
        .map(|hit| {
            let mut calls = Vec::new();
            let contact = hit.return_word != 0;
            if contact && s.work_28c > 30 {
                calls.push(Call::Effect10860 {
                    position: s.proposed_position,
                    velocity: s.velocity,
                    scale: f(0x1f0),
                });
                if s.work_44 != 0 {
                    calls.push(Call::Engine8006b710([26, 0, 0, 0]));
                    calls.push(Call::Engine8002f8c4([s.player_slot, 4, 20, 0]));
                }
            }
            let mut position = hit.resolved_position;
            let mut velocity = std::array::from_fn(|i| position[i] - s.previous_position[i]);
            let wall = s.wall.map(|w| {
                let wall_hit = contact && w.work_70 >= 0 && w.normal[1].abs() < f(0x210);
                let mut impulse = w.impulse;
                let mut cooldown = w.cooldown;
                let mut vehicle_state = w.vehicle_state;
                if wall_hit {
                    impulse[0] = f(0x1f0) * w.normal[0];
                    impulse[2] = f(0x1f0) * w.normal[2];
                    velocity[1] = f(0x1c8);
                    position = hit.returned_from.unwrap_or(s.previous_position);
                    calls.push(Call::Engine8006b710([18, 0, 0, 0]));
                    calls.push(Call::Engine8002f8c4([s.player_slot, 4, 30, 0]));
                    if s.work_44 != 0 && cooldown == 0 {
                        if let Some(id) = match w.mode {
                            0 => Some(0x687),
                            1 => Some(0x50b),
                            _ => None,
                        } {
                            calls.push(Call::Engine80011714(id));
                        }
                        if w.engine_byte_23 == 3 {
                            vehicle_state = 10;
                            calls.push(Call::Cart9884);
                        }
                    }
                    cooldown = 180;
                }
                if w.work_70 >= 0 {
                    for i in 0..3 {
                        position[i] += impulse[i];
                    }
                    impulse[0] *= f(0x214);
                    impulse[2] *= f(0x214);
                }
                WallUpdate {
                    wall_hit,
                    impulse,
                    cooldown,
                    vehicle_state,
                    align_to_normal: contact,
                }
            });
            let advanced = wall.is_some();
            let tail = s
                .tail
                .map(|t| t.finish(slot.unwrap(), velocity, f(0x1e0), f(0x218)))
                .transpose()?;
            let done = tail.is_some();
            Ok::<_, crate::Error>(Response {
                tail,
                wall,
                work_74: hit.return_word,
                work_28c: if contact {
                    0
                } else {
                    s.work_28c.wrapping_add(1)
                },
                position,
                velocity,
                calls,
                next: if done {
                    "returned"
                } else if advanced {
                    "velocity_transform_2fa0"
                } else if contact {
                    "contact_2e00"
                } else {
                    "no_contact_2f5c"
                },
            })
        })
        .transpose()?;
    Ok(Update { request, response })
}

/// State carried across the return from 0x2da8 into 0x4b50.
pub(crate) struct FallCarry {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub angles: [i32; 3],
    pub state: i32,
    pub work_74: i32,
    pub work_44: i32,
    pub mode: i32,
    pub saved_position: [f32; 3],
}

impl Input {
    pub(crate) fn driving_audio(
        &self,
        bytes: &[u8],
        controls: crate::kart_driving_audio::Controls,
    ) -> Result<crate::kart_driving_audio::Output> {
        let wall = self
            .wall
            .as_ref()
            .ok_or_else(|| crate::Error::invalid("missing pre-contact vehicle state"))?;
        crate::kart_driving_audio::tick(
            bytes,
            crate::kart_driving_audio::Input {
                work_44: self.work_44,
                vehicle_state: wall.vehicle_state,
                velocity_xz: [self.velocity[0], self.velocity[2]],
                position: self.proposed_position,
                controls,
            },
        )
    }
}
pub(crate) fn step_for_fall(bytes: &[u8], s: Input) -> Result<(Update, FallCarry)> {
    let wall = s
        .wall
        .as_ref()
        .ok_or_else(|| crate::Error::invalid("missing wall context"))?;
    let mode = wall.mode;
    let angles = s
        .tail
        .as_ref()
        .ok_or_else(|| crate::Error::invalid("missing alignment and contact tail"))?
        .post_alignment_angles;
    let work_44 = s.work_44;
    let saved_position = s.previous_position;
    let update = step(bytes, s)?;
    let r = update
        .response
        .as_ref()
        .ok_or_else(|| crate::Error::invalid("missing engine contact result"))?;
    let state = r
        .wall
        .as_ref()
        .ok_or_else(|| crate::Error::invalid("missing wall response"))?
        .vehicle_state;
    let carry = FallCarry {
        position: r.position,
        velocity: r.velocity,
        angles,
        state,
        mode,
        work_74: r.work_74,
        work_44,
        saved_position,
    };
    Ok((update, carry))
}

/// Mathematical projection of CartD 0x5498. Z, X, Y are appended to
/// the current matrix before inversion. Its inverse acts on a world vector
/// by undoing Z first, then X, then Y. Engine trig and matrix rounding differ.
pub(crate) fn inverse_vehicle_rotation(world: [f32; 3], angles: [i32; 3]) -> Result<[f32; 3]> {
    let trig = |a: i32| (f64::from(a as u16) * (std::f64::consts::TAU / 65536.)).sin_cos();
    let [x, y, z] = world.map(f64::from);
    let (sz, cz) = trig(angles[2]);
    let (x, y) = (cz * x + sz * y, -sz * x + cz * y);
    let (sx, cx) = trig(angles[0]);
    let (y, z) = (cx * y + sx * z, -sx * y + cx * z);
    let (sy, cy) = trig(angles[1]);
    let result = [(cy * x - sy * z) as f32, y as f32, (sy * x + cy * z) as f32];
    if result.iter().all(|v| v.is_finite()) {
        Ok(result)
    } else {
        Err(crate::Error::invalid("nonfinite local kart vector"))
    }
}
