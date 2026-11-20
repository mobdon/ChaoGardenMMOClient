//! The stage module's movable-object helpers (stg42D.rel section 1
//! 0x378C..0x5238): the small physics body some stage objects (PSKULL, the
//! Omochao, ORI) keep at task +0x3C. It probes the land around the object,
//! pushes it out of ceilings, floors and walls, bounces its speed off them,
//! applies gravity and drag and, for rolling objects, turns a rolling
//! matrix. Every stage REL that has these objects carries its own copy;
//! `tests/stage_objects_shared.rs` finds the same bytes in the other stage
//! and boss RELs (Boss_Last2D has them at 0x10C80..0x120D4).
//!
//! The body (`Movable`, 0x188 bytes from `syCalloc(1, 0x188)`, 0x801197FC):
//!
//! | offset | field |
//! |---|---|
//! | +0x00 | `roll`: a matrix the per-frame update multiplies by `+0x30` |
//! | +0x30 | `roll_step`: the rolling step matrix |
//! | +0x60 | `flags` (see the `F_*` constants) |
//! | +0x64 | `probe_offset`: the probe point's offset in the object's frame |
//! | +0x70 | `head`: the ceiling height above the probe point |
//! | +0x74 | `radius`: the wall radius |
//! | +0x78 | `foot`: the floor height relative to the probe point |
//! | +0x7C | `step`: how far the floor may drop before the object goes back |
//! | +0x80 | `wall_bounce`, +0x84 `ground_bounce`, +0x88 `ground_drag` |
//! | +0x8C | `f_8c` (set to 0.1; no routine here reads it) |
//! | +0x90 | `wall_turn`: the yaw step a wall bounce adds or takes |
//! | +0x94 | `last_pos`: the position after the last physics step |
//! | +0xA0 | `ground_normal` |
//! | +0xAC | `shadow`: GetShadowPosXYZ's 0xCC-byte block (point, 6 rays) |
//! | +0x178 | `probe_turned`: the probe offset turned by the object's angles |
//! | +0x184 | `task_flag`: the twp flags (+4) of the last update |
//!
//! Routines (stg42D section 1):
//!
//! * 0x378C (probe): `njPushMatrix(nj_unit_matrix)` (0x80115D60 with the
//!   identity at 0x80357880), `njRotateZ/X/Y` by the object's Rotation z,
//!   x, y, `njCalcPoint` of the probe offset into +0x178, pop; the shadow
//!   block's point is the position plus that. With flag 0x2000 it runs
//!   fn_80066E1C on the block and sets flag 0x4000 to its answer, else
//!   GetShadowPosXYZ (0x80067210).
//! * 0x38AC (ceiling): flag 0x20 is cleared; when `point.y + head` is above
//!   ray 3's coordinate and that ray's normal y is negative, the vertical
//!   speed becomes 0, the position drops by `0.3 * overlap` (sec4 +0x1E4)
//!   and flag 0x20 is set.
//! * 0x3924 (ground): `point.y + foot` against ray 2. With flag 1 and not
//!   0x100, when that point minus `step` is above the ground or no ground
//!   was found, the object goes back to `last_pos` minus `0.4 * speed` in X
//!   and Z (sec4 +0x1E8), stops, gets flags 0x140 and the normal (0, 1, 0).
//!   Otherwise flag 0x40 is cleared; when the point is below the ground and
//!   its normal y is positive, the speed bounces (y times `-ground_bounce`,
//!   x and z times `ground_drag`), the position rises by `0.3 * depth`,
//!   with flag 4 Rotation x and z turn toward the ground's angles by 0x80
//!   (`AdjustAngle`, 0x8002B918), the ground normal is kept, flag 0x40 is
//!   set and 0x100 cleared; else the normal is (0, 1, 0).
//! * 0x3B68 (walls): flag 0x80 is cleared. Rays 0, 1, 4, 5 (directions
//!   (-1, 0, 0), (1, 0, 0), (0, 0, -1), (0, 0, 1), sec4 +0x1F0..+0x21C) are
//!   taken when they hit and the normal faces the ray (`njInnerProduct`
//!   below -0.5, sec4 +0x224; with flag 8, `|dot|` above 0.5, sec4 +0x220).
//!   The signed distance from the probe point to the hit plane is
//!   fn_80012EA4. A point behind the wall within `radius` is a candidate
//!   (the nearest is kept; ray 5 does not update the nearest distance, the
//!   last ray, so it changes nothing); otherwise, while no candidate is
//!   known, the position moves along the normal's X and Z by
//!   `0.35 * (radius - distance)` (sec4 +0x228) when that is not negative,
//!   flag 0x80 is set and, when the speed runs into the wall, it is
//!   reflected: `speed += n * (-2 * dot(speed, n) * wall_bounce)` (sec4
//!   +0x22C) with n the unit normal (`njUnitVector`; (1, 0, 0) when its
//!   estimate is 0), and the yaw changes by `wall_turn`, down when `n x
//!   speed` points up (`njOuterProduct`). With a candidate the position
//!   becomes its foot point, flag 0x80 is set and the speed is reflected
//!   the same way (without the facing test; a zero normal becomes the
//!   ray's own axis).
//! * 0x491C (physics step): the four above, then `last_pos` = position.
//! * 0x49D0 (free): `syFree` (0x80119828) when the pointer is set.
//! * 0x49F8 (create): `syCalloc(1, 0x188)`; its defaults (sec4 +0x1E0..
//!   +0x250): radius 10, probe offset (0, 5, 0), head 15, foot -5, step 50,
//!   bounces 0.7 / 0.7, `f_8c` 0.1, wall bounce 0.8, wall turn 0x400,
//!   `last_pos` the position, normal (0, 1, 0), flags 0x500 without 0x2000,
//!   both matrices the identity (`njUnitMatrix`, 0x801153E4).
//! * 0x4B44 (set-up, task): `ObjectMovableInitialize(twp, mwp, 10)`
//!   (0x800DE884, ported here) and 0x49F8 with the position; the body goes
//!   to task +0x3C.
//! * 0x4BA8 (release, task): 0x49D0 on task +0x3C, which is then cleared.
//! * 0x4BE0 (per-frame update, task): see [`update`].
//!
//! The REL's `fabsf` (+0x84, `fabs`) and `sqrtf` (+0x74CC, listed by
//! `light_modmod` and ported as `enemy_lib::engine::sqrtf_rel`) are reused.
//! main.dol routines: the land probes GetShadowPosXYZ / fn_80066E1C,
//! allocation and the byte at 0x801CC168 +0x37 are host calls; CCL_Entry
//! is `Event::CollisionEntry`; `CCL_IsPushed` (0x80006774),
//! `ObjectMovableInitialize`, fn_80012EA4 (point-to-plane distance, ported
//! here) and the Ninja and MSL math run on the crate's native ports
//! (`ninja_matrix`, `ninja_vector`, `msl_math::atan2f`,
//! `enemy_lib::engine`). The host can answer the probes with
//! `land_probe::shadow_xyz` and [`write_shadow`].
use super::host::{Event, ObjectHost, V3};
use crate::{
    enemy_lib::engine::{
        adjust_angle, fctiwz_f64, sqrtf_dol, sqrtf_rel, unit_vector_len, BAMS_PER_RADIAN,
    },
    land_probe::{Shadow, RAYS},
    msl_math::atan2f,
    ninja_matrix::{concat, mult_vec, mult_vec_sr, rotate, Axis, Mtx, IDENTITY},
    ninja_trig::{fctiwz, fmadds, fnmsubs},
    ninja_vector::{inner_product, outer_product},
};

