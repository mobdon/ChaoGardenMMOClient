//! Native Chao Race rules, ported from the GameCube SA2B (GSNE8P)
//! `ChaoStgRace.rel` (module 76, SHA-1 2a13767bd9988d0101ea23d434efcb397e915f2b)
//! and `ChaoMain.rel` (module 60, `CM`). Addresses are ChaoStgRace `.text`
//! offsets unless marked `CM`. `tests/chao_race.rs` checks the movement,
//! stamina, trip and progress routines bit for bit against the PowerPC.
//!
//! Skill values are what `AL_ParameterGetSkill` (CM 0x20934) returns in the
//! race stage (stage 4): three times the Chao's skill points, as a u16.
//! A racer record is the Chao task: +0x34 Chao work (+0xC yaw, +0x14 position,
//! +0x88 race stamina, +0x8C its maximum, +0xA0 flags), +0x38 movement
//! (+0 velocity, +0xC acceleration, +0x48 gravity).
//!
//! The course geometry, path following, obstacles, the movement integrator
//! (main.dol MOV_Control) and the player's cheering are not ported.

/// Native constants (each cites its source).
pub mod native {
    /// CM ChaoGlobal (.data 0x2A50) +0x126 `SkillWalk`: no trips at or below it.
    pub const SKILL_WALK: u16 = 150;
    /// CM ChaoGlobal +0x274 `RaceIntervalSubStamina`: fn_76_D2B4 drains every 12 frames.
    pub const INTERVAL_SUB_STAMINA: u32 = 12;
    /// CM ChaoGlobal +0x280 `RaceSubStaminaRun`.
    pub const SUB_STAMINA_RUN: i32 = 20;
    /// fn_76_D578 0xD76C: the second run behaviour drains every 6 frames (an immediate).
    pub const INTERVAL_SUB_STAMINA_D578: i32 = 6;
    /// fn_76_D2B4 0xD37C / fn_76_D578 0xD630: trips are rolled every 50 frames.
    pub const TRIP_PERIOD: i32 = 50;
    /// fn_76_1764 0x1988: start stamina = GetSkill(stamina) * 5 + 10000.
    pub const START_STAMINA_BASE: i32 = 10000;
    pub const START_STAMINA_PER_SKILL: i32 = 5;
    /// rodata 0xC88: 1/32768.
    pub const RAND_SCALE: f32 = 3.051_757_8e-5;
    /// Eight lanes (fn_76_698 0x79C: `cmpwi 8`).
    pub const LANES: usize = 8;
    /// 13 course progress bytes (CM fn_60_24B6C `cmpwi 0xD`).
    pub const COURSES: usize = 13;
}

fn unit(r: i32) -> f32 {
    r as f32 * native::RAND_SCALE
}

/// The movement block (task+0x38) and position (Chao work +0x14).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Body {
    pub velocity: [f32; 3],
    pub accel: [f32; 3],
    pub position: [f32; 3],
    /// Movement +0x48.
    pub gravity: f32,
}

/// The speed factor every movement routine shares: 0.7 with no stamina left
/// (work +0x88 == 0), else `turbo` (work +0xA0 & 0x100, fn_76_2CC) when set.
fn condition(a: f32, stamina: i32, turbo: bool, turbo_ratio: f32) -> f32 {
    if stamina == 0 {
        a * 0.7f32
    } else if turbo {
        a * turbo_ratio
    } else {
        a
    }
}

fn base(skill: u16, base: f32, ratio: f32) -> f32 {
    base + ratio * (1.0e-4f32 * skill as f32)
}

/// fn_76_C3BC, run (skill 2): a = 0.095 + 0.055 * skill / 10000 (rodata
/// 0xC18, 0xC1C, 0xC20); exhausted x0.7, turbo x1.3. Sets the horizontal
/// acceleration from the yaw (`sin`, `cos` = njSin/njCos of work +0xC).
pub fn run(body: &mut Body, skill: u16, stamina: i32, turbo: bool, sin: f32, cos: f32) -> f32 {
    let a = condition(base(skill, 0.095, 0.055), stamina, turbo, 1.3);
    body.accel[0] = a * sin;
    body.accel[2] = a * cos;
    a
}

