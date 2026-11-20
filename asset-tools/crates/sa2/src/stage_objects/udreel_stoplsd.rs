//! UDREEL (object list row 11), the pulley a player hangs from, and STOPLSD
//! (row 12), the invisible collision that stops a light-speed dash.
//!
//! stg42D.rel section 1.
//!
//! STOPLSD: the object-list exec 0xCB8C runs once. Unless
//! `CheckRangeOut(task)` (0x800655F4) reports the task out, the variant
//! (twp +1) is bit 0 of Rotation.z, twp +6 the low half of Rotation.x,
//! Rotation.x becomes 0 and Rotation.y and .z lose their low byte; the
//! variant's CollisionData row (sec5 +0xDEA8, 0x30 bytes per variant) is
//! registered (`CCL_Init(task, row, 1, 4)`, 0x8000F8B8); the first
//! collision info's a, b, c (`cwp->info +0x14..+0x1C`) become Scale, and
//! for variant 0 its word +0x2C the masked Rotation.z; fn_800068E4(cwp)
//! (`CCL_CalcRange`); twp +2 = 0x15; exec 0xCCBC, display 0xCCB4 and
//! destructor 0xCCB8 (both `blr`). The exec calls `CCL_Entry` (0x8000F4EC)
//! after the range check.
//!
//! UDREEL: the object-list exec 0x1EDC8, after the range check, allocates
//! its 0x4C-byte work (`syCalloc(1, 0x4C)`, 0x801197FC; task +0x38) and
//! does nothing more while that fails. Then: display 0x20114 when the
//! stage's word at sec6 +0x6C8 is set, else display 0x1FF48 with the second
//! display (task +0x1C) 0x20088; exec 0x1F0E4, destructor 0x1F094. Scale.x
//! and .y are ordered (swapped when y < x). The rope length (work +0x38)
//! starts at `-(-20 * (1 + Scale.y))` (sec4 +0xA18, +0xA1C), the speed
//! (+0x3C) at 0 (sec4 +0xA20), twp +0 and +1 at 0. It registers the
//! CollisionData row at sec5 +0x20BD0 (`CCL_Init(task, row, 1, 4)`) and
//! copies the 0x38-byte dynamic land object at sec5 +0x20B30 into the work
//! (flags lose bits 0-1, rotation (0, Rotation.y, 0), position the task's)
//! and registers it with fn_800232A4(flags, task, work): flags 1, plus
//! 0x20000000, 0x40000000 or 0x60000000 as `5 + model radius` (sec4 +0xA24;
//! the radius is the object's model +0x24) falls to 200, 100 or 30 and
//! below (sec4 +0xA28, +0xA2C, +0xA30). Unless Rotation.x has bit 0 the
//! reel starts down: speed 0, length `-(-20 * Scale.x)`, state 3. The
//! shadow height (+0x48) starts unknown (-1000000, sec4 +0xA34).
//!
//! The exec 0x1F0E4 skips the range check while a player hangs on (twp +0
//! bit 0); when the byte at 0x801CC168 +0x42 is 1 it clears twp +0 bits 0
//! and 1. While the shadow height is unknown it asks `GetShadowPos`
//! (0x80067AF0) every 16th frame (counter at task +0x4C) below the rope's
//! end, keeping an answer other than -1000000. The hang depth is `8 +
//! length` (sec4 +0xA3C) when the length is above -7 (sec4 +0xA38), else 1;
//! the rope's end is at `-3.5 - hang` (sec4 +0xA40). When twp +0 bit 1 (let
//! go) is set it clears bits 0 and 1, throws the player along `(0, 1.1 +
//! Scale.z, 0)` (sec4 +0xA44) turned by Rotation.y and `Rotation.z +
//! 0x3400` (`njPushMatrixEx`, `njUnitMatrix`, `njRotateY`, `njRotateZ`,
//! `njCalcVector`, `njPopMatrixEx`; the shared `ninja_matrix` port),
//! levels the player (Rotation.x and .z 0, .y `0x8000 - Rotation.y`) and
//! calls `SetVelocityP(slot, v)` (0x8003A3DC). The work's +0x40 becomes the
//! rope's end and +0x44 `hang / 20` (sec4 +0xA48). Then by state (twp +1):
//!
//! * 0: a hanging player starts state 1.
//! * 1: the length moves by the speed, then the speed loses `0.02 *
//!   (length - 20 * (1 + Scale.y))` (sec4 +0xA58); once it is below 0 it
//!   falls by `2.5 * 0.1` (sec5 +0x20C00, +0x20C04) down to -2.5, the
//!   state becomes 2 and bit 2 of twp +0 clears.
//! * 2: with bit 2 (at the bottom) the length moves by the speed and
//!   springs: below -5 (sec4 +0xA4C) the speed turns positive and the
//!   length stays at -5; the speed becomes `0.92 * speed - 0.1 * (length -
//!   20 * Scale.x)` (sec4 +0xA50, +0xA54, with the length before the
//!   clamp); without a player it stops at `-(-20 * Scale.x)` and goes to
//!   state 4 (Rotation.x bit 0) or 3. Without bit 2 it falls (speed loses
//!   0.25 down to -2.5, then the length moves) and sets bit 2 below `-(-20 *
//!   Scale.x)`, with sound 0x101D at the rope's end (`SE_CallV2_Timer(0x101D,
//!   twp, 1, 0, 10, &at)`, 0x8006AFFC).
//! * 3: speed 0, length `-(-20 * Scale.x)`; a hanging player starts state 4.
//! * 4: with bit 3 (at the top) the length moves and springs around `-20 *
//!   (1 + Scale.y)`; without a player it stops there and goes to state 0,
//!   clearing bit 3. Without bit 3 the length moves, then the speed gains
//!   0.25 up to 2.5, bit 3 is set at or above `-(-20 * (1 + Scale.y))`, and
//!   the sound plays.
//! * In states 2 and 4, while a player hangs on and its mode byte (twp +0)
//!   is not 0x33, twp +0 bit 1 is set and fn_8002FB2C(slot, 4, 0xF, 0) runs.
//!
//! After the states: with a model, twp +4 bit 0x100 tells whether a player
//! is within `32 + radius` (sec4 +0xA5C) of the work's position
//! (`_rename_EitherPlayerWithinSphere`, 0x800398D8, the shared
//! `player_range::player_in_range`). Without a hanging player: if the
//! collision reports a hit, the grab wait (twp +3) is above 60, the other
//! collision's id (`cwp +0xA4`, its +0) is 0 and `IsThisTaskPlayer` of its
//! task (+0x9C; 0x80037EDC) gives a slot, the player grabs on: twp +0 bit
//! 0, twp +6 the slot, speed 1.0, `SetInputP(slot, 0x14, 0x4B)` (0x8003A508)
//! and fn_8002FB2C(slot, 5, 0xB, 0). Else the collision info's +0xC is the
//! rope's end less 1, fn_800068E4(cwp), `CCL_Entry`, and the wait counts up
//! to 61 with info attribute bit 0x10 set (cleared after). With a hanging
//! player it puts the player under the rope's end: down by the character's
//! grip height (playerpwp +3 picks sec5 +0x20B68..+0x20B94 through the
//! jump table at sec5 +0x20C08, else 16.5 at sec4 +0xA60), x and z 0.35
//! (sec4 +0xA64) of the way from the rope to the player, through
//! fn_800399BC(slot, at); turns the player by `0.35 * (short)(0x8000 -
//! Rotation.y - player Rotation.y)` (`fctiwz`, then `short`), levels it and
//! zeroes the wait.
//!
//! The destructor 0x1F094 removes the dynamic land object (fn_800231CC,
//! when the work has a model) and frees the work (`syFree`, 0x80119828).
//!
//! The displays (0x1FF48, 0x20088, 0x20114 and the stage module's shadow
//! helper 0x746C) are draw-call sequences; see [`display`],
//! [`display_shadow`] and [`display_alt`].
//!
//! The player calls (velocity, input, actions, position, task lookup) and
//! the engine calls (allocation, sound, shadow probe, dynamic land, draws)
//! are host hooks named after their main.dol routines.
//!
//! 0x1F0E4, 0x20088 and 0x20114 call stage module helpers outside this
//! block (fabsf at 0x84, the shadow helper 0x746C) with a direct `bl`
//! whose displacement the REL does not relocate; their bytes match the
//! other stages only with that displacement masked too
//! (`tests/so_skull.rs` checks it), so they are listed in
//! [`BL_MASKED_ROUTINES`].
use super::host::{Event, ObjectHost, V3};
use super::skull::{Draw, Twp};
use crate::ninja_matrix::{Axis, MatrixStack};
use crate::player_range::player_in_range;

