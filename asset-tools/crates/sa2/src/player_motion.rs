//! The player motion step fn_8003D8D0 (GSNE8P main.dol): the action state
//! machine every character's exec runs on its motion work (player work
//! +0x174), checked against main.dol by tests/player_motion.rs.
//!
//! The state machine (action, frame, mode and end flags) is ported here.
//! The keyframe blend between two actions (fn_8003CA00, through
//! fn_8003D678) and the freeing of its buffers (fn_8003C908, syFree) only
//! feed the model's pose; they come back as [`BlendEvent`]s for the
//! renderer side, in call order.
// Negated comparisons keep the compiled branches' NaN behaviour.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::ninja_trig::fmadds;

/// One 0x10-byte action entry (the motion work's table at +0x24).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Action {
    /// +0: index into the character motion table (0x801E5D28).
    pub motion: i16,
    /// +2: index into the character model table (0x801E668C).
    pub model: i16,
    /// +4: the mode the action plays in.
    pub mode: i16,
    /// +6: the action that follows.
    pub next: i16,
    /// +8: blend speed into this action.
    pub blend: f32,
    /// +0xC: frames per step.
    pub speed: f32,
}

/// The motion work (player work +0x174) fields fn_8003D8D0 reads and
/// writes.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MotionWork {
    /// +0: the mode (0..0xF).
    pub mode: i16,
    /// +2: the requested action.
    pub request: i16,
    /// +4: the current action; +6 the previous frame's.
    pub action: i16,
    pub last_action: i16,
    /// +8: the queued next action.
    pub next: i16,
    /// +0xA: frames in the current mode.
    pub timer: i16,
    /// +0xC: flags (1: ended, 2: ends next step, 4: no blend).
    pub flags: i16,
    /// +0x10: the frame; +0x14 the previous frame's; +0x18 the frame the
    /// next action starts at.
    pub frame: f32,
    pub last_frame: f32,
    pub start_frame: f32,
    /// What +0x1C and +0x20 point at (speed scales for modes 0xA-0xC).
    pub scale_a: f32,
    pub scale_b: f32,
    /// Whether +0x28 (the blend buffer) is set.
    pub has_blend: bool,
}

/// The blend buffer work the renderer does (not ported here).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendEvent {
    /// Mode 0: MAlloc(0xC) for the buffer (+4 = 2, +8 = 3, +0xA = 2) and
    /// syCalloc(0x6F, 0x18) for its node records.
    Create,
    /// fn_8003D678: build the blend from the current pose (action
    /// `from`) toward action `to`.
    Blend { from: i16, to: i16 },
    /// fn_8003C908: free the node records' keys for the requested action.
    FreeKeys { request: i16 },
    /// Modes 0xE/0xF: syFree the buffer's records and the buffer.
    Destroy,
}

/// The tables fn_8003D8D0 reads: the actions, each motion's frame count
/// (NJS_MOTION +4 of the character motion table entry) and each model's
/// table word (+0 of the character model table entry, compared to decide
/// whether two actions can blend).
pub struct MotionTables<'a> {
    pub actions: &'a [Action],
    pub frames: &'a dyn Fn(i16) -> u32,
    pub model_key: &'a dyn Fn(i16) -> u32,
}

const ONE: f32 = 1.0; // r2-0x75C8
const ZERO: f32 = 0.0; // r2-0x75F8
const TENTH: f32 = f32::from_bits(0x3dcc_cccd); // r2-0x75A8

impl MotionTables<'_> {
    fn act(&self, i: i32) -> Action {
        self.actions[(i & 0xFFFF) as usize]
    }

    fn act_s(&self, i: i16) -> Action {
        self.actions[i as u16 as usize]
    }

    fn nf(&self, i: i32) -> f32 {
        (self.frames)(self.act(i).motion) as f32
    }
}

/// fn_8003D720 and the same tail in modes 1, 5 and 9: where the frame
/// starts after a mode change.
fn start_after(w: &mut MotionWork, t: &MotionTables) {
    let m = w.mode;
    if m == 7 || m == 8 {
        w.frame = t.nf(w.action as i32) - ONE;
    } else if m == 3 || ((m as i32 - 0xA) & 0xFFFF) <= 1 || m == 0xD {
        w.frame = w.start_frame;
        w.start_frame = ZERO;
    } else {
        w.frame = ZERO;
    }
    w.timer = 0;
}

/// The "action ended, go to the next" step of modes 4, 7 and 0xC.
fn follow(w: &mut MotionWork, t: &MotionTables, req: i32) {
    w.request = t.act(req).next & 0xFF;
    w.action = w.request;
    w.mode = t.act_s(w.action).mode;
    w.next = t.act_s(w.request).next & 0xFF;
    w.flags |= 1;
    start_after(w, t);
}

fn wrap_down(w: &mut MotionWork, nf: f32, minus_one: bool) {
    let top = if minus_one { nf - ONE } else { nf };
    while top <= w.frame {
        w.frame -= top;
    }
    while ZERO > w.frame {
        w.frame += nf;
    }
}

