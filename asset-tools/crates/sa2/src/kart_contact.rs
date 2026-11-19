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
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Contact {
    return_word: i32,
    resolved_position: [f32; 3],
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
}
#[derive(Serialize)]
struct Response {
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
        Response {
            work_74: hit.return_word,
            work_28c: if contact {
                0
            } else {
                s.work_28c.wrapping_add(1)
            },
            position: hit.resolved_position,
            velocity: std::array::from_fn(|i| hit.resolved_position[i] - s.previous_position[i]),
            calls,
            next: if contact {
                "contact_2e00"
            } else {
                "no_contact_2f5c"
            },
        }
    });
    Ok(Update { request, response })
}