/// The routines this file ports (stg42D.rel section 1).
pub const ROUTINES: &[super::Routine] = &[
    super::Routine {
        name: "STOPLSD load (0xCB8C)",
        stg42d: 0xcb8c,
        len: 0x128,
        masked_sha256: "0244a47aae55bf2e45bbe485741b2e0939f2c3b90ce88825f7ef545df8f038cb",
    },
    super::Routine {
        name: "STOPLSD display (0xCCB4)",
        stg42d: 0xccb4,
        len: 0x4,
        masked_sha256: "f332ea5b5437103cbb6f1508679da89eec9288ad775c96c439a17fccabe3de8e",
    },
    super::Routine {
        name: "STOPLSD destructor (0xCCB8)",
        stg42d: 0xccb8,
        len: 0x4,
        masked_sha256: "f332ea5b5437103cbb6f1508679da89eec9288ad775c96c439a17fccabe3de8e",
    },
    super::Routine {
        name: "STOPLSD exec (0xCCBC)",
        stg42d: 0xccbc,
        len: 0x40,
        masked_sha256: "33a69b488f8fea9534a7d39ddc4ba28d257c600f9854f0236f22138dab103384",
    },
    super::Routine {
        name: "UDREEL load (0x1EDC8)",
        stg42d: 0x1edc8,
        len: 0x2cc,
        masked_sha256: "4a73704452d86bf54c2fdad62375fb2837d36eead765432cc5db0cf5fdc14535",
    },
    super::Routine {
        name: "UDREEL destructor (0x1F094)",
        stg42d: 0x1f094,
        len: 0x50,
        masked_sha256: "2313ac558bfc070f8c2beb6ff24e7bbafbfafcbf415d987579d14afd35b3cbe2",
    },
    super::Routine {
        name: "UDREEL display (0x1FF48)",
        stg42d: 0x1ff48,
        len: 0x140,
        masked_sha256: "b4d7c517522fe8bda8801bee6dcf199c348ca792656c3e9dff2b5954be4741ce",
    },
    super::Routine {
        name: "shadow model helper (0x746C)",
        stg42d: 0x746c,
        len: 0x60,
        masked_sha256: "48ce736dab347f6a974fd777ac5d357d5f812fe49caaef3d1588058f54f4ed3a",
    },
];

