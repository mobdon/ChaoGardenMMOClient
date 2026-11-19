//! Mode 5 pitch alignment and the motion-4 countdown. Angle helpers belong to the engine.
use crate::{Error, Result};
use serde::Deserialize;
use serde_json::{json, Value};

pub(super) struct Entry {
    pub angles: [i32; 3],
    pub phase: [i32; 3],
    pub target: [i32; 2],
    pub flags: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Tick {
    velocity_xz: [f32; 2],
    acceleration_y: f32,
    adjusted_angles: Vec<i32>,
    difference: Option<i32>,
}

pub(super) fn run(mut s: Entry, ticks: Vec<Tick>, duration: u16) -> Result<Value> {
    if ticks.len() > 2048 {
        return Err(Error::invalid("too many aim updates"));
    }
    let mut sub = 3;
    let mut timer = 0u16;
    let mut rows = Vec::new();
    for t in ticks {
        let count = match sub {
            3 => 1,
            4 => 3,
            5 => 6,
            _ => return Err(Error::invalid("aim countdown already finished")),
        };
        if t.adjusted_angles.len() != count
            || (sub == 3) != t.difference.is_some()
            || !t
                .velocity_xz
                .iter()
                .chain([&t.acceleration_y])
                .all(|v| v.is_finite())
        {
            return Err(Error::invalid("invalid aim observations"));
        }
        let mut results = t.adjusted_angles.into_iter();
        let mut calls = Vec::new();
        let mut adjust = |angle: &mut i32, target: i32, step: i32| {
            calls.push([*angle, target, step]);
            *angle = results.next().expect("validated result count");
        };
        let mut velocity = t.velocity_xz;
        let mut ay = t.acceleration_y;
        let mut motion = None;
        let mut diff_args = None;
        if sub != 5 {
            for (v, d) in s.phase.iter_mut().zip([0x210, 0x250, 0x300]) {
                *v = v.wrapping_add(d);
            }
        }
        match sub {
            3 => {
                velocity = velocity.map(|v| if v.abs() < 0.005 { 0. } else { v * 0.5 });
                adjust(&mut s.angles[0], s.target[0], 192);
                diff_args = Some([s.angles[0], s.target[0]]);
                if t.difference == Some(0) {
                    if s.flags & 0x400 == 0 {
                        s.target[0] = 0;
                        s.flags |= 0x400;
                    } else {
                        motion = Some(4);
                        timer = duration;
                        sub = 4;
                    }
                }
            }
            4 => {
                adjust(&mut s.angles[1], s.target[1], 128);
                adjust(&mut s.angles[0], 0, 128);
                adjust(&mut s.angles[2], 0, 128);
                timer = timer.wrapping_sub(1);
                if f32::from(timer) < 0.5 * f32::from(duration) {
                    sub = 5;
                }
            }
            5 => {
                ay -= 0.0015;
                for v in &mut s.phase {
                    adjust(v, 0, 128);
                }
                adjust(&mut s.angles[0], 0, 256);
                adjust(&mut s.angles[2], 0, 256);
                adjust(&mut s.angles[1], s.target[1], 256);
                timer = timer.wrapping_sub(1);
                if timer == 0 {
                    sub = 6;
                }
            }
            _ => unreachable!(),
        }
        ay += 0.05;
        rows.push(json!({"submode":sub,"flags":s.flags,"timer":timer,"angles":s.angles,"phase":s.phase,"target":s.target,
            "velocityXZ":velocity,"accelerationY":ay,"angleCalls":calls,"differenceArgs":diff_args,"motion":motion}));
    }
    Ok(json!(rows))
}
