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
    /// Sonic's data1 (position +0x14, rotation +0x08) for the Chao's display.
    static RIDER: Cell<u32> = const { Cell::new(0) };
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
    // SA2_EV_SKIP=exec,display (debug): skip this step in a cutscene.
    if RIDER.get() == 0 && std::env::var("SA2_EV_SKIP").is_ok_and(|v| v.contains("exec")) {
        return;
    }
    let a = rt::sec(m, 60, 1) + CHAO_EXEC;
    // The exec's own walker (.text 0xAA0) also calls the node transform
    // 0x1454: keep the pose for the draw walker only, or the exec uses up
    // the node indices 0..39 and the draw gets none.
    let draw = CHAO_DRAW.replace(false);
    rel_original(c, m, a, chao_exec);
    CHAO_DRAW.set(draw);
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
    let f = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
    if f % 60 == 0 {
        eprintln!("chao-player lvl: frame {f} draw {addr:#x} obj {obj:#x} motion {mot:#x} nodes {n}");
    }
    if n >= 56 {
        SKEL.set(NODES.with_borrow(|n| n.clone()));
    } else if n > 0 {
        // In the ball the motion draw is the ball model alone (2 nodes, lvl9\g30b).
        let obj2 = m.r32(P1_OBJ2);
        if (0x8000_0000..0x8C00_0000).contains(&obj2) && BALL_STATES.contains(&m.r16(obj2 + OBJ2_ACTION)) {
            (c.r, c.f[1]) = (regs, f1);
            ball_draw(c, m, addr);
        }
    }
    // A new rider skeleton size: its node positions in root space, once.
    if n > 8 && SEEN_SKEL.with_borrow_mut(|s| s.insert((obj, n))) {
        let nodes = NODES.with_borrow(|v| v.clone());
        let p: Vec<[f32; 3]> = nodes.iter().map(|e| { let r = inv_mul(&nodes[0], e); [r[0][3], r[1][3], r[2][3]].map(|x| (x * 1000.0).round() / 1000.0) }).collect();
        eprintln!("chao-player skel: obj {obj:#x} nodes {n} root-space {p:?}");
    }
}

/// Animation states in which the rider's own motion draw is his spin ball
/// (state 30 = PLMOT_BALL: Sonic's display swaps the body for the ball
/// model, seen with SA2_SHOW_SONIC in Green Forest, lvl9\g30s).
/// unverified: 11/12 (PLMOT_STARTSPIN/SPINDASH) draw the ball the same way.
const BALL_STATES: [u16; 3] = [30, 11, 12];

/// The game's own ball draw, scaled to the Chao's size (BOARD_SCALE, the
/// Sonic -> Chao size mob set for the board) and centred on the Chao.
fn ball_draw(c: &mut Cpu, m: &mut Mem, addr: u32) {
    // The ball's centre: its root node as this frame's capture walked it.
    let Some(ball) = NODES.with_borrow(|n| n.first().copied()) else { return };
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
    let (s, b) = (BOARD_SCALE, [ball[0][3], ball[1][3], ball[2][3]]);
    // Scale about the ball's centre, then move that centre onto the Chao's.
    let ws: [[f32; 4]; 3] = std::array::from_fn(|i| std::array::from_fn(|j| if j < 3 { w[i][j] * s } else { w[i][3] * s + (1.0 - s) * b[i] + target[i] - b[i] }));
    let scaled = mul34(&base, &ws);
    for r in 0..3 {
        for k in 0..4 {
            m.w32(cur_ptr + 4 * (4 * r + k) as u32, scaled[r][k].to_bits());
        }
    }
    original(c, m, addr);
    for r in 0..3 {
        for k in 0..4 {
            m.w32(cur_ptr + 4 * (4 * r + k) as u32, cur[r][k].to_bits());
        }
    }
}

