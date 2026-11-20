//! LIGHT SW (object list row 0) and MODMOD (row 1).
//!
//! # LIGHT SW
//!
//! stg42D.rel section 1: set-up 0x13C78, an empty display 0x13D74 and
//! delete 0x13D78, the player test 0x13D7C and the exec 0x14030.
//!
//! Set-up (0x13C78): unless `CheckRangeOut(task)` (0x800655F4) reports the
//! task out, it asks `syCalloc(1, 4)` (0x801197FC) for its work (task +0x40);
//! when that fails nothing else happens and the set-up runs again next
//! frame. Otherwise the low byte of Rotation.x becomes the light index
//! (twp +3), the low byte of Rotation.y the mode (twp +1, read as a signed
//! byte; 3 and above, or negative, become 0) and the low 2 bits of
//! Rotation.z the shape (twp +6, u16). Rotation.x is cleared, the low bytes
//! of Rotation.y and Rotation.z are cleared, and work bytes 0 and 1 are
//! cleared. Exec, display and delete become 0x14030, 0x13D74 and 0x13D78.
//!
//! The player test (0x13D7C, [`player_inside`]) reads `playertwp[p]`
//! (GSNE8P 0x801E7768), its collision work (+0x2C) and that work's first
//! collision info (+0xC); any null link means "not inside". With the info's
//! centre (+8) and sizes a (+0x14) and b (+0x18):
//! * the reach is, by shape, 0: `njScalor(Scale)` (0x801151F4); 1: Scale.x;
//!   2: the REL's own `sqrtf` (+0x74CC) of `Scale.x² + Scale.y²`; any other
//!   shape is never inside;
//! * `njDistanceP2P(centre, Position)` (0x8011B65C) above `reach + a` is
//!   outside; shape 1 (a sphere) is then inside;
//! * shape 0 (a box): the centre is taken into the switch's frame
//!   (`njPushMatrixEx`, `njUnitMatrix(0)`, `njTranslateEx(Position)`,
//!   `njRotateY(Rotation.y)`, `njRotateZ(Rotation.z)`, `njInvertMatrix(0)`,
//!   `njCalcPoint`, `njPopMatrixEx`); inside when each local coordinate is
//!   within `±(Scale + a)` of that axis;
//! * shape 2 (an upright cylinder): the same with `njRotateX(Rotation.x)`
//!   between Y and Z; inside when the local y is within `±(Scale.y + b)` and
//!   `a + sqrtf(x² + z²)` is not above Scale.x.
//!
//! The comparisons are the compiled ones, so a NaN passes the check it
//! reaches (the code only branches out on an ordered result).
//!
//! The exec (0x14030), after `CheckRangeOut`, by mode:
//! * 1: for player 0, then 1, while inside, `fn_80025028(light, p)`;
//! * 0 and 2: for player 0, then 1: when the player's flag (work byte p) is
//!   1 and the player is no longer inside, the flag drops to 0 and the
//!   player's light goes back: in mode 2 to the byte at GSNE8P 0x803AD927,
//!   otherwise to the one it saved (work byte 2 + p). When the flag is not 1
//!   and the player is inside, the player's current light (byte 0x803AD924 +
//!   p) is saved, `fn_80025028(light, p)` runs and the flag becomes 1;
//! * any other mode does nothing.
//!
//! `fn_80025028(value, p)` (main.dol, five instructions) stores the value's
//! low byte at `r13 - 0x799C + (p & 1)`, which is 0x803AD924 + (p & 1) with
//! GSNE8P's small-data base 0x803B52C0; [`PlayerLights::set`] is that port.
//!
//! # MODMOD
//!
//! stg42D.rel section 1: set-up 0x20310, delete 0x20398 (empty), exec
//! 0x2039C and display 0x203C4.
//!
//! Set-up: unless out of range, display, exec and delete become 0x203C4,
//! 0x2039C and 0x20398, and twp +6 (u16) becomes `(Rotation.z & 0xF) % 2`
//! (signed division as compiled; the operand is never negative). The exec
//! only runs `CheckRangeOut`. The display: `njPushMatrix(0)` (0x80115D60),
//! `njTranslate(0, Position)` (0x80115BD8), `njRotateX(0, Rotation.x)`
//! (0x80115C60), `njRotateY(0, Rotation.y)` (0x80115828), `njScale(0, 1 +
//! Scale)` (0x80115178; 1.0 from section 4 +0xA78), then `fn_8011E23C` with
//! the model of row twp +6 of the stage's 8-byte table (stg42D section 5
//! +0x21194, the model pointer at +4 of each row; stg42D's two rows point at
//! section 5 +0x20E78 and +0x20D10, their names at +0 start "hyoji_hpbar"),
//! and `njPopMatrix(1)` (0x80115DC8). In GSNE8P `fn_8011E23C` is
//! `li r3, 0; blr`: it draws nothing.
//!
//! # Shared code
//!
//! The player test calls the REL's local `sqrtf` copy (+0x74CC) with an
//! unrelocated branch, so its bytes differ between stage RELs in those two
//! branch words (0x13E30, 0x13FE8) only; [`ROUTINES`] lists it in three
//! pieces around them. The `sqrtf` copy is byte-identical to stg09D.rel
//! 0xBEDC, ported as `enemy_lib::engine::sqrtf_rel` and reused here, as are
//! `njScalor` and `njDistanceP2P` (`enemy_lib::engine`), the rotations and
//! `njCalcPoint` (`ninja_matrix`) and `njTranslateEx` (`enemy_lib::e_ai`).
use super::host::{ObjectHost, V3};
use crate::enemy_lib::{
    e_ai::translate,
    engine::{distance, scalor, sqrtf_rel},
};
use crate::ninja_matrix::{mult_vec, rotate, Axis, Mtx, IDENTITY};
use crate::ninja_trig::fmadds;

