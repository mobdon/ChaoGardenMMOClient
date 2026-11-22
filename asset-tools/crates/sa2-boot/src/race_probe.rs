//! Chao MMO glue, debug only (off unless SA2_RACE_PROBE is set): checks whether
//! a Chao's heading follows its twp angle Y, which the player-controlled Race
//! steering will write. Hooks the main.dol movement step `fn_8002F1C0(task,
//! flags)` that the Race run behaviour (ChaoStgRace fn_76_D2B4) calls, logs
//! each Chao task's angle Y and position every 30 calls, and with
//! SA2_RACE_PROBE=steer holds angle Y a quarter turn from its first value after
//! 300 calls. twp = task +0x34; angle (x, y, z) at twp +0x08, position at twp
//! +0x14 (CreateChildTask copies +0x08..+0x1C; ENGINE-MAP). Unverified: that
//! +0x0C is the heading the movement step uses; this probe is the check.
use crate::{guest_call, original};
use sa2_recomp::rt::{Cpu, Mem};
use sa2_recomp::{global, global::Global};
use std::collections::HashMap;

const MOVE_STEP: u32 = 0x8002_F1C0;
const QUARTER_TURN: u32 = 0x4000;

/// Per task: (calls seen, first angle Y).
static SEEN: Global<Option<HashMap<u32, (u32, u32)>>> = global!(None);
static STEER: Global<bool> = global!(false);
/// Race to start without menu input once the Entrance is loaded: (type, course).
static START: Global<Option<(u32, u32)>> = global!(None);

/// ChaoMain (60) .text: race type/course/level setters, area change.
const CHAOMAIN: u32 = 60;
const SET_RACE_TYPE: u32 = 0x2_4CD0;
const SET_RACE_COURSE: u32 = 0x2_4CA8;
const SET_RACE_LEVEL: u32 = 0x2_4C80;
const CHANGE_AREA: u32 = 0x2_7E00;
const RACE_AREA: u32 = 4;
/// ChaoStgEntrance (75) .text 0x64: fills entrant slots 1..7 and the rivals.
const ENTRANCE: u32 = 75;
const FILL_ENTRANTS: u32 = 0x64;

pub fn install(c: &mut Cpu, _m: &mut Mem) {
    let Ok(v) = std::env::var("SA2_RACE_PROBE") else { return };
    STEER.set(v.trim() == "steer");
    SEEN.set(Some(HashMap::new()));
    // SA2_RACE_START=type:course (menu cursor values; which type is Beginner is unverified).
    START.set(std::env::var("SA2_RACE_START").ok().and_then(|v| {
        let (t, c) = v.trim().split_once(':')?;
        Some((t.parse().ok()?, c.parse().ok()?))
    }));
    eprintln!("race-probe: on (start {:?})", START.get());
    eprintln!("race-probe: on (steer {})", STEER.get());
    c.rt.set_override(MOVE_STEP, move_step);
}

fn move_step(c: &mut Cpu, m: &mut Mem) {
    let task = c.r[3];
    let twp = m.r32(task + 0x34);
    if twp != 0 {
        let mut seen = SEEN.take().unwrap_or_default();
        let e = seen.entry(task).or_insert((0, m.r32(twp + 0x0C)));
        e.0 += 1;
        let (n, first) = *e;
        SEEN.set(Some(seen));
        if STEER.get() && n >= 300 {
            m.w32(twp + 0x0C, first.wrapping_add(QUARTER_TURN));
        }
        if n % 30 == 0 {
            let f = |o| f32::from_bits(m.r32(twp + o));
            eprintln!(
                "race-probe: task {task:#x} call {n} angle_y {:#x} pos ({:.2}, {:.2}, {:.2})",
                m.r32(twp + 0x0C),
                f(0x14),
                f(0x18),
                f(0x1C)
            );
        }
    }
    original(c, m, MOVE_STEP);
}

/// Called every top-level frame: does the Entrance menu's confirm steps
/// (ChaoStgEntrance fn_75_448: 0x1CD0, 0x1D18, 0x2678, gate 0xF5A0) once.
/// Slot 0 (the player's Chao) is left to the area change, which keeps the
/// held Chao (unverified); the menu's message step fn_75_B510 is skipped.
pub fn frame(c: &mut Cpu, m: &mut Mem) {
    let Some((ty, course)) = START.get() else { return };
    let (Some(cm), Some(en)) = (text(m, CHAOMAIN), text(m, ENTRANCE)) else { return };
    START.set(None);
    eprintln!("race-probe: starting race type {ty} course {course}");
    guest_call(c, m, cm + SET_RACE_TYPE, &[ty]);
    guest_call(c, m, cm + SET_RACE_COURSE, &[course]);
    guest_call(c, m, cm + SET_RACE_LEVEL, &[0]);
    guest_call(c, m, en + FILL_ENTRANTS, &[]);
    guest_call(c, m, cm + CHANGE_AREA, &[RACE_AREA]);
}

/// A linked module's .text (section 1), or None.
fn text(m: &Mem, id: u32) -> Option<u32> {
    let mut info = m.r32(sa2_recomp::rt::OS_MODULE_HEAD);
    while info != 0 {
        if m.r32(info) == id {
            return Some(m.r32(m.r32(info + 0x10) + 8) & !1);
        }
        info = m.r32(info + 4);
    }
    None
}
