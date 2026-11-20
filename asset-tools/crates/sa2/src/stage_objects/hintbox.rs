//! HINTBOX (object list row 15): the floating hint monitor that opens a
//! hint message when a player stands by it and presses the action button.
//!
//! stg42D.rel section 1: the object-list function 0x11694 (set-up), its
//! exec 0x1180C and destructor 0x117C0. The display 0x11DF0, its shadow
//! draw 0x11608 and the vertex-colour fill 0x11D64 it uses are rendering
//! only and are not ported here.
//!
//! The exec calls the shared `__fabsf` leaf at section 1 +0x84 twice
//! (0x11A40, 0x11A6C). The stage linker resolved those two `bl`s inside the
//! module, so no relocation covers them and their displacement depends on
//! where the exec sits; that is the only reason the whole routine matches
//! no other REL byte for byte. With those two displacements masked as well
//! it is identical in 52 other stage RELs, and [`ROUTINES`] lists it as the
//! three pieces around the two branches, each found as is.
//!
//! The types here ([`TaskWk`], [`BoxWork`], [`BoxHost`], [`Text`]) are
//! shared with SCHBOX (`schbox.rs`), whose code is a near copy.
use super::host::{ObjectHost, V3};
use crate::msl_math::atan2f;
use crate::ninja_trig::{fctiwz, nj_sin};
use crate::player_range::player_in_range;

/// The routines this file ports (stg42D.rel section 1).
pub const ROUTINES: &[super::Routine] = &[
    super::Routine {
        name: "HINTBOX set-up (object list)",
        stg42d: 0x11694,
        len: 0x12c,
        masked_sha256: "5a23631f2ca49aeff14eff77bca5b0538c9076be859e2471bf028a5a24cb1162",
    },
    super::Routine {
        name: "HINTBOX destructor",
        stg42d: 0x117c0,
        len: 0x4c,
        masked_sha256: "87561bc9b44341e5e66b6e56de59afcfdb08aa7fbb973183ce5d956c12f5525d",
    },
    super::Routine {
        name: "HINTBOX exec (to the first __fabsf call)",
        stg42d: 0x1180c,
        len: 0x234,
        masked_sha256: "cae2daa04e2e9e309bcdbe375d465909acb3b053dd6a14e97efd5b402488af00",
    },
    super::Routine {
        name: "HINTBOX exec (between the __fabsf calls)",
        stg42d: 0x11a44,
        len: 0x28,
        masked_sha256: "bb6ac343579f056d8b966f1fc02c0bdffefb9e4126d32b64cf426dbbe5977edc",
    },
    super::Routine {
        name: "HINTBOX exec (after the second __fabsf call)",
        stg42d: 0x11a70,
        len: 0x2f4,
        masked_sha256: "cdd5bceae79fab28972a2b8c7e97ed12b4954f032a37840ebd82b46a24c7fd31",
    },
];

/// Exec, display and destructor the set-up installs (stg42D section 1
/// offsets; task +0x10, +0x14, +0x18).
pub const EXEC: u32 = 0x1180c;
pub const DISPLAY: u32 = 0x11df0;
pub const DESTRUCTOR: u32 = 0x117c0;

/// syCalloc(1, 0x2C) (0x801197FC): the size of [`BoxWork`] for HINTBOX.
pub const WORK_SIZE: u32 = 0x2c;
/// Player radius for EitherPlayerWithinSphere (stg42D section 4 +0x6F8).
pub const RADIUS: f32 = 50.0;
/// Fade step per frame (section 4 +0x6FC).
pub const FADE_STEP: f32 = 0.05;
/// Bob height: 0.7 * njSin(phase) (section 4 +0x6F4).
pub const BOB: f32 = 0.7;
/// GetShadowPos's "no ground" answer (section 4 +0x6F0).
pub const NO_GROUND: f32 = -1_000_000.0;
/// Radians to BAMS, a double (section 4 +0x700).
pub const BAMS_PER_RAD: f64 = 10430.38043493439;
/// The player action the box offers while a player stands by it
/// (`playerpwp +0xD`) and the state it waits for (`+0xE`).
pub const ACTION_HINT: i8 = 0x32;
/// SE_Call id (0x8013, SN_HINTBOX).
pub const SE_HINT: u32 = 0x8013;
/// Message display time in frames (fn_800E0118's third argument).
pub const MESSAGE_TIME: u32 = 0xb4;
/// Frames before the box can be used again.
pub const REUSE_WAIT: u16 = 30;
/// Phase step while the box spins (BAMS per frame).
pub const SPIN: i32 = 0x300;
/// The largest turn per frame (BAMS).
pub const TURN_MAX: i32 = 0x200;

