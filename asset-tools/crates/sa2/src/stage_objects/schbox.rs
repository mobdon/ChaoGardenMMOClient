//! SCHBOX (object list row 14): the treasure-hunt hint monitor ("search
//! box"). A player stands by it facing it and presses the action button; it
//! shows the current hint text (fn_8008C6C0), remembers it for the pause
//! menu, rises for a second and disappears.
//!
//! stg42D.rel section 1:
//! * 0x1BFB4, the object-list function ([`setup`]): it removes itself in the
//!   modes where `0x801CC168 +0x23` is 1 or 2, allocates the 0x28-byte work
//!   block, picks its display (0x1CF3C when the stage word at section 6
//!   +0x6B8 is set, else 0x1CB18 with 0x1CCD4 as the late display), starts
//!   the pause hint task (0x1BA38) and shares a collision
//!   (`CCL_InitShare`, CollisionData row at section 5 +0x1E358) whose
//!   `colli_range` (+8) becomes 8.0;
//! * 0x1C1B0, the exec ([`exec`]); 0x1C164, the destructor, the same code as
//!   HINTBOX's 0x117C0 ([`super::hintbox::destroy`]);
//! * the "SearchBoxManExec" task (0x1B92C starts it, exec 0x1B8F0,
//!   destructor 0x1B8D0, its task at section 6 +0x6BC): it counts frames
//!   after a hint and ends after 0x12D;
//! * 0x1B9C8 ([`hint_blocking`]): whether boxes are dimmed and closed;
//! * the "PauseHintExec" task (0x1BA38 starts it, its task at section 6
//!   +0x6C4): exec 0x1BEAC and destructor 0x1BED8 close its message; its
//!   display 0x1BAC0 ([`pause_display`]) runs while the game is paused and
//!   cycles through the remembered hints (section 5 +0x1E388, three words)
//!   every 180 frames, with the pad's left/right (0x20000 / 0x10000) going
//!   back or forward.
//!
//! The three displays 0x1CB18, 0x1CCD4 and 0x1CF3C and the shadow draw
//! 0x1BF34 are rendering only (they call the stage module's own helpers at
//! section 1 +0x6F28 and +0x5C84 and Ninja's drawing) and are not ported.
//!
//! The facing test of mode 1 builds the player's matrix as compiled
//! (`njUnitMatrix`, `njTranslateEx(ptwp->pos)`, `njRotateZ`, `njRotateX`,
//! `njRotateY` by `0x8000 - ang.y`, `njInvertMatrix`, then `njCalcPoint` of
//! the box); the player is offered the box only when the box lies on the -X
//! side of that frame. `njTranslateEx` and `PSMTXInverse` are private in
//! `land_nodes` / `land_contact` (owned elsewhere), so this file carries a
//! copy of each, checked against main.dol by tests/so_hintbox.rs.
//!
//! Float tests follow the compiled `fcmpo` branches: `bge`/`ble` are taken
//! for an unordered compare, so a NaN distance is out of reach and a NaN
//! fade or alpha is left as it is.
use super::hintbox::{
    angle_to, destroy, snap, turn, BoxHost, BoxWork, TaskWk, Text, ACTION_HINT, BOB, FADE_STEP,
    SE_HINT, SPIN,
};
use super::host::V3;
use crate::land_contact::invert;
use crate::land_nodes::translate;
use crate::ninja_matrix::{mult_vec, rotate, Axis, IDENTITY};
use crate::ninja_trig::nj_sin;
use crate::player_range::{distance, vector_length};

