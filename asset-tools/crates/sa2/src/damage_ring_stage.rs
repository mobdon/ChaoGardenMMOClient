//! Read-only stage adapter for an isolated damage ring. No native player/save mutation.
use crate::{
    collision,
    damage_ring::{self, DamageCall, DroppedRing, Phase, ProbeClock, Surface},
    stage, Error, Result,
};
use serde::Deserialize;
use serde_json::{json, Value};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    slot: u8,
    base_character: u8,
    rings: u16,
    position: [f32; 3],
    scatter_random: f32,
    ring_index: Option<usize>,
    spin_random: f32,
    frames: usize,
    probe_depth: f64,
}
pub fn replay(stage: &stage::Stage, request: &[u8]) -> Result<Value> {
    let r: Request = serde_json::from_slice(request)?;
    if r.frames > 422
        || !r.probe_depth.is_finite()
        || !(0.0..=100000.).contains(&r.probe_depth)
        || r.probe_depth == 0.
    {
        return Err(Error::invalid(
            "expected at most 422 frames and positive finite probe depth <=100000",
        ));
    }
    let calls = damage_ring::scatter(
        r.slot,
        r.base_character,
        r.rings,
        r.position,
        r.scatter_random,
    )?;
    let spawns = calls
        .iter()
        .filter_map(|c| match c {
            DamageCall::CreateDroppedRing { yaw, .. } => Some(*yaw),
            _ => None,
        })
        .collect::<Vec<_>>();
    let Some(index) = r.ring_index else {
        if r.frames != 0 {
            return Err(Error::invalid("flight frames require a spawned ring index"));
        }
        return Ok(json!({"calls":calls,"frames":[]}));
    };
    let yaw = *spawns
        .get(index)
        .ok_or_else(|| Error::invalid("ring index outside damage scatter"))?;
    let world = collision::bake_stage(stage, 1.)?;
    let mut clock = ProbeClock::default();
    for _ in 0..index {
        clock.next_delay();
    }
    let trig = |a: i32| ((a % 65536) as f32 * (std::f32::consts::TAU / 65536.)).sin_cos();
    let gravity = [0., -1., 0.];
    let mut ring = DroppedRing::launch(
        r.position,
        yaw,
        gravity,
        0,
        0,
        r.spin_random,
        false,
        &mut clock,
        trig,
    )?;
    let initial = serde_json::to_value(&ring)?;
    let mut frames = vec![];
    for _ in 0..r.frames {
        let mut query = None;
        let mut error = None;
        let event = ring.tick(
            gravity,
            false,
            false,
            None,
            |p, _| {
                match world.raycast(p.map(f64::from), [0., -1., 0.], r.probe_depth) {
                    Ok(hit) => {
                        // No conversion from arbitrary triangle normals to native floor angles is verified yet.
                        if hit
                            .as_ref()
                            .is_some_and(|h| h.normal[0] != 0. || h.normal[2] != 0.)
                        {
                            error = Some(Error::invalid(
                                "sloped static hit requires native floor angles",
                            ));
                            return None;
                        }
                        let surface = hit.as_ref().map(|h| Surface {
                            coordinate: h.position[1] as f32,
                            pitch: 0,
                            roll: 0,
                        });
                        query = Some(json!({"origin":p,"hit":hit}));
                        surface
                    }
                    Err(e) => {
                        error = Some(e);
                        None
                    }
                }
            },
            trig,
        );
        if let Some(e) = error {
            return Err(e);
        }
        frames.push(json!({"task":ring,"events":event,"staticProbe":query}));
        if ring.phase == Phase::Destroyed {
            break;
        }
    }
    Ok(
        json!({"calls":calls,"initial":initial,"frames":frames,"scope":"One dropped ring. Static down-ray adapter on horizontal terrain with supplied launch position/randomness and a fresh shared probe clock. No moving surfaces, player contacts, native broad phase or save writes."}),
    )
}
