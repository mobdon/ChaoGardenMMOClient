//! The walker exec (0x800B3A8C) and the per-frame steps around its mode
//! handlers, ported from SA2B's GameCube main.dol (GSNE8P) and checked
//! against it by tests/walker_exec.rs.
//!
//! Engine-owned work (tasks, sounds) is not run here: it is returned as
//! [`EngineCall`]s keyed by their GSNE8P address, for the runtime to run.
// Negated comparisons keep the compiled branches' NaN behaviour.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::{
    land_nodes::nj_scalor,
    native_work::{pw, tw, Work},
    ninja_matrix::{rotate_by_angles, Axis, MatrixStack, Vec3},
    ninja_trig::fmadds,
    walker_act::{action_step, go_stand, reset_f48, ActGlobals, ActSteps, LockWorld},
    walker_collide::adjust_angle,
    walker_move::SteerCalls,
};

/// A call into engine code the runtime owns, by GSNE8P address.
#[derive(Debug, Clone, Copy, PartialEq)]
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
    /// fn_80011714(id): play a voice line.
    PlayVoice { id: i16 },
    /// A visual-effect routine not ported yet (for example fn_800BCF50,
    /// the walker's debris burst), with its argument. It only feeds what
    /// is drawn.
    Deferred { addr: u32, arg: i32 },
    /// A visual-effect routine not ported yet that takes the player number
    /// and two points (for example fn_800B9020, the walker's jet effect,
    /// with player +0x3F4/+0x40C or +0x400/+0x418). It uses the engine's
    /// `rand`.
    DeferredAt {
        addr: u32,
        arg: i32,
        points: [crate::ninja_matrix::Vec3; 2],
    },
    /// fn_80069C58(id, task work): stop this task's sound `id`.
    StopSound { id: i32 },
    /// _rename_KillPlayer (0x8003BB7C)(pnum).
    KillPlayer { pnum: i8 },
    /// The battle voice queue (fn_800118E0/fn_8001162C over both players,
    /// then fn_800117EC(id, 3)).
    BattleVoice { id: i32 },
    /// A call through the function pointer stored at `ptr` (for example
    /// 0x801A2A18, the ring loss), with one argument.
    Indirect { ptr: u32, arg: i32 },
    /// Write `len` bytes of `value` into the game state block 0x801CC168
    /// at `at`.
    GameStateBytes { at: u16, len: u8, value: u8 },
    /// CCL_Entry (0x8000F4EC)(task): enter the task's collision for this
    /// frame.
    CclEntry,
    /// Controller rumble: fn_8002FB2C or fn_8002F9D4 (pnum, a, b, c).
    Rumble { addr: u32, args: [i32; 4] },
    /// A sound engine call: SE_Call (0x8006B978)(id, a, b, c) or
    /// fn_8006B7EC(id, a, b, volume, position).
    Sound {
        addr: u32,
        id: i32,
        args: [i32; 3],
        pos: Option<crate::ninja_matrix::Vec3>,
    },
}

