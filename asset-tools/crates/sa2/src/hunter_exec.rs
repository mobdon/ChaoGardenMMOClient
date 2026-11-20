//! The hunter exec fn_8009F8A0 (task +0x10 for Knuckles, Rouge, Tikal and
//! Chaos), ported from SA2B's GameCube main.dol (GSNE8P).
//!
//! The work areas stay in their native layout ([`Work`]). Every shared
//! player routine it calls goes through [`HunterWorld::step`] by its GSNE8P
//! address, in the original order. That covers the ground move
//! fn_8004C734, the walker thread's movement steps and the land probes. A
//! runtime binds each address to its Rust port (`player_accel`,
//! `walker_move`, …). Engine-owned work (sound, rumble, effects) is an
//! [`EngineCall`]. Data outside the three work areas is a [`HunterWorld`]
//! accessor. The routine's own logic, constants and branch order are
//! main.dol's; `.sdata2` constants are read from r2 = 0x803B6520 at the
//! addresses noted.
#![allow(clippy::neg_cmp_op_on_partial_ord)] // fcmpo: NaN takes the "not" branch

use crate::{
    hunter::collision_center,
    native_work::Work,
    ninja_matrix::{MatrixStack, Vec3},
    ninja_trig::fmadds,
    ninja_vector::scalor,
    player_turn::inline_sqrtf,
    walker_act::{history_back, se_at, se_call},
    walker_exec::{Engine, EngineCall, PlayerHistory},
};

const ZERO: f32 = 0.0; // 0x803AF85C
const ONE: f32 = 1.0; // 0x803AF860
const TWO: f32 = 2.0; // 0x803AF890
const HOLD_SCALE: f32 = f32::from_bits(0x3c23_d70a); // 0x803AF908, 0.01
const RISE_CAP: f32 = f32::from_bits(0x3ecc_cccd); // 0x803AF9E4, 0.4
const ROUGE_STEP_CD: f32 = f32::from_bits(0x3e99_999a); // 0x803AF9F4, 0.3
const DIG_SPD_71: f32 = 3.125; // 0x803AFA0C
const DIG_SPD_6E: f32 = f32::from_bits(0x3ed5_5555); // 0x803AFA10
const DIG_SPD: f32 = 0.625; // 0x803AFA14
const CHASE_CAP: f32 = f32::from_bits(0x3fbc_3c3c); // 0x803AFA18
const PUSH_RANGE: f32 = f32::from_bits(0x3c23_d70b); // 0x803AFA1C
const ROUGE_STEP: f32 = f32::from_bits(0x3d4c_cccd); // 0x803AFA20, 0.05

/// The three work areas: `t` task work (task +0x34), `m` motion work
/// (+0x38), `p` the 0x420-byte hunter player work (+0x40).
#[derive(Debug, Clone)]
pub struct HunterWork {
    pub t: Work,
    pub m: Work,
    pub p: Work,
}

impl HunterWork {
    pub fn new() -> Self {
        Self {
            t: Work::new(0x30),
            m: Work::new(0x40),
            p: Work::new(0x420),
        }
    }
}

impl Default for HunterWork {
    fn default() -> Self {
        Self::new()
    }
}

/// Pointer slots in the player work that name effect objects.
pub mod slot {
    pub const DIG_FX: usize = 0x3E4;
    pub const TRAIL_A: usize = 0x3E8;
    pub const TRAIL_B: usize = 0x3EC;
}

