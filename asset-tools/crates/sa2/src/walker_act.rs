//! The walker's action step fn_800B24F4 and its helpers (GSNE8P main.dol),
//! on native-layout work areas ([`Work`]), checked against main.dol by
//! tests/walker_act.rs.
// Negated comparisons keep the compiled branches' NaN behaviour.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::{
    land_nodes::nj_scalor,
    native_work::{pw, tw, Work},
    ninja_matrix::{rotate_by_angles, MatrixStack, Vec3},
    ninja_trig::fmadds,
    ninja_vector::inner_product,
    walker_exec::{approach, Engine, EngineCall, PlayerHistory},
    walker_move::{slope_aim, turn_back, SteerCalls},
};

/// Engine state the action step reads.
#[derive(Debug, Clone, Default)]
pub struct ActGlobals {
    /// The game state block at 0x801CC168 (0x100 bytes).
    pub gs: Work,
    /// GetStageNum.
    pub stage: i32,
    /// 0x801CC168 +0x49.
    pub gs_b49: i8,
    /// Per-player button bytes at r13-0x7A38 (pressed) and r13-0x7A28
    /// (held).
    pub pressed: [i8; 8],
    pub held: [i8; 8],
    /// The action button bytes at r13-0x7A3C and r13-0x7A4C.
    pub action: [i8; 8],
    pub button_4c: [i8; 8],
    /// Each player's pad word +0x10 (0x801E5438 + 0x34 * pnum).
    pub pad_10: [u32; 8],
    /// The collision query's +0x20 (player +0x90).
    pub query_20: f32,
    /// The frame count of the character motion table's entry 0x82.
    pub motion_82_frames: u32,
    /// Whether the character motion table's entry 0x56 is loaded
    /// (0x801E5D28 + 0x2B4).
    pub motion_56: bool,
    /// The gravity vector (0x801E5624).
    pub gravity: Vec3,
}

/// The routines the action step hands over to.
pub trait ActSteps {
    /// fn_80054D04(task work, motion work, player work).
    fn stand(&mut self, t: &mut Work, m: &mut Work, p: &mut Work);
    /// fn_8004E560(task work, player work, NULL, NULL): whether the stick
    /// is held.
    fn analog(&mut self, t: &Work, p: &Work) -> bool;
    /// fn_8004F11C(task work, player work): the flags of what the player
    /// holds or hangs from (its +0x94/+0x9C objects).
    fn hold_flags(&mut self, t: &Work, p: &Work) -> u32;
    /// fn_800542A0(task work, motion work, player work): let go of the
    /// held object.
    fn release_hold(&mut self, t: &mut Work, m: &mut Work, p: &mut Work);
    /// The action-button handlers not ported here (fn_80053490,
    /// fn_800530B0, fn_80053570, fn_80053520, fn_800534F4, fn_800535C0,
    /// fn_800AC850, fn_800AD208, fn_800ACF38, fn_800ACB9C), by address.
    fn button_action(&mut self, addr: u32, t: &mut Work, m: &mut Work, p: &mut Work);
    /// fn_80055734(task work, player work): the damage step. Returns
    /// whether the player was hurt.
    fn damage(&mut self, t: &mut Work, p: &mut Work) -> bool;
    /// fn_800060EC(task, kind) found a contact (for fn_8004E490).
    fn touching(&mut self, kind: i32) -> bool;
    /// The acceleration of the ride at player +0xA0 (its task's +0x3C
    /// block, +0x10), when there is one.
    fn ride_acc(&mut self, p: &Work) -> Option<Vec3>;
}

/// fn_8004691C(motion work, player work): zero both speeds.
pub fn zero_speeds(m: &mut Work, p: &mut Work) {
    p.set_v3(pw::SPD, [0.0; 3]);
    m.set_v3(0, [0.0; 3]);
}

/// fn_8005523C(task work, motion work, player work): back to standing,
/// unless in mode 0x36 with the game's per-player bit in +0x49 set.
pub fn go_stand(t: &mut Work, m: &mut Work, p: &mut Work, g: &ActGlobals, s: &mut dyn ActSteps) {
    if t.ub(tw::MODE) == 0x36
        && g.gs_b49 as i32 & 1i32.checked_shl(p.b(pw::PNUM) as u32).unwrap_or(0) != 0
    {
        return;
    }
    t.set_b(tw::MODE, 0);
    p.set_h(0x18, 0);
    s.stand(t, m, p);
}

/// fn_8004E454(task work, player work): whether the player's button is
/// pressed this frame (r13-0x7A38), never with task flag 0x4000.
pub fn button_pressed(t: &Work, p: &Work, g: &ActGlobals) -> bool {
    t.h(tw::FLAG) & 0x4000 == 0 && g.pressed[p.b(pw::PNUM) as usize] != 0
}

/// fn_8004E400(task work, player work): while the button is held
/// (r13-0x7A28) the hold count +0x12 counts up; true while it was below
/// +0xC0.
pub fn button_held(t: &Work, p: &mut Work, g: &ActGlobals) -> bool {
    if t.h(tw::FLAG) & 0x4000 != 0 || g.held[p.b(pw::PNUM) as usize] == 0 {
        return false;
    }
    let n = p.h(0x12);
    p.set_h(0x12, n.wrapping_add(1));
    (n as i32) < p.i(0xC0)
}

/// Push a request onto the player's 8-entry ring at +4 (count at +0xC).
pub fn push_request(p: &mut Work, v: u8) {
    let n = p.ub(0xC);
    p.0[0xC] = n.wrapping_add(1);
    p.0[4 + (n & 7) as usize] = v;
}

/// fn_80053784(task work, player work): in mode 0, queue requests 3 (when
/// none is queued) and 0x2A.
pub fn queue_stand_requests(t: &Work, p: &mut Work) -> bool {
    if t.b(tw::MODE) != 0 {
        return false;
    }
    if p.b(0xC) == 0 {
        push_request(p, 3);
    }
    push_request(p, 0x2A);
    true
}

const F48_HIGH: f32 = 12.0; // r2-0x68DC
const F48_BASE: f32 = 10.0; // r2-0x69B4
const F48_BOOST: f32 = 2.0; // r2-0x699C

/// fn_800B5630(player work): the walker's +0x48 by stage and character.
pub fn reset_f48(p: &mut Work, g: &ActGlobals) {
    let pnum = p.ub(pw::PNUM);
    if g.stage == 0x1D && pnum == 1 {
        p.set_f(0x48, F48_HIGH);
    } else if g.stage == 0x14 && pnum == 1 {
        p.set_f(0x48, F48_BASE);
    } else {
        match p.b(pw::CHAR) {
            7 => {
                p.set_f(0x48, F48_BASE);
                if p.w(0x24) & 0x80_0000 != 0 {
                    p.set_f(0x48, p.f(0x48) + F48_BOOST);
                }
            }
            6 => p.set_f(0x48, F48_BASE),
            _ => {}
        }
    }
}

const ZERO: f32 = 0.0; // r2-0x69E8

fn set_mode(t: &mut Work, p: &mut Work, mode: i8, action: i16) {
    t.set_b(tw::MODE, mode);
    p.set_h(pw::ACTION, action);
}

