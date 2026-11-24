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

#[path = "knux_rig.rs"]
mod knux_rig;

mod emerald_orbit;

const SONIC_EXEC: u32 = 0x8008_3E44;
const SONIC_DISP: u32 = 0x8007_BFF4;
const KNUX_EXEC: u32 = 0x8009_F8A0;
/// Tails's and Eggman's walkers (coordinator note 2026-11-23; unverified here).
const WALKER_EXEC: u32 = 0x800B_3A8C;
const KNUX_DISP: u32 = 0x8009_9BA0;
const ROUGE_DISP: u32 = 0x8009_8280;
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
    /// The frame the Chao went live (see `starting`).
    /// Sonic's data1 (position +0x14, rotation +0x08) for the Chao's display.
    static RIDER: Cell<u32> = const { Cell::new(0) };
    /// A new player's hatched Chao: its task seen in its exec, waiting to be
    /// taken over, and whether it has been (the player then has their Chao).
    static HATCHED: Cell<u32> = const { Cell::new(0) };
    static HATCH_TAKEN: Cell<bool> = const { Cell::new(false) };
    /// Rider position at the previous Chao display (for the emotion ball).
    static LAST: Cell<[f32; 3]> = const { Cell::new([0.0; 3]) };
}

/// ChaoMain offsets in its .text (section 1): the behaviour (AI) step the
/// Chao's exec runs each frame, and the Chao's display.
const AL_BEHAVIOR_CONTROL: u32 = 0xF8;
const CHAO_DISPLAY: u32 = 0x31950;
const CHAO_EXEC: u32 = 0x31B80;
const AL_SET_MOTION_LINK: u32 = 0x1B504;
const AL_DISABLE_NADE: u32 = 0x5A3C;
/// Motion the garden trace shows while a Chao stands thinking (ALBHV_Think).
const STAND_MOTION: u32 = 1;
// Chao World stand: alm_tati (motion 0), as the hub's idle.

/// Run a REL function that this module overrides, by dropping the override
/// for the one call.
pub(crate) fn rel_original(c: &mut Cpu, m: &mut Mem, addr: u32, me: rt::Func) {
    c.rt.overrides.remove(&addr);
    c.rt.modules_changed();
    rt::call(c, m, addr);
    c.rt.set_override(addr, me);
}

/// The player Chao's AI does not run: it rides, it does not wander.
pub(crate) fn behavior(c: &mut Cpu, m: &mut Mem) {
    // The lobby's standing resident (Tails, the guide) runs no AI either.
    if crate::start_stage::standing_resident(m, c.r[3]) {
        return;
    }
    // Except while the tutorial evolves it: the cocoon is a behaviour of its own.
    // Unverified until a run: that this is enough for ChaoMain's growth check to start it.
    if let Step::Live(chao) = STEP.get() {
        if (m.r32(chao + 0x34) == c.r[3] || chao == c.r[3]) && !crate::start_stage::evolving() {
            return;
        }
    }
    // Other players' Chao are draw-only: no local AI.
    if REMOTE.with_borrow(|r| r.values().any(|e| e.task != 0 && !e.idle && (e.task == c.r[3] || m.r32(e.task + 0x34) == c.r[3]))) {
        return;
    }
    let a = rt::sec(m, 60, 1) + AL_BEHAVIOR_CONTROL;
    rel_original(c, m, a, behavior);
}

/// Pin before the Chao's own exec too, so its emotion ball and shadow are
/// computed at the rider's position this frame.
fn chao_exec(c: &mut Cpu, m: &mut Mem) {
    if idle_hidden(m, c.r[3]) {
        return;
    }
    remote_pin(m, c.r[3]);
    // A new player's own Chao is the one that hatches from their egg: the Chao
    // whose record (chaowk +0x5C) is the egg's save slot. tick() takes it over.
    if new_player_waiting() {
        let cw = m.r32(c.r[3] + 0x34);
        let slot = crate::start_stage::hatch_slot().unwrap_or(0);
        if slot != 0 && (0x8000_0000..0x8C00_0000).contains(&cw) && m.r32(cw + 0x5C) == slot {
            HATCHED.set(c.r[3]);
        }
    }
    // Beyond the REMOTE_NEAR nearest, a remote Chao's exec (motion and
    // ChaoMain's per-frame work) runs every REMOTE_FAR_EVERY frames; its
    // position still follows every frame and it is drawn every frame.
    let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
    let skip = REMOTE.with_borrow(|r| r.values().any(|e| e.task != 0 && !e.idle && e.task == c.r[3] && (e.offscreen || (e.rank >= REMOTE_NEAR && (f + e.rank as u64) % REMOTE_FAR_EVERY != 0))));
    if skip {
        return;
    }
    if let Step::Live(chao) = STEP.get() {
        if c.r[3] == chao {
            pin(m, chao, RIDER.get());
        }
    }
    // SA2_EV_SKIP=exec,display (debug): skip this step in a cutscene.
    if RIDER.get() == 0 && std::env::var("SA2_EV_SKIP").is_ok_and(|v| v.contains("exec")) {
        return;
    }
    let a = rt::sec(m, 60, 1) + CHAO_EXEC;
    // The exec's own walker (.text 0xAA0) also calls the node transform
    // 0x1454: keep the pose for the draw walker only, or the exec uses up
    // the node indices 0..39 and the draw gets none.
    let draw = CHAO_DRAW.replace(false);
    let me = c.r[3];
    rel_original(c, m, a, chao_exec);
    CHAO_DRAW.set(draw);
    // The character's exec owns the heading while it moves the Chao: the
    // Chao's own exec turning its angle afterwards made it drift round when
    // walking sideways (mob). unverified: which Chao step turns it.
    if let (Step::Live(chao), Some(ang)) = (STEP.get(), CHAR_ANG.get()) {
        if me == chao && PMOVE_CHAO.get() == chao && self_drive(m) {
            let cw = m.r32(chao + 0x34);
            // SA2_YAW_LOG=1 (debug): every frame, the yaw the character set and
            // the one the Chao's exec left, with the position, to see a drift or jerk.
            if std::env::var("SA2_YAW_LOG").is_ok_and(|v| v == "1") {
                let f = |a: u32| f32::from_bits(m.r32(a));
                eprintln!("yaw: frame {} char {:#06x} chao {:#06x} pos ({:.2},{:.2})", sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed), ang[1] & 0xFFFF, m.r32(cw + 0x0C) & 0xFFFF, f(cw + 0x14), f(cw + 0x1C));
            }
            // His whole angle; the draw drops his pitch and roll (SA2_CHAO_TILT there).
            for (i, v) in ang.into_iter().enumerate() {
                m.w32(cw + 0x08 + 4 * i as u32, v);
            }
        }
    }
}

/// mob 2026-11-21 (board.py): Sonic's board drawn at the Chao's size.
const BOARD_SCALE: f32 = 0.40;

thread_local! {
    /// The board drawn this frame: (frame, triangles in Chao space from
    /// Sonic's model space, board centroid in world).
    static BOARD: std::cell::RefCell<Option<(u64, Vec<[crate::chao_retarget::V3; 3]>, [f32; 3])>> = const { std::cell::RefCell::new(None) };
}

fn apply(a: &[[f32; 4]; 3], p: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| (0..3).map(|k| a[i][k] * p[k]).sum::<f32>() + a[i][3])
}

/// City Escape's board task display (stg13D ObjectBoardDisp, .text 0x518,
/// size 0x218, decomp o_ce_board.c): its Ninja draws are the board.
fn from_board_disp(c: &Cpu, m: &Mem) -> bool {
    const STAGE: u32 = 0x803A_D820;
    if RIDER.get() == 0 || m.r16(STAGE) != 13 {
        return false;
    }
    // The stage number is set before stg13D (module 13) links, and sec()
    // panics while unlinked: look in the OS module list first (no caught
    // panic: the web build cannot unwind).
    let linked = std::iter::successors(Some(m.r32(rt::OS_MODULE_HEAD)), |&i| Some(m.r32(i + 4)))
        .take_while(|&i| i != 0)
        .take(256)
        .any(|i| m.r32(i) == 13);
    let Some(text) = linked.then(|| rt::sec(m, 13, 1)) else {
        if sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) % 60 == 0 {
            eprintln!("chao-player lvl: stg13D unlinked; draw lr {:#x} obj {:#x}", c.lr, c.r[3]);
        }
        return false;
    };
    let disp = text + 0x518;
    (disp..disp + 0x218).contains(&c.lr)
}

/// The board (rest vertices `verts`, object space) drawn scaled about its
/// centre; remembered for the seat.
fn board_draw(c: &mut Cpu, m: &mut Mem, addr: u32) {
    let (obj, root) = (c.r[3], SKEL.with_borrow(|s| s.first().copied()));
    let Some(root) = root else { return original(c, m, addr) };
    let rest = crate::chao_retarget::read_chao(&|a| m.r32(a), obj, [vec![], vec![]]);
    let cur_ptr = m.r32(NJ_CURRENT_MATRIX);
    let cur = mtx(m, cur_ptr);
    let base = mtx(m, m.r32(NJ_BASE_MATRIX));
    // A capture pass (drawing nothing) for the board's node matrices as its
    // motion poses them this frame: the rest pose is not where it draws.
    let (regs, f1) = (c.r, c.f[1]);
    NODES.with_borrow_mut(|n| n.clear());
    // Only the motion draws walk nodes through the captured transform.
    if addr != 0x8011_DF44 {
        CAPTURE.set(true);
        original(c, m, addr);
        CAPTURE.set(false);
        (c.r, c.f[1]) = (regs, f1);
    }
    for r in 0..3 {
        for k in 0..4 {
            m.w32(cur_ptr + 4 * (4 * r + k) as u32, cur[r][k].to_bits());
        }
    }
    let nodes = NODES.with_borrow(|n| n.clone());
    let w = inv_mul(&base, &cur);
    // World node matrices: captured, or the rest tree under the board placement.
    let worlds: Vec<[[f32; 4]; 3]> = if nodes.len() == rest.nodes.len() {
        nodes
    } else {
        rest.worlds(&[]).iter().map(|(r, t)| std::array::from_fn(|i| std::array::from_fn(|j| {
            if j < 3 { (0..3).map(|k| w[i][k] * r[k][j]).sum() } else { (0..3).map(|k| w[i][k] * t[k]).sum::<f32>() + w[i][3] }
        }))).collect()
    };
    let verts: Vec<[f32; 3]> = rest.verts.iter().zip(&worlds).flat_map(|(vs, wm)| vs.iter().map(|&v| apply(wm, v)).collect::<Vec<_>>()).collect();
    if verts.is_empty() {
        return original(c, m, addr);
    }
    let cw = verts.iter().fold([0.0f32; 3], |a, v| [a[0] + v[0], a[1] + v[1], a[2] + v[2]]).map(|x| x / verts.len() as f32);
    let rel = inv_mul(&root, &[[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]]);
    let tris = verts.iter().map(|&v| { let q = crate::chao_retarget::to_chao(apply(&rel, v)); [q, q, q] }).collect();
    let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
    if f % 60 == 0 {
        eprintln!("chao-player lvl: frame {f} board obj {obj:#x} nodes {}/{} verts {} centre {cw:?}", worlds.len(), rest.nodes.len(), verts.len());
    }
    // The centre in Sonic's model space: the Chao draws before the board, so
    // seat() places it with next frame's root instead of lagging a frame.
    BOARD.set(Some((f, tris, apply(&rel, cw))));
    BOARD_AXES.set(Some((f, w[0][0].atan2(w[2][0]).to_degrees(), w[0][2].atan2(w[2][2]).to_degrees())));
    // Draw scaled about its world centre: cur' = base * S_c * inv(base) * cur.
    let s = BOARD_SCALE;
    let ws: [[f32; 4]; 3] = std::array::from_fn(|i| std::array::from_fn(|j| if j < 3 { w[i][j] * s } else { w[i][3] * s + (1.0 - s) * cw[i] }));
    for r in 0..3 {
        for k in 0..4 {
            let v = (0..3).map(|q| base[r][q] * ws[q][k]).sum::<f32>() + if k == 3 { base[r][3] } else { 0.0 };
            m.w32(cur_ptr + 4 * (4 * r + k) as u32, v.to_bits());
        }
    }
    original(c, m, addr);
    for r in 0..3 {
        for k in 0..4 {
            m.w32(cur_ptr + 4 * (4 * r + k) as u32, cur[r][k].to_bits());
        }
    }
}

thread_local! {
    static SEEN_SKEL: std::cell::RefCell<std::collections::BTreeSet<(u32, usize)>> = Default::default();
    /// Set while Sonic's in-stage display is walked for his skeleton.
    static LCAP: Cell<bool> = const { Cell::new(false) };
}

/// A Ninja draw inside Sonic's in-stage display: record the node matrices
/// of each motion draw, draw nothing.
fn level_capture(c: &mut Cpu, m: &mut Mem, addr: u32) {
    if CAPTURE.get() {
        if addr == 0x8011_DF44 {
            return;
        }
        return original(c, m, addr);
    }
    let (obj, mot) = (c.r[3], c.r[4]);
    let (regs, f1) = (c.r, c.f[1]);
    NODES.with_borrow_mut(|n| n.clear());
    CAPTURE.set(true);
    original(c, m, addr);
    CAPTURE.set(false);
    // SA2_SHOW_SONIC (debug): draw Sonic too, to compare the seat with his.
    // SA2_SHOW_SONIC=ahead,side (test): drawn that far ahead of and beside his own place (his
    // model units along his own axes), so he and the Chao stand next to each other in one picture.
    if let Ok(v) = std::env::var("SA2_SHOW_SONIC") {
        (c.r, c.f[1]) = (regs, f1);
        let ofs: Vec<f32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        let cur_ptr = m.r32(NJ_CURRENT_MATRIX);
        let keep = mtx(m, cur_ptr);
        if let [ahead, side] = ofs[..] {
            // He faces -x; z is his side.
            for r in 0..3 { m.w32(cur_ptr + 4 * (4 * r + 3) as u32, (keep[r][3] - keep[r][0] * ahead + keep[r][2] * side).to_bits()); }
        }
        original(c, m, addr);
        for r in 0..3 { m.w32(cur_ptr + 4 * (4 * r + 3) as u32, keep[r][3].to_bits()); }
    }
    let n = NODES.with_borrow(|n| n.len());
    let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
    if f % 60 == 0 {
        eprintln!("chao-player lvl: frame {f} draw {addr:#x} obj {obj:#x} motion {mot:#x} nodes {n}");
    }
    let t = std::env::var("SA2_DISP_TEST").unwrap_or_default();
    if n >= 56 && t == "noskel" {
    } else if n >= 56 && t == "skel10" {
        SKEL.set(NODES.with_borrow(|n| n[..10].to_vec()));
    } else if n >= 56 && t == "noobj" {
        SKEL.set(NODES.with_borrow(|n| n.clone()));
    } else if n >= 56 {
        let nodes = NODES.with_borrow(|n| n.clone());
        // Self mode: captured under the scaled view, back to full size.
        let nodes = match SELF_SCALE.get() {
            Some((cur, curs)) => nodes.iter().map(|e| mul34(&cur, &inv_mul(&curs, e))).collect(),
            None => nodes,
        };
        SKEL.set(nodes);
        // The motion and frame this skeleton was drawn with (stride_lock).
        RIDER_MOTION.set((mot, f1.ps0 as f32));
        if SELF_CAP.get() {
            RIDER_OBJ.set(obj);
            return;
        }
        RIDER_OBJ.set(obj);
        // Knuckles's run states: the overlay's skeleton is Shadow's body walked with Shadow's
        // skate or Sonic's run (knux_rig::run_style), under the same placement, drawing nothing.
        if LAST_EXEC.get() == KNUX_EXEC && addr == DRAW_MOTION {
            if let Some((body, motion, frame)) = knux_rig::run_source(m, mot, f1.ps0 as f32) {
                (c.r, c.f[1]) = (regs, f1);
                (c.r[3], c.r[4]) = (body, motion);
                c.f[1].ps0 = f64::from(frame);
                NODES.with_borrow_mut(|n| n.clear());
                CAPTURE.set(true);
                original(c, m, addr);
                CAPTURE.set(false);
                if NODES.with_borrow(|n| n.len()) == 62 {
                    SKEL.set(NODES.with_borrow(|n| n.clone()));
                    RIDER_OBJ.set(body);
                }
            }
        }
    } else if n > 0 && SELF_CAP.get() {
        // Self mode (Chao World): no spin ball; the Chao stays and plays its
        // own motion in the air (mob 2026-11-23: the ball only in Sonic's and
        // Shadow's own levels). SA2_SELF_BALL=1 (debug): the ball as he draws it.
        if std::env::var("SA2_SELF_BALL").is_ok_and(|v| v == "1") {
            (c.r, c.f[1]) = (regs, f1);
            original(c, m, addr);
        }
    } else if n > 0 {
        // In the ball the motion draw is the ball model alone (2 nodes, lvl9\g30b).
        let obj2 = m.r32(P1_OBJ2);
        if (0x8000_0000..0x8C00_0000).contains(&obj2) && in_ball(m, obj2) {
            (c.r, c.f[1]) = (regs, f1);
            // The ball's centre: its root node as this frame's capture walked it.
            let Some(b) = NODES.with_borrow(|n| n.first().map(|r| [r[0][3], r[1][3], r[2][3]])) else { return };
            ball_draw(c, m, addr, b);
        }
    }
    // A new rider skeleton size: its node positions in root space, once.
    if n > 8 && SEEN_SKEL.with_borrow_mut(|s| s.insert((obj, n))) {
        let nodes = NODES.with_borrow(|v| v.clone());
        let p: Vec<[f32; 3]> = nodes.iter().map(|e| { let r = inv_mul(&nodes[0], e); [r[0][3], r[1][3], r[2][3]].map(|x| (x * 1000.0).round() / 1000.0) }).collect();
        eprintln!("chao-player skel: obj {obj:#x} nodes {n} root-space {p:?}");
    }
}

/// State 30 (PLMOT_BALL): the rider's motion draw is the ball model alone
/// (SA2_SHOW_SONIC in Green Forest, lvl9\g30s). Forced 11/12 drew the body
/// (lvl9\g11s, g12s).
const BALL_STATE: u32 = 30;

/// The rider is in the ball: his state's PL_ACTION entry (obj2 +0x198,
/// 0x10 bytes each) plays the same motion number as state 30's, so every
/// state the game itself draws as the ball (spin dash, homing) counts.
fn in_ball(m: &Mem, obj2: u32) -> bool {
    let table = m.r32(obj2 + OBJ2_PLACTPTR);
    if !(0x8000_0000..0x8C00_0000).contains(&table) {
        return false;
    }
    let act = u32::from(m.r16(obj2 + OBJ2_ACTION));
    let ball = m.r16(table + 0x10 * BALL_STATE);
    if !SAID_BALL.replace(true) {
        let all: Vec<u32> = (0..256).filter(|&i| m.r16(table + 0x10 * i) == ball).collect();
        eprintln!("chao-player ball: motion {ball} in states {all:?}");
        if std::env::var_os("SA2_PLACT_DUMP").is_some() {
            for i in 0..160 {
                let e: Vec<u16> = (0..8).map(|k| m.r16(table + 0x10 * i + 2 * k)).collect();
                eprintln!("plact {i}: {e:04x?}");
            }
        }
    }
    m.r16(table + 0x10 * act) == ball
}

thread_local! {
    static SAID_BALL: Cell<bool> = const { Cell::new(false) };
}

/// The game's own ball draw, scaled to the Chao's size (BOARD_SCALE, the
/// Sonic -> Chao size used for the board) and centred on the Chao.
fn ball_draw(c: &mut Cpu, m: &mut Mem, addr: u32, b: [f32; 3]) {
    let Step::Live(chao) = STEP.get() else { return };
    let cwk = m.r32(chao + 0x34);
    // The Chao's centre: its position (chaowk +0x14) raised by its rest mesh centroid height.
    let cy = CHAO_REST.with_borrow(|r| r.as_ref().map(|r| {
        let w = r.worlds(&[]);
        let v: Vec<f32> = r.verts.iter().zip(&w).flat_map(|(vs, (rr, t))| vs.iter().map(move |v| t[1] + (0..3).map(|k| rr[1][k] * v[k]).sum::<f32>())).collect();
        v.iter().sum::<f32>() / v.len().max(1) as f32
    })).unwrap_or(0.0);
    let f = |a: u32| f32::from_bits(m.r32(a));
    let target = [f(cwk + 0x14), f(cwk + 0x18) + cy, f(cwk + 0x1C)];
    let cur_ptr = m.r32(NJ_CURRENT_MATRIX);
    let cur = mtx(m, cur_ptr);
    let base = mtx(m, m.r32(NJ_BASE_MATRIX));
    let w = inv_mul(&base, &cur);
    // The captured centre is in view space (the walk's current matrix): to world.
    let bv: [[f32; 4]; 3] = std::array::from_fn(|i| std::array::from_fn(|j| if j < 3 { f32::from(u8::from(i == j)) } else { b[i] }));
    let bw = inv_mul(&base, &bv);
    let b = [bw[0][3], bw[1][3], bw[2][3]];
    let s = BOARD_SCALE;
    // Scale about the ball's centre, then move that centre onto the Chao's.
    let ws: [[f32; 4]; 3] = std::array::from_fn(|i| std::array::from_fn(|j| if j < 3 { w[i][j] * s } else { w[i][3] * s + (1.0 - s) * b[i] + target[i] - b[i] }));
    let scaled = mul34(&base, &ws);
    for r in 0..3 {
        for k in 0..4 {
            m.w32(cur_ptr + 4 * (4 * r + k) as u32, scaled[r][k].to_bits());
        }
    }
    // The Dark Chao's ball is Shadow's own: his model of the same slot
    // (Sonic's model number + 130, the offset Sonic's display uses between
    // Sonic's 43 and Shadow's 173) with Shadow's texture list.
    // unverified: the +130 offset holds for the ball slot (check the crop).
    let cur_list = m.r32(c.r[13].wrapping_sub(0x7228)) + 0x20;
    let keep = (m.r32(cur_list), c.r[3]);
    if let Some(tex) = SHADOW_TEX.get().filter(|_| addr == DRAW_MOTION && std::env::var("SA2_BALL_SH").map_or(true, |v| v != "0")) {
        let k = (0..0x428).find(|&k| model_obj(m, k) == c.r[3]);
        if let Some(sh) = k.map(|k| model_obj(m, k + 130)).filter(|&o| o != 0) {
            c.r[3] = sh;
            m.w32(cur_list, tex);
        }
    }
    original(c, m, addr);
    m.w32(cur_list, keep.0);
    c.r[3] = keep.1;
    for r in 0..3 {
        for k in 0..4 {
            m.w32(cur_ptr + 4 * (4 * r + k) as u32, cur[r][k].to_bits());
        }
    }
}

thread_local! {
    /// Shadow's texture list once loaded into a Sonic level.
    static SHADOW_TEX: Cell<Option<u32>> = const { Cell::new(None) };
}
/// The character loader's calls (Sonic's 0x800874C0 at 0x800875F0/0x800875FC):
/// texture list by name 0x80072548 and model file by name 0x80050210, which
/// fills the model table 0x801E6688; with Shadow's own names TERIOSTEX
/// (0x801748E0) and TERIOSMDL.PRS (0x801748EC).
const LOAD_TEX: u32 = 0x8007_2548;
const LOAD_MDL: u32 = 0x8005_0210;
const SHADOW_TEX_NAME: u32 = 0x8017_48E0;
const SHADOW_MDL_NAME: u32 = 0x8017_48EC;
const MODELS: u32 = 0x801E_6688;

thread_local! {
    /// Model slots this module loaded, kept out of the game's table.
    static EXTRA_MODELS: std::cell::RefCell<std::collections::BTreeMap<u32, u32>> = Default::default();
    /// Every slot the last load_models call filled or replaced, with its model.
    static LAST_LOAD: std::cell::RefCell<std::collections::BTreeMap<u32, u32>> = Default::default();
    /// KNUCKMDL's own models by slot (the gloves and Knuckles's body), whatever
    /// the shared table or EXTRA_MODELS hold there.
    static KNUX_MODELS: std::cell::RefCell<std::collections::BTreeMap<u32, u32>> = Default::default();
    /// TERIOSMDL's own models by slot (the Air Shoes and Shadow's body):
    /// a slot the game's table already held kept the table's model, so
    /// model_obj there drew another file's model (a hand) on the feet.
    static SHADOW_MODELS: std::cell::RefCell<std::collections::BTreeMap<u32, u32>> = Default::default();
}

/// A TERIOSMDL model by slot: its own load first, then the shared lookup.
fn shadow_obj(m: &Mem, k: u32) -> u32 {
    SHADOW_MODELS.with_borrow(|e| e.get(&k).copied()).unwrap_or_else(|| model_obj(m, k))
}

/// A model slot: one this module loaded, else the game's table.
/// SA2_GEAR_TUNE=<file> (debug, the page's ?geartune=1 panel): per gear piece
/// "name dx dy dz rx ry rz scale", read about 4 times a second, applied in
/// the piece's own model frame after its fit (move, then turn x, y, z in
/// degrees, then scale). None when there is no tuner or no line for it.
fn gear_tune(name: &str) -> Option<[[f32; 4]; 3]> {
    let path = std::env::var("SA2_GEAR_TUNE").ok()?;
    let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
    if GEAR_TUNE_AT.replace(f / 15) != f / 15 {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let map = text.lines().filter_map(|l| {
            let v: Vec<&str> = l.split_whitespace().collect();
            let n: Vec<f32> = v.get(1..8)?.iter().filter_map(|x| x.parse().ok()).collect();
            (n.len() == 7).then(|| (v[0].to_string(), [n[0], n[1], n[2], n[3], n[4], n[5], n[6]]))
        }).collect();
        GEAR_TUNE.set(map);
        // "set key value" lines: the panel's effect toggles and sound mute list.
        let sets = text.lines().filter_map(|l| {
            let mut w = l.splitn(3, ' ');
            (w.next()? == "set").then_some(())?;
            Some((w.next()?.to_string(), w.next().unwrap_or("").trim().to_string()))
        }).collect();
        DEBUG_SETS.set(sets);
    }
    let v = GEAR_TUNE.with_borrow(|t| t.get(name).copied())?;
    Some(tune_matrix(&v))
}

/// The fists' set places ("fist0" and "fist1" tuner lines: dx dy dz rx ry rz
/// scale). Fist 1 (fist0) is tuned by hand. Fist 2 is drawn as its reflection
/// across the Chao's left-right plane (mirror_fist, from the two fits), also
/// in the tuner while its "Link fists" is on. Fist 2's line here is that
/// reflection written out (SA2_FIST_LOG=1 logs it), where the panel's fist 2
/// sliders start when unlinked; it was [0.3, 1.26, 1.8, 60, -10, 0, 1.35] by
/// hand before (0.073 model units and 7.6 degrees off the reflection).
/// The left plate (331) is the right one's (329) mirror along its own z, not
/// x, and each is fitted to its own arm, so the two lines differ in more than a sign.
const FIST_SET: [[f32; 7]; 2] = [
    [0.3, 1.26, -2.2, -60.0, 10.0, 0.0, 1.35],
    [0.137, 1.368, 1.873, 53.84, -14.64, 0.72, 1.32],
];

thread_local! {
    /// The two gloves' fitted places in the Chao's rest model space.
    static FIST_D: Cell<Option<([[f32; 4]; 3], [[f32; 4]; 3])>> = const { Cell::new(None) };
    /// Fist 2's line as last logged.
    static FIST2_SAID: Cell<[f32; 7]> = const { Cell::new([0.0; 7]) };
}

/// Fist 2's tuner matrix as the reflection of fist 1's (`t1`), from the
/// fitted gloves (`fit`: per arm node 3 / 10, the glove's place under its arm
/// node at rest).
fn fist2_mirror(m: &Mem, fit: &[(u32, [[f32; 4]; 3]); 2], t1: &[[f32; 4]; 3]) -> Option<[[f32; 4]; 3]> {
    let (d1, d2) = match FIST_D.get() {
        Some(d) => d,
        None => {
            let w = CHAO_REST.with_borrow(|r| r.as_ref().map(|r| r.worlds(&[])))?;
            let world = |n: usize| -> [[f32; 4]; 3] { let (r, t) = w[n]; std::array::from_fn(|i| [r[i][0], r[i][1], r[i][2], t[i]]) };
            let d = (mul34(&world(3), &fit[0].1), mul34(&world(10), &fit[1].1));
            FIST_D.set(Some(d));
            d
        }
    };
    let t1 = *t1;
    let t2 = mirror_fist(&d1, &t1, &d2);
    let line = tune_values(&t2);
    let said = FIST2_SAID.get();
    let log = std::env::var("SA2_FIST_LOG").is_ok_and(|v| v == "1");
    // With the tuner or the log: fist 2's line whenever fist 1's moves it.
    if (log || std::env::var_os("SA2_GEAR_TUNE").is_some()) && (0..7).any(|i| (line[i] - said[i]).abs() > 1e-3) {
        eprintln!("fist-log: fist 2 follows fist 1 by reflection: fist1 {:?} -> fist1 line for fist 2 {line:?}", tune_values(&t1));
    }
    if log && said == [0.0; 7] {
        // How far each plate is from the other's z mirror (0 = exact mirrors).
        let verts = |model: u32| -> Vec<[f32; 3]> {
            let g = crate::chao_retarget::read_chao(&|a| m.r32(a), knux_obj(m, model), [vec![], vec![]]);
            g.verts.iter().zip(g.worlds(&[])).flat_map(|(vs, (r, t))| vs.iter().map(move |v| std::array::from_fn(|i| t[i] + (0..3).map(|q| r[i][q] * v[q]).sum::<f32>())).collect::<Vec<_>>()).collect()
        };
        let (a, b) = (verts(fit[0].0), verts(fit[1].0));
        let worst = a.iter().map(|p| b.iter().map(|q| ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] + q[2]).powi(2)).sqrt()).fold(f32::MAX, f32::min)).fold(0.0f32, f32::max);
        // The hand-set fist 2 against the mirror: where the plate's origin sat, and its turn.
        let old = mul34(&d2, &tune_matrix(&FIST_SET[1]));
        let new = mul34(&d2, &t2);
        let dp: [f32; 3] = std::array::from_fn(|i| old[i][3] - new[i][3]);
        let r3 = |x: &[[f32; 4]; 3]| -> [[f32; 3]; 3] { std::array::from_fn(|i| std::array::from_fn(|j| x[i][j])) };
        eprintln!("fist-log: plates {} / {} ({} / {} vertices), worst distance from the other's z mirror {worst:.4}", fit[0].0, fit[1].0, a.len(), b.len());
        eprintln!("fist-log: fist 2 as fist 1's mirror: line {:?}; the set line is off by {dp:?} (Chao model units, {:.3} long) and {:.2} degrees", tune_values(&t2), (dp[0] * dp[0] + dp[1] * dp[1] + dp[2] * dp[2]).sqrt(), crate::skate::angle_between(&r3(&old), &r3(&new)));
        // The plate's box, for the drawn check (fist_drawn_log).
        let (lo, hi) = a.iter().fold(([f32::MAX; 3], [f32::MIN; 3]), |(l, h), v| (std::array::from_fn(|i| l[i].min(v[i])), std::array::from_fn(|i| h[i].max(v[i]))));
        FIST_BOX.set(Some((lo, hi)));
    }
    FIST2_SAID.set(line);
    Some(t2)
}

thread_local! {
    static FIST_BOX: Cell<Option<([f32; 3], [f32; 3])>> = const { Cell::new(None) };
}

/// SA2_FIST_LOG=1, once a second: the two gloves as drawn this frame (view
/// space), in the Chao's own frame: fist 2's plate corners reflected across
/// the Chao's left-right plane against fist 1's (largest distance in Chao
/// model units, and the angle between the two turns). Standing it shows how
/// far the drawn pair is from a mirror (the arms' own pose included); in a
/// run the arms swing opposite ways, so it is not zero there.
fn fist_drawn_log(places: &[Option<[[f32; 4]; 3]>; 2]) {
    let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
    let (Some(p1), Some(p2), Some((lo, hi))) = (places[0], places[1], FIST_BOX.get()) else { return };
    if f % 60 != 0 {
        return;
    }
    let chao = CHAO_PLACE.get();
    let (l1, l2) = (inv_mul(&chao, &p1), inv_mul(&chao, &p2));
    let mut worst = 0.0f32;
    for k in 0..8 {
        let g: [f32; 3] = std::array::from_fn(|i| if k >> i & 1 == 0 { lo[i] } else { hi[i] });
        let (a, b) = (apply(&l1, g), apply(&l2, [g[0], g[1], -g[2]]));
        worst = worst.max(((a[0] + b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt());
    }
    // fist 2's turn reflected: MX * R2 * MZ.
    let r1: [[f32; 3]; 3] = std::array::from_fn(|i| std::array::from_fn(|j| l1[i][j]));
    let r2: [[f32; 3]; 3] = std::array::from_fn(|i| std::array::from_fn(|j| l2[i][j] * if i == 0 { -1.0 } else { 1.0 } * if j == 2 { -1.0 } else { 1.0 }));
    eprintln!("fist-log: drawn f{f} motion {} moving {}: fist 2 reflected against fist 1: corners up to {worst:.4} model units apart, turn {:.2} degrees apart", LAST_MOTION.get(), MOVING.get(), crate::skate::angle_between(&r1, &r2));
}

/// Fist 2's set place as the mirror of fist 1's (mob 2026-11-23: fist 1 is
/// the hand-tuned one). `d1`, `d2`: each glove's fitted place in the Chao's
/// rest model space; `t1`: fist 1's tuner matrix. The left plate is the right
/// plate mirrored along its own z, so fist 2 is fist 1 mirrored across the
/// Chao's left-right plane when d2 * T2 * MZ = MX * d1 * t1.
pub(crate) fn mirror_fist(d1: &[[f32; 4]; 3], t1: &[[f32; 4]; 3], d2: &[[f32; 4]; 3]) -> [[f32; 4]; 3] {
    const MX: [[f32; 4]; 3] = [[-1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]];
    const MZ: [[f32; 4]; 3] = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, -1.0, 0.0]];
    inv_mul(d2, &mul34(&mul34(&mul34(&MX, d1), t1), &MZ))
}

/// A tuner matrix back as its line "dx dy dz rx ry rz scale" (tune_matrix's inverse).
pub(crate) fn tune_values(t: &[[f32; 4]; 3]) -> [f32; 7] {
    let s = (0..3).map(|i| t[i][0] * t[i][0]).sum::<f32>().sqrt().max(1e-9);
    let r = |i: usize, j: usize| t[i][j] / s;
    let ry = (-r(2, 0)).clamp(-1.0, 1.0).asin();
    let (rx, rz) = (r(2, 1).atan2(r(2, 2)), r(1, 0).atan2(r(0, 0)));
    [t[0][3], t[1][3], t[2][3], rx.to_degrees(), ry.to_degrees(), rz.to_degrees(), s]
}

/// A tuner line "dx dy dz rx ry rz scale" as a matrix in the piece's own
/// frame (move, then turn x, y, z in degrees, then scale).
fn tune_matrix(v: &[f32; 7]) -> [[f32; 4]; 3] {
    let (sx, cx) = v[3].to_radians().sin_cos();
    let (sy, cy) = v[4].to_radians().sin_cos();
    let (sz, cz) = v[5].to_radians().sin_cos();
    let rx = [[1.0, 0.0, 0.0], [0.0, cx, -sx], [0.0, sx, cx]];
    let ry = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
    let rz = [[cz, -sz, 0.0], [sz, cz, 0.0], [0.0, 0.0, 1.0]];
    let r = crate::skate::mul(&rz, &crate::skate::mul(&ry, &rx));
    std::array::from_fn(|i| std::array::from_fn(|j| if j < 3 { r[i][j] * v[6] } else { v[i] }))
}

/// A debug panel setting ("set key value" in the tuner file), None without one.
fn debug_set(key: &str) -> Option<String> {
    std::env::var_os("SA2_GEAR_TUNE")?;
    let _ = gear_tune("");
    DEBUG_SETS.with_borrow(|s| s.get(key).cloned())
}

/// A debug toggle that is switched off in the panel.
fn debug_off(key: &str) -> bool {
    debug_set(key).is_some_and(|v| v == "0")
}

/// Sound ids muted in the panel's list (hex, comma or space separated).
fn debug_muted(id: u32) -> bool {
    debug_set("mute").is_some_and(|l| l.split([',', ' ']).filter_map(|x| u32::from_str_radix(x.trim().trim_start_matches("0x"), 16).ok()).any(|x| x == id))
}

thread_local! {
    static DEBUG_SETS: std::cell::RefCell<std::collections::BTreeMap<String, String>> = Default::default();
    static GEAR_TUNE_AT: Cell<u64> = const { Cell::new(u64::MAX) };
    static GEAR_TUNE: std::cell::RefCell<std::collections::BTreeMap<String, [f32; 7]>> = Default::default();
}

/// Shadow's Air Shoes as the client exports them (docs/UPGRADES.md): his own
/// heel/toe mesh from body model 65 (left heel node 23 with toe 24, drawn in
/// the heel's frame) with the Air Shoes overlay 79 (slot 159) on it, at the
/// hub's boot place on Chao foot `n`. TERIOSMDL 80/81 (slots 161/163) are
/// hand models, which the feet drew before (test copy, run r58).
/// SA2_SHADOW_BOOT=0 (debug): the old slot draw.
fn shadow_boot(c: &mut Cpu, m: &mut Mem, n: usize, tex: u32, cur_ptr: u32, list: u32) {
    ensure_rider_rest(m); // sets RIDER_OBJ (his body model) in self mode
    let heel = node_obj(m, RIDER_OBJ.get(), 23);
    let overlay = model_obj(m, 159);
    if heel == 0 {
        return;
    }
    // The heel drawn alone in its own frame: its offset, rotation and scale
    // cleared and its sibling cut for the draw, put back after.
    let keep: Vec<u32> = (0x08..0x34).step_by(4).map(|o| m.r32(heel + o)).collect();
    m.w32(heel + 0x30, 0);
    for o in (0x08..0x20).step_by(4) { m.w32(heel + o, 0); }
    for o in (0x20..0x2C).step_by(4) { m.w32(heel + o, 1.0f32.to_bits()); }
    let br = crate::chao_retarget::read_chao(&|a| m.r32(a), heel, [vec![], vec![]]);
    let bv: Vec<[f32; 3]> = br.verts.iter().zip(br.worlds(&[])).flat_map(|(vs, (r, t))| vs.iter().map(move |v| std::array::from_fn(|i| t[i] + (0..3).map(|k| r[i][k] * v[k]).sum::<f32>())).collect::<Vec<_>>()).collect();
    for (i, o) in (0x08..0x34).step_by(4).enumerate() { m.w32(heel + o, keep[i]); }
    // The hub's own Air Shoes file when there is one (it fits as in the hub).
    let hub = hub_shoe(c, m);
    // Without the hub's shoe file nothing is drawn (the baked fallback lay
    // on the floor beside the Chao on a site with a stale file list).
    // SA2_BOOT_PARTS set (debug): the baked heel/toe and overlay.
    if hub == 0 && std::env::var_os("SA2_BOOT_PARTS").is_none() {
        return;
    }
    let bv = if hub != 0 { HUB_SHOE_VERTS.with_borrow(|v| v.clone()) } else if baked_boot(c, m) != 0 { BAKED_VERTS.with_borrow(|v| v.clone()) } else { bv };
    jet_log(m, &bv);
    let node = SHOES[n].0;
    let hp = if bv.is_empty() { None } else { hub_boot_place(m, n, node, &br, &bv) };
    if let Some(place) = hp.map(|p| mul34(&p, &gear_tune(if n == 0 { "boot0" } else { "boot1" }).unwrap_or_else(|| tune_matrix(&BOOT_TUNE)))) {
        if sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) % 300 == 0 {
            let (lo, hi) = bv.iter().fold(([f32::MAX; 3], [f32::MIN; 3]), |(l, h), v| (std::array::from_fn(|i| l[i].min(v[i])), std::array::from_fn(|i| h[i].max(v[i]))));
            eprintln!("chao-player boot {n}: Shadow heel {heel:#x} ({} nodes, {} vertices, box {lo:?}..{hi:?}, per node {:?}), overlay {overlay:#x}, place scale {:.3}", br.nodes.len(), bv.len(), br.verts.iter().map(Vec::len).collect::<Vec<_>>(), (0..3).map(|i| place[i][0] * place[i][0]).sum::<f32>().sqrt());
        }
        let put = |m: &mut Mem| for r in 0..3 { for k in 0..4 { m.w32(cur_ptr + 4 * (4 * r + k) as u32, place[r][k].to_bits()); } };
        m.w32(list, tex);
        let lcap = LCAP.replace(false);
        // His heel/toe vertices are weighted to the leg above them: drawn
        // straight from his body they stretched across the screen (runs
        // r61-r65). Drawn from an unweighted copy baked from his rest pose.
        // SA2_BOOT_PARTS (debug): bit 1 the baked heel/toe, bit 2 the overlay.
        // Default the overlay alone: the baked copy still lands off the foot
        // and its cached polygon list holds more of his leg (run r76).
        let parts = std::env::var("SA2_BOOT_PARTS").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(2);
        if hub != 0 {
            if n == 0 && sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) % 600 == 300 {
                tex_log(m, "Shadow's (at the shoe draw)", tex);
                // The polygon lists' first words: the hub shoe's, and Shadow's own heel and toe
                // (his body model's nodes 23 and 24), to compare their material chunks.
                let words = |o: u32| -> String { let mdl = m.r32(o + 4); let pl = if mdl >> 24 == 0x80 { m.r32(mdl + 4) } else { 0 }; if pl >> 24 != 0x80 { return "-".into() } (0..28).map(|k| format!("{:04x}", m.r16(pl + 2 * k))).collect::<Vec<_>>().join(" ") };
                let body = SHADOW_MODELS.with_borrow(|e| e.values().copied().collect::<Vec<_>>()).into_iter().find(|&o| (0x8000_0000..0x8C00_0000).contains(&o) && crate::chao_retarget::read_chao(&|a| m.r32(a), o, [vec![], vec![]]).nodes.len() == 62).unwrap_or(0);
                eprintln!("plist-log: hub shoe {} | Shadow heel 23 {} | toe 24 {} | heel 13 {}", words(hub), words(node_obj(m, body, 23)), words(node_obj(m, body, 24)), words(node_obj(m, body, 13)));
            }
            foot_log(m, n, node, "the hub's Air Shoe file", hub);
            put(m);
            call(c, m, 0x8011_DF44, &[hub]);
        }
        if hub == 0 && parts & 1 != 0 {
            let baked = baked_boot(c, m);
            // SA2_BAKE_DRAW (debug): 0 bake only, 2 the vertices alone (no polygons).
            let mode = std::env::var("SA2_BAKE_DRAW").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(1);
            if baked != 0 && mode == 2 {
                let mdl = m.r32(baked + 4);
                let pl = m.r32(mdl + 4);
                m.w32(mdl + 4, 0);
                put(m);
                call(c, m, 0x8011_DF44, &[baked]);
                m.w32(mdl + 4, pl);
            }
            if baked != 0 && mode == 1 {
                put(m);
                call(c, m, 0x8011_DF44, &[baked]);
            }
        }
        if hub == 0 && parts & 2 != 0 && (0x8000_0000..0x8C00_0000).contains(&overlay) {
            put(m);
            call(c, m, 0x8011_DF44, &[overlay]);
        }
        // The jets as the game draws them on Shadow (fn_8007BC7C, matrices
        // saved by his display's node callback fn_8007CF1C): the right toe
        // jets TERIOSMDL 96/97 (slots 193/195) in his right toe node's frame
        // (body node 14, the node in slot 149), the right heel jets 98/99
        // (slots 197/199) in his right heel node's (13, slot 181), and each
        // frame the other of the two pairs by the game's frame count
        // (GAME +0x7C bit 0: set = 96 with 98, clear = 97 with 99).
        // The hub's shoe file is his right heel node's frame, recentred by
        // its shoeOriginShift (1.0672683, 0.2922889, -0.2537795) and then
        // mirrored in x (its up axis is that node's rest up with x negated):
        // file = MX * (heel frame - shift). Before 2026-11-23 the jets were
        // put at heel frame + (shift x, -y, -z) with no mirror, and the toe
        // jets in the heel's frame instead of the toe's.
        // Glue: shown while the skate is in (the game: while his skating
        // motions 8-10 play).
        if skate_blend(false) > 0.0 && !debug_off("jets") && knux_rig::jets() {
            let odd = m.r32(0x801C_C168 + 0x7C) & 1 != 0;
            // SA2_JET_OLD=1 (debug): where they were drawn before, to compare.
            let old = std::env::var("SA2_JET_OLD").is_ok_and(|v| v == "1");
            let to_file = if old { [[1.0, 0.0, 0.0, 1.067_268_3], JET_HEEL_TO_FILE[1], JET_HEEL_TO_FILE[2]] } else { JET_HEEL_TO_FILE };
            let heel = if hub != 0 { mul34(&place, &to_file) } else { place };
            let toe = jet_toe_in_heel(m).filter(|_| hub != 0 && !old).map_or(heel, |rel| mul34(&heel, &rel));
            if std::env::var("SA2_JET_LOG").is_ok_and(|v| v == "1") {
                // Each jet's origin in the drawn shoe's own frame (constant = fixed to the shoe),
                // and where the game's own jet matrices for Shadow (saved by his display's node
                // callback: toe 540 / 588, heel 636 / 684, right / left) sit in that same frame.
                let o = |x: &[[f32; 4]; 3]| { let r = inv_mul(&place, x); [r[0][3], r[1][3], r[2][3]] };
                let (nt, nh) = if n == 0 { (540, 636) } else { (588, 684) };
                let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
                eprintln!("jet-frame: f{f} foot {n} ours toe {:?} heel {:?} game toe {:?} heel {:?}", o(&toe), o(&heel), o(&mtx(m, 0x801F_E758 + nt)), o(&mtx(m, 0x801F_E758 + nh)));
            }
            for (js, at) in if odd { [(193u32, toe), (197, heel)] } else { [(195, toe), (199, heel)] } {
                let jo = shadow_obj(m, js);
                if (0x8000_0000..0x8C00_0000).contains(&jo) {
                    for r in 0..3 { for k in 0..4 { m.w32(cur_ptr + 4 * (4 * r + k) as u32, at[r][k].to_bits()); } }
                    call(c, m, 0x8011_DF44, &[jo]);
                }
            }
        }
        LCAP.set(lcap);
    }
}

/// Shadow's right heel node frame -> the hub shoe file's space:
/// MX * (p - shoeOriginShift), as one matrix.
const JET_HEEL_TO_FILE: [[f32; 4]; 3] = [[-1.0, 0.0, 0.0, 1.067_268_3], [0.0, 1.0, 0.0, -0.292_288_9], [0.0, 0.0, 1.0, 0.253_779_5]];

/// SA2_FOOT_LOG=1 (debug): a texture list as loaded (NJS_TEXLIST: entries,
/// count; each entry 12 bytes: name, attribute, loaded texture).
fn tex_log(m: &Mem, whose: &str, list: u32) {
    if list == 0 || !std::env::var("SA2_FOOT_LOG").is_ok_and(|v| v == "1") {
        return;
    }
    let (names, n) = (m.r32(list), m.r32(list + 4).min(64));
    let ok = |a: u32| (0x8000_0000..0x8C00_0000).contains(&a);
    let rows: Vec<String> = (0..n).map(|i| {
        let e = names + 12 * i;
        let (name, attr, addr) = (m.r32(e), m.r32(e + 4), m.r32(e + 8));
        // The loaded texture's first words (its own header), to tell two lists' entries apart.
        let head = if ok(addr) { (0..6).map(|k| format!("{:x}", m.r32(addr + 4 * k))).collect::<Vec<_>>().join("/") } else { "-".into() };
        format!("{i}:{}:{attr:#x}:{addr:#x}[{head}]", if ok(name) { crate::cstr(m, name) } else { format!("{name:#x}") })
    }).collect();
    eprintln!("tex-log: stage {} {whose} texture list {list:#x}, {n} entries: {}", m.r16(STAGE_NUM), rows.join(" "));
}

/// SA2_FOOT_LOG=1 (debug), each frame: the model drawn at Chao foot `n`
/// (its node), where it comes from, and its node and vertex counts.
fn foot_log(m: &Mem, n: usize, node: usize, what: &str, obj: u32) {
    if !std::env::var("SA2_FOOT_LOG").is_ok_and(|v| v == "1") {
        return;
    }
    let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
    let (nodes, verts) = if (0x8000_0000..0x8C00_0000).contains(&obj) {
        let r = crate::chao_retarget::read_chao(&|a| m.r32(a), obj, [vec![], vec![]]);
        (r.nodes.len(), r.verts.iter().map(Vec::len).sum::<usize>())
    } else {
        (0, 0)
    };
    eprintln!("foot-log: f{f} stage {} foot {n} (Chao node {node}): {what}, object {obj:#x}, {nodes} nodes, {verts} vertices", m.r16(STAGE_NUM));
}

thread_local! {
    static JET_LOGGED: Cell<bool> = const { Cell::new(false) };
    /// (body model, his right toe node 14 in his right heel node 13's frame, at rest).
    static JET_TOE: Cell<Option<(u32, [[f32; 4]; 3])>> = const { Cell::new(None) };
}

/// Shadow's right toe node (14) in his right heel node's (13) frame, from his
/// body model's rest pose (the shoe on the Chao is rigid, so the rest relation
/// stands in for his live toe bend).
fn jet_toe_in_heel(m: &Mem) -> Option<[[f32; 4]; 3]> {
    // Shadow's own body (TERIOSMDL's 62-node model), also when Sonic rides.
    let body = SHADOW_MODELS.with_borrow(|e| e.values().copied().find(|&o| JET_TOE.get().is_some_and(|(b, _)| b == o)))
        .or_else(|| SHADOW_MODELS.with_borrow(|e| e.values().copied().collect::<Vec<_>>()).into_iter().find(|&o| (0x8000_0000..0x8C00_0000).contains(&o) && crate::chao_retarget::read_chao(&|a| m.r32(a), o, [vec![], vec![]]).nodes.len() == 62))
        .unwrap_or(RIDER_OBJ.get());
    if !(0x8000_0000..0x8C00_0000).contains(&body) {
        return None;
    }
    if let Some((b, rel)) = JET_TOE.get() {
        if b == body {
            return Some(rel);
        }
    }
    let w = crate::chao_retarget::read_chao(&|a| m.r32(a), body, [vec![], vec![]]).worlds(&[]);
    if w.len() != 62 {
        return None;
    }
    let m34 = |(r, t): ([[f32; 3]; 3], [f32; 3])| -> [[f32; 4]; 3] { std::array::from_fn(|i| [r[i][0], r[i][1], r[i][2], t[i]]) };
    let rel = inv_mul(&m34(w[13]), &m34(w[14]));
    JET_TOE.set(Some((body, rel)));
    Some(rel)
}

/// SA2_JET_LOG=1 (debug), once: what the jets' native placement needs.
/// The game draws each jet at a marker node of Shadow's body, saved by his
/// display's node callback (fn_8007CF1C, drawn by fn_8007BC7C): right toe jets
/// (slots 193/195) at the node in slot 149, left toe (185/187) at 151, right
/// heel (197/199) at 181, left heel (189/191) at 183.
fn jet_log(m: &Mem, hub_verts: &[[f32; 3]]) {
    if JET_LOGGED.get() || !std::env::var("SA2_JET_LOG").is_ok_and(|v| v == "1") {
        return;
    }
    let body = RIDER_OBJ.get();
    if !(0x8000_0000..0x8C00_0000).contains(&body) {
        return;
    }
    JET_LOGGED.set(true);
    let bounds = |p: &[[f32; 3]]| p.iter().fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), v| (std::array::from_fn(|i| lo[i].min(v[i])), std::array::from_fn(|i| hi[i].max(v[i]))));
    let rest = crate::chao_retarget::read_chao(&|a| m.r32(a), body, [vec![], vec![]]);
    let worlds = rest.worlds(&[]);
    eprintln!("jet-log: body {body:#x} nodes {} hub shoe verts {} box {:?}", rest.nodes.len(), hub_verts.len(), bounds(hub_verts));
    for i in [13usize, 14, 23, 24] {
        if let (Some(v), Some((r, t))) = (rest.verts.get(i), worlds.get(i)) {
            let w: Vec<[f32; 3]> = v.iter().map(|v| std::array::from_fn(|a| t[a] + (0..3).map(|k| r[a][k] * v[k]).sum::<f32>())).collect();
            eprintln!("jet-log: body node {i}: {} verts, local box {:?}, as placed by its rest node {:?}, node rest rot {r:?} pos {t:?}", v.len(), bounds(v), bounds(&w));
        }
    }
    for slot in [149u32, 151, 181, 183] {
        let o = shadow_obj(m, slot);
        let idx = (0..rest.nodes.len()).find(|&i| node_obj(m, body, i) == o);
        eprintln!("jet-log: marker slot {slot} obj {o:#x} body node {idx:?} rest {:?}", idx.and_then(|i| worlds.get(i)));
    }
    for slot in [185u32, 187, 189, 191, 193, 195, 197, 199] {
        let o = shadow_obj(m, slot);
        if !(0x8000_0000..0x8C00_0000).contains(&o) {
            eprintln!("jet-log: jet slot {slot} not loaded");
            continue;
        }
        let j = crate::chao_retarget::read_chao(&|a| m.r32(a), o, [vec![], vec![]]);
        let jw = j.worlds(&[]);
        let v: Vec<[f32; 3]> = j.verts.iter().zip(&jw).flat_map(|(vs, (r, t))| vs.iter().map(move |v| std::array::from_fn(|a| t[a] + (0..3).map(|k| r[a][k] * v[k]).sum::<f32>())).collect::<Vec<_>>()).collect();
        eprintln!("jet-log: jet slot {slot} obj {o:#x} nodes {} verts {} box {:?} node0 {:?}", j.nodes.len(), v.len(), bounds(&v), j.nodes.first());
        if slot >= 193 && !v.is_empty() {
            // In the shoe file's space: where it was drawn before, where the game's rule puts it.
            let toe = if slot < 197 { jet_toe_in_heel(m) } else { None };
            let centre = |f: &dyn Fn([f32; 3]) -> [f32; 3]| { let s = v.iter().fold([0.0f32; 3], |a, &p| { let q = f(p); [a[0] + q[0], a[1] + q[1], a[2] + q[2]] }); s.map(|x| x / v.len() as f32) };
            let old = centre(&|p| [p[0] + 1.067_268_3, p[1] - 0.292_288_9, p[2] + 0.253_779_5]);
            let new = centre(&|p| apply(&JET_HEEL_TO_FILE, toe.map_or(p, |t| apply(&t, p))));
            eprintln!("jet-log: jet slot {slot} centre in the shoe file's space: before {old:?} now {new:?} moved {:?} (shoe box half size [1.989, 2.156, 1.285])", [new[0] - old[0], new[1] - old[1], new[2] - old[2]]);
        }
    }
}

/// The hub's Air Shoes (its shadow-air-shoes-right.glb: TERIOSMDL 65 heel/toe
/// and overlay 78, TERIOSTEX), converted by scripts/convert-hub-shoe-to-ninja.py
/// into a Ninja chunk vertex and polygon list, read from the site's
/// /mmo-assets/shadow-air-shoes-right.njsh (SA2_HUB_SHOE overrides; local
/// game-derived data, never in git) into game heap 1 once per load. 0 without it.
fn hub_shoe(c: &mut Cpu, m: &mut Mem) -> u32 {
    let path = std::env::var("SA2_HUB_SHOE").unwrap_or_else(|_| "/mmo-assets/shadow-air-shoes-right.njsh".into());
    shoe_file(c, m, &HUB_SHOE, &HUB_SHOE_VERTS, &path)
}

type ShoeCell = std::thread::LocalKey<Cell<Option<u32>>>;
type ShoeVerts = std::thread::LocalKey<std::cell::RefCell<Vec<[f32; 3]>>>;

/// A rigid shoe file (.njsh, scripts/convert-hub-shoe-to-ninja.py) read into
/// game heap 1 once per stage load: its object (0 without the file) in
/// `cell`, its vertices for the fit in `verts`. The Air Shoes and Sonic's
/// Light Shoes both come through here.
fn shoe_file(c: &mut Cpu, m: &mut Mem, cell: &'static ShoeCell, verts_out: &'static ShoeVerts, path: &str) -> u32 {
    if let Some(o) = cell.get() {
        return o;
    }
    let Ok(b) = std::fs::read(path) else {
        eprintln!("chao-player boot: no shoe file at {path}");
        cell.set(Some(0));
        return 0;
    };
    let u = |o: usize| b.get(o..o + 4).map_or(0, |x| u32::from_be_bytes([x[0], x[1], x[2], x[3]]));
    let (n, vlen, plen) = (u(4), u(8) as usize, u(12) as usize);
    if &b[..4.min(b.len())] != b"NJSH" || b.len() < 16 + vlen + plen {
        eprintln!("chao-player boot: {path} is not a hub shoe file");
        cell.set(Some(0));
        return 0;
    }
    // A shoe file made before the converter wrote material chunks has none
    // and takes its diffuse colour from whatever was drawn last: such a file
    // gets one white material chunk in front (NJD_CM_D, source alpha /
    // inverse source alpha). (This was not what made the shoes pink in
    // Rouge's stages: that was the draw inside her display, see chao_display_inner.)
    let old_file = b.get(16 + vlen + 1) != Some(&0x11);
    let lead: &[u8] = if old_file { &[0x25, 0x11, 0x00, 0x02, 0xFF, 0xFF, 0xFF, 0xFF] } else { &[] };
    let plist: Vec<u8> = [lead, &b[16 + vlen..16 + vlen + plen]].concat();
    let plen = plist.len();
    let (model, obj) = (64u32, 0x40u32);
    let total = (vlen + plen) as u32 + model + obj + 8;
    let mem = call(c, m, OS_ALLOC_FROM_HEAP, &[GEAR_HEAP, total]);
    if mem == 0 {
        cell.set(Some(0));
        return 0;
    }
    zero(m, mem, total);
    let vl = mem;
    let pl = (mem + vlen as u32 + 3) & !3;
    m.load(vl, &b[16..16 + vlen]);
    m.load(pl, &plist);
    let mdl = (pl + plen as u32 + 3) & !3;
    let o = mdl + model;
    m.w32(mdl, vl);
    m.w32(mdl + 4, pl);
    m.w32(o, 0x17); // UNIT_POS | UNIT_ANG | UNIT_SCL | BREAK (no child)
    m.w32(o + 4, mdl);
    for j in 0..3 { m.w32(o + 32 + 4 * j, 1.0f32.to_bits()); }
    // Its vertices for the fit: VN chunks (8-byte header, 24 bytes a vertex).
    let f = |o: usize| f32::from_bits(u(o));
    let (mut verts, mut a) = (vec![], 16usize);
    while a < 16 + vlen && u(a) & 0xFF != 0xFF {
        let (size, cnt) = ((u(a) >> 16) as usize, (u(a + 4) >> 16) as usize);
        for k in 0..cnt { let p = a + 8 + 24 * k; verts.push([f(p), f(p + 4), f(p + 8)]); }
        a += 4 + 4 * size;
    }
    eprintln!("chao-player boot: shoe file {path} ({n} vertices, {} read) at {o:#x}", verts.len());
    verts_out.set(verts);
    cell.set(Some(o));
    o
}

thread_local! {
    static HUB_SHOE: Cell<Option<u32>> = const { Cell::new(None) };
    static HUB_SHOE_VERTS: std::cell::RefCell<Vec<[f32; 3]>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Shadow's left heel (body node 23) and toe (24) as a new two-node object
/// whose vertices are his rest-pose skinned positions, in the heel's frame:
/// the whole body's vertex chunks run on the CPU (Ninja chunk weights:
/// NF vertices carry index | weight << 16, chunk flags & 3 = 0 start,
/// else add), then one plain VN chunk holds every cache entry, and the heel
/// and toe keep their own polygon lists. Made once per load in game heap 1.
/// unverified: the NF weight scale (255 = 1.0), checked only by the drawn result.
fn baked_boot(c: &mut Cpu, m: &mut Mem) -> u32 {
    let body = RIDER_OBJ.get();
    if let Some((b, o)) = BAKED_BOOT.get() {
        if b == body {
            return o;
        }
    }
    let r32 = |a: u32| m.r32(a);
    let f = |a: u32| f32::from_bits(m.r32(a));
    // Nodes depth first with their objects and world (rotation, position).
    let rest = crate::chao_retarget::read_chao(&r32, body, [vec![], vec![]]);
    let w = rest.worlds(&[]);
    let objs: Vec<u32> = (0..w.len()).map(|k| node_obj(m, body, k)).collect();
    if w.len() < 25 || objs[23] == 0 || objs[24] == 0 {
        return 0;
    }
    let mut cache: std::collections::BTreeMap<u32, ([f32; 3], [f32; 3])> = Default::default();
    let mut shoe: std::collections::BTreeSet<u32> = Default::default();
    for (k, &o) in objs.iter().enumerate() {
        let att = r32(o + 4);
        if att == 0 { continue; }
        let (r, t) = w[k];
        let mut vp = r32(att);
        while vp != 0 {
            let hdr = r32(vp);
            let typ = hdr & 0xFF;
            if typ == 0xFF { break; }
            let (status, size) = ((hdr >> 8) & 3, hdr >> 16);
            let w2 = r32(vp + 4);
            let (off, cnt) = (w2 & 0xFFFF, w2 >> 16);
            let stride = if cnt != 0 { (size - 1) / cnt } else { 0 };
            let nf = typ == 37 || typ == 44;
            let vn = (41..=47).contains(&typ);
            for i in 0..cnt {
                let a = vp + 8 + i * stride * 4;
                let pos = [f(a), f(a + 4), f(a + 8)];
                let nor = if vn { [f(a + 12), f(a + 16), f(a + 20)] } else { [0.0, 1.0, 0.0] };
                let (idx, wt) = if nf { let x = r32(a + (stride - 1) * 4); (off + (x & 0xFFFF), (x >> 16) as f32 / 255.0) } else { (off + i, 1.0) };
                let p: [f32; 3] = std::array::from_fn(|j| (t[j] + (0..3).map(|q| r[j][q] * pos[q]).sum::<f32>()) * wt);
                let n: [f32; 3] = std::array::from_fn(|j| (0..3).map(|q| r[j][q] * nor[q]).sum::<f32>() * wt);
                if k == 23 || k == 24 {
                    shoe.insert(idx);
                }
                let e = cache.entry(idx).or_insert(([0.0; 3], [0.0; 3]));
                if !nf || status == 0 {
                    *e = (p, n);
                } else {
                    for j in 0..3 { e.0[j] += p[j]; e.1[j] += n[j]; }
                }
            }
            vp += 4 + size * 4;
        }
    }
    // The toe draws polygon list cache 1 (its plist: chunk 0x0105); the
    // polygons are in the node that caches list 1 (chunk 0x0104), after it.
    let r16 = |a: u32| if a & 2 == 0 { r32(a) >> 16 } else { r32(a - 2) & 0xFFFF };
    let toe_pl = r32(r32(objs[24] + 4) + 4);
    let want = if toe_pl != 0 && r16(toe_pl) & 0xFF == 5 { r16(toe_pl) >> 8 } else { u32::MAX };
    let mut polys = 0u32;
    'nodes: for &o in &objs {
        let att = r32(o + 4);
        let mut a = if att != 0 { r32(att + 4) } else { 0 };
        for _ in 0..4096 {
            if a == 0 { break; }
            let h = r16(a);
            let t = h & 0xFF;
            if t == 0xFF { break; }
            if t == 4 && h >> 8 == want {
                polys = a + 2;
                break 'nodes;
            }
            a += match t { 0..=7 => 2, 8..=15 => 4, _ => 4 + 2 * r16(a + 2) };
        }
    }
    if polys == 0 {
        eprintln!("chao-player boot: no cached polygon list {want} for his toe");
        BAKED_BOOT.set(Some((body, 0)));
        return 0;
    }
    // Only the strips of his shoe (as the hub's export keeps only the mesh
    // weighted to the heel): the cached list also holds more of his leg.
    let (plist, kept, dropped) = shoe_strips(&|a| r16(a) as u16, polys, &shoe);
    let Some(&max) = cache.keys().next_back() else { return 0 };
    // Into the heel's frame.
    let (hr, ht) = w[23];
    let inv = |v: [f32; 3], d: [f32; 3]| -> [f32; 3] { std::array::from_fn(|j| (0..3).map(|q| hr[q][j] * (v[q] - d[q])).sum::<f32>()) };
    let cnt = max + 1;
    // Chunks of at most 64 vertices, as his own (flags 0x80 like his heel's
    // VN chunk): one chunk of 803 crashed the draw (run r71).
    const PER: u32 = 64;
    let chunks = cnt.div_ceil(PER);
    let words = 2 * chunks + cnt * 6 + 1;
    // Model and object blocks with zeroed room past their Ninja fields
    // (unverified whether the GC draw keeps more per model).
    let (vl_size, model_size, obj_size) = (words * 4, 64, 0x40);
    let pl_size = (2 * plist.len() as u32 + 3) & !3;
    let total = vl_size + 2 * (model_size + obj_size) + pl_size;
    let mem = call(c, m, OS_ALLOC_FROM_HEAP, &[GEAR_HEAP, total]);
    if mem == 0 {
        eprintln!("chao-player boot: no room to bake his heel/toe ({total} bytes)");
        BAKED_BOOT.set(Some((body, 0)));
        return 0;
    }
    zero(m, mem, total);
    let vl = mem;
    let mut a = vl;
    for ch in 0..chunks {
        let (off, n) = (ch * PER, PER.min(cnt - ch * PER));
        m.w32(a, ((1 + n * 6) << 16) | (0x80 << 8) | 41);
        m.w32(a + 4, (n << 16) | off);
        a += 8;
        for i in off..off + n {
            let (p, nv) = cache.get(&i).copied().unwrap_or(([0.0; 3], [0.0, 1.0, 0.0]));
            let (p, nv) = (inv(p, ht), inv(nv, [0.0; 3]));
            for j in 0..3 {
                m.w32(a + 4 * j as u32, p[j].to_bits());
                m.w32(a + 12 + 4 * j as u32, nv[j].to_bits());
            }
            a += 24;
        }
    }
    m.w32(a, 0xFF);
    let (m_heel, m_toe) = (mem + vl_size, mem + vl_size + model_size);
    let (o_heel, o_toe) = (mem + vl_size + 2 * model_size, mem + vl_size + 2 * model_size + obj_size);
    let pl_at = o_toe + obj_size;
    for (i, &w) in plist.iter().enumerate() {
        m.w16(pl_at + 2 * i as u32, w);
    }
    for (mdl, vlist, pl) in [(m_heel, vl, pl_at), (m_toe, 0, 0)] {
        m.w32(mdl, vlist);
        m.w32(mdl + 4, pl);
        for j in 0..3 { m.w32(mdl + 8 + 4 * j, 0); }
        m.w32(mdl + 20, 0); // radius 0, as his own heel model
    }
    for (obj, mdl, child) in [(o_heel, m_heel, 0), (o_toe, m_toe, 0)] {
        // UNIT_POS | UNIT_ANG | UNIT_SCL, and BREAK (0x10): the walk
        // (0x8011DFC0) enters the child pointer unchecked unless BREAK is set.
        m.w32(obj, 0x17);
        m.w32(obj + 4, mdl);
        for j in 0..3 { m.w32(obj + 8 + 4 * j, 0); m.w32(obj + 20 + 4 * j, 0); m.w32(obj + 32 + 4 * j, 1.0f32.to_bits()); }
        m.w32(obj + 44, child);
        m.w32(obj + 48, 0);
    }
    eprintln!("chao-player boot: baked his heel/toe ({cnt} cache entries, {} set, shoe {}) at {o_heel:#x}: {kept} shoe strips kept, {dropped} dropped", cache.len(), shoe.len());
    BAKED_BOOT.set(Some((body, o_heel)));
    BAKED_VERTS.set(shoe.iter().filter_map(|i| cache.get(i)).map(|&(p, _)| inv(p, ht)).collect());
    o_heel
}

thread_local! {
    /// (his body model, the baked heel/toe object) once made this load.
    static BAKED_BOOT: Cell<Option<(u32, u32)>> = const { Cell::new(None) };
    /// The baked shoe's own vertices (heel and toe), in the heel's frame.
    static BAKED_VERTS: std::cell::RefCell<Vec<[f32; 3]>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// A Ninja chunk polygon list (big-endian shorts from `start`, to its end
/// chunk) with only the strips whose vertices are all in `keep`; material
/// and other state chunks kept, polygon cache/draw chunks (types 4/5)
/// dropped. Returns the new list and the strip counts kept and dropped.
/// Strip types 64-75 by their shorts per vertex (index plus UV/normal/
/// colour); an unknown type is kept whole.
/// unverified: per-vertex sizes of types 67-69 and 73-75 (not in his shoe so far).
fn shoe_strips(r16: &dyn Fn(u32) -> u16, start: u32, keep: &std::collections::BTreeSet<u32>) -> (Vec<u16>, usize, usize) {
    let (mut out, mut kept, mut dropped) = (vec![], 0, 0);
    let mut a = start;
    for _ in 0..8192 {
        let h = r16(a);
        let t = h & 0xFF;
        match t {
            0xFF => break,
            0 | 4 | 5 => a += 2,
            1..=7 => { out.push(h); a += 2; }
            8..=15 => { out.extend([h, r16(a + 2)]); a += 4; }
            64..=75 => {
                let size = r16(a + 2) as u32;
                let end = a + 4 + 2 * size;
                let base = match t { 64 | 73 => 1, 65 | 66 | 70 => 3, 67 => 4, 71 | 72 | 74 | 75 => 5, _ => 6 };
                let w = r16(a + 4);
                let (n, uf) = (w & 0x3FFF, (w >> 14) as u32);
                let mut p = a + 6;
                let mut strips: Vec<u16> = vec![];
                let mut nk = 0u16;
                for _ in 0..n {
                    let cnt = (r16(p) as i16).unsigned_abs() as u32;
                    let s0 = p;
                    p += 2;
                    let mut all = true;
                    for i in 0..cnt {
                        all &= keep.contains(&(r16(p) as u32));
                        p += 2 * base + if i >= 2 { 2 * uf } else { 0 };
                    }
                    if all {
                        strips.extend((s0..p).step_by(2).map(r16));
                        nk += 1;
                    } else {
                        dropped += 1;
                    }
                }
                if nk > 0 {
                    out.extend([h, 1 + strips.len() as u16, ((uf as u16) << 14) | nk]);
                    out.extend(strips);
                    kept += nk as usize;
                }
                a = end;
            }
            _ => {
                let size = r16(a + 2) as u32;
                out.extend((a..a + 4 + 2 * size).step_by(2).map(r16));
                a += 4 + 2 * size;
            }
        }
    }
    out.push(0x00FF);
    (out, kept, dropped)
}

/// The object of node `index` (depth first, as read_chao counts) in a model
/// tree (NJS_OBJECT: child +0x2C, sibling +0x30), 0 when there is none.
fn node_obj(m: &Mem, root: u32, index: usize) -> u32 {
    let ok = |o: u32| (0x8000_0000..0x8C00_0000).contains(&o);
    if !ok(root) {
        return 0;
    }
    let (mut stack, mut n) = (vec![root], 0usize);
    while let Some(o) = stack.pop() {
        if n == index {
            return o;
        }
        n += 1;
        let (child, sib) = (m.r32(o + 0x2C), m.r32(o + 0x30));
        if ok(sib) { stack.push(sib); }
        if ok(child) { stack.push(child); }
        if n > 512 { break; }
    }
    0
}

/// Knuckles's fist for a Hammer Gloves plate: right plate 329 with fist 163,
/// left 331 with 161 (KNUCKMDL 80/81, the pair the slot log showed as hands).
/// unverified: which of 161/163 is his right fist (by analogy with Shadow's
/// boots, where 161 is the left).
fn knux_fist(plate: u32) -> u32 {
    if plate == 329 { 163 } else { 161 }
}

/// A KNUCKMDL model by slot: its own load first, then the shared lookup.
fn knux_obj(m: &Mem, k: u32) -> u32 {
    KNUX_MODELS.with_borrow(|e| e.get(&k).copied()).unwrap_or_else(|| model_obj(m, k))
}

fn model_obj(m: &Mem, k: u32) -> u32 {
    EXTRA_MODELS.with_borrow(|e| e.get(&k).copied()).unwrap_or_else(|| m.r32(MODELS + 4 * k))
}

/// Load a model file with the game's loader, then take the slots it filled
/// out of the game's table: Chao World frees that table on an area change,
/// and freeing these crashed there (OSFreeToHeap at 0xffffffe4, lvl9 gdn).
/// shortcut: they are never freed (loaded once per run).
fn load_models(c: &mut Cpu, m: &mut Mem, name: u32) -> u32 {
    let before: Vec<u32> = (0..0x428).map(|k| m.r32(MODELS + 4 * k)).collect();
    let r = call(c, m, LOAD_MDL, &[name]);
    LAST_LOAD.with_borrow_mut(|l| l.clear());
    for k in 0..0x428u32 {
        let now = m.r32(MODELS + 4 * k);
        if now != 0 && now != before[k as usize] {
            LAST_LOAD.with_borrow_mut(|l| { l.insert(k, now); });
        }
        // The loader also replaces slots that were filled: KNUCKMDL put
        // Knuckles's own models over the character's (Shadow's) boot slots
        // 161/163, so the feet drew Knuckles's hands. The game's table keeps
        // what it had; this file's model stays reachable through LAST_LOAD.
        if now != 0 && before[k as usize] != 0 && now != before[k as usize] {
            m.w32(MODELS + 4 * k, before[k as usize]);
        }
        if now != 0 && before[k as usize] == 0 {
            // First file wins a slot: Shadow's boots are slots 161/163, and
            // KNUCKMDL, loaded next into the same (emptied) slots, overwrote
            // them with Knuckles's own models (his hand drawn as a "boot").
            EXTRA_MODELS.with_borrow_mut(|e| { e.entry(k).or_insert(now); });
            m.w32(MODELS + 4 * k, 0);
        }
    }
    r
}

/// Shadow's boots: TERIOSMDL models in slots 161/163 (a 2-node, 174-vertex
/// left/right pair; drawn 3x on the Chao feet they show as his air shoes,
/// lvl9 sbv3). The left is his own left boot (no mirror).
const SHOES: [(usize, usize, u32); 2] = [(13, 14, 161), (6, 24, 163)];
/// The client's fit (docs/MIRRORED-GEAR-FIT.md): boot length 0.90 of the Chao foot.
const SHOE_LENGTH_RATIO: f32 = 0.90;


/// Shadow's boots on the Chao's feet (nodes 13/6, which follow the rider's
/// legs 7->14 and 17->24, the retarget's LIMBS; Sonic's 6-15 leg is his
/// right). Drawn from the rider's own display, under its view, like the
/// ball (a draw from inside ChaoMain's walk showed nothing). Each boot takes
/// the rider's foot rotation, sits at the Chao foot node from the last posed
/// Chao, and is scaled to the Chao foot.
/// shortcut: lengths are longest vertex extents, not a heel-to-toe axis; the
/// Chao foot position is last frame's pose.
fn air_shoes(c: &mut Cpu, m: &mut Mem) {
    let (Some(tex), Step::Live(chao)) = (SHADOW_TEX.get(), STEP.get()) else { return };
    if std::env::var("SA2_AIR_SHOES").is_ok_and(|v| v == "0") || debug_off("shoes") {
        if sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) % 300 == 0 { eprintln!("chao-player shoes: own motion {}", OWN_MOTION.get()); }
        return;
    }
    let cwk = m.r32(chao + 0x34);
    let f = |a: u32| f32::from_bits(m.r32(a));
    let cpos = [f(cwk + 0x14), f(cwk + 0x18), f(cwk + 0x1C)];
    let (sy, cy) = ((m.r32(cwk + 0x0C) as i32 as f32) * std::f32::consts::TAU / 65536.0).sin_cos();
    let ry = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
    let posed = POSED.with_borrow(|p| p.clone());
    let skel = SKEL.with_borrow(|s| s.clone());
    // The hub's shoe (shadow_boot) in every stage, placed from the Chao's own
    // feet. Before 2026-11-23 only Chao World took it; a level drew the model
    // table's slots 161/163 on the feet, which hold hand models (TERIOSMDL
    // 80/81: 2 nodes, 174 vertices, the same pair the guard below lets
    // through as "Shadow's shoe"): white gloved hands on the Chao's feet on
    // City Escape's board, and no shoes.
    // SA2_SHOE_MODELS / SA2_SHADOW_BOOT=0 (debug): the old slot draw.
    let models: Vec<u32> = std::env::var("SA2_SHOE_MODELS").ok().map(|v| v.split(',').filter_map(|x| x.trim().parse().ok()).collect()).unwrap_or_default();
    let slot_draw = !models.is_empty() || std::env::var("SA2_SHADOW_BOOT").is_ok_and(|v| v == "0");
    if slot_draw && !chao_world(m) && (posed.len() < 14 || skel.len() != 62) {
        if sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) % 300 == 0 {
            eprintln!("chao-player shoes: skipped (posed {}, skel {})", posed.len(), skel.len());
        }
        return;
    }
    let cur_ptr = m.r32(NJ_CURRENT_MATRIX);
    let keep_m = mtx(m, cur_ptr);
    let base = mtx(m, m.r32(NJ_BASE_MATRIX));
    let list = m.r32(c.r[13].wrapping_sub(0x7228)) + 0x20;
    let keep_list = m.r32(list);
    let regs = (c.r, c.f);
    let ext = |vs: Vec<[f32; 3]>| (0..3).map(|k| { let (lo, hi) = vs.iter().fold((f32::MAX, f32::MIN), |(l, h), v| (l.min(v[k]), h.max(v[k]))); hi - lo }).fold(0.0f32, f32::max);
    for (n, &(node, foot, model)) in SHOES.iter().enumerate() {
        if !slot_draw {
            shadow_boot(c, m, n, tex, cur_ptr, list);
            continue;
        }
        foot_log(m, n, node, "model table slot (debug)", shadow_obj(m, models.get(n).copied().unwrap_or(model)));
        // Chao World: the right boot (78 = slot 163) on both feet, as the hub does.
        let model = models.get(n).copied().unwrap_or(if chao_world(m) { 163 } else { model });
        let obj = shadow_obj(m, model);
        if !(0x8000_0000..0x8C00_0000).contains(&obj) {
            continue;
        }
        // The boot's vertices in its own model space (its nodes carry the rest
        // offset from Shadow's foot): centred on the Chao foot below.
        let br = crate::chao_retarget::read_chao(&|a| m.r32(a), obj, [vec![], vec![]]);
        // Only Shadow's shoe (TERIOSMDL 80/81: 2 nodes, 174 vertices): another
        // file's model in these slots (a hand) is never drawn on the feet.
        if models.is_empty() && (br.nodes.len() != 2 || br.verts.iter().map(Vec::len).sum::<usize>() != 174) {
            if sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) % 300 == 0 {
                eprintln!("chao-player shoes: slot {model} holds {obj:#x} ({} nodes), not Shadow's shoe; not drawn", br.nodes.len());
            }
            continue;
        }
        let bv: Vec<[f32; 3]> = br.verts.iter().zip(br.worlds(&[])).flat_map(|(vs, (r, t))| vs.iter().map(move |v| std::array::from_fn(|i| t[i] + (0..3).map(|k| r[i][k] * v[k]).sum::<f32>())).collect::<Vec<_>>()).collect();
        let centre = bv.iter().fold([0.0f32; 3], |a, v| [a[0] + v[0], a[1] + v[1], a[2] + v[2]]).map(|x| x / bv.len().max(1) as f32);
        let boot = ext(bv.clone());
        let len = CHAO_REST.with_borrow(|r| r.as_ref().map(|r| ext(r.verts[node].clone()))).unwrap_or(0.0);
        if sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) % 300 == 0 {
            eprintln!("chao-player shoes: boot {n} obj {obj:#x} boot {boot} foot {len} feet {:?}", FEET.with_borrow(|f| f[n].map(|v| [v[0][3], v[1][3], v[2][3]])));
        }
        if boot <= 0.0 || len <= 0.0 {
            continue;
        }
        // Chao World, native rig: the web hub's fit (mmo-play src/chao-attachments.js
        // 19-49): the right boot on node 6, mirrored in x for node 13, fitted in
        // the Chao's rest model space, then carried by the animated foot node.
        if chao_world(m) {
            let hp = hub_boot_place(m, n, node, &br, &bv);
            if sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) % 120 == 0 {
                eprintln!("chao-player boot {n}: place {:?} foot {:?}", hp.map(|p| [p[0][3], p[1][3], p[2][3], p[0][0], p[1][1], p[2][2]]), FEET.with_borrow(|f| f[n].map(|v| [v[0][3], v[1][3], v[2][3]])));
            }
            let hp = hp.map(|p| gear_tune(if n == 0 { "boot0" } else { "boot1" }).map_or(p, |t| mul34(&p, &t)));
            if let Some(place) = hp {
                for r in 0..3 { for k in 0..4 { m.w32(cur_ptr + 4 * (4 * r + k) as u32, place[r][k].to_bits()); } }
                m.w32(list, tex);
                let lcap = LCAP.replace(false);
                // SA2_SHADOW_FOOT=1 (debug): also the foot node of his body
                // model (nodes 14/24) with its children. Off: his body is a
                // weighted model, and a subtree drawn alone stretched into a
                // screen-sized mesh once he moved (test copy, run r52).
                let foot_obj = node_obj(m, RIDER_OBJ.get(), foot);
                if foot_obj != 0 && std::env::var("SA2_SHADOW_FOOT").is_ok_and(|v| v == "1") {
                    let sib = m.r32(foot_obj + 0x30);
                    m.w32(foot_obj + 0x30, 0);
                    // The node's own rest offset is in its object: draw it at the boot's place.
                    let keep: Vec<u32> = (0x08..0x2C).step_by(4).map(|o| m.r32(foot_obj + o)).collect();
                    for o in (0x08..0x20).step_by(4) { m.w32(foot_obj + o, 0); }
                    for o in (0x20..0x2C).step_by(4) { m.w32(foot_obj + o, 1.0f32.to_bits()); }
                    call(c, m, 0x8011_DF44, &[foot_obj]);
                    for (i, o) in (0x08..0x2C).step_by(4).enumerate() { m.w32(foot_obj + o, keep[i]); }
                    m.w32(foot_obj + 0x30, sib);
                    for r in 0..3 { for k in 0..4 { m.w32(cur_ptr + 4 * (4 * r + k) as u32, place[r][k].to_bits()); } }
                }
                call(c, m, 0x8011_DF44, &[obj]);
                // Heel/toe jets (Shadow's right jets 96-99 = slots 181-184, the
                // hub's: modelled in the same foot frame as the boot) while moving.
                // shortcut: all four drawn; the hub picks narrow/wide by speed.
                if MOVING.get() && !debug_off("jets") && knux_rig::jets() {
                    for js in 181..=184u32 {
                        let jo = model_obj(m, js);
                        if (0x8000_0000..0x8C00_0000).contains(&jo) {
                            for r in 0..3 { for k in 0..4 { m.w32(cur_ptr + 4 * (4 * r + k) as u32, place[r][k].to_bits()); } }
                            call(c, m, 0x8011_DF44, &[jo]);
                        }
                    }
                }
                LCAP.set(lcap);
            }
            continue;
        }
        // SA2_SHOE_SCALE=k (debug): k times the fitted size.
        // shortcut: on the native rig the boot is 2.6x the rest fit (measured
        // headless: at 1x it sits inside the adult foot); upgrade by measuring
        // the drawn adult foot instead of the rest model's.
        let native = if chao_world(m) { 2.6 } else { 1.0 };
        let s = native * SHOE_LENGTH_RATIO * len / boot * std::env::var("SA2_SHOE_SCALE").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(1.0);
        let _ = (node, &posed, ry, cpos);
        let Some(fv) = FEET.with_borrow(|f| f[n]) else {
            if sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) % 300 == 0 { eprintln!("chao-player shoes: no foot {n}"); }
            continue;
        };
        // Native rig: the boot centred on the Chao foot mesh's centre (its
        // vertices' mean under the foot node), not on the node origin.
        let wpos = if chao_world(m) {
            let fc = CHAO_REST.with_borrow(|r| r.as_ref().map(|r| { let v = &r.verts[node]; v.iter().fold([0.0f32; 3], |a, p| [a[0] + p[0], a[1] + p[1], a[2] + p[2]]).map(|x| x / v.len().max(1) as f32) })).unwrap_or([0.0; 3]);
            std::array::from_fn(|i| fv[i][3] + (0..3).map(|k| fv[i][k] * fc[k]).sum::<f32>())
        } else {
            [fv[0][3], fv[1][3], fv[2][3]]
        };
        // The rider's foot rotation under this view; the Chao foot's view position.
        // Native rig (Chao World): the Chao foot node's own rotation, unit-scaled.
        let fw = if chao_world(m) {
            let n = |j: usize| (0..3).map(|i| fv[i][j] * fv[i][j]).sum::<f32>().sqrt().max(1e-6);
            let l = [n(0), n(1), n(2)];
            let _ = l; // keeps the foot node's draw scale (the Chao's size)
            let u: [[f32; 3]; 3] = std::array::from_fn(|i| std::array::from_fn(|j| fv[i][j]));
            // The boot is modelled on Shadow's foot (forward -x); the Chao's is +z:
            // a half turn about y, measured headless (SA2_SHOE_TURN=k: k quarter turns, debug).
            let k = std::env::var("SA2_SHOE_TURN").ok().and_then(|v| v.parse::<i32>().ok()).unwrap_or(2).rem_euclid(4);
            let (sn, cs) = [(0.0, 1.0), (1.0, 0.0), (0.0, -1.0), (-1.0, 0.0)][k as usize];
            let f = [[cs, 0.0, sn], [0.0, 1.0, 0.0], [-sn, 0.0, cs]];
            std::array::from_fn(|i| std::array::from_fn(|j| if j < 3 { (0..3).map(|q| u[i][q] * f[q][j]).sum() } else { fv[i][3] }))
        } else {
            mul34(&base, &skel[foot])
        };
        let place: [[f32; 4]; 3] = std::array::from_fn(|r| std::array::from_fn(|k| if k < 3 { fw[r][k] * s } else { wpos[r] - (0..3).map(|q| fw[r][q] * s * centre[q]).sum::<f32>() }));
        for r in 0..3 { for k in 0..4 { m.w32(cur_ptr + 4 * (4 * r + k) as u32, place[r][k].to_bits()); } }
        m.w32(list, tex);
        // Not through level_capture: this draw is meant to show.
        let lcap = LCAP.replace(false);
        call(c, m, 0x8011_DF44, &[obj]);
        LCAP.set(lcap);
    }
    (c.r, c.f) = regs;
    m.w32(list, keep_list);
    for r in 0..3 { for k in 0..4 { m.w32(cur_ptr + 4 * (4 * r + k) as u32, keep_m[r][k].to_bits()); } }
}

/// The hub's boot mount (chao-attachments.js 19-27): the boot model's own up
/// and toe directions, turned to the Chao foot's +y / +z.
const BOOT_UP: [f32; 3] = [0.802_577_7, -0.596_420_6, 0.012_308_6];
const BOOT_FWD: [f32; 3] = [-0.596_517_8, -0.802_573, 0.006_565_9];

/// View-space matrix for boot `n` (0 = node 13, mirrored; 1 = node 6): the
/// hub's rest fit B (scale 0.90 of the foot's z length, centred in x/z, sole
/// on the foot's lowest point), carried by the foot: FEET * inv(rest foot) * B.
fn hub_boot_place(_m: &Mem, n: usize, node: usize, br: &crate::chao_retarget::ChaoRest, bv: &[[f32; 3]]) -> Option<[[f32; 4]; 3]> {
    let _ = br;
    boot_place(_m, n, node, bv, None)
}

/// The same fit for a shoe with its own mount (`own`: the shoe model's frame
/// to the Chao's rest model space), or the Air Shoes' mount without one.
fn boot_place(_m: &Mem, n: usize, node: usize, bv: &[[f32; 3]], own: Option<[[f32; 3]; 3]>) -> Option<[[f32; 4]; 3]> {
    let fv = FEET.with_borrow(|f| f[n])?;
    let (worlds, foot_v) = CHAO_REST.with_borrow(|r| r.as_ref().map(|r| (r.worlds(&[]), r.verts[6].clone())))?;
    let (rr, rt) = worlds[6];
    let foot: Vec<[f32; 3]> = foot_v.iter().map(|v| std::array::from_fn(|i| rt[i] + (0..3).map(|k| rr[i][k] * v[k]).sum::<f32>())).collect();
    let x = [BOOT_UP[1] * BOOT_FWD[2] - BOOT_UP[2] * BOOT_FWD[1], BOOT_UP[2] * BOOT_FWD[0] - BOOT_UP[0] * BOOT_FWD[2], BOOT_UP[0] * BOOT_FWD[1] - BOOT_UP[1] * BOOT_FWD[0]];
    let mount = [x, BOOT_UP, BOOT_FWD];
    // Native frame instead of the hub's Babylon numbers (they are in a mirrored
    // space): the boot is modelled in Shadow's foot node frame, so turn it by
    // Shadow's rest foot rotation (node 24, his model space) and his facing
    // (-x) to the Chao's (+z).
    ensure_rider_rest(_m);
    // The hub's own shoe file is in the hub's coordinates: its mount as it is
    // (src/chao-attachments.js shadowShoeMountRotation / mirroredShadowShoeRest).
    let hub = own.is_none() && HUB_SHOE.get().is_some_and(|o| o != 0);
    let mount = if let Some(own) = own { own } else if hub { mount } else { RIDER_REST.with_borrow(|r| r.as_ref().and_then(|(_, v)| v.get(24).copied()))
        .map(|rf| crate::skate::mul(&[[0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]], &crate::skate::unit(&rf)))
        .unwrap_or(mount) };
    // SA2_BOOT_FIX="rx,ry,rz" (debug, degrees): a turn after the hub mount, to
    // find the handedness difference between the hub (Babylon) and the game.
    let mount = match std::env::var("SA2_BOOT_FIX").ok().map(|v| v.split(',').filter_map(|x| x.trim().parse::<f32>().ok()).collect::<Vec<_>>()) {
        Some(a) if a.len() == 3 => {
            let (sx, cx) = a[0].to_radians().sin_cos(); let (sy, cy) = a[1].to_radians().sin_cos(); let (sz, cz) = a[2].to_radians().sin_cos();
            let rx = [[1.0, 0.0, 0.0], [0.0, cx, -sx], [0.0, sx, cx]];
            let ry = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
            let rz = [[cz, -sz, 0.0], [sz, cz, 0.0], [0.0, 0.0, 1.0]];
            crate::skate::mul(&rz, &crate::skate::mul(&ry, &crate::skate::mul(&rx, &mount)))
        }
        _ => mount,
    };
    let shoe: Vec<[f32; 3]> = bv.iter().map(|v| std::array::from_fn(|i| (0..3).map(|k| mount[i][k] * v[k]).sum::<f32>())).collect();
    let bounds = |p: &[[f32; 3]]| p.iter().fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), v| (std::array::from_fn(|i| lo[i].min(v[i])), std::array::from_fn(|i| hi[i].max(v[i]))));
    let ((mut flo, mut fhi), (slo, shi)) = (bounds(&foot), bounds(&shoe));
    // The drawn foot mesh's size against the rest model's (same node frame).
    let d = DRAWN_FOOT.get();
    if (0x8000_0000..0x8C00_0000).contains(&d) {
        let dv = crate::chao_retarget::read_chao(&|a| _m.r32(a), d, [vec![], vec![]]).verts.first().cloned().unwrap_or_default();
        let ext = |p: &[[f32; 3]]| (0..3).map(|k| { let (l, h) = p.iter().fold((f32::MAX, f32::MIN), |(l, h), v| (l.min(v[k]), h.max(v[k]))); h - l }).fold(0.0f32, f32::max);
        let (de, re) = (ext(&dv), ext(&foot_v));
        if de > 0.0 && re > 0.0 && (de / re - 1.0).abs() > 0.01 {
            let r = de / re;
            let c: [f32; 3] = std::array::from_fn(|i| (flo[i] + fhi[i]) / 2.0);
            flo = std::array::from_fn(|i| c[i] + (flo[i] - c[i]) * r);
            fhi = std::array::from_fn(|i| c[i] + (fhi[i] - c[i]) * r);
            if sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) % 600 == 0 { eprintln!("chao-player boot fit: drawn foot {de} vs rest {re}"); }
        }
    }
    if foot.is_empty() || shoe.is_empty() || shi[2] - slo[2] <= 0.0 { return None; }
    // SA2_SHOE_SCALE=k (debug): k times the hub fit, to see it in a check.
    // shortcut: 1.4x by eye on the self-driven Chao with the character's own
    // rest foot as the mount (lobby close-ups: 1.0 sits inside the foot, 1.6
    // swallows it); rider mode used 1.7 with the hub mount. The hub's shoe
    // file: 1.6 by eye (test copy run r83: at 1.0 it sat inside the foot,
    // at 1.6 it covers the foot as in the hub). The computed fit
    // alone sits inside the foot: find the measuring difference.
    let sc = (fhi[2] - flo[2]) * SHOE_LENGTH_RATIO / (shi[2] - slo[2]) * std::env::var("SA2_SHOE_SCALE").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(if hub || own.is_some() { 1.6 } else { 1.4 });
    if sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) % 600 == 0 {
        eprintln!("chao-player boot fit: foot {:?}..{:?} shoe {:?}..{:?} scale {sc}", flo, fhi, slo, shi);
    }
    let fc: [f32; 3] = std::array::from_fn(|i| (flo[i] + fhi[i]) / 2.0);
    let scn: [f32; 3] = std::array::from_fn(|i| (slo[i] + shi[i]) / 2.0);
    let pos = [fc[0] - scn[0] * sc, flo[1] - slo[1] * sc, fc[2] - scn[2] * sc];
    // B (rest model space, 3x4), mirrored in x for the other foot.
    let mir = if node == 6 { 1.0 } else { -1.0 };
    let b: [[f32; 4]; 3] = std::array::from_fn(|i| { let f = if i == 0 { mir } else { 1.0 }; std::array::from_fn(|j| f * if j < 3 { mount[i][j] * sc } else { pos[i] }) });
    // inv(rest of this foot node) * B, then the animated foot (view space).
    let (nr, nt) = worlds[node];
    let ninv = crate::skate::inv(&nr);
    let rel: [[f32; 4]; 3] = std::array::from_fn(|i| std::array::from_fn(|j| {
        let v: f32 = (0..3).map(|k| ninv[i][k] * b[k][j]).sum();
        if j < 3 { v } else { v - (0..3).map(|k| ninv[i][k] * nt[k]).sum::<f32>() }
    }));
    Some(mul34(&fv, &rel))
}

/// Knuckles's Hammer Gloves item (KNUCKMDL 164 right / 165 left = slots
/// 329/331, KNUCKTEX) on the Chao's arm nodes 3/10, with the client's fit
/// (src/scene.js gearMountRotation and attachOwnerGear): the glove's source
/// up/forward (Knuckles142 rest wrist/hand matrices) turned to the Chao's +y/+z,
/// scaled to 1.12 of the Chao hand's largest extent (hand points past 0.55
/// of the reach from arm node 3/10 towards its distal marker 4/11), centred
/// on the hand, then fixed rigidly under the arm node.
/// The side: the arm node on the Chao's right (-x, facing +z) takes the right glove.
const GLOVE_REFS: [(u32, [f32; 3], [f32; 3]); 2] = [
    (329, [0.150_553_9, 0.933_591_6, 0.325_177_2], [-0.984_045_9, 0.173_063_3, -0.041_265_2]),
    (331, [0.171_001_5, 0.896_849_7, -0.407_944_7], [-0.985_151_6, 0.162_074_6, -0.056_639_6]),
];
/// shortcut: Knuckles's fist at 1.0 of the Chao hand's size, so his cuff keeps
/// his proportions round the Chao's thin wrist (1.6 swallowed the arm); the
/// gear tuner (?geartune=1) gives the measured value to commit.
const GLOVE_SCALE: f32 = 1.0;
/// KNUCKMDL body model 142 in the model table (slot 2n+1).
const KNUX_BODY_SLOT: u32 = 285;
const GLOVE_REACH: f32 = 0.55;
/// The hub's glove: the Hammer Gloves cuff alone, 1.12 of the Chao palm's
/// largest extent, centred on the palm (src/scene.js attachOwnerGear).
const HUB_GLOVE_SCALE: f32 = 1.12;
/// SA2_GLOVE_NATIVE=1 (debug): the older fit on Knuckles's hand frame with
/// his fist under the cuff. Default: the hub's fit, as the hub draws it.
fn glove_native() -> bool {
    std::env::var("SA2_GLOVE_NATIVE").is_ok_and(|v| v == "1")
}
/// Where along the arm (shoulder 0 -> hand tip 1) the glove's origin (his
/// wrist) sits, and how far the fists turn out from the hips (degrees):
/// a presentation fit, not game data. SA2_GLOVE_UP / SA2_GLOVE_OUT override.
/// Degrees the cuff tips about its own centre (positive: top in towards the body).
fn glove_up() -> f32 {
    std::env::var("SA2_GLOVE_UP").ok().and_then(|v| v.parse().ok()).unwrap_or(0.35)
}
fn glove_roll() -> f32 {
    std::env::var("SA2_GLOVE_ROLL").ok().and_then(|v| v.parse().ok()).unwrap_or(0.0)
}
fn glove_out() -> f32 {
    std::env::var("SA2_GLOVE_OUT").ok().and_then(|v| v.parse().ok()).unwrap_or(15.0)
}

fn glove_fit(m: &Mem) -> Option<[(u32, [[f32; 4]; 3]); 2]> {
    let rest = CHAO_REST.with_borrow(|r| r.as_ref().map(|r| (r.worlds(&[]), r.verts.clone())))?;
    let (w, verts) = rest;
    if w.len() < 12 {
        return None;
    }
    let world = |n: usize| -> [[f32; 4]; 3] { let (r, t) = w[n]; std::array::from_fn(|i| std::array::from_fn(|j| if j < 3 { r[i][j] } else { t[i] })) };
    let bounds = |pts: &[[f32; 3]]| { let lo = (0..3).map(|k| pts.iter().map(|p| p[k]).fold(f32::MAX, f32::min)).collect::<Vec<_>>(); let hi = (0..3).map(|k| pts.iter().map(|p| p[k]).fold(f32::MIN, f32::max)).collect::<Vec<_>>(); ([0, 1, 2].map(|k| (lo[k] + hi[k]) / 2.0), (0..3).map(|k| hi[k] - lo[k]).fold(0.0f32, f32::max)) };
    // Right arm = the one at -x.
    let arms = if w[3].1[0] < w[10].1[0] { [(3usize, 4usize), (10, 11)] } else { [(10, 11), (3, 4)] };
    let mut out = [(0u32, [[0.0f32; 4]; 3]); 2];
    for (k, &(arm, tip)) in arms.iter().enumerate() {
        let (model, up, fwd) = GLOVE_REFS[k];
        // The hub's vectors are in its glTF space, whose root mirrors the
        // source x (chao-attachments.js note): mirror them back for the game.
        // The hub mirrors them only because its glTF root mirrors X; the
        // game's plate is in Knuckles's own coordinates: as read.
        let (up, fwd) = if !glove_native() || std::env::var("SA2_GLOVE_NOMIRROR").is_ok() { (up, fwd) } else { ([-up[0], up[1], up[2]], [-fwd[0], fwd[1], fwd[2]]) };
        let obj = knux_obj(m, model);
        if !(0x8000_0000..0x8C00_0000).contains(&obj) {
            return None;
        }
        let aw = world(arm);
        let pts: Vec<[f32; 3]> = verts[arm].iter().map(|&v| apply(&aw, v)).collect();
        let (sh, hand) = (w[arm].1, w[tip].1);
        let ax = { let d = [hand[0] - sh[0], hand[1] - sh[1], hand[2] - sh[2]]; let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-6); d.map(|x| x / l) };
        let dot = |p: &[f32; 3]| (0..3).map(|i| (p[i] - sh[i]) * ax[i]).sum::<f32>();
        let reach = pts.iter().map(dot).fold(f32::MIN, f32::max);
        let hp: Vec<[f32; 3]> = pts.iter().filter(|p| dot(p) >= reach * GLOVE_REACH).copied().collect();
        if hp.is_empty() {
            return None;
        }
        // R maps the glove's source up/forward to +y/+z.
        let n = |v: [f32; 3]| { let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt(); v.map(|x| x / l) };
        let (u, f) = (n(up), n(fwd));
        let x = n([u[1] * f[2] - u[2] * f[1], u[2] * f[0] - u[0] * f[2], u[0] * f[1] - u[1] * f[0]]);
        let r = [x, u, f];
        // Native mount (default): the glove model is built in Knuckles's hand
        // node frame (his display draws it there), so turn it by his rest hand
        // rotation (KNUCKMDL body model 142 = slot 285: hand nodes 61 right,
        // 42 left, the frames the hub vectors were read from) and his facing
        // (-x) to the Chao's (+z), as the boots do with Shadow's foot.
        // SA2_GLOVE_HUB=1 (debug): the hub's vectors instead.
        let native = glove_native().then(|| {
            let body = knux_obj(m, KNUX_BODY_SLOT);
            if !(0x8000_0000..0x8C00_0000).contains(&body) { return None; }
            let kr = crate::chao_retarget::read_chao(&|a| m.r32(a), body, [vec![], vec![]]).worlds(&[]);
            let node = if k == 0 { 61 } else { 42 };
            let (hr, _) = *kr.get(node)?;
            let r0 = kr.first().map(|x| x.0)?;
            Some(crate::skate::mul(&[[0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]], &crate::skate::unit(&crate::skate::mul(&crate::skate::inv(&r0), &hr))))
        }).flatten();
        let r = native.unwrap_or(r);
        // Point the fist out along the arm (shoulder -> hand marker), not the
        // Chao's +z: the glove's forward goes to the arm direction, its up stays
        // world up (flattened against the arm).
        let r = if native.is_some() || !std::env::var("SA2_GLOVE_ALONG_ARM").is_ok() { r } else {
            // The hub's fist points down and out: forward = the arm direction
            // tipped down by SA2_GLOVE_TIP degrees (default 0: 45 hid them), up = toward +y.
            let tip = std::env::var("SA2_GLOVE_TIP").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0).to_radians();
            let h = n([ax[0], 0.0, ax[2]]);
            let a = n([h[0] * tip.cos(), -tip.sin(), h[2] * tip.cos()]);
            let y = n([-h[0] * tip.sin(), tip.cos(), -h[2] * tip.sin()]);
            let x2 = n([y[1] * a[2] - y[2] * a[1], y[2] * a[0] - y[0] * a[2], y[0] * a[1] - y[1] * a[0]]);
            // columns x2, y, a: maps +x/+y/+z to them.
            let b: [[f32; 3]; 3] = std::array::from_fn(|i| [x2[i], y[i], a[i]]);
            crate::skate::mul(&b, &r)
        };
        // SA2_GLOVE_FIX="rx,ry,rz" (debug, degrees): turn the glove in its own
        // model frame before the mount, to find the way it faces on the Chao.
        let r = match std::env::var("SA2_GLOVE_FIX").ok().map(|v| v.split(',').filter_map(|x| x.trim().parse::<f32>().ok()).collect::<Vec<_>>()) {
            Some(a) if a.len() == 3 => {
                let (sx, cx) = a[0].to_radians().sin_cos(); let (sy, cy) = a[1].to_radians().sin_cos(); let (sz, cz) = a[2].to_radians().sin_cos();
                let rx = [[1.0, 0.0, 0.0], [0.0, cx, -sx], [0.0, sx, cx]];
                let ry = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
                let rz = [[cz, -sz, 0.0], [sz, cz, 0.0], [0.0, 0.0, 1.0]];
                crate::skate::mul(&r, &crate::skate::mul(&rz, &crate::skate::mul(&ry, &rx)))
            }
            _ => r,
        };
        let gr = crate::chao_retarget::read_chao(&|a| m.r32(a), obj, [vec![], vec![]]);
        let gv: Vec<[f32; 3]> = gr.verts.iter().zip(gr.worlds(&[])).flat_map(|(vs, (rr, t))| vs.iter().map(move |v| std::array::from_fn(|i| t[i] + (0..3).map(|q| rr[i][q] * v[q]).sum::<f32>())).collect::<Vec<_>>()).collect();
        let src: Vec<[f32; 3]> = gv.iter().map(|p| std::array::from_fn(|i| (0..3).map(|q| r[i][q] * p[q]).sum())).collect();
        let ((hc, hsize), (sc, ssize)) = (bounds(&hp), bounds(&src));
        if ssize <= 0.0 {
            return None;
        }
        // Native mount: the Hammer Gloves item (1 node, 42 vertices) is only
        // the metal plate; Knuckles's own fist (KNUCKMDL 80/81 = slots 161/163,
        // drawn with it, see knux_fist) gives the size: the fist, not the
        // plate, is fitted to the Chao's hand (the plate alone, fitted, made a
        // box down to the floor; test build, build 13).
        let ssize = if native.is_some() {
            let fist = knux_obj(m, knux_fist(model));
            if (0x8000_0000..0x8C00_0000).contains(&fist) {
                let fr = crate::chao_retarget::read_chao(&|a| m.r32(a), fist, [vec![], vec![]]);
                let fv: Vec<[f32; 3]> = fr.verts.iter().zip(fr.worlds(&[])).flat_map(|(vs, (rr, t))| vs.iter().map(move |v| std::array::from_fn(|i| t[i] + (0..3).map(|q| rr[i][q] * v[q]).sum::<f32>())).collect::<Vec<_>>()).collect();
                let fs: Vec<[f32; 3]> = fv.iter().map(|p| std::array::from_fn(|i| (0..3).map(|q| r[i][q] * p[q]).sum())).collect();
                let (_, fsize) = bounds(&fs);
                if fsize > 0.0 { fsize } else { ssize }
            } else {
                ssize
            }
        } else {
            ssize
        };
        // SA2_GLOVE_SCALE=k (debug): k times the fitted size.
        let s = hsize * if native.is_some() { GLOVE_SCALE } else { HUB_GLOVE_SCALE } / ssize * std::env::var("SA2_GLOVE_SCALE").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(1.0);
        // SA2_GLOVE_PIVOT=wrist (debug): the glove's end nearest the shoulder
        // (its cuff: the vertices in the first 15% along the arm) sits on the
        // wrist, where the hand starts along the arm, instead of the glove's
        // centre on the hand's centre.
        // Native mount: the glove's origin (Knuckles's hand node) on the
        // Chao's wrist (where the hand starts along the arm), so the arm goes
        // through the cuff (mob 2026-11-22: bigger, higher on the wrist).
        let t: [f32; 3] = if native.is_some() && !std::env::var("SA2_GLOVE_PIVOT").is_ok() {
            std::array::from_fn(|i| sh[i] + ax[i] * reach * glove_up())
        } else if std::env::var("SA2_GLOVE_PIVOT").is_ok_and(|v| v == "wrist") {
            let along = |p: &[f32; 3]| (0..3).map(|i| p[i] * ax[i]).sum::<f32>();
            let (lo, hi) = src.iter().map(along).fold((f32::MAX, f32::MIN), |(l, h), v| (l.min(v), h.max(v)));
            let cuff: Vec<&[f32; 3]> = src.iter().filter(|p| along(p) <= lo + 0.15 * (hi - lo)).collect();
            let cc: [f32; 3] = std::array::from_fn(|i| cuff.iter().map(|p| p[i]).sum::<f32>() / cuff.len().max(1) as f32);
            let wrist: [f32; 3] = std::array::from_fn(|i| sh[i] + ax[i] * reach * GLOVE_REACH);
            std::array::from_fn(|i| wrist[i] - cc[i] * s)
        } else {
            std::array::from_fn(|i| hc[i] - sc[i] * s)
        };
        // Rolled about the arm's own axis (shoulder -> hand) through the
        // cuff's centre: the side frame turns off the outside of the arm.
        // SA2_GLOVE_ROLL (degrees, debug); mirrored for the left arm.
        let roll = glove_roll() * if sh[0] < 0.0 { 1.0 } else { -1.0 };
        let (r, t) = if roll == 0.0 { (r, t) } else {
            let (sn, cs) = roll.to_radians().sin_cos();
            let k = ax;
            let kx = [[0.0, -k[2], k[1]], [k[2], 0.0, -k[0]], [-k[1], k[0], 0.0]];
            let rot: [[f32; 3]; 3] = std::array::from_fn(|i| std::array::from_fn(|j| f32::from(u8::from(i == j)) * cs + sn * kx[i][j] + (1.0 - cs) * k[i] * k[j]));
            // Keep the cuff's centre (t + s * sc) where it was.
            let c: [f32; 3] = std::array::from_fn(|i| t[i] + s * sc[i]);
            let rc: [f32; 3] = std::array::from_fn(|i| (0..3).map(|q| rot[i][q] * sc[q]).sum::<f32>());
            (crate::skate::mul(&rot, &r), std::array::from_fn(|i| c[i] - s * rc[i]))
        };
        let desired: [[f32; 4]; 3] = std::array::from_fn(|i| std::array::from_fn(|j| if j < 3 { r[i][j] * s } else { t[i] }));
        // Tipped about the Chao's forward axis (+z) through the cuff's
        // centre (its own middle): positive tips the top in towards the
        // body. The right arm (-x) turns towards -x, the left towards +x.
        let out_deg = glove_out() * if sh[0] < 0.0 { -1.0 } else { 1.0 };
        let piv: [f32; 3] = std::array::from_fn(|i| t[i] + s * sc[i]);
        let sh = piv;
        let (so, co) = out_deg.to_radians().sin_cos();
        let rz = [[co, -so, 0.0], [so, co, 0.0], [0.0, 0.0, 1.0]];
        let desired: [[f32; 4]; 3] = std::array::from_fn(|i| std::array::from_fn(|j| {
            let lin = (0..3).map(|q| rz[i][q] * desired[q][j]).sum::<f32>();
            if j < 3 { lin } else { sh[i] + (0..3).map(|q| rz[i][q] * (desired[q][3] - sh[q])).sum::<f32>() }
        }));
        out[k] = (model, inv_mul(&aw, &desired));
        let _ = arm;
    }
    // Index by arm node: [node 3, node 10].
    if arms[0].0 == 10 { out.swap(0, 1); }
    Some(out)
}

fn hammer_gloves(c: &mut Cpu, m: &mut Mem) {
    let Some(tex) = KNUX_TEX.get() else { return };
    let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
    if std::env::var("SA2_HAMMER_GLOVES").is_ok_and(|v| v == "0") || debug_off("fists") || GLOVE_FRAME.replace(f) == f {
        return;
    }
    if GLOVE_FIT.with_borrow(|g| g.is_none()) {
        let fit = glove_fit(m);
        if fit.is_some() {
            eprintln!("chao-player: Hammer Gloves fitted {fit:?}");
        }
        GLOVE_FIT.set(fit);
        FIST_D.set(None);
    }
    let Some(fit) = GLOVE_FIT.with_borrow(|g| *g) else { return };
    let cur_ptr = m.r32(NJ_CURRENT_MATRIX);
    let keep_m = mtx(m, cur_ptr);
    let list = m.r32(c.r[13].wrapping_sub(0x7228)) + 0x20;
    let keep_list = m.r32(list);
    let regs = (c.r, c.f);
    let mut drawn = [None; 2];
    for (k, (model, local)) in fit.into_iter().enumerate() {
        let Some(node) = HANDS.with_borrow(|h| h[k]) else { continue };
        let place = mul34(&node, &local);
        // Fist 1: the tuner's line when the panel is open, else as set.
        // Fist 2: fist 1's reflection (also in the panel while "Link fists"
        // is on: `set link 1`); unlinked, the panel's own fist 2 line.
        let t1 = gear_tune("fist0").unwrap_or_else(|| tune_matrix(&FIST_SET[0]));
        // Unlinked unless the panel links them (mob's saved setting: link 0).
        let linked = debug_set("link").as_deref() == Some("1");
        let set = if k == 0 {
            t1
        } else {
            let own = if linked { fist2_mirror(m, &fit, &t1) } else { gear_tune("fist1") };
            own.unwrap_or_else(|| tune_matrix(&FIST_SET[1]))
        };
        let place = mul34(&place, &set);
        drawn[k] = Some(place);
        if f % 300 == 0 {
            let ft = FEET.with_borrow(|x| x[k].map(|v| [v[0][3], v[1][3], v[2][3]]));
            eprintln!("chao-player gloves: {k} arm node at {:?}, fist at {:?}, foot {:?}, local t {:?}", [node[0][3], node[1][3], node[2][3]], [place[0][3], place[1][3], place[2][3]], ft, [local[0][3], local[1][3], local[2][3]]);
        }
        if std::env::var("SA2_FOOT_LOG").is_ok_and(|v| v == "1") {
            let ft = FEET.with_borrow(|x| x.map(|v| v.map(|v| [v[0][3], v[1][3], v[2][3]])));
            eprintln!("foot-log: f{f} hand {k} (Chao arm node {}): Hammer Gloves plate, KNUCKMDL slot {model}, at {:?} (view); arm node at {:?}; feet at {ft:?}", [3, 10][k], [place[0][3], place[1][3], place[2][3]], [node[0][3], node[1][3], node[2][3]]);
        }
        for r in 0..3 { for q in 0..4 { m.w32(cur_ptr + 4 * (4 * r + q) as u32, place[r][q].to_bits()); } }
        m.w32(list, tex);
        let lcap = LCAP.replace(false);
        call(c, m, 0x8011_DF44, &[knux_obj(m, model)]);
        // Knuckles's own fist under the plate (same frame: both are built on
        // his hand node).
        let fist = knux_obj(m, knux_fist(model));
        if glove_native() && (0x8000_0000..0x8C00_0000).contains(&fist) {
            for r in 0..3 { for q in 0..4 { m.w32(cur_ptr + 4 * (4 * r + q) as u32, place[r][q].to_bits()); } }
            call(c, m, 0x8011_DF44, &[fist]);
        }
        LCAP.set(lcap);
    }
    fist_drawn_log(&drawn);
    (c.r, c.f) = regs;
    m.w32(list, keep_list);
    for r in 0..3 { for q in 0..4 { m.w32(cur_ptr + 4 * (4 * r + q) as u32, keep_m[r][q].to_bits()); } }
}

/// A character's yaw (BAMS; it faces (cos, sin) in x, z) as the Chao yaw
/// that faces the same way (the Chao faces (sin, cos)).
pub(crate) fn chao_yaw(player_yaw: u32) -> u32 {
    0x4000u32.wrapping_sub(player_yaw) & 0xFFFF
}

/// Debug (SA2_TEST_CAM): a view matrix (Ninja: x right, y up, looking down
/// -z) at `eye` looking at `at`.
pub(crate) fn look_at(eye: [f32; 3], at: [f32; 3]) -> [[f32; 4]; 3] {
    let n = |v: [f32; 3]| { let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6); [v[0] / l, v[1] / l, v[2] / l] };
    let cross = |a: [f32; 3], b: [f32; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let f = n([at[0] - eye[0], at[1] - eye[1], at[2] - eye[2]]);
    let r = n(cross(f, [0.0, 1.0, 0.0]));
    let u = cross(r, f);
    let rows = [r, u, [-f[0], -f[1], -f[2]]];
    std::array::from_fn(|i| std::array::from_fn(|j| if j < 3 { rows[i][j] } else { -(rows[i][0] * eye[0] + rows[i][1] * eye[1] + rows[i][2] * eye[2]) }))
}

const NJ_SET_CAMERA: u32 = 0x8011_242C;

/// SA2_TEST_CAM=front|side|rear[,distance[,height]] (debug, test only, no game data):
/// after the game sets its camera (njSetCamera 0x8011242C), the view matrix
/// is replaced by a look-at on the player's Chao from a fixed place by the
/// character's heading: front = ahead of it looking back, side = to its
/// right, rear = behind. The game's own camera work is untouched, so the
/// stick still steers by the game camera.
/// unverified: that every draw reads the view from _nj_base_matrix after
/// njSetCamera (some may cull by the game camera's own place).
fn test_cam(c: &mut Cpu, m: &mut Mem) {
    original(c, m, NJ_SET_CAMERA);
    let Ok(v) = std::env::var("SA2_TEST_CAM") else { return };
    let Step::Live(chao) = STEP.get() else { return };
    let cw = m.r32(chao + 0x34);
    // SA2_TAKEOFF's second Chao, while it is out: the camera follows that one.
    let other = Some(TAKEOFF.get()).filter(|&t| t != 0 && t != u32::MAX).map(|t| m.r32(t + 0x34)).filter(|&w| w != 0);
    if let Some(w) = other {
        let f = |a: u32| f32::from_bits(m.r32(a));
        let mut it = v.split(',').skip(1);
        let dist: f32 = it.next().and_then(|d| d.trim().parse().ok()).unwrap_or(9.0);
        let height: f32 = it.next().and_then(|d| d.trim().parse().ok()).unwrap_or(3.0);
        let yaw = (m.r32(w + 0x0C) & 0xFFFF) as f32 * std::f32::consts::TAU / 65536.0;
        // To its right (it faces (sin, cos)); "world" keeps the camera still, looking at where it started.
        let at = [f(w + 0x14), f(w + 0x18) + height, f(w + 0x1C)];
        let view = look_at([at[0] + yaw.cos() * dist, at[1] + 0.5, at[2] - yaw.sin() * dist], at);
        let (base, cur) = (m.r32(NJ_BASE_MATRIX), m.r32(NJ_CURRENT_MATRIX));
        for a in [base, cur] {
            for r in 0..3 { for k in 0..4 { m.w32(a + 4 * (4 * r + k) as u32, view[r][k].to_bits()); } }
        }
        return;
    }
    // In a level: by the hidden character's own yaw (his task work).
    let rider = if self_drive(m) { 0 } else { RIDER.get() };
    if cw == 0 || (!self_drive(m) && rider == 0) {
        return;
    }
    // side[,distance[,height above the Chao's spot looked at]] (9 and 3 when not given).
    let mut it = v.split(',');
    let side = it.next().unwrap_or("rear");
    let dist: f32 = it.next().and_then(|d| d.trim().parse().ok()).unwrap_or(9.0);
    let height: f32 = it.next().and_then(|d| d.trim().parse().ok()).unwrap_or(3.0);
    let f = |a: u32| f32::from_bits(m.r32(a));
    let yaw = (m.r32(if rider != 0 { rider } else { cw } + 0x0C) & 0xFFFF) as f32 * std::f32::consts::TAU / 65536.0;
    let fwd = if rider != 0 || PMOVE_CHAO.get() == chao { [yaw.cos(), 0.0, yaw.sin()] } else { [yaw.sin(), 0.0, yaw.cos()] };
    let right = [-fwd[2], 0.0, fwd[0]];
    let d = match side { "front" => fwd, "side" => right, _ => [-fwd[0], 0.0, -fwd[2]] };
    // In a level the Chao's own task work falls between its pins to him (its own
    // gravity runs on): his spot is where it is drawn.
    let spot = if rider != 0 { rider } else { cw };
    let at = [f(spot + 0x14), f(spot + 0x18) + height, f(spot + 0x1C)];
    let view = look_at([at[0] + d[0] * dist, at[1] + 0.5, at[2] + d[2] * dist], at);
    let (base, cur) = (m.r32(NJ_BASE_MATRIX), m.r32(NJ_CURRENT_MATRIX));
    for a in [base, cur] {
        for r in 0..3 { for k in 0..4 { m.w32(a + 4 * (4 * r + k) as u32, view[r][k].to_bits()); } }
    }
}

/// Place the Chao on the rider right before it draws (after its own exec).
fn chao_display(c: &mut Cpu, m: &mut Mem) {
    // The player's own Chao in self mode is drawn upright: the character's
    // exec leaves his pitch and roll (slope and run lean) in the shared task
    // work after the Chao's exec, and the draw laid the Chao flat with them.
    // SA2_CHAO_TILT=1 (debug): drawn with his tilt.
    let task = c.r[3];
    let cw = m.r32(task + 0x34);
    let own = matches!(STEP.get(), Step::Live(ch) if ch == task) && cw != 0 && self_drive(m) && !std::env::var("SA2_CHAO_TILT").is_ok_and(|v| v == "1");
    if own && PMOVE_CHAO.get() == task {
        let his = m.r32(0x801E_7768);
        if (0x8000_0000..0x8C00_0000).contains(&his) { take_push(m, his, cw); }
    }
    let keep = (m.r32(cw.wrapping_add(0x08)), m.r32(cw.wrapping_add(0x10)));
    // The character's exec leaves its own yaw in the shared task work, and
    // the two do not mean the same: the character goes along (cos, sin) of
    // it (test copy run clip-B2: yaw 0xA01F while moving along (-1, -1)),
    // the Chao is drawn facing (sin, cos) (AL_ForwardSpd). Copied as it was,
    // the Chao faced his way only on one diagonal and was mirrored
    // elsewhere (up to 180 degrees off). Drawn with its own meaning of the
    // same heading; the task work keeps his value for his exec.
    let keep_yaw = m.r32(cw.wrapping_add(0x0C));
    let turn = own && PMOVE_CHAO.get() == task && player_move_on() && !std::env::var("SA2_CHAO_YAW_RAW").is_ok_and(|v| v == "1");
    if own {
        m.w32(cw + 0x08, 0);
        m.w32(cw + 0x10, 0);
    }
    // Swimming: drawn where the native swim holds a Chao, just under the
    // surface, while the hidden character walks the pond's floor.
    let keep_y = m.r32(cw.wrapping_add(0x18));
    let swim = SWIM.get().filter(|_| own);
    if let Some(level) = swim {
        m.w32(cw + 0x18, (level - SWIM_HEIGHT).to_bits());
    }
    if turn {
        m.w32(cw + 0x0C, chao_yaw(keep_yaw));
    }
    chao_display_inner(c, m);
    if swim.is_some() {
        m.w32(cw + 0x18, keep_y);
    }
    if own {
        m.w32(cw + 0x08, keep.0);
        m.w32(cw + 0x10, keep.1);
    }
    if turn {
        m.w32(cw + 0x0C, keep_yaw);
    }
}

fn chao_display_inner(c: &mut Cpu, m: &mut Mem) {
    if remote_offscreen(m, c.r[3]) {
        return;
    }
    // SA2_HIDE_CHAO=1 (test only): the player's own Chao and its gear are not
    // drawn, so the hidden character (SA2_SHOW_SONIC) can be seen alone.
    if matches!(STEP.get(), Step::Live(ch) if ch == c.r[3]) && std::env::var("SA2_HIDE_CHAO").is_ok_and(|v| v == "1") {
        return;
    }
    // The walker's pilot (own_pilot) draws the player's Chao at the seat:
    // its task's own display, at the hidden character, stands down.
    if PILOT_SEAT.get().is_none() && pilot_recent() && matches!(STEP.get(), Step::Live(ch) if ch == c.r[3]) {
        return;
    }
    if idle_hidden(m, c.r[3]) {
        return;
    }
    crate::name_tags::begin(tag_label(m, c.r[3]));
    if let Step::Live(chao) = STEP.get() {
        if c.r[3] == chao {
            knux_rig::draw_state_log(c, m);
            knux_rig::blend_reset(c, m);
            pin(m, chao, RIDER.get());
            follow_ball(m, chao);
        }
    }
    // SA2_EV_SKIP=exec,display (debug): skip this step in a cutscene.
    if RIDER.get() == 0 && std::env::var("SA2_EV_SKIP").is_ok_and(|v| v.contains("display")) {
        return;
    }
    let a = rt::sec(m, 60, 1) + CHAO_DISPLAY;
    let live = matches!(STEP.get(), Step::Live(ch) if ch == c.r[3]);
    // A cutscene has no player task (player 1's task work pointer 0x801E7768 is 0). In a
    // stage the Chao's first draw can come a frame before the hidden character's exec has
    // set RIDER: taken for a cutscene, that one frame wrote his limb pose into the Chao's
    // stored node matrices (write_pose) and the Chao's head never came back from it for
    // the whole stage (no head in Wild Canyon, Pumpkin Hill and Security Hall; logged:
    // pose_chao called twice at the Chao's first frame, never again).
    // SA2_EV_FIRST=1 (debug): as before.
    let in_stage = (0x8000_0000..0x8C00_0000).contains(&m.r32(0x801E_7768)) && !std::env::var("SA2_EV_FIRST").is_ok_and(|v| v == "1");
    let ev = RIDER.get() == 0 && live && !in_stage;
    // In a stage: ChaoMain's draw walker (.text 0x88CC, from the display's
    // 0x8260) calls the node transform 0x1454 per node; chao_node replaces
    // each result with the retargeted pose (Chao-local, posed below).
    // Chao World: the native Chao rig and its own motions draw it (no rider
    // skeleton copied onto it); elsewhere the rider's pose is retargeted.
    // Chao World self mode: the hidden Shadow's live skeleton (walked by his
    // display, drawing nothing) poses the Chao, so it walks, runs and skates
    // with his motions. SA2_CHAO_RETARGET=1 (debug): also outside self mode;
    // =0: the Chao's own motions.
    // The walker's pilot sits on its native rig (no rider skeleton there).
    // Levels (2026-11-23): the same as the lobby, the Chao's own rig and
    // motions with the overlay from the hidden character's node turns
    // (skate.rs, the pause Extras limits). His limb directions copied on the
    // Chao (level_pose) laid it flat on the board, face down, with his crouch.
    // SA2_LEVEL_RETARGET=1 (debug): that pose, as before.
    // Only for riders on Sonic's skeleton (his exec): the overlay's node map is
    // his and Shadow's. Knuckles's and Rouge's stages keep their own path
    // (knuckles_rules: the Chao's own motions, mirrored where set).
    let old_level = !chao_world(m) && (!matches!(LAST_EXEC.get(), SONIC_EXEC | KNUX_EXEC) || std::env::var("SA2_LEVEL_RETARGET").is_ok_and(|v| v == "1"));
    if PILOT_SEAT.get().is_none() && live && RIDER.get() != 0 && (old_level || (self_retarget(m) && self_in_motion()) || std::env::var("SA2_CHAO_RETARGET").is_ok_and(|v| v == "1")) {
        level_pose(m, c.r[3]);
        CHAO_DRAW.set(true);
        CHAO_NODE.set(0);
        let a = rt::sec(m, 60, 1) + CHAO_DISPLAY;
        rel_original(c, m, a, chao_display);
        // The gear from the hands and feet this draw recorded, in every
        // stage. Before 2026-11-23 a level drew it from inside the hidden
        // character's display: a frame behind the Chao, and in whatever
        // colour state his own draw had left (the Air Shoes came out pink
        // in Rouge's stages).
        hammer_gloves(c, m);
        air_shoes(c, m);
        try_on(c, m);
        return crate::name_tags::finish();
    }
    // ChaoMain queues the model draw past this call (no walker runs inside
    // it, ev28), so the flag stays on for the frame while in a cutscene.
    CHAO_DRAW.set(ev);
    if live && RIDER.get() != 0 {
        // The rest model, for the boots' size (level_pose reads it when retargeting).
        let cw = m.r32(c.r[3] + 0x34);
        let root = if cw == 0 { 0 } else { m.r32(cw + 0x510) };
        if (0x8000_0000..0x8C00_0000).contains(&root) {
            CHAO_REST.with_borrow_mut(|r| if r.is_none() { *r = Some(crate::chao_retarget::read_chao(&|a| m.r32(a), root, [vec![], vec![]])); });
        }
        NATIVE_DRAW.set(true);
        CHAO_NODE.set(0);
        let level = !chao_world(m) && PILOT_SEAT.get().is_none();
        LEVEL_RIG.set(level);
        BOARD_PLACE.set(if level { board_place(m).or_else(|| knux_rig::wall_place(m)) } else { None });
        rel_original(c, m, a, chao_display);
        LEVEL_RIG.set(false);
        BOARD_PLACE.set(None);
        NATIVE_DRAW.set(false);
        knux_rig::hit_log(m, c.r[3]);
        // The gear from the feet and hands this native draw just recorded.
        let t = std::env::var("SA2_GEAR_TEST").unwrap_or_default();
        if !t.contains('g') { hammer_gloves(c, m); }
        if !t.contains('s') { air_shoes(c, m); }
        if !t.contains('t') { try_on(c, m); }
        if sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) % 600 == 0 {
            let rest = CHAO_REST.with_borrow(|r| r.as_ref().map(|r| r.nodes.len()));
            eprintln!("chao-player native: walked {} node transforms, rest model nodes {rest:?}, hands {:?}", CHAO_NODE.get(), HANDS.with_borrow(|h| h.map(|x| x.map(|v| [v[0][3], v[1][3], v[2][3]]))));
        }
        return;
    }
    if ev {
        write_pose(m, c.r[3]);
    }
    rel_original(c, m, a, chao_display);
    crate::name_tags::finish();
    if ev && sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) % 60 == 0 {
        eprintln!("chao-player ev: Chao drew {} nodes, posed {}", CHAO_NODE.get(), POSED.with_borrow(|p| p.len()));
    }
}

thread_local! {
    /// The player's Chao is drawn in a level on its own rig: the overlay is full on.
    static LEVEL_RIG: Cell<bool> = const { Cell::new(false) };
    /// On the board: the Chao's placement (view space) for this draw.
    static BOARD_PLACE: Cell<Option<[[f32; 4]; 3]>> = const { Cell::new(None) };
}

/// While the board drew this frame or the last: where the Chao stands on it
/// (view space). The hidden character's root node gives the facing and the
/// tilt (the Chao's +z is his -x, as seat()); the spot is the board's centre
/// at the top of its deck, the board being drawn at BOARD_SCALE about its
/// centre. shortcut: the deck's top is the board's highest point along his up
/// axis, so the Chao's feet are not put on his two foot spots; upgrade with
/// the deck under each foot if they are seen off it.
fn board_place(m: &Mem) -> Option<[[f32; 4]; 3]> {
    if std::env::var_os("SA2_STATE_LOG").is_some() && sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) % 120 == 0 {
        eprintln!("board-place: called, board {}", BOARD.with_borrow(|b| b.is_some()));
    }
    let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
    let (bf, tris, local) = BOARD.with_borrow(|b| b.clone())?;
    let root = SKEL.with_borrow(|s| ((s.len() == 62 && skel_site("c3"))).then(|| s[0]))?;
    if f > bf + 1 {
        return None;
    }
    // The board's vertices are kept in Chao axes (to_chao: y is his y).
    let top = tris.iter().map(|t| t[0][1]).fold(f32::MIN, f32::max);
    let spot = apply(&root, [local[0], local[1] + (top - local[1]) * BOARD_SCALE, local[2]]);
    let col = |j: usize| { let l = (0..3).map(|i| root[i][j] * root[i][j]).sum::<f32>().sqrt().max(1e-6); [root[0][j] / l, root[1][j] / l, root[2][j] / l] };
    let (c0, c1, c2) = (col(0), col(1), col(2));
    let world: [[f32; 4]; 3] = std::array::from_fn(|i| [c2[i], c1[i], -c0[i], spot[i]]);
    Some(mul34(&mtx(m, m.r32(NJ_BASE_MATRIX)), &world))
}

/// The emotion ball's anchor follows the head from the previous frame, so on
/// a rider moving ~5 units a frame it trails behind. Move it with the Chao.
/// shortcut: offsets +0x738/+0x77C found by a chaowk dump (two float triples
/// ~10 above the root that track it); not yet matched to the decompiled struct.
fn follow_ball(m: &mut Mem, chao: u32) {
    let cw = m.r32(chao + 0x34);
    let f = |m: &Mem, a: u32| f32::from_bits(m.r32(a));
    let now = [f(m, cw + 0x14), f(m, cw + 0x18), f(m, cw + 0x1C)];
    let last = LAST.replace(now);
    if last == [0.0; 3] {
        return;
    }
    for base in [0x738, 0x77C] {
        for i in 0..3u32 {
            let a = cw + base + 4 * i;
            m.w32(a, (f(m, a) + now[i as usize] - last[i as usize]).to_bits());
        }
    }
}

/// The player's own Chao's taskwk (0 before it exists): its sounds are muted.
pub fn own_twp(m: &Mem) -> u32 {
    match STEP.get() { Step::Live(chao) => m.r32(chao + 0x34), _ => 0 }
}

thread_local! {
    /// own_pilot is drawing the Chao at this matrix (view space) right now.
    static PILOT_SEAT: Cell<Option<[[f32; 4]; 3]>> = const { Cell::new(None) };
    /// The frame own_pilot last drew the Chao in.
    static PILOT_FRAME: Cell<u64> = const { Cell::new(u64::MAX) };
    /// own_pilot was called in this run: the walker exec makes the Chao.
    static PILOT_USED: Cell<bool> = const { Cell::new(false) };
    static PILOT_SAID: Cell<bool> = const { Cell::new(false) };
}

thread_local! {
    /// The character exec that ran last (Sonic's, Knuckles's or the walkers').
    static LAST_EXEC: Cell<u32> = const { Cell::new(0) };
    /// The handler another module had on the voice start before ours.
    static PREV_VOICE: Cell<Option<sa2_recomp::rt::Func>> = const { Cell::new(None) };
}

/// The player's Chao rest model height in its own model units (the y range
/// of its vertices), once the Chao is live; for scaling the walker's seat.
pub(crate) fn chao_rest_height(m: &Mem) -> Option<f32> {
    let Step::Live(chao) = STEP.get() else { return None };
    let cw = m.r32(chao + 0x34);
    let root = if cw == 0 { 0 } else { m.r32(cw + 0x510) };
    if !(0x8000_0000..0x8C00_0000).contains(&root) {
        return None;
    }
    CHAO_REST.with_borrow_mut(|r| Some(r.get_or_insert_with(|| crate::chao_retarget::read_chao(&|a| m.r32(a), root, [vec![], vec![]])).height()))
}

/// The pilot was drawn this frame or the last: the Chao task's own display
/// (at the hidden character) stands down.
fn pilot_recent() -> bool {
    sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed).wrapping_sub(PILOT_FRAME.get()) <= 1
}

/// For the Chao Walker (walker.rs): draw the player's own Chao as the pilot.
/// `seat`: Ninja's current matrix at the walker's seat node (view space,
/// camera included), read in the walker's per-node callback. The Chao's own
/// display draws it right now with that matrix in place of its own placement
/// (so its size is the matrix's scale; scale 1 = the Chao model's own units,
/// its rest height is logged once), on its native rig in its current motion.
/// Returns true only when the Chao drew its nodes in this call. The first
/// call also lets the walker exec (0x800B3A8C) load and make the Chao.
/// unverified: not run in a walker stage from this tree (walker.rs calls it
/// from another branch). Its emotion ball and shadow stay with the Chao
/// task's own position (the walker's), not the seat.
pub(crate) fn own_pilot(c: &mut Cpu, m: &mut Mem, seat: &[[f32; 4]; 3]) -> bool {
    PILOT_USED.set(true);
    let Step::Live(chao) = STEP.get() else { return false };
    if RIDER.get() == 0 || m.r32(chao + 0x34) == 0 || !module_linked(m, 60) {
        return false;
    }
    let keep = (c.r, c.f, c.lr);
    PILOT_SEAT.set(Some(*seat));
    CHAO_NODE.set(0);
    c.r[3] = chao;
    chao_display(c, m);
    PILOT_SEAT.set(None);
    (c.r, c.f, c.lr) = keep;
    let drew = CHAO_NODE.get() > 0;
    if drew {
        PILOT_FRAME.set(sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed));
        if !PILOT_SAID.replace(true) {
            let h = CHAO_REST.with_borrow(|r| r.as_ref().map(|r| r.height()));
            eprintln!("chao-player pilot: drawn at the seat ({} nodes), Chao rest model height {h:?} in its own units", CHAO_NODE.get());
        }
    }
    drew
}

fn pin(m: &mut Mem, chao: u32, data1: u32) {
    let cw = m.r32(chao + 0x34);
    if cw == 0 {
        return;
    }
    if data1 == 0 {
        // In a cutscene: Sonic's root as his skipped draw left it.
        if let Some((p, yaw)) = EV_SPOT.get() {
            for (i, v) in p.into_iter().enumerate() {
                m.w32(cw + 0x14 + 4 * i as u32, v.to_bits());
            }
            m.w32(cw + 0x08, 0);
            m.w32(cw + 0x0C, yaw);
            m.w32(cw + 0x10, 0);
        }
        return;
    }
    for o in (0..12).step_by(4) {
        m.w32(cw + 0x14 + o, m.r32(data1 + 0x14 + o));
    }
    // The Chao's own fall (MOVE_WORK Velo.y / Acc.y) kept building up on a
    // body the character places: in self mode it cut a jump short of his
    // (measured 27 units, not his own apex), in a level it pulled the Chao's
    // task work down between pins. Cleared here, where the body is placed.
    // SA2_CHAO_FALL=1 (debug): left as it was.
    let mw = m.r32(chao + 0x38);
    if mw != 0 && !std::env::var("SA2_CHAO_FALL").is_ok_and(|v| v == "1") {
        m.w32(mw + 0x04, 0);
        m.w32(mw + 0x10, 0);
    }
    // Upright: only the rider's yaw. Its pitch/roll would tip the Chao while
    // the emotion ball stays world-up above where the head would be.
    m.w32(cw + 0x08, 0);
    // SA2_CHAO_YAW_OFS (BAMS, debug until measured): the Chao's facing vs the rider's.
    let ofs = std::env::var("SA2_CHAO_YAW_OFS").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
    // The character's yaw goes along (cos, sin), the Chao faces (sin, cos)
    // (chao_yaw): copied as it was, the Chao faced his way on one diagonal
    // only (City Escape before the board: 0 to 90 degrees off). In self mode
    // data1 is the Chao's own task work and chao_display turns it at the draw.
    let yaw = m.r32(data1 + 0x0C);
    let yaw = if data1 == cw || std::env::var("SA2_CHAO_YAW_RAW").is_ok_and(|v| v == "1") { yaw } else { chao_yaw(yaw) };
    m.w32(cw + 0x0C, yaw.wrapping_add(ofs) & 0xFFFF);
    m.w32(cw + 0x10, 0);
}

pub fn install(c: &mut Cpu, _m: &mut Mem) {
    if std::env::var_os("SA2_HEAP_LOG").is_some() {
        sa2_recomp::os::add_frame_hook(|_, m| heap_log(m));
        c.rt.set_override(0x8007_2820, |c, m| tex_load_log(c, m, 0x8007_2820));
        c.rt.set_override(LOAD_TEX, |c, m| tex_load_log(c, m, LOAD_TEX));
    }
    // SA2_HANG_LOG=1 (debug): a thread that prints the latest guest calls when
    // frames stop for 5 s while calls go on (a guest loop that never ends).
    if std::env::var("SA2_HANG_LOG").is_ok_and(|v| v.trim() == "1") {
        std::thread::spawn(|| {
            use std::sync::atomic::Ordering::Relaxed;
            let (mut f0, mut said) = (u64::MAX, 0u64);
            loop {
                std::thread::sleep(std::time::Duration::from_secs(5));
                let f = sa2_recomp::host::FRAMES.load(Relaxed);
                if f == f0 && said != f {
                    said = f;
                    let t: Vec<String> = sa2_recomp::rt::trace().iter().map(|a| format!("{a:#x}")).collect();
                    eprintln!("hang: frame {f} for 5 s, calls {}, last calls {}", sa2_recomp::rt::CALLS.load(Relaxed), t.join(" "));
                }
                f0 = f;
            }
        });
    }
    if !std::env::var("SA2_CHAO_PLAYER").is_ok_and(|v| v.trim() == "1") {
        return;
    }
    if rt::mem1() < 0x0300_0000 {
        eprintln!("chao-player: needs SA2_MEM1_MB >= 48");
        return;
    }
    // Sonic, Shadow and Metal share one exec and display; Knuckles and Rouge
    // one exec (0x8009F8A0) with their own displays (coordinator notes).
    // SA2_SE_LOG=1 (debug): the game's other sound calls too (the positional
    // ones are logged in start_stage), with the caller, to find which sound
    // a moving Chao makes.
    // The character's own sounds while its exec moves a Chao (the landing
    // and stop sound at main.dol 0x80080CB4..0x80080CFC: 0x2016, 0x201A for
    // Metal's physics row 12, else 0x2003) are not played: mob 2026-11-22.
    c.rt.set_override(SE_CALLS[0], |c, m| se_log(c, m, SE_CALLS[0]));
    if std::env::var("SA2_SE_LOG").is_ok_and(|v| v == "1") {
        c.rt.set_override(SE_CALLS[1], |c, m| se_log(c, m, SE_CALLS[1]));
        c.rt.set_override(SE_CALLS[2], |c, m| se_log(c, m, SE_CALLS[2]));
        c.rt.set_override(SE_CALLS[3], |c, m| se_log(c, m, SE_CALLS[3]));
    }
    c.rt.set_override(SONIC_EXEC, |c, m| exec(c, m, SONIC_EXEC));
    c.rt.set_override(KNUX_EXEC, |c, m| exec(c, m, KNUX_EXEC));
    c.rt.set_override(knux_rig::SET_PLAYER_CHAR, knux_rig::set_char);
    c.rt.set_override(knux_rig::AFTER_IMAGE_DISP, knux_rig::after_image);
    c.rt.set_override(knux_rig::SHADOW_DISP, knux_rig::shadow_disp);
    if std::env::var_os("SA2_CHAO_DRAWLOG").is_some() {
        c.rt.set_override(0x8011_E23C, |c, m| knux_rig::draw_probe(c, m, 0x8011_E23C));
        c.rt.set_override(0x8011_E368, |c, m| knux_rig::draw_probe(c, m, 0x8011_E368));
        c.rt.set_override(0x8011_E6F0, |c, m| knux_rig::draw_probe(c, m, 0x8011_E6F0));
        c.rt.set_override(0x8011_D484, |c, m| knux_rig::draw_probe(c, m, 0x8011_D484));
        c.rt.set_override(0x8011_D554, |c, m| knux_rig::draw_probe(c, m, 0x8011_D554));
        c.rt.set_override(0x8011_D410, |c, m| knux_rig::draw_probe(c, m, 0x8011_D410));
        c.rt.set_override(0x8011_D2F4, |c, m| knux_rig::draw_probe(c, m, 0x8011_D2F4));
        c.rt.set_override(0x8011_DF20, |c, m| knux_rig::draw_probe(c, m, 0x8011_DF20));
        c.rt.set_override(0x8011_F720, |c, m| knux_rig::draw_probe(c, m, 0x8011_F720));
        c.rt.set_override(0x8011_F6B4, |c, m| knux_rig::draw_probe(c, m, 0x8011_F6B4));
        c.rt.set_override(0x8011_F668, |c, m| knux_rig::draw_probe(c, m, 0x8011_F668));
        c.rt.set_override(0x8011_ED60, |c, m| knux_rig::draw_probe(c, m, 0x8011_ED60));
    }
    c.rt.set_override(knux_rig::GLIDE_WALL, |c, m| knux_rig::wall_check(c, m, knux_rig::GLIDE_WALL));
    c.rt.set_override(knux_rig::JUMP_WALL, |c, m| knux_rig::wall_check(c, m, knux_rig::JUMP_WALL));
    // The walkers' exec: only once walker.rs asks for the pilot (own_pilot).
    c.rt.set_override(WALKER_EXEC, |c, m| {
        LAST_EXEC.set(WALKER_EXEC);
        if PILOT_USED.get() { exec(c, m, WALKER_EXEC) } else { original(c, m, WALKER_EXEC) }
    });
    c.rt.set_override(DESTROY_TASK, destroy_task);
    if std::env::var_os("SA2_TEST_CAM").is_some() {
        c.rt.set_override(NJ_SET_CAMERA, test_cam);
    }
    c.rt.set_override(SPIN_FLASH_DISP, spin_flash);
    if std::env::var_os("SA2_TRACE_EVDRAW").is_some() {
        c.rt.set_override(DRAW_MOTION, trace_draw_motion);
        c.rt.set_override(0x8011_DF44, |c, m| ev_draw(c, m, 0x8011_DF44));
        c.rt.set_override(0x8011_E17C, |c, m| ev_draw(c, m, 0x8011_E17C));
        c.rt.set_override(0x8011_E1EC, |c, m| ev_draw(c, m, 0x8011_E1EC));
        c.rt.set_override(0x8011_F294, |c, m| ev_draw(c, m, 0x8011_F294));
        c.rt.set_override(0x8011_F21C, |c, m| ev_draw(c, m, 0x8011_F21C));
    }
    // The intro cutscene: skip Sonic's eyes and shape-motion parts, read his
    // node matrices from the skipped walk, put the Chao at his root.
    c.rt.set_override(DRAW_MOTION, trace_draw_motion);
    c.rt.set_override(EV_SHAPE_DRAW, ev_shape_draw);
    c.rt.set_override(0x8011_E19C, |c, m| ev_draw(c, m, 0x8011_E19C));
    c.rt.set_override(0x8011_E1C4, |c, m| ev_draw(c, m, 0x8011_E1C4));
    c.rt.set_override(0x8011_E1EC, |c, m| ev_draw(c, m, 0x8011_E1EC));
    c.rt.set_override(CNK_CACHE_DRAW, cnk_cache_draw);
    c.rt.set_override(0x8011_DF44, |c, m| ev_draw(c, m, 0x8011_DF44));
    c.rt.set_override(NODE_TRANSFORM, node_transform);
    sa2_recomp::os::add_frame_hook(ev_frame_hook);
    // walker.rs (SA2_CHAO_WALKER) installs its own handler there first: kept, and
    // it takes the voice task's calls while a walker is the character (voice).
    c.rt.set_override(NATIVE_JETS, native_jets);
    for (at, tag) in [(SONIC_DISP_LATE, "late"), (SONIC_DISP_2C, "2c")] {
        // Only where asked for: the probe below is not settled yet.
        if std::env::var("SA2_GAP_HIDE").is_ok_and(|v| v.split(',').any(|x| x == tag)) {
            c.rt.set_override(at, match tag { "late" => |c: &mut Cpu, m: &mut Mem| gap_disp(c, m, SONIC_DISP_LATE), _ => |c: &mut Cpu, m: &mut Mem| gap_disp(c, m, SONIC_DISP_2C) });
        }
    }
    PREV_VOICE.set(c.rt.overrides.get(&ADX_START_AFS).copied());
    c.rt.set_override(ADX_START_AFS, voice);
    c.rt.set_override(SONIC_DISP, |c, m| display(c, m, SONIC_DISP));
    // SA2_CHAO_KNUX_DISP=0 (debug): leave Knuckles's and Rouge's displays alone.
    if !std::env::var("SA2_CHAO_KNUX_DISP").is_ok_and(|v| v == "0") {
        c.rt.set_override(KNUX_DISP, |c, m| display(c, m, KNUX_DISP));
        c.rt.set_override(ROUGE_DISP, |c, m| display(c, m, ROUGE_DISP));
    }
}

/// fn_8007BC7C: the character's effect display; for Shadow it draws his Air
/// Shoes' jets at his own toe and heel nodes while his skate motions play
/// (gate: model slot 185 loaded, main.dol 0x8007BD2C).
const NATIVE_JETS: u32 = 0x8007_BC7C;

/// While the player's Chao is out, the hidden Shadow's own jets are not
/// drawn: they followed his feet (drawn at the Chao's size), not the shoes on
/// the Chao, so a second set of flames moved about the Chao's feet out of
/// step with its shoes (seen with the Chao's shoes switched off: flames
/// still there). The game's own gate is used: slot 185 empty for the call.
/// SA2_SHOW_SONIC (debug: the character drawn) keeps them, to compare.
fn native_jets(c: &mut Cpu, m: &mut Mem) {
    let gate = MODELS + 4 * 185;
    let keep = m.r32(gate);
    let hide = matches!(STEP.get(), Step::Live(_)) && std::env::var_os("SA2_SHOW_SONIC").is_none();
    if hide {
        m.w32(gate, 0);
    }
    original(c, m, NATIVE_JETS);
    if hide {
        m.w32(gate, keep);
    }
}

/// The character task's other draw functions (task +0x1C and +0x2C, read
/// from his task in the lobby). SA2_GAP_HIDE=late,2c (debug probe): each is
/// skipped in Chao World while the player's Chao is not up, to find which
/// one draws the lone shadow seen on arriving in an area.
const SONIC_DISP_LATE: u32 = 0x8007_BEB4;
const SONIC_DISP_2C: u32 = 0x8007_E304;

fn gap_disp(c: &mut Cpu, m: &mut Mem, at: u32) {
    if chao_world(m) && !matches!(STEP.get(), Step::Live(_)) {
        return;
    }
    original(c, m, at);
}

/// ADX start from an AFS archive; the voice task calls it at 0x80012760
/// (see walker.rs). mob 2026-11-21: no Sonic lines while the Chao plays.
/// shortcut: drops every voice line the voice task starts in the stage, not
/// only Sonic's; event (cutscene) audio is not covered yet.
const ADX_START_AFS: u32 = 0x8013_476C;
const FROM_VOICE_TASK: u32 = 0x8001_2768;
/// mob 2026-11-21: the Dark Chaos lines "from 2P" = the Dark Chao Walker's own
/// sound (0x201E, walker.rs), in the 2P walker bank, loaded by name with the
/// game's bank loader and its own string.
/// shortcut: unverified that loading this bank in City Escape leaves the
/// stage's own banks intact; check by ear.
const LOAD_BANK: u32 = 0x8006_CFEC;
const WALKER_BANK_2P: u32 = 0x8017_5E9C; // "se_ch_wk_BATTLE.mlt"

/// The own Chao's taskwk (task +0x34), 0 while it isn't out: its own Chao
/// sounds are not played to its player (start_stage's positional sound hooks).
pub(crate) fn own_chao_twp(m: &Mem) -> u32 {
    match STEP.get() {
        Step::Live(task) => m.r32(task + 0x34),
        _ => 0,
    }
}

fn voice(c: &mut Cpu, m: &mut Mem) {
    // From the start of the run: the drop line plays before the Chao is loaded.
    if c.lr == FROM_VOICE_TASK {
        // In a walker stage the walker's own handler (its sound in place of
        // the line) takes the call, when walker.rs installed one.
        if let (true, Some(prev)) = (LAST_EXEC.get() == WALKER_EXEC, PREV_VOICE.get()) {
            return prev(c, m);
        }
        // MMO choice: the player's own voice line is dropped, with no replacement.
        return;
    }
    original(c, m, ADX_START_AFS);
}

/// Ninja "draw object with motion" (r3 object, r4 motion, f1 frame), which
/// eventD calls for every cutscene entity.
const DRAW_MOTION: u32 = 0x8011_E214;

thread_local! {
    static SEEN_DRAWS: std::cell::RefCell<std::collections::BTreeSet<(u32, u32)>> = Default::default();
}

/// SA2_TRACE_EVDRAW: log each new (object, motion) pair drawn, with the
/// object's root position (+0x14) and the module's lr, to find the
/// cutscene's Sonic.
fn trace_draw_motion(c: &mut Cpu, m: &mut Mem) {
    ev_draw(c, m, DRAW_MOTION);
}

/// Trace/hide for every Ninja draw call eventD's entity dispatcher uses.
fn ev_draw(c: &mut Cpu, m: &mut Mem, addr: u32) {
    let (obj, mot, lr) = (c.r[3], c.r[4], c.lr);
    if CHAO_DRAW.get() && sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) % 60 == 0 {
        eprintln!("chao-player ev: chao draw via {addr:#x} obj {obj:#x} motion {mot:#x} lr {lr:#x} nodes so far {}", CHAO_NODE.get());
    }
    if LCAP.get() {
        // Knuckles's own items (claws, gloves, necklace) at his hidden hands: not drawn.
        if addr == 0x8011_DF44 && knux_rig::item_draw(c.lr) { return; }
        return level_capture(c, m, addr);
    }
    if knux_rig::late_body(lr) {
        return;
    }
    if from_board_disp(c, m) {
        return board_draw(c, m, addr);
    }
    if EV_SONIC.contains(&obj) {
        // The cutscene's Sonic: walk his motion for the node matrices, draw nothing.
        NODES.with_borrow_mut(|n| n.clear());
        CAPTURE.set(true);
        original(c, m, addr);
        CAPTURE.set(false);
        if obj == EV_SONIC[0] {
            SKEL.set(NODES.with_borrow(|n| n.clone()));
        }
        let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
        if f % 60 == 0 {
            eprintln!("chao-player ev motion: frame {f} obj {obj:#x} nodes {}", NODES.with_borrow(|n| n.len()));
        }
        return;
    }
    if std::env::var_os("SA2_TRACE_EVDRAW").is_some()
        && SEEN_DRAWS.with_borrow_mut(|s| s.insert((obj, mot))) {
        eprintln!(
            "evdraw {addr:#010x}: obj {obj:#010x} motion {mot:#010x} frame {} lr {lr:#010x} frame# {}",
            c.f[1].ps0,
            sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed)
        );
    }
    // SA2_EV_HIDE=addr,addr: skip those objects (debug, to identify Sonic).
    let hide = std::env::var("SA2_EV_HIDE").unwrap_or_default();
    if hide.split(',').any(|h| u32::from_str_radix(h.trim().trim_start_matches("0x"), 16) == Ok(obj)) {
        return;
    }
    original(c, m, addr);
}

/// eventD's shape-motion draw (r3 object, r4 motion, r5 shape; eventD .text
/// +0x6524/+0x6C1C): it runs the Ninja motion walker 0x80120808 with the
/// per-node draw callback r6 = 0x8011DF20 (njCnkCacheDrawModel).
const EV_SHAPE_DRAW: u32 = 0x8011_F2BC;
const CNK_CACHE_DRAW: u32 = 0x8011_DF20;
const NJ_BASE_MATRIX: u32 = 0x803A_E0C0; // _nj_base_matrix (pointer)
const NJ_CURRENT_MATRIX: u32 = 0x803A_E0C4; // _nj_current_matrix_ptr_
/// The intro's (e0000) Sonic: model pointers of his event entity records as
/// eventD's scene table (eventD sec6+0x14BC, 0x2C records) holds them in the
/// drop scenes; clearing these four in scene 3 (eyes, 3 shape-motion parts)
/// removed him (chao-player\ev14\frame_004309.png).
/// shortcut: loaded addresses from one run of e0000, not file offsets; they
/// hold while e0000 loads at the same place (deterministic boot). Upgrade to
/// event-file offsets before other cutscenes.
/// unverified: 0x81285fa4/0x812a4e4c/0x81299380 (other drop scenes) are Sonic.
const EV_SONIC: [u32; 8] = [
    0x812a_24ec, 0x8128_5fa4, 0x8128_a5fc, 0x8129_0160, 0x8129_5984, 0x812a_4e4c, 0x8129_9380, 0,
];

thread_local! {
    /// Sonic's root (world) and the Chao's yaw (BAMS) from his last skipped draw.
    static EV_SPOT: Cell<Option<([f32; 3], u32)>> = const { Cell::new(None) };
    /// Set while a Sonic model is walked: node draws are recorded, not drawn.
    static CAPTURE: Cell<bool> = const { Cell::new(false) };
    /// Node world matrices (3x4) of the model being walked, in walk order.
    static NODES: std::cell::RefCell<Vec<[[f32; 4]; 3]>> = const { std::cell::RefCell::new(Vec::new()) };
    static EV_FRAME: Cell<u64> = const { Cell::new(u64::MAX) };
    /// Sonic's root this frame, for the frame hook's load/create step.
    static PENDING: Cell<Option<[u32; 3]>> = const { Cell::new(None) };
}

fn ev_frame_hook(c: &mut Cpu, m: &mut Mem) {
    if let Some(p) = PENDING.take() {
        if !matches!(STEP.get(), Step::Live(_)) {
            tick(c, m, p);
        }
    }
}

/// TEST (SA2_SKEL_OFF=c1,c4,k2): a reader of the captured skeleton that is switched off, to find which one hides the head.
pub(crate) fn skel_site(id: &str) -> bool {
    !std::env::var("SA2_SKEL_OFF").is_ok_and(|v| v.split(',').any(|x| x == id))
}

fn mtx(m: &Mem, a: u32) -> [[f32; 4]; 3] {
    std::array::from_fn(|i| std::array::from_fn(|j| f32::from_bits(m.r32(a + 4 * (4 * i + j) as u32))))
}

/// inv(a) * b for 3x4 affine matrices (rotation part may carry scale).
fn inv_mul(a: &[[f32; 4]; 3], b: &[[f32; 4]; 3]) -> [[f32; 4]; 3] {
    let r = |i: usize, j: usize| a[i][j];
    let det = r(0, 0) * (r(1, 1) * r(2, 2) - r(1, 2) * r(2, 1)) - r(0, 1) * (r(1, 0) * r(2, 2) - r(1, 2) * r(2, 0))
        + r(0, 2) * (r(1, 0) * r(2, 1) - r(1, 1) * r(2, 0));
    let d = if det.abs() < 1e-12 { 1.0 } else { 1.0 / det };
    let inv: [[f32; 3]; 3] = [
        [(r(1, 1) * r(2, 2) - r(1, 2) * r(2, 1)) * d, (r(0, 2) * r(2, 1) - r(0, 1) * r(2, 2)) * d, (r(0, 1) * r(1, 2) - r(0, 2) * r(1, 1)) * d],
        [(r(1, 2) * r(2, 0) - r(1, 0) * r(2, 2)) * d, (r(0, 0) * r(2, 2) - r(0, 2) * r(2, 0)) * d, (r(0, 2) * r(1, 0) - r(0, 0) * r(1, 2)) * d],
        [(r(1, 0) * r(2, 1) - r(1, 1) * r(2, 0)) * d, (r(0, 1) * r(2, 0) - r(0, 0) * r(2, 1)) * d, (r(0, 0) * r(1, 1) - r(0, 1) * r(1, 0)) * d],
    ];
    std::array::from_fn(|i| {
        std::array::from_fn(|j| {
            let s: f32 = (0..3).map(|k| inv[i][k] * b[k][j]).sum();
            if j == 3 { s - (0..3).map(|k| inv[i][k] * a[k][3]).sum::<f32>() } else { s }
        })
    })
}

fn ev_shape_draw(c: &mut Cpu, m: &mut Mem) {
    if !EV_SONIC[..7].contains(&c.r[3]) {
        return original(c, m, EV_SHAPE_DRAW);
    }
    let obj = c.r[3];
    NODES.with_borrow_mut(|n| n.clear());
    CAPTURE.set(true);
    original(c, m, EV_SHAPE_DRAW);
    CAPTURE.set(false);
    let Some(root) = SKEL.with_borrow(|n| n.first().copied()) else { return };
    // Sonic faces -x in his model space; the Chao faces +z at yaw 0.
    let (fx, fz) = (-root[0][0], -root[2][0]);
    let yaw = (fx.atan2(fz) * 65536.0 / std::f32::consts::TAU) as i32 as u32;
    EV_SPOT.set(Some(([root[0][3], root[1][3], root[2][3]], yaw)));
    // Once a frame (several Sonic parts draw per frame): load/create the Chao.
    let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
    if EV_FRAME.replace(f) != f {
        if f % 60 == 0 {
            eprintln!("chao-player ev: frame {f} model {:#x} nodes {} root {:?} yaw {yaw:#x}",
                obj, NODES.with_borrow(|n| n.len()), [root[0][3], root[1][3], root[2][3]]);
        }
        // Creating inside the draw stalled the frame loop at Live (ev15, frame
        // 2016). SA2_EV_CREATE=hook creates from the frame hook instead;
        // unset = Sonic hidden only (no Chao in the cutscene yet).
        let p = root.map(|row| row[3].to_bits());
        match std::env::var("SA2_EV_CREATE").as_deref() {
            Ok("hook") => PENDING.set(Some(p)),
            Ok("draw") if !matches!(STEP.get(), Step::Live(_)) => tick(c, m, p),
            _ => {}
        }
    }
}

/// While a Sonic model is walked: record the node's world matrix
/// (inv(base) * current), draw nothing.
fn cnk_cache_draw(c: &mut Cpu, m: &mut Mem) {
    // Sonic's display 0x8007C7B0: with obj2 +0x24 & 0x40008 it draws the
    // spin effect model (0x801E6688 +0xCC / +0x324, efftex) at a node matrix.
    if LCAP.get() && (c.lr == 0x8007_C7C4 || c.lr == 0x8007_C810) {
        let cur = mtx(m, m.r32(NJ_CURRENT_MATRIX));
        let w = inv_mul(&mtx(m, m.r32(NJ_BASE_MATRIX)), &cur);
        return ball_draw(c, m, CNK_CACHE_DRAW, [w[0][3], w[1][3], w[2][3]]);
    }
    if !CAPTURE.get() {
        return original(c, m, CNK_CACHE_DRAW);
    }
    if knux_rig::walk_unseen(c, m) {
        return;
    }
    c.r[3] = 0;
}

/// Ninja node transform (r3 node) the motion walker 0x8012099C applies to
/// each node after its push: afterwards the current matrix is that node's.
const NODE_TRANSFORM: u32 = 0x8011_3C4C;

fn node_transform(c: &mut Cpu, m: &mut Mem) {
    original(c, m, NODE_TRANSFORM);
    if CAPTURE.get() {
        let cur = mtx(m, m.r32(NJ_CURRENT_MATRIX));
        let base = mtx(m, m.r32(NJ_BASE_MATRIX));
        NODES.with_borrow_mut(|n| n.push(inv_mul(&base, &cur)));
    }
}

/// Steam Sonic's 62 nodes (sonicmdl.prs depth-first, the retarget's ids) ->
/// the e0000 cutscene Sonic skeleton's 56 (0x812a24ec, depth-first), matched
/// by parent structure and rest offsets (chao-player\ev25\steam62.txt vs
/// ev24\skeleton.txt): legs z-0.283/-0.324 6-15 -> 20-28 and z+0.324 16-25
/// -> 6-14 (Steam's extra ankle offset node shares its parent's), tail 26-30
/// -> 15-19, spine 31-34 -> 29-32, arms z+ 35-42 -> 49-55 and z- 54-61 ->
/// 33-39, head 43-53 -> 40-48. The event has no Steam node 2 (0.094 up): it
/// uses node 2 (5.13 up, Steam 1) for both.
const STEAM_TO_EV: [u8; 62] = [
    0, 2, 2, 3, 4, 5, 20, 21, 22, 23, 24, 24, 25, 26, 27, 28, 6, 7, 8, 9, 10, 10, 11, 12, 13, 14, 15,
    16, 17, 18, 19, 29, 30, 31, 32, 49, 50, 51, 52, 53, 54, 55, 55, 40, 41, 42, 43, 44, 43, 44, 45,
    46, 47, 48, 33, 34, 35, 36, 37, 38, 39, 39,
];

thread_local! {
    static SAID_POSE: Cell<bool> = const { Cell::new(false) };
    static SAID_NODE: Cell<bool> = const { Cell::new(false) };
    /// Set while the cutscene Chao draws: its node matrices come from the pose.
    static CHAO_DRAW: Cell<bool> = const { Cell::new(false) };
    static CHAO_NODE: Cell<usize> = const { Cell::new(0) };
    static CHAO_FRAME: Cell<u64> = const { Cell::new(u64::MAX) };
    /// base * Chao placement, as its display left it before the first node.
    static CHAO_PLACE: Cell<[[f32; 4]; 3]> = const { Cell::new([[0.0; 4]; 3]) };
    static CHAO_REST: std::cell::RefCell<Option<crate::chao_retarget::ChaoRest>> = const { std::cell::RefCell::new(None) };
    /// Retargeted Chao node worlds (Chao model space) for this frame.
    static POSED: std::cell::RefCell<Vec<crate::chao_retarget::Xf>> = const { std::cell::RefCell::new(Vec::new()) };
    /// Sonic's cutscene skeleton (56 world matrices) from this frame's walk.
    static SKEL: std::cell::RefCell<Vec<[[f32; 4]; 3]>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// ChaoMain's per-node transform (r3 AL_OBJECT, same first 13 words as an
/// NJS_OBJECT), called by its node walker (.text 0xAA0) right after the push.
const AL_NODE_TRANSFORM: u32 = 0x1454;

/// Debug: ChaoMain's recursive matrix-push walkers; log which run while the
/// cutscene Chao displays (finding its node draw).
const AL_WALKERS: [u32; 6] = [0xAA0, 0x13DC, 0x88CC, 0x2CB9C, 0x2DEA4, 0x5113C];
macro_rules! walker_probe {
    ($($n:ident = $i:expr),*) => {$(
        fn $n(c: &mut Cpu, m: &mut Mem) {
            if CHAO_DRAW.get() && sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) % 60 == 0 {
                eprintln!("chao-player ev: walker {:#x} r3 {:#x} r4 {:#x}", AL_WALKERS[$i], c.r[3], c.r[4]);
            }
            let a = rt::sec(m, 60, 1) + AL_WALKERS[$i];
            rel_original(c, m, a, $n);
        }
    )*};
}
walker_probe!(w0 = 0, w1 = 1, w2 = 2, w3 = 3, w4 = 4, w5 = 5);

fn chao_node(c: &mut Cpu, m: &mut Mem) {
    if !CHAO_DRAW.get() {
        let a = rt::sec(m, 60, 1) + AL_NODE_TRANSFORM;
        if NATIVE_DRAW.get() && CHAO_NODE.get() == 0 {
            // The walker's pilot: the seat matrix in place of the display's
            // own placement; every node below inherits it.
            if let Some(seat) = PILOT_SEAT.get().or(BOARD_PLACE.get()) {
                let cur_ptr = m.r32(NJ_CURRENT_MATRIX);
                for r in 0..3 { for k in 0..4 { m.w32(cur_ptr + 4 * (4 * r + k) as u32, seat[r][k].to_bits()); } }
            }
            if LEVEL_RIG.get() && !std::env::var("SA2_GEAR_TEST").is_ok_and(|v| v.contains('r')) { knux_rig::at_root(m); }
            CHAO_PLACE.set(mtx(m, m.r32(NJ_CURRENT_MATRIX)));
        }
        rel_original(c, m, a, chao_node);
        // The player's Chao on its native rig: its foot nodes for the boots.
        if NATIVE_DRAW.get() {
            let i = CHAO_NODE.replace(CHAO_NODE.get() + 1);
            // Only a real model node: in the Chao's fly and swim motions (130,
            // 142) the walker's r3 here is not one, and reading it as a model
            // ran into unmapped memory (the game stopped 15 frames into a jump).
            if i == 6 { DRAWN_FOOT.set(if plain_object(m, c.r[3]) { c.r[3] } else { 0 }); }
            let native = mtx(m, m.r32(NJ_CURRENT_MATRIX));
            // The feet on the hidden Sonic's walk, jog and run (the Chao's own motions 100, 101, 119).
            let own = if (i == 6 || i == 13) && GAIT.get().is_some() && matches!(LAST_MOTION.get(), 100 | 101 | 119) { foot_limit(m, i, &native) } else { native };
            // Wearing Shadow's shoes and moving: the web hub's skate overlay
            // (skate.rs) on its 8 mapped nodes, from the rider's live pose.
            let blend = skate_blend(i == 0);
            if LEVEL_RIG.get() || (blend > 0.0 && SHADOW_TEX.get().is_some()) {
                if let Some(&(_, src, w, cap)) = crate::skate::MAP.iter().find(|e| e.0 == i) {
                    // The pause Extras switches choose; SA2_SKATE_CAPS (debug) overrides them.
                    let (w, cap) = match std::env::var("SA2_SKATE_CAPS") {
                        Ok(mode) => crate::skate::limits(i, w, cap, &mode),
                        Err(_) => {
                            let (body, head) = crate::pause_extras::rig_limits();
                            crate::skate::rig_limits(i, w, cap, body, head)
                        }
                    };
                    let (w, cap) = knux_rig::spin_limits(i, w, cap);
                    if !std::env::var("SA2_ROW_OFF").is_ok_and(|v| v == "all" || v.split('+').any(|x| x.parse() == Ok(i))) { skate_node(m, i, src, w, cap); }
                    if (i == 6 || i == 13) && !std::env::var("SA2_GEAR_TEST").is_ok_and(|v| v.contains('f')) {
                        foot_place(m, i, src, &own, blend);
                    }
                }
            }
            // The hands as this draw places them, for the hammer gloves: after
            // the skate overlay, or the gloves stay on the Chao's own run arms
            // while the arm meshes take the skate pose (up to 75 degrees apart).
            // (Punch 2 on the level rig: the Chao's karate mirrored, as on the old path.)
            if LEVEL_RIG.get() && MIRROR.get() { mirror_node(m, i, m.r32(NJ_CURRENT_MATRIX)); }
            if matches!(i, 1 | 14 | 15 | 16 | 17 | 18) && std::env::var_os("SA2_STATE_LOG").is_some() && sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) % 120 == 0 {
                let (p, cur) = (CHAO_PLACE.get(), mtx(m, m.r32(NJ_CURRENT_MATRIX)));
                let (o, x) = (inv_mul(&p, &own), inv_mul(&p, &cur));
                let det = |x: &[[f32; 4]; 3]| x[0][0] * (x[1][1] * x[2][2] - x[1][2] * x[2][1]) - x[0][1] * (x[1][0] * x[2][2] - x[1][2] * x[2][0]) + x[0][2] * (x[1][0] * x[2][1] - x[1][1] * x[2][0]);
                eprintln!("head-node: node {i} model {:#x} own det {:.3} at [{:.2} {:.2} {:.2}] drawn det {:.3} at [{:.2} {:.2} {:.2}] flags {:#x}", m.r32(c.r[3].wrapping_add(4)), det(&o), o[0][3], o[1][3], o[2][3], det(&x), x[0][3], x[1][3], x[2][3], m.r32(c.r[3]));
            }
            record_hand(m, i, m.r32(NJ_CURRENT_MATRIX));
            knux_rig::hit_node(m, i);
            track_log(m, i, &own);
            head_trace(m, i, &own);
            if (i == 6 || i == 13) && std::env::var("SA2_FOOT_TRACE").is_ok_and(|v| v == "1") {
                // SA2_FOOT_TRACE=1 (debug): each foot as drawn, in every gait (foot-trace has the run only):
                // its turn from the rest model in the Chao's placement, and what the body's drawn turn adds.
                let r3 = |x: &[[f32; 4]; 3]| -> [[f32; 3]; 3] { std::array::from_fn(|a| std::array::from_fn(|b| x[a][b])) };
                let p = CHAO_PLACE.get();
                let rest = CHAO_REST.with_borrow(|r| r.as_ref().and_then(|r| r.worlds(&[]).get(i).map(|x| x.0)));
                if let Some(rest) = rest {
                    let turn = |x: &[[f32; 4]; 3]| crate::skate::mul(&crate::skate::unit(&r3(&inv_mul(&p, x))), &crate::skate::inv(&crate::skate::unit(&rest)));
                    let (d, o) = (turn(&mtx(m, m.r32(NJ_CURRENT_MATRIX))), turn(&native));
                    // The side turn read from the side axis (skate::foot_twist's way): it holds when the foot is pitched steeply.
                    let yaw = |t: &[[f32; 3]; 3]| (-t[2][0]).atan2(t[0][0]).to_degrees();
                    let body = BODY.get().map(|(bo, bd, _)| side_roll(&crate::skate::mul(&crate::skate::unit(&r3(&inv_mul(&p, &bd))), &crate::skate::inv(&crate::skate::unit(&r3(&inv_mul(&p, &bo)))))));
                    eprintln!("foot-drawn: f{} node {i} chao motion {} blend {blend:.2} | drawn side {:.1} roll {:.1} pitch {:.1} | own motion side {:.1} roll {:.1} pitch {:.1} | body added side {:.1} roll {:.1}",
                        sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed), LAST_MOTION.get(),
                        yaw(&d), side_roll(&d).1, (-d[1][2]).atan2(d[2][2]).to_degrees(),
                        yaw(&o), side_roll(&o).1, (-o[1][2]).atan2(o[2][2]).to_degrees(),
                        body.map_or(f32::NAN, |b| b.0), body.map_or(f32::NAN, |b| b.1));
                }
            }
            if i == 7 || i == 8 {
                let k = std::env::var("SA2_TAIL_K").ok().and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(0.25);
                tail_calm(m, i, i - 7, &own, k);
            }
            if i == 1 {
                // The body: every other part hangs from it (head, arms, feet, tail).
                if GAIT.get().is_some() && !std::env::var("SA2_BOB_CALM").is_ok_and(|v| v == "0") {
                    tail_calm(m, i, 2, &own, head_smooth());
                }
                let feet_at = bob_calm(m);
                BODY.set(Some((own, mtx(m, m.r32(NJ_CURRENT_MATRIX)), feet_at)));
            }
            if i == 14 && GAIT.get().is_some() && !std::env::var("SA2_BOB_CALM").is_ok_and(|v| v == "0") {
                // The head's own turn, eased like the body's.
                tail_calm(m, i, 3, &own, head_smooth());
            }
            if let Some(k) = SHOES.iter().position(|s| s.0 == i) {
                let cur = mtx(m, m.r32(NJ_CURRENT_MATRIX));
                FEET.with_borrow_mut(|f| f[k] = Some(cur));
                if !std::env::var("SA2_GEAR_TEST").is_ok_and(|v| v.contains('h')) { hide_foot(m, m.r32(NJ_CURRENT_MATRIX)); }
            }
        }
        return crate::name_tags::node(mtx(m, m.r32(NJ_CURRENT_MATRIX)));
    }
    let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
    // In a stage the display resets the count; in a cutscene, each frame.
    if CHAO_FRAME.replace(f) != f && RIDER.get() == 0 {
        CHAO_NODE.set(0);
    }
    let i = CHAO_NODE.replace(CHAO_NODE.get() + 1);
    let cur_ptr = m.r32(NJ_CURRENT_MATRIX);
    if i == 0 {
        CHAO_PLACE.set(mtx(m, cur_ptr));
        let root = c.r[3];
        CHAO_REST.with_borrow_mut(|r| {
            if r.is_none() {
                let rest = crate::chao_retarget::read_chao(&|a| m.r32(a), root, [vec![], vec![]]);
                eprintln!("chao-player ev: Chao rest model {root:#x} nodes {}", rest.nodes.len());
                *r = Some(rest);
            }
        });
        if RIDER.get() == 0 {
            pose_chao(false);
        } else {
            level_track_log(m);
        }
    }
    let a = rt::sec(m, 60, 1) + AL_NODE_TRANSFORM;
    rel_original(c, m, a, chao_node);
    crate::name_tags::node(mtx(m, cur_ptr));
    // The hands as the Chao's own motion places them (replaced below when posed).
    record_hand(m, i, cur_ptr);
    if MIRROR.get() {
        mirror_node(m, i, cur_ptr);
        record_foot(m, i, cur_ptr);
        return record_hand(m, i, cur_ptr);
    }
    let Some((r, t)) = POSED.with_borrow(|p| p.get(i).copied()) else {
        // Its own motion (Knuckles's and Rouge's stages): the feet as drawn, for the shoes.
        record_foot(m, i, cur_ptr);
        if i == 40 && !SAID_NODE.replace(true) {
            eprintln!("chao-player: node transform past 40 nodes, posed {} (pose not applied)", POSED.with_borrow(|p| p.len()));
        }
        return;
    };
    if !SAID_POSE.replace(true) {
        eprintln!("chao-player: Sonic's pose applied to the Chao's draw (node {i}, rider {:#x})", RIDER.get());
    }
    let p = if POSED_WORLD.get() { mtx(m, m.r32(NJ_BASE_MATRIX)) } else { CHAO_PLACE.get() };
    // current = place * [r t]
    for row in 0..3 {
        for col in 0..4 {
            let v: f32 = if col < 3 {
                (0..3).map(|k| p[row][k] * r[k][col]).sum()
            } else {
                (0..3).map(|k| p[row][k] * t[k]).sum::<f32>() + p[row][3]
            };
            m.w32(cur_ptr + 4 * (4 * row + col) as u32, v.to_bits());
        }
    }
    record_hand(m, i, cur_ptr);
    // The foot nodes' view-space matrices, for the boots (next draw).
    if let Some(k) = SHOES.iter().position(|s| s.0 == i) {
        let cur = mtx(m, cur_ptr);
        FEET.with_borrow_mut(|f| f[k] = Some(cur));
        hide_foot(m, cur_ptr);
    }
}

/// SA2_TRACK_LOG=1 in a level (rider mode), once per Chao draw: the hidden
/// character's action state and motion, whether his pose is on the Chao
/// (posed nodes; 0 = the Chao's own motion) and whether it sits on the board,
/// and the world headings in degrees: his root node (he faces -x), the Chao's
/// placement (it faces +z; on the board, the seat's), and the board's own x
/// and z axes less his heading (constant through a turn = the board turns with him).
fn level_track_log(m: &Mem) {
    if !std::env::var("SA2_TRACK_LOG").is_ok_and(|v| v == "1") {
        return;
    }
    let deg = |x: f32, z: f32| x.atan2(z).to_degrees();
    let wrap = |d: f32| (d + 540.0).rem_euclid(360.0) - 180.0;
    let Some(root) = SKEL.with_borrow(|s| s.first().copied()) else { return };
    let rider = deg(-root[0][0], -root[2][0]);
    let seated = POSED_WORLD.get();
    let chao = if seated {
        // seat(): the Chao's +z is his root's -x.
        rider
    } else {
        let place = inv_mul(&mtx(m, m.r32(NJ_BASE_MATRIX)), &CHAO_PLACE.get());
        deg(place[0][2], place[2][2])
    };
    let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
    let board = BOARD_AXES.get().filter(|b| f <= b.0 + 1).map(|(_, x, z)| (wrap(x - rider), wrap(z - rider)));
    eprintln!(
        "track-lvl: f{f} state {} motion {:#x} skel {} posed {} seated {seated} heading: rider {rider:.1} chao {chao:.1} diff {:.1} board x/z less rider {board:.1?}",
        m.r8(RIDER.get()), RIDER_MOTION.get().0, SKEL.with_borrow(|s| s.len()), POSED.with_borrow(|p| p.len()), wrap(rider - chao)
    );
}

thread_local! {
    /// The board's world x and z axis headings (degrees) and the frame it drew in.
    static BOARD_AXES: Cell<Option<(u64, f32, f32)>> = const { Cell::new(None) };
}

/// Self mode has no rider display to capture the rider's model from, so
/// the boots' mount (Shadow's rest foot, node 24) had no rest pose and fell
/// back to the hub's mirrored numbers (boot sideways, test build 9). Take the
/// character's body model from the model table instead: the first loaded slot
/// whose model has the rider skeleton's 62 nodes.
/// shortcut: "first 62-node model" stands in for the character's body slot
/// (not yet read from playerwk); in Chao World only the character's set is loaded.
fn ensure_rider_rest(m: &Mem) {
    if RIDER_REST.with_borrow(|r| r.is_some()) || sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) % 60 != 0 {
        return;
    }
    // Shadow's own body first: KNUCKMDL's body (slot 285) has 62 nodes too.
    let shadow: Vec<u32> = SHADOW_MODELS.with_borrow(|e| e.values().copied().collect());
    let found = shadow.into_iter().chain((0..0x428u32).map(|k| model_obj(m, k))).filter(|o| (0x8000_0000..0x8C00_0000).contains(o)).find(|&o| crate::chao_retarget::read_chao(&|a| m.r32(a), o, [vec![], vec![]]).nodes.len() == 62);
    let Some(obj) = found else { return };
    let rr = crate::chao_retarget::read_chao(&|a| m.r32(a), obj, [vec![], vec![]]).worlds(&[]);
    let r0 = rr.first().map(|x| x.0).unwrap_or([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
    let inv0 = crate::skate::inv(&r0);
    eprintln!("chao-player: rider rest pose from model {obj:#x} (62 nodes)");
    RIDER_OBJ.set(obj);
    RIDER_REST.set(Some((obj, rr.into_iter().map(|(rot, _)| crate::skate::mul(&inv0, &rot)).collect())));
}

/// MOTION_CTRL at chaowk +0xAC (research gsne8p-disc/chao/pseudo/motion-engine.md,
/// decomp motion.h): flag u16 +0, curr_num +4, minfo[0] at +0x18 = {u16
/// mode; f32 frame +4, start +8, end +0xC, spd +0x10}.
const CHAO_MOTION_CTRL: u32 = 0xAC;
thread_local! {
    /// The rider's last full-skeleton motion draw: (NJS_MOTION, frame).
    static RIDER_MOTION: Cell<(u32, f32)> = const { Cell::new((0, 0.0)) };
}

/// The hidden Sonic's ground states by his own animation number (his action
/// table rows), as seen in a speed ramp with his own physics row: 6 his walk
/// (from a stand), 7 his jog (from 0.34 a frame), 8 his run (from 1.27), 9
/// his fastest run (from 2.18), 10 above that.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Gait { Walk, Jog, Run }

pub(crate) fn gait(his_action: u32) -> Option<Gait> {
    match his_action {
        6 => Some(Gait::Walk),
        7 => Some(Gait::Jog),
        8..=10 => Some(Gait::Run),
        _ => None,
    }
}

impl Gait {
    /// The Chao's own motion under it, from the pair the garden itself moves a
    /// Chao with (ChaoMain 0x19F10, by its Run skill): 100 alm_aruki, its walk,
    /// in his walk; 101 alm_hasiru, its faster go, in his jog; and 119 alm_run
    /// under his run laid over it. SA2_GAIT_WALK / SA2_GAIT_JOG=n (debug): another motion.
    fn motion(self) -> u32 {
        let env = |k: &str, d: u32| std::env::var(k).ok().and_then(|v| v.trim().parse().ok()).unwrap_or(d);
        match self {
            Gait::Walk => env("SA2_GAIT_WALK", 100),
            Gait::Jog => env("SA2_GAIT_JOG", 101),
            Gait::Run => 119,
        }
    }
}

thread_local! {
    static GAIT: Cell<Option<Gait>> = const { Cell::new(None) };
    /// Frames in a row the body has not moved (the gait's stop).
    static STILL: Cell<u32> = const { Cell::new(0) };
}

// Playback: the Chao's own walk and go are kept on the hidden Sonic's own
// stride (stride_lock), one cycle of the Chao's for one of his, so it steps
// when he steps and its step rate rises with his speed as his does. Its feet
// cannot be kept on the ground it covers: one cycle of its run moves a foot
// 0.47 units in 30 frames (0.016 a frame, measured with SA2_GAIT_LOG), against
// 0.34 to 3 a frame at his speeds.

/// SA2_GAIT_LOG=1 (debug), a line a frame in Chao World: the hidden
/// character's animation state and forward speed, the ground covered, the
/// Chao's motion and frame, the overlay's share, and how far the Chao's
/// facing (the shared chao_yaw of his yaw, as it is drawn) is off the way it
/// actually travelled, in degrees.
fn gait_log(m: &Mem, chao: u32, data1: u32, moved: [f32; 2]) {
    if !std::env::var("SA2_GAIT_LOG").is_ok_and(|v| v == "1") {
        return;
    }
    let (cw, obj2) = (m.r32(chao + 0x34), m.r32(P1_OBJ2));
    if cw == 0 || !(0x8000_0000..0x8C00_0000).contains(&obj2) {
        return;
    }
    let f = |a: u32| f32::from_bits(m.r32(a));
    let his = m.r32(data1 + 0x0C) & 0xFFFF;
    let drawn = if data1 == cw && PMOVE_CHAO.get() == chao { chao_yaw(his) } else { m.r32(cw + 0x0C) } & 0xFFFF;
    let a = drawn as f32 * std::f32::consts::TAU / 65536.0;
    let d = (moved[0] * moved[0] + moved[1] * moved[1]).sqrt();
    // The Chao faces (sin, cos) of its yaw.
    let off = if d > 0.02 { (moved[0] * a.cos() - moved[1] * a.sin()).atan2(moved[0] * a.sin() + moved[1] * a.cos()).to_degrees() } else { 0.0 };
    let ctrl = cw + CHAO_MOTION_CTRL;
    let table = m.r32(obj2 + OBJ2_PLACTPTR);
    let act = u32::from(m.r16(obj2 + OBJ2_ACTION));
    // Each foot ahead of the body along its facing (last draw; view space is rigid).
    let p = CHAO_PLACE.get();
    let len = (0..3).map(|r| p[r][2] * p[r][2]).sum::<f32>().sqrt().max(1e-6);
    let feet = FEET.with_borrow(|x| x.map(|k| k.map(|k| (0..3).map(|r| (k[r][3] - p[r][3]) * p[r][2]).sum::<f32>() / len)));
    eprintln!("gait: f{} his action {act} (motion {}) speed {:.3} covered {d:.3} | Chao motion {} frame {:.1} of {:.0}..{:.0} link {} rate {:.2} | overlay {:.2} | his yaw {his:#06x} drawn {drawn:#06x} off travel {off:.1} | feet ahead {:.3} {:.3}",
        sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed), if (0x8000_0000..0x8C00_0000).contains(&table) { m.r16(table + 0x10 * act) } else { 0 }, f(obj2 + 0x64), LAST_MOTION.get(), f(ctrl + 0x1C), f(ctrl + 0x20), f(ctrl + 0x24), m.r16(ctrl) & 2 != 0, f(ctrl + 0x0C), skate_blend(false), feet[0].unwrap_or(f32::NAN), feet[1].unwrap_or(f32::NAN));
}

/// Where the Chao's stride frame goes so its cycle phase moves `blend` of
/// the way (the short way round) to the rider's phase; both phases 0..1.
pub(crate) fn locked_phase(chao: f32, rider: f32, blend: f32) -> f32 {
    let err = (rider - chao + 1.5).rem_euclid(1.0) - 0.5;
    (chao + err * blend).rem_euclid(1.0)
}

/// The Chao's own walk/run supplies the leg pivots under the feet that take
/// the rider's rotations: its cycle is kept on the rider's stride, as the hub
/// does (mmo-play src/shadow-skate.js apply(): run.goToFrame(locked)), or
/// the feet turn in his rhythm on legs that step in another.
/// Presentation rule from the hub, not game data: phase 0 of the Chao's
/// cycle is put on phase 0 of the rider's motion (unverified that both start
/// on the same foot).
fn stride_lock(m: &mut Mem, chao: u32) {
    // In his walk and jog (the Chao's own motion alone) the cycle is his too.
    let b = if GAIT.get().is_some_and(|g| g != Gait::Run) && !std::env::var("SA2_GAIT_LOCK").is_ok_and(|v| v == "0") { 1.0 } else { skate_blend(false) };
    let (cw, obj2) = (m.r32(chao + 0x34), m.r32(P1_OBJ2));
    if b <= 0.0 || OWN_MOTION.get() || cw == 0 || !(0x8000_0000..0x8C00_0000).contains(&obj2) || std::env::var("SA2_STRIDE_LOCK").is_ok_and(|v| v == "0") {
        return;
    }
    let f = |m: &Mem, a: u32| f32::from_bits(m.r32(a));
    // The rider's motion draw of the last frame: NJS_MOTION (nbFrame u32 +4)
    // and the frame it was drawn at.
    let (mot, nframe) = RIDER_MOTION.get();
    if !(0x8000_0000..0x8C00_0000).contains(&mot) {
        return;
    }
    let n = m.r32(mot + 4) as f32;
    let ctrl = cw + CHAO_MOTION_CTRL;
    // Not while the Chao's motion is blending to another (flag 0x2, link).
    // shortcut: a rider motion under 8 frames (his 2-frame stand, drawn for the
    // first frames of a move) has no stride to lock to; upgrade with his
    // action state if a short stride motion turns up.
    if n < 8.0 || !nframe.is_finite() || m.r16(ctrl) & 2 != 0 {
        return;
    }
    let (frame, start, end) = (f(m, ctrl + 0x1C), f(m, ctrl + 0x20), f(m, ctrl + 0x24));
    let len = end - start;
    if !(len > 0.0) || !frame.is_finite() {
        return;
    }
    // On Sonic's run the Chao's run cycle is put a tenth of a cycle ahead of his: on the
    // same phase its feet lifted when his did but went forward 6.5 of his 32 frames late
    // (measured, SA2_FOOT_TRACE); a tenth splits that between lift and swing.
    // SA2_STRIDE_PHASE=x (debug): another share of a cycle.
    let ahead = if GAIT.get() == Some(Gait::Run) { std::env::var("SA2_STRIDE_PHASE").ok().and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(0.1) } else { 0.0 };
    let to = start + locked_phase((frame - start) / len, nframe / n + ahead, b) * len;
    m.w32(ctrl + 0x1C, to.to_bits());
    if std::env::var("SA2_TRACK_LOG").is_ok_and(|v| v == "1") {
        eprintln!("stride: chao motion {} frame {frame:.2} -> {to:.2} of {start:.0}..{end:.0}, rider frame {nframe:.2} of {n:.0}, blend {b:.2}", m.r32(ctrl + 4));
    }
}

thread_local! {
    static TRACK_LINE: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// SA2_TRACK_LOG=1 (debug): one line per drawn frame for the skate-mapped
/// Chao nodes, in degrees: `node: s (the rider node's turn from its rest,
/// seen in the Chao's facing) / c (the drawn Chao node's turn from its rest)
/// / e (between the two) / o (between the drawn node and the Chao's own
/// motion before the overlay)`, with the blend and the Chao's motion.
fn track_log(m: &Mem, i: usize, own: &[[f32; 4]; 3]) {
    if !std::env::var("SA2_TRACK_LOG").is_ok_and(|v| v == "1") {
        return;
    }
    let r3 = |x: &[[f32; 4]; 3]| -> [[f32; 3]; 3] { std::array::from_fn(|a| std::array::from_fn(|b| x[a][b])) };
    if let Some(&(_, src, ..)) = crate::skate::MAP.iter().find(|e| e.0 == i) {
        let now = SKEL.with_borrow(|s| ((s.len() == 62 && skel_site("c5"))).then(|| r3(&inv_mul(&s[0], &s[src]))));
        let rest = RIDER_REST.with_borrow(|r| r.as_ref().and_then(|(_, v)| v.get(src).copied()));
        let chao_rest = CHAO_REST.with_borrow(|r| r.as_ref().and_then(|r| r.worlds(&[]).get(i).map(|x| x.0)));
        if let (Some(now), Some(rest), Some(chao_rest)) = (now, rest, chao_rest) {
            let p = CHAO_PLACE.get();
            let cur = mtx(m, m.r32(NJ_CURRENT_MATRIX));
            let (s, c, e) = crate::skate::track(&now, &rest, &r3(&inv_mul(&p, &cur)), &chao_rest);
            let o = crate::skate::angle_between(&r3(own), &r3(&cur));
            TRACK_LINE.with_borrow_mut(|l| l.push_str(&format!(" {i}: s{s:.0} c{c:.0} e{e:.0} o{o:.0}")));
        }
    }
    if i == 14 {
        let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
        // Headings in the world, degrees: the rider's root node as his display
        // drew it (he faces -x) and the Chao's placement (it faces +z).
        let base = mtx(m, m.r32(NJ_BASE_MATRIX));
        let place = inv_mul(&base, &CHAO_PLACE.get());
        let chao_yaw = place[0][2].atan2(place[2][2]).to_degrees();
        let rider_yaw = SKEL.with_borrow(|s| s.first().map(|r| (-r[0][0]).atan2(-r[2][0]).to_degrees()));
        let diff = rider_yaw.map(|r| (r - chao_yaw + 540.0).rem_euclid(360.0) - 180.0);
        TRACK_LINE.with_borrow_mut(|l| l.push_str(&format!(" heading: rider {rider_yaw:.1?} chao {chao_yaw:.1} diff {diff:.1?}")));
        eprintln!("track: f{f} blend {:.2} motion {}{}", skate_blend(false), LAST_MOTION.get(), TRACK_LINE.take());
    }
}

/// Owner's rule (mob 2026-11-24: "far too much twist in the feet/boots"): on the hidden
/// Sonic's walk and jog the Chao's own motion turns its feet to the side (the jog by 16
/// degrees either way) and rolls them (6 degrees). The foot keeps the motion's pitch and
/// its foot point; its side turn is eased to the run's ankle limit (SA2_FOOT_TWIST, 6
/// degrees) and its roll is taken out (skate::ankle_limit). The run's overlay starts from
/// this foot, so the fade between jog and run stays inside the limit too; at the whole
/// overlay the run's own foot replaces it as before. Returns the foot as now drawn.
/// MMO glue. SA2_FOOT_LIMIT=0 (debug): as the motion has it.
fn foot_limit(m: &mut Mem, i: usize, own: &[[f32; 4]; 3]) -> [[f32; 4]; 3] {
    let rest = CHAO_REST.with_borrow(|r| r.as_ref().and_then(|r| r.worlds(&[]).get(i).map(|x| x.0)));
    let (Some(rest), false) = (rest, std::env::var("SA2_FOOT_LIMIT").is_ok_and(|v| v == "0")) else { return *own };
    let r3 = |x: &[[f32; 4]; 3]| -> [[f32; 3]; 3] { std::array::from_fn(|a| std::array::from_fn(|b| x[a][b])) };
    let p = CHAO_PLACE.get();
    let local = r3(&inv_mul(&p, own));
    // Its turn from rest in the Chao's placement, and the turn that takes it to the limited one.
    let d = crate::skate::mul(&crate::skate::unit(&local), &crate::skate::inv(&crate::skate::unit(&rest)));
    let cap = std::env::var("SA2_FOOT_TWIST").ok().and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(6.0).to_radians();
    let fix = crate::skate::mul(&crate::skate::ankle_limit(&d, cap), &crate::skate::inv(&d));
    let turned = crate::skate::mul(&r3(&p), &crate::skate::mul(&fix, &local));
    let cur_ptr = m.r32(NJ_CURRENT_MATRIX);
    let mut out = *own;
    for row in 0..3 {
        for col in 0..3 {
            out[row][col] = turned[row][col];
            m.w32(cur_ptr + 4 * (4 * row + col) as u32, turned[row][col].to_bits());
        }
    }
    out
}

/// One skate-mapped node: rotation from skate::desired (Chao model space,
/// under the Chao's placement), position kept as the native pose drew it.
fn skate_node(m: &mut Mem, i: usize, src: usize, w: f32, cap: f32) {
    let r3 = |x: &[[f32; 4]; 3]| -> [[f32; 3]; 3] { std::array::from_fn(|a| std::array::from_fn(|b| x[a][b])) };
    let Some(now) = SKEL.with_borrow(|s| ((s.len() == 62 && skel_site("c6"))).then(|| r3(&inv_mul(&s[0], &s[src])))) else { return };
    let obj = RIDER_OBJ.get();
    if !(0x8000_0000..0x8C00_0000).contains(&obj) { return; }
    let rest = RIDER_REST.with_borrow_mut(|r| {
        if r.as_ref().map(|(o, _)| *o) != Some(obj) {
            let rr = crate::chao_retarget::read_chao(&|a| m.r32(a), obj, [vec![], vec![]]).worlds(&[]);
            // Relative to the rider's root node, like the live pose (SKEL[0]-relative).
            let r0 = rr.first().map(|x| x.0).unwrap_or([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
            let inv0 = crate::skate::inv(&r0);
            *r = Some((obj, rr.into_iter().map(|(rot, _)| crate::skate::mul(&inv0, &rot)).collect()));
        }
        r.as_ref().and_then(|(_, v)| v.get(src).copied())
    });
    let chao_rest = CHAO_REST.with_borrow(|r| r.as_ref().and_then(|r| r.worlds(&[]).get(i).map(|x| x.0)));
    let (Some(rest), Some(chao_rest)) = (rest, chao_rest) else { return };
    let d = crate::skate::desired(&now, &rest, &chao_rest, w, cap);
    if i == 14 && std::env::var_os("SA2_STATE_LOG").is_some() && sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) % 120 == 0 {
        let det = |x: &[[f32; 3]; 3]| x[0][0] * (x[1][1] * x[2][2] - x[1][2] * x[2][1]) - x[0][1] * (x[1][0] * x[2][2] - x[1][2] * x[2][0]) + x[0][2] * (x[1][0] * x[2][1] - x[1][1] * x[2][0]);
        eprintln!("head-row: his node {src} now det {:.3} rest det {:.3} chao rest det {:.3} desired det {:.3} now {now:?} rest {rest:?}", det(&now), det(&rest), det(&chao_rest), det(&d));
    }
    let p = CHAO_PLACE.get();
    let cur_ptr = m.r32(NJ_CURRENT_MATRIX);
    // Blended with the Chao's own walk pose while the skate comes in or goes out.
    let b = skate_blend(false);
    for row in 0..3 {
        for col in 0..3 {
            let v: f32 = (0..3).map(|k| p[row][k] * d[k][col]).sum();
            let a = cur_ptr + 4 * (4 * row + col) as u32;
            let own = f32::from_bits(m.r32(a));
            m.w32(a, (own + (v - own) * b).to_bits());
        }
    }
}

thread_local! {
    /// Per eased node (tail 7, 8; body 1; head 14): the frame, the rotation drawn and the motion's own rotation, last time.
    static TAIL: std::cell::RefCell<[Option<(u64, [[f32; 3]; 3], [[f32; 3]; 3])>; 4]> = const { std::cell::RefCell::new([None, None, None, None]) };
}

/// The player's Chao's tail (nodes 7 and 8; no overlay): its own motion's
/// turn is followed a quarter of the way a frame, in the Chao's placement.
/// The Chao's cycle is kept on the character's stride (stride_lock), which
/// plays its motion many times faster than the game does, and the tail
/// whipped with it (mob 2026-11-24: "the chaos tail goes wild"); at the
/// motion's own rate this lag is a few frames of a 30-frame swing.
/// MMO glue. SA2_TAIL_CALM=0 (debug): as the motion has it; SA2_TAIL_K=x:
/// the share a frame. SA2_FOOT_TRACE=1 logs each node's turn a frame.
fn tail_calm(m: &mut Mem, i: usize, slot: usize, own: &[[f32; 4]; 3], k: f32) {
    let p = CHAO_PLACE.get();
    let cur_ptr = m.r32(NJ_CURRENT_MATRIX);
    // The node as it stands now (its parent may already have been eased).
    let l = inv_mul(&p, &mtx(m, cur_ptr));
    let rot: [[f32; 3]; 3] = std::array::from_fn(|a| std::array::from_fn(|b| l[a][b]));
    // Its size is the rest model's (no stretch from a motion's scale keys).
    let rest = CHAO_REST.with_borrow(|r| r.as_ref().and_then(|r| r.worlds(&[]).get(i).map(|x| x.0)));
    let len: [f32; 3] = std::array::from_fn(|j| { let of = rest.as_ref().unwrap_or(&rot); (0..3).map(|r| of[r][j] * of[r][j]).sum::<f32>().sqrt().max(1e-6) });
    let own_len: [f32; 3] = std::array::from_fn(|j| (0..3).map(|r| rot[r][j] * rot[r][j]).sum::<f32>().sqrt());
    let unit = crate::skate::unit(&rot);
    let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
    let prev = TAIL.with_borrow(|t| t[slot]).filter(|x| f.wrapping_sub(x.0) <= 2);
    let k = k.clamp(0.0, 1.0);
    let on = !std::env::var("SA2_TAIL_CALM").is_ok_and(|v| v == "0");
    let drawn = match prev {
        Some((_, was, _)) if on => {
            // Part of the way from the last drawn turn to this one, made a rotation again.
            let mix: [[f32; 3]; 3] = std::array::from_fn(|a| std::array::from_fn(|b| was[a][b] + (unit[a][b] - was[a][b]) * k));
            let col = |j: usize| [mix[0][j], mix[1][j], mix[2][j]];
            let norm = |v: [f32; 3]| { let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6); [v[0] / n, v[1] / n, v[2] / n] };
            let x = norm(col(0));
            let y0 = col(1);
            let d = x[0] * y0[0] + x[1] * y0[1] + x[2] * y0[2];
            let y = norm([y0[0] - d * x[0], y0[1] - d * x[1], y0[2] - d * x[2]]);
            let z = [x[1] * y[2] - x[2] * y[1], x[2] * y[0] - x[0] * y[2], x[0] * y[1] - x[1] * y[0]];
            std::array::from_fn(|a| [x[a], y[a], z[a]])
        }
        _ => unit,
    };
    if on {
        for row in 0..3 {
            for c in 0..3 {
                let v: f32 = (0..3).map(|q| p[row][q] * drawn[q][c] * len[c]).sum();
                m.w32(cur_ptr + 4 * (4 * row + c) as u32, v.to_bits());
            }
        }
    }
    let o = inv_mul(&p, own);
    let own_u = crate::skate::unit(&std::array::from_fn(|a| std::array::from_fn(|b| o[a][b])));
    if std::env::var("SA2_FOOT_TRACE").is_ok_and(|v| v == "1") {
        if let Some((_, was, own_was)) = prev {
            eprintln!("tail-trace: f{f} node {i} chao motion {} own step {:.2} drawn step {:.2} own size {:.3} {:.3} {:.3} rest size {:.3} {:.3} {:.3}", LAST_MOTION.get(), crate::skate::angle_between(&own_u, &own_was), crate::skate::angle_between(&drawn, &was), own_len[0], own_len[1], own_len[2], len[0], len[1], len[2]);
        }
    }
    TAIL.with_borrow_mut(|t| t[slot] = Some((f, drawn, own_u)));
}

thread_local! {
    /// The body node (1) of this draw: as the Chao's own motion has it, and as drawn (overlay, easing).
    /// And where the feet take the body to be (view space): its place eased as it was when the feet were approved.
    static BODY: Cell<Option<([[f32; 4]; 3], [[f32; 4]; 3], [f32; 3])>> = const { Cell::new(None) };
}

thread_local! {
    /// The body node's place in the Chao's placement as last drawn, and the frame (bob_calm).
    static BOB: Cell<Option<(u64, [f32; 3], [f32; 3])>> = const { Cell::new(None) };
    /// The frame and the running mean of the hidden character's body node height in his root's space (the run's bounce).
    static HIS_BODY: Cell<Option<(u64, f32)>> = const { Cell::new(None) };
    static BOB_SAID: Cell<bool> = const { Cell::new(false) };
}

/// The player's Chao on the hidden Sonic's stride (walk, jog, run without the
/// Air Shoes): its own motion is played at his step rate, several times its
/// own, and the whole Chao bounced at that rate, its head more than his
/// (mob 2026-11-24: "the chaos head bounces as well"; measured up and down
/// 0.20 / 0.36 / 0.41 Chao units in walk / jog / run against his head's 0.07
/// / 0.10 / 0.25 at the Chao's size). The body node (1), which every other
/// part hangs from, is followed part of the way a frame in place, and in his
/// walk and jog in turn as well (tail_calm's easing), so the head, arms, feet
/// and shoes keep their place on the Chao while the bounce is eased. MMO glue.
/// SA2_BOB_CALM=0 (debug): as the motion has it. Returns where the feet take
/// the body to be (view space).
fn bob_calm(m: &mut Mem) -> [f32; 3] {
    let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
    if std::env::var("SA2_FOOT_TRACE").is_ok_and(|v| v == "1") && !BOB_SAID.replace(true) {
        let parents = CHAO_REST.with_borrow(|r| r.as_ref().map(|r| r.nodes.iter().map(|n| n.0).collect::<Vec<_>>()));
        eprintln!("bob: Chao node parents {parents:?}");
    }
    let on = GAIT.get().is_some() && !std::env::var("SA2_BOB_CALM").is_ok_and(|v| v == "0");
    let p = CHAO_PLACE.get();
    let cur_ptr = m.r32(NJ_CURRENT_MATRIX);
    let l = inv_mul(&p, &mtx(m, cur_ptr));
    let now = [l[0][3], l[1][3], l[2][3]];
    let prev = BOB.get().filter(|x| f.wrapping_sub(x.0) <= 2);
    // Two followers of the body's place: the body and all on it by the owner's head
    // smoothing; the feet by the share they were approved with (SA2_BOB_K, 0.2).
    let k_feet = std::env::var("SA2_BOB_K").ok().and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(0.2).clamp(0.0, 1.0);
    let k = head_smooth();
    let (drawn, feet): ([f32; 3], [f32; 3]) = match prev {
        Some((_, was, was_feet)) if on => (std::array::from_fn(|a| was[a] + (now[a] - was[a]) * k), std::array::from_fn(|a| was_feet[a] + (now[a] - was_feet[a]) * k_feet)),
        _ => (now, now),
    };
    let view = |v: [f32; 3]| -> [f32; 3] { std::array::from_fn(|row| (0..3).map(|q| p[row][q] * v[q]).sum::<f32>() + p[row][3]) };
    // Owner's rule (mob 2026-11-24; not native Chao behaviour): "a tiny amount of bounce of
    // the chao's body in each step while it is running". In his run only, the body rises and
    // falls with his own body node (4) about its running mean, at the Chao's size (the leg
    // ratio, 0.28) and a share of it: SA2_BODY_BOUNCE (0.2; 0 = none). Measured on the test
    // copy: the body travels 0.116 Chao units peak to peak with it (0.4 gave 0.18; his own
    // is 0.277 at the Chao's size). Added after the easing, so the head only rides it; the
    // feet keep their own track of the body's place.
    let share = std::env::var("SA2_BODY_BOUNCE").ok().and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(0.2);
    let his = SKEL.with_borrow(|s| (s.len() == 62).then(|| inv_mul(&s[0], &s[4])[1][3]));
    let bounce = match (his, on && GAIT.get() == Some(Gait::Run)) {
        (Some(y), true) => {
            let mean = HIS_BODY.get().filter(|x| f.wrapping_sub(x.0) <= 2).map_or(y, |x| x.1 + (y - x.1) * 0.05);
            HIS_BODY.set(Some((f, mean)));
            (y - mean) * 0.28 * share * skate_blend(false)
        }
        _ => 0.0,
    };
    let shown = [drawn[0], drawn[1] + bounce, drawn[2]];
    if on {
        let v = view(shown);
        for row in 0..3 {
            m.w32(cur_ptr + 4 * (4 * row + 3) as u32, v[row].to_bits());
        }
    }
    if std::env::var("SA2_FOOT_TRACE").is_ok_and(|v| v == "1") {
        eprintln!("bob-trace: f{f} chao motion {} own {now:?} drawn {shown:?} feet {feet:?} his body {:?} bounce {bounce:.4}", LAST_MOTION.get(), his.map(|y| y * 0.28));
    }
    BOB.set(Some((f, drawn, feet)));
    view(feet)
}

/// Owner's rule (mob 2026-11-24: the Chao's head "bounces too much", "a lot
/// smoother"): the share a frame by which the body's place and turn and the
/// head's turn follow the Chao's own motion and the overlay while it goes on
/// the hidden Sonic's stride. SA2_HEAD_SMOOTH=x (0.05; 1 = no smoothing).
fn head_smooth() -> f32 {
    std::env::var("SA2_HEAD_SMOOTH").ok().and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(0.05).clamp(0.0, 1.0)
}

/// SA2_FOOT_TRACE=1 (debug), a line a frame at the Chao's body (1) and head (14)
/// nodes: where the node is in the Chao's placement as its own motion has it
/// and as drawn, the rider's node (body 4, head 44) in his root node's space,
/// and each one's turn from rest in degrees (rider / Chao / between), to
/// compare the Chao's head and body bob with his.
fn head_trace(m: &Mem, i: usize, own: &[[f32; 4]; 3]) {
    if !(i == 1 || i == 14) || !std::env::var("SA2_FOOT_TRACE").is_ok_and(|v| v == "1") {
        return;
    }
    let src = if i == 1 { 4 } else { 44 };
    let Some(now) = SKEL.with_borrow(|s| ((s.len() == 62 && skel_site("c7"))).then(|| inv_mul(&s[0], &s[src]))) else { return };
    let p = CHAO_PLACE.get();
    let (o, c) = (inv_mul(&p, own), inv_mul(&p, &mtx(m, m.r32(NJ_CURRENT_MATRIX))));
    let r3 = |x: &[[f32; 4]; 3]| -> [[f32; 3]; 3] { std::array::from_fn(|a| std::array::from_fn(|b| x[a][b])) };
    let rest_r = RIDER_REST.with_borrow(|r| r.as_ref().and_then(|(_, v)| v.get(src).copied()));
    let chao_r = CHAO_REST.with_borrow(|r| r.as_ref().and_then(|r| r.worlds(&[]).get(i).map(|x| x.0)));
    let (t, t_own) = match (rest_r, chao_r) {
        (Some(a), Some(b)) => (crate::skate::track(&r3(&now), &a, &r3(&c), &b), crate::skate::track(&r3(&now), &a, &r3(&o), &b).1),
        _ => ((f32::NAN, f32::NAN, f32::NAN), f32::NAN),
    };
    let (mot, mf) = RIDER_MOTION.get();
    eprintln!("head-trace: f{} node {i} motion {mot:#x} frame {mf:.2} blend {:.2} chao motion {} | his {:?} | own {:?} | drawn {:?} | turn his {:.1} chao {:.1} between {:.1} own {:.1}",
        sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed), skate_blend(false), LAST_MOTION.get(),
        [now[0][3], now[1][3], now[2][3]], [o[0][3], o[1][3], o[2][3]], [c[0][3], c[1][3], c[2][3]], t.0, t.1, t.2, t_own);
}

thread_local! {
    /// The rider model's rest node positions in his root node's space (foot_place).
    static RIDER_REST_POS: std::cell::RefCell<Option<(u32, Vec<[f32; 3]>)>> = const { std::cell::RefCell::new(None) };
    static FOOT_SAID: Cell<u32> = const { Cell::new(0) };
}

/// A foot node (6, 13) of the overlay, after the rider's foot turn
/// (skate_node). On Sonic's run that turn is replaced by its pitch alone
/// (skate::foot_pitch); the foot node is never moved, so the shoe's top stays
/// on the Chao as it does for Shadow's Air Shoes, and the shoes and their
/// jets, drawn from the foot matrix, stay rigid on it. (Two earlier tries
/// were taken out again, mob 2026-11-24: moving the foot to where his foot
/// is took the boot tops off the body; leaning the foot toward it twisted
/// the shoes.) SA2_FOOT_TRACE=1: a line a foot a frame, with where his foot
/// would put the Chao's (skate::foot_target), for comparison only.
/// Of a turn in the Chao's axes (it faces +z, y up): how far it turns the forward
/// axis to the side (degrees, about y) and how far it rolls the side axis (about z).
fn side_roll(d: &[[f32; 3]; 3]) -> (f32, f32) {
    (d[0][2].atan2(d[2][2]).to_degrees(), d[1][0].atan2(d[0][0]).to_degrees())
}

fn foot_place(m: &mut Mem, i: usize, src: usize, own: &[[f32; 4]; 3], blend: f32) {
    let obj = RIDER_OBJ.get();
    if !(0x8000_0000..0x8C00_0000).contains(&obj) { return; }
    let Some(now) = SKEL.with_borrow(|s| ((s.len() == 62 && skel_site("c8"))).then(|| inv_mul(&s[0], &s[src]))) else { return };
    let now_p = [now[0][3], now[1][3], now[2][3]];
    // His hip joint above the foot (the leg's first node: 17 over heel 23, 7 over heel 13).
    let hip = if src == 23 { 17 } else { 7 };
    let rest = RIDER_REST_POS.with_borrow_mut(|r| {
        if r.as_ref().map(|(o, _)| *o) != Some(obj) {
            let w = crate::chao_retarget::read_chao(&|a| m.r32(a), obj, [vec![], vec![]]).worlds(&[]);
            let (r0, p0) = w.first().copied().unwrap_or(([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], [0.0; 3]));
            let inv0 = crate::skate::inv(&r0);
            *r = Some((obj, w.iter().map(|(_, p)| { let d = [p[0] - p0[0], p[1] - p0[1], p[2] - p0[2]]; std::array::from_fn(|a| (0..3).map(|k| inv0[a][k] * d[k]).sum()) }).collect()));
        }
        r.as_ref().and_then(|(_, v)| Some((*v.get(src)?, *v.get(hip)?)))
    });
    // The Chao's foot node sits at the top of its leg (its turning point) and the foot mesh
    // hangs under it: what follows his heel is the foot itself (the mesh's middle, where the
    // shoe is), so the node goes where its turn leaves the foot on the target.
    let chao = CHAO_REST.with_borrow(|r| r.as_ref().and_then(|r| {
        let (rot, at) = *r.worlds(&[]).get(i)?;
        let v = r.verts.get(i)?;
        let n = v.len().max(1) as f32;
        let mid: [f32; 3] = std::array::from_fn(|k| v.iter().map(|p| p[k]).sum::<f32>() / n);
        Some((at, rot, mid))
    }));
    let (Some((rest_p, hip_p)), Some((chao_p, chao_r, mid))) = (rest, chao) else { return };
    let mv = |r: &[[f32; 3]; 3], v: [f32; 3]| -> [f32; 3] { std::array::from_fn(|a| (0..3).map(|k| r[a][k] * v[k]).sum()) };
    let lever_rest = mv(&chao_r, mid);
    let foot_rest: [f32; 3] = std::array::from_fn(|k| chao_p[k] + lever_rest[k]);
    // Leg lengths at rest: his hip joint down to his heel node, the Chao's foot node down to its foot's middle.
    let his_leg = (hip_p[1] - rest_p[1]).abs().max(1e-3);
    let chao_leg = lever_rest.iter().map(|x| x * x).sum::<f32>().sqrt();
    let scale = std::env::var("SA2_FOOT_SCALE").ok().and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(chao_leg / his_leg);
    let target = crate::skate::foot_target(now_p, rest_p, foot_rest, scale);
    let p = CHAO_PLACE.get();
    let r3 = |x: &[[f32; 4]; 3]| -> [[f32; 3]; 3] { std::array::from_fn(|a| std::array::from_fn(|b| x[a][b])) };
    let own_l = inv_mul(&p, own);
    // The foot's middle as the Chao's own motion has it, and the way to it after his turn.
    let own_foot: [f32; 3] = { let l = mv(&r3(&own_l), mid); std::array::from_fn(|k| own_l[k][3] + l[k]) };
    // The foot node never leaves the Chao's own foot point (mob 2026-11-24: "dont let the
    // top of the shoe come off the point of the chao"), for both pairs of shoes, and on
    // Sonic's run the foot is not twisted: of his foot's turn only the pitch (toe up or
    // down along the way it goes) is taken, eased toward SA2_FOOT_PITCH degrees at most
    // (35); its turn to the side and its roll are left out, so the shoe never turns
    // sideways or rolls under the body. SA2_FOOT_POS=0 (debug): his whole turn, as before.
    // On Shadow's skate (the Air Shoes, or Shadow himself as the character) the foot takes
    // his whole turn, as it always did.
    let obj2 = m.r32(P1_OBJ2);
    let sonic_run = (0x8000_0000..0x8C00_0000).contains(&obj2) && m.r8(obj2 + 1) == 0 && !shadow_style();
    let on = sonic_run && !std::env::var("SA2_FOOT_POS").is_ok_and(|v| v == "0");
    let cap = std::env::var("SA2_FOOT_PITCH").ok().and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(35.0).to_radians();
    let rest_r = RIDER_REST.with_borrow(|r| r.as_ref().and_then(|(_, v)| v.get(src).copied()));
    let pitch = rest_r.map(|rest| crate::skate::foot_pitch(&r3(&now), &rest, cap));
    // Owner's rule (mob 2026-11-24): "bring the back foot further back while keeping it on
    // the chao, like 10-25 degrees". While his foot trails (his heel behind its rest place;
    // he faces -x), the shoe is swung back about the Chao's foot point by 10 degrees when
    // it is just behind, up to SA2_FOOT_BACK (25) at his full back reach (0.8 of his leg),
    // in the plane it travels in only, eased in over the first seventh of that reach.
    let back_max = std::env::var("SA2_FOOT_BACK").ok().and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(25.0);
    let back = crate::skate::foot_back((now_p[0] - rest_p[0]) / (0.8 * his_leg), back_max).to_radians();
    // Owner's rule (mob 2026-11-24): "bring the foot forward like 5 more degrees on the
    // front step". While his foot leads (his heel ahead of its rest place), the shoe is
    // swung forward about the Chao's foot point, up to SA2_FOOT_FWD degrees (5) at his full
    // forward reach, by the same easing as the back swing.
    let fwd_max = std::env::var("SA2_FOOT_FWD").ok().and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(5.0);
    let fwd = crate::skate::foot_back((rest_p[0] - now_p[0]) / (0.8 * his_leg), fwd_max).to_radians();
    // All are one turn about the Chao's side axis (toe down = sole back); together at most 60 degrees either way.
    let pitch = pitch.map(|p| (p + back - fwd).clamp(-60f32.to_radians(), 60f32.to_radians()));
    // The feet hang from the body node, and on his run the body takes his torso's turn
    // (up to 24 degrees, with its twist and roll): the foot points swung round with it,
    // one forward and one back, one up and one down, in his rhythm (mob 2026-11-24:
    // "ur twisting the feet side to side"). On Sonic's run the foot point is put back
    // where the Chao's own motion has it under the body: the body's drawn turn is
    // taken out of the foot's place, its drawn position kept. SA2_FOOT_SWAY=1 (debug):
    // left as the body carries it.
    let body = BODY.get();
    let sway = std::env::var("SA2_FOOT_SWAY").is_ok_and(|v| v == "1");
    if let (true, false, Some((body_own, body_drawn, feet_at))) = (on, sway, body) {
        let cur_ptr = m.r32(NJ_CURRENT_MATRIX);
        let cur = mtx(m, cur_ptr);
        let back = crate::skate::mul(&r3(&body_own), &crate::skate::inv(&r3(&body_drawn)));
        let d: [f32; 3] = std::array::from_fn(|k| cur[k][3] - body_drawn[k][3]);
        for row in 0..3 {
            // Under the body's place as the feet follow it (the head's smoothing does not move them).
            let v: f32 = (0..3).map(|k| back[row][k] * d[k]).sum::<f32>() + feet_at[row];
            m.w32(cur_ptr + 4 * (4 * row + 3) as u32, (cur[row][3] + (v - cur[row][3]) * blend).to_bits());
        }
    }
    // Owner's rule (mob 2026-11-24): "give it a tiny twist in the ankle to have it slightly
    // more real". The side turn of his own foot in his run, eased to SA2_FOOT_TWIST degrees
    // at most (6; 0 = none), about the Chao's up axis at its foot point. No roll.
    let twist_max = std::env::var("SA2_FOOT_TWIST").ok().and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(6.0).to_radians();
    let twist = rest_r.map(|rest| crate::skate::foot_twist(&r3(&now), &rest, twist_max)).unwrap_or(0.0);
    if let (true, Some(pitch)) = (on, pitch) {
        let cur_ptr = m.r32(NJ_CURRENT_MATRIX);
        let (sn, cs) = pitch.sin_cos();
        let (st, ct) = twist.sin_cos();
        let turn = crate::skate::mul(&crate::skate::mul(&[[ct, 0.0, st], [0.0, 1.0, 0.0], [-st, 0.0, ct]], &[[1.0, 0.0, 0.0], [0.0, cs, -sn], [0.0, sn, cs]]), &chao_r);
        for row in 0..3 {
            for col in 0..3 {
                let v: f32 = (0..3).map(|k| p[row][k] * turn[k][col]).sum();
                let own_v = own[row][col];
                m.w32(cur_ptr + 4 * (4 * row + col) as u32, (own_v + (v - own_v) * blend).to_bits());
            }
        }
    }
    if std::env::var("SA2_FOOT_TRACE").is_ok_and(|v| v == "1") {
        let said = FOOT_SAID.get();
        if said & (1 << (i & 31)) == 0 {
            FOOT_SAID.set(said | 1 << (i & 31));
            eprintln!("foot-rest: Chao node {i} rest {chao_p:?} foot middle {foot_rest:?} leg {chao_leg:.4} | rider node {src} rest {rest_p:?} hip node {hip} {hip_p:?} leg {his_leg:.4} | leg scale {scale:.4}");
        }
        let chao_r = Some(chao_r);
        let cur = mtx(m, m.r32(NJ_CURRENT_MATRIX));
        // The foot's middle from the matrix as it now stands (what the foot and its shoe are drawn with).
        let drawn_foot: [f32; 3] = { let c = inv_mul(&p, &cur); let l = mv(&r3(&c), mid); std::array::from_fn(|k| c[k][3] + l[k]) };
        let e = match (rest_r, chao_r) { (Some(a), Some(b)) => crate::skate::track(&r3(&now), &a, &r3(&inv_mul(&p, &cur)), &b).2, _ => f32::NAN };
        let (mot, mf) = RIDER_MOTION.get();
        eprintln!("foot-trace: f{} node {i} motion {mot:#x} frame {mf:.2} blend {blend:.2} | his {now_p:?} | own {own_foot:?} | target {target:?} | drawn {drawn_foot:?} | rot {e:.2} | node moved {:.4} | pitch {:.1} back {:.1} fwd {:.1} twist {:.2} | foot turn from rest: side {:.1} roll {:.1} | node at {:?} | body turn added: side {:.1} roll {:.1}",
            sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed),
            (0..3).map(|k| { let c = inv_mul(&p, &cur); (c[k][3] - own_l[k][3]).powi(2) }).sum::<f32>().sqrt(), pitch.unwrap_or(f32::NAN).to_degrees(), back.to_degrees(), fwd.to_degrees(), twist.to_degrees(),
            side_roll(&crate::skate::mul(&crate::skate::unit(&r3(&inv_mul(&p, &cur))), &crate::skate::inv(&crate::skate::unit(&chao_r.unwrap())))).0,
            side_roll(&crate::skate::mul(&crate::skate::unit(&r3(&inv_mul(&p, &cur))), &crate::skate::inv(&crate::skate::unit(&chao_r.unwrap())))).1,
            { let c = inv_mul(&p, &cur); [c[0][3], c[1][3], c[2][3]] },
            body.map(|(o, d, _)| side_roll(&crate::skate::mul(&crate::skate::unit(&r3(&inv_mul(&p, &d))), &crate::skate::inv(&crate::skate::unit(&r3(&inv_mul(&p, &o))))))).unwrap_or((f32::NAN, f32::NAN)).0,
            body.map(|(o, d, _)| side_roll(&crate::skate::mul(&crate::skate::unit(&r3(&inv_mul(&p, &d))), &crate::skate::inv(&crate::skate::unit(&r3(&inv_mul(&p, &o))))))).unwrap_or((f32::NAN, f32::NAN)).1);
    }
}

/// How far the skate is blended in (0 = the Chao's own walk, 1 = skating),
/// the hub's rule (src/shadow-skate.js SkateCadence): in while it runs
/// (the hub's skate speed band; here the game's own walk/run switch), out
/// when it slows, over the hub's 0.42 s in and 0.28 s out (smoothstep).
/// `step`: advance one frame (once per draw, at node 0).
fn skate_blend(step: bool) -> f32 {
    if LEVEL_RIG.get() {
        // Out while the Chao plays its own motion (Knuckles's punch, glide, climb).
        return knux_rig::overlay(step);
    }
    // Swimming or in the air in Chao World the Chao plays its own motion: no overlay, no jets.
    if SWIM.get().is_some() || AIR.get() {
        SKATE_PROGRESS.set(0.0);
        return 0.0;
    }
    if step {
        // While it moves at all (walk 578 or run 119): the hidden character's
        // skeleton is in its walk, run or skate then, and the Chao's body,
        // head, arms and feet follow it. (Only from the run on, the Chao's own
        // walk played for the first half second of every move and followed
        // nothing: test copy run t1, e up to 100 degrees at blend 0.)
        // SA2_SKATE_RUN_ONLY=1 (debug): only while it runs, as before.
        let run_only = std::env::var("SA2_SKATE_RUN_ONLY").is_ok_and(|v| v == "1");
        let target = match GAIT.get() {
            // His walk and jog: the Chao's own motion alone.
            Some(g) => if g == Gait::Run { 1.0f32 } else { 0.0 },
            None => if MOVING.get() && (LAST_MOTION.get() == 119 || (!run_only && LAST_MOTION.get() == 578)) { 1.0f32 } else { 0.0 },
        };
        let p = SKATE_PROGRESS.get();
        let dt = if target > p { 1.0 / (0.42 * 60.0) } else { 1.0 / (0.28 * 60.0) };
        SKATE_PROGRESS.set(p + (target - p).clamp(-dt, dt));
    }
    let p = SKATE_PROGRESS.get();
    p * p * (3.0 - 2.0 * p)
}

thread_local! {
    static SKATE_PROGRESS: Cell<f32> = const { Cell::new(0.0) };
}

thread_local! {
    static RIDER_OBJ: Cell<u32> = const { Cell::new(0) };
    /// The foot node object (6) the native draw walked this frame: the drawn
    /// (adult) foot, whose mesh may differ in size from the rest model's.
    static DRAWN_FOOT: Cell<u32> = const { Cell::new(0) };
    static RIDER_REST: std::cell::RefCell<Option<(u32, Vec<[[f32; 3]; 3]>)>> = const { std::cell::RefCell::new(None) };
}

/// Gear debugger try-on (only with the ?geartune=1 panel): each planned
/// character item (shared/upgrade-catalog.json wearable models, PC model id n
/// = GC table slot 2n+1) drawn on the Chao at a node origin at half size, for
/// the panel to move into place ("Copy values" gives the numbers to commit).
/// Sonic's items need SONICMDL/SONICTEX, loaded once into heap 1 here.
/// (panel key, file, model slot, Chao node: 1 body, 16 head, 3/10 arms, 6/13 feet)
const TRY_ON: [(&str, u8, u32, usize, [f32; 7]); 7] = [
    ("sunglasses", b'K', 323, 16, [0.0, -0.35, -0.38, 0.0, -90.0, -180.0, 1.5]),  // KNUCKMDL 161, set in the tuner
    ("necklace", b'K', 337, 1, [0.0, -0.26, 1.5, 0.0, 90.0, 0.0, 1.35]),   // KNUCKMDL 168, set in the tuner
    ("sflame", b'T', 201, 16, CROWN_TUNE),                                     // TERIOSMDL 100, on the head as a crown
    // Sonic's Light Shoes: placed as the Air Shoes are (light_shoe_place), the tuner's line on top.
    ("lshoe0", b'S', 27, 6, TUNE_NONE),                                          // SONICMDL 13 (right)
    ("lshoe1", b'S', 29, 13, TUNE_NONE),                                         // SONICMDL 14 (left)
    ("ring", b'S', 51, 16, CROWN_TUNE),                                         // SONICMDL 25 Flame Ring, on the head as a crown
    ("bracelet", b'S', 53, 3, [0.78, -0.68, -0.06, 170.0, 90.0, 0.0, 1.09]),     // SONICMDL 26, set in the tuner
];
const TUNE_NONE: [f32; 7] = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0];
/// How far the Flame Ring crown sits above the head node (tuner dy, the head's
/// own up): mob asked for 0.5 to 1 higher, this is the top of it. Change this one number
/// (and the panel's start value in sa2-web/www/main.js) to move both rings.
const CROWN_UP: f32 = 1.0;
const CROWN_TUNE: [f32; 7] = [0.0, CROWN_UP, 0.0, 0.0, 0.0, 0.0, 1.0];
/// The hub shoes' set size from the tuner (the fit as it is).
const BOOT_TUNE: [f32; 7] = TUNE_NONE;

fn try_on(c: &mut Cpu, m: &mut Mem) {
    emerald_orbit::draw(c, m);
    if debug_set("tryon").as_deref() != Some("1") {
        return;
    }
    if SONIC_TEX.get().is_none() {
        let heap_var = c.r[13].wrapping_sub(SY_HEAP_SDA);
        let keep_heap = m.r32(heap_var);
        m.w32(heap_var, GEAR_HEAP);
        write_str(m, b"SONICTEX");
        let tex = call(c, m, LOAD_TEX, &[STR]);
        write_str(m, b"SONICMDL.PRS");
        let mdl = load_models(c, m, STR);
        SONIC_MODELS.set(LAST_LOAD.with_borrow(|l| l.clone()));
        m.w32(heap_var, keep_heap);
        eprintln!("chao-player try-on: Sonic's textures {tex:#x}, models {mdl:#x} ({} slots)", SONIC_MODELS.with_borrow(|s| s.len()));
        SONIC_TEX.set(Some(tex));
    }
    let cur_ptr = m.r32(NJ_CURRENT_MATRIX);
    let keep_m = mtx(m, cur_ptr);
    let list = m.r32(c.r[13].wrapping_sub(0x7228)) + 0x20;
    let keep_list = m.r32(list);
    let regs = (c.r, c.f);
    let lcap = LCAP.replace(false);
    for &(key, file, slot, node, set) in &TRY_ON {
        if slot == 0 || debug_off(key) {
            continue;
        }
        let (obj, tex) = match file {
            b'K' => (knux_obj(m, slot), KNUX_TEX.get()),
            b'T' => (shadow_obj(m, slot), SHADOW_TEX.get()),
            // Sonic's file loaded by this module, or, when he is the stage's own
            // character (the load then fills no slot), his models in the game's table.
            _ => (SONIC_MODELS.with_borrow(|s| if s.is_empty() { Some(model_obj(m, slot)) } else { s.get(&slot).copied() }).unwrap_or(0), SONIC_TEX.get()),
        };
        // The Light Shoes come from their own shoe file, not a model slot.
        let obj = if key.starts_with("lshoe") { 0x8000_0000 } else { obj };
        let Some(at) = NODE_MATS.with_borrow(|n| n.get(&node).copied()) else { continue };
        let (Some(tex), true) = (tex.filter(|&t| t != 0), (0x8000_0000..0x8C00_0000).contains(&obj)) else { continue };
        let half: [[f32; 4]; 3] = std::array::from_fn(|i| std::array::from_fn(|j| if i == j { 0.5 } else { 0.0 }));
        // The Light Shoes sit on the feet as the Air Shoes do; without that
        // place (no foot drawn yet) they are not drawn.
        let (obj, place) = if key.starts_with("lshoe") {
            let Some((o, p)) = light_shoe_place(c, m, node) else { continue };
            (o, p)
        } else {
            (obj, mul34(&at, &half))
        };
        let place = mul34(&place, &gear_tune(key).unwrap_or_else(|| tune_matrix(&set)));
        for r in 0..3 { for q in 0..4 { m.w32(cur_ptr + 4 * (4 * r + q) as u32, place[r][q].to_bits()); } }
        m.w32(list, tex);
        call(c, m, 0x8011_DF44, &[obj]);
    }
    LCAP.set(lcap);
    (c.r, c.f) = regs;
    m.w32(list, keep_list);
    for r in 0..3 { for q in 0..4 { m.w32(cur_ptr + 4 * (4 * r + q) as u32, keep_m[r][q].to_bits()); } }
}

/// Sonic's Light Shoes on Chao foot `node` (6 or 13), made and drawn the way
/// the Air Shoes are: one rigid shoe file (his own right heel/toe mesh from
/// his body model with the Light Shoes item on it, scripts/convert-light-shoes.py
/// then scripts/convert-hub-shoe-to-ninja.py, read from the site's
/// /mmo-assets/sonic-light-shoes-right.njsh; SA2_LIGHT_SHOE overrides), through
/// the same fit (boot_place) at the Air Shoes' set size (BOOT_TUNE), mirrored
/// in x on node 13. Returns the shoe object and its view-space place; None
/// without the file. (Before: the item alone, a heel piece with no shoe.)
fn light_shoe_place(c: &mut Cpu, m: &mut Mem, node: usize) -> Option<(u32, [[f32; 4]; 3])> {
    let n = SHOES.iter().position(|s| s.0 == node)?;
    let path = std::env::var("SA2_LIGHT_SHOE").unwrap_or_else(|_| "/mmo-assets/sonic-light-shoes-right.njsh".into());
    let obj = shoe_file(c, m, &LIGHT_SHOE, &LIGHT_SHOE_VERTS, &path);
    if obj == 0 {
        return None;
    }
    let verts = LIGHT_SHOE_VERTS.with_borrow(|v| v.clone());
    let x = [LIGHT_UP[1] * LIGHT_FWD[2] - LIGHT_UP[2] * LIGHT_FWD[1], LIGHT_UP[2] * LIGHT_FWD[0] - LIGHT_UP[0] * LIGHT_FWD[2], LIGHT_UP[0] * LIGHT_FWD[1] - LIGHT_UP[1] * LIGHT_FWD[0]];
    let place = boot_place(m, n, node, &verts, Some([x, LIGHT_UP, LIGHT_FWD]))?;
    Some((obj, mul34(&place, &tune_matrix(&BOOT_TUNE))))
}

/// The Light Shoe file's own up and toe directions, as BOOT_UP / BOOT_FWD are
/// the Air Shoe file's: up is the export's exportedPropStandingUp for Sonic's
/// right heel (-0.8284783, -0.5575026, -0.0530512) with x negated (the file
/// is mirrored back in x); forward is up turned a quarter in the file's xy
/// plane, which is how the Air Shoes' pair relate.
/// unverified: that rule for forward on Sonic's shoe (seen on the foot, not measured).
const LIGHT_UP: [f32; 3] = [0.828_478_3, -0.557_502_6, -0.053_051_2];
const LIGHT_FWD: [f32; 3] = [-0.558_288_4, -0.829_646_1, 0.0];

thread_local! {
    static LIGHT_SHOE: Cell<Option<u32>> = const { Cell::new(None) };
    static LIGHT_SHOE_VERTS: std::cell::RefCell<Vec<[f32; 3]>> = const { std::cell::RefCell::new(Vec::new()) };
}


thread_local! {
    static SONIC_TEX: Cell<Option<u32>> = const { Cell::new(None) };
    static SONIC_MODELS: std::cell::RefCell<std::collections::BTreeMap<u32, u32>> = Default::default();
    /// This draw's view-space matrices of the Chao nodes the try-on uses.
    static NODE_MATS: std::cell::RefCell<std::collections::BTreeMap<usize, [[f32; 4]; 3]>> = Default::default();
}

/// A Ninja object that can be read as a model: itself, its model's two lists
/// and its child and sibling links all in RAM (or absent), one level down too.
fn plain_object(m: &Mem, o: u32) -> bool {
    let ram = |a: u32| (0x8000_0000..0x8C00_0000).contains(&a);
    let node = |o: u32| {
        if !ram(o) {
            return false;
        }
        let (mdl, child, sib) = (m.r32(o + 4), m.r32(o + 0x2C), m.r32(o + 0x30));
        (mdl == 0 || (ram(mdl) && [m.r32(mdl), m.r32(mdl + 4)].iter().all(|&l| l == 0 || ram(l)))) && [child, sib].iter().all(|&l| l == 0 || ram(l))
    };
    node(o) && [m.r32(o + 0x2C), m.r32(o + 0x30)].iter().all(|&l| l == 0 || node(l))
}

/// Under the hub's shoes the Chao's own foot is not drawn, as the hub hides
/// the foot meshes its footwear covers (src/scene.js attachOwnerGear): the
/// foot node's matrix is collapsed to its origin after the shoe's mount
/// was taken from it. Only the player's own Chao wears the shoes.
/// A foot node's view-space matrix for the shoes (next shoe draw), then the
/// Chao's own foot hidden under them.
fn record_foot(m: &mut Mem, i: usize, cur_ptr: u32) {
    if let Some(k) = SHOES.iter().position(|s| s.0 == i) {
        let cur = mtx(m, cur_ptr);
        FEET.with_borrow_mut(|f| f[k] = Some(cur));
        hide_foot(m, cur_ptr);
    }
}

fn hide_foot(m: &mut Mem, cur_ptr: u32) {
    let air = HUB_SHOE.get().is_some_and(|o| o != 0) && !debug_off("shoes") && !std::env::var("SA2_AIR_SHOES").is_ok_and(|v| v == "0");
    // Also under Sonic's Light Shoes while the tuner shows them.
    let light = LIGHT_SHOE.get().is_some_and(|o| o != 0) && debug_set("tryon").as_deref() == Some("1") && !(debug_off("lshoe0") && debug_off("lshoe1"));
    if air || light {
        for r in 0..3 { for k in 0..3 { m.w32(cur_ptr + 4 * (4 * r + k) as u32, 0); } }
    }
}

/// Arm nodes 3/10's view-space matrices from this draw walk, for the gloves.
fn record_hand(m: &Mem, i: usize, cur_ptr: u32) {
    if matches!(i, 1 | 3 | 6 | 10 | 13 | 16) {
        let cur = mtx(m, cur_ptr);
        NODE_MATS.with_borrow_mut(|n| { n.insert(i, cur); });
    }
    if i == 3 || i == 10 {
        let cur = mtx(m, cur_ptr);
        HANDS.with_borrow_mut(|h| h[usize::from(i == 10)] = Some(cur));
    }
}

thread_local! {
    static GLOVE_FRAME: Cell<u64> = const { Cell::new(u64::MAX) };
    static HANDS: std::cell::RefCell<[Option<[[f32; 4]; 3]>; 2]> = const { std::cell::RefCell::new([None; 2]) };
    static KNUX_TEX: Cell<Option<u32>> = const { Cell::new(None) };
    /// Rest fit per Chao arm node (3, 10): local placement under the node.
    static GLOVE_FIT: std::cell::RefCell<Option<[(u32, [[f32; 4]; 3]); 2]>> = const { std::cell::RefCell::new(None) };
    static FEET: std::cell::RefCell<[Option<[[f32; 4]; 3]>; 2]> = const { std::cell::RefCell::new([None; 2]) };
}

#[derive(Default)]
struct Mirror {
    /// This frame's node worlds as the walker computed them (unmirrored) and as written.
    truew: Vec<[[f32; 4]; 3]>,
    written: Vec<[[f32; 4]; 3]>,
    /// Chao-local node matrices, this frame and last.
    local: Vec<[[f32; 4]; 3]>,
    last: Vec<[[f32; 4]; 3]>,
    /// Left/right partner of each node (itself on the midline), from the rest pose.
    partner: Vec<usize>,
}

thread_local! {
    static MIR: std::cell::RefCell<Mirror> = Default::default();
}

/// a * b for 3x4 affine matrices.
fn mul34(a: &[[f32; 4]; 3], b: &[[f32; 4]; 3]) -> [[f32; 4]; 3] {
    std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|k| a[i][k] * b[k][j]).sum::<f32>() + if j == 3 { a[i][3] } else { 0.0 }))
}

/// S * l * S with S the reflection in the Chao's own x.
fn reflect_x(l: &[[f32; 4]; 3]) -> [[f32; 4]; 3] {
    std::array::from_fn(|r| std::array::from_fn(|c| if (r == 0) != (c == 0) { -l[r][c] } else { l[r][c] }))
}

/// Each node's left/right partner: the node whose rest position is nearest
/// its own reflected in x (itself on the midline).
fn mirror_partners(rest: &crate::chao_retarget::ChaoRest) -> Vec<usize> {
    let pos: Vec<[f32; 3]> = rest.worlds(&[]).iter().map(|&(_, p)| p).collect();
    let parent: Vec<Option<usize>> = rest.nodes.iter().map(|n| usize::try_from(n.0).ok()).collect();
    partners_of(&pos, &parent)
}

/// A joint and the part hanging from it rest on the same spot (the Chao's
/// shoulder 2 and arm 3, 9 and 10): of the nodes equally near the reflected
/// spot, the one as deep in the tree as the node itself is its partner. Taking
/// the first gave the arm the other shoulder's pose: in punch 2 both arms
/// hung at the sides and no arm punched (test copy 2026-11-24, knux/q-front).
pub(crate) fn partners_of(pos: &[[f32; 3]], parent: &[Option<usize>]) -> Vec<usize> {
    let depth = |mut k: usize| { let mut n = 0; while let Some(p) = parent.get(k).copied().flatten().filter(|&p| p < k) { k = p; n += 1; } n };
    (0..pos.len()).map(|i| {
        let q = [-pos[i][0], pos[i][1], pos[i][2]];
        let d = |j: usize| (0..3).map(|k| (pos[j][k] - q[k]).powi(2)).sum::<f32>();
        let best = (0..pos.len()).map(d).fold(f32::INFINITY, f32::min);
        let near = (0..pos.len()).filter(|&j| d(j) <= best + 1e-4);
        near.clone().find(|&j| depth(j) == depth(i)).or(near.clone().next()).unwrap_or(i)
    }).collect()
}

/// Punch 2 (Knuckles 110): the Chao's karate 582 mirrored in its own x. A
/// negative-determinant root matrix turned the faces inside out, so instead
/// node i draws as S * L(partner(i)) * S: its partner's pose reflected,
/// applied to its own (mirror-image) mesh, keeping the winding.
/// shortcut: a partner later in the walk uses last frame's matrix (one frame behind).
fn mirror_node(m: &mut Mem, i: usize, cur_ptr: u32) {
    let place = CHAO_PLACE.get();
    let cur = mtx(m, cur_ptr);
    let parent = CHAO_REST.with_borrow(|r| r.as_ref().and_then(|r| r.nodes.get(i)).and_then(|n| usize::try_from(n.0).ok()));
    let out = MIR.with_borrow_mut(|s| {
        if i == 0 {
            s.last = std::mem::take(&mut s.local);
            s.truew.clear();
            s.written.clear();
            if s.partner.is_empty() {
                s.partner = CHAO_REST.with_borrow(|r| r.as_ref().map(mirror_partners)).unwrap_or_default();
            }
        }
        if s.truew.len() != i {
            return None;
        }
        // The walker built this node on the parent as written: undo that.
        let (pw, pt) = match parent {
            Some(p) if p < i => (s.written[p], s.truew[p]),
            _ => (place, place),
        };
        let t = mul34(&pt, &inv_mul(&pw, &cur));
        s.truew.push(t);
        s.local.push(inv_mul(&place, &t));
        let j = s.partner.get(i).copied().unwrap_or(i);
        let l = if j <= i { s.local[j] } else { s.last.get(j).copied().unwrap_or(s.local[i]) };
        let out = mul34(&place, &reflect_x(&l));
        if std::env::var_os("SA2_MIRROR_LOG").is_some() && (i == 3 || i == 10 || i == 1) {
            let own = s.local[i];
            eprintln!("mirror: f{} node {i} partner {j} parent {parent:?} own at [{:.2} {:.2} {:.2}] x axis [{:.2} {:.2} {:.2}] partner at [{:.2} {:.2} {:.2}] last {} partners {:?}", sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed), own[0][3], own[1][3], own[2][3], own[0][0], own[1][0], own[2][0], l[0][3], l[1][3], l[2][3], s.last.len(), if i == 1 { s.partner.clone() } else { vec![] });
        }
        s.written.push(out);
        Some(out)
    });
    if let Some(out) = out {
        for r in 0..3 {
            for k in 0..4 {
                m.w32(cur_ptr + 4 * (4 * r + k) as u32, out[r][k].to_bits());
            }
        }
    }
}

/// ChaoMain's exec walker (.text 0xAA0) copies each node's matrix to the
/// pointer at AL_OBJECT+0xC4 (njGetMatrix at 0xC14/0xDE8); the display draws
/// from those. With the exec off in a cutscene, write the posed matrices there.
/// The AL_OBJECT root is chaowk+0x510 (ChaoMain 0xA24 passes it to 0xAA0).
/// unverified: the stored matrices are Chao model space (display adds world).
fn write_pose(m: &mut Mem, chao: u32) {
    let cw = m.r32(chao + 0x34);
    let root = if cw == 0 { 0 } else { m.r32(cw + 0x510) };
    let ram = |a: u32| (0x8000_0000..0x8C00_0000).contains(&a);
    if !ram(cw) || !ram(root) {
        return;
    }
    // Only a sane node tree (the Chao has ~40): a wrong root must not walk garbage.
    let (mut n, mut stack) = (0, vec![root]);
    while let Some(o) = stack.pop() {
        if !ram(o) || n > 100 {
            return;
        }
        n += 1;
        for k in [0x2c, 0x30] {
            let v = m.r32(o + k);
            if v != 0 {
                stack.push(v);
            }
        }
    }
    if n < 20 {
        return;
    }
    CHAO_REST.with_borrow_mut(|r| {
        if r.is_none() {
            let rest = crate::chao_retarget::read_chao(&|a| m.r32(a), root, [vec![], vec![]]);
            eprintln!("chao-player ev: Chao AL root {root:#x} nodes {}", rest.nodes.len());
            *r = Some(rest);
        }
    });
    pose_chao(false);
    let seated = seat(m);
    let mut nodes = vec![];
    fn walk(m: &Mem, o: u32, out: &mut Vec<u32>) {
        let mut o = o;
        while o >= 0x8000_0000 && out.len() < 128 {
            out.push(o);
            walk(m, m.r32(o + 0x2c), out);
            o = m.r32(o + 0x30);
        }
    }
    walk(m, root, &mut nodes);
    // World placement: the Chao's position (cw+0x14) and yaw (cw+0x0C, BAMS).
    // unverified: the exec stores world-space node matrices (no camera).
    let f = |a: u32| f32::from_bits(m.r32(a));
    let pos = [f(cw + 0x14), f(cw + 0x18), f(cw + 0x1C)];
    let (sy, cy) = ((m.r32(cw + 0x0C) as i32 as f32) * std::f32::consts::TAU / 65536.0).sin_cos();
    let ry = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
    // On the board: Sonic's root rotation (with the slope), at the board centre.
    let (ry, pos) = seated.unwrap_or((ry, pos));
    POSED.with_borrow(|p| {
        for (&n, &(r0, t0)) in nodes.iter().zip(p.iter()) {
            let r: [[f32; 3]; 3] = std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|k| ry[i][k] * r0[k][j]).sum()));
            let t: [f32; 3] = std::array::from_fn(|i| (0..3).map(|k| ry[i][k] * t0[k]).sum::<f32>() + pos[i]);
            let dst = m.r32(n + 0xC4);
            if dst < 0x8000_0000 {
                continue;
            }
            for row in 0..3 {
                for col in 0..3 {
                    m.w32(dst + 4 * (4 * row + col) as u32, r[row][col].to_bits());
                }
                m.w32(dst + 4 * (4 * row + 3) as u32, t[row].to_bits());
            }
        }
    });
}

/// The stage pose for this frame's draw: Sonic's retargeted pose, seated on
/// the board while it draws, in the Chao's local space (its display places
/// it at chaowk +0x14 with yaw +0x0C, as `pin` set them); seated, in world.
fn level_pose(m: &Mem, chao: u32) {
    let cw = m.r32(chao + 0x34);
    let root = if cw == 0 { 0 } else { m.r32(cw + 0x510) };
    if !(0x8000_0000..0x8C00_0000).contains(&root) {
        return POSED.set(vec![]);
    }
    CHAO_REST.with_borrow_mut(|r| {
        if r.is_none() {
            let dirs = std::fs::read_to_string(std::env::var("SA2_CHAO_ARM_DIRS").unwrap_or_default())
                .ok().and_then(|j| crate::chao_retarget::parse_arm_dirs(&j)).unwrap_or_default();
            let rest = crate::chao_retarget::read_chao(&|a| m.r32(a), root, dirs);
            eprintln!("chao-player lvl: Chao rest {root:#x} nodes {}", rest.nodes.len());
            *r = Some(rest);
        }
    });
    pose_chao(self_retarget(m));
    POSED_WORLD.set(false);
    // SA2_HEAD_LOG=a-b (debug): the posed head (node 16) and body (node 1)
    // up vectors and the rider's spine 2->34 each frame, to see head jitter.
    if let Some((a, b)) = std::env::var("SA2_HEAD_LOG").ok().and_then(|r| { let (a, b) = r.split_once('-')?; Some((a.parse::<u64>().ok()?, b.parse::<u64>().ok()?)) }) {
        let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
        if (a..=b).contains(&f) {
            let up = |r: &[[f32; 3]; 3]| [r[0][1], r[1][1], r[2][1]].map(|x| (x * 1000.0).round() / 1000.0);
            let sp = SKEL.with_borrow(|s| if (s.len() == 62 && skel_site("c9")) { let a = inv_mul(&s[0], &s[2]); let b = inv_mul(&s[0], &s[34]); [b[0][3] - a[0][3], b[1][3] - a[1][3], b[2][3] - a[2][3]].map(|x| (x * 1000.0).round() / 1000.0) } else { [0.0; 3] });
            POSED.with_borrow(|p| if p.len() > 16 {
                eprintln!("head f{f}: body up {:?} head up {:?} head pos {:?} spine {sp:?}", up(&p[1].0), up(&p[16].0), p[16].1.map(|x| (x * 1000.0).round() / 1000.0));
            });
        }
    }
    let Some((r, c)) = seat(m) else { return };
    // Seated: world matrices (r p + c); chao_node puts them under the view
    // (base) matrix, so the display's own placement does not matter.
    let mul = |a: &[[f32; 3]; 3], v: [f32; 3]| -> [f32; 3] { std::array::from_fn(|i| (0..3).map(|k| a[i][k] * v[k]).sum()) };
    POSED.with_borrow_mut(|p| {
        for (r0, t0) in p.iter_mut() {
            *r0 = std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|k| r[i][k] * r0[k][j]).sum()));
            let w = mul(&r, *t0);
            *t0 = [w[0] + c[0], w[1] + c[1], w[2] + c[2]];
        }
    });
    POSED_WORLD.set(true);
    if sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) % 120 == 0 {
        let f = |a: u32| f32::from_bits(m.r32(a));
        eprintln!("chao-player lvl: seated, board centre {c:?}, chao root {:?}, posed root {:?}",
            [f(cw + 0x14), f(cw + 0x18), f(cw + 0x1C)], POSED.with_borrow(|p| p.first().map(|x| x.1)));
    }
}

thread_local! {
    /// POSED holds world matrices (seated on the board), not Chao-local ones.
    static POSED_WORLD: Cell<bool> = const { Cell::new(false) };
    /// The rider moved since the last frame: the legs take its motion only then
    /// (standing, the Chao's own legs).
    static MOVING: Cell<bool> = const { Cell::new(false) };
    static LAST_RIDER_POS: Cell<[u32; 3]> = const { Cell::new([0; 3]) };
    static TREADMILL: Cell<Option<[u32; 3]>> = const { Cell::new(None) };
    /// The player's Chao is being drawn on its native rig (Chao World).
    static NATIVE_DRAW: Cell<bool> = const { Cell::new(false) };
}

/// While a board drew this or last frame: seat the posed Chao on it
/// (retarget `seat_on_board`), and return the Chao-space -> world rotation
/// and origin (board centre) to place it with.
fn seat(m: &Mem) -> Option<([[f32; 3]; 3], [f32; 3])> {
    let _ = m;
    let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
    let (bf, mut tris, local) = BOARD.with_borrow(|b| b.clone())?;
    if f > bf + 1 || RIDER.get() == 0 {
        return None;
    }
    let (root, sonic): ([[f32; 4]; 3], Vec<[[f32; 4]; 3]>) = SKEL.with_borrow(|s| {
        (s.first().copied().unwrap_or_default(), s.iter().map(|e| inv_mul(&s[0], e)).collect())
    });
    if sonic.len() != 62 {
        return None;
    }
    CHAO_REST.with_borrow(|r| {
        let rest = r.as_ref()?;
        POSED.with_borrow_mut(|p| { rest.seat_on_board(p, &sonic, &mut tris, BOARD_SCALE); });
        Some(())
    })?;
    // Sonic root rotation, columns normalised, times from_chao (c -> [-c2, c1, c0]).
    let col = |j: usize| { let l = (0..3).map(|i| root[i][j] * root[i][j]).sum::<f32>().sqrt().max(1e-6); [root[0][j] / l, root[1][j] / l, root[2][j] / l] };
    let (c0, c1, c2) = (col(0), col(1), col(2));
    let r: [[f32; 3]; 3] = std::array::from_fn(|i| [c2[i], c1[i], -c0[i]]);
    Some((r, apply(&root, local)))
}

/// Retarget Sonic's skeleton (made model-space by his root) onto the Chao.
thread_local! { static POSE_CALLS: Cell<u32> = const { Cell::new(0) }; }

fn pose_chao(upright: bool) {
    if std::env::var_os("SA2_STATE_LOG").is_some() {
        let n = POSE_CALLS.replace(POSE_CALLS.get() + 1);
        if n < 5 || n % 200 == 0 {
            eprintln!("pose-chao: call {n} at f{} (native draw {}, chao draw {}, level rig {}, skel {}, node {}, rider {:#x}, last exec {:#x}, own motion {})", sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed), NATIVE_DRAW.get(), CHAO_DRAW.get(), LEVEL_RIG.get(), SKEL.with_borrow(|s| s.len()), CHAO_NODE.get(), RIDER.get(), LAST_EXEC.get(), OWN_MOTION.get());
        }
    }
    let sonic: Vec<[[f32; 4]; 3]> = SKEL.with_borrow(|s| {
        if s.len() < 56 {
            return vec![];
        }
        if (s.len() == 62 && skel_site("c10")) {
            return s.iter().map(|e| inv_mul(&s[0], e)).collect();
        }
        STEAM_TO_EV.iter().map(|&e| inv_mul(&s[0], &s[e as usize])).collect()
    });
    let posed = CHAO_REST.with_borrow(|r| match (r, sonic.len()) {
        _ if OWN_MOTION.get() => vec![],
        (Some(rest), 62) if upright && !std::env::var("SA2_CHAO_LEAN").is_ok_and(|v| v == "1") => rest.pose_upright(&sonic, ARM_FWD.get()),
        (Some(rest), 62) => {
            let mut p = rest.pose(&sonic, false, ARM_FWD.get());
            // Chao World self mode: the body keeps the Chao's own upright
            // tilt (Shadow's run lean laid it flat, face down, on the Chao);
            // the whole pose turns about the body node back to its rest up.
            // SA2_CHAO_LEAN=1 (debug): his lean as it is.
            if false && upright && p.len() > 1 {
                let rest_up = { let r = rest.worlds(&[])[1].0; [r[0][1], r[1][1], r[2][1]] };
                let (br, bp) = p[1];
                let up = [br[0][1], br[1][1], br[2][1]];
                let fix = crate::chao_retarget::rot_between(up, rest_up);
                for (r, t) in p.iter_mut() {
                    *r = crate::skate::mul(&fix, r);
                    let d = [t[0] - bp[0], t[1] - bp[1], t[2] - bp[2]];
                    *t = std::array::from_fn(|i| bp[i] + (0..3).map(|k| fix[i][k] * d[k]).sum::<f32>());
                }
            }
            p
        }
        _ => vec![],
    });
    POSED.set(posed);
}

/// The player's own Chao task once it exists (for the Chao World chart).
pub fn player_chao() -> Option<u32> {
    match STEP.get() {
        Step::Live(chao) => Some(chao),
        _ => None,
    }
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

fn display(c: &mut Cpu, m: &mut Mem, addr: u32) {
    if self_drive(m) {
        // His display walked for his skeleton only (SKEL), drawing nothing
        // and no gear (the Chao's draw carries it).
        // Always in self mode: the hub's skate overlay (skate_node) reads his
        // live pose, also when his whole skeleton is not copied on the Chao.
        if !std::env::var("SA2_SELF_CAP").is_ok_and(|v| v == "0") {
            // His effects (the jump's spin ball) are drawn, at the Chao's
            // size: the view is scaled by BOARD_SCALE about the Chao's spot
            // for the call (current and base matrix), and the captured
            // skeleton is taken back to full size.
            let Step::Live(chao) = STEP.get() else { return };
            let cw = m.r32(chao + 0x34);
            let p = [0x14, 0x18, 0x1C].map(|o| f32::from_bits(m.r32(cw + o)));
            let s = BOARD_SCALE;
            let t: [[f32; 4]; 3] = std::array::from_fn(|i| std::array::from_fn(|j| if j < 3 { if i == j { s } else { 0.0 } } else { (1.0 - s) * p[i] }));
            let (cur_ptr, base_ptr) = (m.r32(NJ_CURRENT_MATRIX), m.r32(NJ_BASE_MATRIX));
            let (cur, base) = (mtx(m, cur_ptr), mtx(m, base_ptr));
            let put = |m: &mut Mem, a: u32, x: &[[f32; 4]; 3]| for r in 0..3 { for k in 0..4 { m.w32(a + 4 * (4 * r + k) as u32, x[r][k].to_bits()); } };
            let (curs, bases) = (mul34(&cur, &t), mul34(&base, &t));
            put(m, cur_ptr, &curs);
            if base_ptr != cur_ptr {
                put(m, base_ptr, &bases);
            }
            SELF_SCALE.set(Some((cur, curs)));
            SELF_CAP.set(true);
            LCAP.set(true);
            original(c, m, addr);
            LCAP.set(false);
            SELF_CAP.set(false);
            SELF_SCALE.set(None);
            put(m, cur_ptr, &cur);
            if base_ptr != cur_ptr {
                put(m, base_ptr, &base);
            }
        }
        return;
    }
    // SA2_RIDER_DISP_OFF=1 (test): skip the hidden rider's display in Chao
    // World, to see whether the white skate glow comes from it.
    if chao_world(m) && matches!(STEP.get(), Step::Live(_)) && std::env::var("SA2_RIDER_DISP_OFF").is_ok_and(|v| v == "1") {
        return;
    }
    if matches!(STEP.get(), Step::Live(_)) {
        // SA2_DISP_TEST (debug, for the missing head): plain = his display as the game runs it;
        // nocap = inside it the motion draws run as the game's (no walk for the skeleton).
        match std::env::var("SA2_DISP_TEST").as_deref() {
            Ok("plain") => return original(c, m, addr),
            Ok("skip") => return,
            _ => {}
        }
        // The rider's own display, walked for his node matrices, drawing nothing.
        LCAP.set(true);
        original(c, m, addr);
        LCAP.set(false);
        return;
    }
    // Never Sonic: until the player's Chao is up (a new area, a reload, after
    // the transporter) the rider draws nothing in any stage.
}

/// The stage number (i16 at 0x803AD820); 90 and up are Chao World, where the
/// game loads ChaoMain itself: a second copy at BASE collided with it (web
/// lobby: "no function at 0x812012d0", lr inside BASE), so there the
/// player's Chao is created on the game's own module.
const STAGE_NUM: u32 = 0x803A_D820;

/// Free bytes in OS heap `h` (its free list, HeapArray 0x803ADC68).
pub(crate) fn os_heap_free(m: &Mem, h: u32) -> u32 {
    let mut free = 0u32;
    let mut cell = m.r32(m.r32(0x803A_DC68) + 12 * h + 4);
    for _ in 0..100_000 {
        if !(0x8000_0000..0x8C00_0000).contains(&cell) { break; }
        free += m.r32(cell + 8);
        cell = m.r32(cell + 4);
    }
    free
}

/// SA2_HEAP_LOG (debug): each texture file the game loads by name (0x80072548, 0x80072820):
/// its name, what the loader answered, and each heap's free bytes and largest free block after.
fn tex_load_log(c: &mut Cpu, m: &mut Mem, addr: u32) {
    let (name, list) = (c.r[3], c.r[4]);
    original(c, m, addr);
    let text: String = (0..24).map(|i| m.r8(name + i)).take_while(|&b| b != 0).map(|b| b as char).collect();
    let heaps: Vec<String> = (0..2).map(|h| {
        let (mut free, mut big, mut cell) = (0u32, 0u32, m.r32(m.r32(0x803A_DC68) + 12 * h + 4));
        for _ in 0..100_000 {
            if !(0x8000_0000..0x8C00_0000).contains(&cell) { break; }
            free += m.r32(cell + 8);
            big = big.max(m.r32(cell + 8));
            cell = m.r32(cell + 4);
        }
        format!("h{h} free {} KB largest {} KB", free / 1024, big / 1024)
    }).collect();
    // 0x80072548 takes no list: its r4 is whatever was there (Radical Highway: 0xFFFFD412, a read there stopped the game).
    let lw = |o: u32| if (0x8000_0000..0x8C00_0000).contains(&list) { m.r32(list + o) } else { 0 };
    eprintln!("tex-load: {addr:#x} {text:?} list {list:#x} -> {:#x} (list now: names {:#x} count {}); {}", c.r[3], lw(0), lw(4), heaps.join(", "));
}

/// SA2_HEAP_LOG=n (debug): every n frames, the OS heaps (HeapArray
/// 0x803ADC68: size, free list, allocated list per 12-byte descriptor) as
/// total and free bytes, and the arena (0x803ADC70/74).
fn heap_log(m: &Mem) {
    let Some(n) = std::env::var("SA2_HEAP_LOG").ok().and_then(|v| v.parse::<u64>().ok()) else { return };
    let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
    if n == 0 || f % n != 0 {
        return;
    }
    let (arr, num) = (m.r32(0x803A_DC68), m.r32(0x803A_DC6C));
    let mb = |b: u32| b as f32 / 1048576.0;
    let mut out = vec![];
    for h in 0..num.min(16) {
        let d = arr + 12 * h;
        let size = m.r32(d) as i32;
        if size < 0 {
            continue;
        }
        let free = os_heap_free(m, h);
        out.push(format!("heap {h}: {:.2} MB, free {:.2} MB", mb(size as u32), mb(free)));
    }
    eprintln!("chao-player heap: frame {f} stage {} arena {:#x}-{:#x} {}", m.r16(STAGE_NUM), m.r32(0x803A_DC70), m.r32(0x803A_DC74), out.join("; "));
}

/// SA2_INFO_OUT=<file> (debug panel): twice a second a line with the OS
/// heaps' free space, the player's and other players' Chao, stage, area and
/// the player's Chao's place, for the page's info overlay.
fn info_out(m: &Mem, area: u16) {
    let Ok(path) = std::env::var("SA2_INFO_OUT") else { return };
    let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
    if f % 30 != 0 {
        return;
    }
    let num = m.r32(0x803A_DC6C).min(4);
    let heaps: Vec<String> = (0..num).map(|h| format!("h{h} {} KB", os_heap_free(m, h) / 1024)).collect();
    let remote = REMOTE.with_borrow(|r| (r.len(), r.values().filter(|e| e.task != 0).count()));
    let place = match STEP.get() {
        Step::Live(chao) => { let cw = m.r32(chao + 0x34); let g = |o: u32| f32::from_bits(m.r32(cw + o)); format!("({:.0}, {:.0}, {:.0})", g(0x14), g(0x18), g(0x1C)) }
        s => format!("{s:?}"),
    };
    let line = format!("frame {f} stage {} area {area}\n{}\nothers {} ({} drawn)\nChao {place}\n", m.r16(STAGE_NUM), heaps.join(", "), remote.0, remote.1);
    let _ = std::fs::write(path, line);
}

/// Debug panel "set tp x y z": the player's Chao (and the hidden character
/// it moves as) is put at that spot once per new value, to try spawn and
/// teleporter places.
fn debug_teleport(m: &mut Mem, task: u32) {
    let Some(v) = debug_set("tp") else { return };
    if TP_DONE.with_borrow(|d| *d == v) {
        return;
    }
    let p: Vec<f32> = v.split([',', ' ']).filter_map(|x| x.trim().parse().ok()).collect();
    let Step::Live(chao) = STEP.get() else { return };
    let cw = m.r32(chao + 0x34);
    if p.len() != 3 || cw == 0 {
        return;
    }
    TP_DONE.set(v);
    for twp in [cw, m.r32(task + 0x34)] {
        if twp != 0 {
            for (i, x) in p.iter().enumerate() {
                m.w32(twp + 0x14 + 4 * i as u32, x.to_bits());
            }
        }
    }
    eprintln!("chao-player: debug teleport to {p:?}");
}

/// A /mmo/goto-in line (sa2-web staff-goto.js): "seq area" or "seq area x y z",
/// area 0 the lobby. None for anything else.
fn parse_goto(text: &str) -> Option<(u32, u32, Option<[f32; 3]>)> {
    let v: Vec<&str> = text.split_whitespace().collect();
    let (seq, area) = (v.first()?.parse().ok()?, v.get(1)?.parse::<u32>().ok().filter(|a| *a <= 12)?);
    let at = match v.len() {
        2 => None,
        5 => {
            let p = [v[2].parse::<f32>().ok()?, v[3].parse().ok()?, v[4].parse().ok()?];
            Some(p).filter(|p| p.iter().all(|x| x.is_finite()))?;
            Some(p)
        }
        _ => return None,
    };
    Some((seq, if area == 0 { crate::start_stage::LOBBY } else { area }, at))
}

/// SA2_GOTO_IN (staff go-to and follow, the server's `staff-goto` event): each
/// new line moves the player's Chao to that area, with ChaoMain's own area
/// change as SA2_CHAO_AREA_AT below, and then to that spot as debug_teleport.
/// `area` is the current one (0 while loading).
fn staff_goto(c: &mut Cpu, m: &mut Mem, task: u32, area: u32) {
    let Ok(path) = std::env::var("SA2_GOTO_IN") else { return };
    if sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) % 15 == 0 {
        if let Some((seq, to, at)) = std::fs::read_to_string(path).ok().and_then(|t| parse_goto(&t)) {
            if GOTO_SEQ.replace(seq) != seq {
                GOTO.set(Some((to, at, false)));
            }
        }
    }
    let Some((to, at, asked)) = GOTO.get() else { return };
    if !chao_world(m) {
        // In a level: a go-to must not fire later, back in Chao World.
        return GOTO.set(None);
    }
    if area == 0 || !module_linked(m, 60) {
        return;
    }
    if area != to {
        if !asked {
            eprintln!("chao-player: staff go-to, area {area} to {to}");
            call(c, m, rt::sec(m, 60, 1) + 0x27E00, &[to]);
            GOTO.set(Some((to, at, true)));
        }
        return;
    }
    let Step::Live(chao) = STEP.get() else { return };
    let cw = m.r32(chao + 0x34);
    if cw == 0 {
        return;
    }
    // After an area change the lobby makes the player's Chao twice (the second
    // 1.4 s after the first, at the lobby's own spot): the place goes to the
    // task that has stayed for 3 s.
    if asked {
        let (held, n) = GOTO_HELD.get();
        let n = if held == chao { n + 1 } else { 0 };
        GOTO_HELD.set((chao, n));
        if n < 180 {
            return;
        }
    }
    GOTO.set(None);
    let Some(p) = at else { return };
    for twp in [cw, m.r32(task + 0x34)] {
        if twp != 0 {
            for (i, x) in p.iter().enumerate() {
                m.w32(twp + 0x14 + 4 * i as u32, x.to_bits());
            }
        }
    }
}

thread_local! {
    static LEAVE_DONE: Cell<bool> = const { Cell::new(false) };
    static GOTO_SEQ: Cell<u32> = const { Cell::new(0) };
    /// The Chao task seen after a go-to's area change, and for how many frames.
    static GOTO_HELD: Cell<(u32, u32)> = const { Cell::new((0, 0)) };
    /// The go-to under way: (area, spot, area change asked).
    static GOTO: Cell<Option<(u32, Option<[f32; 3]>, bool)>> = const { Cell::new(None) };
    static TP_DONE: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// The character's jump spin flash, its own task (main.dol display 0x8008B0CC:
/// work at task +0x40: +1 player, +4 position, +0x1C uniform scale, drawn
/// with njTranslateEx/njScale). Self mode: shrunk to the Chao's size
/// (BOARD_SCALE) about the Chao's spot, as the character's own display is.
const SPIN_FLASH_DISP: u32 = 0x8008_B0CC;
fn spin_flash(c: &mut Cpu, m: &mut Mem) {
    let w = m.r32(c.r[3] + 0x40);
    let Step::Live(chao) = STEP.get() else { return original(c, m, SPIN_FLASH_DISP) };
    let cw = m.r32(chao + 0x34);
    if !self_drive(m) || w == 0 || cw == 0 || m.r8(w + 1) != 0 {
        return original(c, m, SPIN_FLASH_DISP);
    }
    // Chao World: no spin effect either (it goes with the ball).
    if !std::env::var("SA2_SELF_BALL").is_ok_and(|v| v == "1") {
        return;
    }
    let keep: Vec<u32> = (0..4).map(|i| m.r32(w + 4 + 4 * i)).collect();
    let (s, keep_scale) = (BOARD_SCALE, m.r32(w + 0x1C));
    for i in 0..3u32 {
        let p = f32::from_bits(m.r32(cw + 0x14 + 4 * i));
        let v = f32::from_bits(keep[i as usize]);
        m.w32(w + 4 + 4 * i, (p + (v - p) * s).to_bits());
    }
    m.w32(w + 0x1C, (f32::from_bits(keep_scale) * s).to_bits());
    original(c, m, SPIN_FLASH_DISP);
    for i in 0..3u32 {
        m.w32(w + 4 + 4 * i, keep[i as usize]);
    }
    m.w32(w + 0x1C, keep_scale);
}

/// Moving (or within 30 frames of it): Shadow's skeleton poses the Chao.
/// Standing still the Chao keeps its own stand (his idle, retargeted,
/// bent it over with its hands on the floor; test copy run r56).
fn self_in_motion() -> bool {
    sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed).saturating_sub(MOVED_AT.get()) < 30
}

thread_local! {
    static MOVED_AT: Cell<u64> = const { Cell::new(0) };
}

/// Self mode with the Chao posed from the hidden character's skeleton
/// (default; SA2_CHAO_RETARGET=0: the Chao's own motions).
fn self_retarget(m: &Mem) -> bool {
    // Off by default: the retargeted body lay flat and upside down in live
    // play (build 16 and test copy run r107). The Chao keeps its own rig and
    // motions with the hub's skate overlay (skate.rs) while it wears the
    // shoes, as the hub does. SA2_CHAO_RETARGET=1 (debug): his skeleton.
    // Off by default: the hub's way is the Chao's own rig and motions at its
    // own scale with the skate overlay (skate.rs) from his live pose.
    // SA2_CHAO_RETARGET=1 (debug): his limb directions copied on the Chao.
    self_drive(m) && std::env::var("SA2_CHAO_RETARGET").is_ok_and(|v| v == "1")
}

thread_local! {
    /// The character's display is walked only for SKEL (self mode).
    static SELF_CAP: Cell<bool> = const { Cell::new(false) };
    /// The view before and after the self-mode scale (to undo it on the skeleton).
    static SELF_SCALE: Cell<Option<([[f32; 4]; 3], [[f32; 4]; 3])>> = const { Cell::new(None) };
}

fn chao_world(m: &Mem) -> bool {
    m.r16(STAGE_NUM) as i16 >= 90
}

/// SA2_CHAO_SELF=1: in Chao World the player's Chao drives itself
/// (chao_drive.rs) and Sonic's exec and display do not run at all.
fn self_drive(m: &Mem) -> bool {
    chao_world(m) && matches!(STEP.get(), Step::Live(_)) && std::env::var("SA2_CHAO_SELF").is_ok_and(|v| v.trim() == "1")
}

/// SA2_CHAO_PMOVE=1 (with SA2_CHAO_SELF=1): the character's own exec moves
/// the Chao instead of chao_drive.
fn player_move_on() -> bool {
    std::env::var("SA2_CHAO_PMOVE").is_ok_and(|v| v.trim() == "1")
}

/// The step the game moved the character's task work `own` since the Chao's
/// place was mirrored into it, taken by the Chao's `cw`. Before the Chao is
/// drawn too: the push that stands him on a pad's top comes after his exec,
/// and drawn before it the Chao stood 0.58 into the pad.
/// The step from `at` to `now` when it is a push: something moved, and no
/// coordinate by more than PUSH_MAX (the game placing the character).
pub(crate) fn push_step(at: [f32; 3], now: [f32; 3]) -> Option<[f32; 3]> {
    let step: [f32; 3] = std::array::from_fn(|i| now[i] - at[i]);
    (step.iter().any(|d| *d != 0.0) && step.iter().all(|d| d.abs() <= PUSH_MAX)).then_some(step)
}

fn take_push(m: &mut Mem, own: u32, cw: u32) {
    if let Some(at) = MIRRORED.take() {
        if let Some(step) = push_step(at, std::array::from_fn(|i| f32::from_bits(m.r32(own + 0x14 + 4 * i as u32)))) {
            for (i, d) in step.iter().enumerate() {
                let a = cw + 0x14 + 4 * i as u32;
                m.w32(a, (f32::from_bits(m.r32(a)) + d).to_bits());
            }
            if std::env::var("SA2_PUSH_LOG").is_ok_and(|v| v == "1") {
                eprintln!("chao-drive: pushed {step:?}");
            }
        }
    }
}

/// The character's exec (Sonic/Shadow/Metal 0x80083E44: action state in twp
/// +0x00, then e.g. 0x80048490, 0x8004C734, 0x80046DE4 speed (pwp +0x64 +=
/// acc +0x58), 0x80045880, 0x8003E668 per state) run on the Chao's body: for
/// the call the task's twp (+0x34) is the Chao's, with the character's
/// collision work (twp +0x2C) so its land and object collision apply.
/// The Chao's own MOVE_WORK step and AL_DetectCollision stand down: both
/// skip while MOVE_WORK flag 0x10 is set (MOV_Control 0x8002F448, ChaoMain
/// .text 0x3600). The character's display never runs.
/// shortcut: the Chao twp's first word (mode, smode, id, btimer) carries the
/// character's action state while this runs; unverified that no Chao code
/// reads it for the player's Chao (its AI is off).
/// The longest step in one frame taken as a push by a solid object.
const PUSH_MAX: f32 = 3.0;

fn player_move(c: &mut Cpu, m: &mut Mem, addr: u32, task: u32, chao: u32) {
    let (cw, mw, own) = (m.r32(chao + 0x34), m.r32(chao + 0x38), m.r32(task + 0x34));
    if cw == 0 || own == 0 {
        return;
    }
    if PMOVE_CHAO.replace(chao) != chao {
        eprintln!("chao-drive: player move on the Chao's body (twp {cw:#x}, character twp {own:#x}, state {})", m.r8(own));
    }
    if mw != 0 {
        m.w16(mw + 0x40, m.r16(mw + 0x40) | 0x10);
    }
    // Solid objects push the player through its collision work's task, which
    // is the character's: its task work moved since the Chao's place was
    // mirrored into it, and the Chao takes the same step (mob, 2026-11-24:
    // the lobby's exit teleporter had no collision). A step over PUSH_MAX is
    // not a push (the game placing the character) and is left alone.
    take_push(m, own, cw);
    // The Chao's exec enters its own collision (CCL_Entry) while twp +0xA0
    // has bit 8; the character's collision is at the same spot, and a
    // jump into it went down the character's hit path (main.dol 0x80055A50:
    // knock-back, then an unchecked hook at 0x801A2A18 that is null in Chao
    // World). Unverified that the hitter was the Chao itself.
    m.w32(cw + 0xA0, m.r32(cw + 0xA0) & !8);
    let chao_cwp = m.r32(cw + 0x2C);
    m.w32(task + 0x34, cw);
    m.w32(cw + 0x2C, m.r32(own + 0x2C));
    c.r[3] = task;
    // The character's action state and flags (twp +0x00..+0x08: mode,
    // smode, id, btimer, flag, wtimer) stay in its own task work and are lent
    // to the Chao's for the call only: left in the Chao's, the Chao's own exec
    // read them as its own and its motion stopped (mob: it moved, its
    // animation did not).
    let chao_head = [m.r32(cw), m.r32(cw + 4)];
    m.w32(cw, m.r32(own));
    m.w32(cw + 4, m.r32(own + 4));
    IN_PMOVE.set(true);
    original(c, m, addr);
    IN_PMOVE.set(false);
    m.w32(own, m.r32(cw));
    m.w32(own + 4, m.r32(cw + 4));
    // His body's hit shape is put by his exec at his own centre height above
    // his feet (5.4, whatever his land collision's centre is set to), over the
    // Chao's head: object triggers at the Chao's height were missed (the lobby
    // exit's, 4.0 above its pad, by the game's own hit). Brought down to the
    // fitted centre height, along the same line from his feet.
    if let Some((obj2, full)) = FULL_SIZE.get().filter(|o| o.0 == m.r32(P1_OBJ2)) {
        let (fit, full) = (f32::from_bits(m.r32(obj2 + PW_CENTRE)), f32::from_bits(full[2]));
        let body = { let p = m.r32(own + 0x2C); if (0x8000_0000..0x8C00_0000).contains(&p) { m.r32(p + 0xC) } else { 0 } };
        if fit > 0.0 && fit < full && (0x8000_0000..0x8C00_0000).contains(&body) {
            for o in [0u32, 4, 8] {
                let (p, at) = (f32::from_bits(m.r32(cw + 0x14 + o)), f32::from_bits(m.r32(body + 8 + o)));
                m.w32(body + 8 + o, (p + (at - p) * fit / full).to_bits());
            }
        }
    }
    CHAR_ANG.set(Some([m.r32(cw + 0x08), m.r32(cw + 0x0C), m.r32(cw + 0x10)]));
    m.w32(cw, chao_head[0]);
    m.w32(cw + 4, chao_head[1]);
    m.w32(task + 0x34, own);
    m.w32(cw + 0x2C, chao_cwp);
    if sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) % 60 == 0 {
        let f = |a: u32| f32::from_bits(m.r32(a));
        eprintln!("chao-drive: pmove state {} pos ({:.1},{:.1},{:.1}) yaw {:#x} flag {:#x} chao mode {} motion {}", m.r8(own), f(cw + 0x14), f(cw + 0x18), f(cw + 0x1C), m.r32(cw + 0x0C), m.r16(own + 4), m.r8(cw), LAST_MOTION.get());
    }
}

thread_local! {
    /// The hidden character's own land collision size (player work, its three words), to put back.
    static FULL_SIZE: Cell<Option<(u32, [u32; 3])>> = const { Cell::new(None) };
    /// His body hit shape's own three sizes (his collision table's first entry), to put back.
    static FULL_BODY: Cell<Option<(u32, [u32; 3])>> = const { Cell::new(None) };
}

/// The Chao's land collision size from its own move work (as ChaoMain set it
/// for this Chao; test copy 2026-11-24: +0xB4 = 1.5, its body collision
/// entry's size too, and +0xAC = 3.0), and the hidden character's as its
/// physics row has it (player work +0x130 radius, +0x134 height, +0x140
/// centre height; Knuckles 4.0 / 11.4 / 5.7).
/// unverified: that +0xB4 is the radius and +0xAC the height by name; the two
/// numbers are the game's.
const CHAO_MOVE_RADIUS: u32 = 0xB4;
const CHAO_MOVE_HEIGHT: u32 = 0xAC;
const PW_RADIUS: u32 = 0x130;
const PW_HEIGHT: u32 = 0x134;
const PW_CENTRE: u32 = 0x140;

/// The size the hidden character collides with the land at, for a Chao of
/// this size: the Chao's radius and height, and for the centre height the
/// height of the Chao's own body collision's centre (`chao_centre`, 2.0), or
/// without one the same share of the height as his own.
pub(crate) fn fitted_size(chao_radius: f32, chao_height: f32, chao_centre: Option<f32>, his: [f32; 3]) -> Option<[f32; 3]> {
    let ok = chao_radius > 0.1 && chao_radius < his[0] && chao_height > 0.1 && chao_height < his[1] && his[1] > 0.0;
    ok.then(|| [chao_radius, chao_height, chao_centre.filter(|c| *c > 0.0 && *c < chao_height).unwrap_or(chao_height * his[2] / his[1])])
}

/// The Chao's body collision entry (its collision table, 0x30 bytes an entry: kind at +0,
/// centre at +8, sizes a b c at +0x14; the body is kind 129: centre (0, 2, 0), sizes 1.5,
/// 2.5, 0.7 on the test copy): its centre height and its three sizes.
fn chao_body_shape(m: &Mem, chao: u32) -> Option<(f32, [u32; 3])> {
    let cw = m.r32(chao + 0x34);
    let cwp = if cw == 0 { 0 } else { m.r32(cw + 0x2C) };
    if !(0x8000_0000..0x8C00_0000).contains(&cwp) { return None; }
    let (info, n) = (m.r32(cwp + 0xC), u32::from(m.r16(cwp + 0xA)).clamp(1, 8));
    if !(0x8000_0000..0x8C00_0000).contains(&info) { return None; }
    (0..n).map(|k| info + 0x30 * k).find(|&e| m.r8(e) == 129).map(|e| (f32::from_bits(m.r32(e + 0xC)), [m.r32(e + 0x14), m.r32(e + 0x18), m.r32(e + 0x1C)]))
}

/// MMO rule (mob 2026-11-24: "need shrunk but his jump and dig need to be as
/// deep as the full size", then "do the same with sonic/shadow collision"):
/// while the player's Chao stands in for the hidden character, in every
/// stage, he collides with the land at the Chao's size. Only these three
/// numbers of his row change: speeds, jump and the rest stay his.
/// SA2_FULL_SIZE=1 (debug): his own size, to compare.
fn collision_fit(m: &mut Mem, chao: u32) {
    let (obj2, mw) = (m.r32(P1_OBJ2), m.r32(chao + 0x38));
    if !(0x8000_0000..0x8C00_0000).contains(&obj2) || !(0x8000_0000..0x8C00_0000).contains(&mw) {
        return;
    }
    let at = [PW_RADIUS, PW_HEIGHT, PW_CENTRE];
    let full = match FULL_SIZE.get().filter(|o| o.0 == obj2) {
        Some(o) => o.1,
        None => { let o = at.map(|a| m.r32(obj2 + a)); FULL_SIZE.set(Some((obj2, o))); o }
    };
    let f = |m: &Mem, a: u32| f32::from_bits(m.r32(a));
    let body = chao_body_shape(m, chao);
    let fit = fitted_size(f(m, mw + CHAO_MOVE_RADIUS), f(m, mw + CHAO_MOVE_HEIGHT), body.map(|b| b.0), full.map(f32::from_bits));
    let use_full = std::env::var("SA2_FULL_SIZE").is_ok_and(|v| v == "1");
    let size = match fit { Some(s) if !use_full => s.map(f32::to_bits), _ => full };
    for (a, v) in at.into_iter().zip(size) { m.w32(obj2 + a, v); }
    // His body's hit shape (the first entry of his collision table: his own sizes 4.0, 4.0,
    // 0.65, at his position plus the centre height above): the Chao's own body sizes, so
    // what is hit is what is seen. His other four spheres (8.0, 4.5, 2.75, 30.0: attack
    // and pick-up reaches, not told apart yet) stay his.
    let his_cwp = { let t = m.r32(0x801E_7768); if (0x8000_0000..0x8C00_0000).contains(&t) { m.r32(t + 0x2C) } else { 0 } };
    let his_body = if (0x8000_0000..0x8C00_0000).contains(&his_cwp) { m.r32(his_cwp + 0xC) } else { 0 };
    if (0x8000_0000..0x8C00_0000).contains(&his_body) {
        let own = match FULL_BODY.get().filter(|o| o.0 == his_body) {
            Some(o) => o.1,
            None => { let o = [0x14, 0x18, 0x1C].map(|a| m.r32(his_body + a)); FULL_BODY.set(Some((his_body, o))); o }
        };
        let shape = match (body, fit) { (Some(b), Some(_)) if !use_full => b.1, _ => own };
        for (a, v) in [0x14, 0x18, 0x1C].into_iter().zip(shape) { m.w32(his_body + a, v); }
    }
    if SIZE_SAID.replace(obj2) != obj2 {
        eprintln!("chao-player size: his land collision radius {:.2} height {:.2} centre {:.2} -> {:?} (Chao move work {:.2} {:.2})", f32::from_bits(full[0]), f32::from_bits(full[1]), f32::from_bits(full[2]), size.map(f32::from_bits), f(m, mw + CHAO_MOVE_RADIUS), f(m, mw + CHAO_MOVE_HEIGHT));
    }
}

thread_local! {
    static SIZE_SAID: Cell<u32> = const { Cell::new(0) };
    static WIN_DONE: Cell<usize> = const { Cell::new(0) };
}

/// A new stage (not a Chao World area change): the game has released the
/// last stage's memory, so the textures, models and motions this module
/// loaded there are gone. Forget them so they load again; drawing the gear
/// from the freed models hung the game after a level EXIT into Chao World
/// (frames stopped right after the new Chao's first draw).
/// Unverified: which allocations the stage change frees; this drops all of them.
fn drop_stage_loads() {
    knux_rig::forget_run_look();
    FULL_SIZE.set(None);
    FULL_BODY.set(None);
    RIDER_REST.set(None);
    RIDER_OBJ.set(0);
    GEAR_LOAD_AT.set(sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) + GEAR_LOAD_DELAY);
    SHADOW_TEX.set(None);
    KNUX_TEX.set(None);
    EXTRA_MODELS.with_borrow_mut(|e| e.clear());
    KNUX_MODELS.with_borrow_mut(|e| e.clear());
    SHADOW_MODELS.with_borrow_mut(|e| e.clear());
    BAKED_BOOT.set(None);
    HUB_SHOE.set(None);
    LIGHT_SHOE.set(None);
    emerald_orbit::forget();
    GLOVE_FIT.set(None);
    SHADOW_MOVES.set(false);
    BOARD.set(None);
    crate::card_box::drop_stage_loads();
}

/// Frames after a stage change before the gear files load. Loading Shadow's
/// and Knuckles's files right after a level EXIT, while Chao World was still
/// loading behind its card, hung the game in a main.dol loop (0x8013B82C..
/// 0x8013E8E0); with that load off the lobby came up (test build 5).
/// shortcut: a fixed delay, not the game's own "stage loaded" state (not
/// found yet); raise it if a slow load still collides.
const GEAR_LOAD_DELAY: u64 = 180;

/// ChaoStgLobby (GSNE8P module 65, ChaoStgLobby.prs): ALO_LobbyExitExecutor's
/// exec is .text 0x58C (fn_65_58C, set at 0xCA4 with CreateElementalTask).
/// Its mode 1 runs the game's "player hits my trigger" test (main.dol
/// 0x80005F88 with 0) and on a hit sets twp flag 0x100, locks the player
/// (0x80022EB8, 0x8001DD50) and opens message 49 ("Exit from Chao garden.
/// OK?", ChaoMain .text 0x1D11C/0x1D04C/0x1CF90), mode 4/5 wait for the
/// answer, and Yes calls ChaoMain .text 0x27DA0, the game's own way out of
/// Chao World (mode 6).
/// MMO glue: on the hit it goes straight to the Yes path, no window and no
/// lock, and start_stage takes the game on to its own Stage Select.
const LOBBY_MODULE: u32 = 65;
const LOBBY_EXIT_EXEC: u32 = 0x58C;
const HIT_PLAYER: u32 = 0x8000_5F88;
const LEAVE_CHAO_WORLD: u32 = 0x2_7DA0;
/// The exit trigger's radius plus the player's width (as tiny_transporter's PLAYER_R).
const EXIT_REACH: f32 = 1.8 + 2.0;

fn lobby_exit_hook(c: &mut Cpu, m: &mut Mem) {
    let at = if chao_world(m) && module_linked(m, LOBBY_MODULE) { rt::sec(m, LOBBY_MODULE, 1) + LOBBY_EXIT_EXEC } else { 0 };
    if at != 0 && EXIT_HOOKED.replace(at) != at {
        c.rt.set_override(at, lobby_exit_exec);
    }
}

fn lobby_exit_exec(c: &mut Cpu, m: &mut Mem) {
    let (task, at) = (c.r[3], rt::sec(m, LOBBY_MODULE, 1) + LOBBY_EXIT_EXEC);
    let twp = m.r32(task + 0x34);
    // SA2_EXIT_AT=frame (debug): the exit counts as walked into from that frame on.
    let debug = std::env::var("SA2_EXIT_AT").ok().and_then(|v| v.trim().parse::<u64>().ok()).is_some_and(|f| sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) >= f);
    // MMO glue: the game's own hit did not fire for the Chao standing in the
    // trigger (headless run, 2026-11-24; mob: the centre was hard to hit), so
    // the Chao within reach of the trigger's ball counts too: the ball's radius
    // (1.8, ChaoStgLobby .data +0x12870) and the player's width, measured flat
    // (the platform under it is solid, so the Chao there is on top of it).
    let own = own_twp(m);
    let reach = twp != 0 && own != 0 && {
        let d = |o: u32| f32::from_bits(m.r32(own + o)) - f32::from_bits(m.r32(twp + o));
        d(0x14).hypot(d(0x1C)) <= EXIT_REACH
    };
    if twp != 0 && m.r8(twp) == 1 && m.r16(twp + 4) & 0x100 == 0 && (debug || reach || call(c, m, HIT_PLAYER, &[task, 0]) != 0) {
        eprintln!("chao-player: lobby exit -> the game's Stage Select (by {})", if debug { "debug" } else if reach { "reach" } else { "the game's own hit" });
        m.w16(twp + 4, m.r16(twp + 4) | 0x100);
        m.w8(twp, 6);
        crate::start_stage::to_stage_select();
        call(c, m, rt::sec(m, 60, 1) + LEAVE_CHAO_WORLD, &[]);
        return;
    }
    c.r[3] = task;
    rel_original(c, m, at, lobby_exit_exec);
}

/// The game's allocator (main.dol 0x8011989C) takes its OS heap index from
/// the small-data word at r13 - 0x71B4 (29108) and passes it to
/// OSAllocFromHeap; the gear loads run with it set to heap 1.
pub(crate) const SY_HEAP_SDA: u32 = 29108;
pub(crate) const GEAR_HEAP: u32 = 1;

/// main.dol sound calls by use count: 0x8006B978 (257 callers), 0x8006B1FC
/// (Sonic's exec at 0x800846FC), 0x8006BB10, 0x8006C78C.
const SE_CALLS: [u32; 4] = [0x8006_B978, 0x8006_B1FC, 0x8006_BB10, 0x8006_C78C];

fn se_log(c: &mut Cpu, m: &mut Mem, at: u32) {
    if SE_LOG.get() || debug_set("selog").is_some_and(|v| v == "1") {
        eprintln!("se: {at:#x} id {:#x} r4 {:#x} lr {:#x} pmove {}", c.r[3], c.r[4], c.lr, IN_PMOVE.get());
    }
    if at == SE_CALLS[0] && (IN_PMOVE.get() || chao_voice(c.r[3]) || debug_muted(c.r[3])) {
        // shortcut: no sound slot reported to the caller (-1); the callers
        // seen (0x80080CFC) ignore it.
        c.r[3] = u32::MAX;
        return;
    }
    original(c, m, at);
}

thread_local! {
    static SE_LOG: Cell<bool> = Cell::new(std::env::var("SA2_SE_LOG").is_ok_and(|v| v == "1"));
}

/// A Chao voice sound id (the 0x60xx Chao bank).
/// shortcut: the whole 0x6000..0x60FF range is taken as Chao voices from the
/// ids seen in a lobby run (0x6014, 0x6015, 0x601C); check a garden by ear.
pub(crate) fn chao_voice(id: u32) -> bool {
    (0x6000..0x6100).contains(&id)
}

/// The character's exec is running on the Chao's body (player_move): its own
/// movement sounds are not played (start_stage's positional sound hooks).
pub(crate) fn character_sounds_muted() -> bool {
    IN_PMOVE.get()
}

thread_local! {
    static IN_PMOVE: Cell<bool> = const { Cell::new(false) };
    static EXIT_HOOKED: Cell<u32> = const { Cell::new(0) };
    static GEAR_LOAD_AT: Cell<u64> = const { Cell::new(0) };
    static LIVE_STAGE_NUM: Cell<u16> = const { Cell::new(u16::MAX) };
    static PMOVE_CHAO: Cell<u32> = const { Cell::new(0) };
    /// The Chao's place as last mirrored into the character's task work.
    static MIRRORED: Cell<Option<[f32; 3]>> = const { Cell::new(None) };
    /// The heading the character's exec left on the Chao's body (twp +0x08..).
    static CHAR_ANG: Cell<Option<[u32; 3]>> = const { Cell::new(None) };
    static AREA_DONE: Cell<bool> = const { Cell::new(false) };
    static LIVE_STAGE: Cell<u16> = const { Cell::new(u16::MAX) };
    static SELF_LOGGED: Cell<bool> = const { Cell::new(false) };
    static TASK_LOGGED: Cell<bool> = const { Cell::new(false) };
}

fn exec(c: &mut Cpu, m: &mut Mem, addr: u32) {
    LAST_EXEC.set(addr);
    let task = c.r[3];
    // In Chao World the area is ChaoMain's AL_GetStageNumber (.text 0x27FA0).
    let area = if chao_world(m) && module_linked(m, 60) { call(c, m, rt::sec(m, 60, 1) + 0x27FA0, &[]) as u16 } else { 0 };
    // Self mode only for a Chao made in this stage and area: on the first
    // frame after a change STEP still names the Chao of the stage just left
    // (from a level, on the ChaoMain copy at BASE), until the reset below.
    let me = self_drive(m) && LIVE_STAGE.get() == m.r16(STAGE_NUM) ^ (area << 8);
    if SELF_LOGGED.replace(me) != me {
        eprintln!("chao-drive: self mode {me} (stage {}, step {:?})", m.r16(STAGE_NUM), STEP.get());
    }
    let pmove = me && player_move_on();
    // SA2_WIN_AT=frame[,frame...] (debug, test only): once at each of those frames in a stage, the game's own
    // stage-clear call for player 0 (main.dol 0x8001C810: it starts the result screen
    // 0x8005DD88 / 0x8005ED8C), to test the result and the way back without playing to the goal.
    if !chao_world(m) && std::env::var("SA2_WIN_AT").ok().and_then(|v| v.split(',').nth(WIN_DONE.get())?.trim().parse::<u64>().ok()).is_some_and(|f| sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) >= f) {
        WIN_DONE.set(WIN_DONE.get() + 1);
        eprintln!("chao-player: debug stage clear (0x8001C810) in stage {}", m.r16(STAGE_NUM));
        let keep = c.r;
        call(c, m, 0x8001_C810, &[0]);
        c.r = keep;
    }
    // SA2_TASK_LOG=1 (debug): his task's words, once.
    if std::env::var("SA2_TASK_LOG").is_ok_and(|v| v == "1") && !TASK_LOGGED.replace(true) {
        eprintln!("task-log: character task {task:#x} words +0x00..+0x40 {:x?}", (0..0x40).step_by(4).map(|o| m.r32(task + o)).collect::<Vec<_>>());
    }
    if !me {
        c.r[3] = task;
        // Self mode in Chao World: the character is never seen or heard, also
        // before the Chao takes over (its landing sound on arrival, mob).
        let hidden = chao_world(m) && std::env::var("SA2_CHAO_SELF").is_ok_and(|v| v.trim() == "1");
        IN_PMOVE.set(hidden);
        original(c, m, addr);
        IN_PMOVE.set(false);
        // SA2_YAW_LOG=1 (debug): the visible character's own yaw, to compare.
        let own = m.r32(task + 0x34);
        if own != 0 && chao_world(m) && std::env::var("SA2_YAW_LOG").is_ok_and(|v| v == "1") {
            let f = |a: u32| f32::from_bits(m.r32(a));
            eprintln!("yaw: frame {} char {:#06x} native pos ({:.2},{:.2}) height {:.2} state {} flags {:#x}", sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed), m.r32(own + 0x0C) & 0xFFFF, f(own + 0x14), f(own + 0x1C), f(own + 0x18), m.r8(own), m.r16(own + 4));
        }
    } else if let (true, Step::Live(chao)) = (pmove, STEP.get()) {
        let pwp = m.r32(P1_OBJ2);
        if (0x8000_0000..0x8C00_0000).contains(&pwp) {
            PRE_VY.set(f32::from_bits(m.r32(pwp + 0x68)));
        }
        player_move(c, m, addr, task, chao);
    }
    // A new stage or Chao World area drops the Chao task:
    // create the player's Chao again.
    // Offline players' cached Chao leave after the grace period.
    crate::chao_cache::tick();
    if chao_world(m) && matches!(STEP.get(), Step::Live(_)) && module_linked(m, 60) {
        export_pose(m);
        export_record(m);
        take_skills(m);
        remote_tick(c, m);
        crate::name_tags::flush();
    }
    crate::kinder_chart::frame(c, m);
    takeoff_test(c, m);
    info_out(m, area);
    debug_teleport(m, task);
    staff_goto(c, m, task, u32::from(area));
    crate::card_box::frame(c, m);
    lobby_exit_hook(c, m);
    // SA2_LEAVE_AT=frame (debug): leave Chao World for the Stage Select once
    // at that frame, as the lobby exit does (to test the Stage Select headless).
    if chao_world(m) && module_linked(m, 60) && std::env::var("SA2_LEAVE_AT").ok().and_then(|v| v.trim().parse::<u64>().ok())
        .is_some_and(|f| sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) >= f) && !LEAVE_DONE.replace(true) {
        eprintln!("chao-player: debug leave -> the game's Stage Select");
        crate::start_stage::to_stage_select();
        call(c, m, rt::sec(m, 60, 1) + LEAVE_CHAO_WORLD, &[]);
    }
    // SA2_CHAO_AREA_AT=frame,area (debug): change Chao World area once at that
    // frame, with ChaoMain's own area change (.text 0x27E00, as start_stage
    // uses it; 7 = the lobby), to test the gear across a second change.
    if chao_world(m) && module_linked(m, 60) {
        if let Some((f, to)) = std::env::var("SA2_CHAO_AREA_AT").ok().and_then(|v| { let (f, a) = v.split_once(',')?; Some((f.trim().parse::<u64>().ok()?, a.trim().parse::<u32>().ok()?)) }) {
            if sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) >= f && !AREA_DONE.replace(true) {
                eprintln!("chao-player: debug area change to {to}");
                call(c, m, rt::sec(m, 60, 1) + 0x27E00, &[to]);
            }
        }
    }
    let stage = m.r16(STAGE_NUM) ^ (area << 8);
    if LIVE_STAGE.replace(stage) != stage {
        RIDER.set(0);
        let new_stage = LIVE_STAGE_NUM.replace(m.r16(STAGE_NUM)) != m.r16(STAGE_NUM);
        if new_stage {
            drop_stage_loads();
        }
        // A new stage settles for a second before the Chao is made (tick).
        // An area change inside Chao World does not need it (ChaoMain stays
        // linked): the second was an empty path on every arrival (mob 2026-11-23).
        if new_stage && matches!(STEP.get(), Step::Live(_) | Step::Wait(_)) {
            STEP.set(Step::Wait(0));
        } else if let Step::Live(_) = STEP.get() {
            STEP.set(Step::Wait(if chao_world(m) { 60 - area_settle() } else { 0 }));
        }
        // The game dropped the remote Chao's tasks too, without freeing their
        // blocks one by one (the remote heap only shrank across a garden ->
        // lobby change): their heap starts empty again for the new area.
        // Only when every remote task went through DestroyTask (destroy_task
        // clears its entry): a remote Chao made while the lobby was still
        // loading outlived the area step on arrival, its blocks were handed
        // out again, and its FreeTask on leaving crashed the task loop
        // ("no function at 0x10000004" right after "remote ... offline").
        // The card box's portrait Chao (extra_chao) lives there too.
        let kept = REMOTE.with_borrow(|r| r.values().filter(|e| e.task != 0).count()) + EXTRA_TASKS.with_borrow(|x| x.len());
        if kept == 0 {
            REMOTE_ALLOC.with_borrow_mut(|r| if let Some((reg, sizes)) = r.as_mut() {
                *reg = crate::chao_cache::Region::new(REMOTE_HEAP_START, REMOTE_HEAP_END);
                sizes.clear();
            });
        } else {
            eprintln!("chao-player remote: {kept} remote Chao outlived the area change; their blocks stay allocated");
        }
        GARDEN.set(None);
    }
    let mut data1 = m.r32(task + 0x34);
    if let (true, Step::Live(chao)) = (me, STEP.get()) {
        // The Chao is the player: it moves itself, and the character's task
        // work mirrors its facing and position (with the action state and
        // flags when the character's exec moved it), so camera and objects
        // that read playertwp[0] follow the Chao. playertwp[0] itself stays
        // the character's own: the Chao's task work is freed when the area
        // changes, which would leave playertwp[0] dangling.
        data1 = m.r32(chao + 0x34);
        let own = m.r32(task + 0x34);
        if data1 != 0 {
            if !pmove {
                crate::chao_drive::drive(c, m, chao, rt::sec(m, 60, 1));
            }
            if own != 0 {
                for o in (0x08..0x20).step_by(4) {
                    m.w32(own + o, m.r32(data1 + o));
                }
                if pmove {
                    MIRRORED.set(Some([0x14, 0x18, 0x1C].map(|o| f32::from_bits(m.r32(own + o)))));
                }
            }
        }
    }
    if data1 == 0 {
        return;
    }
    let pos = [m.r32(data1 + 0x14), m.r32(data1 + 0x18), m.r32(data1 + 0x1C)];
    if let Step::Live(chao) = STEP.get() {
        RIDER.set(data1);
        let lp = LAST_RIDER_POS.replace(pos);
        let d2: f32 = (0..3).map(|i| { let d = f32::from_bits(pos[i]) - f32::from_bits(lp[i]); d * d }).sum();
        MOVING.set(d2 > 0.01 * 0.01);
        if MOVING.get() {
            MOVED_AT.set(sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed));
        }
        // Chao World: the hub's gait (mmo-play src/chao-locomotion.js) with the
        // game's own motions: AL_motion_table 0 alm_tati (stand), 578
        // alm_aruki_ncz (walk), 119 alm_run (run); common-motions.ini.
        // shortcut: walk/run switch at 1.0 / 0.9 units a frame (hysteresis as
        // the hub's 1.05/0.95); not yet scaled to the hub's speed units.
        let old_level = !matches!(addr, SONIC_EXEC | KNUX_EXEC) || std::env::var("SA2_LEVEL_RETARGET").is_ok_and(|v| v == "1");
        if chao_world(m) || !old_level {
            let d = d2.sqrt();
            let last = LAST_MOTION.get();
            let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
            let on_board = !chao_world(m) && BOARD.with_borrow(|b| b.as_ref().is_some_and(|b| f <= b.0 + 1));
            // Chao World: swimming or in the air the Chao plays its own motion for it.
            let own_state = if me && !menu_area(area) { chao_world_state(c, m, task, chao, area) } else { (SWIM.set(None), AIR.set(false)); None };
            // mob 2026-11-24, Chao World with no shoes or the Light Shoes: the
            // look follows the hidden Sonic's own state (gait): the Chao's own
            // walk in his walk, its own run in his jog, and his run laid over
            // it only in his run. Speeds and change points are his.
            // A frame or two without ground covered (a turn on the spot, a
            // bump) is not a stop: the gait holds for 6 frames.
            let still = if d >= 0.01 { 0 } else { STILL.get().saturating_add(1) };
            STILL.set(still);
            let gait = ((me || addr == KNUX_EXEC) && own_state.is_none() && (d >= 0.01 || (GAIT.get().is_some() && still < 6)) && (addr == KNUX_EXEC || !shadow_style())).then(|| gait(u32::from(m.r16(m.r32(P1_OBJ2).wrapping_add(OBJ2_ACTION))))).flatten();
            GAIT.set(gait);
            let want = if let Some(mo) = own_state { mo } else if addr == KNUX_EXEC && OWN_MOTION.get() { last } else if let Some(g) = gait.filter(|_| !on_board) { g.motion() } else if addr == KNUX_EXEC { knux_rig::under_motion(still, last) } else if d < 0.01 || on_board { 0 } else if d >= if last == 119 { 0.9 } else { 1.0 } { 119 } else { 578 };
            if want != last {
                call(c, m, rt::sec(m, 60, 1) + AL_SET_MOTION_LINK, &[chao, want]);
                LAST_MOTION.set(want);
            }
            gait_log(m, chao, data1, [f32::from_bits(pos[0]) - f32::from_bits(lp[0]), f32::from_bits(pos[2]) - f32::from_bits(lp[2])]);
            stride_lock(m, chao);
        }
        pin(m, chao, data1);
        // SA2_TREADMILL=1 (test only): after each step he is put back where he
        // was when the switch first saw him, keeping his speed and state, so a
        // full run can be filmed for as long as wanted in a small room.
        if std::env::var("SA2_TREADMILL").is_ok_and(|v| v == "1") {
            let spot = TREADMILL.get().unwrap_or(pos);
            TREADMILL.set(Some(spot));
            let cw = m.r32(chao + 0x34);
            for (k, v) in spot.into_iter().enumerate() {
                if k != 1 {
                    m.w32(data1 + 0x14 + 4 * k as u32, v);
                    if cw != 0 { m.w32(cw + 0x14 + 4 * k as u32, v); }
                }
            }
            LAST_RIDER_POS.set([spot[0], m.r32(data1 + 0x18), spot[2]]);
        }
        if sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) % 300 == 0 {
            eprintln!("chao-player: type {:#x} analysed {} yaw rider {:#x} chao {:#x}", m.r8(INFO + 0x80), m.r8(INFO + 0x438), m.r32(data1 + 0x0C), m.r32(m.r32(chao + 0x34) + 0x0C));
        }
        // ChaoMain AL_DisableNade (.text 0x5A3C, = CCL_Disable(task, 2)):
        // the rider's B is its own move, not Pet (action 31). Each frame, as
        // the Chao's behaviours may enable touch again (AL_EnableTouch 0x5AB0).
        call(c, m, rt::sec(m, 60, 1) + AL_DISABLE_NADE, &[chao]);
        collision_fit(m, chao);
        if addr == KNUX_EXEC { knux_rig::no_touch(c, m, chao); }
        // The Chao's gear (Shadow's and Knuckles's textures and models, for the
        // Air Shoes, the ball and the Hammer Gloves), once per stage, in every
        // stage the Chao plays, whoever the hidden character is: each file is
        // loaded by this module into its own maps (SHADOW_MODELS, KNUX_MODELS),
        // never read from a model table slot the stage's own character filled.
        // Before 2026-11-23: Sonic's stages and Chao World only; Shadow's own
        // stages got no gear at all, Knuckles's and Rouge's the gloves alone.
        if (addr == SONIC_EXEC || addr == KNUX_EXEC) && SHADOW_TEX.get().is_none() && std::env::var("SA2_BALL_SH").map_or(true, |v| v != "0")
            && sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) >= GEAR_LOAD_AT.get() {
            // The gear goes in game heap 1: in heap 0 it took ~1.5 MB of the
            // lobby's 3.93 MB, leaving 0.05 MB (menus thread, build 7 heap log).
            let heap_var = c.r[13].wrapping_sub(SY_HEAP_SDA);
            let keep_heap = m.r32(heap_var);
            m.w32(heap_var, GEAR_HEAP);
            let tex = call(c, m, LOAD_TEX, &[SHADOW_TEX_NAME]);
            let mdl = load_models(c, m, SHADOW_MDL_NAME);
            SHADOW_MODELS.set(LAST_LOAD.with_borrow(|l| l.clone()));
            // Every TERIOSMDL model by slot with its node and vertex counts (to find his shoe models).
            // (The character's own file is already in the game's table: list that too.)
            let all: Vec<String> = (0..0x428u32).filter_map(|k| { let o = shadow_obj(m, k); (0x8000_0000..0x8C00_0000).contains(&o).then_some((k, o)) }).map(|(k, o)| {
                let r = crate::chao_retarget::read_chao(&|a| m.r32(a), o, [vec![], vec![]]);
                format!("{k}:{}n/{}v", r.nodes.len(), r.verts.iter().map(Vec::len).sum::<usize>())
            }).collect();
            eprintln!("chao-player: model slots {}", all.join(" "));
            eprintln!("chao-player: Shadow's textures {tex:#x} ({} entries), models {mdl:#x}", if tex != 0 { m.r32(tex + 4) } else { 0 });
            SHADOW_TEX.set(Some(tex).filter(|&t| t != 0));
            tex_log(m, "Shadow's", tex);
            if tex == 0 {
                GEAR_LOAD_AT.set(u64::MAX); // no retry every frame; the next stage tries again
            }
            // Knuckles's textures and models for the Hammer Gloves item (KNUCKTEX /
            // KNUCKMDL.PRS, main.dol 0x80174F04 / 0x80174F10).
            let ktex = call(c, m, LOAD_TEX, &[0x8017_4F04]);
            let kmdl = load_models(c, m, 0x8017_4F10);
            KNUX_MODELS.set(LAST_LOAD.with_borrow(|l| l.clone()));
            // Node and vertex counts name each model (Shadow's boot: 2 nodes, 174 vertices).
            let size = |o: u32| if (0x8000_0000..0x8C00_0000).contains(&o) { let r = crate::chao_retarget::read_chao(&|a| m.r32(a), o, [vec![], vec![]]); (r.nodes.len(), r.verts.iter().map(Vec::len).sum::<usize>()) } else { (0, 0) };
            eprintln!("chao-player: boot slots 161/163 -> {:#x} {:?} / {:#x} {:?} (table {:#x}/{:#x}), glove slots 329/331 -> {:#x} {:?} / {:#x} {:?}, Knuckles body 285 -> {:#x} {:?}",
                shadow_obj(m, 161), size(shadow_obj(m, 161)), shadow_obj(m, 163), size(shadow_obj(m, 163)), m.r32(MODELS + 4 * 161), m.r32(MODELS + 4 * 163),
                knux_obj(m, 329), size(knux_obj(m, 329)), knux_obj(m, 331), size(knux_obj(m, 331)), knux_obj(m, KNUX_BODY_SLOT), size(knux_obj(m, KNUX_BODY_SLOT)));
            m.w32(heap_var, keep_heap);
            eprintln!("chao-player: Knuckles's textures {ktex:#x}, models {kmdl:#x} (heap {GEAR_HEAP}, was {keep_heap})");
            KNUX_TEX.set(Some(ktex).filter(|&t| t != 0));
            tex_log(m, "Knuckles's", ktex);
            // SA2_MODEL_DUMP (debug): every loaded model slot with its node and vertex counts.
            if std::env::var_os("SA2_MODEL_DUMP").is_some() {
                for k in 0..0x428u32 {
                    let o = m.r32(MODELS + 4 * k);
                    if (0x8000_0000..0x8C00_0000).contains(&o) {
                        let r = crate::chao_retarget::read_chao(&|a| m.r32(a), o, [vec![], vec![]]);
                        eprintln!("model {k}: {o:#x} nodes {} verts {}", r.nodes.len(), r.verts.iter().map(Vec::len).sum::<usize>());
                    }
                }
            }
        }
        // Physics stays native for Knuckles and Rouge (mob's rules).
        if addr == SONIC_EXEC {
            physics_row(m);
            shadow_moves(c, m);
            force_sonic_state(m);
        } else if addr == KNUX_EXEC {
            knuckles_rules(c, m, chao);
            knux_rig::run_style(c, m);
        }
        return;
    }
    tick(c, m, pos);
}

/// playerwk m.action (the animation state, = PC AnimInfo.Current) and m.plactptr (mtnwk at +0x174 after look +0x144;
/// decomp player.h; +0xC0 is the physics row, matching 0x8004F838's copy).
/// The motion number is the PL_ACTION entry's actnum (0x10-byte entries).
const OBJ2_ACTION: u32 = 0x178;
const OBJ2_PLACTPTR: u32 = 0x198;

thread_local! {
    static LAST_ACT: Cell<u16> = const { Cell::new(u16::MAX) };
    /// The Chao plays its own motion: no retarget this frame.
    static OWN_MOTION: Cell<bool> = const { Cell::new(false) };
    /// The posed Chao flipped in its own x (punch 2).
    static MIRROR: Cell<bool> = const { Cell::new(false) };
    static ARM_FWD: Cell<f32> = const { Cell::new(0.0) };
}

/// mob's approved rules for Knuckles (coordinator 2026-11-21, 2026-11-24):
/// the Chao's own motion in his punch 1 and 2, glide and climb
/// (knux_rig::own_motion); every other state of his (punch 3, hammer, dig,
/// drill) is the Chao's own rig with the overlay from his skeleton, as in
/// Sonic's stages. Rouge's stages run Knuckles.
/// SA2_LEVEL_RETARGET=1 (debug): the old limb copy, arms biased 0.8 forward
/// in 111 / 138.
fn knuckles_rules(c: &mut Cpu, m: &mut Mem, chao: u32) {
    let obj2 = m.r32(P1_OBJ2);
    if !(0x8000_0000..0x8C00_0000).contains(&obj2) {
        return;
    }
    knux_rig::test_upgrades(m, obj2);
    let idx = u32::from(m.r16(obj2 + OBJ2_ACTION));
    let table = m.r32(obj2 + OBJ2_PLACTPTR);
    if !(0x8000_0000..0x8C00_0000).contains(&table) {
        return;
    }
    // The animation state (PC CharObj2 AnimInfo.Current; mob's numbers).
    // SA2_KNUX_STATE=n (debug): treat the rider as in state n, to check a rule.
    let act = std::env::var("SA2_KNUX_STATE").ok().filter(|_| forced_now()).and_then(|v| v.parse().ok()).unwrap_or(idx as u16);
    let own = knux_rig::own_motion(act);
    if LAST_ACT.replace(act) != act {
        eprintln!("chao-player knux: state {act} motion {} -> chao motion {own:?}", m.r16(table + 0x10 * idx));
        if let Some((mo, _)) = own {
            // Linked in over as many frames as his own row links this state in.
            let step = knux_rig::link_frames(f32::from_bits(m.r32(table + 0x10 * idx + 8)));
            call(c, m, rt::sec(m, 60, 1) + knux_rig::AL_SET_MOTION_LINK_STEP, &[chao, mo, step]);
            LAST_MOTION.set(mo);
        }
    }
    OWN_MOTION.set(own.is_some());
    knux_rig::SPIN.set(knux_rig::spins(act));
    MIRROR.set(own.is_some_and(|o| o.1));
    ARM_FWD.set(if matches!(act, 111 | 138) { 0.8 } else { 0.0 });
    if own.is_some_and(|o| o.0 == 582 || o.0 == 155) {
        knux_rig::punch_lock(m, chao, act);
    }
    knux_rig::log(m, chao, act);
}

/// SA2_SONIC_STATE=n[,flags] (debug): hold the rider in animation state n
/// (obj2 +0x178; 30 = ball, PLMOT_BALL) and OR `flags` (hex) into obj2
/// +0x24, the word Sonic's display (0x8007C770) tests with 0x40008 before
/// its efftex ball draw. No input needed to see the ball.
fn force_sonic_state(m: &mut Mem) {
    let Ok(v) = std::env::var("SA2_SONIC_STATE") else { return };
    if !forced_now() {
        return;
    }
    let obj2 = m.r32(P1_OBJ2);
    if !(0x8000_0000..0x8C00_0000).contains(&obj2) {
        return;
    }
    let mut it = v.split(',');
    if let Some(n) = it.next().and_then(|n| n.trim().parse::<u16>().ok()) {
        m.w16(obj2 + OBJ2_ACTION, n);
    }
    if let Some(f) = it.next().and_then(|f| u32::from_str_radix(f.trim().trim_start_matches("0x"), 16).ok()) {
        m.w32(obj2 + 0x24, m.r32(obj2 + 0x24) | f);
    }
}

/// Sonic's and Shadow's action tables in main.dol (PL_ACTION, 0x10-byte
/// rows: motion u16, model u16, mode u16, next u16, blend f32, speed f32);
/// Sonic's loader stores Sonic's at playerwk +0x198.
const SONIC_ACTIONS: u32 = 0x801B_762C;
const SHADOW_ACTIONS: u32 = 0x801B_82DC;
const ACTION_ROWS: u32 = 160;
/// The game's motion file load by name (Sonic's loader at 0x80087640) into
/// the motion table 0x801E5D28 (8-byte entries, motion at +4).
const LOAD_MTN: u32 = 0x8005_0094;
const MOTIONS: u32 = 0x801E_5D28;
const SHADOW_MTN_NAME: u32 = 0x8017_4918; // "TERIOSMTN.PRS"
/// Sonic's table with Shadow's rows where Shadow plays his own motion (Chao RAM).
const MIXED_ACTIONS: u32 = 0x82F0_2000;

thread_local! {
    static SHADOW_MOVES: Cell<bool> = const { Cell::new(false) };
    static SHADOW_STYLE: Cell<Option<bool>> = const { Cell::new(None) };
}

// ---- Chao World: swimming, the jump's look and flying (mob 2026-11-23) ----
// The hidden character still moves the body (player_move); these change what
// the Chao plays and, in flight, his fall and forward speed.

/// Garden water levels by ChaoMain's stage number (AL_CheckWater .text
/// 0x48964, .rodata 0x9308..0x9310): Neutral, Hero, Dark. No other area has water.
const WATER_LEVEL: [(u16, f32); 3] = [(1, -2.2), (2, -1.5), (3, -1.0)];
/// AL_CheckWater: in the water when the level is above the Chao's spot by more than this (.rodata 0x9318).
const WATER_DEPTH: f32 = 2.0;
/// The swim behaviour holds the Chao this far below the level (fn_60_482DC, .rodata 0x92CC).
const SWIM_HEIGHT: f32 = 2.1;
/// The Chao's swim motion by its Swim skill (fn_60_47A50 and the race's
/// fn_76_CB54: below ChaoGlobal +310 (300) motion 142, below +312 (2000) 148, else 145).
const SWIM_MOTIONS: [u32; 3] = [142, 148, 145];
/// The Chao's motion in the air: 130, what the race plays when a Chao leaves
/// a ledge and while it flies (ChaoStgRace fn_76_CA0C / fn_76_D1A4).
/// shortcut: no separate native jump motion was identified; upgrade when one is.
const AIR_MOTION: u32 = 130;
/// The race's swim (ChaoStgRace fn_76_C6EC): forward acceleration 0.0027 +
/// 0.018 s/10000 against a drag of 0.06 of the speed, s = the Swim points
/// (top speed 0.045 a frame untrained, 0.345 at 9999).
const SWIM_ACC: f32 = 0.0027;

/// A skill of the player's own Chao as the race reads it: the points in its
/// own record (task work +0x5C, u16 at +0x38 + 2 x skill; 0 Swim, 1 Fly),
/// which is what ChaoMain AL_ParameterGetSkill (.text 0x20934) returns in the
/// race. (In the gardens and the lobby that function returns three times the
/// points; the race's rules are used here with the race's own input.)
/// SA2_CHAO_SWIM / SA2_CHAO_FLY=points (test only): that value instead; the
/// record is not touched.
fn own_skill(m: &Mem, chao: u32, skill: u32) -> u16 {
    if let Some(v) = std::env::var(["SA2_CHAO_SWIM", "SA2_CHAO_FLY"][skill as usize & 1]).ok().and_then(|v| v.trim().parse().ok()) {
        return v;
    }
    let rec = m.r32(m.r32(chao + 0x34) + 0x5C);
    let v = if (0x8000_0000..0x8C00_0000).contains(&rec) { m.r16(rec + 0x38 + 2 * skill) } else { 0 };
    if !SKILL_SAID.replace(true) {
        eprintln!("chao-player: the Chao's own record {rec:#x}: Swim {} points (level {}), Fly {} points (level {})", m.r16(rec + 0x38), m.r8(rec + 0x30), m.r16(rec + 0x3A), m.r8(rec + 0x31));
    }
    v
}

/// One frame of the race's swim speed: pushed on, it gains the skill's
/// acceleration against the drag; let go, only the drag acts.
pub(crate) fn swim_step(v: f32, pushed: bool, skill: f32) -> f32 {
    v * 0.94 + if pushed { SWIM_ACC + 0.018 * skill / 10000.0 } else { 0.0 }
}
const CHAO_GLOBAL: u32 = 0x2A50; // ChaoMain .data

/// A flag the pad read sees too (the game reads the pad on another thread than its tasks run on).
struct Flag(std::sync::atomic::AtomicBool);
impl Flag {
    const fn new() -> Self { Flag(std::sync::atomic::AtomicBool::new(false)) }
    fn get(&self) -> bool { self.0.load(std::sync::atomic::Ordering::Relaxed) }
    fn set(&self, v: bool) { self.0.store(v, std::sync::atomic::Ordering::Relaxed) }
    fn replace(&self, v: bool) -> bool { self.0.swap(v, std::sync::atomic::Ordering::Relaxed) }
}
/// The hidden character is off the ground (Chao World, self mode).
static AIR: Flag = Flag::new();
/// Jump was let go since leaving the ground; the fly press is held; that press is still down.
static AIR_RELEASED: Flag = Flag::new();
static FLY: Flag = Flag::new();
static FLY_MASK: Flag = Flag::new();

thread_local! {
    /// In garden water: the water level.
    static SWIM: Cell<Option<f32>> = const { Cell::new(None) };
    /// Flight speed (forward, up) by the race's rule; his own fall per frame, measured while he is in the air.
    static FLY_V: Cell<Option<(f32, f32)>> = const { Cell::new(None) };
    static HIS_GRAVITY: Cell<f32> = const { Cell::new(0.0) };
    static PRE_VY: Cell<f32> = const { Cell::new(0.0) };
    static FLY_SEEN: Cell<(u8, u16)> = const { Cell::new((0, 0)) };
    static CLIMB: Cell<(u32, Option<f32>)> = const { Cell::new((0, None)) };
    static LAST_FLAT: Cell<Option<[f32; 2]>> = const { Cell::new(None) };
    static SWIM_V: Cell<f32> = const { Cell::new(0.0) };
    static SKILL_SAID: Cell<bool> = const { Cell::new(false) };
    static UP_FOR: Cell<u32> = const { Cell::new(0) };
    static CLIMB_V: Cell<f32> = const { Cell::new(0.0) };
    static FLY_T: Cell<Option<(u32, u32)>> = const { Cell::new(None) };
    static FLY_Y: Cell<Option<f32>> = const { Cell::new(None) };
    static CLIMB_WIN: Cell<(u32, [f32; 2], f32, bool)> = const { Cell::new((0, [0.0; 2], 0.0, false)) };
}

/// Pushed against a pool wall: frames before the lift starts, the flat
/// headway a frame under which the body counts as stopped, the lift a frame
/// (as a part of the swim speed), and how far above the water it may go.
const CLIMB_AFTER: u32 = 8;
const CLIMB_FREE: u32 = 20;
const CLIMB_WINDOW: u32 = 8;
const FALL_FRAMES: u32 = 8;
const CLIMB_STUCK: f32 = 0.5; // of the swim speed
const CLIMB_RATE: f32 = 0.3;
const CLIMB_TOP: f32 = 6.0;

/// One frame of the way out of a walled pool. Before the lift: (frames
/// stopped at a wall, None). Lifting: (frames of headway in a row, the height
/// the body is held at or above). It starts only in the water, after
/// CLIMB_AFTER stopped frames; each stopped frame lifts a step, and with
/// headway the height is held (a slanted wall lets him on a little at each
/// height). It ends when the stick is let go, after CLIMB_FREE frames of
/// headway (over the bank), or at CLIMB_TOP over the water.
pub(crate) fn climb_step(was: (u32, Option<f32>), level: Option<f32>, swimming: bool, pushed: bool, stopped: bool, y: f32) -> (u32, Option<f32>) {
    let Some(level) = level else { return (0, None) };
    if !pushed {
        return (0, None);
    }
    match was.1 {
        None if !(swimming && stopped) => (0, None),
        None if was.0 + 1 < CLIMB_AFTER => (was.0 + 1, None),
        None => (0, Some(y + CLIMB_RATE)),
        Some(to) => {
            let (free, to) = if stopped { (0, to + CLIMB_RATE) } else { (was.0 + 1, to) };
            if free >= CLIMB_FREE || to > level + CLIMB_TOP { (0, None) } else { (free, Some(to)) }
        }
    }
}

/// Chao World areas the player does not walk in, only the game's own menus
/// run there: the race (4), its entry hall (5), the stadium (9) and Karate
/// (10, 11). The hidden character has no floor there, so he counted as in the
/// air and every confirm press was held back as the flight's second press.
pub(crate) fn menu_area(area: u16) -> bool {
    matches!(area, 4 | 5 | 9 | 10 | 11)
}

pub(crate) fn water_level(area: u16) -> Option<f32> {
    WATER_LEVEL.iter().find(|w| w.0 == area).map(|w| w.1)
}

pub(crate) fn in_water(y: f32, level: f32) -> bool {
    level > y + WATER_DEPTH
}

pub(crate) fn swim_motion(skill: u32, low: u32, mid: u32) -> u32 {
    SWIM_MOTIONS[if skill < low { 0 } else if skill < mid { 1 } else { 2 }]
}

/// The garden reads a skill as three times its points and lets a Chao fly at
/// 400 (ChaoMain fn_60_4BC10): 134 points.
const GARDEN_FLY_POINTS: u16 = 134;
const GARDEN_FLY_TOP: f32 = 100.0;

/// Which flight the second press starts. mob 2026-11-24: the garden's
/// take-off for every Chao, held level after its climb for the lift time.
/// SA2_FLY_MODE=race: the race's glide for everyone; =gate: the take-off only
/// from GARDEN_FLY_POINTS, the glide under that; =garden: the take-off with
/// the game's own sink (no level hold).
pub(crate) fn garden_flier(points: u16) -> bool {
    match std::env::var("SA2_FLY_MODE").as_deref() {
        Ok("race") => false,
        Ok("gate") => points >= GARDEN_FLY_POINTS,
        _ => true,
    }
}

/// The best Fly a Chao can reach by the game's rules (S grade, level 99, three reincarnations).
const BEST_POINTS: f32 = 3266.0;

/// MMO rule (mob 2026-11-24), not the game's: the strong-lift time of the player's take-off grows with its Fly
/// points, 300 frames at 0 to 600 at BEST_POINTS (the game draws 300..600 at random for a garden Chao).
/// SA2_FLY_LIFT=frames (test only): that time instead, to film the hand-over to the glide.
pub(crate) fn lift_frames(points: f32) -> u32 {
    if let Some(n) = std::env::var("SA2_FLY_LIFT").ok().and_then(|v| v.trim().parse().ok()) {
        return n;
    }
    (300.0 + 300.0 * points.clamp(0.0, BEST_POINTS) / BEST_POINTS) as u32
}

/// One frame of the garden's flight (ChaoMain fn_60_3D978): it starts at 0.4
/// forward and 0.35 up; forward gains 0.05 against a drag of 0.1 (0.5 a
/// frame); the lift cancels 0.95 of the Chao's gravity `g` while its timer
/// runs and 0.92 after. (The garden Chao does not steer; the player's does.)
pub(crate) fn garden_fly_step(v: Option<(f32, f32)>, timer_runs: bool, g: f32) -> (f32, f32) {
    let Some(v) = v else { return (0.4, 0.35) };
    (v.0 * 0.9 + 0.05, v.1 - g * (1.0 - if timer_runs { 0.95 } else { 0.92 }))
}

/// One frame of the race's flight (ChaoStgRace fn_76_C500): forward
/// acceleration 0.018 + 0.067 s/10000, sink 0.01 + 0.005 (10000 - s)/10000,
/// drag 0.1 on the forward speed and 0.05 on the vertical; s = the Fly skill.
pub(crate) fn fly_step(v: (f32, f32), skill: f32) -> (f32, f32) {
    let s = skill.clamp(0.0, 10000.0);
    let (acc, sink) = (0.018 + 0.067 * s / 10000.0, 0.01 + 0.005 * (10000.0 - s) / 10000.0);
    (v.0 * 0.9 + acc, v.1 * 0.95 - sink)
}

const PAD_BUTTON_A: u16 = 0x0100;

/// Controller 1 as the game is to read it. In the air in Chao World the
/// second press of jump starts the Chao's flight and is held back from the
/// game, so the character's own second-press move (homing attack) never
/// fires; letting go ends the flight. The held-back press stays held back
/// after landing until it is let go (it would be a new jump).
pub(crate) fn pad_filter(mut p: sa2_asset_tools::input::PadStatus) -> sa2_asset_tools::input::PadStatus {
    let a = p.button & PAD_BUTTON_A != 0;
    if !a {
        FLY.set(false);
        FLY_MASK.set(false);
        if AIR.get() {
            AIR_RELEASED.set(true);
        }
    } else if AIR.get() && AIR_RELEASED.get() {
        if !FLY.replace(true) && std::env::var("SA2_FLY_LOG").is_ok_and(|v| v == "1") {
            eprintln!("fly: second press in the air: held back from the game, flight on");
        }
        FLY_MASK.set(true);
    }
    if !AIR.get() {
        AIR_RELEASED.set(false);
        FLY.set(false);
    }
    if FLY_MASK.get() {
        p.button &= !PAD_BUTTON_A;
    }
    p
}

/// Chao World, self mode, after the hidden character moved the body: is the
/// Chao swimming or in the air, and the motion it plays then (None: the usual
/// stand / walk / run). In flight his speed is set by the race's rule.
fn chao_world_state(_c: &mut Cpu, m: &mut Mem, task: u32, chao: u32, area: u16) -> Option<u32> {
    let (own, cw, pwp) = (m.r32(task + 0x34), m.r32(chao + 0x34), m.r32(P1_OBJ2));
    if own == 0 || cw == 0 {
        (SWIM.set(None), AIR.set(false));
        return None;
    }
    let f = |m: &Mem, a: u32| f32::from_bits(m.r32(a));
    let level = water_level(area).filter(|&l| in_water(f(m, cw + 0x18), l));
    // Swimming goes at the Chao's own Swim skill, by the race's rule
    // (swim_step): what the character moved this frame is cut to that speed,
    // back along the way he came, and his own speed is set to it.
    let now = [f(m, cw + 0x14), f(m, cw + 0x1C)];
    let was = LAST_FLAT.replace(Some(now)).unwrap_or(now);
    let moved = ((now[0] - was[0]).powi(2) + (now[1] - was[1]).powi(2)).sqrt();
    let pushed = { let s = crate::pad::port0().stick; i32::from(s[0]).pow(2) + i32::from(s[1]).pow(2) > 30 * 30 };
    let swim_v = if level.is_some() {
        let v = swim_step(SWIM_V.get(), pushed, f32::from(own_skill(m, chao, 0)));
        if moved > v && moved > 0.0 {
            let k = v / moved;
            let to = [was[0] + (now[0] - was[0]) * k, was[1] + (now[1] - was[1]) * k];
            m.w32(cw + 0x14, to[0].to_bits());
            m.w32(cw + 0x1C, to[1].to_bits());
            LAST_FLAT.set(Some(to));
        }
        if (0x8000_0000..0x8C00_0000).contains(&pwp) && f(m, pwp + 0x64) > v {
            m.w32(pwp + 0x64, v.to_bits());
        }
        if std::env::var("SA2_FLY_LOG").is_ok_and(|v| v == "1") {
            eprintln!("swim: f{} skill {} speed {v:.4} (he moved {moved:.4}) motion {}", sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed), own_skill(m, chao, 0), swim_motion(u32::from(own_skill(m, chao, 0)), 300, 2000));
        }
        v
    } else {
        0.0
    };
    SWIM_V.set(swim_v);
    // Out of a walled pool (the Hero garden): the hidden character walks the
    // pool floor and its wall stopped him, so the Chao could only jump out.
    // Pushed against a wall while swimming, the body is lifted up the wall
    // until it passes over the bank.
    // MMO glue, not the game: a garden Chao has no such wall to pass (its own
    // collision is not the character collision used here).
    // Stopped = no headway the way he faces (his yaw goes along (cos, sin)):
    // sliding along the wall does not count as getting on.
    let yaw = (m.r32(cw + 0x0C) & 0xFFFF) as f32 * std::f32::consts::TAU / 65536.0;
    let headway = (now[0] - was[0]) * yaw.cos() + (now[1] - was[1]) * yaw.sin();
    // (Lifted above the water he no longer swims: the speed he swam at still sets what counts as headway.)
    if level.is_some() {
        CLIMB_V.set(swim_v.max(SWIM_ACC));
    }
    // Judged over CLIMB_WINDOW frames: too little headway in sum, or too little
    // ground covered (an untrained swimmer is so slow that at a wall his
    // heading flips from frame to frame and single frames show headway).
    let here = LAST_FLAT.get().unwrap_or(now);
    let (n, from, sum, mut stopped) = CLIMB_WIN.get();
    let (n, sum) = (n + 1, sum + headway);
    if n >= CLIMB_WINDOW {
        let need = CLIMB_STUCK * CLIMB_V.get() * CLIMB_WINDOW as f32;
        stopped = sum < need || ((here[0] - from[0]).powi(2) + (here[1] - from[1]).powi(2)).sqrt() < need;
        CLIMB_WIN.set((0, here, 0.0, stopped));
    } else {
        CLIMB_WIN.set((n, from, sum, stopped));
    }
    let climb = climb_step(CLIMB.get(), water_level(area), level.is_some(), pushed, stopped, f(m, cw + 0x18));
    if climb.1.is_some() != CLIMB.get().1.is_some() {
        eprintln!("chao-player: {} (height {:.2}, at {:.1} {:.1})", if climb.1.is_some() { "at a pool wall: up the bank" } else { "off the pool wall" }, f(m, cw + 0x18), f(m, cw + 0x14), f(m, cw + 0x1C));
    }
    CLIMB.set(climb);
    if let Some(y) = climb.1.filter(|&to| to > f(m, cw + 0x18)) {
        m.w32(cw + 0x18, y.to_bits());
        if (0x8000_0000..0x8C00_0000).contains(&pwp) {
            m.w32(pwp + 0x68, 0);
        }
    }
    let climbing = climb.1.is_some();
    if SWIM.replace(level).is_some() != level.is_some() {
        eprintln!("chao-player: {} (area {area}, height {:.2}, at {:.1} {:.1})", if level.is_some() { "into the water: swimming" } else { "out of the water" }, f(m, cw + 0x18), f(m, cw + 0x14), f(m, cw + 0x1C));
    }
    // His status flags (task work +4): bit 0 on the ground (0x1 standing, 0x400 in a jump, seen in the lobby).
    // In the air: his jump (state 6) at once; any other time off the ground
    // only after FALL_FRAMES, as uneven ground (the Neutral garden) drops him
    // for a few frames at a time, which is not a fall to play the air motion for.
    let up = level.is_none() && !climbing && m.r16(own + 4) & 1 == 0 && m.r8(own) != 0;
    let up_for = if up { UP_FOR.get() + 1 } else { 0 };
    UP_FOR.set(up_for);
    let air = up && (m.r8(own) == 6 || AIR.get() || up_for >= FALL_FRAMES);
    if std::env::var("SA2_FLY_LOG").is_ok_and(|v| v == "1") && FLY_SEEN.replace((m.r8(own), m.r16(own + 4))) != (m.r8(own), m.r16(own + 4)) {
        eprintln!("fly: his state {} flags {:#x} (pad A {}, fly {})", m.r8(own), m.r16(own + 4), crate::pad::port0().button & PAD_BUTTON_A != 0, FLY.get());
    }
    if AIR.replace(air) != air && std::env::var("SA2_FLY_LOG").is_ok_and(|v| v == "1") {
        eprintln!("fly: {} (his state {}, flags {:#x})", if air { "in the air" } else { "landed" }, m.r8(own), m.r16(own + 4));
    }
    let speed = |m: &Mem| (0x8000_0000..0x8C00_0000).contains(&pwp).then(|| (f(m, pwp + 0x64), f(m, pwp + 0x68)));
    if air && !FLY.get() {
        // His own fall per frame, for the flight to cancel.
        if let Some((_, vy)) = speed(m) {
            let g = PRE_VY.get() - vy;
            if (0.0..1.0).contains(&g) {
                HIS_GRAVITY.set(g);
            }
            if std::env::var("SA2_FLY_LOG").is_ok_and(|v| v == "1") {
                eprintln!("air: f{} height {:.2} his speed ({:.3}, {:.3}, {:.3}) before his step up {:.3}", sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed), f(m, cw + 0x18), f(m, pwp + 0x64), vy, f(m, pwp + 0x6C), PRE_VY.get());
            }
        }
    }
    if air && FLY.get() {
        if let Some(now) = speed(m) {
            let skill = f32::from(own_skill(m, chao, 1));
            let v = if garden_flier(skill as u16) {
                // The garden's take-off (garden_fly_step), its timer drawn as the game draws it.
                // One take-off for each time in the air: letting go and pressing
                // again goes on from his speed, with no second kick and the same timer.
                let again = FLY_T.get().is_some();
                let (n, timer) = match FLY_T.get() {
                    Some((n, t)) => (n + 1, t),
                    None => (0, lift_frames(skill)),
                };
                FLY_T.set(Some((n, timer)));
                let mw = m.r32(chao + 0x38);
                let g = if mw != 0 { -f(m, mw + 0x48) } else { 0.055 };
                // mob 2026-11-24: when the strong lift ends the flight goes
                // straight on as the race's glide by the same Fly points (the
                // garden's own weaker lift, 0.92, is not used for the player).
                let from = FLY_V.get().or(again.then_some(now));
                let v = if n < timer { garden_fly_step(from, true, g) } else { fly_step(from.unwrap_or(now), skill) };
                // MMO rule (mob 2026-11-24), not the game's: at the top of the kick's climb the height is held
                // level until the lift time is up, so Fly counts from flat ground. SA2_FLY_MODE=garden: the game's own sink.
                let v = if n < timer && v.1 < 0.0 && !std::env::var("SA2_FLY_MODE").is_ok_and(|m| m == "garden") { (v.0, 0.0) } else { v };
                if f(m, cw + 0x18) > GARDEN_FLY_TOP {
                    m.w32(cw + 0x18, GARDEN_FLY_TOP.to_bits());
                }
                v
            } else {
                fly_step(FLY_V.get().unwrap_or(now), skill)
            };
            FLY_V.set(Some(v));
            // His next step takes his own fall off before it moves: given back
            // here, and what his step still got wrong last frame (his fall is
            // not the same every frame) is made up, so the height follows the
            // flight's own speed exactly (the take-off climbed 15 units where
            // the garden Chao climbs 22).
            let y = f(m, cw + 0x18);
            let miss = FLY_Y.get().map_or(0.0, |want| (want - y).clamp(-0.5, 0.5));
            FLY_Y.set(Some(y + miss + v.1));
            m.w32(pwp + 0x64, v.0.to_bits());
            m.w32(pwp + 0x68, (v.1 + miss + HIS_GRAVITY.get()).to_bits());
            // (The Chao's own fall is cleared in pin, so only the race's sink acts.)
            if std::env::var("SA2_FLY_LOG").is_ok_and(|v| v == "1") {
                eprintln!("fly: f{} skill {skill} speed forward {:.3} up {:.3} (his fall {:.3}) height {:.2} yaw {:#06x} at ({:.1}, {:.1}) lift {:?}", sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed), v.0, v.1, HIS_GRAVITY.get(), f(m, cw + 0x18), m.r32(cw + 0x0C) & 0xFFFF, f(m, cw + 0x14), f(m, cw + 0x1C), FLY_T.get());
            }
        }
    } else {
        FLY_V.set(None);
        FLY_Y.set(None);
    }
    if !air {
        FLY_T.set(None);
    }
    if level.is_some() || climbing {
        let g = rt::sec(m, 60, 5) + CHAO_GLOBAL;
        return Some(swim_motion(u32::from(own_skill(m, chao, 0)), u32::from(m.r16(g + 310)), u32::from(m.r16(g + 312))));
    }
    // SA2_AIR_MOTION=n (debug): another Chao motion in the air.
    air.then(|| std::env::var("SA2_AIR_MOTION").ok().and_then(|v| v.parse().ok()).unwrap_or(AIR_MOTION))
}

/// The Chao's movement follows the shoes it wears (mob 2026-11-23), in Chao
/// World and off the board in Sonic's levels, where the hidden character is
/// Sonic: with Shadow's Air Shoes shown it moves like Shadow (skate), with
/// only Sonic's Light Shoes or no shoes it runs like Sonic. Both pairs on:
/// the Air Shoes count. Switched live, by the action table his exec reads.
/// SA2_LOBBY_CHAR=sonic|shadow (debug) forces one.
fn shadow_style() -> bool {
    match std::env::var("SA2_LOBBY_CHAR").as_deref() {
        Ok("sonic") => false,
        Ok("shadow") => true,
        _ => !std::env::var("SA2_AIR_SHOES").is_ok_and(|v| v == "0") && !debug_off("shoes"),
    }
}

/// Moving like Shadow (physics stays Metal's row): every state whose Shadow
/// row plays a different motion number takes Shadow's row (motion, blend,
/// speed; Sonic's model field kept), from Shadow's own motion file loaded
/// once, into the gear's heap (Chao World's own heap has no room).
fn shadow_moves(c: &mut Cpu, m: &mut Mem) {
    let obj2 = m.r32(P1_OBJ2);
    if !(0x8000_0000..0x8C00_0000).contains(&obj2) || m.r8(obj2 + 1) != 0 || std::env::var("SA2_SHADOW_MOVES").is_ok_and(|v| v == "0") {
        return;
    }
    if !SHADOW_MOVES.replace(true) {
        let rows: Vec<u32> = (0..ACTION_ROWS).filter(|&i| m.r16(SONIC_ACTIONS + 0x10 * i) != m.r16(SHADOW_ACTIONS + 0x10 * i)).collect();
        let need: Vec<u32> = rows.iter().map(|&i| u32::from(m.r16(SHADOW_ACTIONS + 0x10 * i))).collect();
        // Keep every motion Sonic already has; take only Shadow's own ones.
        let keep: Vec<u32> = (0..0x960).step_by(4).map(|o| m.r32(MOTIONS + o)).collect();
        let heap_var = c.r[13].wrapping_sub(SY_HEAP_SDA);
        let keep_heap = m.r32(heap_var);
        m.w32(heap_var, GEAR_HEAP);
        let mtn = call(c, m, LOAD_MTN, &[SHADOW_MTN_NAME]);
        m.w32(heap_var, keep_heap);
        for (k, o) in (0..0x960u32).step_by(4).enumerate() {
            if !need.contains(&(o / 8)) {
                m.w32(MOTIONS + o, keep[k]);
            }
        }
        for o in (0..0x10 * ACTION_ROWS).step_by(4) {
            m.w32(MIXED_ACTIONS + o, m.r32(SONIC_ACTIONS + o));
        }
        for &i in &rows {
            let (d, s) = (MIXED_ACTIONS + 0x10 * i, SHADOW_ACTIONS + 0x10 * i);
            m.w16(d, m.r16(s));
            for o in [4, 6, 8, 0xC] {
                m.w32(d + o, m.r32(s + o));
            }
        }
        eprintln!("chao-player: Shadow's motions {mtn:#x}; states {rows:?} -> motions {need:?}");
    }
    let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
    let board = BOARD.with_borrow(|b| b.as_ref().is_some_and(|b| f <= b.0 + 2));
    let shadow = shadow_style();
    if SHADOW_STYLE.replace(Some(shadow)) != Some(shadow) {
        eprintln!("chao-player: the Chao moves like {} (Air Shoes shown: {shadow})", if shadow { "Shadow" } else { "Sonic" });
    }
    m.w32(obj2 + OBJ2_PLACTPTR, if board || !shadow { SONIC_ACTIONS } else { MIXED_ACTIONS });
}

/// SA2_FORCE_FROM=frame (debug): forced states start at this frame, so the
/// rider has landed and the camera settled first.
fn forced_now() -> bool {
    let from = std::env::var("SA2_FORCE_FROM").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
    sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) >= from
}

/// Physics table (0x84 bytes a row) that 0x8004F838 copies to the player's
/// obj2 +0xC0; player 1 obj2 pointer (walker.rs, native).
const PHYSICS: u32 = 0x8019_6578;
const P1_OBJ2: u32 = 0x801E_7728;
/// mob 2026-11-21: off the board mob's Chao moves with Metal Sonic's row;
/// on the board Sonic's own row as the game set it.
/// Row = the character id byte obj2+3 (0x8004F838 multiplies it by 0x84);
/// Metal Sonic's loader 0x80086BA4 stores 0xC there (Sonic's 0x800874C0: 0).
const METAL_ROW: u32 = 12;
const ROW_JUMP_SPEED: u32 = 0x18;

fn physics_row(m: &mut Mem) {
    let obj2 = m.r32(P1_OBJ2);
    if !(0x8000_0000..0x8C00_0000).contains(&obj2) {
        return;
    }
    let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
    let board = BOARD.with_borrow(|b| b.as_ref().is_some_and(|b| f <= b.0 + 2));
    // mob 2026-11-21: the board has its own physics. The game's row is
    // whatever it last wrote there (Metal's is ours), kept for the board.
    let now: Vec<u32> = (0..0x84).step_by(4).map(|o| m.r32(obj2 + 0xC0 + o)).collect();
    let metal = PHYSICS + 0x84 * METAL_ROW;
    let metal: Vec<u32> = (0..0x84).step_by(4).map(|o| m.r32(metal + o)).collect();
    let own = SONIC_ROW.with_borrow_mut(|r| {
        if now != metal && r.as_ref() != Some(&now) {
            if r.is_some() {
                eprintln!("chao-player: the game set a new physics row at frame {f} (board {board})");
            }
            *r = Some(now);
        }
        r.clone().unwrap_or_default()
    });
    // mob 2026-11-24: in Chao World the ground speeds, acceleration and the
    // walk / jog / run change points are Sonic's own: his row as the game set
    // it. SA2_CHAO_SPEED=metal (test) is Metal Sonic's row as before, =run the
    // old scaling by Run points. Levels keep Metal's row off the board.
    let metal = match (chao_world(m), std::env::var("SA2_CHAO_SPEED").as_deref()) {
        (true, Ok("metal")) | (false, _) => metal,
        (true, Ok("run")) => crate::chao_speed::row(m, INFO, &own, &metal),
        (true, _) => if own.len() == metal.len() { own.clone() } else { metal },
    };
    // mob 2026-11-23: every Chao jumps as Sonic does; only mob's own Chao has
    // Metal Sonic's jump. The jump is the row's jump speed (+0x18: Sonic 1.66,
    // Metal Sonic 2.52; the hold time +0x00 and hold lift +0x38 are the same in
    // both rows). The client is told nothing about whose account it is, so
    // Metal's jump is behind SA2_CHAO_JUMP=metal (test only) until it is.
    let metal_jump = std::env::var("SA2_CHAO_JUMP").is_ok_and(|v| v.trim() == "metal");
    for (i, o) in (0..0x84).step_by(4).enumerate() {
        let his = board || (o == ROW_JUMP_SPEED && !metal_jump && own.len() > i);
        // (With Sonic's row in Chao World the Metal jump is Metal's own row's jump speed.)
        let v = if o == ROW_JUMP_SPEED && metal_jump { m.r32(PHYSICS + 0x84 * METAL_ROW + o) } else if his { own[i] } else { metal[i] };
        m.w32(obj2 + 0xC0 + o, v);
    }
}

thread_local! {
    static SONIC_ROW: std::cell::RefCell<Option<Vec<u32>>> = const { std::cell::RefCell::new(None) };
}

/// Frames an area change inside Chao World waits before the Chao is made again.
const AREA_SETTLE: u32 = 3;
/// SA2_AREA_WAIT=frames (debug) sets it.
fn area_settle() -> u32 {
    std::env::var("SA2_AREA_WAIT").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(AREA_SETTLE).min(60)
}

/// The ChaoMain load/create steps, driven once a frame from Sonic's exec in a
/// stage or from his cutscene draw in an event; `pos` is where to create it.
fn tick(c: &mut Cpu, m: &mut Mem, pos: [u32; 3]) {
    let next = match STEP.get() {
        // let the stage settle for a second before loading
        Step::Wait(n) if n < 60 => Step::Wait(n + 1),
        // Leaving by the lobby exit: no new Chao while the game goes to its menu.
        Step::Wait(_) if crate::start_stage::to_stage_select_pending() => Step::Wait(60),
        Step::Wait(_) if chao_world(m) && module_linked(m, 60) && new_player_waiting() => {
            // No Chao is made for a new player (no default Chao): the hooks go in
            // so the hatched Chao is seen in its exec, and that Chao becomes the player.
            hook(c, m);
            match HATCHED.replace(0) {
                0 => Step::Wait(60),
                chao => {
                    eprintln!("chao-player: the hatched Chao {chao:#x} (slot {:#x}) is the player", m.r32(m.r32(chao + 0x34) + 0x5C));
                    HATCH_TAKEN.set(true);
                    call(c, m, LOAD_BANK, &[WALKER_BANK_2P]);
                    Step::Live(chao)
                }
            }
        }
        Step::Wait(_) if chao_world(m) => {
            if module_linked(m, 60) { create(c, m, pos) } else { Step::Wait(60) }
        }
        // A level restarted with the copy at BASE still linked: no second
        // read or link; its world task again if that was destroyed too.
        // Unverified: that the copy's texture lists survive a restart.
        Step::Wait(_) if !linked_copies(m, &[BASE]).is_empty() => {
            if BASE_TASK_GONE.replace(false) {
                call(c, m, rt::sec(m, 60, 1) + 0x29344, &[]); // ALW_Create
            }
            create(c, m, pos)
        }
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
        Step::ReadingMotions => setup_and_create(c, m, pos),
        Step::Live(chao) => Step::Live(chao),
        Step::Failed => Step::Failed,
    };
    if next == Step::Failed && STEP.get() != Step::Failed {
        no_chao();
    }
    if next != STEP.get() && !matches!(next, Step::Wait(_)) {
        eprintln!("chao-player: {next:x?} (frame {})", sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed));
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

fn setup_and_create(c: &mut Cpu, m: &mut Mem, pos: [u32; 3]) -> Step {
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
    // Only the Chao's own lists (AL_MINI_PARTS_TEX .. AL_COMMON_TEX): with
    // AL_TEX_COMMON, AL_MINIMAL_TEX, AL_OBJECT and AL_TOY too, Wild Canyon's
    // textures overflow and it crashes in the texture lookup 0x8011C76C
    // (frame 464; 7 lists run, 8 crash). The garden's objects, toys and
    // animals are not needed on a rider.
    // unverified: that no Chao part draws from AL_TEX_COMMON (crops look right).
    // SA2_CHAO_TEXSET=i,j,.. (debug): load only these lists.
    let set: Vec<usize> = std::env::var("SA2_CHAO_TEXSET").ok().map(|v| v.split(',').filter_map(|x| x.trim().parse().ok()).collect()).unwrap_or_else(|| (0..7).collect());
    for &(name, texlist) in set.iter().filter_map(|&i| TEXTURES.get(i)) {
        call(c, m, s1 + 0x28C50, &[s4 + name, s5 + texlist, 0]);
    }
    call(c, m, s1 + 0x1FA10, &[]);
    call(c, m, s1 + 0x29344, &[]); // ALW_Create
    create(c, m, pos)
}

/// Chao World (stage 90 and up) has ChaoMain, its motions, textures and
/// ALW loaded by the game itself: only the player's Chao is created there,
/// on the game's own module (rt::sec finds it), no second copy.
pub(crate) fn module_linked(m: &Mem, id: u32) -> bool {
    std::iter::successors(Some(m.r32(rt::OS_MODULE_HEAD)), |&i| Some(m.r32(i + 4)))
        .take_while(|&i| i != 0)
        .take(256)
        .any(|i| m.r32(i) == id)
}

/// The page's /dev/area code 252: the player's Chao could not be made, so
/// the page goes back to the login instead of ever showing Sonic.
/// /dev/area code for "no Chao": 251/252 are the shoe pickups (main.js).
const PAGE_NO_CHAO: u8 = 250;

fn no_chao() {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().write(true).open("/dev/area") {
        let _ = f.write_all(&[PAGE_NO_CHAO]);
    }
}

/// SA2_MOTION_LOG (debug): every AL_SetMotionLink(task, motion) a Chao makes,
/// with its caller, to read the native walk/run motion numbers from the game.
fn motion_log(c: &mut Cpu, m: &mut Mem) {
    let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
    eprintln!("motion f{f}: task {:#x} motion {} lr {:#x} (sec1 +{:#x})", c.r[3], c.r[4], c.lr, c.lr.wrapping_sub(rt::sec(m, 60, 1)));
    let a = rt::sec(m, 60, 1) + AL_SET_MOTION_LINK;
    rel_original(c, m, a, motion_log);
}

/// The game left a level in-process (start_stage restarts Chao World): take
/// the copies loaded at BASE/MOT off the module list, so the ChaoMain the game
/// loads next is the only module 60 (two gave "no function at 0x81201310",
/// lr inside BASE). Unverified: nothing else still points into the copies.
pub(crate) fn unlink_copies(c: &mut Cpu, m: &mut Mem) {
    for base in linked_copies(m, &[BASE, MOT]) {
        eprintln!("chao-player: unlinking the copy at {base:#x}");
        call(c, m, sa2_recomp::os::OS_UNLINK, &[base]);
    }
    STEP.set(Step::Wait(0));
}

/// DestroyTask (main.dol 0x80071AF0, GSNE8P symbols). A level restart after
/// the player's deaths destroys the tasks and runs the stage task again with
/// the same stage number, so the stage check in exec never saw it: STEP kept
/// naming the freed Chao task, its work was written every frame into memory
/// the game had handed to new tasks, and the task loop later called heap data
/// (Big Foot combat: "no function at 0x80d74ac0", lr 0x80070928).
const DESTROY_TASK: u32 = 0x8007_1AF0;
/// Task exec pointer (task +0x10).
const TASK_EXEC: u32 = 0x10;

thread_local! {
    /// A task running the ChaoMain copy at BASE (the copy's ALW world task)
    /// was destroyed: the copy's setup must create it again.
    static BASE_TASK_GONE: Cell<bool> = const { Cell::new(false) };
}

fn destroy_task(c: &mut Cpu, m: &mut Mem) {
    let t = c.r[3];
    if let Step::Live(chao) = STEP.get() {
        if t == chao {
            eprintln!("chao-player: the game destroyed the player's Chao task {t:#x} at frame {}; making it again", sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed));
            // Chao World (an area change): made again after AREA_SETTLE frames, not a second.
            STEP.set(Step::Wait(if chao_world(m) { 60 - area_settle() } else { 0 }));
            RIDER.set(0);
        }
    }
    REMOTE.with_borrow_mut(|r| r.values_mut().filter(|e| e.task == t).for_each(|e| e.task = 0));
    EXTRA_TASKS.with_borrow_mut(|x| x.remove(&t));
    let exec = m.r32(t + TASK_EXEC);
    if !linked_copies(m, &[BASE]).is_empty() && (BASE..BSS).contains(&exec) {
        BASE_TASK_GONE.set(true);
    }
    c.r[3] = t;
    original(c, m, DESTROY_TASK);
}

/// Which of `bases` are OSModuleInfo addresses on the OS module list.
fn linked_copies(m: &Mem, bases: &[u32]) -> Vec<u32> {
    std::iter::successors(Some(m.r32(rt::OS_MODULE_HEAD)), |&i| Some(m.r32(i + 4)))
        .take_while(|&i| i != 0)
        .take(256)
        .filter(|i| bases.contains(i))
        .collect()
}

/// A new player's run before their egg's Chao is theirs. After the takeover
/// an area change makes the Chao again as for any player: create() finds it
/// on the card as the first hatched Chao.
fn new_player_waiting() -> bool {
    !HATCH_TAKEN.get() && crate::start_stage::hatch_slot().is_some()
}

/// The overrides the player's Chao runs through.
fn hook(c: &mut Cpu, m: &mut Mem) {
    let s1 = rt::sec(m, 60, 1);
    if c.rt.overrides.contains_key(&(s1 + CHAO_EXEC)) {
        return;
    }
    c.rt.set_override(s1 + AL_BEHAVIOR_CONTROL, behavior);
    c.rt.set_override(s1 + CHAO_DISPLAY, chao_display);
    c.rt.set_override(s1 + CHAO_EXEC, chao_exec);
    c.rt.set_override(s1 + AL_IS_ON_SCREEN3, on_screen3);
    c.rt.set_override(s1 + AL_NODE_TRANSFORM, chao_node);
    if std::env::var_os("SA2_MOTION_LOG").is_some() {
        c.rt.set_override(s1 + AL_SET_MOTION_LINK, motion_log);
    }
    for (a, f) in AL_WALKERS.iter().zip([w0 as rt::Func, w1, w2, w3, w4, w5]) {
        c.rt.set_override(s1 + a, f);
    }
}

fn create(c: &mut Cpu, m: &mut Mem, pos: [u32; 3]) -> Step {
    // The player's own Chao from their card: the first hatched Chao (type >= 2,
    // CHAO_PARAM_GC +0x80) in the loaded Chao save, which ChaoMain fn_60_2C30
    // walks as (.text 0xDFA0)() + 0xA64 + slot * 0x800, (.text 0xCC98)() slots.
    // Copied, so the card's own Chao is never changed by the lobby.
    // No Chao on the card: no default Chao (and never Sonic); the page goes
    // back to the login (create returns Failed -> no_chao).
    zero(m, INFO, 0x800);
    let s1 = rt::sec(m, 60, 1);
    if !chao_world(m) {
        // Story stages (no Chao save loaded): the Dark Chaos as before.
        m.w8(INFO + 0x80, 0x16);
        m.w8(INFO + 0x438, 1);
        m.w8(INFO + 0x84, 0xFF);
        return create_from_info(c, m, pos);
    }
    let save = call(c, m, s1 + 0xDFA0, &[]).wrapping_add(0xA64);
    let slots = call(c, m, s1 + 0xCC98, &[]).min(64);
    let Some(own) = (0..slots).map(|i| save + i * 0x800).find(|&p| m.r8(p + 0x80) >= 2) else {
        // shortcut: live cards hold no game save yet; until the server profile's
        // Chao is mapped in (open question), the profile's Dark Chaos type is used.
        // MMO glue: SA2_OWN_RECORD_IN is the server's copy of the player's own
        // record (a Chao hatched in the lobby is kept there, not on the card).
        if let Some(bytes) = std::env::var("SA2_OWN_RECORD_IN").ok().and_then(|p| std::fs::read(p).ok()).filter(|b| b.len() == 0x800 && b[0x80] >= 2) {
            m.load(INFO, &bytes);
            eprintln!("chao-player: no save Chao on the card; the server's record, type {:#x}", bytes[0x80]);
            return create_from_info(c, m, pos);
        }
        // SA2_CARD_CHAO_ONLY=1: no save Chao -> back to the login.
        if std::env::var("SA2_CARD_CHAO_ONLY").is_ok_and(|v| v == "1") {
            eprintln!("chao-player: no hatched Chao on the card ({slots} slots); back to the login");
            return Step::Failed;
        }
        // SA2_CHAO_TO_SAVE=1 (one-off, the owner's save): the Chao is made in a
        // free save slot of the game's own (ChaoMain fn_60_2C30), so the game's
        // own card save keeps it from then on.
        if std::env::var("SA2_CHAO_TO_SAVE").is_ok_and(|v| v == "1") {
            let slot = call(c, m, s1 + 0x2C30, &[]);
            if slot != 0 {
                eprintln!("chao-player: making the Chao in save slot {slot:#x}");
                m.w8(slot + 0x80, 0x16);
                m.w8(slot + 0x438, 1);
                m.w8(slot + 0x84, 0xFF);
                return create_at(c, m, pos, slot);
            }
        }
        eprintln!("chao-player: no save Chao on the card ({slots} slots); profile Dark Chaos type");
        m.w8(INFO + 0x80, 0x16);
        m.w8(INFO + 0x438, 1);
        m.w8(INFO + 0x84, 0xFF);
        return create_from_info(c, m, pos);
    };
    for o in (0..0x800).step_by(4) {
        m.w32(INFO + o, m.r32(own + o));
    }
    eprintln!("chao-player: the card's Chao at {own:#x}, type {:#x}", m.r8(INFO + 0x80));
    create_from_info(c, m, pos)
}

fn create_from_info(c: &mut Cpu, m: &mut Mem, pos: [u32; 3]) -> Step {
    create_at(c, m, pos, INFO)
}

fn create_at(c: &mut Cpu, m: &mut Mem, pos: [u32; 3], param: u32) -> Step {
    for (o, w) in pos.into_iter().enumerate() {
        m.w32(POS + 4 * o as u32, w);
    }
    let s1 = rt::sec(m, 60, 1);
    let chao = call(c, m, s1 + 0x3148C, &[param, 0, 0, POS, 0]);
    if chao == 0 {
        eprintln!("chao-player: CreateChaoExtra returned 0");
        return Step::Failed;
    }
    let s1 = rt::sec(m, 60, 1);
    hook(c, m);
    // In a cutscene (no rider) the bank load is skipped: suspected of the
    // ev15 stall right after Live. shortcut: no Chao voice in the intro yet.
    if RIDER.get() != 0 || EV_SPOT.get().is_none() {
        call(c, m, LOAD_BANK, &[WALKER_BANK_2P]);
    }
    call(c, m, s1 + AL_SET_MOTION_LINK, &[chao, STAND_MOTION]);
    Step::Live(chao)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_solid_objects_push_is_taken_and_a_placement_is_not() {
        // Stood on the lobby exit's pad: lifted 0.58 to its top after his exec.
        let s = super::push_step([1.0, 2.51, 2.0], [1.0, 3.09, 2.0]).unwrap();
        assert!(s[0] == 0.0 && (s[1] - 0.58).abs() < 1e-5 && s[2] == 0.0);
        assert_eq!(super::push_step([1.0, 2.51, 2.0], [1.0, 2.51, 2.0]), None, "not moved");
        assert_eq!(super::push_step([1.0, 2.51, 2.0], [80.0, 0.0, 80.0]), None, "placed by the game");
    }

    #[test]
    fn garden_water_and_swim_motion_as_the_game_decides() {
        use super::{in_water, swim_motion, water_level};
        assert_eq!(water_level(1), Some(-2.2), "Neutral garden");
        assert_eq!(water_level(7), None, "the lobby has no water");
        assert!(in_water(-4.3, -2.2) && !in_water(-4.1, -2.2), "two below the level");
        assert_eq!([swim_motion(0, 300, 2000), swim_motion(300, 300, 2000), swim_motion(2000, 300, 2000)], [142, 148, 145]);
    }

    #[test]
    fn flight_settles_at_the_races_speeds() {
        // Drag against a steady push: forward acceleration / 0.1, sink step / 0.05.
        let mut v = (0.0f32, 1.0f32);
        for _ in 0..600 { v = super::fly_step(v, 0.0); }
        assert!((v.0 - 0.18).abs() < 1e-3 && (v.1 + 0.3).abs() < 1e-3, "{v:?}");
        let mut w = (0.0f32, 0.0f32);
        for _ in 0..600 { w = super::fly_step(w, 10000.0); }
        assert!(w.0 > v.0 && w.1 > v.1, "a better flier is faster and sinks less: {w:?}");
    }

    #[test]
    fn garden_flight_climbs_then_comes_down_as_measured_in_the_garden() {
        // Chao gravity 0.055: the game's own flight rose to 21.7 units in 1.8 s from flat ground.
        let (mut v, mut y, mut top, mut n) = (super::garden_fly_step(None, true, 0.055), 0.0f32, 0.0f32, 0);
        while y >= 0.0 && n < 2000 {
            y += v.1;
            top = top.max(y);
            v = super::garden_fly_step(Some(v), true, 0.055);
            n += 1;
        }
        assert!((top - 22.0).abs() < 1.0 && (240..300).contains(&n), "top {top} after {n} frames");
        assert!((v.0 - 0.5).abs() < 1e-3);
        // after the timer it comes down faster
        assert!(super::garden_fly_step(Some((0.5, 0.0)), false, 0.055).1 < super::garden_fly_step(Some((0.5, 0.0)), true, 0.055).1);
        assert!(super::garden_flier(0));
        assert_eq!([0.0, 1633.0, 3266.0, 9999.0].map(super::lift_frames), [300, 450, 600, 600]);
    }

    #[test]
    fn swim_speed_settles_at_the_races_speed_for_the_skill() {
        let top = |skill: f32| (0..400).fold(0.0, |v, _| swim_step(v, true, skill));
        assert!((top(0.0) - 0.045).abs() < 1e-4, "{}", top(0.0));
        assert!((top(9999.0) - 0.345).abs() < 1e-3, "{}", top(9999.0));
        // let go: it dies away
        assert!((0..200).fold(0.345, |v, _| swim_step(v, false, 9999.0)) < 1e-4);
        // the motion by the same points: under 300, under 2000, above
        assert_eq!([0, 299, 300, 1999, 2000, 9999].map(|s| swim_motion(s, 300, 2000)), [142, 142, 148, 148, 145, 145]);
    }

    #[test]
    fn stopped_at_a_pool_wall_the_body_is_lifted_over_the_bank() {
        // Hero garden numbers: floor -6.5, water -1.5, bank 0.
        let w = Some(-1.5);
        // swimming freely, or stopped without pushing: nothing
        assert_eq!(climb_step((0, None), w, true, true, false, -6.5), (0, None));
        assert_eq!(climb_step((5, None), w, true, false, true, -6.5), (0, None));
        // stopped at the wall: waits, then rises a step a frame, on above the water
        let (mut s, mut y) = ((0, None), -6.5f32);
        for _ in 0..CLIMB_AFTER + 24 {
            s = climb_step(s, w, in_water(y, -1.5), true, true, y);
            y = s.1.map_or(y, |to| to.max(y));
        }
        assert!(y > 0.0 && y <= -1.5 + CLIMB_TOP, "{y}");
        // headway: the height is held, and after CLIMB_FREE frames of it the lift ends
        let held = s.1;
        s = climb_step(s, w, false, true, false, y);
        assert_eq!(s, (1, held));
        for _ in 1..CLIMB_FREE { s = climb_step(s, w, false, true, false, y); }
        assert_eq!(s, (0, None));
        // never starts out of the water, or where there is none
        assert_eq!(climb_step((CLIMB_AFTER, None), w, false, true, true, 0.0), (0, None));
        assert_eq!(climb_step((CLIMB_AFTER, None), None, true, true, true, 0.0), (0, None));
        // held at the top without getting over, or the stick let go: ends
        assert_eq!(climb_step((0, Some(-1.5 + CLIMB_TOP)), w, false, true, true, 4.5), (0, None));
        assert_eq!(climb_step((0, Some(0.0)), w, false, false, true, 0.0), (0, None));
    }

    #[test]
    fn the_hidden_character_takes_the_chaos_size_and_nothing_else() {
        // Knuckles 4.0 / 11.4 / 5.7 with a Chao of 1.5 / 3.0: centre at the same share of the height.
        let s = super::fitted_size(1.5, 3.0, None, [4.0, 11.4, 5.7]).unwrap();
        assert_eq!((s[0], s[1]), (1.5, 3.0));
        assert!((s[2] - 1.5).abs() < 1e-5);
        // With the Chao's own body collision centre (2.0): that height.
        assert_eq!(super::fitted_size(1.5, 3.0, Some(2.0), [4.0, 11.4, 5.7]).unwrap()[2], 2.0);
        assert!((super::fitted_size(1.5, 3.0, Some(9.0), [4.0, 11.4, 5.7]).unwrap()[2] - 1.5).abs() < 1e-5);
        // Not a size: left as it is.
        assert_eq!(super::fitted_size(0.0, 3.0, None, [4.0, 11.4, 5.7]), None);
        assert_eq!(super::fitted_size(9.0, 3.0, None, [4.0, 11.4, 5.7]), None);
        assert_eq!(super::fitted_size(1.5, 30.0, None, [4.0, 11.4, 5.7]), None);
    }
    #[test]
    fn the_menu_areas_have_no_swim_or_flight() {
        // Lobby, the three gardens and the kindergarten are walked in.
        assert!([0, 1, 2, 3, 6].iter().all(|&a| !super::menu_area(a)));
        assert!([4, 5, 9, 10, 11].iter().all(|&a| super::menu_area(a)));
    }

    #[test]
    fn second_jump_press_in_the_air_is_the_flight() {
        use super::{pad_filter, AIR, FLY, PAD_BUTTON_A};
        let pad = |a: bool| { let mut p = sa2_asset_tools::input::PadStatus::default(); if a { p.button = PAD_BUTTON_A; } p };
        AIR.set(false);
        assert_eq!(pad_filter(pad(true)).button, PAD_BUTTON_A, "the jump press reaches the game");
        AIR.set(true);
        assert_eq!(pad_filter(pad(true)).button, PAD_BUTTON_A, "still the same press");
        assert_eq!(pad_filter(pad(false)).button, 0);
        assert_eq!(pad_filter(pad(true)).button, 0, "the second press is held back");
        assert!(FLY.get());
        AIR.set(false);
        assert_eq!(pad_filter(pad(true)).button, 0, "held through the landing: not a new jump");
        assert!(!FLY.get());
        assert_eq!(pad_filter(pad(false)).button, 0);
        assert_eq!(pad_filter(pad(true)).button, PAD_BUTTON_A, "a new press on the ground jumps");
    }

    #[test]
    fn fist_two_is_fist_ones_mirror() {
        use super::{mirror_fist, mul34, tune_matrix, tune_values};
        // Two unlike fits, as the two arms give (each its own rest frame and size).
        let d1 = mul34(&tune_matrix(&[-1.2, 0.8, 0.3, 20.0, -35.0, 80.0, 0.4]), &tune_matrix(&[0.1, 0.0, 0.2, 5.0, 0.0, 0.0, 1.0]));
        let d2 = tune_matrix(&[1.3, 0.7, 0.25, -15.0, 40.0, -95.0, 0.45]);
        let t1 = tune_matrix(&[0.3, 1.26, -2.2, -60.0, 10.0, 0.0, 1.35]);
        let t2 = mirror_fist(&d1, &t1, &d2);
        let (p1, p2) = (mul34(&d1, &t1), mul34(&d2, &t2));
        // A plate vertex g on fist 1 and its twin (x, y, -z) on fist 2 land on mirrored spots.
        for g in [[0.0f32, 0.0, 0.0], [1.0, 2.0, 3.0], [-0.5, 0.25, -4.0]] {
            let a = super::apply(&p1, g);
            let b = super::apply(&p2, [g[0], g[1], -g[2]]);
            assert!((a[0] + b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4 && (a[2] - b[2]).abs() < 1e-4, "{a:?} {b:?}");
        }
        // The mirror is a plain tuner line (turn and one scale): written out and read back, it is the same matrix.
        let back = tune_matrix(&tune_values(&t2));
        for i in 0..3 { for j in 0..4 { assert!((back[i][j] - t2[i][j]).abs() < 1e-4, "{i} {j}: {} {}", back[i][j], t2[i][j]); } }
    }

    #[test]
    fn chao_yaw_faces_the_way_the_character_goes() {
        for a in [0u32, 0x1234, 0x4000, 0xA01F, 0xFFFF] {
            let t = |b: u32| b as f32 * std::f32::consts::TAU / 65536.0;
            let (p, c) = (t(a), t(super::chao_yaw(a)));
            assert!((p.cos() - c.sin()).abs() < 1e-3 && (p.sin() - c.cos()).abs() < 1e-3, "{a:#x}");
        }
    }
    #[test]
    fn look_at_puts_the_target_ahead_on_minus_z() {
        let v = super::look_at([0.0, 0.0, 10.0], [0.0, 0.0, 0.0]);
        let p = |q: [f32; 3]| -> [f32; 3] { std::array::from_fn(|i| v[i][0] * q[0] + v[i][1] * q[1] + v[i][2] * q[2] + v[i][3]) };
        let near = |a: [f32; 3], b: [f32; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-4);
        assert!(near(p([0.0, 0.0, 0.0]), [0.0, 0.0, -10.0]));
        assert!(near(p([1.0, 2.0, 0.0]), [1.0, 2.0, -10.0])); // x right, y up
    }
    #[test]
    fn stride_lock_takes_the_short_way_round() {
        let near = |a: f32, b: f32| (a - b).abs() < 1e-5;
        assert!(near(super::locked_phase(0.2, 0.6, 1.0), 0.6));
        assert!(near(super::locked_phase(0.2, 0.6, 0.0), 0.2));
        // 0.9 -> 0.1 goes forward through the wrap, not back through 0.5.
        assert!(near(super::locked_phase(0.9, 0.1, 0.25), 0.95));
        assert!(near(super::locked_phase(0.1, 0.9, 0.25), 0.05));
    }
    #[test]
    fn shoe_strips_keeps_only_strips_inside_the_set() {
        // material (type 16, 2 shorts), UV strip chunk (type 65): strip A
        // over 1,2,3 (kept), strip B over 1,9,3 (dropped), end.
        let list: Vec<u16> = vec![0x0010, 2, 0xAAAA, 0xBBBB, 0x0041, 21, 2,
            3, 1, 0, 0, 2, 0, 0, 3, 0, 0,
            3, 1, 0, 0, 9, 0, 0, 3, 0, 0, 0x00FF];
        let r16 = |a: u32| list[(a / 2) as usize];
        let keep: std::collections::BTreeSet<u32> = [1, 2, 3].into();
        let (out, kept, dropped) = shoe_strips(&r16, 0, &keep);
        assert_eq!((kept, dropped), (1, 1));
        assert_eq!(out, vec![0x0010, 2, 0xAAAA, 0xBBBB, 0x0041, 11, 1, 3, 1, 0, 0, 2, 0, 0, 3, 0, 0, 0x00FF]);
    }

    use super::*;

    #[test]
    fn staff_goto_lines_are_an_area_and_maybe_a_spot() {
        assert_eq!(parse_goto("7 1 1.5 0 -20"), Some((7, 1, Some([1.5, 0.0, -20.0]))));
        // Area 0 is the lobby, ChaoMain's area 7.
        assert_eq!(parse_goto("1 0 1 2 3\n"), Some((1, crate::start_stage::LOBBY, Some([1.0, 2.0, 3.0]))));
        assert_eq!(parse_goto("2 10"), Some((2, 10, None)));
        for bad in ["", "3", "3 13", "3 -1", "3 1 2", "3 1 2 3 4 5", "3 1 x 2 3", "3 1 inf 2 3", "3 1 NaN 2 3"] {
            assert_eq!(parse_goto(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn skills_are_raised_to_the_servers_and_never_lowered() {
        // Tiny Garden training: the same level and progress, more points.
        assert_eq!(raised((50, 40, 900), (50, 40, 960)), (50, 40, 960));
        // Tiny Garden fruit: a level-up with its progress reset.
        assert_eq!(raised((50, 90, 900), (51, 5, 915)), (51, 5, 915));
        // The game grew since the server last saw the record: nothing is undone.
        assert_eq!(raised((51, 5, 915), (50, 90, 900)), (51, 5, 915));
    }

    #[test]
    fn own_save_chao_fill_the_garden_to_eight_and_players_take_their_places() {
        // Alone: the player's Chao and 7 of their own others.
        assert!((0..7).all(|rank| idle_has_place(rank, 0)));
        assert!(!idle_has_place(7, 0));
        // 3 other players: 4 places left.
        assert!(idle_has_place(3, 3));
        assert!(!idle_has_place(4, 3));
        // 7 or more other players: none.
        assert!(!idle_has_place(0, 7));
        assert!(!idle_has_place(0, 50));
    }
    use crate::chao_retarget::ChaoRest;

    #[test]
    fn pose_in_lines_are_posed_or_idle() {
        assert_eq!(parse_idle("42 idle"), Some(42));
        assert_eq!(parse_idle("42 1 2 3 0 1 0 0"), None);
        assert_eq!(parse_remote("42 idle"), None);
        assert_eq!(parse_remote("42 1 2 3 9 1 0 0"), Some((42, [1.0, 2.0, 3.0], 9, 1)));
    }

    #[test]
    fn mirror_pairs_left_right_and_keeps_winding() {
        let i3 = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
        // root, a midline head, a left and a right hand
        let rest = ChaoRest {
            nodes: vec![(-1, [0.; 3], i3), (0, [0., 2., 0.], i3), (0, [1., 1., 0.], i3), (0, [-1., 1., 0.], i3)],
            verts: vec![vec![]; 4],
            arm_dirs: [vec![], vec![]],
        };
        assert_eq!(mirror_partners(&rest), vec![0, 1, 3, 2]);
        let l = [[0.0, -1.0, 0.0, 3.0], [1.0, 0.0, 0.0, 4.0], [0.0, 0.0, 1.0, 5.0]];
        let r = reflect_x(&l);
        assert_eq!(reflect_x(&r), l);
        assert_eq!([r[0][3], r[1][3], r[2][3]], [-3.0, 4.0, 5.0]);
        let det = |a: &[[f32; 4]; 3]| a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1]) - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0]) + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]);
        assert_eq!(det(&r), det(&l));
    }
}


// ---- Other players' Chao (MMO glue): pose export/import, draw-only. ----

/// The pose record, one JSON line, as the server relays it ("chao-pose"
/// action; others get it as players[i].chaoPose):
/// {"x","y","z": world position, "angle": yaw BAMS 0..65535, "motion": the
/// Chao motion last set with AL_SetMotionLink (1 = stand while it copies
/// the rider's pose), "frame": 0.0, "gear": bits 1 shoes, 2 gloves, 4 ball}.
/// Written about 10 times a second to SA2_POSE_OUT (a file; the browser page
/// passes a device path and sends each line over the MMO websocket).
const EXPORT_EVERY: u64 = 6;

thread_local! {
    static LAST_MOTION: Cell<u32> = const { Cell::new(STAND_MOTION) };
    static REMOTE: std::cell::RefCell<std::collections::BTreeMap<u64, Remote>> = Default::default();
    static IMPORT_AT: Cell<u64> = const { Cell::new(0) };
}

struct Remote {
    /// The game task, 0 while it has none (far away or after an area change).
    task: u32,
    /// (time received, position, yaw BAMS, motion)
    samples: std::collections::VecDeque<(std::time::Instant, [f32; 3], u16, u32)>,
    last_seen: std::time::Instant,
    motion: u32,
    /// Distance rank among the live remote Chao (0 = nearest).
    rank: usize,
    /// Outside the view at its last display: no exec, no draw.
    offscreen: bool,
    /// Its player is away in Tiny Chao Garden: the Chao lives here on the
    /// game's own Chao AI (no pose to follow, its exec runs every frame).
    idle: bool,
}

/// Remote Chao drawn at once in an area: the nearest ones; the rest stay
/// cached. SA2_REMOTE_MAX overrides it (for the 8/16/24 measurement).
const REMOTE_MAX: usize = 50;
/// The nearest this many remote Chao run every frame; farther ones every
/// REMOTE_FAR_EVERY frames (staggered by rank).
const REMOTE_NEAR: usize = 12;
const REMOTE_FAR_EVERY: u64 = 5;
/// Shown this far behind the newest update, sliding between updates.
const REMOTE_DELAY: std::time::Duration = std::time::Duration::from_millis(100);
/// A player missing from the input this long is offline: their Chao leaves.
const REMOTE_GONE: std::time::Duration = std::time::Duration::from_secs(1);
/// main.dol FreeTask. Chao leave with this, not ChaoMain's KillChao (.text
/// 0x31450: AL_ClearChaoParam zeroes 0x4DC bytes of the record at chaowk
/// +0x5C, then FreeTask). Other players' Chao: their record
/// at chaowk +0x5C is chao_cache's shared copy, and KillChao zeroed it, so a
/// Chao re-made from it after a record update was all zeros (giant blobs on
/// screen, then a task with a garbage display pointer: "no function at
/// 0x10000004", lr 0x800713BC).
pub(crate) const FREE_TASK: u32 = 0x8007_1AE0;

fn export_pose(m: &Mem) {
    let Ok(path) = std::env::var("SA2_POSE_OUT") else { return };
    let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
    let Step::Live(chao) = STEP.get() else { return };
    if f % EXPORT_EVERY != 0 {
        return;
    }
    let cw = m.r32(chao + 0x34);
    let g = |a: u32| f32::from_bits(m.r32(a));
    let obj2 = m.r32(P1_OBJ2);
    let ball = (0x8000_0000..0x8C00_0000).contains(&obj2) && in_ball(m, obj2);
    let gear = u32::from(SHADOW_TEX.get().is_some()) | (u32::from(KNUX_TEX.get().is_some()) << 1) | (u32::from(ball) << 2);
    let line = format!("{{\"x\":{},\"y\":{},\"z\":{},\"angle\":{},\"motion\":{},\"frame\":0.0,\"gear\":{}}}\n",
        g(cw + 0x14), g(cw + 0x18), g(cw + 0x1C), m.r32(cw + 0x0C) & 0xFFFF, LAST_MOTION.get(), gear);
    let _ = std::fs::write(&path, line);
}

/// A pose-in line "id idle": a Chao whose player is away in Tiny Chao Garden.
fn parse_idle(line: &str) -> Option<u64> {
    let (id, rest) = line.split_once(' ')?;
    (rest.trim() == "idle").then(|| id.parse().ok())?
}

fn parse_remote(line: &str) -> Option<(u64, [f32; 3], u32, u32)> {
    let v: Vec<&str> = line.split_whitespace().collect();
    let f = |i: usize| v.get(i)?.parse::<f32>().ok();
    Some((v.first()?.parse().ok()?, [f(1)?, f(2)?, f(3)?], v.get(4)?.parse().ok()?, v.get(5)?.parse().ok()?))
}

/// SA2_POSE_IN: a file of lines "id x y z angle motion frame gear" (the
/// page writes the snapshot's chaoPose records there; a test feeds a
/// synthetic file). Read about 10 times a second.
fn remote_tick(c: &mut Cpu, m: &mut Mem) {
    let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
    let now = std::time::Instant::now();
    if f >= IMPORT_AT.get() {
        IMPORT_AT.set(f + EXPORT_EVERY);
        let text = std::env::var("SA2_POSE_IN").ok().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
        // An idle Chao has no pose: it appears where the player's own Chao
        // stands when it is first seen (an MMO placement choice) and then
        // moves by its own AI.
        let here = match STEP.get() {
            Step::Live(own) => { let cw = m.r32(own + 0x34); [0x14, 0x18, 0x1C].map(|o| f32::from_bits(m.r32(cw + o))) }
            _ => [0.0; 3],
        };
        let posed = text.lines().filter_map(parse_remote).map(|r| (r, false));
        let idle = text.lines().filter_map(parse_idle).map(|id| ((id, here, 0, STAND_MOTION), true));
        for ((id, p, a, mo), idle) in posed.chain(idle).collect::<Vec<_>>() {
            import_record(m, id);
            if std::env::var_os("SA2_RECORD_IN").is_none() {
                // No record relay (a local pose test): the player's own save info.
                let info: Vec<u8> = (0..0x800).map(|i| m.r8(INFO + i)).collect();
                crate::chao_cache::get_or_load(m, id, &info, 0, false);
            } else if crate::chao_cache::info_addr(id).is_none() {
                // Its own record has not come from the server yet: not drawn
                // (never as a copy of the viewer's Chao).
                continue;
            }
            crate::chao_cache::set_online(id, true);
            REMOTE.with_borrow_mut(|r| {
                let e = r.entry(id).or_insert_with(|| Remote { task: 0, samples: Default::default(), last_seen: now, motion: 0, rank: usize::MAX, offscreen: false, idle });
                e.idle = idle;
                if idle && !e.samples.is_empty() {
                    // Keeps its first spot: only a posed Chao follows samples.
                } else if e.samples.back().is_none_or(|s| s.1 != p || s.2 != a as u16 || s.3 != mo) {
                    e.samples.push_back((now, p, a as u16, mo));
                    while e.samples.len() > 4 {
                        e.samples.pop_front();
                    }
                }
                e.last_seen = now;
            });
        }
    }
    let s1 = rt::sec(m, 60, 1);
    for t in PENDING_KILL.with_borrow_mut(std::mem::take) {
        call(c, m, FREE_TASK, &[t]);
    }
    // Offline: the task leaves at once; the cache keeps the record for its grace.
    let gone: Vec<(u64, u32)> = REMOTE.with_borrow(|r| r.iter().filter(|(_, e)| now.duration_since(e.last_seen) > REMOTE_GONE).map(|(&id, e)| (id, e.task)).collect());
    for (id, task) in gone {
        if task != 0 {
            call(c, m, FREE_TASK, &[task]);
        }
        crate::chao_cache::set_online(id, false);
        REMOTE.with_borrow_mut(|r| r.remove(&id));
        eprintln!("chao-player remote: {id} offline");
    }
    // The nearest few get a game task; farther ones lose theirs.
    let rider = RIDER.get();
    let rp = if rider != 0 { [0x14, 0x18, 0x1C].map(|o| f32::from_bits(m.r32(rider + o))) } else { [0.0; 3] };
    let max = std::env::var("SA2_REMOTE_MAX").ok().and_then(|v| v.parse().ok()).unwrap_or(REMOTE_MAX);
    let pos: Vec<(u64, [f32; 3])> = REMOTE.with_borrow(|r| r.iter().filter_map(|(&id, e)| e.samples.back().map(|s| (id, s.1))).collect());
    let d2 = |p: [f32; 3]| (0..3).map(|k| (p[k] - rp[k]).powi(2)).sum::<f32>();
    let mut order = pos.clone();
    order.sort_by(|a, b| d2(a.1).total_cmp(&d2(b.1)));
    for (n, (id, p)) in order.into_iter().enumerate() {
        let task = REMOTE.with_borrow_mut(|r| { let e = r.get_mut(&id).unwrap(); e.rank = n; e.task });
        if n < max && task == 0 {
            let Some(info) = crate::chao_cache::info_addr(id) else { continue };
            for (o, v) in p.into_iter().enumerate() {
                m.w32(POS + 4 * o as u32, v.to_bits());
            }
            // Never let the game run out of heap (each Chao takes about
            // 72 KB): create only with room to spare, in the remote heap.
            let Some(heap) = remote_heap(c, m) else { continue };
            if heap_free(m, heap) < REMOTE_CHAO_BYTES {
                if !SAID_FULL.replace(true) {
                    eprintln!("chao-player remote: heap {heap} full ({} KB free), {id} stays cached", heap_free(m, heap) / 1024);
                }
                continue;
            }
            let cur = c.r[13].wrapping_sub(0x71B4);
            let old = m.r32(cur);
            m.w32(cur, heap);
            let t = call(c, m, s1 + 0x3148C, &[info, 0, 0, POS, 0]);
            m.w32(cur, old);
            eprintln!("chao-player remote: {id} task {t:#x} (heap {heap}: {} KB free)", heap_free(m, heap) / 1024);
            REMOTE.with_borrow_mut(|r| if let Some(e) = r.get_mut(&id) { e.task = t; e.motion = 0; });
        } else if n >= max && task != 0 {
            call(c, m, FREE_TASK, &[task]);
            REMOTE.with_borrow_mut(|r| if let Some(e) = r.get_mut(&id) { e.task = 0; });
        }
    }
    garden_tick(c, m, s1);
    // A new motion number plays the Chao's own motion.
    let changes: Vec<(u32, u32)> = REMOTE.with_borrow_mut(|r| r.values_mut().filter_map(|e| {
        let mo = e.samples.back()?.3;
        (e.task != 0 && !e.idle && mo != e.motion).then(|| { e.motion = mo; (e.task, mo) })
    }).collect());
    for (t, mo) in changes {
        call(c, m, s1 + AL_SET_MOTION_LINK, &[t, mo]);
    }
}

/// The name tag a Chao task carries: the player's own, another player's, or
/// a hub resident's (see name_tags).
fn tag_label(m: &Mem, task: u32) -> Option<String> {
    if matches!(STEP.get(), Step::Live(ch) if ch == task) {
        return Some("me".into());
    }
    if let Some(id) = REMOTE.with_borrow(|r| r.iter().find(|(_, e)| e.task == task && task != 0).map(|(&id, _)| id)) {
        return Some(format!("p{id}"));
    }
    crate::start_stage::resident_type(m, task).map(|t| format!("r{t}"))
}

/// Before a remote Chao's exec: its position and yaw REMOTE_DELAY behind
/// the newest update, interpolated between the two updates around that time.
fn remote_pin(m: &mut Mem, task: u32) {
    let now = std::time::Instant::now();
    let Some((p, a)) = REMOTE.with_borrow(|r| {
        let e = r.values().find(|e| e.task == task && task != 0 && !e.idle)?;
        let t = now.checked_sub(REMOTE_DELAY)?;
        let s = &e.samples;
        let i = s.iter().rposition(|x| x.0 <= t).unwrap_or(0);
        let a0 = *s.get(i)?;
        let b0 = *s.get(i + 1).unwrap_or(&a0);
        let span = b0.0.duration_since(a0.0).as_secs_f32().max(1e-3);
        let k = (t.saturating_duration_since(a0.0).as_secs_f32() / span).clamp(0.0, 1.0);
        let p: [f32; 3] = std::array::from_fn(|j| a0.1[j] + (b0.1[j] - a0.1[j]) * k);
        let d = b0.2.wrapping_sub(a0.2) as i16 as f32;
        Some((p, (a0.2 as f32 + d * k) as i32 as u32 & 0xFFFF))
    }) else { return };
    let cw = m.r32(task + 0x34);
    if cw == 0 {
        return;
    }
    for (i, v) in p.into_iter().enumerate() {
        m.w32(cw + 0x14 + 4 * i as u32, v.to_bits());
    }
    m.w32(cw + 0x0C, a);
}


// ---- The player's own other Chao in a garden (MMO glue). ----
/// A garden shows at most this many Chao on a player's screen (the game's
/// own garden limit, AL_SetChaoOnTheGarden's `AL_GetLocalChaoCount < 8`):
/// the player's Chao, the other players' Chao, then the player's own other
/// Chao from their save in the places left. Only on this player's screen.
const GARDEN_MAX: usize = 8;
/// ChaoMain AL_GetGardenInfo / AL_GetMaxChao (.text), and the save's Chao
/// slots in the garden info (0x800 bytes each; AL_SetChaoOnTheGarden walks
/// them and creates the ones whose +0x81 is this garden).
const AL_GET_GARDEN_INFO: u32 = 0xDFA0;
const AL_GET_MAX_CHAO: u32 = 0xCC98;
const GARDEN_SLOTS: u32 = 0xA64;

#[derive(Clone, Copy)]
struct Garden {
    slots: u32,
    count: u32,
    area: u32,
    players: usize,
}

thread_local! {
    static GARDEN: Cell<Option<Garden>> = const { Cell::new(None) };
}

/// Whether the own save Chao that is number `rank` (from 0) in its garden
/// has a place while `players` other players' Chao are shown.
fn idle_has_place(rank: usize, players: usize) -> bool {
    1 + players + rank < GARDEN_MAX
}

/// Every frame in a garden (areas 1..3): where the save's slots are and how
/// many other players' Chao are shown.
fn garden_tick(c: &mut Cpu, m: &mut Mem, s1: u32) {
    let area = call(c, m, s1 + 0x27FA0, &[]);
    GARDEN.set((1..=3).contains(&area).then(|| Garden {
        slots: call(c, m, s1 + AL_GET_GARDEN_INFO, &[]) + GARDEN_SLOTS,
        count: call(c, m, s1 + AL_GET_MAX_CHAO, &[]).min(64),
        area,
        players: REMOTE.with_borrow(|r| r.values().filter(|e| e.task != 0).count()),
    }));
}

/// True for one of the player's own save Chao in the garden that has no
/// place right now: it neither runs nor draws until a place frees up. The
/// task and its save record stay untouched (KillChao would clear the save
/// record: AL_ClearChaoParam on chaowk +0x5C).
/// Unverified until a live run: that skipping exec and display hides it
/// completely (no shadow, no collision).
fn idle_hidden(m: &Mem, task: u32) -> bool {
    let Some(g) = GARDEN.get() else { return false };
    if matches!(STEP.get(), Step::Live(chao) if chao == task) {
        return false;
    }
    let cw = m.r32(task + 0x34);
    if !(0x8000_0000..0x8C00_0000).contains(&cw) {
        return false;
    }
    let rec = m.r32(cw + 0x5C);
    let Some(off) = rec.checked_sub(g.slots) else { return false };
    if off % 0x800 != 0 || off / 0x800 >= g.count {
        return false;
    }
    let here = |i: u32| m.r8(g.slots + i * 0x800 + 0x80) != 0 && u32::from(m.r8(g.slots + i * 0x800 + 0x81)) == g.area;
    !idle_has_place((0..off / 0x800).filter(|&i| here(i)).count(), g.players)
}

/// The own Chao's live record (0x800 bytes; the pointer at chaowk +0x5C, as
/// agreed with the gardens thread: unverified until the log line below
/// shows it equals INFO for the created Chao) to SA2_RECORD_OUT when it
/// changes, at most every RECORD_EVERY frames. The page PUTs it raw to
/// /api/chao-record.
const RECORD_EVERY: u64 = 180;

thread_local! {
    static RECORD_SENT: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
    static RECORD_SEEN: std::cell::RefCell<std::collections::BTreeMap<u64, std::time::SystemTime>> = Default::default();
}

/// The own Chao's save record (the pointer at chaowk +0x5C) while it is
/// live, for glue that hands it to the game (race_control).
pub(crate) fn own_record(m: &Mem) -> Option<u32> {
    let Step::Live(chao) = STEP.get() else { return None };
    let rec = m.r32(m.r32(chao + 0x34) + 0x5C);
    (0x8000_0000..0x8C00_0000).contains(&rec).then_some(rec)
}

fn export_record(m: &Mem) {
    let Ok(path) = std::env::var("SA2_RECORD_OUT") else { return };
    let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
    let Step::Live(chao) = STEP.get() else { return };
    if f % RECORD_EVERY != 0 {
        return;
    }
    let cw = m.r32(chao + 0x34);
    let rec = m.r32(cw + 0x5C);
    let src = if (0x8000_0000..0x8C00_0000).contains(&rec) { rec } else { INFO };
    let bytes: Vec<u8> = (0..0x800).map(|i| m.r8(src + i)).collect();
    if RECORD_SENT.with_borrow(|s| *s != bytes) {
        eprintln!("chao-player record: exported from {src:#x} (chaowk+0x5C {rec:#x}, INFO {INFO:#x})");
        let _ = std::fs::write(&path, &bytes);
        RECORD_SENT.set(bytes);
    }
}

/// One skill as (level, progress, points): the record's own, raised to the
/// server's where the server is ahead. Level and progress go together (a
/// level-up resets progress); points never go down.
fn raised(own: (u8, u8, u16), server: (u8, u8, u16)) -> (u8, u8, u16) {
    let (level, progress) = (own.0, own.1).max((server.0, server.1));
    (level, progress, own.2.max(server.2))
}

/// MMO glue: SA2_SKILLS_IN is the server's numbers for the player's Chao (7 x
/// level, progress, points u16 big-endian; the answer to the record upload,
/// so they are for this record). The live record is raised to them, which is
/// how Tiny Garden training and fruit reach the game. Record offsets as the
/// game's AL_ParameterGrow uses them: progress +0x20+i, level +0x30+i,
/// points +0x38+2i.
fn take_skills(m: &mut Mem) {
    let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
    if f % RECORD_EVERY != 0 {
        return;
    }
    let Some(rec) = own_record(m) else { return };
    let Some(bytes) = std::env::var("SA2_SKILLS_IN").ok().and_then(|p| std::fs::read(p).ok()).filter(|b| b.len() == 28) else { return };
    for (i, s) in bytes.chunks(4).enumerate() {
        let i = i as u32;
        let own = (m.r8(rec + 0x30 + i), m.r8(rec + 0x20 + i), m.r16(rec + 0x38 + 2 * i));
        let new = raised(own, (s[0], s[1], u16::from_be_bytes([s[2], s[3]])));
        if new != own {
            eprintln!("chao-player skills: skill {i} {own:?} raised to {new:?} from the server");
            m.w8(rec + 0x30 + i, new.0);
            m.w8(rec + 0x20 + i, new.1);
            m.w16(rec + 0x38 + 2 * i, new.2);
        }
    }
}

/// SA2_RECORD_IN/<id>: a remote Chao's raw record (the page writes it when
/// the snapshot's chaoRecordVersion changes). A new file replaces the cached
/// record and re-creates that Chao with it.
fn import_record(m: &mut Mem, id: u64) {
    let Ok(dir) = std::env::var("SA2_RECORD_IN") else { return };
    let path = std::path::Path::new(&dir).join(id.to_string());
    let Ok(t) = std::fs::metadata(&path).and_then(|md| md.modified()) else { return };
    if RECORD_SEEN.with_borrow(|s| s.get(&id) == Some(&t)) {
        return;
    }
    let Ok(bytes) = std::fs::read(&path) else { return };
    RECORD_SEEN.with_borrow_mut(|s| s.insert(id, t));
    if crate::chao_cache::replace(m, id, &bytes).is_some() {
        eprintln!("chao-player record: {id} updated ({} bytes)", bytes.len());
        // The task is re-created from the new record on the next tick.
        let old = REMOTE.with_borrow_mut(|r| r.get_mut(&id).map(|e| std::mem::replace(&mut e.task, 0)).unwrap_or(0));
        if old != 0 {
            PENDING_KILL.with_borrow_mut(|k| k.push(old));
        }
    }
}

thread_local! {
    static PENDING_KILL: std::cell::RefCell<Vec<u32>> = const { std::cell::RefCell::new(Vec::new()) };
}


/// MMO glue: other players' Chao get a heap of their own, 32 MB at
/// 0x83000000..0x85000000 in the raised MEM1 (needs SA2_MEM1_MB >= 80; the
/// browser copies all guest RAM for GX). The game's OS heap array has room
/// for its own two heaps only (OSCreateHeap returned -1), so this heap is
/// ours: the game's malloc/free wrappers (0x8011989C / 0x801198C4) pass the
/// heap index at r13 -0x71B4 to OSAllocFromHeap / OSFreeToHeap; around a
/// remote Chao's creation that index is REMOTE_HEAP, and those two calls are
/// served from this region (first fit, freed blocks reused). Any free of a
/// block inside it comes back here (the game frees a Chao later, from its
/// own task cleanup).
const REMOTE_HEAP_START: u32 = 0x8300_0000;
const REMOTE_HEAP_END: u32 = 0x8500_0000;
/// The heap index the game's wrappers see while a remote Chao is created.
const REMOTE_HEAP: u32 = 0x5243; // not a valid OS heap index
/// One Chao task's heap use, measured (remote tasks 0x11CC0 apart), rounded up.
pub(crate) const REMOTE_CHAO_BYTES: u32 = 0x1_4000;
const OS_ALLOC_FROM_HEAP: u32 = 0x800E_4554;
const OS_FREE_TO_HEAP: u32 = 0x800E_4650;

thread_local! {
    static REMOTE_ALLOC: std::cell::RefCell<Option<(crate::chao_cache::Region, std::collections::BTreeMap<u32, u32>)>> = const { std::cell::RefCell::new(None) };
    static SAID_FULL: Cell<bool> = const { Cell::new(false) };
}

fn remote_heap(c: &mut Cpu, m: &mut Mem) -> Option<u32> {
    if REMOTE_ALLOC.with_borrow(|r| r.is_some()) {
        return Some(REMOTE_HEAP);
    }
    if (rt::mem1() | 0x8000_0000) < REMOTE_HEAP_END {
        if !SAID_FULL.replace(true) {
            eprintln!("chao-player remote: no heap, MEM1 ends below {REMOTE_HEAP_END:#x} (SA2_MEM1_MB >= 80); remote Chao stay cached");
        }
        return None;
    }
    raise_chao_limit(m);
    REMOTE_ALLOC.set(Some((crate::chao_cache::Region::new(REMOTE_HEAP_START, REMOTE_HEAP_END), Default::default())));
    c.rt.set_override(OS_ALLOC_FROM_HEAP, alloc_from_heap);
    c.rt.set_override(OS_FREE_TO_HEAP, free_to_heap);
    eprintln!("chao-player remote: heap at {REMOTE_HEAP_START:#x}..{REMOTE_HEAP_END:#x} ({} KB, about {} Chao)",
        (REMOTE_HEAP_END - REMOTE_HEAP_START) / 1024, (REMOTE_HEAP_END - REMOTE_HEAP_START) / REMOTE_CHAO_BYTES);
    Some(REMOTE_HEAP)
}

/// The blocks now allocated in the module's own heap.
fn remote_blocks() -> std::collections::BTreeSet<u32> {
    REMOTE_ALLOC.with_borrow(|r| r.as_ref().map(|(_, sizes)| sizes.keys().copied().collect()).unwrap_or_default())
}

/// Give blocks of the module's own heap back (ones already freed are skipped).
fn remote_release(blocks: &[u32]) {
    REMOTE_ALLOC.with_borrow_mut(|r| if let Some((reg, sizes)) = r.as_mut() {
        for p in blocks {
            if let Some(n) = sizes.remove(p) {
                reg.free(*p, n);
            }
        }
    });
}

fn alloc_from_heap(c: &mut Cpu, m: &mut Mem) {
    if c.r[3] != REMOTE_HEAP {
        return original(c, m, OS_ALLOC_FROM_HEAP);
    }
    let size = c.r[4];
    c.r[3] = REMOTE_ALLOC.with_borrow_mut(|r| {
        let (reg, sizes) = r.as_mut()?;
        let a = reg.alloc(size)?;
        sizes.insert(a, size);
        Some(a)
    }).unwrap_or(0);
}

fn free_to_heap(c: &mut Cpu, m: &mut Mem) {
    let p = c.r[4];
    if (REMOTE_HEAP_START..REMOTE_HEAP_END).contains(&p) {
        REMOTE_ALLOC.with_borrow_mut(|r| if let Some((reg, sizes)) = r.as_mut() {
            if let Some(n) = sizes.remove(&p) {
                reg.free(p, n);
            }
        });
        return;
    }
    original(c, m, OS_FREE_TO_HEAP);
}

/// ChaoMain's garden flight behaviour (fn_60_3D978) and AL_SetBehavior.
const ALBHV_FLY: u32 = 0x3_D978;
const AL_SET_BEHAVIOR: u32 = 0x7C4;
const INFO2: u32 = 0x82F0_2000;
const POS2: u32 = 0x82F0_0300;

thread_local! {
    static TAKEOFF: Cell<u32> = const { Cell::new(0) };
}

/// SA2_TAKEOFF=frame[,Fly points[,height]] (test only): to film the garden's
/// own flight. At that frame a second, ordinary garden Chao (the player's
/// record copied, its Fly points set in the copy only) is made beside the
/// player's Chao, `height` above the ground, and put into the game's own
/// flight behaviour with AL_SetBehavior: forced, the game itself starts it
/// only when a walking Chao with Fly 400 or more (three times its points)
/// comes to a drop, four times in ten. Its own AI runs (it is not the
/// player's Chao and not a remote one), and SA2_TEST_CAM follows it.
/// One line a frame while it flies. Nothing of it is saved or sent.
fn takeoff_test(c: &mut Cpu, m: &mut Mem) {
    let Some((at, pts, height)) = std::env::var("SA2_TAKEOFF").ok().and_then(|v| {
        let mut it = v.split(',').map(|x| x.trim().parse::<f32>().ok());
        Some((it.next()?? as u64, it.next().flatten().unwrap_or(1000.0) as u16, it.next().flatten().unwrap_or(0.0)))
    }) else { return };
    let (Step::Live(own), true) = (STEP.get(), chao_world(m) && module_linked(m, 60)) else { return };
    let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
    let s1 = rt::sec(m, 60, 1);
    let g = |m: &Mem, a: u32| f32::from_bits(m.r32(a));
    if TAKEOFF.get() == 0 && f >= at {
        let cw = m.r32(own + 0x34);
        let Some(heap) = remote_heap(c, m) else { return };
        let rec: Vec<u8> = (0..0x800).map(|o| m.r8(INFO + o)).collect();
        m.load(INFO2, &rec);
        m.w16(INFO2 + 0x3A, pts);
        for (i, d) in [8.0, height, 8.0].into_iter().enumerate() {
            m.w32(POS2 + 4 * i as u32, (g(m, cw + 0x14 + 4 * i as u32) + d).to_bits());
        }
        let cur = c.r[13].wrapping_sub(SY_HEAP_SDA);
        let old = m.r32(cur);
        m.w32(cur, heap);
        let t = call(c, m, s1 + 0x3148C, &[INFO2, 0, 0, POS2, 0]);
        m.w32(cur, old);
        if t == 0 {
            TAKEOFF.set(u32::MAX);
            return eprintln!("takeoff: no second Chao");
        }
        EXTRA_TASKS.with_borrow_mut(|x| x.insert(t));
        call(c, m, s1 + AL_SET_BEHAVIOR, &[t, s1 + ALBHV_FLY]);
        TAKEOFF.set(t);
        eprintln!("takeoff: f{f} second Chao {t:#x} with Fly {pts} points, forced into the garden flight (fn_60_3D978)");
    }
    let t = TAKEOFF.get();
    if t != 0 && t != u32::MAX {
        let (tw, mw) = (m.r32(t + 0x34), m.r32(t + 0x38));
        if tw != 0 && mw != 0 && f % 5 == 0 {
            eprintln!("takeoff: f{f} at ({:.2}, {:.2}, {:.2}) speed ({:.3}, {:.3}, {:.3}) gravity {:.4} ground flag {} behaviour step {} timer {} motion flags {:#x}",
                g(m, tw + 0x14), g(m, tw + 0x18), g(m, tw + 0x1C), g(m, mw), g(m, mw + 4), g(m, mw + 8), g(m, mw + 0x48), m.r16(mw + 0x40) & 0x400 != 0, m.r16(tw + 438), m.r32(tw + 448), m.r16(mw + 0x40));
        }
    }
}

/// A Chao of its own kind (CreateChaoExtra type 1, as the Chao Select
/// window's) from the record at `info`, standing at `pos`, in the remote
/// heap so the lobby's own heap keeps its room. 0: no heap or no room.
pub(crate) fn extra_chao(c: &mut Cpu, m: &mut Mem, info: u32, pos: u32) -> u32 {
    let Some(heap) = remote_heap(c, m) else { return 0 };
    if heap_free(m, heap) < REMOTE_CHAO_BYTES {
        return 0;
    }
    let cur = c.r[13].wrapping_sub(0x71B4);
    let old = m.r32(cur);
    m.w32(cur, heap);
    let t = call(c, m, rt::sec(m, 60, 1) + 0x3148C, &[info, 1, 0, pos, 0]);
    m.w32(cur, old);
    if t != 0 {
        EXTRA_TASKS.with_borrow_mut(|x| x.insert(t));
    }
    t
}

thread_local! {
    /// Live extra_chao tasks: their blocks are in the remote heap, which must
    /// not start empty again under them (a reset under the card box's
    /// portrait Chao handed its blocks to the next remote Chao, and the task
    /// loop then ran a garbage task: "no function at 0x10000004").
    static EXTRA_TASKS: std::cell::RefCell<std::collections::BTreeSet<u32>> = Default::default();
}

/// The player's Chao's record (0x800 bytes), as [`export_record`] sends it.
pub(crate) fn player_record(m: &Mem) -> Option<Vec<u8>> {
    let Step::Live(chao) = STEP.get() else { return None };
    let rec = m.r32(m.r32(chao + 0x34) + 0x5C);
    let src = if (0x8000_0000..0x8C00_0000).contains(&rec) { rec } else { INFO };
    Some((0..0x800).map(|i| m.r8(src + i)).collect())
}

/// Free bytes left in the remote heap (blocks freed back count as free).
fn heap_free(_m: &Mem, _h: u32) -> u32 {
    REMOTE_ALLOC.with_borrow(|r| r.as_ref().map_or(0, |(reg, _)| (REMOTE_HEAP_END - REMOTE_HEAP_START) - reg.used()))
}

/// ChaoMain's per-category limit of its world list (nbMaxEntry, the u32
/// array at .data sec4 +0x81E0 that ALW_Entry2 0x2984C searches up to;
/// each category has 64 slots of 0x48 in CommuID's 0x1200 block). Category
/// 0's limit is raised to all 64 slots for other players' Chao.
/// unverified: category 0 is the Chao's (read as 0x20 = 32 in the disc data).
const NB_MAX_ENTRY: u32 = 0x81E0;
const WORLD_SLOTS: u32 = 64;

fn raise_chao_limit(m: &mut Mem) {
    let a = rt::sec(m, 60, 4) + NB_MAX_ENTRY;
    let old = m.r32(a);
    if old < WORLD_SLOTS {
        m.w32(a, WORLD_SLOTS);
        eprintln!("chao-player remote: world list limit {old} -> {WORLD_SLOTS}");
    }
}


/// A remote Chao outside the view (with REMOTE_VIEW_MARGIN units to spare):
/// its display draws nothing and its exec is skipped until it comes back;
/// remote_pin keeps its position following meanwhile. The view is the base
/// matrix this display runs under; "in front" is the side the player's own
/// Chao is on (no camera convention assumed).
/// shortcut: the horizontal and vertical limits are |x|,|y| <= REMOTE_VIEW_SLOPE
/// * depth + margin (about 63 degrees each side), wider than the game's view
/// rather than read from its projection; upgrade with the real FOV if Chao pop.
const REMOTE_VIEW_SLOPE: f32 = 2.0;
const REMOTE_VIEW_MARGIN: f32 = 8.0;

/// ChaoMain AL_IsOnScreen3 (.text 0x1A590): the Chao draw (al_draw_9 0x82B0)
/// skips a Chao whose projected bounds fall outside 0..640 x 0..480, the GC's
/// 4:3 screen, so on the browser's wide view the player's and other players'
/// Chao vanished well before the screen edge (mob: they fade before they get
/// off screen). Those Chao always pass; remote_offscreen already culls other
/// players' Chao by a wide view cone, and the game's other Chao keep the test.
const AL_IS_ON_SCREEN3: u32 = 0x1_A590;

fn on_screen3(c: &mut Cpu, m: &mut Mem) {
    let t = c.r[3];
    let ours = matches!(STEP.get(), Step::Live(ch) if ch == t) || REMOTE.with_borrow(|r| r.values().any(|e| e.task != 0 && e.task == t))
        // The card box's portrait Chao: on a wide canvas the box sits left of
        // the game's 640 (card_box left_edge), where this test culled it.
        || EXTRA_TASKS.with_borrow(|x| x.contains(&t));
    let a = rt::sec(m, 60, 1) + AL_IS_ON_SCREEN3;
    // The original still runs: it also stores the camera distance the draw uses.
    rel_original(c, m, a, on_screen3);
    if ours {
        c.r[3] = 1;
    }
}

fn remote_offscreen(m: &Mem, task: u32) -> bool {
    if task == 0 || !REMOTE.with_borrow(|r| r.values().any(|e| e.task == task)) {
        return false;
    }
    let Step::Live(own) = STEP.get() else { return false };
    let base = mtx(m, m.r32(NJ_BASE_MATRIX));
    let view = |cw: u32| -> [f32; 3] {
        let p = [0x14, 0x18, 0x1C].map(|o| f32::from_bits(m.r32(cw + o)));
        std::array::from_fn(|i| (0..3).map(|k| base[i][k] * p[k]).sum::<f32>() + base[i][3])
    };
    let (own_cw, cw) = (m.r32(own + 0x34), m.r32(task + 0x34));
    if own_cw == 0 || cw == 0 {
        return false;
    }
    let front = view(own_cw)[2].signum();
    let v = view(cw);
    let depth = v[2] * front;
    let out = depth < -REMOTE_VIEW_MARGIN
        || v[0].abs() > REMOTE_VIEW_SLOPE * depth.max(0.0) + REMOTE_VIEW_MARGIN
        || v[1].abs() > REMOTE_VIEW_SLOPE * depth.max(0.0) + REMOTE_VIEW_MARGIN;
    REMOTE.with_borrow_mut(|r| if let Some(e) = r.values_mut().find(|e| e.task == task) { e.offscreen = out; });
    out
}
