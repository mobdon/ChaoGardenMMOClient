//! The walker exec (0x800B3A8C) and the per-frame steps around its mode
//! handlers, ported from SA2B's GameCube main.dol (GSNE8P) and checked
//! against it by tests/walker_exec.rs.
//!
//! Engine-owned work (tasks, sounds) is not run here: it is returned as
//! [`EngineCall`]s keyed by their GSNE8P address, for the runtime to run.
// Negated comparisons keep the compiled branches' NaN behaviour.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::ninja_matrix::{rotate_by_angles, MatrixStack, Vec3};

/// A call into engine code the runtime owns, by GSNE8P address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineCall {
    /// CreateElementalTask (0x80071E0C)(list, flags, exec, name). When the
    /// task is created, byte 0 of its player work (+0x40) is set to
    /// `pwp_byte0`, if given.
    CreateElementalTask {
        list: u8,
        flags: u8,
        exec: u32,
        name: u32,
        pwp_byte0: Option<u8>,
    },
}

/// The per-player history the engine keeps at r13-0x7910 (ring indices),
/// r13-0x7908 (positions) and r13-0x7904 (angles): 0x100 entries of 0xC
/// bytes per player.
#[derive(Debug, Clone)]
pub struct PlayerHistory {
    pub index: [u8; 8],
    pub pos: Vec<Vec3>,
    pub ang: Vec<[i32; 3]>,
}

impl PlayerHistory {
    pub fn new(players: usize) -> Self {
        Self {
            index: [0; 8],
            pos: vec![[0.0; 3]; players * 0x100],
            ang: vec![[0; 3]; players * 0x100],
        }
    }
}

/// The fields fn_8004F4D0 reads and writes.
#[derive(Debug, Clone, Copy, Default)]
pub struct EndFrame {
    /// Task flag (+4) and byte +1.
    pub task_flag: i16,
    pub task_b1: u8,
    pub angles: [i32; 3],
    pub pos: Vec3,
    /// Motion work `acc` (+0xC).
    pub mw_acc: Vec3,
    /// Player bytes +0, +1, +3, +0xD; halfwords +0x10, +0x14, +0x16, +0x1A,
    /// +0x1E; words +0x24, +0xA8, +0xAC, +0x1B8; floats +0x38, +0x4C..+0x60,
    /// +0x13C (head height), +0x1B0 (water level).
    pub pnum: u8,
    pub p_b1: u8,
    pub p_b3: u8,
    pub p_bd: u8,
    pub p_h10: i16,
    pub p_h14: i16,
    pub air_timer: i16,
    pub p_h1a: i16,
    pub p_h1e: i16,
    pub p_w24: u32,
    pub p_wa8: u32,
    pub p_wac: u32,
    pub p_w1b8: u32,
    pub p_f38: f32,
    pub p_f4c: [f32; 6],
    pub head: f32,
    pub water: f32,
    /// The collision info's byte +2 (task +0x2C, then +0xC).
    pub ccl_b2: u8,
}

const AIR_LIMIT: i16 = 0x474;
const DROWN_EXEC: u32 = 0x8003_A924;
const DROWN_NAME: u32 = 0x8017_4404;

