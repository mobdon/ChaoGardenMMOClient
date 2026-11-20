//! CartD vehicle audio update (0x5774..0x5b8c). Handles are offsets from the task.
use crate::{city_rel::CityRel, kart_progress, kart_vehicle_init, Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub(crate) work_44: i32,
    pub(crate) vehicle_state: i32,
    pub(crate) velocity_xz: [f32; 2],
    pub(crate) position: [f32; 3],
    pub(crate) controls: Controls,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Controls {
    pub(crate) profile: usize,
    #[serde(default)]
    pub(crate) alternate: bool,
    pub(crate) work_284: i32,
    pub(crate) work_110: i32,
    pub(crate) work_114: i32,
    pub(crate) piece_id: Option<u8>,
    pub(crate) magnitude_return: Option<f32>,
}
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum Call {
    CourseMode(i32),
    Stop(i32),
    Play {
        cue: i32,
        handle: i32,
        args: [i32; 3],
        position: [f32; 3],
    },
    Pitch {
        handle: i32,
        channel: i32,
        value: i16,
    },
    Scripted {
        cue: i32,
        handle: i32,
        args: [i32; 2],
        position: [f32; 3],
    },
}
#[derive(Serialize)]
pub struct Output {
    work_284: i32,
    calls: Vec<Call>,
}
pub fn tick(bytes: &[u8], s: Input) -> Result<Output> {
    kart_progress::verify(bytes)?;
    let rel = CityRel::read_identified(bytes, 80)?;
    tick_verified(&rel, s)
}
pub(crate) fn tick_verified(rel: &CityRel, s: Input) -> Result<Output> {
    let c = s.controls;
    let mut out = Output {
        work_284: c.work_284,
        calls: vec![],
    };
    if s.work_44 == 0 || !matches!(s.vehicle_state, 0..=3 | 8 | 9) {
        return Ok(out);
    }
    let [engine, brake, ..] = kart_vehicle_init::cue_row(rel.data(5)?, c.profile, c.alternate)?;
    let position = s.position;
    if s.vehicle_state >= 8 {
        out.calls.push(Call::Scripted {
            cue: engine,
            handle: engine,
            args: [1, -20],
            position,
        });
        return Ok(out);
    }
    let speed = if s.velocity_xz == [0.; 2] {
        0.
    } else {
        c.magnitude_return
            .ok_or_else(|| Error::invalid("missing horizontal magnitude return"))?
    };
    if !(0.0..2147483648.0).contains(&speed) {
        return Err(Error::invalid("unsupported horizontal magnitude"));
    }
    let mode = kart_vehicle_init::course_mode(
        c.piece_id
            .ok_or_else(|| Error::invalid("missing current course piece"))?,
    );
    out.calls.push(Call::CourseMode(mode.engine_8006c97c_arg));
    let aux = if s.vehicle_state == 0 { 16 } else { brake };
    let play = |cue| Call::Play {
        cue,
        handle: cue,
        args: [1, -20, 1],
        position,
    };
    if mode.return_word != out.work_284 {
        out.work_284 = i32::from(out.work_284 == 0);
        out.calls.extend([Call::Stop(engine), Call::Stop(aux)]);
    } else if c.work_110 != 0 {
        out.calls.extend([play(engine), Call::Stop(aux)]);
    } else {
        let threshold = f32::from_be_bytes(rel.data(4)?[0x1f0..0x1f4].try_into().unwrap());
        out.calls.push(if c.work_114 != 0 && speed > threshold {
            play(brake)
        } else {
            Call::Stop(brake)
        });
        out.calls.push(Call::Stop(engine));
    }
    out.calls.push(Call::Pitch {
        handle: engine,
        channel: -1,
        value: (speed as i32).wrapping_shl(8) as i16,
    });
    if s.vehicle_state != 0 {
        out.calls.push(play(16));
    }
    Ok(out)
}