/// Ported routines whose bytes call a stage module helper outside the
/// shared block with a direct `bl` the REL does not relocate: their
/// relocation-masked hash is recorded here, and they match the other
/// stages only with every branch displacement masked
/// (`tests/so_skull.rs`). They stay out of [`ROUTINES`] because
/// `super::masked_text` keeps that displacement.
pub const BL_MASKED_ROUTINES: &[super::Routine] = &[
    super::Routine {
        name: "UDREEL exec (0x1F0E4)",
        stg42d: 0x1f0e4,
        len: 0xe64,
        masked_sha256: "a7ae5d789edfc3e172b59d51b36e85ec48dff7ca093d72f5cd39088898f17baf",
    },
    super::Routine {
        name: "UDREEL shadow display (0x20088)",
        stg42d: 0x20088,
        len: 0x8c,
        masked_sha256: "c6adaefb632905e6471af6ac197a432bff32785cf98248132fac5d0a702c70fc",
    },
    super::Routine {
        name: "UDREEL display with the stage's models (0x20114)",
        stg42d: 0x20114,
        len: 0x1fc,
        masked_sha256: "04e89c3a531a373df552c3576be1a7e9969a6d1066e83594a05ee00530a90423",
    },
];

// --- STOPLSD -----------------------------------------------------------

/// stg42D sec5 +0xDEA8: STOPLSD's CollisionData rows (0x30 bytes each).
pub const STOPLSD_ROWS: u32 = 0xdea8;
pub const STOPLSD_EXEC: u32 = 0xccbc;
pub const STOPLSD_DISPLAY: u32 = 0xccb4;
pub const STOPLSD_DEST: u32 = 0xccb8;

/// A STOPLSD task.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StopLsdTask {
    pub twp: Twp,
    /// task +0x10 / +0x14 / +0x18 as stg42D section-1 offsets.
    pub exec: u32,
    pub disp: u32,
    pub dest: u32,
}

/// What STOPLSD does beyond the shared collision events.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StopLsdEvent {
    /// The first collision info's word +0x2C is written.
    InfoWord2c(u32),
}

pub trait StopLsdHost: ObjectHost {
    fn stoplsd(&mut self, e: StopLsdEvent);
}

/// STOPLSD's object-list exec (0xCB8C), run once. `rows` is the section-5
/// offset of the stage's first STOPLSD CollisionData row.
pub fn stoplsd_load(t: &mut StopLsdTask, rows: u32, host: &mut impl StopLsdHost) {
    if host.range_out() {
        return;
    }
    let w = &mut t.twp;
    w.id = (w.ang[2] & 1) as i8;
    w.w6 = w.ang[0] as u16;
    w.ang[0] = 0;
    w.ang[1] &= !0xff;
    w.ang[2] &= !0xff;
    let variant = i32::from(w.id);
    host.event(Event::CollisionInit {
        rows: rows.wrapping_add((variant * 0x30) as u32),
        count: 1,
        list: 4,
    });
    host.event(Event::CollisionSizes(w.scale));
    if w.id == 0 {
        host.stoplsd(StopLsdEvent::InfoWord2c(w.ang[2] as u32));
    }
    host.event(Event::CollisionCalcRange);
    w.b2 = 0x15;
    t.exec = STOPLSD_EXEC;
    t.disp = STOPLSD_DISPLAY;
    t.dest = STOPLSD_DEST;
}

/// STOPLSD's exec (0xCCBC).
pub fn stoplsd_exec(host: &mut impl StopLsdHost) {
    if !host.range_out() {
        host.event(Event::CollisionEntry);
    }
}

// --- UDREEL ------------------------------------------------------------