/// fn_800AD910(task work, motion work, player work): act on the command
/// another object left in task byte +1 with task flag 0x1000 (springs,
/// launchers, hits, cut scenes). Returns whether the mode was taken over.
pub fn walker_command(
    t: &mut Work,
    m: &mut Work,
    p: &mut Work,
    g: &ActGlobals,
    s: &mut dyn ActSteps,
    eng: &mut dyn Engine,
) -> bool {
    if t.h(tw::FLAG) & 0x1000 == 0 {
        return false;
    }
    let w28 = p.i(0x28);
    match t.ub(tw::SMODE) {
        4 | 0x10 => set_mode(t, p, 7, 0xD),
        6 => set_mode(t, p, 8, 0xD),
        5 => set_mode(t, p, 0xA, 0x4A),
        8 => set_mode(t, p, 0xA, 0x17),
        1 => {
            set_mode(t, p, 2, 8);
            p.set_f(0x30, ZERO);
            t.clear_h(tw::FLAG, 0x500);
            t.or_h(tw::FLAG, 0x2000);
            return true;
        }
        9 => {
            t.set_b(tw::MODE, 0x12);
            if w28 == 0x56 && !g.motion_56 {
                p.set_i(0x28, 0);
            }
            p.set_h(pw::ACTION, p.i(0x28) as i16);
            zero_speeds(m, p);
        }
        0x14 => {
            t.set_b(tw::MODE, 0x33);
            p.set_h(pw::ACTION, w28 as i16);
            zero_speeds(m, p);
        }
        0xF => go_stand(t, m, p, g, s),
        0x11 => {
            if t.b(tw::MODE) == 1 || p.f(pw::SPD) > p.f(0xE8) {
                p.set_f(0x30, ZERO);
                t.clear_h(tw::FLAG, 0x500);
                t.or_h(tw::FLAG, 0x2000);
            }
            return false;
        }
        0x18 => {
            set_mode(t, p, 0x10, 0x18);
            eng.call(EngineCall::Deferred {
                addr: 0x800B_CF50,
                arg: p.b(pw::PNUM) as i32,
            });
        }
        0x21 => {
            t.set_b(tw::MODE, 0x35);
            p.set_h(pw::ACTION, w28 as i16);
            if w28 == 0x36 || (0x55..0x57).contains(&w28) {
                p.set_w(0x424, 0);
            }
        }
        0x19 => {
            set_mode(t, p, 0x35, 0x38);
            p.set_h(0x16, 0);
        }
        0x1B => set_mode(t, p, 0x37, 0x39),
        0x1C => {
            set_mode(t, p, 0x38, 6);
            zero_speeds(m, p);
        }
        0x24 => {
            t.clear_h(tw::FLAG, 0x3C);
            p.set_h(pw::STATUS, p.h(pw::STATUS) & 0x3FFC);
            reset_f48(p, g);
            go_stand(t, m, p, g, s);
            t.clear_h(tw::FLAG, 0x2500);
            p.set_w(0x424, 0);
            return true;
        }
        0x25 => {
            set_mode(t, p, 0x39, 0x3B);
            t.clear_h(tw::FLAG, 0x2500);
            match p.b(3) {
                7 => eng.call(EngineCall::PlayVoice { id: 0x68C }),
                6 => eng.call(EngineCall::PlayVoice { id: 0x68D }),
                _ => {}
            }
            return true;
        }
        _ => return false,
    }
    t.clear_h(tw::FLAG, 0x2500);
    true
}

/// fn_800AD584(task work, player work): in stage 0x1D or with game byte
/// +0x1E, for a standing or running player with no lock (+0x33 per
/// player) and no game pause (+0x42), queue request 0x41, 0x40 or 0x3F
/// for the player's set flags at +0x2F/+0x2E/+0x2D (3 bytes per player).
pub fn queue_event_requests(t: &Work, p: &mut Work, g: &ActGlobals) -> bool {
    let gs = &g.gs;
    if gs.b(0x1E) == 0 && g.stage != 0x1D {
        return false;
    }
    let n = p.b(pw::PNUM) as isize;
    let at = |o: isize| o as usize;
    if gs.b(at(0x33 + n)) > 0 || gs.b(0x42) != 0 {
        return false;
    }
    let mode = t.b(tw::MODE);
    if mode != 0 && mode != 1 {
        return false;
    }
    let b = 3 * n;
    let req = if gs.ub(at(0x2F + b)) == 1 {
        0x41
    } else if gs.ub(at(0x2E + b)) == 1 {
        0x40
    } else if gs.ub(at(0x2D + b)) == 1 {
        0x3F
    } else {
        return false;
    };
    push_request(p, req);
    true
}

/// fn_80053C74(task work, player work): standing, queue request 0x2E or
/// 0x2D (by +0xD) when the character's button bit in +0x24 is set, else
/// 0x30 when +0xD is 0x2D or 0x30.
pub fn queue_button_requests(t: &Work, p: &mut Work) -> bool {
    if t.b(tw::MODE) != 0 {
        return false;
    }
    let bit = match p.b(pw::CHAR) {
        0 => 0x20,
        1 => 0x8_0000,
        4 => 0x4000,
        5 => 0x1000_0000,
        7 => 0x100_0000,
        6 => 0x200,
        _ => 0,
    };
    let held = p.w(0x24) & bit != 0;
    let d = p.b(0xD);
    let req = if held {
        match d {
            0x2E => 0x2E,
            0x2D => 0x2D,
            _ => return false,
        }
    } else if d == 0x2D || d == 0x30 {
        0x30
    } else {
        return false;
    };
    push_request(p, req);
    true
}

/// fn_80053DBC(task work, player work): on the ground, standing or
/// running with +0xD at 0x32, queue request 0x32.
pub fn queue_ground_request(t: &Work, p: &mut Work) -> bool {
    if t.h(tw::FLAG) & 3 == 0 {
        return false;
    }
    let mode = t.b(tw::MODE);
    if mode != 0 && mode != 1 {
        return false;
    }
    if p.ub(0xD) != 0x32 {
        return false;
    }
    push_request(p, 0x32);
    true
}

/// A lock-on candidate from the engine's two collision target lists
/// (0x801E31B0 counted by fn_80005510(0), then 0x801E2FB0 counted by
/// fn_80005510(1)): entries whose collision has a target task work
/// (+0x9C, then +0x34). `kind` is the first info entry's kind byte and
/// `center` what fn_8000EDD0 gives for it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LockTarget {
    pub kind: u8,
    pub center: Vec3,
}

/// The engine data fn_800AC950 reads.
pub trait LockWorld {
    /// fn_80037AB8(pnum) and the other player's +0x218 point.
    fn partner(&mut self, pnum: i8) -> Option<Vec3>;
    /// The candidates of list 0 (0x801E31B0) or 1 (0x801E2FB0).
    fn targets(&mut self, list: usize) -> Vec<LockTarget>;
}

const LOCK_PARTNER_AHEAD: f32 = 20.0; // r2-0x6950
const LOCK_TARGET_AHEAD: f32 = 30.0; // r2-0x69CC
const LOCK_RANGE: f32 = 15.0; // r2-0x6990