/// The routines this file ports (stg42D.rel section 1).
pub const ROUTINES: &[super::Routine] = &[
    super::Routine {
        name: "SearchBoxMan destructor",
        stg42d: 0x1b8d0,
        len: 0x20,
        masked_sha256: "e3f2cba51105ba2fca4fad23081ba79fa7190b3c290d0b57e5fa472b4a41fc8c",
    },
    super::Routine {
        name: "SearchBoxMan exec",
        stg42d: 0x1b8f0,
        len: 0x3c,
        masked_sha256: "72ae534771640b61c3fc6daf70757dbc3258017a25a1fbb0e8ce2672c3a62ff0",
    },
    super::Routine {
        name: "SearchBoxMan start",
        stg42d: 0x1b92c,
        len: 0x9c,
        masked_sha256: "1566093e15397bd848989e36ac5d0e81191f64f3802499732628f2a0e005eb42",
    },
    super::Routine {
        name: "SCHBOX hint blocking",
        stg42d: 0x1b9c8,
        len: 0x70,
        masked_sha256: "d95d091641421c004f52edfbf5abb1745d81e7c83a13a509cfafce39c0fe6e3f",
    },
    super::Routine {
        name: "PauseHint start",
        stg42d: 0x1ba38,
        len: 0x88,
        masked_sha256: "cc2aa700aaadf041aade474bbdfc358b28d34dc2b8a73b94abb45a2b328f1fe6",
    },
    super::Routine {
        name: "PauseHint display",
        stg42d: 0x1bac0,
        len: 0x3ec,
        masked_sha256: "c2818af1bf52ccbeff07988bfa50e84fe086e8bbadceea32a1aab96bb5034cc3",
    },
    super::Routine {
        name: "PauseHint exec",
        stg42d: 0x1beac,
        len: 0x2c,
        masked_sha256: "91359348ba67ae418edc173a4f39251819abafa55d97988d9ec31cc1521f2864",
    },
    super::Routine {
        name: "PauseHint destructor",
        stg42d: 0x1bed8,
        len: 0x5c,
        masked_sha256: "e8d72018ee25bf292575039f4d0d895ae158bf86e2ce09ff73e9e157ebd0d577",
    },
    super::Routine {
        name: "SCHBOX set-up (object list)",
        stg42d: 0x1bfb4,
        len: 0x1b0,
        masked_sha256: "629c3bc78ff76aba949f42b0562aa1042a8861e80521c931c28a764a235ff95c",
    },
    super::Routine {
        name: "SCHBOX destructor (same code as HINTBOX's)",
        stg42d: 0x1c164,
        len: 0x4c,
        masked_sha256: "87561bc9b44341e5e66b6e56de59afcfdb08aa7fbb973183ce5d956c12f5525d",
    },
    super::Routine {
        name: "SCHBOX exec",
        stg42d: 0x1c1b0,
        len: 0x968,
        masked_sha256: "9b26658e4579ef24a739032b3e9762bad44a86d43ce251bf9fd0913659844081",
    },
];

/// Callbacks (stg42D section 1 offsets).
pub const EXEC: u32 = 0x1c1b0;
pub const DESTRUCTOR: u32 = 0x1c164;
pub const DISPLAY: u32 = 0x1cb18;
pub const DISPLAY_LATE: u32 = 0x1ccd4;
pub const DISPLAY_ALT: u32 = 0x1cf3c;
pub const MAN_EXEC: u32 = 0x1b8f0;
pub const MAN_DESTRUCTOR: u32 = 0x1b8d0;
pub const PAUSE_EXEC: u32 = 0x1beac;
pub const PAUSE_DISPLAY: u32 = 0x1bac0;
pub const PAUSE_DESTRUCTOR: u32 = 0x1bed8;
/// The task names (stg42D section 5 +0x1E394, +0x1E3A8).
pub const MAN_NAME: &str = "SearchBoxManExec";
pub const PAUSE_NAME: &str = "PauseHintExec";