/// stg42D sec5 +0x20BD0: UDREEL's CollisionData row.
pub const UDREEL_ROW: u32 = 0x20bd0;
pub const UDREEL_EXEC: u32 = 0x1f0e4;
pub const UDREEL_DEST: u32 = 0x1f094;
pub const UDREEL_DISPLAY: u32 = 0x1ff48;
pub const UDREEL_DISPLAY_SHADOW: u32 = 0x20088;
pub const UDREEL_DISPLAY_ALT: u32 = 0x20114;

const DEPTH: f32 = -20.0; // sec4 +0xA18
const ONE: f32 = 1.0; // sec4 +0xA1C
const ZERO: f32 = 0.0; // sec4 +0xA20
const RADIUS_PAD: f32 = 5.0; // sec4 +0xA24
const RADIUS_BANDS: [f32; 3] = [200.0, 100.0, 30.0]; // sec4 +0xA28, +0xA2C, +0xA30
/// sec4 +0xA34: the shadow height before one is found.
pub const NO_SHADOW: f32 = -1_000_000.0;
const HANG_FROM: f32 = -7.0; // sec4 +0xA38
const HANG_ADD: f32 = 8.0; // sec4 +0xA3C
const END: f32 = -3.5; // sec4 +0xA40
const THROW: f32 = f32::from_bits(0x3f8c_cccd); // sec4 +0xA44, 1.1
const ROPE_SCALE: f32 = 20.0; // sec4 +0xA48
const FLOOR: f32 = -5.0; // sec4 +0xA4C
const KEEP: f32 = f32::from_bits(0x3f6b_851f); // sec4 +0xA50, 0.92
const PULL: f32 = f32::from_bits(0x3dcc_cccd); // sec4 +0xA54, 0.1
const LIFT_PULL: f32 = f32::from_bits(0x3ca3_d70a); // sec4 +0xA58, 0.02
const NEAR_PAD: f32 = 32.0; // sec4 +0xA5C
const GRIP_DEFAULT: f32 = 16.5; // sec4 +0xA60
const FOLLOW: f32 = f32::from_bits(0x3eb3_3333); // sec4 +0xA64, 0.35
const SHADOW_LIFT: f32 = 0.5; // sec4 +0xA70
const SHADOW_SIZE: f32 = f32::from_bits(0x4066_6666); // sec4 +0xA74, 3.6
const HELPER_SIZE: f32 = f32::from_bits(0x3e4c_cccd); // sec4 +0x2D8, 0.2
const HELPER_ONE: f32 = 1.0; // sec4 +0x1EC
/// sec5 +0x20C00 / +0x20C04: the fastest the reel moves and its gravity
/// factor (stage data, not constants of the code).
const TOP_SPEED: f32 = 2.5;
const GRAVITY: f32 = f32::from_bits(0x3dcc_cccd);
/// The sound the reel plays while it runs free.
pub const REEL_SOUND: u32 = 0x101d;

/// The grip height under the rope's end by character (playerpwp +3): the
/// jump table at stg42D sec5 +0x20C08 picks one of sec5 +0x20B68..+0x20B94;
/// characters 2, 3, 9, 10, 11 and anything outside 0..=16 use 16.5.
pub fn grip_height(character: i8) -> f32 {
    let bits = match character as i32 as u32 {
        0 => 0x4182_6666,  // +0x20B68, 16.3
        1 => 0x4182_6666,  // +0x20B6C
        4 => 0x418d_999a,  // +0x20B70, 17.7
        5 => 0x4176_6666,  // +0x20B74, 15.4
        6 => 0x420c_0000,  // +0x20B7C, 35.0
        7 => 0x420b_3333,  // +0x20B78, 34.8
        8 => 0x4181_999a,  // +0x20B80, 16.2
        12 => 0x4186_6666, // +0x20B84, 16.8
        13 => 0x420a_0000, // +0x20B94, 34.5
        14 => 0x420b_999a, // +0x20B90, 34.9
        15 => 0x417c_cccd, // +0x20B88, 15.8
        16 => 0x418c_0000, // +0x20B8C, 17.5
        _ => return GRIP_DEFAULT,
    };
    f32::from_bits(bits)
}

/// The reel's work (task +0x38, 0x4C bytes).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ReelWork {
    /// +0..+0x38: the dynamic land object (an NJS_OBJECT copy: flags,
    /// model, position, rotation, scale, child, sibling, one more word).
    pub obj: [u32; 14],
    /// +0x38: rope length.
    pub len: f32,
    /// +0x3C: speed.
    pub speed: f32,
    /// +0x40: the rope's end (display).
    pub end: f32,
    /// +0x44: the rope's scale (display).
    pub rope: f32,
    /// +0x48: the shadow height.
    pub shadow: f32,
}

impl ReelWork {
    fn position(&self) -> V3 {
        [2, 3, 4].map(|k| f32::from_bits(self.obj[k]))
    }
}