/// fn_800AC950(task work, player work): whether something to lock onto is
/// within 15 of the point 20 (the other player) or 30 (a target) ahead of
/// the walker's +0x218, while standing, running or in mode 0x11.
pub fn lock_in_range(
    stack: &mut MatrixStack,
    t: &Work,
    p: &Work,
    world: &mut dyn LockWorld,
) -> bool {
    let mode = t.b(tw::MODE);
    if mode != 0 && mode != 1 && mode != 0x11 {
        return false;
    }
    let base = p.v3(0x218);
    let angles = t.ang3(tw::ANG);
    let near = |stack: &mut MatrixStack, q: Vec3, ahead: f32| {
        let v = rotate_by_angles(stack, angles, [1.0, 0.0, 0.0]).map(|x| x * ahead);
        let d = [
            q[0] - base[0] - v[0],
            q[1] - base[1] - v[1],
            q[2] - base[2] - v[2],
        ];
        nj_scalor(d) < LOCK_RANGE
    };
    if let Some(q) = world.partner(p.b(pw::PNUM)) {
        if near(stack, q, LOCK_PARTNER_AHEAD) {
            return true;
        }
    }
    for list in [0, 1] {
        for e in world.targets(list) {
            if e.kind == 0xC {
                continue;
            }
            if near(stack, e.center, LOCK_TARGET_AHEAD) {
                return true;
            }
        }
    }
    false
}

/// SE_Call(id, 0, 0, 0).
pub fn se_call(id: i32) -> EngineCall {
    EngineCall::Sound {
        addr: 0x8006_B978,
        id,
        args: [0; 3],
        pos: None,
    }
}

/// fn_8006B7EC(id, 0, 0, 0x7F, position).
pub fn se_at(id: i32, pos: Vec3) -> EngineCall {
    EngineCall::Sound {
        addr: 0x8006_B7EC,
        id,
        args: [0, 0, 0x7F],
        pos: Some(pos),
    }
}

/// The walkers' jump sounds by voice character (player byte +3).
fn jump_sounds(t: &Work, p: &Work, eng: &mut dyn Engine) {
    let (a, b) = match p.b(3) {
        0xD => (0x2019, 0x3012),
        0xE => (0x201C, 0x3018),
        6 => (0x2000, 0x3000),
        7 => (0x2001, 0x300A),
        _ => return,
    };
    eng.call(se_call(a));
    eng.call(se_at(b, t.v3(tw::POS)));
}

/// fn_800ADD34 (`action` None: 0x79 for character 7, else 0xAD) and
/// fn_800ADED0 (`action` 0x41): start the walker's jump. A ride (player
/// +0xA0) passes its acceleration (its task's +0x3C block, +0x10) to the
/// player's motion work (fn_80039A28); `ride_acc` is that vector.
pub fn walker_jump(
    t: &mut Work,
    m: &mut Work,
    p: &mut Work,
    ride_acc: Option<Vec3>,
    action: Option<i16>,
    eng: &mut dyn Engine,
) {
    if let Some(a) = ride_acc {
        m.set_v3(0xC, a);
    }
    p.set_f(0x68, p.f(0xD8));
    t.set_b(tw::MODE, 6);
    let act = action.unwrap_or(if p.b(pw::CHAR) == 7 { 0x79 } else { 0xAD });
    p.set_h(pw::ACTION, act);
    t.clear_h(tw::FLAG, 0x2500);
    t.clear_h(tw::FLAG, 2);
    p.set_h(0x12, 0);
    jump_sounds(t, p, eng);
}

/// fn_800548E0(task work, motion work, player work): with the stick
/// released and the player stopped, leave the current mode: hanging (task
/// flag 0x800) picks mode 0x17, 0x16 or 0x15 by what is held, otherwise
/// back to standing. Returns whether it stopped.
pub fn stop_check(
    t: &mut Work,
    m: &mut Work,
    p: &mut Work,
    g: &ActGlobals,
    s: &mut dyn ActSteps,
) -> bool {
    if s.analog(t, p) || 0.0 != p.f(pw::SPD) {
        return false;
    }
    if t.h(tw::FLAG) & 0x800 != 0 {
        let r = s.hold_flags(t, p);
        if r & 0x800 != 0 {
            set_mode(t, p, 0x17, 0x2C);
        } else if r & 0x400 != 0 {
            set_mode(t, p, 0x16, 0x2C);
        } else {
            set_mode(t, p, 0x15, 0x23);
        }
    } else {
        go_stand(t, m, p, g, s);
    }
    true
}

/// fn_80038008(pnum, back, pos, ang): the player's position and angles
/// `back` frames ago from the history ring, when `playertwp[pnum]` exists.
pub fn history_back(
    hist: &crate::walker_exec::PlayerHistory,
    pnum: i8,
    back: u8,
) -> (Vec3, [i32; 3]) {
    let n = pnum as usize;
    let slot = (n << 8) + hist.index[n].wrapping_sub(back) as usize;
    (hist.pos[slot], hist.ang[slot])
}

const LAND_UP: f32 = f32::from_bits(0xbf7c_1bda); // r2-0x7358
const LAND_DOWN: f32 = f32::from_bits(0xbf7f_ff58); // r2-0x7354
const LAND_REACH: f32 = 60.0; // r2-0x7350
const LAND_UP_FLIP: f32 = f32::from_bits(0x3f7f_ff58); // r2-0x734C