/// What the exec needs from the rest of the game.
pub trait HunterWorld: Engine {
    /// Run the shared routine at `addr` on (twp, mwp, pwp) (each routine
    /// reads the arguments it takes) and return its r3.
    fn step(&mut self, addr: u32, w: &mut HunterWork) -> i32;
    /// `GetStageNum` (0x8001DD78).
    fn stage(&self) -> i32;
    /// Player 2's swap on entry in stages 0x2C and 0x32: save the gravity
    /// vector 0x801E5624 at 0x801FEAC8, load it from 0x801E5618, save
    /// r13-0x79A8 / r13-0x79AC at r13-0x7834 / r13-0x7830, and load them
    /// from r13-0x79B0 / r13-0x79B4.
    fn p2_enter(&mut self);
    /// The matching restore at exit.
    fn p2_leave(&mut self);
    /// `twp->cwp->info` +8: the first collision sphere's centre.
    fn info_center(&self) -> Vec3;
    fn set_info_center(&mut self, c: Vec3);
    /// The object a pointer slot of the player work names ([`slot`]).
    fn object(&mut self, slot: usize) -> Option<&mut Work>;
    /// fn_80028D94(object in `slot`).
    fn object_update(&mut self, slot: usize);
    /// Byte +2 of the task work of the task at `pwp`+0xA0, when both exist.
    fn a0_kind(&self) -> Option<u8>;
    /// The task work position of the task at `pwp`+0xA8.
    fn a8_pos(&self) -> Option<Vec3>;
    /// A word of main.dol's globals (0x801CC1E4, 0x801CC1E8, the motion
    /// table at 0x801E5D28).
    fn global_w(&self, addr: u32) -> u32;
    /// The frame count at +4 of the motion the table word at `addr` points
    /// to.
    fn motion_frames(&self, addr: u32) -> u32;
    /// The exec's area test over the list at r13-0x78EC: false when the
    /// list is missing or an entry whose mask has `bit` contains `pos`
    /// (fn_800CE154), true otherwise.
    fn areas_clear(&mut self, bit: u32, pos: Vec3) -> bool;
    /// The engine's player history (fn_80038008).
    fn history(&self) -> &PlayerHistory;
    /// Scale (+0xC, +0x10, +0x14) of the motion work of the task at
    /// `pwp`+0x94 (the carried object).
    fn set_held_scale(&mut self, k: f32);
    /// Whether `twp->cwp`+0xA4 names a hit whose task is the carried one.
    fn held_touched(&self) -> bool;
    /// fn_80050008(pnum).
    fn player_check(&mut self, pnum: i8) -> bool;
}

const S_INPUT: u32 = 0x8005_6450;
const S_ACTION: u32 = 0x8009_B8FC;
const S_FIXED: u32 = 0x8005_523C;

fn run(world: &mut dyn HunterWorld, w: &mut HunterWork, steps: &[u32]) {
    for &s in steps {
        world.step(s, w);
    }
}

/// The common tail most actions share: fn_80046DE4, fn_80045880, fn_8003E668.
const MOVE: [u32; 3] = [0x8004_6DE4, 0x8004_5880, 0x8003_E668];

fn moved(world: &mut dyn HunterWorld, w: &mut HunterWork, first: &[u32]) {
    run(world, w, first);
    run(world, w, &MOVE);
}

/// fn_800A26A0(pnum, &pwp[offset]): an effect at a point of the player work.
fn effect_at(world: &mut dyn HunterWorld, w: &HunterWork, offset: usize) {
    world.call(EngineCall::DeferredAt {
        addr: 0x800A_26A0,
        arg: w.p.b(0) as i32,
        points: [w.p.v3(offset), [ZERO; 3]],
    });
}

fn rumble(world: &mut dyn HunterWorld, w: &HunterWork, a: i32, b: i32) {
    world.call(EngineCall::Rumble {
        addr: 0x8002_FB2C,
        args: [w.p.b(0) as i32, a, b, 0],
    });
}

/// The dig effects after cases 0x47..0x4C when `pwp`+0x24 has 0x08001000.
fn dig_effects(world: &mut dyn HunterWorld, w: &HunterWork, pair: bool) {
    if w.p.w(0x24) & 0x0800_1000 == 0 {
        return;
    }
    let knuckles = w.p.ub(1) == 4;
    if pair {
        let (a, b) = if knuckles {
            (0x1e8, 0x1f4)
        } else {
            (0x200, 0x20c)
        };
        effect_at(world, w, a);
        effect_at(world, w, b);
        return;
    }
    let off = match (knuckles, w.t.b(0)) {
        (true, 0x4a | 0x4b | 0x47) => 0x1e8,
        (true, 0x48) => 0x1f4,
        (false, 0x4a | 0x47) => 0x20c,
        (false, 0x4b | 0x48) => 0x200,
        // The original passes a stale r31 here; this action set is not
        // reachable from these cases unless a callee changed the action.
        _ => return,
    };
    effect_at(world, w, off);
}

/// Sound by voice row (+3): Tikal (0xF), Knuckles (4), Rouge (5).
fn row_sound(w: &HunterWork, tikal: i32, knuckles: i32, rouge: i32) -> Option<i32> {
    match w.p.b(3) {
        0xf => Some(tikal),
        4 => Some(knuckles),
        5 => Some(rouge),
        _ => None,
    }
}

