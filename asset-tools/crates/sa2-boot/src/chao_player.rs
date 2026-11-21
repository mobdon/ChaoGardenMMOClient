//! Play a story stage as a Chao: SA2_CHAO_PLAYER=1. Sonic keeps his own
//! physics, controls and task; only his draw is replaced by a native Chao.
//!
//! All game code and data are the game's own (GSNE8P main.dol, ChaoMain.prs,
//! ChaoMotionsD.rel); this module only calls them in an order no stage does:
//! - ChaoMain is read with 0x8001ACAC (async, like the garden's load) into
//!   the raised MEM1 above 0x81800000, expanded with the PRS expander
//!   0x800D2684 and linked with OSLink 0x800E839C. It is loaded once.
//! - Its prolog (ChaoMain 0x28A74) is run without `LoadSubprogram`
//!   (0x8001A01C), which would switch the game into Chao World: the motion
//!   load fn_60_1AEEC is done by hand the same way (ChaoMotionsD.rel, linked,
//!   its prolog run, the module/bss stored at sec6+0x374B8+0x30/+0x34), then
//!   fn_60_52318 and the flag sec6+0x41F48 = 1.
//! - Textures and palette as ChaoMain fn_60_28764 loads them (0x28AC4,
//!   0x1B350, eleven 0x28C50(name, texlist, 0) calls, 0x1FA10).
//! - ALW_Create (0x29344), then CreateChaoExtra (0x3148C) at the player.
//!
//! The steps run inside Sonic's exec (0x80083E44, wrapped), i.e. in the game's
//! own task loop, so the blocking texture loads behave as in the garden.
//! Each frame the Chao's position and rotation follow Sonic (data1 +0x14 and
//! +0x08); Sonic's display (0x8007BFF4) draws nothing once the Chao exists.
//! shortcut: the Chao plays its own behaviour/motions; Sonic's animation is
//! not retargeted onto it yet.
//! shortcut: the save info passed to CreateChaoExtra is a zeroed child
//! (type 2), not the player's own Chao from a save.
use crate::original;
use sa2_recomp::rt::{self, Cpu, Mem};
use std::cell::Cell;

const SONIC_EXEC: u32 = 0x8008_3E44;
const SONIC_DISP: u32 = 0x8007_BFF4;
const READ_FILE: u32 = 0x8001_ACAC;
const PRS_EXPAND: u32 = 0x800D_2684;
const OS_LINK: u32 = 0x800E_839C;

// Raised MEM1 layout (needs SA2_MEM1_MB >= 48).
const BUF: u32 = 0x8200_0000; // ChaoMain.prs as read
const BASE: u32 = 0x8240_0000; // ChaoMain expanded (0x434360 bytes)
const BSS: u32 = 0x8288_0000; // ChaoMain bss (0x50CD0)
const MOT: u32 = 0x82A0_0000; // ChaoMotionsD.rel (0x7FC34)
const MBSS: u32 = 0x82B0_0000;
const STR: u32 = 0x82F0_0000;
const SIZE: u32 = 0x82F0_0100;
const POS: u32 = 0x82F0_0200;
const INFO: u32 = 0x82F0_1000; // CreateChaoExtra save info (0x800)

const CHAO_PRS_LEN: u32 = 0x26_D7EA;
const TEXTURES: [(u32, u32); 11] = [
    (0x8078, 0x98308), (0x808C, 0x97A3C), (0x8094, 0x97C2C), (0x80A0, 0x97AC8),
    (0x80A8, 0x97AC0), (0x80B0, 0x97C34), (0x80BC, 0x97AB0), (0x80CC, 0x97AB8),
    (0x80DC, 0x98598), (0x80EC, 0x989A8), (0x80F8, 0x98F5C),
];

#[derive(Clone, Copy, PartialEq, Debug)]
enum Step {
    Wait(u32),
    ReadingMain,
    ReadingMotions,
    Live(u32), // the Chao's task
    Failed,
}

thread_local! {
    static STEP: Cell<Step> = const { Cell::new(Step::Wait(0)) };
    /// Sonic's data1 (position +0x14, rotation +0x08) for the Chao's display.
    static RIDER: Cell<u32> = const { Cell::new(0) };
}