/// fn_800545D0(task work, motion work, player work): leaving the ground.
/// A running non-walker that ran off flat ground with the floor (or, under
/// reversed gravity, the ceiling) within 60 is put back on the path
/// (mode 0xB, action 0x3D) at its position 3 (walkers) or 2 frames ago,
/// with its character's sound or voice. Otherwise the task falls: mode 6
/// with task flag 0x100, the hold modes with 0x800, else mode 0xA, action
/// 0xF. Returns whether it left the ground.
pub fn leave_ground(
    t: &mut Work,
    m: &mut Work,
    p: &mut Work,
    g: &ActGlobals,
    hist: &crate::walker_exec::PlayerHistory,
    s: &mut dyn ActSteps,
    eng: &mut dyn Engine,
) -> bool {
    if t.h(tw::FLAG) & 3 != 0 {
        return false;
    }
    let pnum = p.b(pw::PNUM);
    let c = p.b(pw::CHAR);
    let mode = t.b(tw::MODE);
    let eligible = g.gs.b((0x3F + pnum as isize) as usize) != 0
        && p.w(0x8C) & 0x0800_0000 == 0
        && c != 6
        && c != 7
        && (mode == 1 || ((c as u8) <= 1 && (0x3D..=0x44).contains(&mode)))
        && p.h(0x14) == 0
        && crate::ninja_vector::inner_product(p.v3(0x7C), g.gravity) < LAND_UP
        && p.f(pw::SPD) >= p.f(0xE4)
        && 0.0 == crate::land_nodes::nj_scalor(p.v3(0x70));
    if eligible {
        let gy = g.gravity[1];
        let y = t.f(tw::POS + 4);
        let back_on = (gy < LAND_DOWN && p.f(0x1B4) <= y - LAND_REACH)
            || (gy > LAND_UP_FLIP && p.f(0x1B0) >= LAND_REACH + y);
        if back_on && p.w(0x1A8) & 0x2002 == 0 {
            t.set_b(tw::MODE, 0xB);
            p.set_h(pw::ACTION, 0x3D);
            zero_speeds(m, p);
            let back = if c == 7 || c == 6 { 3 } else { 2 };
            let (pos, ang) = history_back(hist, pnum, back);
            t.set_v3(tw::POS, pos);
            t.set_ang3(tw::ANG, ang);
            p.set_h(0x18, 0);
            let voice = |eng: &mut dyn Engine, id| {
                eng.call(EngineCall::PlayVoice { id });
                None
            };
            let sound = match p.ub(3) {
                0 | 4 | 6 => Some(0x3002),
                1 | 0xD => Some(0x3014),
                2 => voice(eng, 0x65F),
                3 => voice(eng, 0x65E),
                5 => Some(0x3013),
                7 => Some(0x300C),
                8 => voice(eng, 0xA72),
                0xE => Some(0x301A),
                0xF => Some(0x3022),
                _ => None,
            };
            if let Some(id) = sound {
                eng.call(se_at(id, t.v3(tw::POS)));
            }
            return true;
        }
        t.set_b(tw::MODE, 0xA);
        p.set_h(pw::ACTION, 0xF);
        return true;
    }
    let flag = t.h(tw::FLAG);
    if flag & 0x100 != 0 {
        t.set_b(tw::MODE, 6);
        return true;
    }
    if flag & 0x800 != 0 {
        let r = s.hold_flags(t, p);
        if r & 0x200 != 0 {
            set_mode(t, p, 0x1C, 0x2F);
            return true;
        }
        s.release_hold(t, m, p);
    }
    t.set_b(tw::MODE, 0xA);
    p.set_h(pw::ACTION, 0xF);
    true
}

/// fn_800AD6E0(task work, motion work, player work): on the action button,
/// take the queued request matching the current one (+0xD), or the first,
/// into +0xE and run its handler. Request 0x3D starts the walker's
/// lock-on stance (mode 0x3D, action 0x80) with a rumble and its sounds.
/// Returns whether a request ran.
pub fn action_button(
    t: &mut Work,
    m: &mut Work,
    p: &mut Work,
    g: &ActGlobals,
    s: &mut dyn ActSteps,
    eng: &mut dyn Engine,
) -> bool {
    let pnum = p.b(pw::PNUM);
    if g.action[pnum as usize] == 0 {
        return false;
    }
    let n = p.b(0xC) as i32;
    if n == 0 {
        return false;
    }
    let want = p.b(0xD);
    let mut i = 0;
    if n > 0 {
        while i < n && p.b(4 + i as usize) != want {
            i += 1;
        }
    }
    let req = if n == i { p.b(4) } else { want };
    p.set_b(0xE, req);
    let handler = match (req as i32).wrapping_sub(0xA) as u32 {
        0x0 => 0x8005_3490,
        0x20 => 0x8005_30B0,
        0x23 => 0x8005_3570,
        0x24 => 0x8005_3520,
        0x26 => 0x8005_34F4,
        0x28 => 0x8005_35C0,
        0x33 => {
            set_mode(t, p, 0x3D, 0x80);
            p.set_h(0x17C, 0);
            t.or_h(tw::FLAG, 0x400);
            zero_speeds(m, p);
            eng.call(EngineCall::Rumble {
                addr: 0x8002_F9D4,
                args: [pnum as i32, 3, 0x1E, 0x20],
            });
            eng.call(se_call(0x200A));
            if p.ub(3) == 7 {
                eng.call(se_at(0x300E, t.v3(tw::POS)));
            }
            return true;
        }
        0x34 => {
            // fn_800AC850: the Tails/Chao walkers' lock-on stance.
            set_mode(t, p, 0x3E, 0xB4);
            p.set_h(0x17C, 0);
            t.or_h(tw::FLAG, 0x400);
            zero_speeds(m, p);
            eng.call(EngineCall::Rumble {
                addr: 0x8002_F9D4,
                args: [pnum as i32, 3, 0x1E, 0x20],
            });
            eng.call(se_call(0x2016));
            let id = match p.ub(3) {
                0xD => Some(0x3016),
                0xE => Some(0x301C),
                6 => Some(0x3004),
                _ => None,
            };
            if let Some(id) = id {
                eng.call(se_at(id, t.v3(tw::POS)));
            }
            return true;
        }
        0x35 => 0x800A_D208,
        0x36 => 0x800A_CF38,
        0x37 => 0x800A_CB9C,
        _ => return false,
    };
    s.button_action(handler, t, m, p);
    true
}

const HOVER_LIFT: f32 = f32::from_bits(0x3f4c_cccd); // r2-0x6900, 0.8
const HOVER_PULL: f32 = f32::from_bits(0x3f59_999a); // r2-0x68F8, 0.85
const SLOPE_FLAT: f32 = f32::from_bits(0x3f45_1eb8); // r2-0x6904, 0.77
const SLOPE_FLAT_11: f32 = f32::from_bits(0x3f7a_e148); // r2-0x68F4, 0.98
const LAND_SOFT: f32 = 3.0; // r2-0x6970
const NO_HOVER_Y: f32 = -1_000_000.0; // r2-0x68FC

/// The landing shared by modes 6, 0xA and 0x3C of the action step.
#[allow(clippy::too_many_arguments)]
fn land(
    t: &mut Work,
    m: &mut Work,
    p: &mut Work,
    g: &ActGlobals,
    s: &mut dyn ActSteps,
    sc: &mut dyn SteerCalls,
    eng: &mut dyn Engine,
    clear500: bool,
) {
    let back = turn_back(t.h(tw::FLAG), t.i(tw::ANG + 4), sc, &mut |k| s.touching(k));
    if back && p.f(pw::SPD) > 0.0 {
        set_mode(t, p, 0xC, 0x12);
        p.set_h(0x17C, 6);
    } else if stop_check(t, m, p, g, s) {
        take_mw_tilt(t, m);
        let len = nj_scalor(m.v3(0));
        if len < LAND_SOFT || (p.ub(0x35C) == 1 && p.ub(3) != 0xE) {
            p.set_h(pw::ACTION, 0x10);
        } else {
            p.set_h(pw::ACTION, 0x11);
            eng.call(EngineCall::Rumble {
                addr: 0x8002_FB2C,
                args: [p.b(pw::PNUM) as i32, 6, 0xF, 0],
            });
        }
        p.set_h(0x17C, 0);
    } else {
        take_mw_tilt(t, m);
        set_mode(t, p, 1, 6);
    }
    if clear500 {
        t.clear_h(tw::FLAG, 0x500);
    }
    let pn = p.b(pw::PNUM) as i32;
    eng.call(EngineCall::DeferredAt {
        addr: 0x800B_9020,
        arg: pn,
        points: [p.v3(0x3F4), p.v3(0x40C)],
    });
    eng.call(EngineCall::DeferredAt {
        addr: 0x800B_9020,
        arg: pn,
        points: [p.v3(0x400), p.v3(0x418)],
    });
    eng.call(se_call(0x2003));
}