/// A UDREEL task.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ReelTask {
    pub twp: Twp,
    /// task +0x10 / +0x14 / +0x18 / +0x1C as stg42D section-1 offsets.
    pub exec: u32,
    pub disp: u32,
    pub dest: u32,
    pub disp2: u32,
    /// task +0x38.
    pub work: Option<ReelWork>,
    /// task +0x4C: frames since the last shadow probe.
    pub counter: i32,
    /// `cwp +4` (collision flags).
    pub cwp_flag: u16,
    /// The collision info's attributes (+4) and +0xC.
    pub attr: u32,
    pub info_c: f32,
}

/// A player as UDREEL reads and writes it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ReelPlayer {
    /// `playertwp[slot]` +0 (mode byte).
    pub mode: i8,
    /// `playertwp[slot]` +8..+0x10.
    pub ang: [i32; 3],
    /// `playertwp[slot]` +0x14..+0x1C.
    pub pos: V3,
    /// `playerpwp[slot]` +3 (character).
    pub character: i8,
    /// `twp->cwp->info +8`, when the player has a collision work.
    pub center: Option<V3>,
}

/// What UDREEL starts or changes outside its task.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReelEvent {
    /// fn_800232A4(flags, task, work): the dynamic land object joins.
    DynLand {
        flags: u32,
    },
    /// fn_800231CC(task): it leaves.
    DynLandRemove,
    /// syFree(work) (0x80119828).
    Free,
    /// SE_CallV2_Timer(0x101D, twp, 1, 0, 10, &at) (0x8006AFFC).
    Sound(V3),
    /// SetVelocityP(slot, v) (0x8003A3DC).
    Velocity {
        slot: i32,
        v: V3,
    },
    /// SetInputP(slot, a, b) (0x8003A508).
    Input {
        slot: i32,
        a: u32,
        b: u32,
    },
    /// fn_8002FB2C(slot, a, b, 0): a player action change.
    Action {
        slot: i32,
        a: u32,
        b: u32,
    },
    /// fn_800399BC(slot, at): the player's position.
    PlayerPosition {
        slot: i32,
        at: V3,
    },
    Draw(Draw),
}

pub trait ReelHost: ObjectHost {
    /// syCalloc(1, 0x4C) succeeded.
    fn calloc(&mut self) -> bool;
    /// The stage's word at sec6 +0x6C8 (stg42D) is not 0.
    fn alt_display(&self) -> bool;
    /// The byte at 0x801CC168 +0x42.
    fn mode_42(&self) -> i8;
    /// GetShadowPos's answer for `at`.
    fn shadow_height(&mut self, at: V3) -> f32;
    /// The other collision's id (`cwp +0xA4`, its +0).
    fn other_id(&self) -> u16;
    /// IsThisTaskPlayer(other task): a slot or -1.
    fn is_this_task_player(&mut self) -> i32;
    /// The two player slots (both tables, by slot).
    fn players(&mut self) -> &mut [Option<ReelPlayer>; 2];
    fn reel(&mut self, e: ReelEvent);
}

/// The data a UDREEL takes from its stage's REL.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReelData {
    /// The CollisionData row's section-5 offset (stg42D +0x20BD0).
    pub row: u32,
    /// The 0x38-byte dynamic land object (stg42D sec5 +0x20B30), as words.
    pub template: [u32; 14],
    /// Its model's +0x24 (the radius).
    pub model_radius: f32,
}

fn hang(len: f32) -> f32 {
    if len > HANG_FROM {
        HANG_ADD + len
    } else {
        ONE
    }
}

fn top(scale_y: f32) -> f32 {
    DEPTH * (ONE + scale_y)
}

/// UDREEL's object-list exec (0x1EDC8). It runs every frame until the
/// allocation succeeds.
pub fn udreel_load(t: &mut ReelTask, data: &ReelData, host: &mut impl ReelHost) {
    if host.range_out() {
        return;
    }
    if !host.calloc() {
        t.work = None;
        return;
    }
    let mut w = ReelWork::default();
    if host.alt_display() {
        t.disp = UDREEL_DISPLAY_ALT;
    } else {
        t.disp = UDREEL_DISPLAY;
        t.disp2 = UDREEL_DISPLAY_SHADOW;
    }
    t.exec = UDREEL_EXEC;
    t.dest = UDREEL_DEST;
    let s = &mut t.twp.scale;
    let (x, y) = (s[0], s[1]);
    if y < x {
        s[0] = y;
        s[1] = x;
    }
    t.twp.id = 0;
    w.len = -top(t.twp.scale[1]);
    w.speed = ZERO;
    t.twp.mode = 0;
    host.event(Event::CollisionInit {
        rows: data.row,
        count: 1,
        list: 4,
    });
    w.obj = data.template;
    w.obj[0] &= !3;
    w.obj[6] = t.twp.ang[1] as u32;
    w.obj[7] = 0;
    w.obj[5] = 0;
    for k in 0..3 {
        w.obj[2 + k] = t.twp.pos[k].to_bits();
    }
    let r = RADIUS_PAD + data.model_radius;
    let flags = if r > RADIUS_BANDS[0] {
        1
    } else if r > RADIUS_BANDS[1] {
        0x2000_0001
    } else if r > RADIUS_BANDS[2] {
        0x4000_0001
    } else {
        0x6000_0001
    };
    host.reel(ReelEvent::DynLand { flags });
    if t.twp.ang[0] & 1 == 0 {
        w.speed = ZERO;
        w.len = -(DEPTH * t.twp.scale[0]);
        t.twp.id = 3;
    }
    w.shadow = NO_SHADOW;
    t.work = Some(w);
}