/// fn_76_C500, fly (skill 1): a = 0.018 + 0.067 * skill / 10000, turbo x1.2;
/// sinks by 0.01 + 0.005 * (10000 - skill) / 10000 against a 0.05 vertical and
/// 0.1 horizontal drag. Returns the motion speed (1.6, rodata 0xC54).
pub fn fly(body: &mut Body, skill: u16, stamina: i32, turbo: bool, sin: f32, cos: f32) -> f32 {
    let a = condition(base(skill, 0.018, 0.067), stamina, turbo, 1.2);
    let rise = -body.gravity - 0.05f32 * body.velocity[1];
    let sink = 0.01f32 + 0.005f32 * (1.0e-4f32 * (10000 - i32::from(skill)) as f32);
    body.accel[1] = rise - sink;
    body.accel[0] = a * sin - 0.1f32 * body.velocity[0];
    body.accel[2] = a * cos - 0.1f32 * body.velocity[2];
    1.6
}

/// fn_76_C6EC, swim (skill 0): a = 0.0027 + 0.018 * skill / 10000, turbo x1.3,
/// 0.06 horizontal and 0.05 vertical drag.
pub fn swim(body: &mut Body, skill: u16, stamina: i32, turbo: bool, sin: f32, cos: f32) -> f32 {
    let a = condition(base(skill, 0.0027, 0.018), stamina, turbo, 1.3);
    body.accel[1] = -body.gravity - 0.05f32 * body.velocity[1];
    body.accel[0] = a * sin - 0.06f32 * body.velocity[0];
    body.accel[2] = a * cos - 0.06f32 * body.velocity[2];
    a
}

/// fn_76_C864, climb (skill 3): a = 0.04 + 0.1 * skill / 10000, turbo x1.3;
/// damps the horizontal velocity by 0.95, moves the position 1.1a along the
/// yaw and a upwards. Returns the motion speed 15a.
pub fn climb(body: &mut Body, skill: u16, stamina: i32, turbo: bool, sin: f32, cos: f32) -> f32 {
    let a = condition(base(skill, 0.04, 0.1), stamina, turbo, 1.3);
    body.velocity[0] *= 0.95f32;
    body.velocity[2] *= 0.95f32;
    let reach = 1.1f32 * a;
    body.position[0] += reach * sin;
    body.position[1] += a;
    body.position[2] += reach * cos;
    body.accel[1] = -body.gravity;
    15.0f32 * a
}

/// Which run behaviour: fn_76_D2B4 (the usual one) or fn_76_D578.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunBehaviour {
    D2B4,
    D578,
}

/// One frame of a run behaviour after its first step: the trip roll every 50
/// frames above `SkillWalk` (p = 0.001 + k * (10000 - skill), k = 1e-6 for
/// D2B4 and 8e-7 for D578; D2B4 never trips Chao param +0xDD == 3), the frame
/// counter, and the stamina drain (fn_60_2105C, clamped at 0). Returns whether
/// the racer trips (AL_SetBehavior fn_76_D008).
pub fn run_frame(
    which: RunBehaviour,
    skill: u16,
    counter: &mut i32,
    stamina: &mut i32,
    param_dd: u8,
    rng: &mut impl FnMut() -> i32,
) -> bool {
    let mut trip = false;
    if skill > native::SKILL_WALK && *counter % native::TRIP_PERIOD == 0 {
        let u = unit(rng());
        let k = match which {
            RunBehaviour::D2B4 => 1.0e-6f32,
            RunBehaviour::D578 => 8.0e-7f32,
        };
        let p = 0.001f32 + k * (10000 - i32::from(skill)) as f32;
        trip = u < p && (which == RunBehaviour::D578 || param_dd != 3);
    }
    *counter = counter.wrapping_add(1);
    let drain = match which {
        RunBehaviour::D2B4 => (*counter as u32).is_multiple_of(native::INTERVAL_SUB_STAMINA),
        RunBehaviour::D578 => *counter % native::INTERVAL_SUB_STAMINA_D578 == 0,
    };
    if drain {
        *stamina = sub_stamina(*stamina, native::SUB_STAMINA_RUN);
    }
    trip
}

/// CM fn_60_2105C with a negative amount: work +0x88 += delta, clamped at 0.
pub fn sub_stamina(stamina: i32, amount: i32) -> i32 {
    let v = stamina.wrapping_add(i32::from(-(amount as i16)));
    v.max(0)
}

