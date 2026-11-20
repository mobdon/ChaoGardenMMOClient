//! Player load (milestone E4 part 1; ENGINE-MAP.md 4.3 and 8):
//! `fn_8001DEA8` (load players by character), the Mech Eggman loader
//! `fn_800B5D2C` (character 7, PhysicsArray row 7), the shared player-work
//! setup `fn_8004F838` and `fn_800B5630`.
//!
//! The target is the native one-player story: character 7 with G+0x1E = 0.
//! Other characters' loaders, the row-0xE walker loader 0x800B56E8 (two-player
//! only) and the engine routines the loader calls are logged as
//! [`Event::Engine`] and not run (fail closed). Values those routines return
//! (motion and model handles, a texture size) come from a [`PlayerHost`].
//!
//! Pointer words the native code stores into player work are kept as
//! [`Ptr`]s, not addresses.
use crate::task::{Event, Runtime, TaskId};
use std::collections::BTreeMap;

/// The walker exec, installed by every walker loader.
pub const WALKER_EXEC: u32 = 0x800B_3A8C;
/// "EggWalker", the task name the walker loaders pass.
pub const NAME_EGGWALKER: u32 = 0x8017_5024;
/// Engine-call argument encoding for a pointer into the loading player's work.
pub const ARG_PWP: u32 = 0xFF00_0000;
/// Engine-call argument encoding for allocation `n`.
pub const ARG_ALLOC: u32 = 0xFE00_0000;
/// Size of a player work block.
pub const PLAYER_WORK: usize = 0x454;
/// Character loaders `fn_8001DEA8` calls, by character 0..8.
pub const LOADERS: [u32; 8] = [
    0x8008_74C0,
    0x8008_6E3C,
    0x800A_9930,
    0x800D_92E4,
    0x800A_2360,
    0x800A_1F00,
    0x800B_5AC0,
    0x800B_5D2C,
];
/// The row-0xE walker loader (two-player alternate), not run.
pub const ALT_WALKER_LOADER: u32 = 0x800B_56E8;
/// `fn_8001D74C(player)`: start position (not ported).
pub const START_POSITION: u32 = 0x8001_D74C;
pub const CCL_INIT: u32 = 0x8000_F8B8;
pub const MOTION_LOAD: u32 = 0x8007_2548;
pub const MODEL_LOAD_A: u32 = 0x8005_0210;
pub const MODEL_LOAD_B: u32 = 0x8005_0094;
pub const CHAR_SETUP: u32 = 0x8006_45F8;
pub const WORK_SETUP: u32 = 0x8003_D8D0;
pub const PLAYER_SETUP: u32 = 0x8001_0CC4;
pub const TEX_BUFFER_SIZE: u32 = 0x8010_3128;
pub const TEX_INIT: u32 = 0x8010_334C;
pub const TEX_INIT_LOD: u32 = 0x8010_3608;

// .sdata2 constants the loaders read.
const SPECIAL_138: f32 = f32::from_bits(0x3D8F_5C29); // r2-0x7578
const SPECIAL_114: f32 = f32::from_bits(0xBBE5_6042); // r2-0x73F8
const SPECIAL_110: f32 = f32::from_bits(0xBC9B_A5E3); // r2-0x73F4
const SCALE_1C0: f32 = f32::from_bits(0x3E4C_CCCD); // r2-0x7590
const ONE: f32 = 1.0; // r2-0x75C8
const HEIGHT_12: f32 = 12.0; // r2-0x68DC
const HEIGHT_10: f32 = 10.0; // r2-0x69B4
const HEIGHT_PLUS: f32 = 2.0; // r2-0x699C
/// Stages with the special values in `fn_8004F838` (jump tables
/// 0x80197340 / 0x801972E0, stages 0x1E..0x35).
const SPECIAL_STAGES: [i16; 7] = [30, 32, 40, 43, 44, 50, 53];

/// A pointer the native code stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ptr {
    /// Allocation `n` (in allocation order) plus an offset.
    Alloc(u32, u32),
    /// A task.
    Task(TaskId),
    /// A task's +0x34 twp.
    Twp(TaskId),
    /// A task's +0x38 work.
    Mwp(TaskId),
}

