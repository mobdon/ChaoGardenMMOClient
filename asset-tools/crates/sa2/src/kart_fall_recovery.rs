//! CartD vehicle fall callback, text 0x4b50..0x4eb0.
use crate::{city_rel::CityRel, kart_progress, Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Vehicle {
    position: [f32; 3],
    angles: [i32; 3],
    task_mode: u8,
    saved_position: [f32; 3],
    saved_angles: [i32; 3],
    grounded_position: [f32; 3],
    velocity: [f32; 3],
    work_68: i32,
    work_fc_104: [i32; 3],
    state: i32,
    segment: i32,
    lap: i32,
    work_cc: i32,
    work_254: i32,
    work_25c: i32,
    timer: i32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Vertex {
    segment: i32,
    corner: i32,
    xz: [f32; 2],
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    vehicle: Vehicle,
    work_74: i32,
    work_44: i32,
    work_260: i32,
    mode: i32,
    engine_halfword_64: Option<i16>,
    fall_height: Option<f32>,
    segment_count: Option<i32>,
    vertex: Option<Vertex>,
    angle_radians: Option<f64>,
}
#[derive(Serialize)]
pub struct Output {
    vehicle: Vehicle,
    engine_call_8001ef18: Option<i32>,
}

pub fn tick(bytes: &[u8], s: Input) -> Result<Output> {
    run(bytes, s, None)
}
pub fn course_tick(bytes: &[u8], exe: &[u8], name: &str, s: Input) -> Result<Output> {
    if s.fall_height.is_some() || s.segment_count.is_some() || s.vertex.is_some() {
        return Err(Error::invalid(
            "course supplies fall height, segment count and vertex",
        ));
    }
    let directory = kart_progress::read_crossings(bytes, exe)?;
    let route = directory
        .courses
        .iter()
        .find(|r| r.course == name)
        .ok_or_else(|| Error::invalid("unknown recovery course"))?;
    run(
        bytes,
        Input {
            fall_height: Some(route.fall_height),
            ..s
        },
        Some(&route.projected_segment_xz),
    )
}
fn run(bytes: &[u8], s: Input, route: Option<&[[[f32; 2]; 4]]>) -> Result<Output> {
    kart_progress::verify(bytes)?;
    let count = route
        .map(|r| r.len() as i32)
        .or(s.segment_count)
        .ok_or_else(|| Error::invalid("missing recovery segment count"))?;
    let rel = CityRel::read_identified(bytes, 80)?;
    let data = rel.data(4)?;
    let f = |n: usize| f32::from_be_bytes(data[n..n + 4].try_into().unwrap());
    let mut v = s.vehicle;
    let mut call = None;
    if s.work_74 != 0 {
        v.grounded_position = v.position;
    }
    if v.position[1]
        < s.fall_height
            .ok_or_else(|| Error::invalid("missing fall height"))?
    {
        let story = matches!(s.mode, 0 | 1);
        if story {
            if s.engine_halfword_64
                .ok_or_else(|| Error::invalid("missing engine halfword +0x64"))?
                < 1
            {
                v.state = 10;
                return Ok(Output {
                    vehicle: v,
                    engine_call_8001ef18: Some(8),
                });
            }
            call = Some(4);
        } else {
            v.timer = 181;
        }
        let guided = s.work_44 == 0;
        if !guided {
            if v.timer == 1 {
                return Ok(Output {
                    vehicle: v,
                    engine_call_8001ef18: call,
                });
            }
            let prior = v.timer;
            v.timer = v.timer.wrapping_add(1);
            if prior <= 180 {
                v.position = v.saved_position;
                v.angles = v.saved_angles;
                return Ok(Output {
                    vehicle: v,
                    engine_call_8001ef18: call,
                });
            }
            v.segment = v.segment.wrapping_sub(5);
            if v.segment < 0 {
                if story {
                    v.segment = 1;
                } else {
                    v.segment = v.segment.wrapping_add(count);
                    v.lap = v.lap.wrapping_sub(1);
                    v.work_cc = 2;
                }
            }
        }
        let (seg, corner, height) = if guided {
            (v.work_25c.wrapping_sub(5), s.work_260, 0x28c)
        } else {
            (v.segment, 1, 0x294)
        };
        let xz = if let Some(route) = route {
            *route
                .get(seg as usize)
                .and_then(|quad| quad.get(corner as usize))
                .ok_or_else(|| Error::invalid("recovery address outside projected course"))?
        } else {
            let pt = s
                .vertex
                .ok_or_else(|| Error::invalid("missing selected route vertex"))?;
            if (pt.segment, pt.corner) != (seg, corner) {
                return Err(Error::invalid("wrong recovery route vertex"));
            }
            pt.xz
        };
        v.position = [xz[0], f(height) + v.grounded_position[1], xz[1]];
        v.saved_position = v.position;
        v.saved_position[1] += f(0x27c);
        v.velocity = [v.velocity[0] / f(0x290), f(0x1c8), v.velocity[2] / f(0x290)];
        v.work_68 = 0;
        if guided {
            v.work_254 = v.work_25c.wrapping_sub(4);
            v.work_25c = seg;
            v.task_mode = 0;
            if v.velocity[2].abs() >= f(0x21c) || v.velocity[0].abs() >= f(0x21c) {
                let r = s
                    .angle_radians
                    .ok_or_else(|| Error::invalid("missing recovery heading return"))?;
                let u = r * f64::from_be_bytes(data[0x208..0x210].try_into().unwrap());
                if !(-2147483648.0..2147483648.0).contains(&u) {
                    return Err(Error::invalid("unsupported heading"));
                }
                v.angles[1] = (u as i32).wrapping_add(0x4000).wrapping_neg() & 0xffff;
            }
        } else {
            v.work_fc_104 = [0; 3];
            v.timer = 0;
        }
    }
    Ok(Output {
        vehicle: v,
        engine_call_8001ef18: call,
    })
}