/// fn_76_1764 0x1974..0x199C: start stamina (and its maximum, +0x8C).
pub fn start_stamina(skill: u16) -> i32 {
    i32::from(skill) * native::START_STAMINA_PER_SKILL + native::START_STAMINA_BASE
}

/// Race classes, as CM fn_60_24798 lays out the course table (mode = race
/// module +0x253).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Class {
    Beginner,
    Jewel,
    Challenge,
    Hero,
    Dark,
}

impl Class {
    /// (course numbers, levels per course) from fn_60_24798 / fn_60_24ABC.
    pub fn courses(self) -> (std::ops::Range<u8>, u8) {
        match self {
            Class::Beginner => (0..4, 3),
            Class::Jewel => (4..10, 5),
            Class::Challenge => (10..11, 12),
            Class::Hero => (11..12, 4),
            Class::Dark => (12..13, 4),
        }
    }

    pub fn of_course(course: u8) -> Option<Self> {
        [
            Class::Beginner,
            Class::Jewel,
            Class::Challenge,
            Class::Hero,
            Class::Dark,
        ]
        .into_iter()
        .find(|c| c.courses().0.contains(&course))
    }

    /// CM fn_60_24798: the course table row of a course and level.
    pub fn row(self, course: u8, level: u8) -> Option<usize> {
        let (courses, levels) = self.courses();
        if !courses.contains(&course) || level >= levels {
            return None;
        }
        let level = usize::from(level);
        Some(match self {
            Class::Beginner => usize::from(course) * 3 + level,
            Class::Jewel => 12 + usize::from(course - 4) * 5 + level,
            Class::Challenge => 42 + level,
            Class::Hero => 54 + level,
            Class::Dark => 58 + level,
        })
    }

    /// Emblem byte save+0x3224 + index (fn_76_698 0xADC..0x100C).
    pub fn emblem(self) -> usize {
        self as usize
    }
}

/// One 20-byte course table row (CM .data 0x939C0; the same rows are in
/// sonic2app.exe at VA 0x01354828).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CourseRow {
    /// +0: challenge rows name their rival set with it (CM fn_60_24E40
    /// 0x24ED8: set 42 + index).
    pub index: i32,
    /// +4: race land, 0 neutral, 1 hero, 2 dark (fn_76_1A94 0x1ABC).
    pub land: i32,
    /// +8: course section set, 0..9 = BegSwm BegFly BegRun BegPow JwlSwm JwlFly
    /// JwlRun JwlPow JwlIL JwlLng (fn_76_1A94 0x1BDC, .data 0xDD4 names).
    pub sections: i32,
    /// +0xC: passed to fn_76_E010.
    pub e010: i32,
    /// +0x10: the toy (beginner/jewel, AL_KW_SToyFlagOn) or garden flag
    /// (challenge/hero/dark, AL_OnGardenFlagIndex) a win unlocks, or -1.
    pub unlock: i32,
}

pub const EXE_COURSE_ROWS: u32 = 0x0135_4828;
pub const COURSE_ROWS: usize = 62;
/// CM .data offset of the same table.
pub const REL_COURSE_ROWS: usize = 0x939C0;

pub fn read_course_rows_exe(exe: &[u8]) -> crate::Result<Vec<CourseRow>> {
    let image = crate::pe::read_pe(exe)?;
    let at = image.va_to_offset(EXE_COURSE_ROWS, COURSE_ROWS * 20)?;
    Ok(rows(&exe[at..at + COURSE_ROWS * 20], i32::from_le_bytes))
}