/// The routines this file ports (stg42D.rel section 1).
pub const ROUTINES: &[super::Routine] = &[
    super::Routine {
        name: "LIGHT SW set-up",
        stg42d: 0x13c78,
        len: 0xfc,
        masked_sha256: "13f2d00154df9a8850851afa84d6fcd168104b59f7c1d07df3a1a411476ca042",
    },
    super::Routine {
        name: "LIGHT SW display (empty)",
        stg42d: 0x13d74,
        len: 0x4,
        masked_sha256: "f332ea5b5437103cbb6f1508679da89eec9288ad775c96c439a17fccabe3de8e",
    },
    super::Routine {
        name: "LIGHT SW delete (empty)",
        stg42d: 0x13d78,
        len: 0x4,
        masked_sha256: "f332ea5b5437103cbb6f1508679da89eec9288ad775c96c439a17fccabe3de8e",
    },
    super::Routine {
        name: "LIGHT SW player test (to sqrtf)",
        stg42d: 0x13d7c,
        len: 0xb4,
        masked_sha256: "08f990e2c0ded907a621577618d40d82a7447ac06f2529e23e0015fccaaa3615",
    },
    super::Routine {
        name: "LIGHT SW player test (box, cylinder)",
        stg42d: 0x13e34,
        len: 0x1b4,
        masked_sha256: "61431e061326822b1189f239512875691ef3c9ee088745a9d4589518fc6dfda5",
    },
    super::Routine {
        name: "LIGHT SW player test (tail)",
        stg42d: 0x13fec,
        len: 0x44,
        masked_sha256: "8050c1db0b2905feb49dd19d4ccee710634304e1f03909bba3d730428bf835d5",
    },
    super::Routine {
        name: "LIGHT SW exec",
        stg42d: 0x14030,
        len: 0x188,
        masked_sha256: "1ede6b4e72b361553acd8a713eb72cd767e21bd346386dc73cf0bcc23381a90d",
    },
    super::Routine {
        name: "REL sqrtf (= stg09D 0xBEDC)",
        stg42d: 0x74cc,
        len: 0x8c,
        masked_sha256: "3749807ef3e3ec30dff3fb10159d8b89863bba289c739f54d255372957bc6af2",
    },
    super::Routine {
        name: "MODMOD set-up",
        stg42d: 0x20310,
        len: 0x88,
        masked_sha256: "fee0fa830c426caa5f88b4a78d1edbdef067eec5e2461c43838fe0113c578874",
    },
    super::Routine {
        name: "MODMOD delete (empty)",
        stg42d: 0x20398,
        len: 0x4,
        masked_sha256: "f332ea5b5437103cbb6f1508679da89eec9288ad775c96c439a17fccabe3de8e",
    },
    super::Routine {
        name: "MODMOD exec",
        stg42d: 0x2039c,
        len: 0x28,
        masked_sha256: "b54688cc8faf95704168ed005f832d19ea498fa1f2c119e62fb71da28762d286",
    },
    super::Routine {
        name: "MODMOD display",
        stg42d: 0x203c4,
        len: 0xac,
        masked_sha256: "24f02dd01efd0752893b5e26ee550035ba868289572ffea4a7d5a8e599fee981",
    },
];

