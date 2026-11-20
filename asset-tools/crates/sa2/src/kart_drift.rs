//! CartD state 2 drift windup. Engine math is a projection; task arithmetic is f32.
use crate::{kart_progress, Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    sustained: Option<Sustained>,
    heading: i32,
    work_68: i32,
    work_a4: i32,
    work_a8: i32,
    position: [f32; 3],
    velocity: [f32; 3],
    accelerate: bool,
    work_74: i32,
    drift_handling: f32,
    weight: f32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Sustained {
    stick_x: i16,
    speed_threshold: f32,
    work_fc: i32,
    angle_service_return: Option<i32>,
}
#[derive(Serialize)]
pub struct Update {
    #[serde(skip_serializing_if = "Option::is_none")]
    work_fc: Option<i32>,
    heading: i32,
    pub(crate) work_68: i32,
    work_a4: i32,
    vehicle_state: i32,
    local_velocity: [f32; 3],
    pub(crate) position: [f32; 3],
    math_source: &'static str,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoryTick {
    pub(crate) contact: Option<DriveContact>,
    pub(crate) audio: Option<DriftAudio>,
    heading: Option<i32>,
    position: Option<[f32; 3]>,
    velocity: [f32; 3],
    work_74: i32,
    buttons_held: Option<u32>,
    work_fc: Option<i32>,
    angle_service_return: Option<i32>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DriveContact {
    pub(crate) query: crate::kart_contact::StoryContact,
    pub(crate) work_28c: i32,
    pub(crate) work_44: i32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DriftAudio {
    pub(crate) work_44: i32,
    work_284: i32,
    pub(crate) work_28c: i32,
    piece_id: Option<u8>,
    magnitude_return: Option<f32>,
}
impl DriftAudio {
    pub(crate) fn run(
        self,
        rel: &crate::city_rel::CityRel,
        mode: i32,
        motion: &Update,
        velocity: [f32; 3],
        held: u32,
    ) -> Result<crate::kart_driving_audio::Output> {
        crate::kart_driving_audio::tick_verified(
            rel,
            crate::kart_driving_audio::Input {
                work_44: self.work_44,
                vehicle_state: motion.vehicle_state,
                position: motion.position,
                velocity_xz: [velocity[0], velocity[2]],
                controls: crate::kart_driving_audio::Controls {
                    profile: mode as usize,
                    alternate: false,
                    work_284: self.work_284,
                    work_110: i32::from(held & 4 != 0),
                    work_114: i32::from(held & 0x402 != 0 && self.work_28c <= 10),
                    piece_id: self.piece_id,
                    magnitude_return: self.magnitude_return,
                },
            },
        )
    }
}
impl StoryTick {
    pub(crate) fn held(&self, sampled: Option<u32>) -> Result<u32> {
        if sampled.zip(self.buttons_held).is_some_and(|(a, b)| a != b) {
            return Err(Error::invalid("drift buttons conflict with controller"));
        }
        sampled
            .or(self.buttons_held)
            .ok_or_else(|| Error::invalid("missing held buttons for drift windup"))
    }
    pub(crate) fn bind_task_pose(&mut self, position: [f32; 3], yaw: i32) -> Result<()> {
        if self.heading.is_some_and(|v| v != yaw)
            || self
                .position
                .is_some_and(|p| p.map(f32::to_bits) != position.map(f32::to_bits))
        {
            return Err(Error::invalid(
                "drift task pose conflicts with preceding vehicle frame",
            ));
        }
        self.position = Some(position);
        self.heading = Some(yaw);
        Ok(())
    }
    pub(crate) fn world_velocity(&self) -> [f32; 3] {
        self.velocity
    }
}
pub(crate) struct DriftCarry {
    work_fc: Option<i32>,
    angular_offset: i32,
    index: i32,
    entry_stick: i32,
    tuning: [f32; 3],
}
impl DriftCarry {
    pub(crate) fn new(
        (angular_offset, index, entry_stick): (i32, i32, i32),
        tuning: [f32; 3],
    ) -> Self {
        Self {
            work_fc: None,
            angular_offset,
            index,
            entry_stick,
            tuning,
        }
    }
    pub(crate) fn advance(
        &mut self,
        bytes: &[u8],
        s: StoryTick,
        held: u32,
        stick: Option<i16>,
        state: i32,
    ) -> Result<Update> {
        let sustained = if state == 3 {
            if s.work_fc.zip(self.work_fc).is_some_and(|(a, b)| a != b) {
                return Err(Error::invalid("conflicting sustained drift rotation"));
            }
            Some(Sustained {
                stick_x: stick
                    .ok_or_else(|| Error::invalid("sustained drift requires controller sample"))?,
                speed_threshold: self.tuning[2],
                work_fc: self
                    .work_fc
                    .or(s.work_fc)
                    .ok_or_else(|| Error::invalid("missing initial sustained drift rotation"))?,
                angle_service_return: s.angle_service_return,
            })
        } else {
            if s.work_fc.is_some() || s.angle_service_return.is_some() {
                return Err(Error::invalid(
                    "unexpected sustained drift inputs during windup",
                ));
            }
            None
        };
        let next = tick(
            bytes,
            Input {
                sustained,
                heading: s
                    .heading
                    .ok_or_else(|| Error::invalid("missing drift task heading"))?,
                position: s
                    .position
                    .ok_or_else(|| Error::invalid("missing drift task position"))?,
                velocity: s.velocity,
                work_68: self.angular_offset,
                work_a4: self.index,
                work_a8: self.entry_stick,
                work_74: s.work_74,
                accelerate: held & 4 != 0,
                drift_handling: self.tuning[0],
                weight: self.tuning[1],
            },
        )?;
        self.angular_offset = next.work_68;
        self.index = next.work_a4;
        self.work_fc = next.work_fc;
        Ok(next)
    }
}
impl Update {
    pub(crate) fn state(&self) -> i32 {
        self.vehicle_state
    }
}
fn rotate([x, y, z]: [f32; 3], angle: i32) -> [f32; 3] {
    let (s, c) = (f64::from(angle as u16) * (std::f64::consts::TAU / 65536.)).sin_cos();
    [
        (c * f64::from(x) + s * f64::from(z)) as f32,
        y,
        (-s * f64::from(x) + c * f64::from(z)) as f32,
    ]
}
pub fn tick(bytes: &[u8], s: Input) -> Result<Update> {
    kart_progress::verify(bytes)?;
    if s.sustained.is_none() && !(0..5).contains(&s.work_a4) {
        return Err(Error::invalid("state 2 requires windup index 0 through 4"));
    }
    let rel = crate::city_rel::CityRel::read_identified(bytes, 80)?;
    let data = rel.data(5)?;
    let constants = rel.data(4)?;
    let f = |i: usize| f32::from_be_bytes(constants[i..i + 4].try_into().unwrap());
    let [x, y, z] = s.velocity;
    let heading = if x.abs() >= f(0x21c) || z.abs() >= f(0x21c) {
        let scale = f64::from_be_bytes(constants[0x208..0x210].try_into().unwrap());
        (-((f64::from(z).atan2(f64::from(x)) * scale) as i32 + 0x4000)) & 0xffff
    } else {
        s.heading
    };
    let offset = if let Some(ctx) = &s.sustained {
        let turn = (f(0x218) * (f(0x260) * f32::from(ctx.stick_x))) as i32;
        s.heading
            .wrapping_add(s.work_68)
            .wrapping_sub(heading)
            .wrapping_sub(turn)
    } else {
        let at = 0x3be4 + s.work_a4 as usize * 4;
        let amount = i32::from_be_bytes(data[at..at + 4].try_into().unwrap());
        if s.work_a8 < 0 {
            s.work_68.wrapping_add(amount)
        } else {
            s.work_68.wrapping_sub(amount)
        }
    };
    let angle = heading.wrapping_add(offset);
    let mut v = rotate([x, 0., z], angle.wrapping_neg());
    let mut speed = f64::from(v[0]).hypot(f64::from(v[2])) as f32;
    v[2] *= s.drift_handling;
    if !s.accelerate {
        speed *= f(0x25c);
    }
    let norm = f64::from(v[0]).hypot(f64::from(v[2]));
    if norm != 0. {
        v[0] = (f64::from(v[0]) / norm) as f32 * speed;
        v[2] = (f64::from(v[2]) / norm) as f32 * speed;
    }
    let delta = rotate(v, angle);
    let mut position = std::array::from_fn(|i| s.position[i] + delta[i]);
    position[1] += y - if y > 0. && s.work_74 != 0 {
        s.weight * f(0x234)
    } else {
        s.weight
    };
    if !position.iter().all(|x| x.is_finite()) {
        return Err(Error::invalid("nonfinite kart drift projection"));
    }
    let (vehicle_state, work_a4, work_fc) = if let Some(ctx) = s.sustained {
        let counter = if v[0].abs() < f(0x22c) {
            s.work_a4.wrapping_add(1)
        } else {
            0
        };
        let magnitude = (x as f64).hypot(y as f64).hypot(z as f64) as f32;
        let factor = f64::from_be_bytes(constants[0x268..0x270].try_into().unwrap());
        let exit = counter >= 20
            || s.velocity == [0.; 3]
            || f64::from(magnitude) < factor * f64::from(ctx.speed_threshold);
        let fc = if s.work_74 != 0 {
            ctx.angle_service_return
                .ok_or_else(|| Error::invalid("missing drift angle service return"))?
        } else {
            ctx.work_fc.wrapping_add(91)
        };
        (if exit { 0 } else { 3 }, counter, Some(fc))
    } else if s.work_a4 == 4 {
        (3, 0, None)
    } else {
        (2, s.work_a4 + 1, None)
    };
    Ok(Update {
        heading,
        work_68: offset,
        work_a4,
        vehicle_state,
        work_fc,
        local_velocity: v,
        position,
        math_source: "mathematical_projection",
    })
}