/// The routines this file ports (stg42D.rel section 1).
pub const ROUTINES: &[super::Routine] = &[
    super::Routine {
        name: "movable probe (0x378C)",
        stg42d: 0x378c,
        len: 0x120,
        masked_sha256: "1fd2a47f25379a53cde44036b516b153a92700106881490cfd65593cd2a7cb35",
    },
    super::Routine {
        name: "movable ceiling (0x38AC)",
        stg42d: 0x38ac,
        len: 0x78,
        masked_sha256: "3bbb33acae6340111034085a5ff2c90f472fbb1e60b62ecd195c5964cd565824",
    },
    super::Routine {
        name: "movable ground (0x3924)",
        stg42d: 0x3924,
        len: 0x244,
        masked_sha256: "37a080f3148afc2ad741fc097132bf982ee287d198140bbd6d9ef5fbd6b1e2df",
    },
    super::Routine {
        name: "movable walls (0x3B68)",
        stg42d: 0x3b68,
        len: 0xdb4,
        masked_sha256: "a588e6ebd3e85b0a4c514f00c4b2fd15f2e9dd9d2ac4aef3cad838c35062c082",
    },
    super::Routine {
        name: "movable physics step (0x491C)",
        stg42d: 0x491c,
        len: 0xb4,
        masked_sha256: "15083c150940605df76c6fbcf28e009ce32de034151acd85dedc2dd370ede660",
    },
    super::Routine {
        name: "movable free (0x49D0)",
        stg42d: 0x49d0,
        len: 0x28,
        masked_sha256: "bd454a0899854d9d8da46f3918ba4194f91ab7817104843b29a7be3b82da85a5",
    },
    super::Routine {
        name: "movable create (0x49F8)",
        stg42d: 0x49f8,
        len: 0x14c,
        masked_sha256: "9bb8e5b9068a76e64c86a1c4f0a255f287609b66fa52d4759280c0e50de04fc9",
    },
    super::Routine {
        name: "movable set-up (0x4B44)",
        stg42d: 0x4b44,
        len: 0x64,
        masked_sha256: "bc2b2e6dfba40f67b3227e2fb7f2d4df628ef873aac8d06eec66fd946033927e",
    },
    super::Routine {
        name: "movable release (0x4BA8)",
        stg42d: 0x4ba8,
        len: 0x38,
        masked_sha256: "66056df84bd90c97cb474d85bdfc0a5b181cc2a8894a1e5cfd63d17a44611bac",
    },
    super::Routine {
        name: "movable update (0x4BE0)",
        stg42d: 0x4be0,
        len: 0x658,
        masked_sha256: "464963819926205004eacc80bc935d11e149ddc9e80d0742ef9426a8ebc98b70",
    },
];

