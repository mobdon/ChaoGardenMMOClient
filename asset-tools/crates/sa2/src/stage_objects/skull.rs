//! SKULL and PSKULL (object list rows 10 and 13), the skull ornament and
//! the pushable skull block.
//!
//! stg42D.rel section 1:
//!
//! * SKULL: the object-list exec 0x1DB78 runs once. Unless
//!   `CheckRangeOut(task)` (0x800655F4) reports the task out, it installs
//!   exec 0x1DB14, display 0x1DA40 and destructor 0x1D598, sets the fade
//!   (task +0x40) to 1.0 (sec4 +0x9A4) and, only when Scale.x (twp +0x20)
//!   is below 1.0 (`fcmpo`, `bge` skips), registers the CollisionData row
//!   at sec5 +0x1EDA0 (`CCL_Init(task, row, 1, 4)`, 0x8000F8B8). The exec
//!   0x1DB14 calls `CCL_Entry` (0x8000F4EC) on the same condition after the
//!   range check.
//! * PSKULL: the object-list exec 0x1D500 checks the range, runs the set-up
//!   0x1D3E0, then sets the state (twp +1) to 0 and moves bit 0x10 of the
//!   three collision infos' attributes (`cwp->info[k] +4`): clear on infos
//!   0 and 1, set on info 2. The set-up installs exec 0x1D62C, display
//!   0x1DA40 and destructor 0x1D598; stores the low byte of Rotation.z in
//!   twp +3 (bits 4-7: drop chance, bits 0-3: drop kind); registers the
//!   three CollisionData rows at sec5 +0x1ED10 (`CCL_Init(task, row, 3,
//!   4)`) and sets bit 0x10 on infos 0 and 1 and clears it on info 2; calls
//!   the stage module's movable-object set-up 0x4B44 (`dyn_land::setup`:
//!   `ObjectMovableInitialize(twp, mwp, 10)`, 0x800DE884, and the body at
//!   task +0x3C from 0x49F8); when the body exists it stores 0.1 (sec4
//!   +0x9A0) as its ground bounce (+0x84) and sets its rest flag 0x10; the state becomes 2, the fade 1.0, and the movable
//!   work's word +0x30 (mwp = task +0x38) 0x1300.
//! * PSKULL exec 0x1D62C, after the range check, by state (signed byte):
//!   - 2: when the collision reports a hit (`cwp +4` bit 0) the vertical
//!     speed (mwp +4) becomes 2.0 (sec4 +0x9BC) and the state 1; then bit
//!     0x10 of info 2 is cleared and `CCL_Entry` runs.
//!   - 1 (falling): with no wait (twp +6) left it stops once the vertical
//!     speed is below 0.0 (sec4 +0x9AC); with a wait it counts down and
//!     stops when the count reaches 0. Stopping zeroes the wait and the
//!     state and puts the attribute bits back as 0x1D500 left them.
//!     Otherwise the speed (mwp +0..+8) is scaled by 0.99 (sec4 +0x9B8; y,
//!     z, x in that order), the vertical speed loses 0.1, the position
//!     (twp +0x14) moves by it and `CCL_Entry` runs.
//!   - anything else: when the twp flags (+4) have bit 0x8000 (the movable
//!     update's contact bit), the fade is 1.0; 0x1D5D0 looks for the player
//!     whose player work +0x94 is this task (`playerpwp` table 0x801E7728,
//!     slot 0 then 1); with one, the skull turns to `0x4000 - player
//!     Rotation.y` (`playertwp` table 0x801E7768, +0xC) and, the first time
//!     (task +0x30 has a SET entry and fn_80065388 reports 0),
//!     `_rename_SetConditionFlag(task, 1)` (0x8006539C) and, when the chance
//!     bits are not 0, `rand()` (0x8010DC88) decides a drop: `rand * 2^-15`
//!     (sec4 +0x9A8) below `0.05 + 0.1 * chance` (sec4 +0x9C0, +0x9A0)
//!     calls fn_800260FC(kind % 3, x, 3.0 + y, z) (sec4 +0x9C4). The wait
//!     is zeroed.
//!     Without the bit, the wait counts up to 0xB40 (fade 1.0); after that
//!     the skull fades unless a player is within 50.0 (sec4 +0x9C8;
//!     `_rename_EitherPlayerWithinSphere`, 0x800398D8, the shared
//!     `player_range::player_in_range`): the fade loses 1/60 (sec4 +0x9D0)
//!     down to 0.0, and once it is at or below 0.01 (sec4 +0x9CC) the task
//!     ends: `DeadOut` (0x800653B0) with a SET entry, else `FreeTask`
//!     (0x80071AE0), and nothing more runs. Every other path of this branch
//!     ends with the movable-object update 0x4BE0.
//! * The shared destructor 0x1D598 releases the movable body
//!   (0x4BA8, `dyn_land::release`: `syFree` through 0x49D0 when task +0x3C
//!   is set, then clears it) and zeroes the fade.
//! * The shared display 0x1DA40: `njSetTexture` (texlist sec5 +0x1E3DC),
//!   push, `njTranslateEx(position)`, `njRotateY(0, Rotation.y)`, then with
//!   the fade at 1.0 or more fn_8011E158(model sec5 +0x1ECF0); else the
//!   blended path (fn_8002B304, fn_8002B35C, `OffControl3D(0x220)`,
//!   `OnControl3D(0x810)`, fn_801218C8(0xFF, 0x800), fn_800156FC(fade, 1.0,
//!   1.0, 1.0), the model, fn_8002B348, fn_8002B2F8); pop.
//!
//! The movable-object routines 0x4B44 (set-up), 0x4BA8 (release) and 0x4BE0
//! (per-frame update) are the stage module's shared helpers, ported in
//! `dyn_land`; their own engine calls (allocation, the land probes) go
//! through [`MovableHost`], which [`SkullHost`] extends. The per-frame
//! update needs the body: the original reads it through task +0x3C even
//! when the set-up's allocation failed (a null read on GC); the port skips
//! the update then. The SET-state, drop, delete and draw calls are engine
//! boundaries.
//!
//! 0x1D3E0, 0x1D598 and 0x1D62C call those helpers with a direct `bl`
//! whose displacement the REL does not relocate; their bytes match the
//! other stages only with that displacement masked too, so they are listed
//! in [`BL_MASKED_ROUTINES`] (`tests/so_skull.rs` checks them). PSKULL's
//! load 0x1D500 is found as it is in 39 other stage RELs.
use super::dyn_land::{self, Body, Movable, MovableHost};
use super::host::{Event, V3};
use crate::enemy_lib::Rand;
use crate::player_range::player_in_range;