/// fn_8009F8A0: one frame of a hunter.
pub fn hunter_exec(stack: &mut MatrixStack, w: &mut HunterWork, world: &mut dyn HunterWorld) {
    let swap = w.p.b(0) != 0 && matches!(world.stage(), 0x2c | 0x32);
    if swap {
        world.p2_enter();
    }
    world.step(S_INPUT, w);
    let action = w.t.b(0);
    if action == 0x39 || action == 0x37 {
        if w.t.ub(1) == 0x24 {
            w.t.clear_h(4, 0x3c);
            w.p.set_h(0x10, w.p.h(0x10) & 0x3ffc);
            world.step(S_FIXED, w);
        }
    } else {
        world.step(S_ACTION, w);
    }
    if w.t.h(4) & 0x100 != 0 {
        w.p.set_h(0x3a2, w.p.h(0x3a2).wrapping_add(1));
    }
    action_body(w, world);
    tail(stack, w, world);
    if swap {
        world.p2_leave();
    }
}

/// The 0x58-entry jump table at 0x801C2AD4.
fn action_body(w: &mut HunterWork, world: &mut dyn HunterWorld) {
    let spd_x = |w: &HunterWork| w.p.f(0x64);
    match w.t.b(0) as i32 as u32 {
        0 => {
            run(world, w, &[0x8004_8490, 0x8004_C734]);
            run(world, w, &MOVE);
            if w.t.h(4) & 1 != 0 && world.a0_kind() == Some(0x19) {
                w.p.set_h(0x176, 0xd3);
            } else {
                world.step(0x8005_4D04, w);
            }
        }
        1 => {
            if w.p.h(0x17a) == 0x1b && w.p.h(0x178) != 0x1b {
                w.p.set_f(0x64, spd_x(w) + w.p.f(0xe4));
            }
            run(world, w, &[0x8004_C734, 0x8004_6DE4]);
            if world.step(0x8004_5880, w) == 2 {
                w.t.set_b(0, 0x10);
                w.p.set_h(0x176, 0x18);
                if let Some(id) = row_sound(w, 0x3028, 0x300b, 0x301c) {
                    world.call(se_at(id, w.t.v3(0x14)));
                }
                rumble(world, w, 3, 7);
                world.call(EngineCall::Deferred {
                    addr: 0x800B_CF50,
                    arg: w.p.b(0) as i32,
                });
                w.p.set_f(0x64, -w.p.f(0x64));
                w.p.set_f(0x6c, -w.p.f(0x6c));
            } else {
                run(world, w, &[0x8003_E668, 0x8005_4A08]);
            }
        }
        2 => run(
            world,
            w,
            &[0x8003_9484, 0x8003_8DD4, 0x8003_8A70, 0x8005_4A08],
        ),
        5 => run(world, w, &[0x8004_8834, 0x8003_8DD4, 0x8003_8A70]),
        6 | 0x1c | 0x1d => {
            world.step(0x8004_7A1C, w);
            if world.step(0x8003_6CE4, w) == 0 {
                world.step(0x8004_B330, w);
            }
            run(world, w, &MOVE);
        }
        7 | 0x1e => moved(world, w, &[0x8004_B330]),
        8 | 0x1f => {
            let first = if w.p.f(0x68) >= w.p.f(0xdc) {
                0x8004_8ACC
            } else {
                0x8004_B330
            };
            moved(world, w, &[first]);
        }
        0xa => {
            world.step(0x8004_7A1C, w);
            if w.p.h(0x176) == 0x4a {
                world.step(0x8004_8724, w);
            } else if world.step(0x8003_6CE4, w) == 0 {
                world.step(0x8004_B330, w);
            }
            run(world, w, &MOVE);
        }
        0xc => {
            moved(world, w, &[0x8004_8EC4]);
            if spd_x(w) >= ZERO && world.step(0x8004_E490, w) != 0 {
                w.t.set_b(0, 1);
                w.p.set_h(0x176, 0x1b);
                world.step(0x8004_8490, w);
                w.t.set_w(0xc, w.m.w(0x1c));
            }
        }
        0x3b => {
            world.step(0x8004_7A1C, w);
            if w.p.f(0x68) > ZERO {
                run(world, w, &[0x8004_B330, 0x8004_6DE4]);
            } else {
                run(world, w, &[0x8004_A848, 0x8004_63C4]);
            }
            match world.step(0x8009_36AC, w) {
                2 => {
                    world.step(0x8009_0C80, w);
                    rumble(world, w, 3, 0xf);
                }
                1 => {
                    world.step(0x8003_E668, w);
                    rumble(world, w, 3, 0xf);
                    w.p.0[0x35c] = w.p.0[0x35c].wrapping_add(0xc);
                }
                3 => {
                    w.t.clear_h(4, 0x500);
                    w.t.set_b(0, 0xc);
                    w.p.set_h(0x176, 0x6b);
                }
                _ => run(world, w, &[0x8004_5880, 0x8003_E668]),
            }
        }
        0x3c => {
            world.step(0x8004_7A1C, w);
            if w.p.h(0x3ac) == 0x1e {
                w.t.set_i(0xc, w.t.i(0xc).wrapping_add(0x400));
            }
            let first = if w.m.f(4) > ZERO {
                0x8004_6DE4
            } else {
                0x8004_63C4
            };
            run(world, w, &[first, 0x8004_5880, 0x8003_E668]);
        }
        0xe => {
            if w.p.h(0x176) == 0x13 {
                world.step(0x8004_8EC4, w);
            } else {
                world.step(0x8004_AA1C, w);
                if world.step(0x8004_F11C, w) & 0x400 != 0 {
                    world.step(0x8006_A148, w);
                }
                if w.p.h(0x17a) == 0x13 {
                    match w.p.b(3) {
                        4 => world.call(se_at(0x3006, w.t.v3(0x14))),
                        5 => world.call(se_at(0x3017, w.t.v3(0x14))),
                        _ => {}
                    }
                }
            }
            run(world, w, &[0x8004_6600, 0x8004_5880, 0x8003_E668]);
        }
        0xf => moved(world, w, &[0x8004_7A1C, 0x8004_C734]),
        0x10 => {
            if w.p.h(0x176) == 0x7c {
                w.p.set_h(0x17c, 0);
            }
            moved(world, w, &[0x8004_8EC4]);
        }
        0x11 => moved(world, w, &[0x8004_7F14, 0x8004_8ACC]),
        0x4e => {
            world.call(EngineCall::Sound {
                addr: 0x8006_9F6C,
                id: 0x2016,
                args: [0, 0, 0x1e],
                pos: Some(w.t.v3(0x14)),
            });
            moved(world, w, &[0x8004_7A1C, 0x8004_B330]);
            world.step(0x8009_B5CC, w);
        }
        0x45 => {
            if !matches!(w.p.h(0x178), 0x6a | 0x68) {
                run(world, w, &[0x8004_A4F0, 0x8004_6510, 0x8003_F760]);
            }
            world.step(0x8003_E668, w);
        }
        0x46 => run(world, w, &[0x8008_BEDC, 0x8003_E668]),
        0x47 | 0x48 | 0x4a | 0x4b => {
            run(world, w, &[0x8004_D7FC, 0x8004_8724]);
            let spd = match w.p.h(0x176) {
                0x71 | 0x70 => DIG_SPD_71,
                0x6e => DIG_SPD_6E,
                _ => DIG_SPD,
            };
            w.p.set_f(0x64, spd);
            world.step(0x8004_6DE4, w);
            if world.step(0x8004_5880, w) != 0 {
                let (pos, _) = history_back(world.history(), w.p.b(0), 1);
                w.t.set_v3(0x14, pos);
                world.step(0x8004_691C, w);
            }
            world.step(0x8003_E668, w);
            dig_effects(world, w, false);
        }
        0x49 => {
            world.step(0x8004_D7D8, w);
            match world.a8_pos() {
                None => world.step(0x8004_B330, w),
                Some(o) => {
                    let dz = w.t.f(0x1c) - o[2];
                    let dx = w.t.f(0x14) - o[0];
                    let mut d = fmadds(dx, dx, dz * dz);
                    if d > ZERO {
                        d = inline_sqrtf(d);
                    }
                    w.p.set_f(0x64, if d > CHASE_CAP { CHASE_CAP } else { d });
                    world.step(0x8004_8724, w)
                }
            };
            run(world, w, &MOVE);
            dig_effects(world, w, true);
        }
        0x4c => {
            moved(world, w, &[0x8004_D7FC, 0x8004_8724]);
            dig_effects(world, w, true);
        }
        0x4d => {
            moved(world, w, &[0x8004_8724]);
            if world.global_w(0x801C_C1E4) & 3 == 0 {
                world.step(0x8009_B464, w);
            }
        }
        0x24 | 0x25 => run(world, w, &[0x8003_8138, 0x8009_6CA8]),
        0x4f => {
            moved(
                world,
                w,
                &[0x8009_14F8, 0x8004_7A1C, 0x8004_8490, 0x8004_B330],
            );
            if w.p.f(0x1b0) < w.p.f(0x228) {
                if w.p.h(0x178) == 0x79 && w.p.h(0x176) != 0x7a {
                    if let Some(id) = row_sound(w, 0x3026, 0x3009, 0x301a) {
                        world.call(se_call(id));
                    }
                    rumble(world, w, 6, 0xf);
                }
                w.p.set_h(0x176, 0x7a);
            } else if !matches!(w.p.h(0x176), 0x78 | 0x79) {
                w.p.set_h(0x176, 0x78);
            }
        }
        0x50 => {
            moved(world, w, &[0x8009_14F8, 0x8004_7A1C, 0x8004_B330]);
            if w.p.f(0x1b0) < TWO + w.p.f(0x228) {
                if w.p.h(0x178) == 0x75 && world.global_w(0x801C_C1E4) & 0xf == 0 {
                    world.call(timer_sound(w, 0x7004, 0x28, 8));
                }
            } else {
                world.call(timer_sound(w, 0x7007, 0x7f, 4));
            }
        }
        0x51 => {
            moved(world, w, &[0x8004_7A1C, 0x8004_B330]);
            if w.p.f(0x1b0) > w.p.f(0x228) {
                world.call(timer_sound(w, 0x7007, 0x7f, 4));
            }
        }
        0x13 => moved(world, w, &[0x8004_8490, 0x8004_C734]),
        0x29 | 0x14 | 0x2b | 0x31 => moved(world, w, &[0x8004_8ACC]),
        0x15 => {
            moved(world, w, &[0x8004_8490, 0x8004_C734]);
            world.step(0x8005_4B8C, w);
        }
        0x16 | 0x17 => run(
            world,
            w,
            &[
                0x8004_EA64,
                0x8004_AA1C,
                0x8004_6600,
                0x8004_5880,
                0x8003_E668,
            ],
        ),
        0x1a => moved(world, w, &[0x8004_D820, 0x8004_8ACC]),
        0x19 => {
            if w.p.h(0x17a) == 0x22 && w.p.h(0x178) != 0x22 {
                w.p.set_f(0x64, spd_x(w) + w.p.f(0xe4));
            }
            moved(world, w, &[0x8004_C734]);
            world.step(0x8005_4A08, w);
        }
        0x1b | 0x2a | 0x26 | 0x35 => moved(world, w, &[0x8004_8EC4]),
        0x20 => run(
            world,
            w,
            &[0x8004_AA1C, 0x8004_68FC, 0x8004_5880, 0x8003_E668],
        ),
        0x21 => moved(world, w, &[0x8004_7A1C, 0x8004_D820, 0x8004_8ACC]),
        0x22 => {
            world.step(0x8004_ED24, w);
        }
        0x23 => {
            world.set_held_scale(HOLD_SCALE * w.p.f(0x184));
            moved(world, w, &[0x8004_8ACC]);
        }
        0x2c => moved(world, w, &[0x8004_D6D0, 0x8004_8ACC]),
        0x34 => {
            world.step(0x8004_7A1C, w);
            let first = if w.p.h(0x14) > 0 {
                0x8004_8724
            } else {
                0x8004_B330
            };
            moved(world, w, &[first]);
        }
        0x37 => {
            let cy = world.info_center()[1];
            let h = w.p.f(0x1b0);
            if w.p.h(0x180) & 1 != 0 && h > cy && w.p.w(0x1a8) & 2 != 0 {
                let v = h - cy;
                w.p.set_f(0x68, if v > RISE_CAP { RISE_CAP } else { v });
            } else {
                w.p.set_f(0x68, ZERO);
            }
            moved(world, w, &[0x8004_7A1C, 0x8004_8EC4]);
        }
        0x39 => {
            if w.t.h(4) & 3 != 0 {
                w.p.set_h(0x176, 0x3b);
            }
            moved(world, w, &[0x8004_7A1C, 0x8004_8EC4]);
        }
        _ => {}
    }
}