/// The engine as the ported routines call it: engine-owned calls run at
/// once, in the order the routine makes them, so that calls which use
/// the engine's `rand` (the deferred effects) keep its sequence.
pub trait Engine {
    fn call(&mut self, c: EngineCall);
    /// MSL `rand` (0x8010DC88).
    fn rand(&mut self) -> i32;
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

/// The routines the walker exec calls on its task, motion and player work,
/// by GSNE8P address. The runtime maps each address to its port (for
/// example 0x80047A1C to `walker_move::air_level`) and fails closed on
/// one that is not ported.
pub trait ExecSteps {
    /// A routine taking (task or task work, motion work, player work).
    /// Returns its result (0 for void routines).
    fn call(&mut self, addr: u32, t: &mut Work, m: &mut Work, p: &mut Work) -> i32;
    /// SetVelocityP (0x8003A3DC)(pnum, v).
    fn set_velocity(&mut self, pnum: i8, v: Vec3);
    /// fn_8004E560(task work, player work, &angle, &magnitude).
    fn analog(&mut self, t: &Work, p: &Work) -> Option<(i32, f32)>;
    /// The frame count and speed of the action at player +0x178 and the
    /// motion work's scale (*+0x194): what the footstep timing reads.
    fn step_motion(&mut self, p: &Work) -> (u32, f32, f32);
    /// The collision info: set entry 0's and entry 2's centers (+8, +0x68).
    fn set_ccl_centers(&mut self, a: Vec3, b: Vec3);
}

/// What the exec reads besides its work areas and the action step's
/// globals.
#[derive(Debug, Clone, Default)]
pub struct ExecGlobals {
    pub act: ActGlobals,
    /// The laser button bytes at r13-0x7A2C.
    pub laser: [i8; 8],
    /// The task's collision +0xA0 halfword, and its first info entry's
    /// +0xC float.
    pub ccl_a0: i16,
    pub info0_c: f32,
    /// The point the lock-on target at player +0x39C points at, if any.
    pub lock_target: Option<Vec3>,
}

const LASER_SPREAD: f32 = 3.0; // r2-0x6970
const JET_MAX: f32 = 95.0; // r2-0x68F0
const JET_BASE: f32 = 6.5; // r2-0x68EC
const HALF: f32 = 0.5; // r2-0x696C
const GAUGE_UP: f32 = f32::from_bits(0x3ccc_cccd); // r2-0x68E8
const GAUGE_DOWN: f32 = f32::from_bits(0x3d4c_cccd); // r2-0x6954
const CANNON_STEP: f32 = f32::from_bits(0x3dcc_cccd); // r2-0x69B8
const RUN_STICK: f32 = f32::from_bits(0x3f78_51ec); // r2-0x68E4
const PUSH_SCALE: f32 = f32::from_bits(0x3f7a_e148); // r2-0x68F4, 0.98
const PUSH_BASE: Vec3 = [0.25, 0.0, 0.0]; // 0x80174FC8 / 0x80174FD4
const JET_REAR: f32 = 7.5; // r2-0x6920
const JET_DUST: f32 = -4.0; // r2-0x68E0
const STEP_MIN: f32 = f32::from_bits(0x3dcc_cccd); // r2-0x69B8
const STEP_EGG: f32 = 24.0; // r2-0x6994
const STEP_TAILS: f32 = 12.0; // r2-0x68DC
const HOVER_FALL: f32 = f32::from_bits(0xbeb3_3333); // r2-0x68CC
const HOVER_HIGH: f32 = 120.0; // r2-0x68D8
const HOVER_SLOW: f32 = 1.5; // r2-0x68D4
const HOVER_HOLD: f32 = f32::from_bits(0x3f93_3333); // r2-0x68D0
const ROPE_MAX: f32 = f32::from_bits(0x3ecc_cccd); // r2-0x68C8
const SWAY_MAX: f32 = 0.5; // r2-0x696C
const SWAY_MIN: f32 = -0.5; // r2-0x691C

/// fn_8002AE5C(value, target, step): move toward the target by `step`.
pub fn approach(v: f32, target: f32, step: f32) -> f32 {
    if v > target {
        let n = v - step;
        if n >= target {
            n
        } else {
            target
        }
    } else if v >= target {
        v
    } else {
        let n = v + step;
        if n <= target {
            n
        } else {
            target
        }
    }
}

fn roll_f000(p: &mut Work, o: usize, add: u32) {
    let v = p.w(o);
    if v != 0 {
        p.set_w(o, v.wrapping_add(add) & 0xF000);
    }
}

fn sound(id: i32) -> EngineCall {
    EngineCall::Sound {
        addr: 0x8006_B978,
        id,
        args: [0; 3],
        pos: None,
    }
}

fn laser_capable(mode: i8) -> bool {
    !(mode == 0x43
        || ((0xF..=0x3B).contains(&mode) && mode != 0x12 && mode != 0x11 && mode != 0x33))
}

/// The exec's laser, lock-on and gauge block (0x800B3B8C..0x800B4558).
#[allow(clippy::too_many_arguments)]
fn laser_block(
    stack: &mut MatrixStack,
    t: &mut Work,
    m: &mut Work,
    p: &mut Work,
    g: &ExecGlobals,
    s: &mut dyn ExecSteps,
    eng: &mut dyn Engine,
) {
    let pnum = p.b(pw::PNUM);
    let pn = pnum as usize;
    let gs = &g.act.gs;
    if !laser_capable(t.b(tw::MODE)) {
        p.set_b(0x360, 0);
        if p.w(0x24) & 0x20_0080 != 0 {
            roll_f000(p, 0x430, 0x1000);
        }
        p.set_h(0x364, 0);
        p.set_h(0x36C, 0);
        p.set_i(0x424, adjust_angle(p.i(0x424), 0, 0xC00));
        roll_f000(p, 0x434, 0x2000);
        p.set_i(0x428, adjust_angle(p.i(0x428), 0, 0x200));
        p.set_i(0x42C, p.i(0x42C).wrapping_add(0x200));
        if gs.b(0x49) == 0 {
            if p.f(0x374) > 0.0 {
                p.set_f(0x374, approach(p.f(0x374), 0.0, GAUGE_DOWN));
            }
            if p.f(0x378) > 0.0 {
                p.set_f(0x378, approach(p.f(0x378), 0.0, CANNON_STEP));
            }
        }
        return;
    }
    let mode = t.b(tw::MODE);
    if p.b(0x360) > 0 {
        p.set_b(0x360, p.b(0x360) - 1);
        if p.w(0x24) & 0x20_0080 != 0 {
            roll_f000(p, 0x430, 0x1000);
        }
    } else if g.act.action[pn] != 0 && mode != 0x3D && mode != 0x3E {
        let k = LASER_SPREAD + p.f(pw::SPD).abs();
        let v = p.v3(0x2E4).map(|x| x * k);
        let w24 = p.w(0x24);
        let from = p.v3(0x248);
        if w24 & 0x20_0000 != 0 || w24 & 0x80 != 0 {
            eng.call(EngineCall::DeferredAt {
                addr: 0x800B_9FF0,
                arg: pnum as i32,
                points: [v, from],
            });
            let shot = if w24 & 0x20_0000 != 0 {
                0x800B_98C4
            } else {
                0x800B_95A4
            };
            eng.call(EngineCall::DeferredAt {
                addr: shot,
                arg: pnum as i32,
                points: [v, from],
            });
            eng.call(EngineCall::Rumble {
                addr: 0x8002_FB2C,
                args: [pnum as i32, 4, 0xF, 0],
            });
            p.set_w(0x430, 0x3000);
            eng.call(sound(0x2013));
        } else {
            eng.call(EngineCall::DeferredAt {
                addr: 0x800B_A5DC,
                arg: pnum as i32,
                points: [v, from],
            });
            eng.call(EngineCall::Rumble {
                addr: 0x8002_FB2C,
                args: [pnum as i32, 4, 0x2D, 0],
            });
            eng.call(sound(0x2012));
        }
        p.set_h(0x364, 0x168);
        let b3 = p.b(3);
        p.set_b(
            0x360,
            if b3 == 6 || b3 == 7 {
                0xD
            } else if b3 == 0xD {
                8
            } else {
                0x14
            },
        );
        if (t.ub(2) == 3 || gs.b(0x1E) != 0) && p.h(0x14) <= 0 && t.h(tw::FLAG) & 3 != 0 {
            let x = p.f(pw::SPD);
            for o in [0xF4, 0xEC, 0xE8, 0xE4] {
                if x > p.f(o) {
                    p.set_f(pw::SPD, p.f(o));
                    break;
                }
            }
        }
    }
    let cannon = !(p.ub(3) == 7 && p.ub(2) == 1) && g.laser[pn] != 0 && p.h(0x364) > 0;
    if cannon {
        let n = p.h(0x364) - 1;
        p.set_h(0x364, if n < 0 { 0 } else { n });
        p.set_b(0x35C, 1);
        if p.h(0x366) <= 0 {
            s.call(0x800A_9B84, t, m, p);
        } else {
            p.set_h(0x366, p.h(0x366) - 1);
        }
        let target = if p.b(0x35E) == 0 {
            let a = p.v3(0x254);
            let b = p.v3(0x2F0);
            [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
        } else {
            let v = p.v3(0x390);
            if p.h(0x366) <= 0 {
                p.set_b(0x35E, 0);
            }
            v
        };
        eng.call(EngineCall::DeferredAt {
            addr: 0x800B_9D60,
            arg: pnum as i32,
            points: [p.v3(0x254), target],
        });
        eng.call(EngineCall::Sound {
            addr: 0x8006_B1FC,
            id: 0x200E,
            args: [1, 0, 3],
            pos: None,
        });
    } else if g.laser[pn] != 0 && p.h(0x364) <= 0 {
        p.set_b(0x35C, 0);
    }
    if p.h(0x36A) > 0 || p.h(0x368) > 0 {
        p.set_b(0x35F, 1);
    }
    if g.laser[pn] == 0 && p.h(0x368) > 0 {
        p.set_h(0x36A, p.h(0x36A).wrapping_add(p.h(0x368)));
        eng.call(EngineCall::DeferredAt {
            addr: 0x800A_BD84,
            arg: pnum as i32,
            points: [[p.h(0x368) as f32, 0.0, 0.0], [0.0; 3]],
        });
        p.set_h(0x368, 0);
        p.set_w(0x380, 0);
    }
    if p.h(0x36A) > 0 {
        if p.h(0x36C) <= 0 {
            p.set_b(0x35D, 1);
            p.set_h(0x36C, 8);
        } else {
            p.set_h(0x36C, p.h(0x36C) - 1);
        }
        if p.h(0x36C) <= 4 {
            if let Some(q) = g.lock_target {
                let mut a = p.v3(0x2D8);
                let base = p.v3(0x23C);
                let mut b = [q[0] - base[0], q[1] - base[1], q[2] - base[2]];
                a[1] = 0.0;
                b[1] = 0.0;
                if nj_scalor(a) != 0.0 && nj_scalor(b) != 0.0 {
                    let a = crate::land_contact::unit_vector(a);
                    let b = crate::land_contact::unit_vector(b);
                    let d = crate::ninja_vector::vector_angle(a, b, Some([0.0, 1.0, 0.0]));
                    let cur = p.i(0x424);
                    p.set_i(0x424, adjust_angle(cur, cur.wrapping_add(d), 0xC00));
                }
            }
        }
        p.set_w(0x39C, 0);
    } else {
        p.set_h(0x36C, 8);
        p.set_i(0x424, adjust_angle(p.i(0x424), 0, 0xC00));
    }
    roll_f000(p, 0x434, 0x2000);
    if mode != 0x3D && mode != 0x3E && mode != 0x41 {
        if p.ub(0x35C) == 1 {
            if let Some((a, _)) = s.analog(t, p) {
                let d = crate::walker_collide::sub_angle(a, t.i(tw::ANG + 4));
                p.set_i(0x428, adjust_angle(p.i(0x428), d, 0x1000));
            }
        } else {
            p.set_i(0x428, adjust_angle(p.i(0x428), 0, 0x200));
        }
    }
    p.set_i(0x42C, p.i(0x42C).wrapping_add(0x200));
    if p.ub(3) == 6 && p.h(0x36E) > 0 {
        if p.h(0x36E) > 0x60 {
            p.set_h(0x36E, 0x5F);
        }
        let k = p.h(0x36E) as f32 / JET_MAX;
        let base = p.v3(0x218);
        let dir = p.v3(0x2B4);
        let reach = JET_BASE + k;
        let v = [0, 1, 2].map(|j| fmadds(dir[j], reach, base[j]));
        let sp = -nj_scalor(p.v3(pw::SPD));
        let back = HALF * sp;
        let w = dir.map(|x| x * back);
        eng.call(EngineCall::DeferredAt {
            addr: 0x800B_8A9C,
            arg: pnum as i32,
            points: [v, w],
        });
        let n = p.h(0x36E) - 1;
        p.set_h(0x36E, if n <= 0 { 0 } else { n });
    }
    let _ = stack;
    if (gs.b(0x1E) != 0 || g.act.stage == 0x1D) && t.ub(tw::MODE) != 0x36 {
        let at = (0x2E + 3 * pnum as isize) as usize;
        if gs.b(at) != 0 {
            if p.f(0x374) < 1.0 {
                p.set_f(0x374, approach(p.f(0x374), 1.0, GAUGE_UP));
            }
        } else if gs.b(0x49) == 0 && p.f(0x374) > 0.0 {
            p.set_f(0x374, approach(p.f(0x374), 0.0, GAUGE_DOWN));
        }
        if p.ub(3) == 7 {
            let n = g.act.motion_82_frames.wrapping_sub(1) as f32;
            if gs.b(at + 1) != 0 {
                if p.f(0x378) < n {
                    p.set_f(0x378, approach(p.f(0x378), n, CANNON_STEP));
                }
            } else if gs.b(0x49) == 0 && t.ub(tw::MODE) != 0x41 && p.f(0x378) > 0.0 {
                p.set_f(0x378, approach(p.f(0x378), 0.0, CANNON_STEP));
            }
        }
    }
}

/// The push a held stick gives a standing Tails walker (modes 1 and 0x3C).
fn stick_push(s: &mut dyn ExecSteps, stack: &mut MatrixStack, t: &Work, m: &Work, p: &Work) {
    if p.ub(3) != 6 || p.ub(2) != 1 {
        return;
    }
    let Some((a, mag)) = s.analog(t, p) else {
        return;
    };
    if !(mag > RUN_STICK) {
        return;
    }
    stack.push_unit();
    stack.rotate(Axis::Y, a.wrapping_neg());
    let mut v = stack.calc_vector(PUSH_BASE);
    stack.pop();
    let sp = m.v3(0);
    v = [v[0] + sp[0], v[1] + sp[1], v[2] + sp[2]];
    if nj_scalor(v) > p.f(0xD0) {
        let k = PUSH_SCALE * nj_scalor(v);
        v = crate::land_contact::unit_vector(v).map(|x| x * k);
    }
    s.set_velocity(p.b(pw::PNUM), v);
}

/// 0x800B3A8C, the walkers' exec (Tails and Eggman, and the Chao
/// walkers), on the task work `t`, motion work `m` and player work `p`.
#[allow(clippy::too_many_arguments)]
pub fn walker_exec(
    stack: &mut MatrixStack,
    t: &mut Work,
    m: &mut Work,
    p: &mut Work,
    g: &ExecGlobals,
    hist: &PlayerHistory,
    s: &mut dyn ExecSteps,
    act: &mut dyn ActSteps,
    sc: &mut dyn SteerCalls,
    world: &mut dyn LockWorld,
    eng: &mut dyn Engine,
) {
    if p.ub(pw::CHAR) == 6 && g.ccl_a0 == 5 && t.h(tw::FLAG) & 0x20 != 0 {
        t.clear_h(tw::FLAG, 0x20);
    }
    s.call(0x8005_6450, t, m, p);
    p.set_b(0x35C, 0);
    p.set_b(0x35D, 0);
    p.set_b(0x35F, 0);
    let mode = t.b(tw::MODE);
    if mode == 0x39 || mode == 0x37 {
        if t.ub(tw::SMODE) == 0x24 {
            t.clear_h(tw::FLAG, 0x3C);
            p.set_h(pw::STATUS, p.h(pw::STATUS) & 0x3FFC);
            reset_f48(p, &g.act);
            go_stand(t, m, p, &g.act, act);
        }
    } else {
        action_step(stack, t, m, p, &g.act, hist, act, sc, world, eng);
    }
    laser_block(stack, t, m, p, g, s, eng);
    let chain = |s: &mut dyn ExecSteps, t: &mut Work, m: &mut Work, p: &mut Work, first: &[u32]| {
        for a in first {
            s.call(*a, t, m, p);
        }
        s.call(0x8004_6DE4, t, m, p);
        let r = s.call(0x8004_5880, t, m, p);
        s.call(0x8003_E668, t, m, p);
        r
    };
    let pnum = p.b(pw::PNUM);
    let mut tail = true;
    match t.b(tw::MODE) {
        0 => {
            chain(s, t, m, p, &[0x8004_C734]);
            if p.ub(3) != 0xE && p.ub(0x35C) == 1 && p.h(pw::ACTION) == 0x11 {
                p.set_h(pw::ACTION, 0);
            }
            s.call(0x8005_4D04, t, m, p);
        }
        1 => {
            let speed = nj_scalor(m.v3(0));
            if p.h(0x17A) == 0x1B && p.h(0x178) != 0x1B {
                p.set_f(pw::SPD, p.f(pw::SPD) + p.f(0xE4));
            }
            stick_push(s, stack, t, m, p);
            s.call(0x8004_C734, t, m, p);
            s.call(0x8004_6DE4, t, m, p);
            if s.call(0x8004_5880, t, m, p) == 2 {
                t.set_b(tw::MODE, 0x10);
                p.set_h(pw::ACTION, 0x18);
                eng.call(EngineCall::Deferred {
                    addr: 0x800B_CF50,
                    arg: pnum as i32,
                });
                p.set_f(pw::SPD, -p.f(pw::SPD));
                p.set_f(0x6C, -p.f(0x6C));
            } else {
                s.call(0x8003_E668, t, m, p);
            }
            if p.h(pw::ACTION) != 0x1B {
                let x = p.f(pw::SPD);
                p.set_h(
                    pw::ACTION,
                    if x > p.f(0xF4) {
                        0xA
                    } else if x > p.f(0xE8) {
                        8
                    } else {
                        6
                    },
                );
                p.set_f(0x44, p.f(pw::SPD));
            }
            if p.ub(3) == 6 {
                let v_len = nj_scalor(p.v3(pw::SPD));
                if v_len >= p.f(0xE8) {
                    let base = p.v3(0x218);
                    let dir = p.v3(0x2B4);
                    let v = [0, 1, 2].map(|j| fmadds(JET_REAR, dir[j], base[j]));
                    let k = HALF * -nj_scalor(p.v3(pw::SPD));
                    let w = dir.map(|x| x * k);
                    let fast = nj_scalor(p.v3(pw::SPD)) > p.f(0xF4);
                    eng.call(EngineCall::DeferredAt {
                        addr: 0x800B_8A9C,
                        arg: pnum as i32 | (fast as i32) << 8,
                        points: [v, w],
                    });
                    if speed < p.f(0xF4) && nj_scalor(m.v3(0)) > p.f(0xF4) {
                        let d = dir.map(|x| x * JET_DUST);
                        eng.call(EngineCall::DeferredAt {
                            addr: 0x800B_F6A4,
                            arg: 0x20,
                            points: [v, d],
                        });
                        eng.call(EngineCall::DeferredAt {
                            addr: 0x800B_95A4,
                            arg: pnum as i32,
                            points: [d, v],
                        });
                    }
                }
                p.set_h(0x36E, p.h(0x36E).wrapping_add(6));
            }
            if t.h(tw::FLAG) & 1 != 0 && p.f(pw::SPD) >= STEP_MIN {
                let (nf, speed, scale) = s.step_motion(p);
                let step = scale * speed;
                let half = HALF * nf as f32;
                let add = if p.ub(pw::CHAR) == 7 {
                    STEP_EGG
                } else {
                    STEP_TAILS
                };
                let (lo, hi) = (add, half + add);
                let f = p.f(0x184);
                let hit = (f >= lo && f < step + lo) || (f >= hi - step && f < hi);
                if hit {
                    if p.h(0x370) == 0 {
                        eng.call(sound(match p.ub(3) {
                            0xD => 0x201B,
                            0xE => 0x201E,
                            _ => 0x2007,
                        }));
                        p.set_h(0x370, 1);
                    }
                } else {
                    p.set_h(0x370, 0);
                }
            }
        }
        2 => {
            path_accel(stack, t, p, g, s);
            path_speed(stack, t, m, p, s);
            s.call(0x8003_8A70, t, m, p);
        }
        6 => {
            s.call(0x8004_7A1C, t, m, p);
            if s.call(0x8003_6CE4, t, m, p) == 0 {
                s.call(0x8004_B330, t, m, p);
            }
            chain(s, t, m, p, &[]);
        }
        7 => {
            chain(s, t, m, p, &[0x8004_B330]);
        }
        8 => {
            let a = if p.f(0x68) >= p.f(0xDC) {
                0x8004_8ACC
            } else {
                0x8004_B330
            };
            chain(s, t, m, p, &[a]);
        }
        0xA => {
            s.call(0x8004_7A1C, t, m, p);
            if p.h(pw::ACTION) == 0x4A {
                s.call(0x8004_8724, t, m, p);
            } else if s.call(0x8003_6CE4, t, m, p) == 0 {
                s.call(0x8004_B330, t, m, p);
            }
            chain(s, t, m, p, &[]);
        }
        0x3C => {
            let st = g.act.stage;
            if st == 0x2B || st == 0x35 {
                if p.f(0x37C) >= t.f(tw::POS + 4) - HOVER_HIGH {
                    if p.f(0x68) < HOVER_SLOW {
                        m.set_f(0x10, HOVER_HOLD * p.f(0x138));
                    }
                    eng.call(EngineCall::Sound {
                        addr: 0x8006_AFFC,
                        id: 9,
                        args: [1, 0x14, 4],
                        pos: Some(t.v3(tw::POS)),
                    });
                } else {
                    p.set_f(0x37C, -1_000_000.0);
                }
            }
            stick_push(s, stack, t, m, p);
            s.call(0x8004_7A1C, t, m, p);
            s.call(0x8004_B330, t, m, p);
            if p.f(0x68) < HOVER_FALL {
                p.set_f(0x68, HOVER_FALL);
            }
            chain(s, t, m, p, &[]);
            eng.call(EngineCall::DeferredAt {
                addr: 0x800B_9180,
                arg: pnum as i32,
                points: [t.v3(tw::POS), [0.0; 3]],
            });
            if p.ub(3) == 6 {
                p.set_h(0x36E, p.h(0x36E).wrapping_add(8));
            }
            eng.call(EngineCall::Sound {
                addr: 0x8006_B5EC,
                id: 0x2009,
                args: [1, 0, 0],
                pos: None,
            });
        }
        0x3D | 0x3E | 0x3F | 0x40 | 0x10 | 0x2A | 0x26 | 0x35 => {
            chain(s, t, m, p, &[0x8004_8EC4]);
        }
        0x41 => {
            chain(s, t, m, p, &[0x8004_8370, 0x8004_8ACC]);
        }
        0xC => {
            chain(s, t, m, p, &[0x8004_8EC4]);
            let turned = if p.ub(3) == 0xE {
                p.f(pw::SPD) <= 0.0 && s.call(0x8004_E490, t, m, p) != 0
            } else {
                p.f(pw::SPD) < p.f(0xE4) && s.call(0x8004_E490, t, m, p) != 0
            };
            if turned {
                t.set_b(tw::MODE, 1);
                p.set_h(pw::ACTION, 0x1B);
                s.call(0x8004_8490, t, m, p);
                t.set_i(tw::ANG + 4, m.i(0x1C));
            }
        }
        0xF => {
            chain(s, t, m, p, &[0x8004_7A1C, 0x8004_8EC4]);
        }
        0x11 => {
            chain(s, t, m, p, &[0x8004_7F14, 0x8004_8ACC]);
        }
        0x43 => {
            chain(s, t, m, p, &[0x8004_8834]);
        }
        0x37 => {
            if p.h(0x180) & 1 != 0 && p.f(0x1B0) > g.info0_c && p.w(0x1A8) & 2 != 0 {
                let v = p.f(0x1B0) - g.info0_c;
                p.set_f(0x68, if v > ROPE_MAX { ROPE_MAX } else { v });
            } else {
                p.set_f(0x68, 0.0);
            }
            chain(s, t, m, p, &[0x8004_7A1C, 0x8004_8EC4]);
        }
        0x38 => {
            s.call(0x8004_7A1C, t, m, p);
            let a = p.h(0x178);
            if a == 0x8A || a == 0xBD {
                s.call(0x8004_8400, t, m, p);
            } else {
                s.call(0x8004_B168, t, m, p);
            }
            let x = p.f(pw::SPD);
            if x > SWAY_MAX {
                p.set_f(pw::SPD, SWAY_MAX);
            } else if x < SWAY_MIN {
                p.set_f(pw::SPD, SWAY_MIN);
            }
            chain(s, t, m, p, &[]);
            p.set_f(0x44, p.f(pw::SPD));
        }
        0x39 => {
            if t.h(tw::FLAG) & 3 != 0 {
                p.set_h(pw::ACTION, 0x3C);
            }
            chain(s, t, m, p, &[0x8004_7A1C, 0x8004_8EC4]);
        }
        0x3A => {
            s.call(0x800A_E054, t, m, p);
        }
        _ => tail = true,
    }
    let _ = tail;
    s.call(0x8003_D8D0, t, m, p);
    let up = [0.0, p.f(0x140), 0.0];
    let c = rotate_by_angles(stack, t.ang3(tw::ANG), up);
    let pos = t.v3(tw::POS);
    let a = [c[0] + pos[0], c[1] + pos[1], c[2] + pos[2]];
    s.set_ccl_centers(a, a);
    s.call(0x8006_7600, t, m, p);
    s.call(0x800A_C698, t, m, p);
    let a = p.h(pw::ACTION);
    if a != 0x36 && a != 0x55 && t.ub(tw::MODE) != 0x36 {
        eng.call(EngineCall::CclEntry);
    }
    s.call(0x8004_F4D0, t, m, p);
}

const PATH_STOP: f32 = f32::from_bits(0x3dcc_cccd); // r2-0x75A8, 0.1
const PATH_STOP_NEG: f32 = f32::from_bits(0xbdcc_cccd); // r2-0x75A0, -0.1

/// fn_80039484(task work, motion work, player work): the path mode's
/// acceleration: gravity times `weight` in the task frame, air resistance
/// +0x114 on x, and the stick's push (+0xFC times its magnitude, or +0x104
/// without it); a stopped player without the stick stays put while the
/// push is within the stop share +0x128 of the y term.
fn path_accel(
    stack: &mut MatrixStack,
    t: &Work,
    p: &mut Work,
    g: &ExecGlobals,
    s: &mut dyn ExecSteps,
) {
    if t.h(tw::FLAG) & 0x4000 != 0 {
        return;
    }
    let w = p.f(0x138);
    let gv = g.act.gravity;
    let mut a = [0.0f32; 3];
    for j in 0..3 {
        a[j] = fmadds(gv[j], w, a[j]);
    }
    let mut a = crate::ninja_matrix::unrotate_by_angles(stack, t.ang3(tw::ANG), a);
    a[0] = fmadds(p.f(0x114), p.f(pw::SPD), a[0]);
    let stick = s.analog(t, p);
    let push = match stick {
        None => p.f(0x104),
        Some((_, mag)) => p.f(0xFC) * mag,
    };
    a[0] += push;
    if 0.0 == p.f(pw::SPD) {
        let lim = a[1] * p.f(0x128);
        if stick.is_none() && a[0] < lim && a[0] > -lim {
            a[0] = 0.0;
        }
    }
    p.set_v3(0x58, a);
}

/// fn_80038DD4(task work, motion work, player work): the path mode's speed:
/// x takes the acceleration, stopping at zero crossings and below 0.1;
/// y and z are dropped, and the world speed (fn_80046950 clamps it) goes to
/// the motion work.
fn path_speed(
    stack: &mut MatrixStack,
    t: &mut Work,
    m: &mut Work,
    p: &mut Work,
    s: &mut dyn ExecSteps,
) {
    let x0 = p.f(pw::SPD);
    let x = x0 + p.f(0x58);
    p.set_f(pw::SPD, x);
    if x0 > 0.0 {
        if x < 0.0 || (x0 >= PATH_STOP && x < PATH_STOP) {
            p.set_f(pw::SPD, 0.0);
        }
    } else if x0 < 0.0 && (x > -0.0 || (x0 <= PATH_STOP_NEG && x > PATH_STOP_NEG)) {
        p.set_f(pw::SPD, 0.0);
    }
    p.set_f(0x68, 0.0);
    p.set_f(0x6C, 0.0);
    let v = rotate_by_angles(stack, t.ang3(tw::ANG), p.v3(pw::SPD));
    m.set_v3(0, v);
    s.call(0x8004_6950, t, m, p);
}