/// Flags (+0x60).
pub const F_FALL_BACK: u32 = 0x1; // ground: go back when the floor drops away
pub const F_ROLL: u32 = 0x2; // update: turn the rolling matrix
pub const F_TILT: u32 = 0x4; // ground: tilt to the ground's angles
pub const F_WALL_ANY_SIDE: u32 = 0x8; // walls: take walls facing either way
pub const F_REST: u32 = 0x10; // update: no gravity or physics while still
pub const F_CEILING: u32 = 0x20; // ceiling hit this step
pub const F_GROUND: u32 = 0x40; // on the ground
pub const F_WALL: u32 = 0x80; // wall hit this step
pub const F_FELL_BACK: u32 = 0x100; // went back to the last position
pub const F_WATER: u32 = 0x2000; // probe with fn_80066E1C
pub const F_WATER_HIT: u32 = 0x4000; // fn_80066E1C's answer
pub const F_SLOW: u32 = 0x10000; // light gravity, half-speed moves
pub const F_KEEP_ACTIVE: u32 = 0x8000_0000; // update runs while 0x801CC19F is set

const ZERO: f32 = 0.0; // sec4 +0x1E0
const PUSH_OUT: f32 = f32::from_bits(0x3e99_999a); // sec4 +0x1E4, 0.3
const BACK_OFF: f32 = f32::from_bits(0x3ecc_cccd); // sec4 +0x1E8, 0.4
const ONE: f32 = 1.0; // sec4 +0x1EC
const FACING: f32 = 0.5; // sec4 +0x220
const FACING_NEG: f32 = -0.5; // sec4 +0x224
const WALL_PUSH: f32 = f32::from_bits(0x3eb3_3333); // sec4 +0x228, 0.35
const REFLECT: f32 = -2.0; // sec4 +0x22C
const MINUS_ONE: f32 = -1.0; // sec4 +0x230
const ROLL_SCALE: f32 = f32::from_bits(0x4336_0b61); // sec4 +0x260, 65536/360
const DEGREES: f32 = 360.0; // sec4 +0x264
const ROLL_CIRCUMFERENCE: f32 = f32::from_bits(0x427b_3334); // sec4 +0x268, 62.8
const BOUNCE_STOP: f32 = f32::from_bits(0x3e75_c28f); // sec4 +0x26C, 0.24
const STILL: f32 = f32::from_bits(0x3a83_126f); // sec4 +0x270, 0.001
const GRAVITY_SLOW: f32 = f32::from_bits(0x3c83_126f); // sec4 +0x274, 0.016
const GRAVITY: f32 = f32::from_bits(0x3da3_d70a); // sec4 +0x278, 0.08
const DRAG: f32 = f32::from_bits(0x3f7f_3b64); // sec4 +0x27C, 0.997
const NUDGE: f32 = f32::from_bits(0xbe87_2b02); // sec4 +0x280, -0.264
const MAX_FALL: f32 = f32::from_bits(0x401c_cccd); // sec4 +0x284, 2.45
const SETTLE: f32 = f32::from_bits(0x3f66_6666); // sec4 +0x288, 0.9

/// stg42D sec5 +0xD0F0: the vector the rolling matrix pushes the speed
/// with, (0, 0.1, 0). It is stage data; `tests/so_dynland.rs` checks every
/// REL with these helpers loads the same words.
pub const ROLL_PUSH: V3 = [0.0, f32::from_bits(0x3dcc_cccd), 0.0];

/// The wall rays 0x3B68 tests and their directions (sec4 +0x1F0, +0x1FC,
/// +0x208, +0x214).
const WALL_RAYS: [(usize, V3); 4] = [
    (0, [-1.0, 0.0, 0.0]),
    (1, [1.0, 0.0, 0.0]),
    (4, [0.0, 0.0, -1.0]),
    (5, [0.0, 0.0, 1.0]),
];

/// The size `syCalloc` is asked for.
pub const SIZE: u32 = 0x188;

