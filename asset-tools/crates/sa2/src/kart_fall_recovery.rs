//! CartD vehicle fall callback, text 0x4b50..0x4eb0.
use crate::{city_rel::CityRel, kart_progress, Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Vehicle {
    position: [f32; 3],
    angles: [i32; 3],
    task_mode: u8,
    saved_position: [f32; 3],
    saved_angles: [i32; 3],
    grounded_position: [f32; 3],
    velocity: [f32; 3],
    work_68: i32,
    work_fc_104: [i32; 3],
    state: i32,
    segment: i32,
    lap: i32,
    work_cc: i32,
    work_254: i32,
    work_25c: i32,
    timer: i32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Vertex {
    segment: i32,
    corner: i32,
    xz: [f32; 2],
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    vehicle: Vehicle,
    work_74: i32,
    work_44: i32,
    work_260: i32,
    mode: i32,
    engine_halfword_64: Option<i16>,
    fall_height: Option<f32>,
    segment_count: Option<i32>,
    vertex: Option<Vertex>,
    angle_radians: Option<f64>,
}
#[derive(Serialize)]
pub struct Output {
    #[serde(skip)]
    course_length: i32,
    vehicle: Vehicle,
    engine_call_8001ef18: Option<i32>,
}

pub fn tick(bytes: &[u8], s: Input) -> Result<Output> {
    run(bytes, s, None)
}
pub fn course_tick(bytes: &[u8], exe: &[u8], name: &str, s: Input) -> Result<Output> {
    if s.fall_height.is_some() || s.segment_count.is_some() || s.vertex.is_some() {
        return Err(Error::invalid(
            "course supplies fall height, segment count and vertex",
        ));
    }
    let directory = kart_progress::read_crossings(bytes, exe)?;
    let route = directory
        .courses
        .iter()
        .find(|r| r.course == name)
        .ok_or_else(|| Error::invalid("unknown recovery course"))?;
    run(
        bytes,
        Input {
            fall_height: Some(route.fall_height),
            ..s
        },
        Some(&route.projected_segment_xz),
    )
}
fn run(bytes: &[u8], s: Input, route: Option<&[[[f32; 2]; 4]]>) -> Result<Output> {
    kart_progress::verify(bytes)?;
    let count = route
        .map(|r| r.len() as i32)
        .or(s.segment_count)
        .ok_or_else(|| Error::invalid("missing recovery segment count"))?;
    let rel = CityRel::read_identified(bytes, 80)?;
    let data = rel.data(4)?;
    let f = |n: usize| f32::from_be_bytes(data[n..n + 4].try_into().unwrap());
    let mut v = s.vehicle;
    let mut call = None;
    if s.work_74 != 0 {
        v.grounded_position = v.position;
    }
    if v.position[1]
        < s.fall_height
            .ok_or_else(|| Error::invalid("missing fall height"))?
    {
        let story = matches!(s.mode, 0 | 1);
        if story {
            if s.engine_halfword_64
                .ok_or_else(|| Error::invalid("missing engine halfword +0x64"))?
                < 1
            {
                v.state = 10;
                return Ok(Output {
                    course_length: count,
                    vehicle: v,
                    engine_call_8001ef18: Some(8),
                });
            }
            call = Some(4);
        } else {
            v.timer = 181;
        }
        let guided = s.work_44 == 0;
        if !guided {
            if v.timer == 1 {
                return Ok(Output {
                    course_length: count,
                    vehicle: v,
                    engine_call_8001ef18: call,
                });
            }
            let prior = v.timer;
            v.timer = v.timer.wrapping_add(1);
            if prior <= 180 {
                v.position = v.saved_position;
                v.angles = v.saved_angles;
                return Ok(Output {
                    course_length: count,
                    vehicle: v,
                    engine_call_8001ef18: call,
                });
            }
            v.segment = v.segment.wrapping_sub(5);
            if v.segment < 0 {
                if story {
                    v.segment = 1;
                } else {
                    v.segment = v.segment.wrapping_add(count);
                    v.lap = v.lap.wrapping_sub(1);
                    v.work_cc = 2;
                }
            }
        }
        let (seg, corner, height) = if guided {
            (v.work_25c.wrapping_sub(5), s.work_260, 0x28c)
        } else {
            (v.segment, 1, 0x294)
        };
        let xz = if let Some(route) = route {
            *route
                .get(seg as usize)
                .and_then(|quad| quad.get(corner as usize))
                .ok_or_else(|| Error::invalid("recovery address outside projected course"))?
        } else {
            let pt = s
                .vertex
                .ok_or_else(|| Error::invalid("missing selected route vertex"))?;
            if (pt.segment, pt.corner) != (seg, corner) {
                return Err(Error::invalid("wrong recovery route vertex"));
            }
            pt.xz
        };
        v.position = [xz[0], f(height) + v.grounded_position[1], xz[1]];
        v.saved_position = v.position;
        v.saved_position[1] += f(0x27c);
        v.velocity = [v.velocity[0] / f(0x290), f(0x1c8), v.velocity[2] / f(0x290)];
        v.work_68 = 0;
        if guided {
            v.work_254 = v.work_25c.wrapping_sub(4);
            v.work_25c = seg;
            v.task_mode = 0;
            if v.velocity[2].abs() >= f(0x21c) || v.velocity[0].abs() >= f(0x21c) {
                let r = s
                    .angle_radians
                    .ok_or_else(|| Error::invalid("missing recovery heading return"))?;
                let u = r * f64::from_be_bytes(data[0x208..0x210].try_into().unwrap());
                if !(-2147483648.0..2147483648.0).contains(&u) {
                    return Err(Error::invalid("unsupported heading"));
                }
                v.angles[1] = (u as i32).wrapping_add(0x4000).wrapping_neg() & 0xffff;
            }
        } else {
            v.work_fc_104 = [0; 3];
            v.timer = 0;
        }
    }
    Ok(Output {
        course_length: count,
        vehicle: v,
        engine_call_8001ef18: call,
    })
}

/// Fields untouched by the two preceding contact callbacks.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContactRecovery {
    contact: crate::kart_contact::Input,
    #[serde(flatten)]
    fields: RecoveryFields,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecoveryFields {
    task_mode: u8,
    saved_angles: Option<[i32; 3]>,
    grounded_position: [f32; 3],
    work_68: i32,
    work_fc_104: [i32; 3],
    segment: Option<i32>,
    lap: Option<i32>,
    work_cc: i32,
    work_254: i32,
    work_25c: i32,
    timer: i32,
    work_260: i32,
    engine_halfword_64: Option<i16>,
    angle_radians: Option<f64>,
}
#[derive(Serialize)]
pub struct ShortUpdate {
    contact: crate::kart_contact::Update,
    recovery: Output,
    #[serde(skip_serializing_if = "Option::is_none")]
    impact: Option<Impact>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pre_contact_audio: Option<crate::kart_driving_audio::Output>,
}
/// Shortened update body, 0x2258..0x2280. Last two callbacks immediately return.
pub fn contact_course_tick(
    bytes: &[u8],
    exe: &[u8],
    name: &str,
    s: ContactRecovery,
) -> Result<ShortUpdate> {
    contact_sequence(bytes, exe, name, s, None)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NormalContact {
    audio: Option<crate::kart_driving_audio::Controls>,
    update: ContactRecovery,
    previous_velocity_xz: [f32; 2],
    work_b8: i32,
}

#[derive(Serialize)]
pub(crate) struct Impact {
    state: i32,
    work_b8: i32,
    engine_8006b710: Option<[i32; 4]>,
    engine_80011714: Option<i32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoryRecovery {
    previous_velocity_xz: Option<[f32; 2]>,
    work_b8: Option<i32>,
    #[serde(flatten)]
    fields: RecoveryFields,
}
pub(crate) enum ImpactCheck {
    Run,
    Skip,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GuidedFall {
    task_mode: u8,
    saved_angles: Option<[i32; 3]>,
    work_fc_104: [i32; 3],
    work_cc: i32,
    timer: i32,
    engine_halfword_64: Option<i16>,
    angle_radians: Option<f64>,
}
impl GuidedFall {
    pub(crate) fn bind(
        self,
        motion: &crate::kart_guided_drive::Tick,
        route: (i32, i32),
        lap: u32,
    ) -> StoryRecovery {
        StoryRecovery {
            previous_velocity_xz: None,
            work_b8: None,
            fields: RecoveryFields {
                task_mode: self.task_mode,
                saved_angles: self.saved_angles,
                grounded_position: motion.previous_ground_position,
                work_68: 0,
                work_fc_104: self.work_fc_104,
                segment: Some(route.0),
                lap: Some(lap as i32),
                work_cc: self.work_cc,
                work_254: motion.guidance.work_254,
                work_25c: motion.guidance.work_25c,
                work_260: motion.guidance.work_260,
                timer: self.timer,
                engine_halfword_64: self.engine_halfword_64,
                angle_radians: self.angle_radians,
            },
        }
    }
}
impl StoryRecovery {
    pub(crate) fn bind_snapshot(
        &mut self,
        velocity: [f32; 3],
        angles: Option<[i32; 3]>,
    ) -> Result<()> {
        let before = [velocity[0], velocity[2]];
        if self
            .previous_velocity_xz
            .is_some_and(|a| a.map(f32::to_bits) != before.map(f32::to_bits))
        {
            return Err(Error::invalid(
                "impact speed conflicts with vehicle update snapshot",
            ));
        }
        self.previous_velocity_xz = Some(before);
        self.bind_angles(angles)
    }
    pub(crate) fn bind_angles(&mut self, angles: Option<[i32; 3]>) -> Result<()> {
        if self
            .fields
            .saved_angles
            .zip(angles)
            .is_some_and(|(a, b)| a != b)
        {
            return Err(Error::invalid(
                "recovery angles conflict with update-start rotation",
            ));
        }
        self.fields.saved_angles = angles.or(self.fields.saved_angles);
        Ok(())
    }
    pub(crate) fn bind_progress(&mut self, route: Option<(i32, i32)>, counter: u32) -> Result<()> {
        if self.fields.lap.is_some_and(|n| n as u32 != counter)
            || self
                .fields
                .segment
                .zip(route)
                .is_some_and(|(a, (b, _))| a != b)
        {
            return Err(Error::invalid(
                "fall recovery conflicts with primary kart progress",
            ));
        }
        self.fields.lap = Some(counter as i32);
        self.fields.segment = route.map(|r| r.0).or(self.fields.segment);
        Ok(())
    }
    pub(crate) fn run(
        self,
        bytes: &[u8],
        exe: &[u8],
        mode: i32,
        contact: crate::kart_contact::Input,
        work_68: i32,
        impact: ImpactCheck,
    ) -> Result<ShortUpdate> {
        if self.fields.work_68 != work_68 {
            return Err(Error::invalid(
                "recovery steering conflicts with normal drive",
            ));
        }
        contact_sequence(
            bytes,
            exe,
            if mode == 0 { "101" } else { "280" },
            ContactRecovery {
                contact,
                fields: self.fields,
            },
            match impact {
                ImpactCheck::Skip => {
                    if self.previous_velocity_xz.is_some() || self.work_b8.is_some() {
                        return Err(Error::invalid(
                            "spin recovery does not run impact comparison",
                        ));
                    }
                    None
                }
                ImpactCheck::Run => Some((
                    self.previous_velocity_xz
                        .ok_or_else(|| Error::invalid("missing previous velocity for impact"))?,
                    self.work_b8
                        .ok_or_else(|| Error::invalid("missing impact spin step"))?,
                )),
            },
        )
    }
}
impl ShortUpdate {
    pub(crate) fn into_story(self) -> (crate::kart_contact::Update, Output, Option<Impact>) {
        (self.contact, self.recovery, self.impact)
    }
}
pub(crate) struct RecoveredProgress {
    pub guided_segments: [i32; 2],
    pub state: i32,
    pub work_68: i32,
    pub route: (i32, i32),
    pub counter: u32,
    pub positions: ([f32; 3], [f32; 3]),
    pub velocity: [f32; 3],
    pub angles: [i32; 3],
}
impl Impact {
    pub(crate) fn spin_step(&self) -> Option<i32> {
        (self.state == 4).then_some(self.work_b8)
    }
}
impl Output {
    pub(crate) fn spin_offset(&self) -> Option<i32> {
        (self.vehicle.state == 4).then_some(self.vehicle.work_68)
    }
    pub(crate) fn progress(&self) -> RecoveredProgress {
        let v = &self.vehicle;
        RecoveredProgress {
            guided_segments: [v.work_254, v.work_25c],
            state: v.state,
            work_68: v.work_68,
            route: (v.segment, self.course_length),
            counter: v.lap as u32,
            positions: (v.position, v.saved_position),
            velocity: v.velocity,
            angles: v.angles,
        }
    }
}
/// The common contact-to-recovery suffix of state 0 and 1 (0x23f8/0x244c).
pub fn normal_contact_tick(
    bytes: &[u8],
    exe: &[u8],
    name: &str,
    s: NormalContact,
) -> Result<ShortUpdate> {
    let audio = s
        .audio
        .map(|v| s.update.contact.driving_audio(bytes, v))
        .transpose()?;
    let mut result = contact_sequence(
        bytes,
        exe,
        name,
        s.update,
        Some((s.previous_velocity_xz, s.work_b8)),
    )?;
    result.pre_contact_audio = audio;
    Ok(result)
}
fn contact_sequence(
    bytes: &[u8],
    exe: &[u8],
    name: &str,
    s: ContactRecovery,
    previous: Option<([f32; 2], i32)>,
) -> Result<ShortUpdate> {
    let (contact, c) = crate::kart_contact::step_for_fall(bytes, s.contact)?;
    let s = s.fields;
    let impact = previous
        .map(|(before, work_b8)| -> Result<Impact> {
            let rel = CityRel::read_identified(bytes, 80)?;
            let tolerance = f32::from_be_bytes(rel.data(4)?[0x240..0x244].try_into().unwrap());
            let speed = |[x, z]: [f32; 2]| (x * x + z * z).sqrt();
            let now = speed([c.velocity[0], c.velocity[2]]);
            let old = speed(before);
            if !old.is_finite() || !now.is_finite() {
                return Err(Error::invalid("unsupported horizontal speed"));
            }
            let spin = now < old - tolerance;
            Ok(Impact {
                state: if spin { 4 } else { c.state },
                work_b8: if !spin {
                    work_b8
                } else if s.work_68 > 0 {
                    0x71c
                } else {
                    -0x71c
                },
                engine_8006b710: spin.then_some([15, 0, 0, 0]),
                engine_80011714: if spin {
                    match c.mode {
                        0 => Some(0x69d),
                        1 => Some(0x529),
                        _ => None,
                    }
                } else {
                    None
                },
            })
        })
        .transpose()?;
    let vehicle = Vehicle {
        position: c.position,
        velocity: c.velocity,
        angles: c.angles,
        state: impact.as_ref().map_or(c.state, |i| i.state),
        saved_position: c.saved_position,
        task_mode: s.task_mode,
        saved_angles: s
            .saved_angles
            .ok_or_else(|| Error::invalid("missing saved rotation for recovery"))?,
        grounded_position: s.grounded_position,
        work_68: s.work_68,
        work_fc_104: s.work_fc_104,
        segment: s
            .segment
            .ok_or_else(|| Error::invalid("missing recovery segment"))?,
        lap: s
            .lap
            .ok_or_else(|| Error::invalid("missing recovery lap"))?,
        work_cc: s.work_cc,
        work_254: s.work_254,
        work_25c: s.work_25c,
        timer: s.timer,
    };
    let recovery = course_tick(
        bytes,
        exe,
        name,
        Input {
            vehicle,
            work_74: c.work_74,
            work_44: c.work_44,
            work_260: s.work_260,
            mode: c.mode,
            engine_halfword_64: s.engine_halfword_64,
            angle_radians: s.angle_radians,
            fall_height: None,
            segment_count: None,
            vertex: None,
        },
    )?;
    Ok(ShortUpdate {
        pre_contact_audio: None,
        contact,
        recovery,
        impact,
    })
}