/// The parts of the object's taskwk (`task +0x34`) these objects use.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TaskWk {
    /// +0 (signed byte).
    pub mode: i8,
    /// +1 (signed byte); SCHBOX treats 6 specially.
    pub smode: i8,
    /// +3: the player the box serves.
    pub player: u8,
    /// +6.
    pub wtimer: u16,
    /// +8, +0xC, +0x10 (BAMS).
    pub ang: [i32; 3],
    /// +0x14.
    pub pos: V3,
    /// +0x20 (Scale from the SET entry).
    pub scale: V3,
}

/// The box's own work block (`task +0x38`, syCalloc'd and zeroed).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BoxWork {
    /// +0: model alpha (HINTBOX keeps 1.0; SCHBOX fades it).
    pub alpha: f32,
    /// +4: the "ready" fade from 0 to 1.
    pub fade: f32,
    /// +8: the facing angle (BAMS).
    pub ang_y: i32,
    /// +0xC: the drawn position (Y snapped to the ground).
    pub pos: V3,
    /// +0x18: SCHBOX's spin angle (frame counter << 9).
    pub spin: i32,
    /// +0x1C: the bob phase.
    pub phase: i32,
    /// +0x20: the bob offset.
    pub bob: f32,
    /// +0x24: the message task fn_800E0118 stored, or 0.
    pub message: u32,
    /// +0x28: HINTBOX sets -1 (nothing reads it here; SCHBOX's block ends
    /// before it).
    pub hint: i32,
}

/// A message text handed to fn_800E0118.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Text {
    /// HINTBOX: entry `n` of the stage's hint text table (the pointer at
    /// stg42D section 6 +0x680).
    Hint(i32),
    /// HINTBOX's built-in text (stg42D section 5 +0x10EAC).
    DefaultHint,
    /// SCHBOX: a text fn_8008C6C0 returned.
    Ptr(u32),
    /// SCHBOX's "NoText" (stg42D section 5 +0x1E3B8), used when
    /// fn_8008C6C0 returns null.
    NoText,
}

/// A player task as the boxes read it from `playertwp` (0x801E7768).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PlayerTwp {
    /// +8, +0xC, +0x10.
    pub ang: [i32; 3],
    /// +0x14.
    pub pos: V3,
}