fn take_mw_tilt(t: &mut Work, m: &Work) {
    t.set_i(tw::ANG, m.i(0x18));
    t.set_i(tw::ANG + 8, m.i(0x20));
}

fn hover_start(t: &mut Work, p: &mut Work) {
    if p.ub(pw::CHAR) == 7 && p.w(0x24) & 0x10_0000 != 0 {
        set_mode(t, p, 0x3C, 0x7F);
        t.clear_h(tw::FLAG, 0x500);
    }
    if p.ub(pw::CHAR) == 6 && p.w(0x24) & 0x40 != 0 {
        set_mode(t, p, 0x3C, 0xB3);
        t.clear_h(tw::FLAG, 0x500);
    }
}

/// The jump check the modes share: with the button pressed, fn_800ADD34
/// at or above +0xE8, else fn_800ADED0.
fn try_jump(
    t: &mut Work,
    m: &mut Work,
    p: &mut Work,
    g: &ActGlobals,
    s: &mut dyn ActSteps,
    eng: &mut dyn Engine,
) -> bool {
    if !button_pressed(t, p, g) {
        return false;
    }
    let ride = s.ride_acc(p);
    if p.f(pw::SPD) >= p.f(0xE8) {
        walker_jump(t, m, p, ride, None, eng);
    } else {
        walker_jump(t, m, p, ride, Some(0x41), eng);
    }
    true
}