/// What the loader needs from the rest of the engine and the game data.
pub trait PlayerHost {
    /// PhysicsArray row (0x80196578 + 0x84 * row; Steam: `sonic2app.exe`).
    fn physics_row(&mut self, row: i32) -> [u8; 0x84];
    /// The upgrade bytes 0x801E50B4 + 0..0x1D.
    fn upgrades(&mut self) -> [u8; 0x1D];
    /// The value an unported routine returns (handles, a size).
    fn engine_value(&mut self, addr: u32, args: &[u32]) -> u32;
    /// `CCL_Init` result stored at twp +0x2C (0 when collision is not ported).
    fn collision(&mut self, task: TaskId) -> u32;
}

/// What the player load reads and writes besides tasks.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Players {
    /// G 0x801CC168.
    pub g: Vec<u8>,
    /// 0x803AD820.
    pub stage: i16,
    /// 0x803AD828 / 0x803AD82C.
    pub chars: [i32; 2],
    /// Player tables at 0x801E5C48 + 0x1AE0 (playerpwp), +0x1B00, +0x1B20
    /// (playertwp), +0x1B40 (playertp).
    pub pwp: [Option<Ptr>; 4],
    pub mwp: [Option<Ptr>; 4],
    pub twp: [Option<Ptr>; 4],
    pub tp: [Option<Ptr>; 4],
    /// Pointer words stored in player work: (task, offset) → pointer.
    pub ptrs: BTreeMap<(u32, usize), Ptr>,
    /// Sizes of the allocations made so far (index = allocation number).
    pub allocs: Vec<u32>,
    /// The float the loader writes through the collision pointer
    /// (`[[twp+0x2C]+0xC]+0x14`), when the host gave one.
    pub collision_radius: Vec<(TaskId, u32)>,
}

impl Players {
    fn g8(&self, off: usize) -> u8 {
        self.g.get(off).copied().unwrap_or(0)
    }
    fn alloc(&mut self, size: u32) -> u32 {
        self.allocs.push(size);
        (self.allocs.len() - 1) as u32
    }
}

fn work(rt: &mut Runtime, t: TaskId) -> &mut Vec<u8> {
    rt.task_mut(t)
        .and_then(|x| x.w40.as_mut())
        .expect("player work installed by the loader")
}

fn put32(w: &mut [u8], off: usize, v: u32) {
    w[off..off + 4].copy_from_slice(&v.to_be_bytes());
}
fn put16(w: &mut [u8], off: usize, v: u16) {
    w[off..off + 2].copy_from_slice(&v.to_be_bytes());
}
fn get32(w: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([w[off], w[off + 1], w[off + 2], w[off + 3]])
}
fn putf(w: &mut [u8], off: usize, v: f32) {
    put32(w, off, v.to_bits());
}

/// `fn_8001DEA8`: load player 0, and player 1 when G+0x1E is set.
pub fn load_players(rt: &mut Runtime, pl: &mut Players, host: &mut dyn PlayerHost) {
    for player in 0..2u32 {
        if player == 1 && pl.g8(0x1E) as i8 == 0 {
            continue;
        }
        let c = pl.chars[player as usize];
        for ch in [0, 1, 4, 5, 2, 3, 7, 6] {
            if c == ch {
                if ch == 7 {
                    mech_eggman(rt, pl, host, player);
                } else {
                    rt.engine(LOADERS[ch as usize], vec![player]);
                }
            }
        }
        rt.engine(START_POSITION, vec![player]);
    }
}