/// What the boxes ask of the game. Each method stands for one main.dol
/// routine or global (GSNE8P addresses).
pub trait BoxHost: ObjectHost {
    /// `0x801CC168 +0x7C`: the frame counter.
    fn frame_counter(&mut self) -> u32;
    /// `0x801CC168 +0x11`: the text language.
    fn language(&mut self) -> i8;
    /// GetShadowPos(x, y, z, &out) (0x80067AF0): the ground height.
    fn shadow_pos(&mut self, p: V3) -> f32;
    /// The positions EitherPlayerWithinSphere (0x800398D8) reads for each
    /// `playertwp` slot (see `player_range`), `None` for an empty slot.
    fn range_players(&mut self) -> [Option<V3>; 2];
    /// `playertwp[i]` (0x801E7768), `None` when the slot is empty.
    fn player_twp(&mut self, i: usize) -> Option<PlayerTwp>;
    /// Whether `playerpwp[i]` (0x801E7728) is set.
    fn player_pwp_present(&mut self, i: usize) -> bool;
    /// `playerpwp[i] +0xE`.
    fn player_action(&mut self, i: usize) -> i8;
    /// Write `playerpwp[i] +0xE`.
    fn set_player_action(&mut self, i: usize, v: i8);
    /// Write `playerpwp[i] +0xD` (the action the player is offered).
    fn offer_player_action(&mut self, i: usize, v: i8);
    /// fn_80022E0C(i): whether player `i` has control.
    fn player_has_control(&mut self, i: u32) -> i8;
    /// fn_80022E24(i): take player `i`'s control away.
    fn control_off(&mut self, i: u32);
    /// fn_80022E64(i): give it back.
    fn control_on(&mut self, i: u32);
    /// fn_800E018C: the message on screen (r13-0x76A0), 0 for none.
    fn message_active(&mut self) -> u32;
    /// fn_800E0118(kind, text, time, language, &slot): open a message;
    /// returns what it stores in the slot.
    fn message_open(&mut self, kind: u32, text: Text, time: u32, language: i8) -> u32;
    /// DestroyTask(task) (0x80071AF0).
    fn destroy_task(&mut self, task: u32);
    /// SE_Call(id, 0, 0, 0) (0x8006B978).
    fn sound(&mut self, id: u32);
    /// fn_8003A54C(player, task, &zero, 1): turn the player to the box.
    fn look_at(&mut self, player: u32);
    /// syCalloc(1, size) (0x801197FC): false when it returns null.
    fn alloc_work(&mut self, size: u32) -> bool;
    /// syFree(work) (0x80119828).
    fn free_work(&mut self);
    /// CCL_Init(task, row, count, list) (0x8000F8B8).
    fn collision_init(&mut self, row: u32, count: u32, list: u32);
    /// `twp->cwp +4` (the collision flags).
    fn collision_flags(&mut self) -> u16;
    /// CCL_Entry(task) (0x8000F4EC).
    fn collision_entry(&mut self);
}

/// `fctiwz` of a double (the turn targets): NaN and out-of-range values
/// give 0x80000000 as the harness's core does.
pub(crate) fn fctiwz_f64(x: f64) -> i32 {
    if x.is_nan() || x >= 2147483648.0 || x < -2147483648.0 {
        i32::MIN
    } else {
        x as i32
    }
}

/// Turn `cur` toward `target` by at most [`TURN_MAX`] (the difference is
/// taken as a 16-bit angle). Returns the step.
pub(crate) fn turn(cur: &mut i32, target: i32) -> i32 {
    let d = i32::from(target.wrapping_sub(*cur) as i16).clamp(-TURN_MAX, TURN_MAX);
    *cur = cur.wrapping_add(d);
    d
}

/// The angle from the box to a player: atan2f(dx, dz) (0x80111168) in BAMS.
pub(crate) fn angle_to(player: V3, pos: V3) -> i32 {
    let a = atan2f(player[0] - pos[0], player[2] - pos[2]);
    fctiwz_f64(BAMS_PER_RAD * f64::from(a))
}

/// The shared per-frame head of both execs: snap the drawn position to the
/// ground (every fourth frame, while the drawn height still equals the SET
/// height). Returns the frame counter.
pub(crate) fn snap(t: &TaskWk, w: &mut BoxWork, host: &mut impl BoxHost) -> u32 {
    let frame = host.frame_counter();
    if w.pos[1] == t.pos[1] && frame & 3 == 0 {
        let y = host.shadow_pos(t.pos);
        if y != NO_GROUND {
            w.pos = [t.pos[0], y, t.pos[2]];
        }
    }
    frame
}