/// One ray of GetShadowPosXYZ's block (0x20 bytes). The probe clears `hit`
/// and resets `coord` every call and writes the rest only for rays that
/// hit, so the other fields keep what an earlier call left.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ShadowRecord {
    /// +0: non-null when the ray hit.
    pub hit: bool,
    /// +4: the land entry's flags.
    pub flags: u32,
    /// +8, +0xC: the angles the ground tilts by.
    pub angle_x: i32,
    pub angle_z: i32,
    /// +0x10: the coordinate along the ray's axis.
    pub coord: f32,
    /// +0x14: the unit normal.
    pub normal: V3,
}

/// GetShadowPosXYZ's block (body +0xAC): the probe point and six rays
/// (X below/above, Y below/above, Z below/above).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ShadowBlock {
    pub point: V3,
    pub rays: [ShadowRecord; RAYS],
}

/// What GetShadowPosXYZ (0x80067210) and fn_80066E1C write into the block
/// from their result (`land_probe::shadow_xyz`): every ray's +0 and
/// coordinate; the rest only for rays that hit.
pub fn write_shadow(block: &mut ShadowBlock, s: &Shadow) {
    for (r, rec) in block.rays.iter_mut().enumerate() {
        rec.hit = false;
        rec.coord = s.coords[r];
        if let Some(h) = s.hits[r] {
            rec.hit = true;
            rec.flags = h.flags;
            rec.angle_x = h.angle_x;
            rec.angle_z = h.angle_z;
            rec.normal = h.normal;
        }
    }
}

/// The movable body at task +0x3C (0x188 bytes).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Movable {
    pub roll: Mtx,
    pub roll_step: Mtx,
    pub flags: u32,
    pub probe_offset: V3,
    pub head: f32,
    pub radius: f32,
    pub foot: f32,
    pub step: f32,
    pub wall_bounce: f32,
    pub ground_bounce: f32,
    pub ground_drag: f32,
    pub f_8c: f32,
    pub wall_turn: i32,
    pub last_pos: V3,
    pub ground_normal: V3,
    pub shadow: ShadowBlock,
    pub probe_turned: V3,
    pub task_flag: i16,
}

/// The body's size in words.
pub const WORDS: usize = (SIZE / 4) as usize;

impl Movable {
    /// The body as 0x49F8 leaves it, for an object at `pos`.
    pub fn new(pos: V3) -> Self {
        Movable {
            roll: IDENTITY,
            roll_step: IDENTITY,
            flags: 0x500,
            probe_offset: [0.0, 5.0, 0.0], // sec4 +0x238 at +0x68
            head: 15.0,                    // sec4 +0x23C
            radius: 10.0,                  // sec4 +0x234
            foot: -5.0,                    // sec4 +0x240
            step: 50.0,                    // sec4 +0x244
            wall_bounce: f32::from_bits(0x3f4c_cccd), // sec4 +0x250, 0.8
            ground_bounce: f32::from_bits(0x3f33_3333), // sec4 +0x248, 0.7
            ground_drag: f32::from_bits(0x3f33_3333), // sec4 +0x248
            f_8c: f32::from_bits(0x3dcc_cccd), // sec4 +0x24C, 0.1
            wall_turn: 0x400,
            last_pos: pos,
            ground_normal: [ZERO, ONE, ZERO],
            shadow: ShadowBlock::default(),
            probe_turned: [0.0; 3],
            task_flag: 0,
        }
    }

    /// The body read from its 0x188 bytes as big-endian words (the
    /// halfword at +0x186 is padding).
    pub fn from_words(w: &[u32; WORDS]) -> Self {
        let f = |k: usize| f32::from_bits(w[k]);
        let v = |k: usize| [f(k), f(k + 1), f(k + 2)];
        let m = |k: usize| -> Mtx {
            std::array::from_fn(|r| std::array::from_fn(|c| f(k + 4 * r + c)))
        };
        let rays = std::array::from_fn(|r| {
            let k = 0x2b + 3 + 8 * r;
            ShadowRecord {
                hit: w[k] != 0,
                flags: w[k + 1],
                angle_x: w[k + 2] as i32,
                angle_z: w[k + 3] as i32,
                coord: f(k + 4),
                normal: v(k + 5),
            }
        });
        Movable {
            roll: m(0),
            roll_step: m(0xc),
            flags: w[0x18],
            probe_offset: v(0x19),
            head: f(0x1c),
            radius: f(0x1d),
            foot: f(0x1e),
            step: f(0x1f),
            wall_bounce: f(0x20),
            ground_bounce: f(0x21),
            ground_drag: f(0x22),
            f_8c: f(0x23),
            wall_turn: w[0x24] as i32,
            last_pos: v(0x25),
            ground_normal: v(0x28),
            shadow: ShadowBlock {
                point: v(0x2b),
                rays,
            },
            probe_turned: v(0x5e),
            task_flag: (w[0x61] >> 16) as i16,
        }
    }