/// SE_CallV2_Timer (0x8006AFFC)(id, twp, 1, volume, timer, &twp.pos).
fn timer_sound(w: &HunterWork, id: i32, volume: i32, timer: i32) -> EngineCall {
    EngineCall::Sound {
        addr: 0x8006_AFFC,
        id,
        args: [1, volume, timer],
        pos: Some(w.t.v3(0x14)),
    }
}

/// From 0x800A1318 to the end.
fn tail(stack: &mut MatrixStack, w: &mut HunterWork, world: &mut dyn HunterWorld) {
    let action = w.t.b(0);
    if action != 0x3b && action != 0x3c && world.global_w(0x801C_C1E8) == 5 {
        push_out(w, world);
    }
    world.step(0x8003_D8D0, w);
    let center = collision_center(
        stack,
        w.t.ub(0),
        w.t.ang3(8),
        w.t.v3(0x14),
        w.p.f(0x140),
        w.p.f(0x3b4),
    );
    world.set_info_center(center);
    let row = w.p.b(3);
    if w.p.w(slot::TRAIL_A) != 0 && row == 5 {
        let d = sub(w.p.v3(0x230), w.p.v3(0x3cc));
        let len = scalor(d);
        w.p.set_v3(0x3cc, w.p.v3(0x230));
        for s in [slot::TRAIL_B, slot::TRAIL_A] {
            if w.p.w(s) == 0 {
                continue;
            }
            if let Some(o) = world.object(s) {
                if len.abs() > o.f(4).abs() {
                    o.set_f(4, len);
                }
            }
            world.object_update(s);
        }
    }
    if w.p.w(slot::DIG_FX) != 0 && (w.p.ub(1) == 4 || row == 5) {
        let len = scalor(sub(w.p.v3(0x218), w.p.v3(0x3d8)));
        w.p.set_v3(0x3d8, w.p.v3(0x218));
        let knuckles = w.p.ub(1) == 4;
        let dig = knuckles && w.t.ub(0) == 0x3b;
        if let Some(o) = world.object(slot::DIG_FX) {
            if dig {
                o.set_b(0, 0x10);
            } else {
                if knuckles {
                    o.set_b(0, 0xf);
                }
                if len.abs() > o.f(4).abs() {
                    o.set_f(4, len);
                }
                if len.abs() > RISE_CAP {
                    o.set_w(0xc, 0x4000);
                }
            }
        }
        world.object_update(slot::DIG_FX);
    }
    if row == 5 {
        rouge_motion(w, world);
    }
    if row == 0x10 {
        let n = w.p.h(0x3ae).wrapping_add(1);
        w.p.set_h(0x3ae, if n >= 0xe { 0 } else { n });
    }
    if world.player_check(w.p.b(0)) && w.t.ub(0) != 0x3a {
        w.t.set_b(0, 0x3a);
        w.t.clear_h(4, 0x3100);
    }
    if w.p.w(0x94) != 0 && world.step(0x8004_F11C, w) & 0x200 == 0 && !world.held_touched() {
        world.step(0x8005_42A0, w);
        if w.p.w(0x94) == 0 {
            if w.t.h(4) & 3 != 0 {
                world.step(S_FIXED, w);
            } else {
                w.t.set_b(0, 0xa);
                w.p.set_h(0x176, 0xf);
            }
        }
    }
    run(world, w, &[0x8006_7600, 0x8009_2C00]);
    let req = w.p.h(0x176);
    if req != 0x36 && req != 0x55 && w.t.ub(0) != 0x36 {
        world.step(0x8000_F4EC, w); // CCL_Entry(task)
    }
    world.step(0x8004_F4D0, w);
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// 0x800A1330..0x800A14E4: walk back along the player history to the
/// first point farther than PUSH_RANGE (squared, in X/Z), move there, and
/// keep stepping by that offset (up to 0x80 times) while the area test
/// stays clear.
fn push_out(w: &mut HunterWork, world: &mut dyn HunterWorld) {
    let bit = 1u32.wrapping_shl(w.p.b(1) as u32);
    if !world.areas_clear(bit, w.t.v3(0x14)) {
        return;
    }
    let (mut dx, mut dz) = (ZERO, ZERO);
    for back in 1u8..0x80 {
        let (h, _) = history_back(world.history(), w.p.b(0), back);
        dz = h[2] - w.t.f(0x1c);
        dx = h[0] - w.t.f(0x14);
        if fmadds(dx, dx, dz * dz) > PUSH_RANGE {
            break;
        }
    }
    let mut k = 0x80u32;
    loop {
        w.t.set_f(0x14, w.t.f(0x14) + dx);
        w.t.set_f(0x1c, w.t.f(0x1c) + dz);
        if k & 0xff == 0 || !world.areas_clear(bit, w.t.v3(0x14)) {
            return;
        }
        k -= 1;
    }
}

/// 0x800A1714..0x800A18E8 (Rouge only): her motion blend state at
/// `pwp`+0x3AE (2 counting in, 3 playing), +0x3B0 (motion) and +0x3B8 (frame).
fn rouge_motion(w: &mut HunterWork, world: &mut dyn HunterWorld) {
    let base = 0x801E_5D28u32;
    let m = w.p.h(0x178);
    let motion: u32 = if matches!(m, 0xf..=0x11 | 0x43 | 0x44 | 0x69 | 0x6b | 0xb3 | 0xb4) {
        0xcd
    } else if w.p.ub(2) == 1 {
        0xd1
    } else {
        0xcf
    };
    match w.p.h(0x3ae) {
        3 => {
            let cur = w.p.w(0x3b0);
            if cur != motion {
                w.p.set_h(0x3ae, 2);
                let word = |i: u32, k: u32| world.global_w(base + 4 + k + 8 * i);
                let (a, b, c, d) = (word(cur, 0), word(motion, 0), word(cur, 8), word(motion, 8));
                w.p.set_w(0x400, a);
                w.p.set_f(0x408, w.p.f(0x3b8));
                w.p.set_w(0x404, b);
                w.p.set_f(0x40c, ZERO);
                w.p.set_w(0x410, c);
                w.p.set_f(0x418, w.p.f(0x3b8));
                w.p.set_w(0x414, d);
                w.p.set_f(0x41c, ZERO);
                w.p.set_f(0x3b8, ZERO);
                w.p.set_w(0x3b0, motion);
            } else {
                let step = if motion == 0xcd {
                    ROUGE_STEP_CD
                } else {
                    ROUGE_STEP
                };
                w.p.set_f(0x3b8, w.p.f(0x3b8) + step);
                let frames = world.motion_frames(base + 4 + 8 * motion) as f32;
                // Never-crash: the original would spin forever on a zero-frame
                // motion; real motions have at least one frame.
                while frames > ZERO && w.p.f(0x3b8) >= frames {
                    w.p.set_f(0x3b8, w.p.f(0x3b8) - frames);
                }
            }
        }
        2 => {
            w.p.set_f(0x3b8, w.p.f(0x3b8) + ROUGE_STEP);
            if w.p.f(0x3b8) >= ONE {
                w.p.set_h(0x3ae, 3);
                w.p.set_f(0x3b8, ZERO);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Fake {
        steps: Vec<u32>,
        engine: Vec<EngineCall>,
        returns: Vec<(u32, i32)>,
        stage: i32,
        p2: Vec<bool>,
        clear: bool,
        hist: Option<PlayerHistory>,
        objs: [Work; 3],
    }

    impl Engine for Fake {
        fn call(&mut self, c: EngineCall) {
            self.engine.push(c);
        }
        fn rand(&mut self) -> i32 {
            0
        }
    }

    impl HunterWorld for Fake {
        fn step(&mut self, addr: u32, _: &mut HunterWork) -> i32 {
            self.steps.push(addr);
            self.returns.iter().find(|r| r.0 == addr).map_or(0, |r| r.1)
        }
        fn stage(&self) -> i32 {
            self.stage
        }
        fn p2_enter(&mut self) {
            self.p2.push(true);
        }
        fn p2_leave(&mut self) {
            self.p2.push(false);
        }
        fn info_center(&self) -> Vec3 {
            [0.0; 3]
        }
        fn set_info_center(&mut self, _: Vec3) {}
        fn object(&mut self, s: usize) -> Option<&mut Work> {
            Some(&mut self.objs[(s - 0x3e4) / 4])
        }
        fn object_update(&mut self, _: usize) {}
        fn a0_kind(&self) -> Option<u8> {
            None
        }
        fn a8_pos(&self) -> Option<Vec3> {
            None
        }
        fn global_w(&self, addr: u32) -> u32 {
            addr
        }
        fn motion_frames(&self, _: u32) -> u32 {
            10
        }
        fn areas_clear(&mut self, _: u32, _: Vec3) -> bool {
            self.clear
        }
        fn history(&self) -> &PlayerHistory {
            self.hist.as_ref().unwrap()
        }
        fn set_held_scale(&mut self, _: f32) {}
        fn held_touched(&self) -> bool {
            false
        }
        fn player_check(&mut self, _: i8) -> bool {
            false
        }
    }

    fn frame(w: &mut HunterWork, f: &mut Fake) {
        hunter_exec(&mut MatrixStack::new(), w, f);
    }

    #[test]
    fn ground_action_runs_the_move_run_in_order() {
        let mut w = HunterWork::new();
        let mut f = Fake::default();
        frame(&mut w, &mut f);
        assert_eq!(
            &f.steps[..9],
            &[
                0x8005_6450,
                0x8009_B8FC,
                0x8004_8490,
                0x8004_C734,
                0x8004_6DE4,
                0x8004_5880,
                0x8003_E668,
                0x8005_4D04,
                0x8003_D8D0
            ]
        );
        assert_eq!(
            &f.steps[9..],
            &[0x8006_7600, 0x8009_2C00, 0x8000_F4EC, 0x8004_F4D0]
        );
    }

    #[test]
    fn climb_results_pick_their_paths() {
        for (r, want_action, want_req) in [(3, 0xc, 0x6b), (0, 0x3b, 0)] {
            let mut w = HunterWork::new();
            w.t.set_b(0, 0x3b);
            w.t.set_h(4, 0x0503);
            let mut f = Fake {
                returns: vec![(0x8009_36AC, r)],
                ..Fake::default()
            };
            frame(&mut w, &mut f);
            assert_eq!(w.t.b(0), want_action);
            assert_eq!(w.p.h(0x176), want_req);
            if r == 3 {
                assert_eq!(w.t.h(4), 3);
            }
        }
    }

    #[test]
    fn player_two_swaps_globals_only_in_two_stages() {
        for (stage, swaps) in [(0x2c, true), (0x32, true), (0x0d, false)] {
            let mut w = HunterWork::new();
            w.p.set_b(0, 1);
            let mut f = Fake {
                stage,
                ..Fake::default()
            };
            frame(&mut w, &mut f);
            assert_eq!(f.p2, if swaps { vec![true, false] } else { vec![] });
        }
    }

    #[test]
    fn rouge_switches_motion_and_counts_in() {
        let mut w = HunterWork::new();
        w.p.set_b(3, 5);
        w.p.set_h(0x3ae, 3);
        w.p.set_w(0x3b0, 0xcf);
        w.p.set_h(0x178, 0x69);
        w.p.set_f(0x3b8, 4.0);
        let mut f = Fake::default();
        frame(&mut w, &mut f);
        assert_eq!(w.p.h(0x3ae), 2);
        assert_eq!(w.p.w(0x3b0), 0xcd);
        assert_eq!(w.p.f(0x408), 4.0);
        assert_eq!(w.p.w(0x400), 0x801E_5D2C + 8 * 0xcf);
        assert_eq!(w.p.w(0x414), 0x801E_5D34 + 8 * 0xcd);
        assert_eq!(w.p.f(0x3b8), 0.0);
    }

    #[test]
    fn push_out_stops_after_0x81_steps() {
        let mut w = HunterWork::new();
        w.p.set_b(1, 4);
        let mut hist = PlayerHistory {
            index: [0; 8],
            pos: vec![[0.0; 3]; 0x800],
            ang: vec![[0; 3]; 0x800],
        };
        hist.pos[0xff] = [1.0, 0.0, 0.0]; // one frame back
        let mut f = Fake {
            clear: true,
            hist: Some(hist),
            ..Fake::default()
        };
        // Pretend the stage global is 5 by routing global_w through the
        // address: only 0x801CC1E8 matters here.
        struct Five(Fake);
        impl Engine for Five {
            fn call(&mut self, c: EngineCall) {
                self.0.call(c)
            }
            fn rand(&mut self) -> i32 {
                0
            }
        }
        impl HunterWorld for Five {
            fn step(&mut self, a: u32, w: &mut HunterWork) -> i32 {
                self.0.step(a, w)
            }
            fn stage(&self) -> i32 {
                0
            }
            fn p2_enter(&mut self) {}
            fn p2_leave(&mut self) {}
            fn info_center(&self) -> Vec3 {
                [0.0; 3]
            }
            fn set_info_center(&mut self, _: Vec3) {}
            fn object(&mut self, s: usize) -> Option<&mut Work> {
                self.0.object(s)
            }
            fn object_update(&mut self, _: usize) {}
            fn a0_kind(&self) -> Option<u8> {
                None
            }
            fn a8_pos(&self) -> Option<Vec3> {
                None
            }
            fn global_w(&self, addr: u32) -> u32 {
                if addr == 0x801C_C1E8 {
                    5
                } else {
                    0
                }
            }
            fn motion_frames(&self, _: u32) -> u32 {
                1
            }
            fn areas_clear(&mut self, b: u32, p: Vec3) -> bool {
                assert_eq!(b, 1 << 4);
                self.0.areas_clear(b, p)
            }
            fn history(&self) -> &PlayerHistory {
                self.0.history()
            }
            fn set_held_scale(&mut self, _: f32) {}
            fn held_touched(&self) -> bool {
                false
            }
            fn player_check(&mut self, _: i8) -> bool {
                false
            }
        }
        let mut five = Five(std::mem::take(&mut f));
        hunter_exec(&mut MatrixStack::new(), &mut w, &mut five);
        assert_eq!(w.t.f(0x14), 129.0);
    }
}
