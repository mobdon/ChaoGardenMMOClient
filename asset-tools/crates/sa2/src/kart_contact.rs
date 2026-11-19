//! Native kart movement query and first response writes: CartD 0x2ca4..0x2dfc.
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

pub fn step(bytes: &[u8], s: Input) -> Result<Update> {
    kart_progress::verify(bytes)?;
    let rel = crate::city_rel::CityRel::read_identified(bytes, 80)?;
    let data = rel.data(4)?;
    let f = |i: usize| f32::from_be_bytes(data[i..i + 4].try_into().unwrap());
    let request = Request {
        from: s.previous_position,
        to: s.proposed_position,
        word_18: 0,
        float_1c: f(0x1f4),
    };
    let response = s.result.map(|hit| {
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
        Response {
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
            next: if advanced {
                "velocity_transform_2fa0"
            } else if contact {
                "contact_2e00"
            } else {
                "no_contact_2f5c"
            },
        }
    });
    Ok(Update { request, response })
}
