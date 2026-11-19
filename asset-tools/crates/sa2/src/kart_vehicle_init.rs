//! CartD's vehicle initialization callback, text 0x2898 through 0x2a28.
use crate::{city_rel::CityRel, kart_progress, Result};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    work_44: i32,
    player_slot: i32,
    position: [f32; 3],
    angles: [i32; 3],
    /// The four words from the selected live profile at +0x14,+0x18,+0x1c,+0x28.
    profile_words: [u32; 4],
}
#[derive(Serialize)]
pub struct Init {
    position: [f32; 3],
    angles: [i32; 3],
    previous_position: [f32; 3],
    velocity: [f32; 3],
    vehicle_state: i32,
    work_70: i32,
    player_slot: i32,
    profile_words_34_to_40: [u32; 4],
    cleared_work_offsets: &'static [u16],
    guidance: Option<crate::kart_guided_drive::Guidance>,
}
pub fn init(bytes: &[u8], s: Input) -> Result<Init> {
    kart_progress::verify(bytes)?;
    let rel = CityRel::read_identified(bytes, 80)?;
    let c = rel.data(4)?;
    let zero = f32::from_be_bytes(c[0x1c8..0x1cc].try_into().unwrap());
    let guided = s.work_44 == 0;
    let guidance = guided
        .then(|| crate::kart_guided_drive::Guidance::at_spawn(s.player_slot, s.angles[1], zero));
    Ok(Init {
        position: s.position,
        angles: s.angles,
        previous_position: s.position,
        velocity: [zero; 3],
        vehicle_state: if guided { 7 } else { 0 },
        work_70: if guided { -1 } else { s.player_slot },
        player_slot: s.player_slot,
        profile_words_34_to_40: s.profile_words,
        cleared_work_offsets: &[
            0x68, 0xf0, 0xf4, 0xf8, 0x108, 0xbc, 0xc4, 0xc8, 0xd8, 0xdc, 0xec, 0x290, 0x28c, 0xcc,
            0x294,
        ],
        guidance,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileInput {
    boost: Option<BoostInput>,
    profile8_write: Option<[u32; 10]>,
    profile: usize,
    alternate: bool,
    velocity: [f32; 3],
    normalize: Option<Normalize>,
    event: Option<EventInput>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Normalize {
    magnitude: f32,
    vector: [f32; 3],
}
#[derive(Serialize)]
pub struct Profile {
    #[serde(skip_serializing_if = "Option::is_none")]
    drive: Option<crate::kart_drive_input::Motion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    boost: Option<Boost>,
    section5_offset: usize,
    cue_words: [i32; 6],
    event_pair: Option<[i32; 2]>,
    event: Option<Event>,
    acceleration: f32,
    brake_power: f32,
    deceleration: f32,
    speed_cap: f32,
    weight: f32,
    field_14: f32,
    drift_handling: f32,
    speed_threshold: f32,
    field_20: f32,
    velocity_limit: f32,
    velocity: [f32; 3],
    shared_velocity_write: Option<[f32; 3]>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoryBoostInput {
    drive: Option<crate::kart_drive_input::StoryDrive>,
    velocity: [f32; 3],
    normalize: Option<Normalize>,
    boost: BoostInput,
}
/// Per-vehicle work +0xd8/+0xdc and the controller's readiness word.
pub(crate) fn story_boost(
    bytes: &[u8],
    mut input: StoryBoostInput,
    story_profile: usize,
    charge: i32,
    timers: Option<(i32, i32)>,
    flag: Option<u32>,
    vehicle_state: Option<i32>,
) -> Result<(i32, (i32, i32), Profile)> {
    let task_position = input.boost.position;
    let b = &mut input.boost;
    if b.work_70 != 0 {
        return Err(crate::Error::invalid("expected primary controller slot"));
    }
    let bind = |word: &mut Option<i32>, retained| -> Result<()> {
        if word.zip(retained).is_some_and(|(x, y)| x != y) {
            return Err(crate::Error::invalid(
                "boost observation conflicts with retained kart state",
            ));
        }
        *word = retained.or(*word);
        Ok(())
    };
    bind(&mut b.work_d8, Some(charge))?;
    bind(&mut b.work_dc, timers.map(|t| t.0))?;
    bind(&mut b.slot_flag, timers.map(|t| t.1))?;
    bind(&mut b.work_44, flag.map(|v| v as i32))?;
    let drive_flag = b.work_44;
    let piece_id = b.piece_id;
    let mut output = profile(
        bytes,
        ProfileInput {
            profile: story_profile,
            alternate: false,
            boost: Some(input.boost),
            profile8_write: None,
            event: None,
            velocity: input.velocity,
            normalize: input.normalize,
        },
    )?;
    let b = output
        .boost
        .as_ref()
        .ok_or_else(|| crate::Error::invalid("missing story boost output"))?;
    if let Some(drive) = input.drive {
        let rel = CityRel::read_identified(bytes, 80)?;
        output.drive = Some(drive.step(
            b.velocity,
            [
                b.acceleration,
                output.brake_power,
                output.deceleration,
                b.speed_cap,
            ],
            task_position,
            output.weight,
            drive_flag,
            story_profile as i32,
            vehicle_state,
            piece_id,
            &rel,
        )?);
    }
    Ok((b.work_d8, (b.work_dc, b.slot_flag), output))
}
/// Resolve the physics row copied by 0x2284 and bound the incoming 3D velocity.
pub fn profile(bytes: &[u8], s: ProfileInput) -> Result<Profile> {
    kart_progress::verify(bytes)?;
    if s.profile > 8 || (s.alternate && !(2..=7).contains(&s.profile)) {
        return Err(crate::Error::invalid(
            "unsupported base/alternate kart profile",
        ));
    }
    let rel = CityRel::read_identified(bytes, 80)?;
    let data = rel.data(5)?;
    let offset = if s.alternate {
        0x3834 + (s.profile - 2) * 40
    } else {
        0x36cc + s.profile * 40
    };
    let row: [f32; 10] = std::array::from_fn(|j| {
        f32::from_be_bytes(data[offset + 4 * j..offset + 4 * j + 4].try_into().unwrap())
    });
    let row = if s.profile == 8 {
        s.profile8_write
            .map(|r| r.map(f32::from_bits))
            .unwrap_or(row)
    } else {
        row
    };
    let cue_words = cue_row(data, s.profile, s.alternate)?;
    let event_pair = (s.profile < 8).then(|| {
        let at = if s.alternate {
            0x3b14 + (s.profile - 2) * 8
        } else {
            0x3ad4 + s.profile * 8
        };
        std::array::from_fn(|j| {
            i32::from_be_bytes(data[at + 4 * j..at + 4 * j + 4].try_into().unwrap())
        })
    });
    let event = s.event.map(|e| profile_event(event_pair, e)).transpose()?;
    let mut velocity = s.velocity;
    let mut shared_velocity_write = None;
    if velocity != [0.; 3] {
        let n = s.normalize.ok_or_else(|| {
            crate::Error::invalid("moving kart requires observed normalization result")
        })?;
        if n.magnitude > row[9] {
            velocity = n.vector.map(|v| v * row[9]);
            shared_velocity_write = Some(velocity);
        }
    }
    let boost = s
        .boost
        .map(|b| boost_step(rel.data(4)?, b, event_pair, velocity, row[0], row[3]))
        .transpose()?;
    Ok(Profile {
        drive: None,
        boost,
        section5_offset: offset,
        cue_words,
        event_pair,
        event,
        acceleration: row[0],
        brake_power: row[1],
        deceleration: row[2],
        speed_cap: row[3],
        weight: row[4],
        field_14: row[5],
        drift_handling: row[6],
        speed_threshold: row[7],
        field_20: row[8],
        velocity_limit: row[9],
        velocity,
        shared_velocity_write,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EventInput {
    work_44: i32,
    selector: usize,
    piece_id: Option<u8>,
}
#[derive(Serialize)]
struct Event {
    return_word: i32,
    call_cart_979c: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    course_mode: Option<CourseMode>,
    engine_args: Option<[i32; 4]>,
}

pub(crate) fn cue_row(data: &[u8], profile: usize, alternate: bool) -> Result<[i32; 6]> {
    if profile >= 16 || (alternate && !(2..=7).contains(&profile)) {
        return Err(crate::Error::invalid("unsupported kart cue profile"));
    }
    let at = if alternate {
        0x39fc + (profile - 2) * 24
    } else {
        0x3924 + profile * 24
    };
    Ok(std::array::from_fn(|j| {
        i32::from_be_bytes(data[at + 4 * j..at + 4 * j + 4].try_into().unwrap())
    }))
}

/// 0x979c uses the signed course-piece byte for the current segment.
#[derive(Serialize)]
pub(crate) struct CourseMode {
    piece_id: u8,
    pub(crate) engine_8006c97c_arg: i32,
    pub(crate) return_word: i32,
}
pub(crate) fn course_mode(piece_id: u8) -> CourseMode {
    let inside = (0x32..=0x47).contains(&(piece_id as i8));
    CourseMode {
        piece_id,
        engine_8006c97c_arg: if inside { 0 } else { -127 },
        return_word: i32::from(inside),
    }
}

fn profile_event(event_pair: Option<[i32; 2]>, e: EventInput) -> Result<Event> {
    let eligible = e.work_44 != 0 && event_pair.is_some();
    let id = if eligible {
        *event_pair
            .as_ref()
            .unwrap()
            .get(e.selector)
            .ok_or_else(|| crate::Error::invalid("kart event selector must be 0 or 1"))?
    } else {
        -1
    };
    Ok(Event {
        return_word: i32::from(eligible),
        call_cart_979c: id >= 0,
        course_mode: e.piece_id.filter(|_| id >= 0).map(course_mode),
        engine_args: (id >= 0).then_some([id, 0, 0, 0]),
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BoostInput {
    work_70: i32,
    work_d8: Option<i32>,
    work_dc: Option<i32>,
    slot_flag: Option<i32>,
    buttons_pressed: Option<u32>,
    normalized_velocity: Option<[f32; 3]>,
    position: [f32; 3],
    work_44: Option<i32>,
    piece_id: Option<u8>,
}
#[derive(Serialize)]
struct Boost {
    work_d8: i32,
    work_dc: i32,
    slot_flag: i32,
    acceleration: f32,
    speed_cap: f32,
    velocity: [f32; 3],
    calls: Vec<BoostCall>,
}
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum BoostCall {
    Effect800bf43c {
        slot: i32,
        kind: i32,
        position: [f32; 3],
        velocity: [f32; 3],
    },
    ProfileEvent(Event),
    Engine8002f76c([i32; 4]),
}
/// 0x4eb4: readiness, activation and per-frame changes before driving.
fn boost_step(
    data: &[u8],
    s: BoostInput,
    pair: Option<[i32; 2]>,
    velocity: [f32; 3],
    acceleration: f32,
    speed_cap: f32,
) -> Result<Boost> {
    let f = |i: usize| f32::from_be_bytes(data[i..i + 4].try_into().unwrap());
    let mut b = Boost {
        work_d8: s
            .work_d8
            .ok_or_else(|| crate::Error::invalid("missing boost work_d8"))?,
        work_dc: s
            .work_dc
            .ok_or_else(|| crate::Error::invalid("missing boost work_dc"))?,
        slot_flag: s
            .slot_flag
            .ok_or_else(|| crate::Error::invalid("missing boost slot_flag"))?,
        acceleration,
        speed_cap,
        velocity,
        calls: vec![],
    };
    if s.work_70 < 0 {
        return Ok(b);
    }
    if s.work_70 > 1 {
        return Err(crate::Error::invalid(
            "unsupported controller slot for kart boost",
        ));
    }
    if b.work_d8 >= 20 {
        b.slot_flag = 1;
        b.work_d8 = 0;
    }
    if b.work_dc != 0 {
        b.acceleration *= f(0x298);
        b.speed_cap *= f(0x298);
        b.work_dc = b.work_dc.wrapping_sub(1);
        let mut position = s.position;
        position[1] -= f(0x29c);
        b.calls.push(BoostCall::Effect800bf43c {
            slot: s.work_70,
            kind: 5,
            position,
            velocity,
        });
    } else if b.slot_flag != 0
        && s.buttons_pressed
            .ok_or_else(|| crate::Error::invalid("missing controller press word"))?
            & 0x200
            != 0
    {
        b.slot_flag = 0;
        b.work_dc = 120;
        b.velocity = s
            .normalized_velocity
            .ok_or_else(|| crate::Error::invalid("missing activation normalization result"))?
            .map(|v| v * f(0x1e0));
        b.calls.push(BoostCall::ProfileEvent(profile_event(
            pair,
            EventInput {
                work_44: s
                    .work_44
                    .ok_or_else(|| crate::Error::invalid("missing boost vehicle flag"))?,
                selector: 1,
                piece_id: s.piece_id,
            },
        )?));
        b.calls
            .push(BoostCall::Engine8002f76c([s.work_70, 4, 50, 120]));
    }
    Ok(b)
}