/// MODMOD's scale base (stg42D section 4 +0xA78).
pub const MODMOD_SCALE_BASE: f32 = 1.0;
/// MODMOD's model table in stg42D (section 5 offset; 8-byte rows, the
/// model pointer at +4).
pub const STG42D_MODMOD_TABLE: u32 = 0x21194;

/// The first collision info of a player's collision work, as the player
/// test reads it (`playertwp[p]->cwp->info`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerColli {
    /// info +8.
    pub center: V3,
    /// info +0x14.
    pub a: f32,
    /// info +0x18.
    pub b: f32,
}

/// A rendering call MODMOD's display makes (GSNE8P routines; matrix 0 is
/// the current one).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Draw {
    /// njPushMatrix(0) (0x80115D60).
    PushMatrix,
    /// njTranslate(0, x, y, z) (0x80115BD8).
    Translate(V3),
    /// njRotateX(0, angle) (0x80115C60).
    RotateX(i32),
    /// njRotateY(0, angle) (0x80115828).
    RotateY(i32),
    /// njScale(0, x, y, z) (0x80115178).
    Scale(V3),
    /// fn_8011E23C(model): the model pointer (+4) of this row of the
    /// stage's MODMOD table (`li r3, 0; blr` in GSNE8P).
    Model { row: u16 },
    /// njPopMatrix(1) (0x80115DC8).
    PopMatrix,
}

/// What LIGHT SW and MODMOD ask of the game.
pub trait LightHost: ObjectHost {
    /// syCalloc(1, 4) (0x801197FC): false when it returns null.
    fn sy_calloc_work(&mut self) -> bool;
    /// `playertwp[p]` (0x801E7768) `->cwp (+0x2C) ->info (+0xC)`; `None`
    /// when any of the three is null.
    fn player_colli(&mut self, player: usize) -> Option<PlayerColli>;
    /// A rendering call, in order.
    fn draw(&mut self, d: Draw);
}

/// The per-player light bytes fn_80025028 writes (GSNE8P 0x803AD924 and
/// 0x803AD925) and the one LIGHT SW mode 2 restores (0x803AD927).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlayerLights {
    pub current: [u8; 2],
    pub default: u8,
}

impl PlayerLights {
    /// fn_80025028(value, p) (main.dol): byte `0x803AD924 + (p & 1)`.
    pub fn set(&mut self, value: u8, player: u32) {
        self.current[(player & 1) as usize] = value;
    }
}

/// Which callback the task runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// The object list function (set-up), run every frame until it
    /// installs the exec.
    Setup,
    /// The exec it installed.
    Main,
}

/// LIGHT SW's task state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightSwTask {
    pub phase: Phase,
    /// twp +1 (signed byte): the mode.
    pub mode: i8,
    /// twp +3: the light index it applies.
    pub light: u8,
    /// twp +6: the shape (0 box, 1 sphere, 2 cylinder).
    pub shape: u16,
    /// twp +8, +0xC, +0x10.
    pub ang: [i32; 3],
    /// twp +0x14.
    pub pos: V3,
    /// twp +0x20.
    pub scl: V3,
    /// The 4-byte work (task +0x40): per player an "inside" flag (bytes 0,
    /// 1) and the light it saved (bytes 2, 3).
    pub work: [u8; 4],
}