pub fn read_course_rows_rel(chao_main: &[u8]) -> crate::Result<Vec<CourseRow>> {
    let word = |o: usize| -> crate::Result<u32> {
        chao_main
            .get(o..o + 4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
            .ok_or_else(|| crate::Error::invalid("REL is truncated"))
    };
    if word(0)? != 60 {
        return Err(crate::Error::invalid(
            "not the GameCube ChaoMain.rel (module 60)",
        ));
    }
    let data = (word(word(16)? as usize + 8 * 5)? & !1) as usize + REL_COURSE_ROWS;
    let bytes = chao_main
        .get(data..data + COURSE_ROWS * 20)
        .ok_or_else(|| crate::Error::invalid("REL is truncated"))?;
    Ok(rows(bytes, i32::from_be_bytes))
}

fn rows(b: &[u8], int: fn([u8; 4]) -> i32) -> Vec<CourseRow> {
    b.chunks(20)
        .map(|r| {
            let f = |k: usize| int([r[4 * k], r[4 * k + 1], r[4 * k + 2], r[4 * k + 3]]);
            CourseRow {
                index: f(0),
                land: f(1),
                sections: f(2),
                e010: f(3),
                unlock: f(4),
            }
        })
        .collect()
}

/// The race progress bytes (race module +0x13, saved at GardenInfo+0x780+0x13;
/// CM AL_InitRaceData 0xDC8C starts them at -1): the highest level won per course.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Progress(pub [i8; native::COURSES]);

impl Default for Progress {
    fn default() -> Self {
        Self([-1; native::COURSES])
    }
}

impl Progress {
    /// CM fn_60_24ABC: raise a course's level (only upwards, below the class's
    /// level count).
    pub fn record(&mut self, course: i32, level: i32) {
        let Ok(c) = usize::try_from(course) else {
            return;
        };
        let Some(old) = self.0.get(c) else { return };
        if level <= i32::from(*old) {
            return;
        }
        let cap = match c {
            0..=3 => 3,
            4..=9 => 5,
            10 => 12,
            11 | 12 => 4,
            _ => return,
        };
        if level < cap {
            self.0[c] = level as i8;
        }
    }

    /// CM fn_60_24B6C (returns the argument itself for a course out of range).
    pub fn get(&self, course: i32) -> i32 {
        usize::try_from(course)
            .ok()
            .and_then(|c| self.0.get(c))
            .map_or(course, |v| i32::from(*v))
    }
}

/// What a race finish grants (fn_76_698 0x938..0x1024). Only a win (the first
/// finisher, bss_10[0], is lane 0, bss_48[0] +0x10) records anything.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Award {
    /// Course row +0x10 when not -1.
    pub unlock: Option<i32>,
    /// Chao param +0xD7 medal and the +0x108 bit it sets.
    pub medal: Option<(u8, u16)>,
    /// Emblem byte indices (save+0x3224 + i) newly set.
    pub emblems: Vec<usize>,
}

/// fn_76_698: on a win, record progress, then the medal and the emblems.
/// `emblems` are the five save bytes 0x3224..0x3228.
pub fn win(
    progress: &mut Progress,
    emblems: &mut [bool; 5],
    class: Class,
    course: u8,
    level: u8,
    row: &CourseRow,
) -> Award {
    let mut award = Award {
        unlock: (row.unlock != -1).then_some(row.unlock),
        ..Default::default()
    };
    progress.record(i32::from(course), i32::from(level));
    let all = |courses: std::ops::Range<i32>, at_least: i32| {
        courses.clone().all(|c| progress.get(c) >= at_least)
    };
    award.medal = match class {
        // 0x9DC: every beginner course won to level 2, and this was level 2: medal 7.
        Class::Beginner if all(0..4, 2) && level == 2 => Some((7, 2)),
        // 0xA78: a jewel course's level 4: medal course - 3, bit 1 << course.
        Class::Jewel if level == 4 => Some((course - 3, 1 << course)),
        // 0xE44..0xF44: challenge level 11 medal 8, hero/dark level 3 medals 9/10.
        Class::Challenge if level == 11 => Some((8, 1)),
        Class::Hero if level == 3 => Some((9, 4)),
        Class::Dark if level == 3 => Some((10, 8)),
        _ => None,
    };
    let checks: [(usize, bool); 5] = [
        (0, all(0..4, 2)),
        (1, all(4..10, 4)),
        (2, progress.get(10) >= 11),
        (3, progress.get(11) >= 3),
        (4, progress.get(12) >= 3),
    ];
    // Beginner/jewel wins check bytes 0 and 1; the others check bytes 2..4.
    let range = if matches!(class, Class::Beginner | Class::Jewel) {
        0..2
    } else {
        2..5
    };
    for (i, ok) in checks[range].iter().copied() {
        if ok && !emblems[i] {
            emblems[i] = true;
            award.emblems.push(i);
        }
    }
    award
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_rows_cover_the_62_course_rows() {
        let mut rows = vec![];
        for class in [
            Class::Beginner,
            Class::Jewel,
            Class::Challenge,
            Class::Hero,
            Class::Dark,
        ] {
            let (courses, levels) = class.courses();
            for c in courses {
                for l in 0..levels {
                    rows.push(class.row(c, l).unwrap());
                }
            }
        }
        assert_eq!(rows, (0..62).collect::<Vec<_>>());
    }

    #[test]
    fn progress_only_rises_and_stays_below_the_cap() {
        let mut p = Progress::default();
        p.record(0, 1);
        p.record(0, 0);
        assert_eq!(p.get(0), 1);
        p.record(0, 3);
        assert_eq!(p.get(0), 1);
        p.record(10, 11);
        assert_eq!(p.get(10), 11);
        assert_eq!(p.get(13), 13);
    }

    #[test]
    fn winning_every_beginner_level_two_gives_medal_seven_and_the_emblem() {
        let mut p = Progress::default();
        let mut e = [false; 5];
        let row = CourseRow {
            index: -1,
            land: 0,
            sections: 0,
            e010: 0,
            unlock: -1,
        };
        for c in 0..3 {
            p.record(c, 2);
        }
        let a = win(&mut p, &mut e, Class::Beginner, 3, 2, &row);
        assert_eq!(a.medal, Some((7, 2)));
        assert_eq!(a.emblems, vec![0]);
        assert!(
            win(&mut p, &mut e, Class::Beginner, 3, 2, &row)
                .emblems
                .is_empty()
        );
    }
}