    /// The body as words (`hit` as 1 or 0, the padding as 0).
    pub fn to_words(&self) -> [u32; WORDS] {
        let mut w = [0u32; WORDS];
        let put_v = |w: &mut [u32; WORDS], k: usize, v: V3| {
            for (j, x) in v.iter().enumerate() {
                w[k + j] = x.to_bits();
            }
        };
        for r in 0..3 {
            for c in 0..4 {
                w[4 * r + c] = self.roll[r][c].to_bits();
                w[0xc + 4 * r + c] = self.roll_step[r][c].to_bits();
            }
        }
        w[0x18] = self.flags;
        put_v(&mut w, 0x19, self.probe_offset);
        w[0x1c] = self.head.to_bits();
        w[0x1d] = self.radius.to_bits();
        w[0x1e] = self.foot.to_bits();
        w[0x1f] = self.step.to_bits();
        w[0x20] = self.wall_bounce.to_bits();
        w[0x21] = self.ground_bounce.to_bits();
        w[0x22] = self.ground_drag.to_bits();
        w[0x23] = self.f_8c.to_bits();
        w[0x24] = self.wall_turn as u32;
        put_v(&mut w, 0x25, self.last_pos);
        put_v(&mut w, 0x28, self.ground_normal);
        put_v(&mut w, 0x2b, self.shadow.point);
        for (r, rec) in self.shadow.rays.iter().enumerate() {
            let k = 0x2b + 3 + 8 * r;
            w[k] = u32::from(rec.hit);
            w[k + 1] = rec.flags;
            w[k + 2] = rec.angle_x as u32;
            w[k + 3] = rec.angle_z as u32;
            w[k + 4] = rec.coord.to_bits();
            put_v(&mut w, k + 5, rec.normal);
        }
        put_v(&mut w, 0x5e, self.probe_turned);
        w[0x61] = (self.task_flag as u16 as u32) << 16;
        w
    }
}

/// What the helpers ask of the game.
pub trait MovableHost: ObjectHost {
    /// `syCalloc(count, size)` (0x801197FC): whether it returned memory.
    fn sy_calloc_801197fc(&mut self, count: u32, size: u32) -> bool;
    /// `syFree` (0x80119828) of the body.
    fn sy_free_80119828(&mut self);
    /// GetShadowPosXYZ (0x80067210) on the block (its point is set).
    fn get_shadow_pos_xyz_80067210(&mut self, block: &mut ShadowBlock);
    /// fn_80066E1C on the block; its return value.
    fn shadow_water_80066e1c(&mut self, block: &mut ShadowBlock) -> bool;
    /// The signed byte at 0x801CC168 +0x37 (0x801CC19F).
    fn byte_801cc19f(&self) -> i8;
}

/// The task fields the helpers use: twp (task +0x34) flags (+4), Rotation
/// (+8) and Position (+0x14), the movable work's speed (task +0x38 +0..+8)
/// and the collision work's flags (`twp->cwp` +4, `None` without a cwp).
pub struct Body<'a> {
    pub flag: i16,
    pub ang: &'a mut [i32; 3],
    pub pos: &'a mut V3,
    pub speed: &'a mut V3,
    pub cwp_flag: Option<u16>,
}

/// `ObjectMovableInitialize(twp, mwp, kind)` (0x800DE884): with a movable
/// work, kinds 10 to 14 set twp flags (10, 12: 0x240; 11: 0x440; 13: 0x840;
/// 14: 0x1040) and store the kind in twp +2. Returns whether it did.
pub fn object_movable_initialize(flag: &mut i16, b2: &mut u8, has_mwp: bool, kind: i32) -> bool {
    if !has_mwp {
        return false;
    }
    let bits = match kind {
        10 | 12 => 0x240,
        11 => 0x440,
        13 => 0x840,
        14 => 0x1040,
        _ => return false,
    };
    *flag |= bits;
    *b2 = kind as u8;
    true
}

/// 0x49F8: a new body for an object at `pos`, or `None` when `syCalloc`
/// fails.
pub fn create(pos: V3, host: &mut impl MovableHost) -> Option<Movable> {
    host.sy_calloc_801197fc(1, SIZE).then(|| Movable::new(pos))
}

/// 0x4B44: `ObjectMovableInitialize(twp, mwp, 10)`, then the body for the
/// task's position (task +0x3C).
pub fn setup(
    flag: &mut i16,
    b2: &mut u8,
    has_mwp: bool,
    pos: V3,
    host: &mut impl MovableHost,
) -> Option<Movable> {
    object_movable_initialize(flag, b2, has_mwp, 10);
    create(pos, host)
}

