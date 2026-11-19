//! Big Foot flight passes and turnarounds from the native takeoff up to entry into mode 5.
#[path = "big_foot_aim.rs"]
mod aim;
#[path = "big_foot_landing.rs"]
mod landing;
#[path = "big_foot_recovery.rs"]
mod recovery;
#[path = "big_foot_volley.rs"]
mod volley;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Flight {
    #[serde(default)]
    repeat_passes: Vec<Pass>,
    aim: Option<Vec<aim::Tick>>,
    landing: Option<Vec<landing::Tick>>,
    recovery: Option<Vec<recovery::Tick>>,
    next_takeoff: Option<super::takeoff::Restart>,
    ground_volley: Option<[volley::Launch; 8]>,
    center_approach: Option<Vec<CenterTick>>,
    extra_byte2: Option<u8>,
    steering: Option<Steering>,
    ticks: Vec<Tick>,
    turnaround: Option<Vec<TurnTick>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pass {
    ticks: Vec<Tick>,
    turnaround: Option<Vec<TurnTick>>,
}

pub(super) struct Trace {
    pub first: Value,
    pub turnaround: Option<Value>,
    pub repeats: Vec<Value>,
    pub center: Option<Value>,
    pub aim: Option<Value>,
    pub landing: Option<Value>,
    pub volley: Option<Value>,
    pub recovery: Option<Value>,
    pub next_takeoff: Option<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Tick {
    helper_angles: Option<[i32; 3]>,
    player_xz: [f32; 2],
    boss_x: f32,
    velocity_x: f32,
    motion: Option<Motion>,
}

pub(super) fn run(
    Flight {
        extra_byte2,
        repeat_passes,
        center_approach,
        landing,
        ground_volley,
        recovery,
        next_takeoff,
        aim,
        ticks,
        mut steering,
        turnaround,
    }: Flight,
    super::takeoff::Handoff {
        mut phase,
        mut flags,
        pass_count,
    }: super::takeoff::Handoff,
    motion4_duration: u16,
    spawn_order: &[u8; 16],
) -> Result<Trace> {
    if repeat_passes.len() > 2 {
        return Err(Error::invalid("at most two repeat passes follow takeoff"));
    }
    if extra_byte2.is_some_and(|v| v != pass_count) {
        return Err(Error::invalid(
            "extra_byte2 disagrees with the native takeoff",
        ));
    }
    let (mut mode, mut sub, mut counter) = (3, 0, pass_count);
    let mut trace = Trace {
        first: Value::Null,
        turnaround: None,
        repeats: Vec::new(),
        center: None,
        aim: None,
        landing: None,
        volley: None,
        recovery: None,
        next_takeoff: None,
    };
    for (index, Pass { ticks, turnaround }) in std::iter::once(Pass { ticks, turnaround })
        .chain(repeat_passes)
        .enumerate()
    {
        let armed = flags & 0x200 != 0;
        if mode != 3
            || sub != 0
            || counter != pass_count - index as u8
            || armed != (index != 0)
            || ticks.len() > 2048
        {
            return Err(Error::invalid(
                "pass requires the previous turnaround handoff",
            ));
        }
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
            let sweeping = sub == 2;
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
            let mut calls = Vec::new();
            if armed {
                if entered {
                    calls.extend([
                        json!("create55248"),
                        json!("attach5649c"),
                        json!({"helperEnabled":1}),
                    ]);
                }
                if sweeping
                    && ((t.velocity_x > 0. && px < t.boss_x)
                        || (t.velocity_x <= 0. && px > t.boss_x))
                {
                    calls.push(json!({"helperEnabled":0}));
                }
                if mode == 4 {
                    calls.push(json!("freeHelper"));
                }
            }
            if armed && mode != 4 {
                let [pitch, yaw, roll] = t
                    .helper_angles
                    .ok_or_else(|| Error::invalid("missing helper angles on active repeat pass"))?;
                calls.push(
                    json!({"helperAngles":[pitch,yaw,roll.wrapping_add(0x1000).clamp(-3072,6144)]}),
                );
            } else if t.helper_angles.is_some() {
                return Err(Error::invalid("unexpected helper angles"));
            }
            if armed {
                row["helperCalls"] = json!(calls);
            }
            match (&mut steering, t.motion) {
                (Some(state), Some(m)) => {
                    row["steering"] = json!(state.step(
                        &mut phase,
                        t.velocity_x,
                        m,
                        if armed {
                            FlightLaw::Repeat
                        } else {
                            FlightLaw::First
                        },
                    )?)
                }
                (None, None) => (),
                _ => return Err(Error::invalid("supply motion on every steering update")),
            }
            frames.push(row);
        }
        let mut turn_output = None;
        if let Some(turnaround) = turnaround {
            if mode != 4 || turnaround.len() > 2048 {
                return Err(Error::invalid(
                    "turnaround requires the arena edge handoff and at most 2048 updates",
                ));
            }
            let mut target = [0.; 2];
            let mut turned = Vec::new();
            for t in turnaround {
                let [x, z] = t.boss_xz;
                if mode != 4 || ![x, z, t.velocity_x].iter().all(|v| v.is_finite()) {
                    return Err(Error::invalid("invalid turnaround update"));
                }
                if (sub == 0) != t.player_xz.is_some() {
                    return Err(Error::invalid("turnaround reads the player only on entry"));
                }
                if let Some([px, pz]) = t.player_xz {
                    if !px.is_finite() || !pz.is_finite() {
                        return Err(Error::invalid("invalid player position"));
                    }
                    target = [
                        px,
                        if (z > 48. && pz > z) || (z < -48. && pz < z) {
                            0.9 * z
                        } else {
                            pz
                        },
                    ];
                    sub = 1;
                }
                if sub == 1 {
                    if (x > 0. && t.velocity_x < 0.) || (x <= 0. && t.velocity_x > 0.) {
                        sub = 2;
                    }
                } else if x.abs() < 150. {
                    if counter == 0 {
                        mode = 5;
                    } else {
                        mode = 3;
                        flags |= 0x200;
                    }
                    sub = 0;
                }
                let mut row = json!({"mode":mode,"submode":sub,"targetXZ":target,"flags":flags});
                match (&mut steering, t.motion) {
                    (Some(state), Some(m)) => {
                        row["steering"] =
                            json!(state.step(&mut phase, t.velocity_x, m, FlightLaw::Turnaround)?)
                    }
                    (None, None) => (),
                    _ => return Err(Error::invalid("supply motion on every steering update")),
                }
                turned.push(row);
            }
            turn_output = Some(json!(turned));
        }
        if index == 0 {
            trace.first = json!(frames);
            trace.turnaround = turn_output;
        } else {
            trace
                .repeats
                .push(json!({"pass":frames,"turnaround":turn_output}));
        }
    }
    let mut landing_entry = None;
    if let Some(ticks) = center_approach {
        if mode != 5 || sub != 0 || counter != 0 || ticks.len() > 2048 {
            return Err(Error::invalid(
                "center approach requires the third turnaround",
            ));
        }
        let state = steering
            .as_mut()
            .ok_or_else(|| Error::invalid("center approach requires continuous steering"))?;
        let (rows, handoff) = center(ticks, state, phase, flags)?;
        trace.center = Some(rows);
        if let Some(ticks) = aim {
            let entry = handoff.ok_or_else(|| Error::invalid("aim requires both center gates"))?;
            let (rows, next) = aim::run(entry, ticks, motion4_duration)?;
            trace.aim = Some(rows);
            landing_entry = next;
        }
    } else if aim.is_some() {
        return Err(Error::invalid("aim requires center approach"));
    }
    let mut recovery_flags = None;
    if let Some(ticks) = landing {
        let (rows, next) = landing::run(
            landing_entry
                .ok_or_else(|| Error::invalid("landing requires completed aim countdown"))?,
            ticks,
            spawn_order,
        )?;
        trace.landing = Some(rows);
        if let Some(observations) = ground_volley {
            let (rows, flags) = volley::run(
                next.ok_or_else(|| {
                    Error::invalid("ground volley requires finished landing animation")
                })?,
                observations,
            )?;
            trace.volley = Some(rows);
            recovery_flags = Some(flags);
        }
    } else if ground_volley.is_some() {
        return Err(Error::invalid("ground volley requires landing"));
    }
    if let Some(ticks) = recovery {
        let (rows, restart) = recovery::run(
            recovery_flags
                .ok_or_else(|| Error::invalid("recovery requires the completed ground volley"))?,
            ticks,
        )?;
        trace.recovery = Some(rows);
        if let Some(input) = next_takeoff {
            trace.next_takeoff =
                Some(input.run(restart.ok_or_else(|| {
                    Error::invalid("next takeoff requires completed recovery")
                })?)?);
        }
    } else if next_takeoff.is_some() {
        return Err(Error::invalid("next takeoff requires recovery"));
    }
    Ok(trace)
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

enum FlightLaw {
    First,
    Repeat,
    Turnaround,
    CenterOuter,
    CenterInner,
}

#[derive(Serialize)]
struct FlightMotion {
    #[serde(rename = "velocityXZ")]
    velocity: [f32; 2],
    acceleration: [f32; 3],
    angles: [i32; 3],
    rates: [f32; 3],
    #[serde(rename = "speedSquared")]
    speed_squared: f32,
    phase: [i32; 3],
}

impl Steering {
    fn step(
        &mut self,
        phase: &mut [i32; 3],
        vx: f32,
        m: Motion,
        law: FlightLaw,
    ) -> Result<FlightMotion> {
        let (thrust, damping, turn_gain) = match law {
            FlightLaw::CenterOuter => (0.1, 0.93, 0.07),
            FlightLaw::CenterInner => (0.07, 0.96, 0.07),
            FlightLaw::First => (0.1, 0.96, 0.081),
            FlightLaw::Repeat => (0.1, 0.94, 0.07),
            FlightLaw::Turnaround => (0.09, 0.92, 0.081),
        };
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
        acc[0] += thrust * sin;
        acc[2] += thrust * cos;
        acc[1] += 0.05;
        let velocity = [vx * damping, m.velocity_z * damping];
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
        self.rates[1] += sine * turn_gain - 0.06 * self.rates[1];
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
        Ok(FlightMotion {
            velocity,
            acceleration: acc,
            angles: self.angles,
            rates: self.rates,
            speed_squared: speed,
            phase: *phase,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TurnTick {
    boss_xz: [f32; 2],
    velocity_x: f32,
    player_xz: Option<[f32; 2]>,
    motion: Option<Motion>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CenterTick {
    boss_xz: [f32; 2],
    velocity_x: f32,
    motion: Motion,
    slot_zero_heading: Option<i32>,
}

fn center(
    ticks: Vec<CenterTick>,
    state: &mut Steering,
    mut phase: [i32; 3],
    mut flags: u16,
) -> Result<(Value, Option<aim::Entry>)> {
    let mut handoff = None;
    let mut sub = 0;
    let mut rows = Vec::new();
    for t in ticks {
        let [x, z] = t.boss_xz;
        let squared = x * x + z * z;
        if sub == 3 || !squared.is_finite() || !t.velocity_x.is_finite() {
            return Err(Error::invalid("invalid center approach update"));
        }
        if sub == 0 {
            flags &= !0x400;
            sub = 1;
        }
        let inner = sub == 2;
        if inner {
            // This submode advances all three phases before calling the common steering helper.
            for (p, delta) in phase.iter_mut().zip([0x210, 0x250, 0x300]) {
                *p = p.wrapping_add(delta);
            }
        }
        let mut movement = state.step(
            &mut phase,
            t.velocity_x,
            t.motion,
            if inner {
                FlightLaw::CenterInner
            } else {
                FlightLaw::CenterOuter
            },
        )?;
        let inside = squared.sqrt() < if inner { 90. } else { 170. };
        let clear_shape = !inner && inside;
        let mut aim = None;
        if inner && inside {
            let heading = t.slot_zero_heading.ok_or_else(|| {
                Error::invalid("center approach needs the native slot-zero heading")
            })?;
            movement.acceleration[0] = 0.;
            movement.acceleration[2] = 0.;
            let target = [state.angles[0].wrapping_sub(0x400), heading];
            aim = Some(target);
            handoff = Some(aim::Entry {
                angles: state.angles,
                rates: state.rates,
                phase,
                target,
                flags,
            });
        } else if t.slot_zero_heading.is_some() {
            return Err(Error::invalid("unexpected slot-zero heading"));
        }
        if inside {
            sub += 1;
        }
        rows.push(json!({"submode":sub,"flags":flags,"targetXZ":[0.,0.],"clearShape7Attribute16":clear_shape,"aimPitchYaw":aim,"steering":movement}));
    }
    Ok((json!(rows), handoff))
}
