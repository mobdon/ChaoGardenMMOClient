//! Target selection for the first airborne pass (the takeoff callback clears bit 0x200).
use crate::{Error, Result};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Flight {
    extra_byte2: u8,
    steering: Option<Steering>,
    ticks: Vec<Tick>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Tick {
    player_xz: [f32; 2],
    boss_x: f32,
    velocity_x: f32,
    motion: Option<Motion>,
}

pub(super) fn run(
    Flight {
        extra_byte2,
        ticks,
        mut steering,
    }: Flight,
    mut phase: [i32; 3],
) -> Result<Value> {
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
        let mut row = json!({"mode":mode,"submode":sub,"targetXZ":target,"extraByte2":counter,
            "enableShape7Attribute16":entered,"callHelper55d78":mode==4});
        match (&mut steering, t.motion) {
            (Some(state), Some(m)) => row["steering"] = state.step(&mut phase, t.velocity_x, m)?,
            (None, None) => (),
            _ => return Err(Error::invalid("supply motion on every steering update")),
        }
        frames.push(row);
    }
    Ok(json!(frames))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Steering {
    angles: [i32; 3],
    rates: [f32; 3],
    previous_speed_squared: f32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Motion {
    velocity_z: f32,
    acceleration: [f32; 3],
    yaw_sin_cos: [f32; 2],
    heading_difference: i32,
    turn_sine: Option<f32>,
}

impl Steering {
    fn step(&mut self, phase: &mut [i32; 3], vx: f32, m: Motion) -> Result<Value> {
        let mut acc = m.acceleration;
        if !self
            .rates
            .iter()
            .chain(acc.iter())
            .chain([&m.velocity_z, &self.previous_speed_squared])
            .all(|v| v.is_finite())
            || !m
                .yaw_sin_cos
                .iter()
                .chain(m.turn_sine.iter())
                .all(|v| (-1. ..=1.).contains(v))
        {
            return Err(Error::invalid("invalid first-flight motion observation"));
        }
        let [sin, cos] = m.yaw_sin_cos;
        acc[0] += 0.1 * sin;
        acc[2] += 0.1 * cos;
        acc[1] += 0.05;
        let velocity = [vx * 0.96, m.velocity_z * 0.96];
        let speed = velocity[0] * velocity[0] + velocity[1] * velocity[1];
        let speed_delta = speed - self.previous_speed_squared;
        self.previous_speed_squared = speed;
        for (angle, delta) in phase.iter_mut().zip([0x210, 0x250, 0x300]) {
            *angle = angle.wrapping_add(delta);
        }
        let turn = (m.heading_difference.wrapping_neg() as f32) * 2.;
        let sine = match (turn.abs() < 16384., m.turn_sine) {
            (true, Some(s)) => s,
            (false, None) => turn.signum(),
            _ => return Err(Error::invalid("turn_sine must match the native lookup")),
        };
        self.rates[1] += sine * 0.081 - 0.06 * self.rates[1];
        // The original computes the angle feedback in double precision, then rounds once.
        let feedback =
            |drive: f32, angle: i32| (f64::from(drive) - 0.0054931640625 * f64::from(angle)) as f32;
        self.rates[2] +=
            0.0026 * feedback(-16. * self.rates[1], self.angles[2]) - 0.045 * self.rates[2];
        self.rates[0] +=
            0.0035 * feedback(70. * speed_delta, self.angles[0]) - 0.08 * self.rates[0];
        for (angle, rate) in self.angles.iter_mut().zip(self.rates) {
            let step = 182.04445 * rate;
            if !(-2147483648. ..2147483648.).contains(&step) {
                return Err(Error::invalid("flight angular step out of range"));
            }
            *angle = angle.wrapping_add(step as i32);
        }
        Ok(
            json!({"velocityXZ":velocity,"acceleration":acc,"angles":self.angles,"rates":self.rates,"speedSquared":speed,"phase":phase}),
        )
    }
}