/// ChaoMain offsets in its .text (section 1): the behaviour (AI) step the
/// Chao's exec runs each frame, and the Chao's display.
const AL_BEHAVIOR_CONTROL: u32 = 0xF8;
const CHAO_DISPLAY: u32 = 0x31950;
const CHAO_EXEC: u32 = 0x31B80;
const AL_SET_MOTION_LINK: u32 = 0x1B504;
/// Motion the garden trace shows while a Chao stands thinking (ALBHV_Think).
const STAND_MOTION: u32 = 1;

/// Run a REL function that this module overrides, by dropping the override
/// for the one call.
fn rel_original(c: &mut Cpu, m: &mut Mem, addr: u32, me: rt::Func) {
    c.rt.overrides.remove(&addr);
    c.rt.modules_changed();
    rt::call(c, m, addr);
    c.rt.set_override(addr, me);
}

/// The player Chao's AI does not run: it rides, it does not wander.
fn behavior(c: &mut Cpu, m: &mut Mem) {
    if let Step::Live(chao) = STEP.get() {
        if m.r32(chao + 0x34) == c.r[3] || chao == c.r[3] {
            return;
        }
    }
    let a = rt::sec(m, 60, 1) + AL_BEHAVIOR_CONTROL;
    rel_original(c, m, a, behavior);
}

/// Pin before the Chao's own exec too, so its emotion ball and shadow are
/// computed at the rider's position this frame.
fn chao_exec(c: &mut Cpu, m: &mut Mem) {
    if let Step::Live(chao) = STEP.get() {
        if c.r[3] == chao {
            pin(m, chao, RIDER.get());
        }
    }
    let a = rt::sec(m, 60, 1) + CHAO_EXEC;
    rel_original(c, m, a, chao_exec);
}

/// Place the Chao on the rider right before it draws (after its own exec).
fn chao_display(c: &mut Cpu, m: &mut Mem) {
    if let Step::Live(chao) = STEP.get() {
        if c.r[3] == chao {
            pin(m, chao, RIDER.get());
        }
    }
    let a = rt::sec(m, 60, 1) + CHAO_DISPLAY;
    rel_original(c, m, a, chao_display);
}

fn pin(m: &mut Mem, chao: u32, data1: u32) {
    let cw = m.r32(chao + 0x34);
    if cw == 0 || data1 == 0 {
        return;
    }
    for o in (0..12).step_by(4) {
        m.w32(cw + 0x14 + o, m.r32(data1 + 0x14 + o));
        m.w32(cw + 0x08 + o, m.r32(data1 + 0x08 + o));
    }
}

pub fn install(c: &mut Cpu, _m: &mut Mem) {
    if !std::env::var("SA2_CHAO_PLAYER").is_ok_and(|v| v.trim() == "1") {
        return;
    }
    if rt::mem1() < 0x0300_0000 {
        eprintln!("chao-player: needs SA2_MEM1_MB >= 48");
        return;
    }
    c.rt.set_override(SONIC_EXEC, exec);
    c.rt.set_override(SONIC_DISP, display);
}

fn call(c: &mut Cpu, m: &mut Mem, f: u32, args: &[u32]) -> u32 {
    crate::guest_call(c, m, f, args)
}

fn write_str(m: &mut Mem, s: &[u8]) {
    m.load(STR, s);
    m.w8(STR + s.len() as u32, 0);
}

fn zero(m: &mut Mem, a: u32, n: u32) {
    m.load(a, &vec![0; n as usize]);
}

fn display(c: &mut Cpu, m: &mut Mem) {
    if matches!(STEP.get(), Step::Live(_)) {
        return; // the Chao is drawn by its own task
    }
    original(c, m, SONIC_DISP);
}