/// UDREEL's destructor (0x1F094).
pub fn udreel_destructor(t: &mut ReelTask, host: &mut impl ReelHost) {
    if t.work.is_some_and(|w| w.obj[1] != 0) {
        host.reel(ReelEvent::DynLandRemove);
    }
    host.reel(ReelEvent::Free);
    t.work = None;
}

/// The hang test of states 2 and 4 (0x1F624 / 0x1F8F8).
fn check_let_go(t: &mut ReelTask, host: &mut impl ReelHost) {
    if t.twp.mode & 1 == 0 {
        return;
    }
    let slot = t.twp.w6 as i16 as i32;
    let mode = player(host, slot).map(|p| p.mode);
    if mode != Some(0x33) {
        t.twp.mode |= 2;
        host.reel(ReelEvent::Action { slot, a: 4, b: 0xf });
    }
}

fn player(host: &mut impl ReelHost, slot: i32) -> Option<&mut ReelPlayer> {
    usize::try_from(slot)
        .ok()
        .and_then(|s| host.players().get_mut(s))
        .and_then(Option::as_mut)
}

fn sound(t: &ReelTask, len: f32, host: &mut impl ReelHost) {
    let mut at = t.twp.pos;
    at[1] += END - hang(len);
    host.reel(ReelEvent::Sound(at));
}