/// `fn_800B5D2C`: the Mech Eggman loader (row 7).
pub fn mech_eggman(rt: &mut Runtime, pl: &mut Players, host: &mut dyn PlayerHost, player: u32) {
    let p = player as usize;
    if pl.g8(0x1E) as i8 != 0 && pl.g8(0x55 + p) as i8 != 0 {
        rt.engine(ALT_WALKER_LOADER, vec![player]);
        return;
    }
    let Ok(t) = rt.create_elemental_task(7, 1, WALKER_EXEC, NAME_EGGWALKER) else {
        return;
    };
    let a = pl.alloc(PLAYER_WORK as u32);
    if let Some(task) = rt.task_mut(t) {
        task.w40 = Some(vec![0; PLAYER_WORK]);
    }
    pl.ptrs.retain(|(id, _), _| *id != t.0);
    let pwp = work(rt, t);
    pwp[0] = player as u8;
    pwp[1] = 7;
    pwp[3] = 7;
    put16(pwp, 0x18, 0);
    if let Some(twp) = rt.task_mut(t).and_then(|x| x.twp.as_mut()) {
        twp[1] = 0;
        twp[0] = 0;
    }
    player_work(rt, pl, host, player, t, a);
    let pwp = work(rt, t);
    put32(pwp, 0x198, 0x801C_3C5C);
    put16(pwp, 0x176, 0);
    rt.engine(CCL_INIT, vec![t.0, 0x801C_69EC, 5, 0]);
    let col = host.collision(t);
    if let Some(twp) = rt.task_mut(t).and_then(|x| x.twp.as_mut()) {
        put32(twp, 0x2C, col);
    }
    if col != 0 {
        let r = get32(work(rt, t), 0x130);
        pl.collision_radius.push((t, r));
    } else {
        rt.events.push(Event::Anomaly(
            "CCL_Init not ported: collision radius write skipped",
        ));
    }
    let two = pl.g8(0x1E) as i8 != 0;
    let alt = pl.g8(0x53 + p) as i8;
    if !two || alt == 0 {
        let v = call(rt, host, MOTION_LOAD, 0x8017_50D0);
        put32(work(rt, t), 0x448, v);
        let v = call(rt, host, MODEL_LOAD_A, 0x8017_50DC);
        put32(work(rt, t), 0x44C, v);
    } else if alt == 1 {
        let v = call(rt, host, MOTION_LOAD, 0x8017_50EC);
        put32(work(rt, t), 0x448, v);
        let v = call(rt, host, MODEL_LOAD_A, 0x8017_50F8);
        put32(work(rt, t), 0x44C, v);
        work(rt, t)[2] = 1;
    }
    let c = work(rt, t)[1] as i8 as i32 as u32;
    rt.engine(CHAR_SETUP, vec![c]);
    let v = call(rt, host, MOTION_LOAD, 0x803A_FC5C);
    put32(work(rt, t), 0x444, v);
    let v = call(rt, host, MODEL_LOAD_B, 0x8017_5108);
    put32(work(rt, t), 0x450, v);
    rt.engine(WORK_SETUP, vec![ARG_PWP | 0x174]);
    let pnum = work(rt, t)[0] as i8 as i32 as u32;
    rt.engine(PLAYER_SETUP, vec![pnum]);
    if let Some(task) = rt.task_mut(t) {
        task.slots[2] = 0x800B_245C;
        task.slots[1] = 0x800B_15B8;
        task.slots[3] = 0x800B_110C;
        task.slots[7] = 0x800B_0D7C;
        task.slots[6] = 0x800B_10CC;
    }
    let stage = pl.stage;
    let g0 = pl.g8(0) as i8;
    let pwp = work(rt, t);
    if two || g0 != 0 {
        put32(pwp, 0x24, 0x0010_0000);
    } else if stage == 0x1D && player == 1 {
        put32(pwp, 0x24, 0x0090_0000);
    } else if stage == 0x14 && player == 1 {
        put32(pwp, 0x24, 0);
    }
    walker_height(pwp, stage);
}

fn call(rt: &mut Runtime, host: &mut dyn PlayerHost, addr: u32, arg: u32) -> u32 {
    rt.engine(addr, vec![arg]);
    host.engine_value(addr, &[arg])
}

/// `fn_800B5630`: the walker's height value at work +0x48.
pub fn walker_height(pwp: &mut [u8], stage: i16) {
    if stage == 0x1D && pwp[0] == 1 {
        putf(pwp, 0x48, HEIGHT_12);
    } else if stage == 0x14 && pwp[0] == 1 {
        putf(pwp, 0x48, HEIGHT_10);
    } else {
        match pwp[1] as i8 {
            7 => {
                putf(pwp, 0x48, HEIGHT_10);
                if get32(pwp, 0x24) & 0x0080_0000 != 0 {
                    let v = f32::from_bits(get32(pwp, 0x48)) + HEIGHT_PLUS;
                    putf(pwp, 0x48, v);
                }
            }
            6 => putf(pwp, 0x48, HEIGHT_10),
            _ => {}
        }
    }
}

