//! Chao World chart on Tab: the Kindergarten Doctor's own medical chart,
//! opened for the player's Chao anywhere in Chao World.
//!
//! The chart is the game's own code in ChaoStgKinder (module 74, GSNE8P);
//! this module only loads it and calls it in an order no area does. Trace:
//! /mnt/project-files/native-menus/TAB-CHART-TRACE.md (static reading).
//! - Outside the Kindergarten the module isn't linked: ChaoStgKinder.prs is
//!   read from the player's files, expanded with the game's PRS expander
//!   0x800D2684 and linked with OSLink 0x800E839C into memory chao_player.rs
//!   only uses in stages. Its prolog is not run (it would create the
//!   Kindergarten rooms); its chart art is loaded with its own A10.
//! - The chart state is the Doctor room's 0x68-byte work (B1C8 allocates it
//!   with syCalloc 0x801197FC): +8 the Chao task, +0xC ChaoMain 15E44(chao),
//!   +0x60 the "MsgAlKinderHe" messages (ChaoMain 1CD20, rodata 0x2974).
//!   D208 opens it on the first (blue) page; C3D8 is its own input (L/R page,
//!   B closes); C3C4 says it closed; B244 is its display (task+0x40 = work).
//! - A task (CreateElementalTask 0x80071E0C) carries it: its main is a host
//!   function at a synthetic address (no guest code there), its second
//!   display (+0x1C) is the module's own B244, as the Doctor room sets it.
//! - Closing (or the game dropping the task on an area change) runs the
//!   task's delete (+0x18, also a host function), which frees what the Doctor
//!   room's own delete (BC18) frees: D238 (the
//!   portrait Chao), syFree(work+0x60), then the work; the borrowed module's
//!   textures (9E8) and link (OSUnlink 0x800E87F8) go too, so the
//!   Kindergarten area's own copy of module 74 is never linked twice.
//!
//! The page sends Tab (and the gamepad's Back) as /dev/pad key bit 15
//! (sa2-web game lib.rs -> [`set_tab`]).
//! shortcut: the player's Chao keeps reading the pad while the chart is open
//! (A/B also reach it), as nothing in the lobby stops it; upgrade by finding
//! the Chao World control lock the Doctor room uses, if any.
use crate::guest_call as call;
use sa2_recomp::rt::{self, Cpu, Mem};
use std::cell::Cell;

const KINDER: u32 = 74;
const CHAOMAIN: u32 = 60;
const FILE: &str = "ChaoStgKinder.prs";

const PRS_EXPAND: u32 = 0x800D_2684;
const OS_LINK: u32 = 0x800E_839C;
const OS_UNLINK: u32 = 0x800E_87F8;
const SY_CALLOC: u32 = 0x8011_97FC;
const SY_FREE: u32 = 0x8011_9828;
const CREATE_ELEMENTAL_TASK: u32 = 0x8007_1E0C;
const FREE_TASK: u32 = 0x8007_1AE0;
const STAGE_NUM: u32 = 0x803A_D820;

/// ChaoStgKinder .text offsets (section 1).
const TEX_LOAD: u32 = 0xA10;
const TEX_RELEASE: u32 = 0x9E8;
const DOCTOR_DISPLAY: u32 = 0xB244;
const INPUT: u32 = 0xC3D8;
const CLOSED: u32 = 0xC3C4;
const OPEN: u32 = 0xD208;
const FREE_PORTRAIT: u32 = 0xD238;
/// ChaoStgKinder rodata (section 4): "MsgAlKinderHe".
const MSG_NAME: u32 = 0x2974;
/// ChaoMain .text: the message file load and the chart's Chao info.
const MSG_LOAD: u32 = 0x1CD20;
const CHAO_INFO: u32 = 0x15E44;

const WORK_SIZE: u32 = 0x68;

/// Where a borrowed ChaoStgKinder goes: chao_player.rs's ChaoMain area,
/// free in Chao World (the game loads ChaoMain itself there).
const BUF: u32 = 0x8200_0000;
const BASE: u32 = 0x8240_0000;
const BSS: u32 = 0x8288_0000;

/// Host functions at addresses no guest code has (outside RAM).
const MAIN: u32 = 0x7E00_0000;
const DELETE: u32 = 0x7E00_0004;
/// The task's name, "MMOChart", after the borrowed bss.
const NAME: u32 = BSS + 0x1000;

#[derive(Clone, Copy, Debug, PartialEq)]
enum State {
    Closed,
    /// task, work, whether the module was borrowed (linked by us)
    Open { task: u32, work: u32, borrowed: bool },
}

thread_local! {
    static TAB: Cell<bool> = const { Cell::new(false) };
    static TAB_WAS: Cell<bool> = const { Cell::new(false) };
    static STATE: Cell<State> = const { Cell::new(State::Closed) };
}

/// This frame's Tab key (page /dev/pad key bit 15).
pub fn set_tab(down: bool) {
    TAB.set(down);
}

/// A press: down now, up last frame.
fn pressed(down: bool, was: bool) -> bool {
    down && !was
}

pub fn install(c: &mut Cpu, _m: &mut Mem) {
    c.rt.set_override(MAIN, main);
    c.rt.set_override(DELETE, delete);
}