/// The routines this file ports (stg42D.rel section 1).
pub const ROUTINES: &[super::Routine] = &[
    super::Routine {
        name: "SKULL/PSKULL display (0x1DA40)",
        stg42d: 0x1da40,
        len: 0xd4,
        masked_sha256: "0d8ad653d8f87d43b1c099e38cc8a0acabbcaa7704a7b86e5e747fe89459bbd4",
    },
    super::Routine {
        name: "SKULL exec (0x1DB14)",
        stg42d: 0x1db14,
        len: 0x64,
        masked_sha256: "f103c1942d0e8ac63e3c93635ef5c6cc21d5e5d902c312305a478efa23aa925b",
    },
    super::Routine {
        name: "SKULL load (0x1DB78)",
        stg42d: 0x1db78,
        len: 0xa0,
        masked_sha256: "65940b9b3e202c8695212a6e19cb0324e4170d8a1e1f6ba7ec8a69bbe36c9e98",
    },
    super::Routine {
        name: "PSKULL load (0x1D500)",
        stg42d: 0x1d500,
        len: 0x98,
        masked_sha256: "d16c7671d1b3aeed7097eff80eeeb1027d4410c1e138a35371ebf44e3bab805f",
    },
    super::Routine {
        name: "PSKULL rider slot (0x1D5D0)",
        stg42d: 0x1d5d0,
        len: 0x5c,
        masked_sha256: "a97c0aa4fb472cb32e8050187acbc9622eb8da673c02bac4a260445cd26c35bf",
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
        name: "PSKULL set-up (0x1D3E0)",
        stg42d: 0x1d3e0,
        len: 0x120,
        masked_sha256: "c87606aa96631d91b2b2b065f6d0f4aff93749cbc896a9364f5bf7d829f62db1",
    },
    super::Routine {
        name: "SKULL/PSKULL destructor (0x1D598)",
        stg42d: 0x1d598,
        len: 0x38,
        masked_sha256: "6337c184ea61690d8aed7bc7f73228b7c948de8305d63149c5ce7da6e36e196e",
    },
    super::Routine {
        name: "PSKULL exec (0x1D62C)",
        stg42d: 0x1d62c,
        len: 0x414,
        masked_sha256: "3cde5890eb320cb78b3f5ed68997150e6c0b8b4ae4b9b95ac655a64e6d8ef3aa",
    },
];