/// syCalloc(1, 0x28).
pub const WORK_SIZE: u32 = 0x28;
/// `cwp->colli_range` after the set-up (section 4 +0x964).
pub const COLLI_RANGE: f32 = 8.0;
/// Use distance (section 4 +0x974).
pub const RADIUS: f32 = 50.0;
/// Dimmed alpha and its step (section 4 +0x950, +0x980).
pub const ALPHA_DIM: f32 = 0.4;
pub const ALPHA_STEP: f32 = 0.09;
/// Push-out radius in sub-mode 6 and the smallest distance pushed
/// (section 4 +0x988, +0x984).
pub const PUSH_RADIUS: f32 = 12.0;
pub const PUSH_MIN: f32 = 0.01;
/// Message time (fn_800E0118's third argument).
pub const MESSAGE_TIME: u32 = 0x78;
/// Frames the box rises after a hint before it removes itself.
pub const RISE_FRAMES: u16 = 0x3c;
/// Turn per frame while rising (BAMS).
pub const RISE_SPIN: i32 = 0x111;
/// Frames SearchBoxMan lives.
pub const MAN_FRAMES: u16 = 0x12d;
/// Frames per remembered hint in the pause menu.
pub const PAUSE_CYCLE: i32 = 180;

/// The SearchBoxMan task (`twp +6` is its frame count).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ManTask {
    pub task: u32,
    pub wtimer: u16,
}

/// The PauseHint task (`twp +6` is its frame count, `task +0x38` its
/// message).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PauseTask {
    pub task: u32,
    pub wtimer: u16,
    pub message: u32,
}

/// The stage module's own state the boxes share.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SearchShared {
    /// Section 5 +0x1E388: the hints shown so far, by hint level.
    pub texts: [Option<Text>; 3],
    /// Section 6 +0x6BC.
    pub man: Option<ManTask>,
    /// Section 6 +0x6C4.
    pub pause: Option<PauseTask>,
    /// Section 6 +0x6C0: set when the last hint was level 2 or above.
    pub last_hint_high: i32,
    /// Section 6 +0x6B8: selects the alternative display.
    pub alt_display: i32,
}

/// A pad as `lbl_801E55D8[port]` points to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pad {
    /// +8: buttons held.
    pub on: u32,
    /// +0x10: buttons pressed this frame.
    pub press: u32,
}