impl LightSwTask {
    /// A task as the SET loader starts it.
    pub fn new(ang: [i32; 3], pos: V3, scl: V3) -> Self {
        Self {
            phase: Phase::Setup,
            mode: 0,
            light: 0,
            shape: 0,
            ang,
            pos,
            scl,
            work: [0; 4],
        }
    }
}

/// One frame of LIGHT SW: the set-up (0x13C78) or the exec (0x14030).
pub fn light_sw_frame(
    task: &mut LightSwTask,
    lights: &mut PlayerLights,
    host: &mut impl LightHost,
) {
    match task.phase {
        Phase::Setup => light_sw_setup(task, host),
        Phase::Main => light_sw_exec(task, lights, host),
    }
}

/// 0x13C78.
pub fn light_sw_setup(task: &mut LightSwTask, host: &mut impl LightHost) {
    if host.range_out() || !host.sy_calloc_work() {
        return;
    }
    task.work = [0; 4];
    task.light = task.ang[0] as u8;
    task.mode = task.ang[1] as u8 as i8;
    if task.mode >= 3 || task.mode < 0 {
        task.mode = 0;
    }
    task.shape = (task.ang[2] & 3) as u16;
    task.ang[0] = 0;
    task.ang[1] &= !0xff;
    task.ang[2] &= !0xff;
    task.phase = Phase::Main;
}

/// 0x14030.
pub fn light_sw_exec(task: &mut LightSwTask, lights: &mut PlayerLights, host: &mut impl LightHost) {
    if host.range_out() {
        return;
    }
    match task.mode {
        1 => {
            for p in 0..2 {
                if player_inside(task, p, host) {
                    lights.set(task.light, p as u32);
                }
            }
        }
        0 | 2 => {
            for p in 0..2 {
                if task.work[p] as i8 == 1 {
                    if !player_inside(task, p, host) {
                        task.work[p] = 0;
                        let back = if task.mode == 2 {
                            lights.default
                        } else {
                            task.work[p + 2]
                        };
                        lights.set(back, p as u32);
                    }
                } else if player_inside(task, p, host) {
                    task.work[p + 2] = lights.current[p];
                    lights.set(task.light, p as u32);
                    task.work[p] = 1;
                }
            }
        }
        _ => {}
    }
}

fn fmsubs(a: f32, c: f32, b: f32) -> f32 {
    fmadds(a, c, -b)
}

fn fnmadds(a: f32, c: f32, b: f32) -> f32 {
    -fmadds(a, c, b)
}

/// PSMTXInverse (0x800ED484) in place, as njInvertMatrix(0) (0x80114FF8)
/// calls it. The same paired-single port as `land_contact`'s (private
/// there): a zero determinant leaves the matrix alone; the reciprocal is
/// Gekko's `fres` estimate and one Newton step.
fn invert(m: &mut Mtx) {
    let [[m00, m01, m02, m03], [m10, m11, m12, m13], [m20, m21, m22, m23]] = *m;
    let a0 = fmsubs(m11, m22, m21 * m12);
    let a1 = fmsubs(m12, m20, m22 * m10);
    let b0 = fmsubs(m21, m02, m01 * m22);
    let b1 = fmsubs(m22, m00, m02 * m20);
    let c0 = fmsubs(m01, m12, m11 * m02);
    let c1 = fmsubs(m02, m10, m12 * m00);
    let d0 = fmsubs(m10, m21, m11 * m20);
    let e0 = fmsubs(m01, m20, m00 * m21);
    let f0 = fmsubs(m00, m11, m01 * m10);
    let det = fmadds(m20, c0, fmadds(m10, b0, m00 * a0));
    if det == 0.0 {
        return;
    }
    let r = crate::msl_math::gekko_fres(det as f64) as f32;
    let inv = -fmsubs(det, r * r, r + r);
    let row = |a: f32, b: f32, c: f32| {
        let (a, b, c) = (a * inv, b * inv, c * inv);
        [a, b, c, fnmadds(c, m23, fmadds(b, m13, a * m03))]
    };
    *m = [row(a0, b0, c0), row(a1, b1, c1), row(d0, e0, f0)];
}