/// stg42D sec5 +0x1EDA0: SKULL's CollisionData row (each stage has its own
/// copy at its own offset).
pub const SKULL_ROW: u32 = 0x1eda0;
/// stg42D sec5 +0x1ED10: PSKULL's three CollisionData rows.
pub const PSKULL_ROWS: u32 = 0x1ed10;
/// stg42D sec5 +0x1E3DC / +0x1ECF0: the display's texlist and model.
pub const TEXLIST: u32 = 0x1e3dc;
pub const MODEL: u32 = 0x1ecf0;

/// Callbacks (stg42D section-1 offsets) the load routines install.
pub const DISPLAY: u32 = 0x1da40;
pub const SKULL_EXEC: u32 = 0x1db14;
pub const DESTRUCTOR: u32 = 0x1d598;
pub const PSKULL_EXEC: u32 = 0x1d62c;

const ONE: f32 = 1.0; // sec4 +0x9A4
const TENTH: f32 = f32::from_bits(0x3dcc_cccd); // sec4 +0x9A0
const UNIT: f32 = f32::from_bits(0x3800_0000); // sec4 +0x9A8, 2^-15
const ZERO: f32 = 0.0; // sec4 +0x9AC
const DRAG: f32 = f32::from_bits(0x3f7d_70a4); // sec4 +0x9B8, 0.99
const KICK: f32 = 2.0; // sec4 +0x9BC
const BASE_CHANCE: f32 = f32::from_bits(0x3d4c_cccd); // sec4 +0x9C0, 0.05
const DROP_LIFT: f32 = 3.0; // sec4 +0x9C4
const NEAR: f32 = 50.0; // sec4 +0x9C8
const GONE: f32 = f32::from_bits(0x3c23_d70a); // sec4 +0x9CC, 0.01
const FADE_STEP: f32 = f32::from_bits(0x3c88_8889); // sec4 +0x9D0, 1/60
/// The wait (twp +6) before an unpushed PSKULL may fade.
pub const FADE_WAIT: u16 = 0xb40;

/// The task work (twp) fields these objects use.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Twp {
    /// +0 (signed byte): mode / flag bits.
    pub mode: i8,
    /// +1 (signed byte): state.
    pub id: i8,
    /// +2.
    pub b2: u8,
    /// +3.
    pub b3: u8,
    /// +4 (halfword flags).
    pub flag: i16,
    /// +6 (halfword).
    pub w6: u16,
    /// +8, +0xC, +0x10: Rotation.
    pub ang: [i32; 3],
    /// +0x14: position.
    pub pos: V3,
    /// +0x20: Scale.
    pub scale: V3,
}

/// A SKULL or PSKULL task.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SkullTask {
    pub twp: Twp,
    /// task +0x10 / +0x14 / +0x18: exec, display and destructor, as stg42D
    /// section-1 offsets (0 = not installed by this code).
    pub exec: u32,
    pub disp: u32,
    pub dest: u32,
    /// task +0x30 holds a SET entry.
    pub has_set: bool,
    /// task +0x40: the fade (draw alpha).
    pub alpha: f32,
    /// `cwp +4` (collision flags).
    pub cwp_flag: u16,
    /// `cwp->info[k] +4` for the three collision infos.
    pub attr: [u32; 3],
    /// mwp (task +0x38) +0..+8: the speed.
    pub speed: V3,
    /// mwp +0x30.
    pub mwp_30: u32,
    /// task +0x3C: the movable body (`dyn_land`), when set.
    pub land: Option<Movable>,
}

/// A player as PSKULL reads it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SkullPlayer {
    /// `playerpwp[slot]` +0x94 is this task.
    pub rides: bool,
    /// `playertwp[slot]` +0xC (Rotation.y).
    pub ang_y: i32,
    /// The player's collision centre (`twp->cwp->info +8`), when it has a
    /// collision work.
    pub center: Option<V3>,
}

