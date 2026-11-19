//! Single kart ring task at CartD +0xa824. Task keys identify collision partners.
use crate::{city_rel::CityRel, kart_progress, Error, Result};
use serde::{Deserialize, Serialize};
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct State {
    mode: i8,
    timer: u16,
    yaw: i32,
    scale_z: f32,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Kart {
    task: u32,
    work_44: i32,
    work_70: i32,
    work_d8: i32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    state: State,
    first_range_metric: f32,
    view_count: i32,
    second_range_metric: Option<f32>,
    registry: [Option<Kart>; 6],
    collision_flags: u16,
    contacts: Vec<u32>,
    has_entry: bool,
}
#[derive(Serialize)]
pub struct Output {
    state: State,
    registry: [Option<Kart>; 6],
    calls: Vec<Call>,
}
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum Call {
    InstallCallbacks([u32; 2]),
    InitCollision {
        section5_offset: u32,
        args: [i32; 2],
    },
    RegisterCollision,
    Engine80063584([i32; 2]),
    Engine8006b710([i32; 4]),
    CollectionEffectAb04,
    Engine80065148,
    Engine80071888,
}
pub fn tick(bytes: &[u8], s: Input) -> Result<Output> {
    kart_progress::verify(bytes)?;
    let rel = CityRel::read_identified(bytes, 80)?;
    let c = rel.data(4)?;
    let f = |at: usize| f32::from_be_bytes(c[at..at + 4].try_into().unwrap());
    let near = s.first_range_metric < f(0x1c0)
        || (s.view_count == 2
            && s.second_range_metric
                .ok_or_else(|| Error::invalid("missing second view range metric"))?
                < f(0x1c0));
    let mut o = Output {
        state: s.state,
        registry: s.registry,
        calls: vec![],
    };
    if !near && o.state.mode != 4 {
        return Ok(o);
    }
    match o.state.mode {
        0 => {
            o.calls.push(Call::InstallCallbacks([0xaa64, 0xab00]));
            o.calls.push(Call::InitCollision {
                section5_offset: 0x2ed3e8,
                args: [1, 5],
            });
            o.state.mode = 1;
        }
        1 => {
            o.state.scale_z = f(0x3b0);
            o.state.mode = 2;
            o.calls.push(Call::RegisterCollision);
        }
        2 => {
            o.state.scale_z = (o.state.scale_z + f(0x3b4)).min(f(0x3b8));
            if s.collision_flags & 1 != 0 {
                if s.contacts.len() > i16::MAX as usize {
                    return Err(Error::invalid("collision list exceeds native signed count"));
                }
                for task in s.contacts {
                    if task == 0 {
                        continue;
                    }
                    if let Some(k) = o.registry.iter_mut().flatten().find(|k| k.task == task) {
                        o.state.mode = 3;
                        if k.work_44 != 0 {
                            o.calls.push(Call::Engine80063584([k.work_70, 1]));
                            o.calls.push(Call::Engine8006b710([0x8005, 0, 0, 0]));
                            k.work_d8 = k.work_d8.wrapping_add(1);
                        }
                    }
                }
            }
            o.calls.push(Call::RegisterCollision);
        }
        3 => {
            o.calls.push(Call::CollectionEffectAb04);
            o.state.mode = 4;
            o.state.timer = 3600;
        }
        4 => {
            o.state.timer = o.state.timer.wrapping_sub(1);
            if o.state.timer == 0 {
                o.state.mode = 1;
            }
        }
        5 => {
            o.calls.push(if s.has_entry {
                Call::Engine80065148
            } else {
                Call::Engine80071888
            });
            return Ok(o);
        }
        _ => {}
    }
    o.state.yaw = o.state.yaw.wrapping_add(364);
    Ok(o)
}