/// 0x4BA8 (with 0x49D0): the body is freed and task +0x3C cleared.
pub fn release(body: &mut Option<Movable>, host: &mut impl MovableHost) {
    if body.is_some() {
        host.sy_free_80119828();
    }
    *body = None;
}

/// fn_80012EA4 (main.dol): the signed distance from `p` to the plane
/// through `q` with normal `n` (`dot(p - q, n) / |n|`, the length through
/// MSL `sqrtf`'s inline Newton steps), and the foot point
/// `p - n * dot / |n|²`.
pub fn plane_distance_80012ea4(p: V3, q: V3, n: V3) -> (f32, V3) {
    let dy = p[1] - q[1];
    let dx = p[0] - q[0];
    let dz = p[2] - q[2];
    let nn = fmadds(n[0], n[0], n[1] * n[1]);
    let d = fmadds(dx, n[0], dy * n[1]);
    let nn = fmadds(n[2], n[2], nn);
    let d = fmadds(dz, n[2], d);
    let t = ppc_nan(d / nn, d, nn);
    let foot = [
        fnmsubs(t, n[0], p[0]),
        fnmsubs(t, n[1], p[1]),
        fnmsubs(t, n[2], p[2]),
    ];
    // r2-0x7E38 (0.0), r2-0x7E30/-0x7E28 (0.5, 3.0), r2-0x7E20 (1.0).
    let len = sqrtf_dol(nn);
    let inv = 1.0 / len;
    (ppc_nan(d * inv, d, inv), foot)
}

/// An invalid operation on two numbers (0 / 0, 0 * inf) gives PowerPC's
/// default NaN, 0x7FC00000 (x86 gives the negative one).
fn ppc_nan(r: f32, a: f32, b: f32) -> f32 {
    if r.is_nan() && !a.is_nan() && !b.is_nan() {
        f32::from_bits(0x7fc0_0000)
    } else {
        r
    }
}

/// 0x378C: the probe point and the land probe.
pub fn probe(pos: V3, ang: [i32; 3], st: &mut Movable, host: &mut impl MovableHost) {
    let mut m = IDENTITY;
    rotate(&mut m, Axis::Z, ang[2]);
    rotate(&mut m, Axis::X, ang[0]);
    rotate(&mut m, Axis::Y, ang[1]);
    st.probe_turned = mult_vec(&m, st.probe_offset);
    st.shadow.point = [
        pos[0] + st.probe_turned[0],
        pos[1] + st.probe_turned[1],
        pos[2] + st.probe_turned[2],
    ];
    if st.flags & F_WATER != 0 {
        if host.shadow_water_80066e1c(&mut st.shadow) {
            st.flags |= F_WATER_HIT;
        } else {
            st.flags &= !F_WATER_HIT;
        }
    } else {
        host.get_shadow_pos_xyz_80067210(&mut st.shadow);
    }
}

/// 0x38AC: the ceiling (ray 3).
pub fn ceiling(pos: &mut V3, speed: &mut V3, st: &mut Movable) {
    let top = st.shadow.point[1] + st.head;
    let rec = st.shadow.rays[3];
    st.flags &= !F_CEILING;
    if top > rec.coord && rec.normal[1] < ZERO {
        speed[1] = ZERO;
        pos[1] -= PUSH_OUT * (top - rec.coord);
        st.flags |= F_CEILING;
    }
}

/// 0x3924: the ground (ray 2).
pub fn ground(pos: &mut V3, speed: &mut V3, ang: &mut [i32; 3], st: &mut Movable) {
    let foot = st.shadow.point[1] + st.foot;
    let rec = st.shadow.rays[2];
    let f = st.flags;
    if f & F_FALL_BACK != 0 && f & F_FELL_BACK == 0 && (foot - st.step > rec.coord || !rec.hit) {
        *pos = st.last_pos;
        st.flags |= 0x140;
        pos[0] -= BACK_OFF * speed[0];
        pos[2] -= BACK_OFF * speed[2];
        *speed = [ZERO; 3];
        st.ground_normal = [ZERO, ONE, ZERO];
        return;
    }
    st.flags &= !F_GROUND;
    if foot < rec.coord && rec.normal[1] > ZERO {
        speed[1] *= -st.ground_bounce;
        speed[0] *= st.ground_drag;
        speed[2] *= st.ground_drag;
        pos[1] += PUSH_OUT * (rec.coord - foot);
        if st.flags & F_TILT != 0 {
            ang[0] = adjust_angle(ang[0], rec.angle_x, 0x80);
            ang[2] = adjust_angle(ang[2], rec.angle_z, 0x80);
        }
        st.ground_normal = rec.normal;
        st.flags |= F_GROUND;
        st.flags &= !F_FELL_BACK;
    } else {
        st.ground_normal = [ZERO, ONE, ZERO];
    }
}

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// The bounce off a wall: `speed += n * (-2 * dot(speed, n) * wall_bounce)`
/// and the yaw turn.
fn reflect(speed: &mut V3, ang: &mut [i32; 3], n: V3, st: &Movable) {
    let k = st.wall_bounce * (REFLECT * inner_product(*speed, n));
    let n = [n[0] * k, n[1] * k, n[2] * k];
    *speed = add(*speed, n);
    if outer_product(n, *speed)[1] > ZERO {
        ang[1] = ang[1].wrapping_sub(st.wall_turn);
    } else {
        ang[1] = ang[1].wrapping_add(st.wall_turn);
    }
}