/// A drawing call (display callbacks).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Draw {
    /// njSetTexture(texlist) (0x80116E18); the address as the stage gives it.
    SetTexture(u32),
    /// njPushMatrixEx (0x80116098).
    Push,
    /// njPopMatrixEx (0x801160E4).
    Pop,
    /// njTranslateEx(&v) (0x8011616C).
    TranslateEx(V3),
    /// njTranslate(0, x, y, z) (0x80115BD8).
    Translate(V3),
    /// njRotateY(0, angle) (0x80115828).
    RotateY(i32),
    /// njScale(0, x, y, z) (0x80115178).
    Scale(V3),
    /// A model draw: `routine` is the main.dol routine (fn_8011E158,
    /// gjDrawModel 0x80121BDC or ds_DrawModelClip 0x8011F514).
    Model {
        routine: u32,
        model: u32,
    },
    /// OnControl3D (0x8002B2E8) / OffControl3D (0x8002B2D8).
    On3D(u32),
    Off3D(u32),
    /// A main.dol routine called with no arguments that matter.
    Call(u32),
    /// fn_801218C8(a, b).
    Blend(u32, u32),
    /// fn_800156FC(a, r, g, b).
    Color([f32; 4]),
    /// A write to a model's word +0x20 (a float), as stored.
    ModelF20 {
        model: u32,
        value: f32,
    },
}

/// What SKULL and PSKULL start or change outside their task.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SkullEvent {
    /// `_rename_SetConditionFlag(task, 1)` (0x8006539C).
    SetCondition,
    /// fn_800260FC(kind, at): the drop.
    Drop {
        kind: u32,
        at: V3,
    },
    /// DeadOut(task) (0x800653B0).
    DeadOut,
    /// FreeTask(task) (0x80071AE0).
    FreeTask,
    Draw(Draw),
}

pub trait SkullHost: MovableHost {
    /// fn_80065388(task): the SET entry's condition (as a signed byte).
    fn condition(&mut self) -> i8;
    /// The two player slots.
    fn players(&self) -> [Option<SkullPlayer>; 2];
    /// MSL `rand` (0x8010DC88) state.
    fn rand(&mut self) -> &mut Rand;
    fn skull(&mut self, e: SkullEvent);
}

/// SKULL's object-list exec (0x1DB78), run once.
pub fn skull_load(t: &mut SkullTask, row: u32, host: &mut impl SkullHost) {
    if host.range_out() {
        return;
    }
    t.disp = DISPLAY;
    t.exec = SKULL_EXEC;
    t.dest = DESTRUCTOR;
    t.alpha = ONE;
    if t.twp.scale[0] < ONE {
        host.event(Event::CollisionInit {
            rows: row,
            count: 1,
            list: 4,
        });
    }
}

/// SKULL's exec (0x1DB14).
pub fn skull_exec(t: &mut SkullTask, host: &mut impl SkullHost) {
    if host.range_out() {
        return;
    }
    if t.twp.scale[0] < ONE {
        host.event(Event::CollisionEntry);
    }
}

/// The shared destructor (0x1D598).
pub fn destructor(t: &mut SkullTask, host: &mut impl SkullHost) {
    dyn_land::release(&mut t.land, host);
    t.alpha = 0.0;
}

/// PSKULL's set-up (0x1D3E0).
fn pskull_setup(t: &mut SkullTask, rows: u32, host: &mut impl SkullHost) {
    t.disp = DISPLAY;
    t.exec = PSKULL_EXEC;
    t.dest = DESTRUCTOR;
    t.twp.id = 0;
    t.twp.b3 = t.twp.ang[2] as u8;
    host.event(Event::CollisionInit {
        rows,
        count: 3,
        list: 4,
    });
    t.attr[0] |= 0x10;
    t.attr[1] |= 0x10;
    t.attr[2] &= !0x10;
    t.land = dyn_land::setup(&mut t.twp.flag, &mut t.twp.b2, true, t.twp.pos, host);
    if let Some(l) = &mut t.land {
        l.ground_bounce = TENTH;
        l.flags |= dyn_land::F_REST;
    }
    t.twp.id = 2;
    t.alpha = ONE;
    t.mwp_30 = 0x1300;
}

fn rest_attributes(t: &mut SkullTask) {
    t.attr[0] &= !0x10;
    t.attr[1] &= !0x10;
    t.attr[2] |= 0x10;
}

/// PSKULL's object-list exec (0x1D500), run once. (It checks the
/// collision work for null before touching the attributes; the set-up
/// before it already wrote them, so a task here always has one.)
pub fn pskull_load(t: &mut SkullTask, rows: u32, host: &mut impl SkullHost) {
    if host.range_out() {
        return;
    }
    pskull_setup(t, rows, host);
    t.twp.id = 0;
    rest_attributes(t);
}

/// 0x1D5D0: the slot whose player work +0x94 is this task, else -1.
pub fn rider_slot(players: &[Option<SkullPlayer>; 2]) -> i32 {
    for (slot, p) in players.iter().enumerate() {
        if p.is_some_and(|p| p.rides) {
            return slot as i32;
        }
    }
    -1
}