/// One 0x54-byte AI racer record (CM .data 0x93ED4 + 0x54 i; sonic2app.exe
/// VA 0x011CE2A0, 115 records). Only the fields the skill code reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RaceBot {
    /// +0x30: 0 and 2 = shared total, 1 = per-skill (base, spread) pairs.
    pub mode: u16,
    /// +0x34: total split over 7 skills.
    pub shared: u32,
    /// +0x38: (base, spread) per skill, swim..intellect.
    pub pairs: [[u16; 2]; 7],
}

/// One 0x24-byte rival set (CM .data 0x93E98 + 0x25F8 + 0x24 i; sonic2app.exe
/// VA 0x011D0888, 62 sets): +0 0 = shuffle the lanes, +8 seven racer records.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RivalSet {
    pub fixed: i32,
    pub unknown4: i32,
    pub bots: [i32; 7],
}

pub const EXE_RACE_BOTS: u32 = 0x011C_E2A0;
pub const RACE_BOTS: usize = 115;
pub const EXE_RIVAL_SETS: u32 = 0x011D_0888;
pub const RIVAL_SETS: usize = 62;
/// CM .data offsets of the same tables.
pub const REL_RACE_BOTS: usize = 0x93ED4;
pub const REL_RIVAL_SETS: usize = 0x93E98 + 0x25F8;

fn bot(b: &[u8], u16v: fn([u8; 2]) -> u16, u32v: fn([u8; 4]) -> u32) -> RaceBot {
    RaceBot {
        mode: u16v([b[0x30], b[0x31]]),
        shared: u32v([b[0x34], b[0x35], b[0x36], b[0x37]]),
        pairs: std::array::from_fn(|i| {
            let o = 0x38 + 4 * i;
            [u16v([b[o], b[o + 1]]), u16v([b[o + 2], b[o + 3]])]
        }),
    }
}
fn set(b: &[u8], i32v: fn([u8; 4]) -> i32) -> RivalSet {
    let w = |k: usize| i32v([b[4 * k], b[4 * k + 1], b[4 * k + 2], b[4 * k + 3]]);
    RivalSet {
        fixed: w(0),
        unknown4: w(1),
        bots: std::array::from_fn(|k| w(2 + k)),
    }
}

