//! Replay a fixed SET spring with explicitly supplied CCL, player and matrix observations.
use crate::set::Placement;
use crate::spring::{SpringCall, SpringKind, SpringMode, SpringPlayer, StageSpring};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    out_point: [f32; 3],
    frames: Vec<Frame>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Frame {
    hit: Option<u8>,
    player: Option<Player>,
    #[serde(default)]
    range_out: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Player {
    position: [f32; 3],
    running_speed: f32,
    item: u16,
}

#[derive(Serialize)]
struct Tick {
    mode: SpringMode,
    selected_slot: u8,
    contact_timer: u8,
    phase_degrees: f32,
    compression: f32,
    calls: Vec<SpringCall>,
}

#[derive(Serialize)]
struct Replay<'a> {
    kind: SpringKind,
    placement: &'a Placement,
    signed_angles: [i32; 3],
    supplied_out_point: [f32; 3],
    trig: &'static str,
    frames: Vec<Tick>,
}

pub fn replay<'a>(
    placement: &'a Placement,
    kind: SpringKind,
    request_bytes: &[u8],
) -> Result<impl Serialize + 'a> {
    if request_bytes.len() > 1_048_576 {
        return Err(Error::invalid("spring request exceeds 1 MiB"));
    }
    let request: Request = serde_json::from_slice(request_bytes)?;
    if !(1..=2048).contains(&request.frames.len()) {
        return Err(Error::invalid("spring replay requires 1 to 2048 frames"));
    }
    if !request
        .out_point
        .iter()
        .chain(&placement.position)
        .chain(&placement.parameters)
        .all(|x| x.is_finite())
    {
        return Err(Error::invalid(
            "nonfinite spring placement or matrix result",
        ));
    }
    let signed_angles = placement.rotation.map(|angle| i32::from(angle as i16));
    let mut spring = StageSpring::new(
        kind,
        placement.position,
        signed_angles,
        placement.parameters,
    );
    let mut frames = Vec::with_capacity(request.frames.len());
    for frame in request.frames {
        if frame.hit.is_some_and(|v| v > 2) {
            return Err(Error::invalid(
                "spring hit must be slot 0, slot 1, or other task 2",
            ));
        }
        let player = frame.player.map(|p| SpringPlayer {
            position: p.position,
            running_speed: p.running_speed,
            item: p.item,
        });
        if player.is_some_and(|p| {
            !p.running_speed.is_finite() || !p.position.iter().all(|v| v.is_finite())
        }) {
            return Err(Error::invalid("nonfinite spring player input"));
        }
        if player.is_some() && spring.mode != SpringMode::Touch {
            return Err(Error::invalid(
                "player observation supplied outside Touch mode",
            ));
        }
        let calls = if frame.range_out {
            Vec::new()
        } else {
            spring.step(frame.hit, player, request.out_point, |a| {
                let radians = f64::from(a as u16) * std::f64::consts::TAU / 65536.0;
                let (sin, cos) = radians.sin_cos();
                (sin as f32, cos as f32)
            })
        };
        if calls.iter().any(|call| match call {
            SpringCall::SetPosition(_, p) | SpringCall::SpringVelocity(_, p) => {
                p.iter().any(|x| !x.is_finite())
            }
            SpringCall::VelocityYRotationNoControl { velocity_y, .. } => !velocity_y.is_finite(),
            _ => false,
        }) {
            return Err(Error::invalid("spring update overflowed"));
        }
        frames.push(Tick {
            mode: spring.mode,
            selected_slot: spring.slot,
            contact_timer: spring.contact_timer,
            phase_degrees: spring.phase_degrees,
            compression: spring.compression,
            calls,
        });
    }
    Ok(Replay {
        kind,
        placement,
        signed_angles,
        supplied_out_point: request.out_point,
        trig: "host f64 sin/cos rounded to f32; retail lookup table not supplied",
        frames,
    })
}