/// UDREEL's exec (0x1F0E4). `data` is the stage's UDREEL data. A slot
/// (twp +6) outside 0..=1 indexes the player tables out of range in the
/// game; here such a slot reads as an empty slot.
pub fn udreel_exec(t: &mut ReelTask, data: &ReelData, host: &mut impl ReelHost) {
    if t.twp.mode & 1 == 0 && host.range_out() {
        return;
    }
    let Some(mut w) = t.work else {
        return;
    };
    if host.mode_42() == 1 {
        t.twp.mode = (t.twp.mode as u8 & 0xfc) as i8;
    }
    if w.shadow == NO_SHADOW && t.counter > 0xf {
        let at = [
            t.twp.pos[0],
            t.twp.pos[1] + (END - hang(w.len)),
            t.twp.pos[2],
        ];
        let h = host.shadow_height(at);
        if h != NO_SHADOW {
            w.shadow = h;
        }
        t.counter = 0;
    } else {
        t.counter = t.counter.wrapping_add(1);
    }
    if t.twp.mode & 2 != 0 {
        t.twp.mode = (t.twp.mode as u8 & 0xfc) as i8;
        let mut stack = MatrixStack::new();
        stack.push_unit();
        stack.rotate(Axis::Y, t.twp.ang[1]);
        stack.rotate(Axis::Z, t.twp.ang[2].wrapping_add(0x3400));
        let v = stack.calc_vector([ZERO, THROW + t.twp.scale[2], ZERO]);
        stack.pop();
        let slot = t.twp.w6 as i16 as i32;
        let yaw = 0x8000i32.wrapping_sub(t.twp.ang[1]);
        if let Some(p) = player(host, slot) {
            p.ang[2] = 0;
            p.ang[0] = 0;
            p.ang[1] = yaw;
        }
        host.reel(ReelEvent::Velocity { slot, v });
    }
    w.end = END - hang(w.len);
    w.rope = hang(w.len) / ROPE_SCALE;
    let sx = t.twp.scale[0];
    let sy = t.twp.scale[1];
    match t.twp.id {
        0 => {
            if t.twp.mode & 1 != 0 {
                t.twp.id = 1;
            }
        }
        1 => {
            w.len += w.speed;
            w.speed -= LIFT_PULL * (w.len + top(sy));
            if w.speed < ZERO {
                w.speed += -TOP_SPEED * GRAVITY;
                if w.speed < -TOP_SPEED {
                    w.speed = -TOP_SPEED;
                }
                t.twp.id = 2;
                t.twp.mode = (t.twp.mode as u8 & !4) as i8;
            }
        }
        2 => {
            if t.twp.mode & 4 != 0 {
                w.len += w.speed;
                let stretch = w.len + DEPTH * sx;
                if w.len < FLOOR {
                    w.speed = w.speed.abs();
                    w.len = FLOOR;
                }
                w.speed = KEEP * w.speed - PULL * stretch;
                if t.twp.mode & 1 == 0 {
                    w.speed = ZERO;
                    w.len = -(DEPTH * sx);
                    t.twp.id = if t.twp.ang[0] & 1 != 0 { 4 } else { 3 };
                    t.twp.mode = (t.twp.mode as u8 & !4) as i8;
                }
            } else {
                w.speed += -TOP_SPEED * GRAVITY;
                if w.speed < -TOP_SPEED {
                    w.speed = -TOP_SPEED;
                }
                w.len += w.speed;
                if w.len < -(DEPTH * sx) {
                    t.twp.mode |= 4;
                }
                sound(t, w.len, host);
            }
            check_let_go(t, host);
        }
        3 => {
            w.speed = ZERO;
            w.len = -(DEPTH * sx);
            if t.twp.mode & 1 != 0 {
                w.speed = ZERO;
                t.twp.id = 4;
            }
        }
        4 => {
            if t.twp.mode & 8 != 0 {
                w.len += w.speed;
                w.speed = KEEP * w.speed - PULL * (w.len + top(sy));
                if t.twp.mode & 1 == 0 {
                    w.speed = ZERO;
                    w.len = -top(sy);
                    t.twp.id = 0;
                    t.twp.mode = (t.twp.mode as u8 & !8) as i8;
                }
            } else {
                w.len += w.speed;
                w.speed += TOP_SPEED * GRAVITY;
                if w.speed > TOP_SPEED {
                    w.speed = TOP_SPEED;
                }
                if w.len >= -top(sy) {
                    t.twp.mode |= 8;
                }
                sound(t, w.len, host);
            }
            check_let_go(t, host);
        }
        _ => {}
    }
    t.work = Some(w);
    if w.obj[1] != 0 {
        let centers = host.players().map(|p| p.and_then(|p| p.center));
        if player_in_range(&centers, w.position(), NEAR_PAD + data.model_radius) != 0 {
            t.twp.flag |= 0x100;
        } else {
            t.twp.flag &= !0x100;
        }
    }
    if t.twp.mode & 1 == 0 {
        if t.cwp_flag & 1 != 0 && t.twp.b3 > 0x3c && host.other_id() == 0 {
            let slot = host.is_this_task_player();
            if slot >= 0 {
                t.twp.mode |= 1;
                t.twp.w6 = slot as i16 as u16;
                let slot = slot as i16 as i32;
                if let Some(w) = &mut t.work {
                    w.speed = ONE;
                }
                host.reel(ReelEvent::Input {
                    slot,
                    a: 0x14,
                    b: 0x4b,
                });
                host.reel(ReelEvent::Action { slot, a: 5, b: 0xb });
                return;
            }
        }
        t.info_c = END - hang(w.len) - ONE;
        host.event(Event::CollisionCalcRange);
        host.event(Event::CollisionEntry);
        if t.twp.b3 <= 0x3c {
            t.attr |= 0x10;
            t.twp.b3 += 1;
        } else {
            t.attr &= !0x10;
        }
        return;
    }
    let slot = t.twp.w6 as i16 as i32;
    let yaw = t.twp.ang[1];
    let mut at = t.twp.pos;
    at[1] += END - hang(w.len);
    let Some(p) = player(host, slot).copied() else {
        return;
    };
    at[1] -= grip_height(p.character);
    at[0] = at[0] + FOLLOW * (p.pos[0] - at[0]);
    at[2] = at[2] + FOLLOW * (p.pos[2] - at[2]);
    let turn = 0x8000i32.wrapping_sub(yaw).wrapping_sub(p.ang[1]) as i16;
    let turn = f32::from(turn) * FOLLOW;
    host.reel(ReelEvent::PlayerPosition { slot, at });
    if let Some(p) = player(host, slot) {
        p.ang[1] = p.ang[1].wrapping_add(i32::from(turn as i32 as i16));
        p.ang[2] = 0;
        p.ang[0] = 0;
    }
    t.twp.b3 = 0;
}

// --- displays ----------------------------------------------------------

/// gjDrawModel (0x80121BDC) and ds_DrawModelClip (0x8011F514).
pub const GJ_DRAW_MODEL: u32 = 0x8012_1bdc;
pub const DRAW_MODEL_CLIP: u32 = 0x8011_f514;
const DISABLE_FOG: u32 = 0x8011_c2bc;
const ENABLE_FOG: u32 = 0x8011_c2b0;
const SET_FOG: u32 = 0x8011_c310;

/// The models and textures the displays take from the stage's REL, as the
/// addresses the stage gives them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReelModels {
    /// stg42D sec5 +0x1F008.
    pub texlist: u32,
    /// The model of the object at stg42D sec5 +0x209B4 (+4), of its child
    /// (+0x2C, the rope) and of the child's child (the hook).
    pub base: u32,
    pub rope: u32,
    pub hook: u32,
    /// The rope model's float +0x20 (zeroed while the rope is drawn).
    pub rope_f20: f32,
    /// The words at stg42D sec5 +0x20BA0 +0xC and +0x2C (filled by the
    /// stage at run time; read by display 0x20114).
    pub alt: [u32; 2],
    /// The shadow helper's texlist and model (stg42D sec5 +0x1AB94,
    /// +0x1ACD0).
    pub shadow_texlist: u32,
    pub shadow_model: u32,
}