/// Reads the racer records and rival sets from the user's sonic2app.exe.
pub fn read_rivals_exe(exe: &[u8]) -> crate::Result<(Vec<RaceBot>, Vec<RivalSet>)> {
    let image = crate::pe::read_pe(exe)?;
    let at = image.va_to_offset(EXE_RACE_BOTS, RACE_BOTS * 0x54)?;
    let bots = exe[at..at + RACE_BOTS * 0x54]
        .chunks(0x54)
        .map(|b| bot(b, u16::from_le_bytes, u32::from_le_bytes))
        .collect();
    let at = image.va_to_offset(EXE_RIVAL_SETS, RIVAL_SETS * 0x24)?;
    let sets = exe[at..at + RIVAL_SETS * 0x24]
        .chunks(0x24)
        .map(|b| set(b, i32::from_le_bytes))
        .collect();
    Ok((bots, sets))
}

/// The same tables from a GameCube ChaoMain.rel.
pub fn read_rivals_rel(chao_main: &[u8]) -> crate::Result<(Vec<RaceBot>, Vec<RivalSet>)> {
    let word = |o: usize| -> crate::Result<u32> {
        chao_main
            .get(o..o + 4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
            .ok_or_else(|| crate::Error::invalid("REL is truncated"))
    };
    if word(0)? != 60 {
        return Err(crate::Error::invalid(
            "not the GameCube ChaoMain.rel (module 60)",
        ));
    }
    let data = (word(word(16)? as usize + 8 * 5)? & !1) as usize;
    let get = |o: usize, n: usize| {
        chao_main
            .get(data + o..data + o + n)
            .ok_or_else(|| crate::Error::invalid("REL is truncated"))
    };
    let bots = get(REL_RACE_BOTS, RACE_BOTS * 0x54)?
        .chunks(0x54)
        .map(|b| bot(b, u16::from_be_bytes, u32::from_be_bytes))
        .collect();
    let sets = get(REL_RIVAL_SETS, RIVAL_SETS * 0x24)?
        .chunks(0x24)
        .map(|b| set(b, i32::from_be_bytes))
        .collect();
    Ok((bots, sets))
}

/// CM fn_60_24E40: the rival set of a race (challenge rows name it in +0).
pub fn rival_set_index(class: Class, course: u8, level: u8, rows: &[CourseRow]) -> Option<usize> {
    let row = class.row(course, level)?;
    match class {
        Class::Challenge => usize::try_from(rows.get(row)?.index).ok().map(|i| 42 + i),
        _ => Some(row),
    }
}

/// CM fn_60_24F48 0x24F98..0x25064: the lane order, shuffled with 32 random
/// swaps (index = (int)(6.9999 * rand / 32768), rodata 0x74A8) unless +0 is set.
pub fn rival_order(set: &RivalSet, rng: &mut impl FnMut() -> i32) -> [usize; 7] {
    let mut order = [0, 1, 2, 3, 4, 5, 6];
    if set.fixed == 0 {
        let pick = |r: i32| (6.999_9f32 * unit(r)) as i32 as usize;
        for _ in 0..32 {
            let a = pick(rng());
            let b = pick(rng());
            order.swap(a, b);
        }
    }
    order
}

/// CM fn_60_253D4 0x26198..0x263D0: a rival's param skills (+0x38, swim..
/// intellect). Mode 0/2: shared / 7 + (int)(500 * rand / 32768); mode 1:
/// base + (int)(spread * (2 * rand / 32768 - 1)), then stamina and guts copy
/// intellect. Every skill is stored, then divided by 3 (the race stage's
/// AL_ParameterGetSkill multiplies by 3 again). The appearance rand draws
/// that come before this in fn_60_253D4 are not ported.
pub fn rival_skills(bot: &RaceBot, rng: &mut impl FnMut() -> i32) -> [u16; 7] {
    let mut s = [0u16; 7];
    match bot.mode {
        0 | 2 => {
            let base = bot.shared / 7;
            for v in &mut s {
                let r = (500.0f32 * unit(rng())) as i32 as u16;
                let sum = base.wrapping_add(u32::from(r)) as u16;
                *v = (i32::from(sum) / 3) as u16;
            }
        }
        1 => {
            for (i, v) in s.iter_mut().enumerate() {
                let u = unit(rng());
                let k = 2.0f32 * u - 1.0f32;
                let r = (bot.pairs[i][1] as f32 * k) as i32;
                let sum = (i32::from(bot.pairs[i][0]).wrapping_add(r)) as u16;
                *v = (i32::from(sum) / 3) as u16;
            }
            s[4] = s[6];
            s[5] = s[6];
        }
        _ => {}
    }
    s
}
