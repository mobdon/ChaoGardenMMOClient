//! The walker's action step fn_800B24F4 and its helpers (GSNE8P main.dol),
//! on native-layout work areas ([`Work`]), checked against main.dol by
//! tests/walker_act.rs.
// Negated comparisons keep the compiled branches' NaN behaviour.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::{
    native_work::{pw, tw, Work},
    walker_exec::EngineCall,
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
    /// Whether the character motion table's entry 0x56 is loaded
    /// (0x801E5D28 + 0x2B4).
    pub motion_56: bool,
}

/// The routines the action step hands over to.
pub trait ActSteps {
    /// fn_80054D04(task work, motion work, player work).
    fn stand(&mut self, t: &mut Work, m: &mut Work, p: &mut Work);
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
    calls: &mut Vec<EngineCall>,
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
            calls.push(EngineCall::Deferred {
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
                7 => calls.push(EngineCall::PlayVoice { id: 0x68C }),
                6 => calls.push(EngineCall::PlayVoice { id: 0x68D }),
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
