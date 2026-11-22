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
const KNUX_EXEC: u32 = 0x8009_F8A0;
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
    if std::env::var_os("SA2_SHOW_SONIC").is_some() {
        (c.r, c.f[1]) = (regs, f1);
        original(c, m, addr);
    }
    let n = NODES.with_borrow(|n| n.len());
    // Any rider (Knuckles's and Rouge's skeletons too): once a frame.
    if n > 8 && !SELF_CAP.get() {
        hammer_gloves(c, m);
    }
    let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
    if f % 60 == 0 {
        eprintln!("chao-player lvl: frame {f} draw {addr:#x} obj {obj:#x} motion {mot:#x} nodes {n}");
    }
    if n >= 56 {
        let nodes = NODES.with_borrow(|n| n.clone());
        // Self mode: captured under the scaled view, back to full size.
        let nodes = match SELF_SCALE.get() {
            Some((cur, curs)) => nodes.iter().map(|e| mul34(&cur, &inv_mul(&curs, e))).collect(),
            None => nodes,
        };
        SKEL.set(nodes);
        if SELF_CAP.get() {
            return;
        }
        RIDER_OBJ.set(obj);
        air_shoes(c, m);
    } else if n > 0 && SELF_CAP.get() {
        // Self mode: the ball as he draws it, under the view scaled to the Chao.
        (c.r, c.f[1]) = (regs, f1);
        original(c, m, addr);
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
    let (sx, cx) = v[3].to_radians().sin_cos();
    let (sy, cy) = v[4].to_radians().sin_cos();
    let (sz, cz) = v[5].to_radians().sin_cos();
    let rx = [[1.0, 0.0, 0.0], [0.0, cx, -sx], [0.0, sx, cx]];
    let ry = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
    let rz = [[cz, -sz, 0.0], [sz, cz, 0.0], [0.0, 0.0, 1.0]];
    let r = crate::skate::mul(&rz, &crate::skate::mul(&ry, &rx));
    Some(std::array::from_fn(|i| std::array::from_fn(|j| if j < 3 { r[i][j] * v[6] } else { v[i] })))
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
    let node = SHOES[n].0;
    let hp = if bv.is_empty() { None } else { hub_boot_place(m, n, node, &br, &bv) };
    if let Some(place) = hp.map(|p| gear_tune(if n == 0 { "boot0" } else { "boot1" }).map_or(p, |t| mul34(&p, &t))) {
        if sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) % 300 == 0 {
            eprintln!("chao-player boot {n}: Shadow heel {heel:#x} ({} nodes, {} vertices), overlay {overlay:#x}", br.nodes.len(), bv.len());
        }
        let put = |m: &mut Mem| for r in 0..3 { for k in 0..4 { m.w32(cur_ptr + 4 * (4 * r + k) as u32, place[r][k].to_bits()); } };
        m.w32(list, tex);
        let lcap = LCAP.replace(false);
        put(m);
        call(c, m, 0x8011_DF44, &[heel]);
        if (0x8000_0000..0x8C00_0000).contains(&overlay) {
            put(m);
            call(c, m, 0x8011_DF44, &[overlay]);
        }
        if MOVING.get() && !debug_off("jets") {
            for js in 181..=184u32 {
                let jo = model_obj(m, js);
                if (0x8000_0000..0x8C00_0000).contains(&jo) {
                    put(m);
                    call(c, m, 0x8011_DF44, &[jo]);
                }
            }
        }
        LCAP.set(lcap);
    }
    for (i, o) in (0x08..0x34).step_by(4).enumerate() { m.w32(heel + o, keep[i]); }
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
    if OWN_MOTION.get() || std::env::var("SA2_AIR_SHOES").is_ok_and(|v| v == "0") || debug_off("shoes") {
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
    // Chao World places the boots from the Chao's own feet (hub fit): no
    // rider skeleton needed there, and in self mode there is none.
    if !chao_world(m) && (posed.len() < 14 || skel.len() != 62) {
        if sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) % 300 == 0 {
            eprintln!("chao-player shoes: skipped (posed {}, skel {})", posed.len(), skel.len());
        }
        return;
    }
    let models: Vec<u32> = std::env::var("SA2_SHOE_MODELS").ok().map(|v| v.split(',').filter_map(|x| x.trim().parse().ok()).collect()).unwrap_or_default();
    let cur_ptr = m.r32(NJ_CURRENT_MATRIX);
    let keep_m = mtx(m, cur_ptr);
    let base = mtx(m, m.r32(NJ_BASE_MATRIX));
    let list = m.r32(c.r[13].wrapping_sub(0x7228)) + 0x20;
    let keep_list = m.r32(list);
    let regs = (c.r, c.f);
    let ext = |vs: Vec<[f32; 3]>| (0..3).map(|k| { let (lo, hi) = vs.iter().fold((f32::MAX, f32::MIN), |(l, h), v| (l.min(v[k]), h.max(v[k]))); hi - lo }).fold(0.0f32, f32::max);
    for (n, &(node, foot, model)) in SHOES.iter().enumerate() {
        if chao_world(m) && models.is_empty() && !std::env::var("SA2_SHADOW_BOOT").is_ok_and(|v| v == "0") {
            shadow_boot(c, m, n, tex, cur_ptr, list);
            continue;
        }
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
                if MOVING.get() && !debug_off("jets") {
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
    let mount = RIDER_REST.with_borrow(|r| r.as_ref().and_then(|(_, v)| v.get(24).copied()))
        .map(|rf| crate::skate::mul(&[[0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]], &crate::skate::unit(&rf)))
        .unwrap_or(mount);
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
    // swallows it); rider mode used 1.7 with the hub mount. The computed fit
    // alone sits inside the foot: find the measuring difference.
    let sc = (fhi[2] - flo[2]) * SHOE_LENGTH_RATIO / (shi[2] - slo[2]) * std::env::var("SA2_SHOE_SCALE").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(1.4);
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
/// Where along the arm (shoulder 0 -> hand tip 1) the glove's origin (his
/// wrist) sits, and how far the fists turn out from the hips (degrees):
/// a presentation fit, not game data. SA2_GLOVE_UP / SA2_GLOVE_OUT override.
/// unverified: first values, from the request for higher and 5-15 degrees out.
fn glove_up() -> f32 {
    std::env::var("SA2_GLOVE_UP").ok().and_then(|v| v.parse().ok()).unwrap_or(0.35)
}
fn glove_out() -> f32 {
    std::env::var("SA2_GLOVE_OUT").ok().and_then(|v| v.parse().ok()).unwrap_or(10.0)
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
        let (up, fwd) = if std::env::var("SA2_GLOVE_NOMIRROR").is_ok() { (up, fwd) } else { ([-up[0], up[1], up[2]], [-fwd[0], fwd[1], fwd[2]]) };
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
        let native = (!std::env::var("SA2_GLOVE_HUB").is_ok_and(|v| v == "1")).then(|| {
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
        let s = hsize * GLOVE_SCALE / ssize * std::env::var("SA2_GLOVE_SCALE").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(1.0);
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
        let desired: [[f32; 4]; 3] = std::array::from_fn(|i| std::array::from_fn(|j| if j < 3 { r[i][j] * s } else { t[i] }));
        // Swung out from the hips about the Chao's forward axis (+z) through
        // the shoulder: the right arm (-x) turns towards -x, the left towards +x.
        let out_deg = glove_out() * if sh[0] < 0.0 { -1.0 } else { 1.0 };
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
    }
    let Some(fit) = GLOVE_FIT.with_borrow(|g| *g) else { return };
    let cur_ptr = m.r32(NJ_CURRENT_MATRIX);
    let keep_m = mtx(m, cur_ptr);
    let list = m.r32(c.r[13].wrapping_sub(0x7228)) + 0x20;
    let keep_list = m.r32(list);
    let regs = (c.r, c.f);
    for (k, (model, local)) in fit.into_iter().enumerate() {
        let Some(node) = HANDS.with_borrow(|h| h[k]) else { continue };
        let place = mul34(&node, &local);
        let place = gear_tune(if k == 0 { "fist0" } else { "fist1" }).map_or(place, |t| mul34(&place, &t));
        if f % 300 == 0 {
            let ft = FEET.with_borrow(|x| x[k].map(|v| [v[0][3], v[1][3], v[2][3]]));
            eprintln!("chao-player gloves: {k} arm node at {:?}, fist at {:?}, foot {:?}, local t {:?}", [node[0][3], node[1][3], node[2][3]], [place[0][3], place[1][3], place[2][3]], ft, [local[0][3], local[1][3], local[2][3]]);
        }
        for r in 0..3 { for q in 0..4 { m.w32(cur_ptr + 4 * (4 * r + q) as u32, place[r][q].to_bits()); } }
        m.w32(list, tex);
        let lcap = LCAP.replace(false);
        call(c, m, 0x8011_DF44, &[knux_obj(m, model)]);
        // Knuckles's own fist under the plate (same frame: both are built on
        // his hand node).
        let fist = knux_obj(m, knux_fist(model));
        if (0x8000_0000..0x8C00_0000).contains(&fist) {
            for r in 0..3 { for q in 0..4 { m.w32(cur_ptr + 4 * (4 * r + q) as u32, place[r][q].to_bits()); } }
            call(c, m, 0x8011_DF44, &[fist]);
        }
        LCAP.set(lcap);
    }
    (c.r, c.f) = regs;
    m.w32(list, keep_list);
    for r in 0..3 { for q in 0..4 { m.w32(cur_ptr + 4 * (4 * r + q) as u32, keep_m[r][q].to_bits()); } }
}

/// Place the Chao on the rider right before it draws (after its own exec).
fn chao_display(c: &mut Cpu, m: &mut Mem) {
    if remote_offscreen(m, c.r[3]) {
        return;
    }
    if idle_hidden(m, c.r[3]) {
        return;
    }
    crate::name_tags::begin(tag_label(m, c.r[3]));
    if let Step::Live(chao) = STEP.get() {
        if c.r[3] == chao {
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
    let ev = RIDER.get() == 0 && live;
    // In a stage: ChaoMain's draw walker (.text 0x88CC, from the display's
    // 0x8260) calls the node transform 0x1454 per node; chao_node replaces
    // each result with the retargeted pose (Chao-local, posed below).
    // Chao World: the native Chao rig and its own motions draw it (no rider
    // skeleton copied onto it); elsewhere the rider's pose is retargeted.
    // Chao World self mode: the hidden Shadow's live skeleton (walked by his
    // display, drawing nothing) poses the Chao, so it walks, runs and skates
    // with his motions. SA2_CHAO_RETARGET=1 (debug): also outside self mode;
    // =0: the Chao's own motions.
    if live && RIDER.get() != 0 && (!chao_world(m) || (self_retarget(m) && self_in_motion()) || std::env::var("SA2_CHAO_RETARGET").is_ok_and(|v| v == "1")) {
        level_pose(m, c.r[3]);
        CHAO_DRAW.set(true);
        CHAO_NODE.set(0);
        let a = rt::sec(m, 60, 1) + CHAO_DISPLAY;
        rel_original(c, m, a, chao_display);
        if self_drive(m) {
            // No rider draw carries the gear in self mode: draw it from the
            // hands and feet this draw recorded.
            hammer_gloves(c, m);
            air_shoes(c, m);
        }
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
        rel_original(c, m, a, chao_display);
        NATIVE_DRAW.set(false);
        // Self mode: no rider display walks, which is where the gear was drawn
        // (level_capture); draw it here, from the feet and hands this native
        // draw just recorded.
        if self_drive(m) {
            hammer_gloves(c, m);
            air_shoes(c, m);
        }
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
    // Upright: only the rider's yaw. Its pitch/roll would tip the Chao while
    // the emotion ball stays world-up above where the head would be.
    m.w32(cw + 0x08, 0);
    // SA2_CHAO_YAW_OFS (BAMS, debug until measured): the Chao's facing vs the rider's.
    let ofs = std::env::var("SA2_CHAO_YAW_OFS").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
    m.w32(cw + 0x0C, m.r32(data1 + 0x0C).wrapping_add(ofs) & 0xFFFF);
    m.w32(cw + 0x10, 0);
}

pub fn install(c: &mut Cpu, _m: &mut Mem) {
    if std::env::var_os("SA2_HEAP_LOG").is_some() {
        sa2_recomp::os::add_frame_hook(|_, m| heap_log(m));
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
    c.rt.set_override(DESTROY_TASK, destroy_task);
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
    c.rt.set_override(ADX_START_AFS, voice);
    c.rt.set_override(SONIC_DISP, |c, m| display(c, m, SONIC_DISP));
    // SA2_CHAO_KNUX_DISP=0 (debug): leave Knuckles's and Rouge's displays alone.
    if !std::env::var("SA2_CHAO_KNUX_DISP").is_ok_and(|v| v == "0") {
        c.rt.set_override(KNUX_DISP, |c, m| display(c, m, KNUX_DISP));
        c.rt.set_override(ROUGE_DISP, |c, m| display(c, m, ROUGE_DISP));
    }
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
        return level_capture(c, m, addr);
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
            CHAO_PLACE.set(mtx(m, m.r32(NJ_CURRENT_MATRIX)));
        }
        rel_original(c, m, a, chao_node);
        // The player's Chao on its native rig: its foot nodes for the boots.
        if NATIVE_DRAW.get() {
            let i = CHAO_NODE.replace(CHAO_NODE.get() + 1);
            if i == 6 { DRAWN_FOOT.set(c.r[3]); }
            // The hands as this native draw places them, for the hammer gloves.
            record_hand(m, i, m.r32(NJ_CURRENT_MATRIX));
            // Wearing Shadow's shoes and moving: the web hub's skate overlay
            // (skate.rs) on its 8 mapped nodes, from the rider's live pose.
            if MOVING.get() && SHADOW_TEX.get().is_some() && !std::env::var("SA2_AIR_SHOES").is_ok_and(|v| v == "0") {
                if let Some(&(_, src, w, cap)) = crate::skate::MAP.iter().find(|e| e.0 == i) {
                    skate_node(m, i, src, w, cap);
                }
            }
            if let Some(k) = SHOES.iter().position(|s| s.0 == i) {
                let cur = mtx(m, m.r32(NJ_CURRENT_MATRIX));
                FEET.with_borrow_mut(|f| f[k] = Some(cur));
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
            pose_chao();
        }
    }
    let a = rt::sec(m, 60, 1) + AL_NODE_TRANSFORM;
    rel_original(c, m, a, chao_node);
    crate::name_tags::node(mtx(m, cur_ptr));
    // The hands as the Chao's own motion places them (replaced below when posed).
    record_hand(m, i, cur_ptr);
    if MIRROR.get() {
        mirror_node(m, i, cur_ptr);
        return record_hand(m, i, cur_ptr);
    }
    let Some((r, t)) = POSED.with_borrow(|p| p.get(i).copied()) else {
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
    }
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

/// One skate-mapped node: rotation from skate::desired (Chao model space,
/// under the Chao's placement), position kept as the native pose drew it.
fn skate_node(m: &mut Mem, i: usize, src: usize, w: f32, cap: f32) {
    let r3 = |x: &[[f32; 4]; 3]| -> [[f32; 3]; 3] { std::array::from_fn(|a| std::array::from_fn(|b| x[a][b])) };
    let Some(now) = SKEL.with_borrow(|s| (s.len() == 62).then(|| r3(&inv_mul(&s[0], &s[src])))) else { return };
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
    let p = CHAO_PLACE.get();
    let cur_ptr = m.r32(NJ_CURRENT_MATRIX);
    for row in 0..3 {
        for col in 0..3 {
            let v: f32 = (0..3).map(|k| p[row][k] * d[k][col]).sum();
            m.w32(cur_ptr + 4 * (4 * row + col) as u32, v.to_bits());
        }
    }
}

thread_local! {
    static RIDER_OBJ: Cell<u32> = const { Cell::new(0) };
    /// The foot node object (6) the native draw walked this frame: the drawn
    /// (adult) foot, whose mesh may differ in size from the rest model's.
    static DRAWN_FOOT: Cell<u32> = const { Cell::new(0) };
    static RIDER_REST: std::cell::RefCell<Option<(u32, Vec<[[f32; 3]; 3]>)>> = const { std::cell::RefCell::new(None) };
}

/// Arm nodes 3/10's view-space matrices from this draw walk, for the gloves.
fn record_hand(m: &Mem, i: usize, cur_ptr: u32) {
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
    let w = rest.worlds(&[]);
    w.iter().map(|&(_, p)| {
        let q = [-p[0], p[1], p[2]];
        let d = |j: usize| (0..3).map(|k| (w[j].1[k] - q[k]).powi(2)).sum::<f32>();
        (0..w.len()).min_by(|&a, &b| d(a).total_cmp(&d(b))).unwrap_or(0)
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
    pose_chao();
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
    pose_chao();
    POSED_WORLD.set(false);
    // SA2_HEAD_LOG=a-b (debug): the posed head (node 16) and body (node 1)
    // up vectors and the rider's spine 2->34 each frame, to see head jitter.
    if let Some((a, b)) = std::env::var("SA2_HEAD_LOG").ok().and_then(|r| { let (a, b) = r.split_once('-')?; Some((a.parse::<u64>().ok()?, b.parse::<u64>().ok()?)) }) {
        let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
        if (a..=b).contains(&f) {
            let up = |r: &[[f32; 3]; 3]| [r[0][1], r[1][1], r[2][1]].map(|x| (x * 1000.0).round() / 1000.0);
            let sp = SKEL.with_borrow(|s| if s.len() == 62 { let a = inv_mul(&s[0], &s[2]); let b = inv_mul(&s[0], &s[34]); [b[0][3] - a[0][3], b[1][3] - a[1][3], b[2][3] - a[2][3]].map(|x| (x * 1000.0).round() / 1000.0) } else { [0.0; 3] });
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
fn pose_chao() {
    let sonic: Vec<[[f32; 4]; 3]> = SKEL.with_borrow(|s| {
        if s.len() < 56 {
            return vec![];
        }
        if s.len() == 62 {
            return s.iter().map(|e| inv_mul(&s[0], e)).collect();
        }
        STEAM_TO_EV.iter().map(|&e| inv_mul(&s[0], &s[e as usize])).collect()
    });
    let posed = CHAO_REST.with_borrow(|r| match (r, sonic.len()) {
        _ if OWN_MOTION.get() => vec![],
        (Some(rest), 62) => rest.pose(&sonic, false, ARM_FWD.get()),
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
        if self_retarget(m) {
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

thread_local! {
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
    if !self_retarget(m) || w == 0 || cw == 0 || m.r8(w + 1) != 0 {
        return original(c, m, SPIN_FLASH_DISP);
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
    self_drive(m) && !std::env::var("SA2_CHAO_RETARGET").is_ok_and(|v| v == "0")
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

/// A new stage (not a Chao World area change): the game has released the
/// last stage's memory, so the textures, models and motions this module
/// loaded there are gone. Forget them so they load again; drawing the gear
/// from the freed models hung the game after a level EXIT into Chao World
/// (frames stopped right after the new Chao's first draw).
/// Unverified: which allocations the stage change frees; this drops all of them.
fn drop_stage_loads() {
    RIDER_REST.set(None);
    RIDER_OBJ.set(0);
    GEAR_LOAD_AT.set(sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) + GEAR_LOAD_DELAY);
    SHADOW_TEX.set(None);
    KNUX_TEX.set(None);
    EXTRA_MODELS.with_borrow_mut(|e| e.clear());
    KNUX_MODELS.with_borrow_mut(|e| e.clear());
    SHADOW_MODELS.with_borrow_mut(|e| e.clear());
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

fn lobby_exit_hook(c: &mut Cpu, m: &mut Mem) {
    let at = if chao_world(m) && module_linked(m, LOBBY_MODULE) { rt::sec(m, LOBBY_MODULE, 1) + LOBBY_EXIT_EXEC } else { 0 };
    if at != 0 && EXIT_HOOKED.replace(at) != at {
        c.rt.set_override(at, lobby_exit_exec);
    }
}

fn lobby_exit_exec(c: &mut Cpu, m: &mut Mem) {
    let (task, at) = (c.r[3], rt::sec(m, LOBBY_MODULE, 1) + LOBBY_EXIT_EXEC);
    let twp = m.r32(task + 0x34);
    if twp != 0 && m.r8(twp) == 1 && m.r16(twp + 4) & 0x100 == 0 && call(c, m, HIT_PLAYER, &[task, 0]) != 0 {
        eprintln!("chao-player: lobby exit -> the game's Stage Select");
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
    /// The heading the character's exec left on the Chao's body (twp +0x08..).
    static CHAR_ANG: Cell<Option<[u32; 3]>> = const { Cell::new(None) };
    static AREA_DONE: Cell<bool> = const { Cell::new(false) };
    static LIVE_STAGE: Cell<u16> = const { Cell::new(u16::MAX) };
    static SELF_LOGGED: Cell<bool> = const { Cell::new(false) };
}

fn exec(c: &mut Cpu, m: &mut Mem, addr: u32) {
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
            eprintln!("yaw: frame {} char {:#06x} native pos ({:.2},{:.2})", sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed), m.r32(own + 0x0C) & 0xFFFF, f(own + 0x14), f(own + 0x1C));
        }
    } else if let (true, Step::Live(chao)) = (pmove, STEP.get()) {
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
    info_out(m, area);
    debug_teleport(m, task);
    crate::card_box::frame(c, m);
    lobby_exit_hook(c, m);
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
        if LIVE_STAGE_NUM.replace(m.r16(STAGE_NUM)) != m.r16(STAGE_NUM) {
            drop_stage_loads();
        }
        if let Step::Live(_) = STEP.get() {
            STEP.set(Step::Wait(0));
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
        if chao_world(m) {
            let d = d2.sqrt();
            let last = LAST_MOTION.get();
            let want = if d < 0.01 { 0 } else if d >= if last == 119 { 0.9 } else { 1.0 } { 119 } else { 578 };
            if want != last {
                call(c, m, rt::sec(m, 60, 1) + AL_SET_MOTION_LINK, &[chao, want]);
                LAST_MOTION.set(want);
            }
        }
        pin(m, chao, data1);
        if sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) % 300 == 0 {
            eprintln!("chao-player: type {:#x} analysed {} yaw rider {:#x} chao {:#x}", m.r8(INFO + 0x80), m.r8(INFO + 0x438), m.r32(data1 + 0x0C), m.r32(m.r32(chao + 0x34) + 0x0C));
        }
        // ChaoMain AL_DisableNade (.text 0x5A3C, = CCL_Disable(task, 2)):
        // the rider's B is its own move, not Pet (action 31). Each frame, as
        // the Chao's behaviours may enable touch again (AL_EnableTouch 0x5AB0).
        call(c, m, rt::sec(m, 60, 1) + AL_DISABLE_NADE, &[chao]);
        // The Hammer Gloves item in Knuckles's and Rouge's levels: Knuckles's
        // models are there in his own; Rouge's need KNUCKMDL. KNUCKTEX is
        // loaded once for the glove's own list.
        if addr == KNUX_EXEC && KNUX_TEX.get().is_none() && !std::env::var("SA2_HAMMER_GLOVES").is_ok_and(|v| v == "0") {
            let obj2 = m.r32(P1_OBJ2);
            let tex = call(c, m, LOAD_TEX, &[0x8017_4F04]);
            if model_obj(m, 329) == 0 {
                load_models(c, m, 0x8017_4F10);
                KNUX_MODELS.set(LAST_LOAD.with_borrow(|l| l.clone()));
            }
            eprintln!("chao-player: Hammer Gloves textures {tex:#x} (rider {})", m.r8(obj2 + 1));
            KNUX_TEX.set(Some(tex).filter(|&t| t != 0));
        }
        // Shadow's textures and models for the Dark Chao's ball, once, in
        // Sonic's levels (playerwk +1 = 0, the byte Sonic's display picks his models by).
        if addr == SONIC_EXEC && SHADOW_TEX.get().is_none() && sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed) % 300 == 0 {
            eprintln!("chao-player: rider char {} (shadow tex not loaded yet)", m.r8(m.r32(P1_OBJ2) + 1));
        }
        if addr == SONIC_EXEC && (m.r8(m.r32(P1_OBJ2) + 1) == 0 || chao_world(m)) && SHADOW_TEX.get().is_none() && std::env::var("SA2_BALL_SH").map_or(true, |v| v != "0")
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
            if !chao_world(m) {
                shadow_moves(c, m);
            }
            force_sonic_state(m);
        } else {
            knuckles_rules(c, m, chao);
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

/// mob's approved rules for Knuckles and Rouge (coordinator 2026-11-21):
/// punch 1 (Knuckles motion 109) = Chao karate 582, punch 2 (110) = 582
/// mirrored, punch 3 on (111 uppercut, 138 hammer) = Knuckles retargeted with
/// the arms biased 0.8 forward; glide (105) = Chao fly 130, climb (100) =
/// Chao climb 155; dig (90-92, 95-96) and the rest retargeted.
/// shortcut: the karate motion starts on the action change, not frame-synced
/// to Knuckles's 109/110 nframe.
/// shortcut: no arm snap (chao-arm-dirs.json is not on this PC): 1:1 arms.
fn knuckles_rules(c: &mut Cpu, m: &mut Mem, chao: u32) {
    let obj2 = m.r32(P1_OBJ2);
    if !(0x8000_0000..0x8C00_0000).contains(&obj2) {
        return;
    }
    let idx = u32::from(m.r16(obj2 + OBJ2_ACTION));
    let table = m.r32(obj2 + OBJ2_PLACTPTR);
    if !(0x8000_0000..0x8C00_0000).contains(&table) {
        return;
    }
    // The animation state (PC CharObj2 AnimInfo.Current; mob's numbers).
    // SA2_KNUX_STATE=n (debug): treat the rider as in state n, to check a rule.
    let act = std::env::var("SA2_KNUX_STATE").ok().filter(|_| forced_now()).and_then(|v| v.parse().ok()).unwrap_or(idx as u16);
    let actnum = m.r16(table + 0x10 * idx);
    if LAST_ACT.get() != act {
        eprintln!("chao-player knux: state {act} actnum {actnum}");
    }
    let (own, mirror, fwd) = match act {
        109 => (Some(582), false, 0.0),
        110 => (Some(582), true, 0.0),
        111 | 138 => (None, false, 0.8),
        105 => (Some(130), false, 0.0),
        100 => (Some(155), false, 0.0),
        _ => (None, false, 0.0),
    };
    if LAST_ACT.replace(act) != act {
        eprintln!("chao-player knux: action {act} -> chao motion {own:?} mirror {mirror} arm fwd {fwd}");
        if let Some(mo) = own {
            let s1 = rt::sec(m, 60, 1);
            call(c, m, s1 + AL_SET_MOTION_LINK, &[chao, mo]);
            LAST_MOTION.set(mo);
        }
    }
    OWN_MOTION.set(own.is_some());
    MIRROR.set(mirror);
    ARM_FWD.set(fwd);
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
}

/// Off the board in Sonic's levels the owner's Chao moves
/// like Shadow (physics stays Metal's row). Every state whose Shadow row plays
/// a different motion number takes Shadow's row (motion, blend, speed; Sonic's
/// model field kept), from Shadow's own motion file loaded once.
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
        let mtn = call(c, m, LOAD_MTN, &[SHADOW_MTN_NAME]);
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
    m.w32(obj2 + OBJ2_PLACTPTR, if board { SONIC_ACTIONS } else { MIXED_ACTIONS });
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
    let metal = if chao_world(m) { crate::chao_speed::row(m, INFO, &own, &metal) } else { metal };
    for (i, o) in (0..0x84).step_by(4).enumerate() {
        m.w32(obj2 + 0xC0 + o, if board { own[i] } else { metal[i] });
    }
}

thread_local! {
    static SONIC_ROW: std::cell::RefCell<Option<Vec<u32>>> = const { std::cell::RefCell::new(None) };
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
            eprintln!("chao-player: the game destroyed the player's Chao task {t:#x}; making it again");
            STEP.set(Step::Wait(0));
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
    use super::*;

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
            // shortcut: a remote record is the owner's own save info until the
            // server relays each Chao's record; get_or_load keeps it per id.
            let info: Vec<u8> = (0..0x800).map(|i| m.r8(INFO + i)).collect();
            crate::chao_cache::get_or_load(m, id, &info, 0, false);
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
    let ours = matches!(STEP.get(), Step::Live(ch) if ch == t) || REMOTE.with_borrow(|r| r.values().any(|e| e.task != 0 && e.task == t));
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
