//! Chao MMO glue, debug only (off unless SA2_RACE_PROBE is set): checks whether
//! a Chao's heading follows its twp angle Y, which the player-controlled Race
//! steering will write. Hooks the main.dol movement step `fn_8002F1C0(task,
//! flags)` that the Race run behaviour (ChaoStgRace fn_76_D2B4) calls, logs
//! each Chao task's angle Y and position every 30 calls, and with
//! SA2_RACE_PROBE=steer holds angle Y a quarter turn from its first value after
//! 300 calls. twp = task +0x34; angle (x, y, z) at twp +0x08, position at twp
//! +0x14 (CreateChildTask copies +0x08..+0x1C; ENGINE-MAP). Unverified: that
//! +0x0C is the heading the movement step uses; this probe is the check.
use crate::original;
use sa2_recomp::rt::{Cpu, Mem};
use sa2_recomp::{global, global::Global};
use std::collections::HashMap;

const MOVE_STEP: u32 = 0x8002_F1C0;
const QUARTER_TURN: u32 = 0x4000;

/// Per task: (calls seen, first angle Y).
static SEEN: Global<Option<HashMap<u32, (u32, u32)>>> = global!(None);
static STEER: Global<bool> = global!(false);

pub fn install(c: &mut Cpu, _m: &mut Mem) {
    let Ok(v) = std::env::var("SA2_RACE_PROBE") else { return };
    STEER.set(v.trim() == "steer");
    SEEN.set(Some(HashMap::new()));
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