/// PSKULL's exec (0x1D62C).
pub fn pskull_exec(t: &mut SkullTask, host: &mut impl SkullHost) {
    if host.range_out() {
        return;
    }
    match t.twp.id {
        2 => {
            if t.cwp_flag & 1 != 0 {
                t.speed[1] = KICK;
                t.twp.id = 1;
            }
            t.attr[2] &= !0x10;
            host.event(Event::CollisionEntry);
            return;
        }
        1 => {
            let w = t.twp.w6;
            let stop = if w == 0 {
                t.speed[1] < ZERO
            } else {
                t.twp.w6 = w - 1;
                t.twp.w6 == 0
            };
            if stop {
                t.twp.w6 = 0;
                t.twp.id = 0;
                rest_attributes(t);
                return;
            }
            t.speed[1] *= DRAG;
            t.speed[2] *= DRAG;
            t.speed[0] *= DRAG;
            t.speed[1] -= TENTH;
            t.twp.pos[1] += t.speed[1];
            t.twp.pos[0] += t.speed[0];
            t.twp.pos[2] += t.speed[2];
            host.event(Event::CollisionEntry);
            return;
        }
        _ => {}
    }
    if t.twp.flag as u16 & 0x8000 != 0 {
        t.alpha = ONE;
        let players = host.players();
        let slot = rider_slot(&players);
        if let Some(p) = usize::try_from(slot).ok().and_then(|s| players[s]) {
            t.twp.ang[1] = 0x4000i32.wrapping_sub(p.ang_y);
            if t.has_set && host.condition() == 0 {
                host.skull(SkullEvent::SetCondition);
                let chance = t.twp.b3 & 0xf0;
                if chance != 0 {
                    let base = f32::from(chance >> 4) * TENTH;
                    let roll = host.rand().rand() as f32 * UNIT;
                    if roll < BASE_CHANCE + base {
                        let at = [t.twp.pos[0], DROP_LIFT + t.twp.pos[1], t.twp.pos[2]];
                        let kind = u32::from(t.twp.b3 & 0xf) % 3;
                        host.skull(SkullEvent::Drop { kind, at });
                    }
                }
            }
        }
        t.twp.w6 = 0;
    } else if t.twp.w6 < FADE_WAIT {
        t.twp.w6 += 1;
        t.alpha = ONE;
    } else {
        let centers = host.players().map(|p| p.and_then(|p| p.center));
        if player_in_range(&centers, t.twp.pos, NEAR) != 0 {
            t.alpha = ONE;
        } else {
            let a = t.alpha;
            if a <= GONE {
                host.skull(if t.has_set {
                    SkullEvent::DeadOut
                } else {
                    SkullEvent::FreeTask
                });
                return;
            }
            t.alpha = a - FADE_STEP;
            if t.alpha < ZERO {
                t.alpha = ZERO;
            }
        }
    }
    // 0x4BE0.
    if let Some(st) = &mut t.land {
        let b = Body {
            flag: t.twp.flag,
            ang: &mut t.twp.ang,
            pos: &mut t.twp.pos,
            speed: &mut t.speed,
            cwp_flag: Some(t.cwp_flag),
        };
        dyn_land::update(b, st, host);
    }
}

/// fn_8011E158, the model draw the display uses.
pub const DRAW_MODEL_8011E158: u32 = 0x8011_e158;

/// The shared display (0x1DA40). `texlist` and `model` are the addresses
/// the stage's REL gives sec5 +0x1E3DC and +0x1ECF0 in stg42D.
pub fn display(t: &SkullTask, texlist: u32, model: u32, host: &mut impl SkullHost) {
    let mut d = |x| host.skull(SkullEvent::Draw(x));
    d(Draw::SetTexture(texlist));
    d(Draw::Push);
    d(Draw::TranslateEx(t.twp.pos));
    d(Draw::RotateY(t.twp.ang[1]));
    let a = t.alpha;
    let draw = Draw::Model {
        routine: DRAW_MODEL_8011E158,
        model,
    };
    if a >= ONE {
        d(draw);
    } else {
        d(Draw::Call(0x8002_b304));
        d(Draw::Call(0x8002_b35c));
        d(Draw::Off3D(0x220));
        d(Draw::On3D(0x810));
        d(Draw::Blend(0xff, 0x800));
        d(Draw::Color([a, ONE, ONE, ONE]));
        d(draw);
        d(Draw::Call(0x8002_b348));
        d(Draw::Call(0x8002_b2f8));
    }
    d(Draw::Pop);
}