fn exec(c: &mut Cpu, m: &mut Mem) {
    let task = c.r[3];
    original(c, m, SONIC_EXEC);
    let data1 = m.r32(task + 0x34);
    if data1 == 0 {
        return;
    }
    let next = match STEP.get() {
        // let the stage settle for a second before loading
        Step::Wait(n) if n < 60 => Step::Wait(n + 1),
        Step::Wait(_) => {
            write_str(m, b"ChaoMain.prs");
            m.w32(SIZE, 0);
            zero(m, BUF + CHAO_PRS_LEN - 8, 8);
            call(c, m, READ_FILE, &[STR, BUF, SIZE]);
            Step::ReadingMain
        }
        Step::ReadingMain => {
            if m.r32(BUF + CHAO_PRS_LEN - 8) == 0 && m.r32(BUF + CHAO_PRS_LEN - 4) == 0 {
                Step::ReadingMain
            } else {
                link_main(c, m)
            }
        }
        Step::ReadingMotions if m.r32(MOT) != 61 => Step::ReadingMotions,
        Step::ReadingMotions => setup_and_create(c, m, data1),
        Step::Live(chao) => {
            RIDER.set(data1);
            pin(m, chao, data1);
            Step::Live(chao)
        }
        Step::Failed => Step::Failed,
    };
    if next != STEP.get() && !matches!(next, Step::Wait(_)) {
        eprintln!("chao-player: {next:x?}");
    }
    STEP.set(next);
}

fn link_main(c: &mut Cpu, m: &mut Mem) -> Step {
    call(c, m, PRS_EXPAND, &[BUF, BASE]);
    if m.r32(BASE) != 60 {
        eprintln!("chao-player: ChaoMain did not expand (id {})", m.r32(BASE));
        return Step::Failed;
    }
    zero(m, BSS, m.r32(BASE + 0x20));
    if call(c, m, OS_LINK, &[BASE, BSS]) == 0 {
        eprintln!("chao-player: OSLink ChaoMain failed");
        return Step::Failed;
    }
    write_str(m, b"ChaoMotionsD.rel");
    zero(m, MOT, 0x10);
    call(c, m, READ_FILE, &[STR, MOT, SIZE]);
    Step::ReadingMotions
}

fn setup_and_create(c: &mut Cpu, m: &mut Mem, data1: u32) -> Step {
    zero(m, MBSS, m.r32(MOT + 0x20) + 1);
    if call(c, m, OS_LINK, &[MOT, MBSS]) == 0 {
        eprintln!("chao-player: OSLink ChaoMotionsD failed");
        return Step::Failed;
    }
    let prolog = m.r32(MOT + 0x34);
    call(c, m, prolog, &[]);
    let (s1, s4, s5, s6) = (rt::sec(m, 60, 1), rt::sec(m, 60, 4), rt::sec(m, 60, 5), rt::sec(m, 60, 6));
    m.w32(s6 + 0x374B8 + 0x30, MOT);
    m.w32(s6 + 0x374B8 + 0x34, MBSS);
    call(c, m, s1 + 0x52318, &[]);
    m.w32(s6 + 0x41F48, 1);
    call(c, m, s1 + 0x28AC4, &[]);
    call(c, m, s1 + 0x1B350, &[]);
    for (name, texlist) in TEXTURES {
        call(c, m, s1 + 0x28C50, &[s4 + name, s5 + texlist, 0]);
    }
    call(c, m, s1 + 0x1FA10, &[]);
    call(c, m, s1 + 0x29344, &[]); // ALW_Create
    zero(m, INFO, 0x800);
    m.w8(INFO + 0x80, 2);
    m.w8(INFO + 0x84, 0xFF);
    for o in (0..12).step_by(4) {
        m.w32(POS + o, m.r32(data1 + 0x14 + o));
    }
    let chao = call(c, m, s1 + 0x3148C, &[INFO, 0, 0, POS, 0]);
    if chao == 0 {
        eprintln!("chao-player: CreateChaoExtra returned 0");
        return Step::Failed;
    }
    let s1 = rt::sec(m, 60, 1);
    c.rt.set_override(s1 + AL_BEHAVIOR_CONTROL, behavior);
    c.rt.set_override(s1 + CHAO_DISPLAY, chao_display);
    c.rt.set_override(s1 + CHAO_EXEC, chao_exec);
    call(c, m, s1 + AL_SET_MOTION_LINK, &[chao, STAND_MOTION]);
    Step::Live(chao)
}