/// Upgrade bits `fn_8004F838` ORs into work +0x24, per character:
/// (index into the upgrade bytes, bit).
fn upgrade_bits(c: i8) -> &'static [(usize, u32)] {
    match c {
        0 => &[(0, 0x1), (1, 0x2), (2, 0x4), (3, 0x8), (4, 0x10), (5, 0x20)],
        1 => &[
            (0x10, 0x1_0000),
            (0x11, 0x2_0000),
            (0x12, 0x4_0000),
            (0x13, 0x8_0000),
        ],
        6 => &[(6, 0x40), (7, 0x80), (8, 0x100), (9, 0x200)],
        7 => &[
            (0x14, 0x10_0000),
            (0x15, 0x20_0000),
            (0x16, 0x40_0000),
            (0x17, 0x80_0000),
            (0x18, 0x100_0000),
        ],
        4 => &[
            (0xA, 0x400),
            (0xB, 0x800),
            (0xC, 0x1000),
            (0xD, 0x2000),
            (0xE, 0x4000),
        ],
        5 => &[
            (0x19, 0x200_0000),
            (0x1A, 0x400_0000),
            (0x1B, 0x800_0000),
            (0x1C, 0x1000_0000),
        ],
        _ => &[],
    }
}

/// `fn_8004F838(player, task)`: shared player-work setup.
fn player_work(
    rt: &mut Runtime,
    pl: &mut Players,
    host: &mut dyn PlayerHost,
    player: u32,
    t: TaskId,
    pwp_alloc: u32,
) {
    let p = player as usize;
    let a90 = pl.alloc(0x84);
    pl.ptrs.insert((t.0, 0x90), Ptr::Alloc(a90, 0));
    pl.tp[p] = Some(Ptr::Task(t));
    pl.twp[p] = Some(Ptr::Twp(t));
    pl.mwp[p] = Some(Ptr::Mwp(t));
    pl.pwp[p] = Some(Ptr::Alloc(pwp_alloc, 0));
    pl.ptrs.insert((t.0, 0x190), Ptr::Alloc(pwp_alloc, 0x64));
    pl.ptrs.insert((t.0, 0x194), Ptr::Alloc(pwp_alloc, 0x44));
    let stage = pl.stage;
    let pwp = work(rt, t);
    put16(pwp, 0x174, 0);
    let row = pwp[3] as i8 as i32;
    let c = pwp[1] as i8;
    let phys = host.physics_row(row);
    pwp[0xC0..0xC0 + 0x84].copy_from_slice(&phys);
    let special = SPECIAL_STAGES.contains(&stage);
    let char_phys = host.physics_row(i32::from(c));
    let pwp = work(rt, t);
    if special && (0..=7).contains(&c) {
        putf(pwp, 0x138, SPECIAL_138);
    } else {
        pwp[0x138..0x13C].copy_from_slice(&char_phys[0x78..0x7C]);
    }
    if special && matches!(c, 0 | 1 | 4 | 5 | 6 | 7) {
        putf(pwp, 0x114, SPECIAL_114);
        putf(pwp, 0x110, SPECIAL_110);
    }
    let v = SCALE_1C0 * f32::from_bits(get32(pwp, 0x130));
    putf(pwp, 0x1C0, v);
    for off in [0x6C, 0x68, 0x64] {
        putf(pwp, off, 0.0);
    }
    putf(pwp, 0x40, ONE);
    if let Some(twp) = rt.task_mut(t).and_then(|x| x.twp.as_mut()) {
        twp[2] = 1;
    }
    let ab4 = pl.alloc(0xF * 0x20);
    pl.ptrs.insert((t.0, 0xB4), Ptr::Alloc(ab4, 0));
    let pwp = work(rt, t);
    put16(pwp, 0x1E, 0);
    if let Some(twp) = rt.task_mut(t).and_then(|x| x.twp.as_mut()) {
        for off in [0x28, 0x24, 0x20] {
            putf(twp, off, ONE);
        }
    }
    let up = host.upgrades();
    let pwp = work(rt, t);
    let mut bits = get32(pwp, 0x24);
    for &(i, b) in upgrade_bits(c) {
        if up[i] != 0 {
            bits |= b;
        }
    }
    put32(pwp, 0x24, bits);
    let size = if matches!(row, 6 | 7 | 13 | 14 | 16) {
        0x100
    } else {
        0x80
    };
    rt.engine(TEX_BUFFER_SIZE, vec![size, size, 2, 0, 0]);
    let bytes = host.engine_value(TEX_BUFFER_SIZE, &[size, size, 2, 0, 0]);
    let tex = pl.alloc(bytes);
    pl.ptrs.insert((t.0, 0x328), Ptr::Alloc(tex, 0));
    rt.engine(
        TEX_INIT,
        vec![ARG_PWP | 0x308, ARG_ALLOC | tex, size, size, 2, 0, 0, 0],
    );
    rt.engine(TEX_INIT_LOD, vec![ARG_PWP | 0x308, 1, 1, 0, 0, 0]);
}