/// What the displays read besides the task.
pub trait ReelDisplayHost {
    /// The stage's word at sec6 +0x630 (stg42D): fog switched for the hook.
    fn fog(&self) -> bool;
    /// The word at 0x801CC168 +0x7C (a frame count).
    fn frame(&self) -> u32;
    fn draw(&mut self, d: Draw);
}

fn draw_rope(w: &ReelWork, m: &ReelModels, host: &mut impl ReelDisplayHost) {
    host.draw(Draw::Push);
    host.draw(Draw::ModelF20 {
        model: m.rope,
        value: ZERO,
    });
    host.draw(Draw::Scale([ONE, w.rope, ONE]));
    host.draw(Draw::Model {
        routine: GJ_DRAW_MODEL,
        model: m.rope,
    });
    host.draw(Draw::ModelF20 {
        model: m.rope,
        value: m.rope_f20,
    });
    host.draw(Draw::Pop);
    host.draw(Draw::Translate([ZERO, w.end, ZERO]));
}

/// The stage module's shadow helper (0x746C).
pub fn shadow_helper(m: &ReelModels, host: &mut impl ReelDisplayHost) {
    host.draw(Draw::SetTexture(m.shadow_texlist));
    host.draw(Draw::Push);
    host.draw(Draw::Scale([HELPER_SIZE, HELPER_ONE, HELPER_SIZE]));
    host.draw(Draw::Model {
        routine: GJ_DRAW_MODEL,
        model: m.shadow_model,
    });
    host.draw(Draw::Pop);
}

fn shadow(t: &ReelTask, w: &ReelWork, m: &ReelModels, host: &mut impl ReelDisplayHost) {
    host.draw(Draw::Push);
    host.draw(Draw::Translate([
        t.twp.pos[0],
        SHADOW_LIFT + w.shadow,
        t.twp.pos[2],
    ]));
    host.draw(Draw::Scale([SHADOW_SIZE, ONE, SHADOW_SIZE]));
    shadow_helper(m, host);
    host.draw(Draw::Pop);
}

fn fog(on: bool, host: &mut impl ReelDisplayHost) {
    if host.fog() {
        host.draw(Draw::Call(if on { ENABLE_FOG } else { DISABLE_FOG }));
        host.draw(Draw::Call(SET_FOG));
    }
}

fn base(t: &ReelTask, m: &ReelModels, host: &mut impl ReelDisplayHost) {
    host.draw(Draw::SetTexture(m.texlist));
    host.draw(Draw::Push);
    host.draw(Draw::TranslateEx(t.twp.pos));
    host.draw(Draw::RotateY(t.twp.ang[1]));
    host.draw(Draw::On3D(0x2400));
}

/// UDREEL's display 0x1FF48.
pub fn display(t: &ReelTask, w: &ReelWork, m: &ReelModels, host: &mut impl ReelDisplayHost) {
    base(t, m, host);
    host.draw(Draw::Model {
        routine: GJ_DRAW_MODEL,
        model: m.base,
    });
    host.draw(Draw::Off3D(0x2400));
    draw_rope(w, m, host);
    fog(false, host);
    host.draw(Draw::Model {
        routine: GJ_DRAW_MODEL,
        model: m.hook,
    });
    fog(true, host);
    host.draw(Draw::Pop);
}

/// UDREEL's second display 0x20088 (the shadow).
pub fn display_shadow(t: &ReelTask, w: &ReelWork, m: &ReelModels, host: &mut impl ReelDisplayHost) {
    shadow(t, w, m, host);
}

/// UDREEL's display 0x20114, used when the stage's word at sec6 +0x6C8 is
/// set: the base and, on 23 of every 46 frames or while a player hangs on,
/// the hook come from the stage's own models (`ds_DrawModelClip`), and the
/// shadow is drawn here.
pub fn display_alt(t: &ReelTask, w: &ReelWork, m: &ReelModels, host: &mut impl ReelDisplayHost) {
    base(t, m, host);
    host.draw(Draw::Model {
        routine: DRAW_MODEL_CLIP,
        model: m.alt[0],
    });
    host.draw(Draw::Off3D(0x2400));
    draw_rope(w, m, host);
    fog(false, host);
    if host.frame() % 46 < 23 || t.twp.mode & 1 != 0 {
        host.draw(Draw::Model {
            routine: DRAW_MODEL_CLIP,
            model: m.alt[1],
        });
    } else {
        host.draw(Draw::Model {
            routine: GJ_DRAW_MODEL,
            model: m.hook,
        });
    }
    fog(true, host);
    host.draw(Draw::Pop);
    shadow(t, w, m, host);
}