/// Each frame from PADRead (pad.rs): a Tab press in Chao World opens the
/// chart; a second press closes it.
pub fn frame(c: &mut Cpu, m: &mut Mem) {
    let down = TAB.get();
    let press = pressed(down, TAB_WAS.replace(down));
    let in_world = m.r16(STAGE_NUM) as i16 >= 90 && linked(m, CHAOMAIN);
    match STATE.get() {
        State::Closed if press && in_world => {
            if let Some(chao) = crate::chao_player::player_chao() {
                STATE.set(open(c, m, chao));
            }
        }
        State::Open { work, .. } if press => {
            // The chart's own close (D1D4 via its B input) is D258: flag off.
            m.w8(work + 0x14, 0);
        }
        _ => {}
    }
}

fn open(c: &mut Cpu, m: &mut Mem, chao: u32) -> State {
    let borrowed = !linked(m, KINDER);
    if borrowed && !borrow(c, m) {
        return State::Closed;
    }
    let s1 = rt::sec(m, KINDER, 1);
    let s4 = rt::sec(m, KINDER, 4);
    let c1 = rt::sec(m, CHAOMAIN, 1);
    if borrowed {
        call(c, m, s1 + TEX_LOAD, &[]);
    }
    let work = call(c, m, SY_CALLOC, &[1, WORK_SIZE]);
    if work == 0 {
        eprintln!("kinder-chart: no memory for the chart");
        if borrowed {
            give_back(c, m);
        }
        return State::Closed;
    }
    m.w32(work + 8, chao);
    let info = call(c, m, c1 + CHAO_INFO, &[chao]);
    m.w32(work + 0xC, info);
    let msg = call(c, m, c1 + MSG_LOAD, &[s4 + MSG_NAME]);
    m.w32(work + 0x60, msg);
    m.load(NAME, b"MMOChart\0");
    let task = call(c, m, CREATE_ELEMENTAL_TASK, &[2, 2, MAIN, NAME]);
    if task == 0 {
        eprintln!("kinder-chart: no task for the chart");
        call(c, m, SY_FREE, &[msg]);
        call(c, m, SY_FREE, &[work]);
        if borrowed {
            give_back(c, m);
        }
        return State::Closed;
    }
    m.w32(task + 0x40, work);
    m.w32(task + 0x18, DELETE);
    m.w32(task + 0x1C, s1 + DOCTOR_DISPLAY);
    call(c, m, s1 + OPEN, &[work]);
    eprintln!("kinder-chart: open (module {})", if borrowed { "borrowed" } else { "the area's" });
    State::Open { task, work, borrowed }
}

/// The chart task's main: the chart's own input; once it says it closed,
/// the task goes (FreeTask runs [`delete`]).
fn main(c: &mut Cpu, m: &mut Mem) {
    let State::Open { task, work, .. } = STATE.get() else {
        return;
    };
    let s1 = rt::sec(m, KINDER, 1);
    call(c, m, s1 + INPUT, &[work]);
    if call(c, m, s1 + CLOSED, &[work]) != 0 {
        call(c, m, FREE_TASK, &[task]);
    }
}

/// The chart task's delete: free the chart as the Doctor room's BC18 does.
fn delete(c: &mut Cpu, m: &mut Mem) {
    let State::Open { task, work, borrowed } = STATE.replace(State::Closed) else {
        return;
    };
    let s1 = rt::sec(m, KINDER, 1);
    call(c, m, s1 + FREE_PORTRAIT, &[work]);
    call(c, m, SY_FREE, &[m.r32(work + 0x60)]);
    m.w32(task + 0x40, 0);
    m.w32(task + 0x1C, 0);
    call(c, m, SY_FREE, &[work]);
    if borrowed {
        give_back(c, m);
    }
    eprintln!("kinder-chart: closed");
}

/// Read, expand and link ChaoStgKinder from the player's files.
fn borrow(c: &mut Cpu, m: &mut Mem) -> bool {
    let Some(prs) = crate::dvd::read_file(FILE) else {
        eprintln!("kinder-chart: {FILE} not found");
        return false;
    };
    if prs.len() as u32 > BASE - BUF {
        eprintln!("kinder-chart: {FILE} is {} bytes, too big for its buffer", prs.len());
        return false;
    }
    m.load(BUF, &prs);
    call(c, m, PRS_EXPAND, &[BUF, BASE]);
    if m.r32(BASE) != KINDER {
        eprintln!("kinder-chart: {FILE} did not expand (id {})", m.r32(BASE));
        return false;
    }
    m.load(BSS, &vec![0; m.r32(BASE + 0x20) as usize]);
    if call(c, m, OS_LINK, &[BASE, BSS]) == 0 {
        eprintln!("kinder-chart: OSLink {FILE} failed");
        return false;
    }
    c.rt.modules_changed();
    true
}

fn give_back(c: &mut Cpu, m: &mut Mem) {
    call(c, m, rt::sec(m, KINDER, 1) + TEX_RELEASE, &[]);
    call(c, m, OS_UNLINK, &[BASE]);
    c.rt.modules_changed();
}

fn linked(m: &Mem, id: u32) -> bool {
    std::iter::successors(Some(m.r32(rt::OS_MODULE_HEAD)), |&i| Some(m.r32(i + 4)))
        .take_while(|&i| i != 0)
        .take(256)
        .any(|i| m.r32(i) == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_opens_once_per_press() {
        assert!(pressed(true, false));
        assert!(!pressed(true, true));
        assert!(!pressed(false, true));
        assert!(!pressed(false, false));
    }

    #[test]
    fn borrowed_module_fits_between_chao_player_areas() {
        // GC ChaoStgKinder.rel is 0xDE248 bytes expanded; bss 0x1A8.
        assert!(BASE + 0xDE248 <= BSS);
        assert!(BSS + 0x1A8 <= 0x82A0_0000, "chao_player MOT");
        assert!(NAME + 9 <= 0x82A0_0000);
    }
}
