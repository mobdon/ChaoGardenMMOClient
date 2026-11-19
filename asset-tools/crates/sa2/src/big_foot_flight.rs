//! Target selection for the first airborne pass (the takeoff callback clears bit 0x200).
use crate::{Error, Result};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Flight {
    extra_byte2: u8,
    ticks: Vec<Tick>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Tick {
    player_xz: [f32; 2],
    boss_x: f32,
    velocity_x: f32,
}

pub(super) fn run(Flight { extra_byte2, ticks }: Flight) -> Result<Value> {
    if ticks.len() > 2048 {
        return Err(Error::invalid("too many first-flight updates"));
    }
    let (mut mode, mut sub, mut counter) = (3, 0, extra_byte2);
    let mut frames = Vec::new();
    for t in ticks {
        let [px, pz] = t.player_xz;
        if mode != 3
            || ![px, pz, t.boss_x, t.velocity_x]
                .iter()
                .all(|v| v.is_finite())
        {
            return Err(Error::invalid("invalid first-flight update"));
        }
        let entered = sub == 0;
        let target = if sub <= 1 {
            sub = 1;
            if (px - t.boss_x).abs() < 10.0 {
                sub = 2;
                [px, 0.]
            } else {
                [px, pz.clamp(-90., 90.)]
            }
        } else {
            let right = t.velocity_x > 0.;
            if (right && t.boss_x > 120.) || (!right && t.boss_x < -120.) {
                mode = 4;
                sub = 0;
                counter = counter.wrapping_sub(1);
            }
            [if right { 120. } else { -120. }, 0.]
        };
        frames.push(
            json!({"mode":mode,"submode":sub,"targetXZ":target,"extraByte2":counter,
            "enableShape7Attribute16":entered,"callHelper55d78":mode==4}),
        );
    }
    Ok(json!(frames))
}