/// What SCHBOX asks of the game beyond [`BoxHost`].
pub trait SearchHost: BoxHost {
    /// `0x801CC168 +0x23`.
    fn play_mode(&mut self) -> i8;
    /// `0x801CC168 +0x17`: which pad drives the pause menu (1 or 2).
    fn pause_pad(&mut self) -> i8;
    /// `0x801CC168 +0x38`: the players sub-mode 6 applies to (bit per
    /// player).
    fn player_mask(&mut self) -> i8;
    /// Whether the task came from a SET entry (`task +0x30`).
    fn has_set_entry(&mut self) -> bool;
    /// DeadOut(task) (0x800653B0).
    fn dead_out(&mut self);
    /// FreeTask(task) (0x80071AE0).
    fn free_task(&mut self);
    /// CCL_InitShare(task, row, count, list) (0x8000F7D8); returns whether
    /// `twp->cwp` is set afterwards.
    fn collision_init_share(&mut self, row: u32, count: u32, list: u32) -> bool;
    /// Write `twp->cwp +8` (colli_range).
    fn set_colli_range(&mut self, r: f32);
    /// CreateElementalTask(flags, level, exec, name) (0x80071E0C); `exec`
    /// is a stg42D section-1 offset. Returns the task, `None` for null.
    fn create_task(&mut self, flags: u32, level: u32, exec: u32, name: &'static str)
        -> Option<u32>;
    /// `lbl_803ADA88 +3` (signed), `None` when the pointer is null.
    fn hunt_level(&mut self) -> Option<i8>;
    /// fn_8008C6C0: the current hint text, 0 for none.
    fn hint_text(&mut self) -> u32;
    /// fn_8001CD40: the game state (0x11 while paused).
    fn game_state(&mut self) -> i16;
    /// `lbl_803ADAD0`.
    fn pause_hint_lock(&mut self) -> i32;
    /// `lbl_801E55D8[port]`.
    fn pad(&mut self, port: usize) -> Pad;
    /// fn_800399BC(player, x, y, z): move a player.
    fn set_player_pos(&mut self, i: u32, p: V3);
}

/// The box's X in the player's frame (mode 1's facing test).
pub fn player_frame_x(player_pos: V3, player_ang: [i32; 3], pos: V3) -> f32 {
    let mut m = IDENTITY;
    translate(&mut m, player_pos);
    rotate(&mut m, Axis::Z, player_ang[2]);
    rotate(&mut m, Axis::X, player_ang[0]);
    rotate(&mut m, Axis::Y, 0x8000i32.wrapping_sub(player_ang[1]));
    invert(&mut m);
    mult_vec(&m, pos)[0]
}

/// 0x1B9C8: nonzero while SearchBoxMan is counting (its count), or when the
/// last hint was level 2 or above and the hunt is still at level 2 or above.
pub fn hint_blocking(shared: &SearchShared, host: &mut impl SearchHost) -> u32 {
    if let Some(m) = shared.man {
        if m.wtimer != 0 {
            return u32::from(m.wtimer);
        }
    }
    if shared.last_hint_high != 0 {
        if let Some(l) = host.hunt_level() {
            if l >= 2 {
                return 1;
            }
        }
    }
    0
}

/// 0x1B92C: start SearchBoxMan if it isn't running, and (re)start its count.
pub fn man_start(shared: &mut SearchShared, host: &mut impl SearchHost) {
    if shared.man.is_none() {
        if let Some(task) = host.create_task(2, 3, MAN_EXEC, MAN_NAME) {
            shared.man = Some(ManTask { task, wtimer: 0 });
        }
    }
    if let Some(m) = shared.man.as_mut() {
        m.wtimer = 1;
    }
}

/// SearchBoxMan's exec (0x1B8F0).
pub fn man_exec(m: &mut ManTask, host: &mut impl SearchHost) {
    m.wtimer = m.wtimer.wrapping_add(1);
    if m.wtimer > MAN_FRAMES {
        host.destroy_task(m.task);
    }
}

/// SearchBoxMan's destructor (0x1B8D0).
pub fn man_destroy(shared: &mut SearchShared, task: u32) {
    if shared.man.is_some_and(|m| m.task == task) {
        shared.man = None;
    }
}

/// 0x1BA38: start PauseHint if it isn't running.
pub fn pause_start(shared: &mut SearchShared, host: &mut impl SearchHost) {
    if shared.pause.is_none() {
        if let Some(task) = host.create_task(2, 2, PAUSE_EXEC, PAUSE_NAME) {
            shared.pause = Some(PauseTask {
                task,
                ..Default::default()
            });
        }
    }
}

/// PauseHint's exec (0x1BEAC): close its message when the game runs.
pub fn pause_exec(p: &PauseTask, host: &mut impl SearchHost) {
    if p.message != 0 {
        host.destroy_task(p.message);
    }
}

/// PauseHint's destructor (0x1BED8).
pub fn pause_destroy(shared: &mut SearchShared, p: &mut PauseTask, host: &mut impl SearchHost) {
    if p.message != 0 {
        host.destroy_task(p.message);
    }
    if shared.pause.is_some_and(|q| q.task == p.task) {
        shared.pause = None;
    }
    p.message = 0;
}

/// PauseHint's display (0x1BAC0), run while paused.
pub fn pause_display(shared: &SearchShared, p: &mut PauseTask, host: &mut impl SearchHost) {
    if host.game_state() != 0x11
        || host.pad(0).on & 0x600 == 0x600
        || host.pause_hint_lock() != 0
        || !host.hunt_level().is_some_and(|l| l > 0)
    {
        return;
    }
    if host.message_active() == 0 && p.message == 0 {
        p.wtimer = 0;
    }
    let w = i32::from(p.wtimer);
    if w % PAUSE_CYCLE == 0 {
        let active = host.message_active();
        if active == 0 || active == p.message {
            let n = shared.texts.iter().take_while(|t| t.is_some()).count();
            if n == 1 {
                if host.message_active() == 0 && p.message == 0 {
                    if let Some(t) = shared.texts[0] {
                        let lang = host.language();
                        p.message = host.message_open(0, t, 0, lang);
                    }
                }
            } else if n > 1 {
                if p.message != 0 {
                    host.destroy_task(p.message);
                }
                let idx = (w / PAUSE_CYCLE) % n as i32;
                if let Some(t) = shared.texts.get(idx as usize).copied().flatten() {
                    let lang = host.language();
                    p.message = host.message_open(0, t, 0, lang);
                }
            }
        }
    }
    let port = host.pause_pad();
    let pad = match port {
        1 => Some(host.pad(0)),
        2 => Some(host.pad(1)),
        _ => None,
    };
    let Some(pad) = pad else {
        p.wtimer = p.wtimer.wrapping_add(1);
        return;
    };
    if pad.on & 0x30000 == 0x30000 {
        return;
    }
    let page = i32::from(p.wtimer) / PAUSE_CYCLE;
    if pad.press & 0x30000 == 0x20000 {
        p.wtimer = ((page - 1) * PAUSE_CYCLE) as u16;
        return;
    }
    if pad.press & 0x30000 == 0x10000 {
        p.wtimer = ((page + 1) * PAUSE_CYCLE) as u16;
        return;
    }
    if pad.on & 0x30000 != 0 {
        return;
    }
    p.wtimer = p.wtimer.wrapping_add(1);
}

/// The object-list function (0x1BFB4). `row` is the stage's CollisionData
/// row (stg42D section 5 +0x1E358). Returns the work block and whether the
/// alternative display (0x1CF3C) was chosen, or `None` when the box removed
/// itself, is out of range or its allocation failed.
pub fn setup(
    t: &mut TaskWk,
    shared: &mut SearchShared,
    row: u32,
    host: &mut impl SearchHost,
) -> Option<(BoxWork, bool)> {
    let mode = host.play_mode();
    if mode == 1 || mode == 2 {
        if host.has_set_entry() {
            host.dead_out();
        } else {
            host.free_task();
        }
        return None;
    }
    if host.range_out() || !host.alloc_work(WORK_SIZE) {
        return None;
    }
    let alt = shared.alt_display != 0;
    t.mode = 0;
    t.smode = 0;
    t.wtimer = 0;
    pause_start(shared, host);
    if host.collision_init_share(row, 1, 4) {
        host.set_colli_range(COLLI_RANGE);
    }
    let w = BoxWork {
        alpha: 1.0,
        fade: 0.0,
        ang_y: t.ang[1],
        pos: t.pos,
        ..Default::default()
    };
    Some((w, alt))
}

/// The destructor (0x1C164): the same code as HINTBOX's 0x117C0.
pub fn destroy_box(work: &mut Option<BoxWork>, host: &mut impl SearchHost) {
    destroy(work, host);
}

/// Mode 1: the player (1 or 2) that may use the box, or 0. Offers the hint
/// action to a player standing by it facing it.
fn user(t: &TaskWk, host: &mut impl SearchHost) -> i32 {
    // r21 is never set when the first player slot is empty; the game always
    // has player 1.
    let mut r = 0;
    for i in 0..2usize {
        if !host.player_pwp_present(i) {
            break;
        }
        let p = host.player_twp(i).unwrap_or_default();
        // `fcmpo; bge`: a NaN distance counts as out of reach.
        r = if distance(t.pos, p.pos) < RADIUS {
            i as i32 + 1
        } else {
            0
        };
        if r != 0 && t.smode == 6 && i32::from(host.player_mask()) & (1 << (r - 1)) == 0 {
            r = 0;
        }
        if r != 0 {
            let k = (r - 1) as usize;
            if host.player_action(k) != ACTION_HINT && host.message_active() == 0 {
                let q = host.player_twp(k).unwrap_or_default();
                if player_frame_x(q.pos, q.ang, t.pos) < 0.0 {
                    host.offer_player_action(k, ACTION_HINT);
                }
                r = 0;
            }
        }
        if r != 0 {
            break;
        }
    }
    r
}

/// One exec frame (0x1C1B0).
pub fn exec(
    t: &mut TaskWk,
    w: &mut BoxWork,
    shared: &mut SearchShared,
    host: &mut impl SearchHost,
) {
    if t.wtimer == 0 && host.range_out() {
        return;
    }
    let frame = snap(t, w, host);
    w.spin = (frame << 9) as i32;
    w.bob = BOB * nj_sin(w.phase);
    match t.mode {
        0 => {
            let f = w.fade;
            if f > 0.0 {
                w.fade = f - FADE_STEP;
                if w.fade < 0.0 {
                    w.fade = 0.0;
                }
            } else if turn(&mut w.ang_y, t.ang[1]) == 0 {
                t.mode = 1;
            }
        }
        1 => {
            let r = user(t, host);
            if hint_blocking(shared, host) == 0
                && r > 0
                && host.message_active() == 0
                && w.message == 0
            {
                let i = r - 1;
                host.set_player_action(i as usize, 0);
                t.player = i as u8;
                t.mode = 2;
            }
        }
        2 => {
            w.fade += FADE_STEP;
            if w.fade >= 1.0 {
                let level = host.hunt_level();
                let k = level.map_or(3, i32::from).clamp(0, 2) as usize;
                shared.last_hint_high = i32::from(level.is_some_and(|l| l >= 2));
                let ptr = host.hint_text();
                w.fade = 1.0;
                t.mode = 3;
                let text = if ptr == 0 {
                    Text::NoText
                } else {
                    Text::Ptr(ptr)
                };
                shared.texts[k] = Some(text);
                for e in shared.texts.iter_mut().skip(k + 1) {
                    *e = None;
                }
                let lang = host.language();
                w.message = host.message_open(1, text, MESSAGE_TIME, lang);
                host.sound(SE_HINT);
            }
            let p = host.player_twp(usize::from(t.player)).unwrap_or_default();
            let target = angle_to(p.pos, t.pos);
            w.phase = w.phase.wrapping_add(SPIN);
            turn(&mut w.ang_y, target);
        }
        3 => {
            if w.message == 0 {
                t.mode = 0;
                man_start(shared, host);
                t.wtimer = 1;
            } else {
                let p = host.player_twp(usize::from(t.player)).unwrap_or_default();
                turn(&mut w.ang_y, angle_to(p.pos, t.pos));
                w.phase = w.phase.wrapping_add(SPIN);
            }
        }
        _ => {}
    }
    // `fcmpo` then `ble`/`bge`: a NaN alpha is left alone.
    if w.message == 0 && hint_blocking(shared, host) != 0 {
        if w.alpha > ALPHA_DIM {
            w.alpha -= ALPHA_STEP;
            if w.alpha < ALPHA_DIM {
                w.alpha = ALPHA_DIM;
            }
        }
    } else if w.alpha < 1.0 {
        w.alpha += ALPHA_STEP;
        if w.alpha > 1.0 {
            w.alpha = 1.0;
        }
    }
    if t.wtimer != 0 {
        t.wtimer = t.wtimer.wrapping_add(1);
        if t.wtimer > RISE_FRAMES {
            if host.has_set_entry() {
                host.dead_out();
            } else {
                host.free_task();
            }
            return;
        }
        t.pos[1] += 1.0;
        w.ang_y = w.ang_y.wrapping_add(RISE_SPIN);
    }
    if t.smode != 6 {
        host.collision_entry();
        return;
    }
    for i in 0..2usize {
        let Some(p) = host.player_twp(i) else {
            break;
        };
        if i32::from(host.player_mask()) & (1 << i) == 0 {
            continue;
        }
        let mut v = [
            p.pos[0] - t.pos[0],
            p.pos[1] - t.pos[1],
            p.pos[2] - t.pos[2],
        ];
        let l = vector_length(v);
        if l > PUSH_MIN && l < PUSH_RADIUS {
            let s = PUSH_RADIUS / l;
            v = v.map(|c| c * s);
            host.set_player_pos(
                i as u32,
                [v[0] + t.pos[0], v[1] + t.pos[1], v[2] + t.pos[2]],
            );
        }
    }
}
