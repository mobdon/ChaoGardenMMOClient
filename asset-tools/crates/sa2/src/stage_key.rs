//! Container-created Chao key, from the installed stage REL's falling-key executor.
use crate::{Error, Result};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    allocation_ok: bool,
    frames: Vec<Frame>,
    pickup: Option<crate::key_pickup::Request>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Frame {
    #[serde(default)]
    game_byte_44: i8,
    /// Only queried after 3600 or when the global byte is nonzero.
    within_800: Option<bool>,
    /// CCL result. Only queried when the key has settled.
    #[serde(default)]
    player_contact: bool,
}
pub(crate) fn replay(mut pos: [f32; 3], floor: f32, mut req: Request) -> Result<Value> {
    if req.frames.len() > 4096 {
        return Err(Error::invalid("expected at most 4096 key updates"));
    }
    if !req.allocation_ok {
        if !req.frames.is_empty() || req.pickup.is_some() {
            return Err(Error::invalid("unallocated key cannot update"));
        }
        return Ok(json!({"allocated":false,"frames":[]}));
    }
    let (mut speed, mut age, mut yaw, mut mode) = (2.5f32, 0, 0i32, 1u8);
    let mut frames = Vec::new();
    let mut pickup = None;
    for f in req.frames {
        let query = age > 3600 || f.game_byte_44 != 0;
        if query != f.within_800.is_some() {
            return Err(Error::invalid(
                "expected radius query result only on expiry or nonzero game byte",
            ));
        }
        let mut calls = vec![];
        if query {
            calls.push(json!("queryWithin800"));
        }
        let expired = query && (f.within_800 == Some(false) || f.game_byte_44 != 0);
        let mut finished = expired;
        if expired {
            if f.player_contact {
                return Err(Error::invalid("expired key cannot report a hit"));
            }
            calls.push(json!("freeTask"));
        } else {
            if !query {
                age += 1;
            }
            pos[1] += speed;
            if pos[1] <= floor {
                speed *= -0.6;
                pos[1] = floor;
                if speed.abs() < 0.3 {
                    mode = 2;
                }
            }
            speed = 0.985 * speed - 0.08;
            if mode == 2 {
                calls.push(json!("checkHitPlayer"));
            }
            if f.player_contact {
                if mode != 2 {
                    return Err(Error::invalid("airborne key does not query a player hit"));
                }
                speed = 2.5;
                mode = 1;
                calls.push(json!({"startPickupTask":pos}));
                if let Some(task) = req.pickup.take() {
                    pickup = Some(crate::key_pickup::replay(pos, f.game_byte_44, task)?);
                }

                calls.push(json!("freeTask")); // spawned by a container, no SET condition
                finished = true;
            } else {
                calls.push(json!("registerCollision"));
            }
            yaw = yaw.wrapping_add(512);
        }
        frames.push(json!({"age":age,"yaw":yaw,"mode":mode,"position":pos,"speed":speed,"calls":calls,"finished":finished}));
        if finished {
            break;
        }
    }
    if req.pickup.is_some() {
        return Err(Error::invalid("pickup replay requires key contact"));
    }
    let mut out = json!({"allocated":true,"initCollision":{"count":1,"id":6},"frames":frames});
    if let Some(task) = pickup {
        out["pickup"] = task;
    }
    Ok(out)
}