/// fn_800B24F4(task work, motion work, player work): the walker's action
/// step, run before its mode handlers.
#[allow(clippy::too_many_arguments)]
pub fn action_step(
    stack: &mut MatrixStack,
    t: &mut Work,
    m: &mut Work,
    p: &mut Work,
    g: &ActGlobals,
    hist: &PlayerHistory,
    s: &mut dyn ActSteps,
    sc: &mut dyn SteerCalls,
    world: &mut dyn LockWorld,
    eng: &mut dyn Engine,
) {
    if p.h(pw::STATUS) as u16 & 0x8000 != 0 {
        t.or_h(tw::FLAG, 0x8000);
        t.set_h(tw::WTIMER, 0);
    } else if t.h(tw::WTIMER) == 0 {
        if s.damage(t, p) {
            t.or_h(tw::FLAG, 0x8000);
            t.clear_h(tw::FLAG, 0x3900);
            if p.h(pw::STATUS) & 3 != 0 {
                p.clear_h(pw::STATUS, 3);
            }
        } else {
            t.clear_h(tw::FLAG, 0x8000);
        }
    } else {
        if g.gs_b49 == 0 {
            t.set_h(tw::WTIMER, t.h(tw::WTIMER).wrapping_sub(1));
        }
        t.clear_h(tw::FLAG, 4);
    }
    queue_event_requests(t, p, g);
    queue_button_requests(t, p);
    queue_ground_request(t, p);
    if p.h(0x368) > 0 {
        push_request(p, 0x42);
    }
    if lock_in_range(stack, t, p, world) {
        let r = if p.ub(3) == 0xE {
            0x3E
        } else if p.ub(pw::CHAR) == 7 {
            0x3D
        } else {
            0x3E
        };
        push_request(p, r);
    }
    queue_stand_requests(t, p);
    let pnum = p.b(pw::PNUM) as usize;
    let mode = t.b(tw::MODE) as i32;
    macro_rules! cmd {
        () => {
            if walker_command(t, m, p, g, s, eng) {
                return;
            }
        };
    }
    match mode {
        0 | 0x11 => {
            cmd!();
            if action_button(t, m, p, g, s, eng)
                || leave_ground(t, m, p, g, hist, s, eng)
                || try_jump(t, m, p, g, s, eng)
            {
                return;
            }
            if s.analog(t, p) {
                set_mode(t, p, 1, 6);
                return;
            }
            if mode == 0x11 {
                if nj_scalor(m.v3(0)) < p.f(0xE0) || g.query_20 > SLOPE_FLAT_11 {
                    t.set_b(tw::MODE, 0);
                    p.set_h(0x18, 0);
                    p.set_h(pw::ACTION, 1);
                }
                return;
            }
            if g.query_20 < SLOPE_FLAT && nj_scalor(p.v3(pw::SPD)) > p.f(0xE0) {
                t.set_b(tw::MODE, 0x11);
                let mut aim = m.i(0x1C);
                let c = slope_aim(
                    stack,
                    t.h(tw::FLAG),
                    t.ang3(tw::ANG),
                    m.v3(0),
                    &mut aim,
                    p.h(0x14),
                    p.w(0x88),
                    p.f(0xE4),
                    g.gravity,
                    sc,
                );
                m.set_i(0x1C, aim);
                p.set_h(pw::ACTION, if c < 0.0 { 0x1D } else { 0x1C });
            }
        }
        1 => {
            cmd!();
            if action_button(t, m, p, g, s, eng)
                || leave_ground(t, m, p, g, hist, s, eng)
                || try_jump(t, m, p, g, s, eng)
                || stop_check(t, m, p, g, s)
            {
                return;
            }
            let back = turn_back(t.h(tw::FLAG), t.i(tw::ANG + 4), sc, &mut |k| s.touching(k));
            if back && p.f(pw::SPD) >= p.f(0xE4) {
                set_mode(t, p, 0xC, 0x12);
                p.set_h(0x17C, 6);
                eng.call(se_call(match p.ub(3) {
                    0xD => 0x201A,
                    0xE => 0x201D,
                    _ => 0x2002,
                }));
            }
        }
        2 => {
            cmd!();
            if leave_ground(t, m, p, g, hist, s, eng) || try_jump(t, m, p, g, s, eng) {
                t.clear_h(tw::FLAG, 0x2000);
                return;
            }
            if p.f(pw::SPD) < p.f(0xE4) {
                set_mode(t, p, 1, 6);
                t.clear_h(tw::FLAG, 0x2000);
                return;
            }
            if stop_check(t, m, p, g, s) {
                t.clear_h(tw::FLAG, 0x2000);
            }
        }
        6 | 0xA | 0x3C => {
            if walker_command(t, m, p, g, s, eng) {
                if mode == 0x3C {
                    eng.call(EngineCall::StopSound { id: 0x2009 });
                }
                return;
            }
            if t.h(tw::FLAG) & 3 != 0 {
                land(t, m, p, g, s, sc, eng, mode == 6);
                if mode == 0x3C {
                    eng.call(EngineCall::StopSound { id: 0x2009 });
                }
                return;
            }
            match mode {
                6 => {
                    if p.h(pw::STATUS) & 0x4000 == 0 && button_pressed(t, p, g) {
                        hover_start(t, p);
                        p.set_f(0x37C, t.f(tw::POS + 4));
                        return;
                    }
                    if button_held(t, p, g) {
                        p.set_f(0x68, fmadds(HOVER_LIFT, p.f(0xF8), p.f(0x68)));
                    }
                    let a = p.h(pw::ACTION) as i32;
                    if (a == 0x41
                        || a == 0x42
                        || ((a - 0x79) & 0xFFFF) <= 1
                        || ((a - 0xAD) & 0xFFFF) <= 1)
                        && p.f(0x68) <= 0.0
                    {
                        p.set_h(pw::ACTION, 0x43);
                    }
                }
                0xA => {
                    if p.h(pw::STATUS) & 0x4000 == 0 && button_pressed(t, p, g) {
                        hover_start(t, p);
                        p.set_f(0x37C, NO_HOVER_Y);
                    }
                }
                _ => {
                    if g.held[pnum] == 0 || p.h(pw::STATUS) & 0x4000 != 0 {
                        set_mode(t, p, 0xA, 0xF);
                        eng.call(EngineCall::StopSound { id: 0x2009 });
                    } else {
                        m.set_f(0x10, fmadds(HOVER_PULL, p.f(0x138), m.f(0x10)));
                    }
                }
            }
        }
        7 | 8 => {
            cmd!();
            if inner_product(m.v3(0), g.gravity) >= 0.0 {
                set_mode(t, p, 0xA, 0xE);
            }
        }
        0xC => {
            cmd!();
            if leave_ground(t, m, p, g, hist, s, eng) || try_jump(t, m, p, g, s, eng) {
                return;
            }
            if p.h(pw::ACTION) != 0x12 {
                go_stand(t, m, p, g, s);
            }
        }
        0xB => {
            cmd!();
            let n = p.h(0x18);
            p.set_h(0x18, n.wrapping_add(1));
            if n <= 0xB4 && g.button_4c[pnum] == 0 && g.pressed[pnum] == 0 && g.action[pnum] == 0 {
                return;
            }
            if t.h(tw::FLAG) & 3 != 0 {
                take_mw_tilt(t, m);
                go_stand(t, m, p, g, s);
            } else {
                set_mode(t, p, 0xA, 0xF);
            }
        }
        0xF => {
            cmd!();
            let h6 = t.h(tw::WTIMER) as u16;
            let landing = if t.ub(2) == 3 && h6 != 0x78 {
                true
            } else if !(h6 > 0x5A || g.stage == 0x1D) {
                true
            } else {
                h6 <= 0x41 && g.stage == 0x1D
            };
            if landing && t.h(tw::FLAG) & 3 != 0 {
                take_mw_tilt(t, m);
                if stop_check(t, m, p, g, s) {
                    p.set_h(pw::ACTION, 0x16);
                } else {
                    set_mode(t, p, 1, 6);
                }
                return;
            }
            if h6 > 0x5A
                || t.h(tw::FLAG) & 3 != 0
                || t.h(tw::FLAG) & 0x4000 != 0
                || p.h(pw::STATUS) & 0x4000 != 0
                || g.held[pnum] == 0
            {
                return;
            }
            hover_start(t, p);
            p.set_f(0x37C, NO_HOVER_Y);
        }
        0x33 => {
            cmd!();
            try_jump(t, m, p, g, s, eng);
        }
        0x12 | 0x38 => {
            walker_command(t, m, p, g, s, eng);
        }
        0x10 => {
            cmd!();
            if p.h(pw::ACTION) == 0x18
                || leave_ground(t, m, p, g, hist, s, eng)
                || p.h(pw::ACTION) == 0x19
            {
                return;
            }
            go_stand(t, m, p, g, s);
        }
        0x43 => {
            let a = p.h(pw::ACTION);
            if a != 0x18 && a != 0x19 {
                go_stand(t, m, p, g, s);
            }
        }
        0x26 => {
            cmd!();
            if g.button_4c[pnum] != 0 || g.pad_10[pnum] & 0x3_06FE != 0 {
                p.set_h(0x362, p.h(0x362).wrapping_sub(0x14));
            }
            if p.h(0x362) <= 0 {
                go_stand(t, m, p, g, s);
            }
        }
        0x2A | 0x35 => {
            cmd!();
            if p.h(0x180) & 1 != 0 {
                go_stand(t, m, p, g, s);
            }
        }
        0x32 => {
            cmd!();
            go_stand(t, m, p, g, s);
        }
        0x2D | 0x30 => {
            cmd!();
            if p.h(0x180) & 1 != 0 {
                go_stand(t, m, p, g, s);
            } else {
                t.set_b(tw::MODE, 0x35);
            }
        }
        0x2E | 0x2F => {
            cmd!();
            let a = p.h(0x178);
            if a == 0x52 || a == 0x53 {
                t.set_b(tw::MODE, 0x2F);
            } else {
                go_stand(t, m, p, g, s);
            }
        }
        0x3D | 0x3E => {
            cmd!();
            let keep = if mode == 0x3D { 0x80 } else { 0xB4 };
            if p.h(pw::ACTION) != keep {
                go_stand(t, m, p, g, s);
                t.clear_h(tw::FLAG, 0x400);
            }
        }
        0x3F | 0x40 => {
            cmd!();
            let want = if mode == 0x3F { 0x83 } else { 0x84 };
            if p.h(0x17A) == want {
                go_stand(t, m, p, g, s);
            }
        }
        0x41 => {
            cmd!();
            let lim = if p.ub(3) == 0xE { 0xDC } else { 0xA0 };
            let n = p.h(0x362);
            p.set_h(0x362, n.wrapping_add(1));
            if n as i32 > lim {
                go_stand(t, m, p, g, s);
            } else if p.ub(3) == 7 {
                p.set_f(0x378, g.motion_82_frames.wrapping_add(2) as f32);
            }
        }
        0x36 => {
            if t.ub(tw::SMODE) == 0xF {
                go_stand(t, m, p, g, s);
                t.clear_h(tw::FLAG, 0x2500);
            }
        }
        0x3A => {
            if g.pad_10[pnum] & 4 != 0 {
                set_mode(t, p, 1, 6);
            }
        }
        _ => {}
    }
}

/// What the damage step asks the engine's collision lists and the other
/// players (through `playertp[pnum]`, the player's own task).
pub trait DamageWorld {
    /// fn_800063F4, CCL_IsHitPlayer (0x800066F4), fn_80006270: whether
    /// each finds a hit on the player's task.
    fn hit_63f4(&mut self) -> bool;
    fn hit_player(&mut self) -> bool;
    fn hit_6270(&mut self) -> bool;
    /// The hit records fn_80006574 walks: whether the last one's owner
    /// (+8) is the player's own task (`None` without records).
    fn last_record_is_self(&mut self) -> Option<bool>;
    /// fn_80005874(task, 0): the kind of the first hit.
    fn first_kind(&mut self) -> i32;
    /// fn_800060EC(task, kind): the task met through an info entry of
    /// `kind`, with its position and IsThisTaskPlayer's answer (-1 when
    /// it is not a player) and that player's character byte.
    fn kind_hit(&mut self, kind: u8) -> Option<(Vec3, i32, i8)>;
    /// The damage the attacker's collision (+0xA4, kinds 2 and 3) names:
    /// its +0x9C task's motion work +0x54.
    fn attacker_damage(&mut self) -> Option<f32>;
    /// The other player's character byte +3 (`playerpwp[pnum ^ 1]`), if any.
    fn other_voice_char(&mut self) -> Option<i8>;
    /// fn_80039B04(pnum, x, y, z): face the player along the vector.
    fn face(&mut self, pnum: i8, v: Vec3);
    /// fn_800542A0 on the player's motion work.
    fn release_hold(&mut self, t: &mut Work, p: &mut Work);
    /// Whether `playerpwp[0]` has status bit 0x4000.
    fn p1_status_4000(&mut self) -> bool;
}

