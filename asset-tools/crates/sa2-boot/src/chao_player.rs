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
    /// Rider position at the previous Chao display (for the emotion ball).
    static LAST: Cell<[f32; 3]> = const { Cell::new([0.0; 3]) };
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
    // SA2_EV_SKIP=exec,display (debug): skip this step in a cutscene.
    if RIDER.get() == 0 && std::env::var("SA2_EV_SKIP").is_ok_and(|v| v.contains("exec")) {
        return;
    }
    let a = rt::sec(m, 60, 1) + CHAO_EXEC;
    rel_original(c, m, a, chao_exec);
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
    rel_original(c, m, a, chao_display);
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
    c.rt.set_override(SONIC_EXEC, exec);
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
    c.rt.set_override(CNK_CACHE_DRAW, cnk_cache_draw);
    sa2_recomp::os::add_frame_hook(ev_frame_hook);
    c.rt.set_override(ADX_START_AFS, voice);
    c.rt.set_override(SONIC_DISP, display);
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
    if EV_SONIC.contains(&obj) {
        return; // the cutscene's Sonic (eyes): the Chao stands in
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
    let Some(root) = NODES.with_borrow(|n| n.first().copied()) else { return };
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
    let cur = mtx(m, m.r32(NJ_CURRENT_MATRIX));
    let base = mtx(m, m.r32(NJ_BASE_MATRIX));
    NODES.with_borrow_mut(|n| n.push(inv_mul(&base, &cur)));
    c.r[3] = 0;
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
    let pos = [m.r32(data1 + 0x14), m.r32(data1 + 0x18), m.r32(data1 + 0x1C)];
    if let Step::Live(chao) = STEP.get() {
        RIDER.set(data1);
        pin(m, chao, data1);
        return;
    }
    tick(c, m, pos);
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
    for (name, texlist) in TEXTURES {
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
    // In a cutscene (no rider) the bank load is skipped: suspected of the
    // ev15 stall right after Live. shortcut: no Chao voice in the intro yet.
    if RIDER.get() != 0 || EV_SPOT.get().is_none() {
        call(c, m, LOAD_BANK, &[WALKER_BANK_2P]);
    }
    call(c, m, s1 + AL_SET_MOTION_LINK, &[chao, STAND_MOTION]);
    Step::Live(chao)
}