/// 0x3B68: the walls (rays 0, 1, 4, 5).
pub fn walls(pos: &mut V3, speed: &mut V3, ang: &mut [i32; 3], st: &mut Movable) {
    let p = st.shadow.point;
    let radius = st.radius;
    let mut best: Option<(usize, V3, V3)> = None;
    let mut best_d = 0.0f32;
    st.flags &= !F_WALL;
    for (ray, dir) in WALL_RAYS {
        let rec = st.shadow.rays[ray];
        if !rec.hit {
            continue;
        }
        let dot = inner_product(dir, rec.normal);
        let facing = if st.flags & F_WALL_ANY_SIDE != 0 {
            dot.abs() > FACING
        } else {
            dot < FACING_NEG
        };
        if !facing {
            continue;
        }
        let q = if ray < 2 {
            [rec.coord, p[1], p[2]]
        } else {
            [p[0], p[1], rec.coord]
        };
        let (d, foot) = plane_distance_80012ea4(p, q, rec.normal);
        if d < ZERO && d.abs() < radius {
            // blt: a NaN nearest distance lets the new one in.
            if best.is_some() && best_d < d.abs() {
                continue;
            }
            if ray != 5 {
                best_d = d.abs();
            }
            best = Some((ray, foot, rec.normal));
            continue;
        }
        if best.is_some() {
            continue;
        }
        let gap = radius - d;
        // cror gt|eq: a NaN gap skips too.
        if gap.partial_cmp(&ZERO).is_none_or(|o| o.is_lt()) {
            continue;
        }
        let s = WALL_PUSH * gap;
        let push = [rec.normal[0] * s, ZERO * s, rec.normal[2] * s];
        *pos = add(*pos, push);
        st.flags |= F_WALL;
        if inner_product(*speed, rec.normal).partial_cmp(&ZERO) != Some(std::cmp::Ordering::Less) {
            continue;
        }
        let (e, n) = unit_vector_len(rec.normal);
        let n = if e == ZERO { [ONE, ZERO, ZERO] } else { n };
        reflect(speed, ang, n, st);
    }
    let Some((ray, foot, normal)) = best else {
        return;
    };
    *pos = foot;
    st.flags |= F_WALL;
    let (e, mut n) = unit_vector_len(normal);
    if e == ZERO {
        let (x, z) = match ray {
            0 => (ONE, ZERO),
            1 => (MINUS_ONE, ZERO),
            4 => (ZERO, ONE),
            _ => (ZERO, MINUS_ONE),
        };
        n = [x, ZERO, z];
    }
    reflect(speed, ang, n, st);
}

/// 0x491C: one physics step.
pub fn physics(
    pos: &mut V3,
    speed: &mut V3,
    ang: &mut [i32; 3],
    st: &mut Movable,
    host: &mut impl MovableHost,
) {
    probe(*pos, *ang, st, host);
    ceiling(pos, speed, st);
    ground(pos, speed, ang, st);
    walls(pos, speed, ang, st);
    st.last_pos = *pos;
}

fn still(v: V3) -> bool {
    v[0].abs() < STILL && v[1].abs() < STILL && v[2].abs() < STILL
}