/// Mode 1's blend check: blend into `to` from `from` unless blending is
/// off, `to` blends at full speed (1.0) or the two use different models.
fn blend_in(
    w: &mut MotionWork,
    t: &MotionTables,
    no_blend: bool,
    to: i32,
    from: i32,
    ev: &mut Vec<BlendEvent>,
) -> bool {
    let a = t.act(to);
    let blends = !no_blend
        && ONE != a.blend
        && (t.model_key)(t.act(from).model) == (t.model_key)(a.model);
    if blends {
        ev.push(BlendEvent::Blend {
            from: w.action,
            to: w.request,
        });
    }
    blends
}

/// fn_8003D8D0(motion work).
pub fn motion_step(w: &mut MotionWork, t: &MotionTables) -> Vec<BlendEvent> {
    let mut ev = vec![];
    let mut req = w.request as u16 as i32;
    let no_blend = w.flags & 4 != 0;
    w.last_action = w.action;
    w.last_frame = w.frame;
    w.flags = 0;
    if w.action as i32 != req && w.mode != 2 && w.mode != 0 && w.mode != 0xF {
        w.mode = 1;
        w.timer = 0;
        let cur = w.action as i32;
        if t.act(req).mode == 0xB && t.act(cur).mode == 0xB {
            let nc = t.nf(cur);
            let nr = t.nf(req);
            let f = (nr * (nc - w.frame)) / nc;
            w.start_frame = fmadds(TENTH, nr, f);
            if w.start_frame > nr || w.start_frame == nr {
                w.start_frame -= nr;
            }
        }
    } else {
        w.timer = w.timer.wrapping_add(1);
    }
    match w.mode {
        0 => {
            w.frame = ZERO;
            w.action = req as i16;
            w.mode = 1;
            w.start_frame = ZERO;
            w.frame = ZERO;
            w.has_blend = true;
            ev.push(BlendEvent::Create);
        }
        1 => {
            let blends = blend_in(w, t, no_blend, req, w.action as i32, &mut ev);
            w.frame = if blends { ZERO } else { ONE };
            w.action = req as i16;
            w.mode = 2;
            mode2(w, t, &mut ev);
        }
        2 => mode2(w, t, &mut ev),
        3 => {
            w.frame += t.act(req).speed;
            let nf = t.nf(req);
            wrap_down(w, nf, true);
        }
        0xA | 0xB => {
            let s = if w.mode == 0xA { w.scale_a } else { w.scale_b };
            w.frame = fmadds(s, t.act(req).speed, w.frame);
            let nf = t.nf(req);
            wrap_down(w, nf, false);
        }
        0xC => {
            w.frame += w.scale_b;
            if t.nf(req) - ONE <= w.frame {
                follow(w, t, req);
            }
        }
        4 => {
            w.frame += t.act(req).speed;
            let top = t.nf(req) - ONE;
            if top <= w.frame {
                follow(w, t, req);
            } else if top <= w.frame + t.act(req).speed {
                w.flags |= 2;
            }
        }
        5 | 9 => {
            let nine = w.mode == 9;
            w.frame += t.act(req).speed;
            if !(t.nf(req) - ONE <= w.frame) {
                return ev;
            }
            if !nine {
                w.next = t.act(req).next;
            }
            w.timer = 0;
            let n = w.next as i32;
            let a = t.act(n);
            let blends =
                ONE != a.blend && (t.model_key)(t.act(req).model) == (t.model_key)(a.model);
            if blends {
                w.request = (n & 0xFF) as i16;
                req = n;
                ev.push(BlendEvent::Blend {
                    from: w.action,
                    to: w.request,
                });
                w.frame = ZERO;
            } else {
                w.frame = ONE;
            }
            w.action = req as i16;
            let a = t.act(req);
            w.next = a.next & 0xFF;
            w.mode = 2;
            w.frame += a.blend;
            if w.frame >= ONE {
                w.mode = a.mode;
                ev.push(BlendEvent::FreeKeys { request: w.request });
                start_after(w, t);
            }
            if nine {
                w.flags |= 1;
            }
        }
        7 => {
            w.frame -= t.act(req).speed;
            if ZERO > w.frame {
                follow(w, t, req);
            }
        }
        6 => {
            w.frame += t.act(req).speed;
            let top = t.nf(req) - ONE;
            if top <= w.frame {
                w.frame = top;
                w.flags |= 1;
            }
        }
        8 => {
            w.frame -= t.act(req).speed;
            if ZERO > w.frame {
                w.frame = ZERO;
                w.flags |= 1;
            }
        }
        0xD => {
            let nf = t.nf(req);
            wrap_down(w, nf, false);
        }
        _ => {
            ev.push(BlendEvent::FreeKeys { request: w.request });
            if w.has_blend {
                ev.push(BlendEvent::Destroy);
                w.has_blend = false;
            }
        }
    }
    ev
}

/// Mode 2: blend toward the current action, then enter its mode.
fn mode2(w: &mut MotionWork, t: &MotionTables, ev: &mut Vec<BlendEvent>) {
    let a = t.act_s(w.action);
    w.frame += a.blend;
    if w.frame >= ONE {
        w.mode = a.mode;
        ev.push(BlendEvent::FreeKeys { request: w.request });
        start_after(w, t);
    }
}