const HURT_SPEED: f32 = 0.5; // r2-0x7398
const KNOCK: f32 = 8.0; // r2-0x7390
const BIG_HIT: f32 = 3.0; // r2-0x7388
const MID_HIT: f32 = 6.0; // r2-0x7328
const FAT_HIT: f32 = 5.0; // r2-0x73DC
const DMG_SPIKE: f32 = f32::from_bits(0x409c_cccd); // r2-0x731C, 4.9
const DMG_CHAO: f32 = f32::from_bits(0x3f99_999a); // r2-0x7318, 1.2
const DMG_EGG: f32 = f32::from_bits(0x3f33_3333); // r2-0x7330, 0.7
const DMG_D: f32 = f32::from_bits(0x3f4c_cccd); // r2-0x738C, 0.8
const HP_MAX: f32 = 10.0; // r2-0x7600
const HP_BONUS: f32 = 2.0; // r2-0x75FC
const DMG_SOFT: f32 = 2.0; // r2-0x73CC

/// AddMechHP (0x80035CF0)(pnum, delta) on a walker's player work: add to
/// the health +0x48, at least 0 and at most 10 (12 with +0x24 bit
/// 0x800000), with the character's voice when it fills up.
pub fn add_mech_hp(p: &mut Work, delta: f32, eng: &mut dyn Engine) {
    let c = p.ub(pw::CHAR);
    if c != 6 && c != 7 {
        return;
    }
    let old = p.f(0x48);
    p.set_f(0x48, old + delta);
    let mut max = HP_MAX;
    if p.w(0x24) & 0x80_0000 != 0 {
        max += HP_BONUS;
    }
    if p.f(0x48) <= 0.0 {
        p.set_f(0x48, 0.0);
    } else if p.f(0x48) >= max {
        p.set_f(0x48, max);
        if old != max {
            match p.b(3) {
                0xD => eng.call(EngineCall::PlayVoice { id: 0xA88 }),
                0xE => eng.call(EngineCall::PlayVoice { id: 0xA93 }),
                _ => {}
            }
        }
    }
}

/// fn_80056D34(task work, player work): the walker's damage from what hit
/// it, then AddMechHP and KillPlayer at zero health.
fn mech_damage(p: &mut Work, gs: &Work, w: &mut dyn DamageWorld, eng: &mut dyn Engine) {
    let other = w.other_voice_char();
    let base = if gs.b(0x1E) != 0 && gs.ub(0x1F) == 1 {
        0.0
    } else if p.w(0x88) & 0x400 != 0 {
        BIG_HIT
    } else if let Some(d) = w.attacker_damage() {
        d
    } else {
        let scale = |d: f32| if other == Some(0xE) { d * DMG_CHAO } else { d };
        if w.kind_hit(8).is_some() {
            scale(DMG_SPIKE)
        } else if w.kind_hit(0xA).is_some() || w.kind_hit(9).is_some() {
            scale(BIG_HIT)
        } else if w.kind_hit(7).is_some() {
            1.0
        } else {
            BIG_HIT
        }
    };
    let mut d = base;
    if p.b(3) == 0xE {
        d *= DMG_EGG;
    }
    if other == Some(0xD) {
        d *= DMG_D;
    }
    eng.call(se_call(if d <= DMG_SOFT { 0x2014 } else { 0x2015 }));
    add_mech_hp(p, -d, eng);
    if p.f(0x48) <= 0.0 {
        eng.call(EngineCall::KillPlayer {
            pnum: p.b(pw::PNUM),
        });
    }
}

fn battle_voice(eng: &mut dyn Engine, id: i32) {
    eng.call(EngineCall::BattleVoice { id });
}