/// 0x4BE0: the per-frame update.
///
/// The object is in contact when it has no collision work or
/// `CCL_IsPushed` (cwp flag 0x10) reports it. With twp flag 0x8000 clear:
///
/// * unless flag 0x80000000 is clear while 0x801CC19F is set (then only
///   the physics part below runs), and unless flag 0x8000 is set:
///   - on the ground (0x40): with flag 2 the speed is pushed by
///     [`ROLL_PUSH`] turned by the yaw and the rolling matrix (`njRotateY`,
///     `njMultiMatrix`, `njCalcVector`), away when it points up; the
///     rolling step matrix becomes `RotY(a) RotX(b) RotY(-a)`, with a the
///     speed's heading (`atan2f(x, z)` in BAMS, sec4 +0x258) less the yaw
///     and b `trunc(65536/360 * (360 * (|speed_xz| / 62.8)))` (the REL's
///     `sqrtf`); then the vertical speed becomes `|y|`, or 0 at or below
///     0.24;
///   - in the air: unless flag 0x10 is set, the speed is still (each
///     component below 0.001) and there is no contact, gravity takes 0.016
///     (flag 0x10000) or 0.08;
///   - the speed is scaled by 0.997; with flag 0x10 and the last update's
///     contact (+0x184 bit 0x8000) a still speed becomes (.., -0.264, ..);
///     a vertical speed beyond 2.45 is clamped to it; the position moves
///     by the speed (by half of it with flag 0x10000); with flag 2 the
///     rolling matrix is multiplied by the step;
/// * the physics step 0x491C runs unless flag 0x10 is set, the speed is
///   still and there is no contact; then `CCL_Entry`.
///
/// With twp flag 0x8000 set (the object is held), flag 0x80000000 is set
/// when 0x801CC19F is, the speed is zeroed, Rotation x and z become
/// `(short)(0.9 * (short)angle)`, flag 0x40 is cleared and both matrices
/// become the identity; no `CCL_Entry`.
///
/// Last, flag 0x80000000 is cleared when 0x801CC19F is 0, and +0x184
/// takes the twp flags, with bit 0x8000 for contact.
pub fn update(b: Body, st: &mut Movable, host: &mut impl MovableHost) {
    let Body {
        flag,
        ang,
        pos,
        speed,
        cwp_flag,
    } = b;
    let mut contact = false;
    let g = host.byte_801cc19f();
    if flag as u16 & 0x8000 == 0 {
        contact = cwp_flag.is_none_or(|f| f & 0x10 != 0);
        let f = st.flags;
        if f & F_KEEP_ACTIVE != 0 || g == 0 {
            if f & 0x8000 == 0 {
                if f & F_GROUND != 0 {
                    if f & F_ROLL != 0 {
                        roll(ang[1], speed, st);
                    }
                    speed[1] = speed[1].abs();
                    if speed[1] <= BOUNCE_STOP {
                        speed[1] = ZERO;
                    }
                } else if f & F_REST == 0 || !still(*speed) || contact {
                    speed[1] -= if st.flags & F_SLOW != 0 {
                        GRAVITY_SLOW
                    } else {
                        GRAVITY
                    };
                }
            }
            speed[0] *= DRAG;
            speed[1] *= DRAG;
            speed[2] *= DRAG;
            if st.flags & F_REST != 0 && st.task_flag as u16 & 0x8000 != 0 && still(*speed) {
                speed[1] = NUDGE;
            }
            let y = speed[1].abs();
            if y > MAX_FALL {
                speed[1] *= MAX_FALL / y;
            }
            if st.flags & F_SLOW != 0 {
                for k in 0..3 {
                    pos[k] += FACING * speed[k];
                }
            } else {
                *pos = add(*pos, *speed);
            }
            if st.flags & F_ROLL != 0 {
                st.roll = concat(&st.roll, &st.roll_step);
            }
        }
        if st.flags & F_REST == 0 || !still(*speed) || contact {
            physics(pos, speed, ang, st, host);
        }
        host.event(Event::CollisionEntry);
    } else {
        if g != 0 {
            st.flags |= F_KEEP_ACTIVE;
        }
        *speed = [ZERO; 3];
        let settle = |a: i32| fctiwz(f32::from(a as i16) * SETTLE) as i16 as i32;
        ang[0] = settle(ang[0]);
        ang[2] = settle(ang[2]);
        st.flags &= !F_GROUND;
        st.roll = IDENTITY;
        st.roll_step = IDENTITY;
    }
    if g == 0 {
        st.flags &= !F_KEEP_ACTIVE;
    }
    st.task_flag = flag;
    if contact {
        st.task_flag = (st.task_flag as u16 | 0x8000) as i16;
    }
}

/// 0x4C84..0x4DDC: the rolling push and step matrix.
fn roll(yaw: i32, speed: &mut V3, st: &mut Movable) {
    let mut m = IDENTITY;
    rotate(&mut m, Axis::Y, yaw);
    let m = concat(&m, &st.roll);
    let v = mult_vec_sr(&m, ROLL_PUSH);
    if v[1] > ZERO {
        speed[0] -= v[0];
        speed[2] -= v[2];
    } else {
        speed[0] += v[0];
        speed[2] += v[2];
    }
    let heading = fctiwz_f64(BAMS_PER_RADIAN * f64::from(atan2f(speed[0], speed[2])));
    let a = heading.wrapping_sub(yaw);
    let len = sqrtf_rel(speed[0] * speed[0] + speed[2] * speed[2]);
    let b = fctiwz(ROLL_SCALE * (DEGREES * (len / ROLL_CIRCUMFERENCE)));
    let mut m = IDENTITY;
    rotate(&mut m, Axis::Y, a);
    rotate(&mut m, Axis::X, b);
    rotate(&mut m, Axis::Y, a.wrapping_neg());
    st.roll_step = m;
}