/// The centre in the switch's frame: njUnitMatrix, njTranslateEx(Position),
/// the rotations in order, njInvertMatrix(0), njCalcPoint. The push and pop
/// around it leave the matrix stack as it was.
fn local_point(task: &LightSwTask, rotations: &[(Axis, i32)], v: V3) -> V3 {
    let mut m = IDENTITY;
    translate(&mut m, task.pos);
    for &(axis, angle) in rotations {
        rotate(&mut m, axis, angle);
    }
    invert(&mut m);
    mult_vec(&m, v)
}

/// Outside `±(size + pad)`, compiled as `v > size + pad` or
/// `v < -size - pad` (unordered: not outside).
fn outside(v: f32, size: f32, pad: f32) -> bool {
    v > size + pad || v < -size - pad
}

/// 0x13D7C: whether player `p` (0 or 1) is inside the switch.
pub fn player_inside(task: &LightSwTask, p: usize, host: &mut impl LightHost) -> bool {
    let Some(c) = host.player_colli(p) else {
        return false;
    };
    let s = task.scl;
    let reach = match task.shape {
        0 => scalor(s),
        1 => s[0],
        2 => sqrtf_rel(s[0] * s[0] + s[1] * s[1]),
        _ => return false,
    };
    if distance(c.center, task.pos) > reach + c.a {
        return false;
    }
    match task.shape {
        1 => true,
        0 => {
            let l = local_point(
                task,
                &[(Axis::Y, task.ang[1]), (Axis::Z, task.ang[2])],
                c.center,
            );
            (0..3).all(|k| !outside(l[k], s[k], c.a))
        }
        _ => {
            let l = local_point(
                task,
                &[
                    (Axis::Y, task.ang[1]),
                    (Axis::X, task.ang[0]),
                    (Axis::Z, task.ang[2]),
                ],
                c.center,
            );
            if outside(l[1], s[1], c.b) {
                return false;
            }
            let r = sqrtf_rel(l[0] * l[0] + l[2] * l[2]);
            // `fcmpo` then `ble`: unordered counts as not above.
            (c.a + r).partial_cmp(&s[0]) != Some(std::cmp::Ordering::Greater)
        }
    }
}

/// MODMOD's task state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModModTask {
    pub phase: Phase,
    /// twp +6: the model table row.
    pub row: u16,
    /// twp +8, +0xC, +0x10.
    pub ang: [i32; 3],
    /// twp +0x14.
    pub pos: V3,
    /// twp +0x20.
    pub scl: V3,
}

impl ModModTask {
    /// A task as the SET loader starts it.
    pub fn new(ang: [i32; 3], pos: V3, scl: V3) -> Self {
        Self {
            phase: Phase::Setup,
            row: 0,
            ang,
            pos,
            scl,
        }
    }
}

/// One exec frame: the set-up (0x20310) or the exec (0x2039C). Both start
/// with `CheckRangeOut`.
pub fn modmod_frame(task: &mut ModModTask, host: &mut impl LightHost) {
    if host.range_out() {
        return;
    }
    if task.phase == Phase::Setup {
        task.phase = Phase::Main;
        let v = task.ang[2] & 0xf;
        task.row = (v % 2) as u16;
    }
}

/// The display (0x203C4), which the set-up installs.
pub fn modmod_display(task: &ModModTask, host: &mut impl LightHost) {
    host.draw(Draw::PushMatrix);
    host.draw(Draw::Translate(task.pos));
    host.draw(Draw::RotateX(task.ang[0]));
    host.draw(Draw::RotateY(task.ang[1]));
    host.draw(Draw::Scale(task.scl.map(|s| MODMOD_SCALE_BASE + s)));
    host.draw(Draw::Model { row: task.row });
    host.draw(Draw::PopMatrix);
}