/// fn_80055734(task work, player work): the damage step. Returns whether
/// the player was hurt.
pub fn damage_step(
    t: &mut Work,
    m: &mut Work,
    p: &mut Work,
    g: &ActGlobals,
    w: &mut dyn DamageWorld,
    eng: &mut dyn Engine,
) -> bool {
    let gs = &g.gs;
    let mut voice: i32 = -1;
    if gs.b(0x42) != 0 || t.ub(2) == 2 {
        return false;
    }
    let mode = t.b(tw::MODE);
    if (0x37..0x3A).contains(&mode) {
        return false;
    }
    let c = p.b(pw::CHAR);
    if t.h(tw::FLAG) & 4 != 0
        && (c == 6 || c == 7)
        && !w.hit_63f4()
        && !w.hit_player()
        && w.last_record_is_self() == Some(true)
    {
        t.clear_h(tw::FLAG, 4);
    }
    if t.h(tw::FLAG) & 4 != 0 && g.stage == 0x2A && (w.hit_6270() || w.hit_63f4()) {
        t.clear_h(tw::FLAG, 4);
    }
    if t.h(tw::FLAG) & 4 == 0 {
        if w.first_kind() == 3 || p.w(0x88) & 0x400 == 0 {
            return false;
        }
    }
    t.set_h(tw::WTIMER, 0x78);
    let pnum = p.b(pw::PNUM);
    let rp = if g.stage == 0x42 { 0 } else { pnum as i32 };
    eng.call(EngineCall::Rumble {
        addr: 0x8002_F9D4,
        args: [rp, 7, 0x3B, 0x18],
    });
    if p.w(0x24) & 0x8000 != 0 {
        p.set_f(pw::SPD, p.f(pw::SPD) * HURT_SPEED);
        return false;
    }
    if p.h(pw::STATUS) & 0x4000 != 0 {
        return false;
    }
    if t.ub(tw::MODE) != 0x38 {
        t.set_b(tw::MODE, 0xF);
        if p.ub(3) == 0xE {
            if w.kind_hit(8).is_some() && w.kind_hit(0xA).is_some() && w.kind_hit(9).is_some() {
                p.set_h(pw::ACTION, 0x15);
            }
        } else {
            p.set_h(pw::ACTION, 0x15);
        }
        t.clear_h(tw::FLAG, 4);
        p.set_f(0x68, p.f(0xD8));
        p.set_f(pw::SPD, -p.f(0xE8));
    }
    t.clear_h(tw::FLAG, 0x2500);
    if let Some((opos, who, och)) = w.kind_hit(7) {
        let burst = |eng: &mut dyn Engine, n: usize| {
            for _ in 0..n {
                eng.call(EngineCall::Deferred {
                    addr: 0x800B_CF50,
                    arg: pnum as i32,
                });
            }
        };
        if who == -1 {
            burst(eng, 4);
            eng.call(EngineCall::DeferredAt {
                addr: 0x800B_F6A4,
                arg: 0x30,
                points: [p.v3(0x218), p.v3(0x2B4)],
            });
        } else if och == 7 {
            burst(eng, 2);
        } else {
            eng.call(EngineCall::DeferredAt {
                addr: 0x800B_F6A4,
                arg: 0x30,
                points: [p.v3(0x218), p.v3(0x2B4)],
            });
        }
        let pos = t.v3(tw::POS);
        let d = crate::land_contact::unit_vector([
            pos[0] - opos[0],
            pos[1] - opos[1],
            pos[2] - opos[2],
        ]);
        w.face(pnum, [-d[0], 0.0, -d[2]]);
        let v = d.map(|x| x * KNOCK);
        m.set_v3(0, v);
        let mut stack = MatrixStack::new();
        let local = crate::ninja_matrix::unrotate_by_angles(&mut stack, t.ang3(tw::ANG), v);
        p.set_v3(pw::SPD, local);
        p.set_h(0x14, 0x3C);
        t.set_b(tw::MODE, 0x43);
        p.set_h(pw::ACTION, 0x18);
    }
    if p.h(pw::STATUS) & 3 != 0 {
        p.clear_h(pw::STATUS, 3);
        return true;
    }
    eng.call(EngineCall::Indirect {
        ptr: 0x801A_2A18,
        arg: pnum as i32,
    });
    eng.call(EngineCall::GameStateBytes {
        at: (0x2D + 3 * pnum as isize) as u16,
        len: 3,
        value: 0,
    });
    let c = p.b(pw::CHAR);
    if c == 7 || c == 6 {
        let before = p.f(0x48);
        mech_damage(p, gs, w, eng);
        let hp = p.f(0x48);
        if t.ub(2) != 3 {
            if !(hp <= 0.0) {
                let pick = |a, b, cc, d| match p.b(3) {
                    0xD => a,
                    0xE => b,
                    6 => cc,
                    7 => d,
                    _ => -1,
                };
                if before > BIG_HIT && hp <= BIG_HIT {
                    voice = pick(0xA83, 0xA8E, 0x67F, 0x67E);
                } else if before > MID_HIT && hp <= MID_HIT {
                    voice = pick(0xA82, 0xA8D, 0x67D, 0x67C);
                }
            }
            if voice != -1 {
                eng.call(EngineCall::PlayVoice { id: voice as i16 });
            }
        } else {
            let st = g.stage;
            if (st == 0x14 || st == 0x1D) && before >= FAT_HIT && p.f(0x48) < FAT_HIT {
                voice = match (st, p.b(3)) {
                    (0x14, 6) => 0x8D8,
                    (0x14, 7) => 0x818,
                    (0x1D, 6) => 0x8E0,
                    (0x1D, 7) => 0x821,
                    _ => -1,
                };
            }
            if voice != -1 {
                battle_voice(eng, voice);
            }
        }
    } else if t.ub(2) == 3 {
        let before = p.f(0x48);
        p.set_f(0x48, approach(before, 0.0, 1.0));
        if p.f(0x48) <= 0.0 {
            eng.call(EngineCall::KillPlayer { pnum });
        } else if before > 1.0 && p.f(0x48) <= 1.0 {
            voice = match (g.stage, p.b(3)) {
                (0x13, 0) => 0x8BF,
                (0x13, 1) => 0x8A7,
                (0x2A, 0) => 0x8C9,
                (0x2A, 1) => 0x8B2,
                (0x21, 4) => 0x83E,
                (0x21, 5) => 0x895,
                _ => -1,
            };
            if voice != -1 {
                battle_voice(eng, voice);
            }
        }
    }
    let dead = (t.h(tw::FLAG) & 0x1000 != 0 && t.ub(tw::SMODE) == 0x25)
        || (t.ub(2) == 3 && p.f(0x48) <= 0.0 && !w.p1_status_4000());
    if dead {
        w.release_hold(t, p);
        if voice == -1 {
            if t.ub(2) != 3 {
                voice = match p.ub(3) {
                    0 | 9 => 0x61F,
                    1 | 0xA => 0x61C,
                    2 | 6 => 0x68D,
                    3 | 7 => 0x68C,
                    4 => 0x514,
                    5 => 0x515,
                    8 => 0xA6B,
                    0xF => 0xA9D,
                    _ => -1,
                };
                if voice != -1 {
                    eng.call(EngineCall::PlayVoice { id: voice as i16 });
                }
            } else {
                let st = g.stage;
                voice = match p.ub(3) {
                    0 if st == 0x13 => 0x8C2,
                    0 if st == 0x2A => 0x8CC,
                    1 if st == 0x13 => 0x8AA,
                    1 if st == 0x2A => 0x8B3,
                    4 => 0x83F,
                    5 => 0x896,
                    6 if st == 0x14 => 0x8D9,
                    6 if st == 0x1D => 0x8E1,
                    7 if st == 0x14 => 0x819,
                    7 if st == 0x1D => 0x822,
                    _ => -1,
                };
                if voice != -1 {
                    battle_voice(eng, voice);
                }
            }
        }
        if !(t.ub(2) == 3 && !(p.f(0x48) <= 0.0)) {
            t.set_b(tw::MODE, 0x39);
            p.set_h(pw::ACTION, 0x3B);
            t.clear_h(tw::FLAG, 0x2500);
        }
        return true;
    }
    if voice != -1 {
        return true;
    }
    if t.ub(2) != 3 {
        let id = match p.ub(3) {
            0 | 9 => 0x300B,
            1 | 0xA | 0xE => 0x301D,
            2 | 6 => 0x3005,
            3 | 7 => 0x300F,
            4 => 0x300A,
            5 => 0x301B,
            8 => 0x3026,
            0xD => 0x3017,
            0xF => 0x3027,
            _ => -1,
        };
        if id != -1 {
            eng.call(se_call(id));
        }
        return true;
    }
    let st = g.stage;
    let hp = p.f(0x48);
    let id = match p.ub(3) {
        0 if st == 0x13 => {
            if hp > DMG_SOFT {
                0x8BD
            } else {
                0x8BE
            }
        }
        0 if st == 0x2A => {
            if hp > DMG_SOFT {
                0x8BD
            } else {
                0x8C8
            }
        }
        1 if st == 0x13 => {
            if hp > DMG_SOFT {
                0x8A5
            } else {
                0x8A6
            }
        }
        1 if st == 0x2A => {
            if hp > DMG_SOFT {
                0x8A5
            } else {
                0x8B1
            }
        }
        4 => {
            if hp >= BIG_HIT {
                0x83C
            } else {
                0x83D
            }
        }
        5 => {
            if hp >= BIG_HIT {
                0x893
            } else {
                0x894
            }
        }
        7 => {
            if hp > MID_HIT {
                0x814
            } else {
                0x815
            }
        }
        6 if st == 0x14 => {
            if hp > MID_HIT {
                0x8D4
            } else {
                0x8D5
            }
        }
        6 if st == 0x1D => {
            if hp > MID_HIT {
                0x8D4
            } else {
                0x8DF
            }
        }
        _ => -1,
    };
    if id != -1 {
        battle_voice(eng, id);
    }
    true
}