/// Place the Chao on the rider right before it draws (after its own exec).
fn chao_display(c: &mut Cpu, m: &mut Mem) {
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
    if live && RIDER.get() != 0 {
        level_pose(m, c.r[3]);
        CHAO_DRAW.set(true);
        CHAO_NODE.set(0);
        let a = rt::sec(m, 60, 1) + CHAO_DISPLAY;
        return rel_original(c, m, a, chao_display);
    }
    // ChaoMain queues the model draw past this call (no walker runs inside
    // it, ev28), so the flag stays on for the frame while in a cutscene.
    CHAO_DRAW.set(ev);
    if ev {
        write_pose(m, c.r[3]);
    }
    rel_original(c, m, a, chao_display);
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
    m.w32(cw + 0x0C, m.r32(data1 + 0x0C));
    m.w32(cw + 0x10, 0);
}

pub fn install(c: &mut Cpu, _m: &mut Mem) {
    if !std::env::var("SA2_CHAO_PLAYER").is_ok_and(|v| v.trim() == "1") {
        return;
    }
    if rt::mem1() < 0x0300_0000 {
        eprintln!("chao-player: needs SA2_MEM1_MB >= 48");
        return;
    }
    // Sonic, Shadow and Metal share one exec and display; Knuckles and Rouge
    // one exec (0x8009F8A0) with their own displays (coordinator notes).
    c.rt.set_override(SONIC_EXEC, |c, m| exec(c, m, SONIC_EXEC));
    c.rt.set_override(KNUX_EXEC, |c, m| exec(c, m, KNUX_EXEC));
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
const PLAY_SE: u32 = 0x8006_B978;
const DARK_CHAO_SOUND: u32 = 0x201E;

fn voice(c: &mut Cpu, m: &mut Mem) {
    // From the start of the run: the drop line plays before the Chao is loaded.
    if c.lr == FROM_VOICE_TASK {
        if matches!(STEP.get(), Step::Live(_)) {
            call(c, m, PLAY_SE, &[DARK_CHAO_SOUND, 0, 0, 0]);
        }
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
        return rel_original(c, m, a, chao_node);
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
    if MIRROR.get() {
        return mirror_node(m, i, cur_ptr);
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
    if matches!(STEP.get(), Step::Live(_)) {
        // The rider's own display, walked for his node matrices, drawing nothing.
        LCAP.set(true);
        original(c, m, addr);
        LCAP.set(false);
        return;
    }
    original(c, m, addr);
}

fn exec(c: &mut Cpu, m: &mut Mem, addr: u32) {
    let task = c.r[3];
    original(c, m, addr);
    let data1 = m.r32(task + 0x34);
    if data1 == 0 {
        return;
    }
    let pos = [m.r32(data1 + 0x14), m.r32(data1 + 0x18), m.r32(data1 + 0x1C)];
    if let Step::Live(chao) = STEP.get() {
        RIDER.set(data1);
        pin(m, chao, data1);
        // ChaoMain AL_DisableNade (.text 0x5A3C, = CCL_Disable(task, 2)):
        // the rider's B is its own move, not Pet (action 31). Each frame, as
        // the Chao's behaviours may enable touch again (AL_EnableTouch 0x5AB0).
        call(c, m, rt::sec(m, 60, 1) + AL_DISABLE_NADE, &[chao]);
        // Physics stays native for Knuckles and Rouge (mob's rules).
        if addr == SONIC_EXEC {
            physics_row(m);
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
    zero(m, INFO, 0x800);
    // Owner Chao (server game.rs "admin-dark-chaos"): Dark Chaos, type 0x16 in
    // the byte the game's own egg code sets (2 = child there).
    // shortcut: alignment and stats are not written yet; their GC offsets in
    // the save info are unverified.
    m.w8(INFO + 0x80, 0x16);
    // Gene "analysed" flag (info+0x438), the 1 AL_GeneAnalyzeCommon (0x1002C)
    // itself writes. With it 0, CreateChaoExtra runs AL_GeneAnalyze, which
    // resets the type to 2 (child): seen as type 2 after creation.
    m.w8(INFO + 0x438, 1);
    m.w8(INFO + 0x84, 0xFF);
    for (o, w) in pos.into_iter().enumerate() {
        m.w32(POS + 4 * o as u32, w);
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
    c.rt.set_override(s1 + AL_NODE_TRANSFORM, chao_node);
    for (a, f) in AL_WALKERS.iter().zip([w0 as rt::Func, w1, w2, w3, w4, w5]) {
        c.rt.set_override(s1 + a, f);
    }
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
    use crate::chao_retarget::ChaoRest;

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
