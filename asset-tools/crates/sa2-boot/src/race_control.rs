//! Chao MMO glue (on with SA2_CHAO_PLAYER=1): the player steers their own Chao
//! in a native Chao Race. The race itself stays the game's: speed, swim, fly,
//! climb, stamina and the A-button cheer (ChaoStgRace fn_76_7FF8 reads the
//! pad) all run as recompiled, and so does the run step's random stumble.
//! Two things are glue:
//!
//! - Lane 1 is the player's own Chao: entrant slot 0 gets the own Chao's save
//!   record through the game's AL_SetSelectedChaoSaveInfo while the Entrance
//!   is loaded and the slot is empty. Unverified: which native code fills
//!   slot 0 in a single-player race (not ChaoStgEntrance, Race, Lobby,
//!   Stadium, Karate or Kinder: none imports the setter for slot 0).
//! - Steering: the stick turns the Chao's heading (twp angle Y) right before
//!   the game's own movement step. Proven on the test build: the path follows
//!   an angle written there. Unverified: the sign of the turn.
//!
//! Offsets are GSNE8P, from the ChaoStgRace (module 76) and ChaoMain (60)
//! disassembly with the sa2b-decomp symbol names.
use crate::guest_call;
use sa2_recomp::rt::{self, Cpu, Mem};

const CHAOMAIN: u32 = 60;
const ENTRANCE: u32 = 75;
const RACE: u32 = 76;
/// main.dol movement step the race behaviours call (fn_76_D2B4 at 0xD500).
const MOVE_STEP: u32 = 0x8002_F1C0;
/// ChaoMain .text.
const AL_GET_SELECTED: u32 = 0x256C;
const AL_SET_SELECTED: u32 = 0x2584;
/// ChaoStgRace .bss 0x48: one 0x14-byte entry per entrant slot, its Chao task
/// at +0x10 (fn_76_1764 stores it at 0x192C after fn_76_103D4 creates it).
const RACERS: u32 = 0x48;
const RACER_TASK: u32 = 0x10;

/// Stick travel ignored around the centre, and the heading change per frame
/// at full stick (0x10000 = a full turn).
/// shortcut: both are hand-picked, there is no native player turn rate for a
/// racing Chao; tune after the first live run.
const DEADZONE: i32 = 16;
const TURN: i32 = 0x180;
const STICK_MAX: i32 = 72;

pub fn install(c: &mut Cpu, _m: &mut Mem) {
    if !std::env::var("SA2_CHAO_PLAYER").is_ok_and(|v| v.trim() == "1") {
        return;
    }
    // The debug probe owns the movement step when it is on.
    if std::env::var_os("SA2_RACE_PROBE").is_none() {
        c.rt.set_override(MOVE_STEP, move_step);
    }
}

/// The heading after one frame of stick: `angle` is the game's angle word
/// (0x10000 = a full turn, wrapping), `stick_x` the pad's -128..127.
pub fn steer(angle: u32, stick_x: i8) -> u32 {
    let x = i32::from(stick_x);
    if x.abs() <= DEADZONE {
        return angle;
    }
    let travel = (x.abs() - DEADZONE).min(STICK_MAX - DEADZONE);
    let step = TURN * travel / (STICK_MAX - DEADZONE) * x.signum();
    angle.wrapping_add(step as u32)
}

/// A linked module's section address, or None.
fn sec(m: &Mem, id: u32, n: u32) -> Option<u32> {
    std::iter::successors(Some(m.r32(rt::OS_MODULE_HEAD)), |&i| Some(m.r32(i + 4)))
        .take_while(|&i| i != 0)
        .take(256)
        .find(|&i| m.r32(i) == id)
        .map(|i| m.r32(m.r32(i + 0x10) + n * 8) & !1)
}

/// The player's racer task (entrant slot 0), or 0 outside a race.
fn racer(m: &Mem) -> u32 {
    sec(m, RACE, 6).map_or(0, |bss| m.r32(bss + RACERS + RACER_TASK))
}

fn move_step(c: &mut Cpu, m: &mut Mem) {
    let task = c.r[3];
    if task != 0 && task == racer(m) {
        let twp = m.r32(task + 0x34);
        if twp != 0 {
            let a = m.r32(twp + 0x0C);
            m.w32(twp + 0x0C, steer(a, crate::pad::stick_x()));
        }
    }
    crate::original(c, m, MOVE_STEP);
}

/// Every top-level frame: puts the player's own Chao in entrant slot 0 at
/// the Entrance.
pub fn frame(c: &mut Cpu, m: &mut Mem) {
    if !std::env::var("SA2_CHAO_PLAYER").is_ok_and(|v| v.trim() == "1") {
        return;
    }
    if sec(m, ENTRANCE, 1).is_some() {
        if let (Some(cm), Some(record)) = (sec(m, CHAOMAIN, 1), crate::chao_player::own_record(m)) {
            if guest_call(c, m, cm + AL_GET_SELECTED, &[0]) == 0 {
                guest_call(c, m, cm + AL_SET_SELECTED, &[0, record]);
                eprintln!("race-control: entrant slot 0 = own Chao record {record:#x}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stick_inside_the_deadzone_keeps_the_heading() {
        for x in [-16i8, -1, 0, 1, 16] {
            assert_eq!(steer(0x1234, x), 0x1234);
        }
    }

    #[test]
    fn full_stick_turns_by_the_rate_each_way_and_wraps() {
        assert_eq!(steer(0x4000, 127), 0x4000 + TURN as u32);
        assert_eq!(steer(0x4000, -128), 0x4000 - TURN as u32);
        assert_eq!(steer(0xFFFF, 127), 0xFFFF + TURN as u32);
        assert_eq!(steer(0, -128), 0u32.wrapping_sub(TURN as u32));
        assert_eq!(steer(u32::MAX, 127), TURN as u32 - 1);
    }

    #[test]
    fn part_stick_turns_less_and_never_past_the_rate() {
        let half = steer(0, 44);
        assert!(half > 0 && half < TURN as u32, "{half:#x}");
        assert_eq!(steer(0, 72), TURN as u32);
        assert_eq!(steer(0, 100), TURN as u32);
    }
}