/// The object-list function (0x11694). `row` is the stage's CollisionData
/// row (stg42D section 5 +0x10E78). Returns the work block, or `None` when
/// the box is out of range or the allocation failed.
pub fn setup(t: &mut TaskWk, row: u32, host: &mut impl BoxHost) -> Option<BoxWork> {
    if host.range_out() || !host.alloc_work(WORK_SIZE) {
        return None;
    }
    t.mode = 0;
    t.smode = 0;
    t.wtimer = 0;
    host.collision_init(row, 1, 4);
    Some(BoxWork {
        alpha: 1.0,
        fade: 0.0,
        ang_y: t.ang[1],
        pos: t.pos,
        bob: 0.0,
        phase: 0,
        message: 0,
        hint: -1,
        spin: 0,
    })
}

/// The destructor (0x117C0).
pub fn destroy(work: &mut Option<BoxWork>, host: &mut impl BoxHost) {
    if let Some(w) = work {
        if w.message != 0 {
            host.destroy_task(w.message);
        }
    }
    host.free_work();
    *work = None;
}

/// The hint text: entry `|Scale.x|` of the table when the stage's count
/// (stg42D section 5 +0x10EA8, -1 there) is not negative and the entry
/// exists, else the built-in text.
pub fn text(scale_x: f32, hint_count: i32) -> Text {
    let mut idx = -1;
    if hint_count >= 0 && fctiwz(scale_x.abs()) < hint_count {
        idx = fctiwz(scale_x.abs());
    }
    if idx >= 0 {
        Text::Hint(idx)
    } else {
        Text::DefaultHint
    }
}

/// One exec frame (0x1180C).
pub fn exec(t: &mut TaskWk, w: &mut BoxWork, hint_count: i32, host: &mut impl BoxHost) {
    if host.range_out() {
        return;
    }
    snap(t, w, host);
    w.bob = BOB * nj_sin(w.phase);
    match t.mode {
        0 => t.mode = 1,
        1 => {
            let mut r = player_in_range(&host.range_players(), t.pos, RADIUS) as i32;
            if r != 0 {
                let i = (r - 1) as usize;
                if host.player_action(i) != ACTION_HINT {
                    host.offer_player_action(i, ACTION_HINT);
                    r = 0;
                }
            }
            if t.wtimer == 0 && r > 0 {
                let i = r - 1;
                if host.player_has_control(i as u32) != 0 {
                    host.set_player_action(i as usize, 0);
                    t.player = i as u8;
                    t.mode = 2;
                }
            }
        }
        2 => {
            w.fade += FADE_STEP;
            if host.message_active() == 0 && w.fade >= 1.0 {
                w.fade = 1.0;
                if w.message != 0 {
                    host.destroy_task(w.message);
                }
                let text = text(t.scale[0], hint_count);
                t.mode = 3;
                host.control_off(u32::from(t.player));
                let lang = host.language();
                w.message = host.message_open(1, text, MESSAGE_TIME, lang);
                host.sound(SE_HINT);
            }
            w.phase = w.phase.wrapping_add(SPIN);
        }
        3 => {
            if host.message_active() == 0 {
                host.control_on(u32::from(t.player));
                t.wtimer = REUSE_WAIT;
                t.mode = 0;
            } else {
                w.phase = w.phase.wrapping_add(SPIN);
            }
        }
        _ => {}
    }
    let r = player_in_range(&host.range_players(), t.pos, RADIUS);
    if r > 0 {
        w.fade += FADE_STEP;
        if w.fade >= 1.0 {
            w.fade = 1.0;
            host.look_at(r - 1);
            let p = host.player_twp(usize::from(t.player)).unwrap_or_default();
            turn(&mut w.ang_y, angle_to(p.pos, t.pos));
        }
    } else {
        // `ble`/`bge` after `fcmpo`: a NaN fade turns and stays NaN.
        let f = w.fade;
        if f > 0.0 {
            w.fade = f - FADE_STEP;
            if w.fade < 0.0 {
                w.fade = 0.0;
            }
        } else {
            turn(&mut w.ang_y, t.ang[1]);
        }
    }
    if t.wtimer != 0 && host.collision_flags() & 1 == 0 {
        t.wtimer -= 1;
    }
    host.collision_entry();
}