/// fn_8004F4D0: the end of a player's frame. Clears the frame's task flags
/// (0x1038), the motion work's acceleration and the player's per-frame
/// fields, counts the two timers +0x14 and +0x1A down to zero, records the
/// position and angles of the player's task (`playertwp[pnum]`, given as
/// `player_pos` / `player_ang`) in the next history slot, and runs the air
/// timer: under water (surface flags 0x2002 with the water at or above the
/// head) it counts up and at 0x474 the drowning task is created; otherwise,
/// and in the exempt states, it resets. `r13_7900` is the word at
/// r13-0x7900, cleared.
pub fn end_frame(
    stack: &mut MatrixStack,
    w: &mut EndFrame,
    hist: &mut PlayerHistory,
    player_pos: Vec3,
    player_ang: [i32; 3],
    game_mode_42: u8,
    r13_7900: &mut u32,
) -> Vec<EngineCall> {
    let mut out = vec![];
    w.task_flag &= !0x1038;
    w.task_b1 = 0;
    w.mw_acc = [0.0; 3];
    w.p_f4c = [0.0; 6];
    w.p_f38 = 0.0;
    w.p_h14 = if w.p_h14 > 0 { w.p_h14 - 1 } else { 0 };
    w.p_h1a = if w.p_h1a > 0 { w.p_h1a - 1 } else { 0 };
    let n = w.pnum as usize;
    let idx = hist.index[n].wrapping_add(1);
    hist.index[n] = idx;
    let slot = (n << 8) + idx as usize;
    hist.pos[slot] = player_pos;
    hist.ang[slot] = player_ang;
    w.p_h1e = 0;
    if (w.p_b1 == 4 && w.p_w24 & 0x2000 != 0)
        || w.p_b3 == 0x10
        || game_mode_42 == 1
        || w.p_h10 & 0x4000 != 0
        || w.p_w1b8 & 0x2002 == 0
    {
        w.air_timer = 0;
    } else {
        let head = rotate_by_angles(stack, w.angles, [0.0, w.head, 0.0]);
        let head_y = head[1] + w.pos[1];
        if w.water >= head_y {
            w.air_timer = w.air_timer.wrapping_add(1);
            if w.air_timer == AIR_LIMIT {
                out.push(EngineCall::CreateElementalTask {
                    list: 8,
                    flags: 0,
                    exec: DROWN_EXEC,
                    name: DROWN_NAME,
                    pwp_byte0: Some(w.pnum),
                });
            }
        } else {
            w.air_timer = 0;
        }
    }
    *r13_7900 = 0;
    w.p_wa8 = 0;
    w.p_wac = 0;
    if w.p_bd == 0x32 {
        w.p_bd = 0;
    }
    w.ccl_b2 &= !0x70;
    out
}

fn word(b: &[u8], o: usize) -> u32 {
    u32::from_be_bytes(b[o..o + 4].try_into().unwrap())
}

fn set_word(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_be_bytes());
}

const CCL_HALF: f32 = 0.5; // r2-0x696C
const CCL_RANGE: f32 = 50.0; // r2-0x6980

/// What fn_800AC698 reads besides the collision info.
#[derive(Debug, Clone, Copy, Default)]
pub struct CclUpdate {
    /// Task mode (+0), byte +2, flag (+4) and position (+0x14).
    pub mode: u8,
    pub task_b2: u8,
    pub pos: Vec3,
    /// Player halfword +0x10, float +0x21C and the three words at +0x1E8.
    pub p_h10: i16,
    pub p_f21c: f32,
    pub p_1e8: [u32; 3],
}

/// fn_800AC698: refresh the walker's collision info (the task's CCL,
/// +0x2C, whose info array at +0xC is `info`, in native big-endian bytes):
/// entries 2-4 get flag 0x10 (entry 2 then loses it again), entries 0 and 1
/// take the push mode 3 or 0 from player bit 0x8000, entry 1 sits at the
/// task's position half way up to +0x21C, entry 0's kind is 0xD in mode
/// 0x41 sub 3, and in modes 0x3D and 0x3E entry 3 is unflagged and takes
/// the player's +0x1E8 vector. Finally fn_800068BC sets the CCL's range
/// (+8) to 50. `ccl_range` is that field; nothing happens without a CCL.
pub fn ccl_update(info: Option<(&mut [u8], &mut f32)>, u: &CclUpdate) {
    let Some((b, ccl_range)) = info else { return };
    for o in [0x64, 0x94, 0xC4] {
        set_word(b, o, word(b, o) | 0x10);
    }
    let push = if u.p_h10 as u16 & 0x8000 != 0 { 3 } else { 0 };
    for o in [3, 0x33] {
        b[o] = (b[o] & !3) | push;
        b[o] = (b[o] & !0xC) | (push << 2);
    }
    b[0x38..0x3C].copy_from_slice(&u.pos[0].to_be_bytes());
    b[0x3C..0x40].copy_from_slice(&(CCL_HALF * (u.pos[1] + u.p_f21c)).to_be_bytes());
    b[0x40..0x44].copy_from_slice(&u.pos[2].to_be_bytes());
    set_word(b, 0x64, word(b, 0x64) & !0x10);
    b[0] = if u.mode == 0x41 && u.task_b2 == 3 {
        0xD
    } else {
        0
    };
    if u.mode == 0x3D || u.mode == 0x3E {
        set_word(b, 0x94, word(b, 0x94) & !0x10);
        for (j, v) in u.p_1e8.iter().enumerate() {
            set_word(b, 0x98 + 4 * j, *v);
        }
    }
    *ccl_range = CCL_RANGE;
}
