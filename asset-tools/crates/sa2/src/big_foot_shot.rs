//! One launched child's position and impact dispatch. Matrix results come from the engine.
use crate::{Error, Result};
use serde::Deserialize;
use serde_json::{json, Value};
type Vec3 = [f32; 3];
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Replay {
    anchor_position: Vec3,
    ticks: Vec<Tick>,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Tick {
    Paused,
    Active {
        rotated_vector: Option<Vec3>,
        atan_results: [f64; 2],
        hit_player: bool,
        hit_other: bool,
        effect_allocations: Option<Vec<bool>>,
    },
}
pub(super) fn run(replay: Replay, target: Vec3, angles: [i32; 3], offset: i32) -> Result<Value> {
    if replay.ticks.len() > 2048 {
        return Err(Error::invalid("too many projectile updates"));
    }
    let mut state = State::launch(replay.anchor_position, target, angles, offset)?;
    let rows = replay
        .ticks
        .into_iter()
        .map(|tick| state.update(tick))
        .collect::<Result<Vec<_>>>()?;
    Ok(json!(rows))
}

#[derive(Clone, Copy)]
pub(super) struct State {
    p: Vec3,
    target: Vec3,
    angles: [i32; 3],
    offset: i32,
    sub: u8,
    timer: u16,
    t: f32,
    step: f32,
    v: Vec3,
    cp: [Vec3; 4],
    removed: bool,
}
impl State {
    pub(super) fn launch(p: Vec3, target: Vec3, angles: [i32; 3], offset: i32) -> Result<Self> {
        if !p.iter().chain(&target).all(|n| n.is_finite()) {
            return Err(Error::invalid("invalid projectile position"));
        }
        Ok(Self {
            p,
            target,
            angles,
            offset,
            sub: 0,
            timer: 0,
            t: 0.,
            step: 0.,
            v: [0.; 3],
            cp: [[0.; 3]; 4],
            removed: false,
        })
    }
    pub(super) fn update(&mut self, tick: Tick) -> Result<Value> {
        let Self {
            mut p,
            target,
            mut angles,
            offset,
            mut sub,
            mut timer,
            mut t,
            mut step,
            mut v,
            mut cp,
            mut removed,
        } = *self;
        if removed {
            return Err(Error::invalid("shot update after removal request"));
        }
        let mut events = Vec::new();
        let mut impacts = Vec::new();
        let mut atan_args = None;
        let mut rotation = None;
        if let Tick::Active {
            rotated_vector,
            atan_results,
            hit_player,
            hit_other,
            effect_allocations,
        } = tick
        {
            let transforming = sub == 0 || (sub == 1 && timer == 0);
            if transforming != rotated_vector.is_some()
                || rotated_vector.iter().flatten().any(|v| !v.is_finite())
            {
                return Err(Error::invalid("invalid shot matrix observation"));
            }
            let before = p;
            if sub == 0 {
                let dir = rotated_vector.unwrap();
                rotation = Some((angles[1].wrapping_add(0x4000), [-1., 0., 0.]));
                v = [dir[0] * (0.8 * 1.8), 0., dir[2] * (0.8 * 1.8)];
                timer = 5;
                sub = 1;
            }
            if sub == 1 {
                p[0] += v[0];
                p[2] += v[2];
                let old = timer;
                timer = timer.wrapping_sub(1);
                if old == 0 {
                    let d: Vec3 = std::array::from_fn(|i| target[i] - p[i]);
                    rotation = Some((offset, d.map(|n| 0.5 * n)));
                    let transformed = rotated_vector.unwrap();
                    let b: Vec3 = std::array::from_fn(|i| transformed[i] + p[i]);
                    cp = [
                        p,
                        b,
                        std::array::from_fn(|i| b[i] - 0.5 * (b[i] - target[i])),
                        target,
                    ];
                    let length = (d[2] * d[2] + (d[0] * d[0] + d[1] * d[1])).sqrt();
                    step = 1.8 * (1. / length);
                    if !step.is_finite() || step <= 0. {
                        return Err(Error::invalid("unrepresentable shot curve"));
                    }
                    sub = 2;
                }
            } else if t <= 1. {
                let u = 1. - t;
                let a = u * (u * u);
                let b = t * ((3. * u) * u);
                let c = (3. * u) * (t * t);
                let d = t * (t * t);
                p = std::array::from_fn(|i| {
                    cp[3][i] * d + (cp[2][i] * c + (cp[0][i] * a + cp[1][i] * b))
                });
                t += step;
                if t > 1. {
                    v = std::array::from_fn(|i| p[i] - before[i]);
                }
            } else {
                for (n, d) in p.iter_mut().zip(v) {
                    *n += d;
                }
            }
            if !p.iter().all(|n| n.is_finite()) {
                return Err(Error::invalid("shot position overflow"));
            }
            let delta: Vec3 = std::array::from_fn(|i| p[i] - before[i]);
            atan_args = Some([
                [delta[0], delta[2]],
                [delta[1], (delta[0] * delta[0] + delta[2] * delta[2]).sqrt()],
            ]);
            if atan_results
                .iter()
                .any(|v| !v.is_finite() || v.abs() > std::f64::consts::PI)
            {
                return Err(Error::invalid("invalid shot angle observation"));
            }
            angles[1] = (10430.38043493439 * atan_results[0]) as i32;
            angles[0] = -((10430.38043493439 * atan_results[1]) as i32);
            if p[1] <= 0. {
                events.push("ground");
            }
            if (p[0] * p[0] + p[2] * p[2]).sqrt() > 160. {
                events.push("radius");
            }
            if hit_player {
                events.push("player");
            }
            if hit_other {
                events.push("other");
            }
            if effect_allocations
                .as_ref()
                .is_some_and(|v| v.len() != events.len())
            {
                return Err(Error::invalid(
                    "expected one effect allocation result per impact",
                ));
            }
            for i in 0..events.len() {
                let mut impact = json!({"position":p,"engineArgs":[0x4019,0,0,60],"effectKind":4,"freeChild":true});
                if let Some(outcomes) = &effect_allocations {
                    impact["effectTask"] = if outcomes[i] {
                        json!({"kind":4,"position":p,"mainOffset":0x42458,"displayOffset":266696,
                        "destructorOffset":271444,"work10":0.0,"work24":0.34_f32,"work38":1.0,"requestResources":true})
                    } else {
                        Value::Null
                    };
                }
                impacts.push(impact);
            }
            removed = !events.is_empty();
            events.push("register");
        }
        *self = Self {
            p,
            target,
            angles,
            offset,
            sub,
            timer,
            t,
            step,
            v,
            cp,
            removed,
        };
        Ok(
            json!({"position":p,"velocity":v,"timer":timer,"submode":sub,"curveTime":t,"curveStep":step,"rotation":rotation,"events":events,"angles":angles,"atanArgs":atan_args,"impacts":impacts}),
        )
    }
}
