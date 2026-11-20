//! The walker's collision helpers under fn_8003EAFC and fn_80043ED4
//! (GSNE8P main.dol), ported instruction by instruction and checked against
//! main.dol by tests/walker_collide.rs.
// Negated comparisons keep the compiled branches' NaN behaviour.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::{
    land_contact::{unit_vector, ContactHit},
    land_nodes::nj_scalor,
    land_select::inline_sqrt,
    msl_math::{atan2f, gekko_fres, gekko_frsqrte},
    ninja_matrix::{rotate_by_angles, unrotate_by_angles, Axis, MatrixStack, Vec3},
    ninja_trig::{fmadds, fnmsubs, nj_cos},
    ninja_vector::{inner_product, outer_product, vector_angle},
};

const ZERO: f32 = 0.0; // r2-0x75F8
/// Radians to BAMS (double at r2-0x75E8).
const BAMS_PER_RAD: f64 = f64::from_bits(0x40c4_5f30_b217_88bb);
const LIFT_FALLING: f32 = f32::from_bits(0x3f19_999a); // r2-0x7514, 0.6
const LIFT_RISING: f32 = f32::from_bits(0x3e4c_cccd); // r2-0x7590, 0.2
const LIFT_SLOW: f32 = f32::from_bits(0x3e99_999a); // r2-0x7580, 0.3
const LIFT_STEEP_DOWN: f32 = f32::from_bits(0xbf19_999a); // r2-0x74F4, -0.6
const LIFT_STEEP_SCALE: f32 = f32::from_bits(0x3fe6_6666); // r2-0x74F0, 1.8
const LIFT_SLOPE: f32 = f32::from_bits(0x3d4c_cccd); // r2-0x74EC, 0.05
const LIFT_SLOPE_SCALE: f32 = f32::from_bits(0x3fd3_3333); // r2-0x74E8, 1.65
const LIFT_FLAT_SCALE: f32 = f32::from_bits(0x3fb3_3333); // r2-0x74E4, 1.4
const LIFT_DOWNHILL: f32 = f32::from_bits(0xbdcc_cccd); // r2-0x75A0, -0.1
const LIFT_LEVEL_Y: f32 = 0.25; // r2-0x7574
const LIFT_LEVEL_SCALE: f32 = 1.5; // r2-0x74FC
const HALF: f32 = 0.5; // r2-0x75CC

/// The inputs fn_800428F8 reads.
#[derive(Debug, Clone, Copy)]
pub struct LiftInput {
    /// The motion this frame (r3).
    pub motion: Vec3,
    /// Player work +0xC4 (PhysicsArray +4): the collision height.
    pub height: f32,
    /// `last_attr` (+0x8C).
    pub last_attr: u32,
    /// `spd.x`, `spd.y` (+0x64, +0x68).
    pub spd_x: f32,
    pub spd_y: f32,
    /// Player work halfword +0x1C (status bits).
    pub status: i16,
    /// PhysicsArray `run_speed` (+0xE8) and the speed at +0xF4.
    pub run_speed: f32,
    pub speed_f4: f32,
    /// The collision struct's up vector (+0x1C).
    pub up: Vec3,
    /// The global vector at 0x801E5624.
    pub global: Vec3,
}

/// fn_800428F8: the collision sphere's offset from the player, given the
/// offset's current y (`out_y`). Returns (0, y, 0): the routine zeroes x and
/// z and lowers y by a share of the height that depends on the floor, the
/// slope and the speed.
pub fn sphere_lift(i: &LiftInput, out_y: f32) -> Vec3 {
    let mut h = i.height;
    let out = |y: f32| [ZERO, y, ZERO];
    let ceiling = i.last_attr & 0x100 != 0;
    if ceiling && i.motion[1] <= ZERO {
        return out(fnmsubs(LIFT_FALLING, h, out_y));
    }
    if ceiling && i.motion[1] > ZERO {
        return out(fnmsubs(LIFT_RISING, h, out_y));
    }
    if !(i.spd_y <= ZERO) {
        if i.status & 0x2000 != 0 {
            return out(out_y - h);
        }
        return out(out_y);
    }
    if i.last_attr & 0x8 != 0 {
        h *= LIFT_SLOW;
    } else {
        let d = -inner_product(i.global, i.up);
        if i.status & 0x2000 != 0 {
            h *= if d <= LIFT_STEEP_DOWN {
                LIFT_STEEP_SCALE
            } else if d <= LIFT_SLOPE {
                LIFT_SLOPE_SCALE
            } else {
                LIFT_FLAT_SCALE
            };
        } else if (i.spd_x >= i.speed_f4 && i.status & 3 != 0) || d <= LIFT_DOWNHILL {
            // Unchanged.
        } else if d > LIFT_DOWNHILL && d < LIFT_RISING {
            let ty = if nj_scalor(i.motion) == ZERO {
                ZERO
            } else {
                unit_vector(i.motion)[1]
            };
            h *= if ty.abs() < LIFT_LEVEL_Y {
                LIFT_LEVEL_SCALE
            } else {
                LIFT_SLOW
            };
        } else if i.spd_x <= i.run_speed {
            h *= HALF;
        } else if i.status & 3 != 0 {
            h *= i.spd_x / i.speed_f4;
        }
    }
    out(out_y - h)
}

const SPECIAL_WEIGHT: f32 = f32::from_bits(0x3d8f_5c29); // r2-0x7578, 0.07
const WATER_WEIGHT: f32 = f32::from_bits(0x3ee6_6666); // r2-0x7500, 0.45
const WATER_RUN_ACCEL: f32 = f32::from_bits(0x3f26_6666); // r2-0x74F8, 0.65
const SINK_F118: f32 = 2.0; // r2-0x75FC
const DRAG_LOW: f32 = f32::from_bits(0x3e19_999a); // r2-0x750C, 0.15
const DRAG_MID: f32 = f32::from_bits(0x3dcc_cccd); // r2-0x75A8, 0.1
const ONE: f32 = 1.0; // r2-0x75C8
/// Stages whose numbers give characters 0, 1 and 4 to 7 the weight 0.07.
pub const LIGHT_STAGES: [i32; 7] = [30, 32, 40, 43, 44, 50, 53];

/// The PhysicsArray fields (rows at 0x80196578) fn_800420C0 copies.
#[derive(Debug, Clone, Copy)]
pub struct PhysicsRow {
    /// Row +0x3C (`run_accel`).
    pub run_accel: f32,
    /// Row +0x58.
    pub f58: f32,
    /// Row +0x74 (the small-body height limit for characters 0 and 1).
    pub f74: f32,
    /// Row +0x78 (`weight`).
    pub weight: f32,
}

/// The player-work fields fn_800420C0 reads and writes.
#[derive(Debug, Clone, Copy)]
pub struct SurfaceWork {
    /// Byte +1: the character (PhysicsArray row).
    pub character: i8,
    /// Byte +3: the row for `run_accel`.
    pub accel_row: i8,
    /// +0x88.
    pub attr: u32,
    /// +0x40.
    pub f40: f32,
    /// +0xFC, +0x118, +0x138.
    pub run_accel: f32,
    pub f118: f32,
    pub weight: f32,
    /// +0x134 (PhysicsArray +0x74).
    pub f134: f32,
    /// +0x1A8 and +0x1B0, as in `walker_frame::SpeedWork`.
    pub flags_1a8: u32,
    pub height_1b0: f32,
}

/// fn_800420C0(task, motion, player, attr): add `attr` to the player's
/// `attr`, reload `weight`, +0x118 and `run_accel` from the PhysicsArray
/// (scaled under water, flag 0x2 of +0x1A8, or in the 0x2000 medium while
/// the surface at +0x1B0 is above the player), and set +0x40 from `attr`.
pub fn apply_surface(w: &mut SurfaceWork, pos_y: f32, attr: u32, stage: i32, rows: &[PhysicsRow]) {
    w.attr |= attr;
    let row = |i: i8| rows[i as usize];
    let weight = || {
        let c = w.character;
        if LIGHT_STAGES.contains(&stage) && matches!(c, 0 | 1 | 4..=7) {
            SPECIAL_WEIGHT
        } else {
            row(c).weight
        }
    };
    let water = w.flags_1a8 & 2 != 0;
    if water && w.height_1b0 > pos_y + w.f134 {
        w.weight = weight();
        w.weight *= if w.character == 8 { HALF } else { WATER_WEIGHT };
        w.f118 = LIFT_LEVEL_SCALE * row(w.character).f58;
        w.run_accel = WATER_RUN_ACCEL * row(w.accel_row).run_accel;
    } else if w.flags_1a8 & 0x2000 != 0 && w.height_1b0 > pos_y + w.f134 {
        w.weight = weight();
        w.weight *= LIFT_FALLING;
        w.f118 = SINK_F118 * row(w.character).f58;
        w.run_accel = WATER_WEIGHT * row(w.accel_row).run_accel;
    } else if !water {
        w.weight = weight();
        w.f118 = row(w.character).f58;
        w.run_accel = row(w.accel_row).run_accel;
    }
    w.f40 = if attr & 0x8 != 0 {
        DRAG_LOW
    } else if attr & 0x4 != 0 {
        DRAG_MID
    } else if attr & 0x10 != 0 {
        HALF
    } else if attr & 0x0020_0000 != 0 {
        LIFT_LEVEL_SCALE
    } else {
        ONE
    };
}

/// fn_80027C2C: the distance between two points (inline Gekko square root).
pub fn point_distance(a: Vec3, b: Vec3) -> f32 {
    let dy = a[1] - b[1];
    let dx = a[0] - b[0];
    let dz = a[2] - b[2];
    sqrtf(fmadds(dz, dz, fmadds(dx, dx, dy * dy)))
}

const NEAR_LIMIT: f32 = 1000.0; // r2-0x7554
const WALL_FACING: f32 = f32::from_bits(0xbe4c_cccd); // r2-0x7568, -0.2
/// VectorAngle above which a hit is not floor (when the global vector also
/// says so).
const FLOOR_ANGLE: i32 = 0x2c00;
const WALL_ANGLE: i32 = 0x7000;

/// What fn_800414F0 / fn_80041984 need besides the hits.
#[derive(Debug, Clone, Copy)]
pub struct SlideInput {
    /// Task byte +0 (the mode).
    pub task_mode: i8,
    /// Player byte +1 and +0x134.
    pub character: i8,
    pub f134: f32,
    /// PhysicsArray rows 0 and 1, +0x74.
    pub row0_74: f32,
    pub row1_74: f32,
    /// Whether the land entry has a task (dynamic land).
    pub entry_has_task: bool,
    /// Query center (+4) and up vector (+0x1C).
    pub center: Vec3,
    pub up: Vec3,
    /// The global vector at 0x801E5624.
    pub global: Vec3,
}

/// The outcome: the return value and whether r13-0x78F4 was set to 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlideResult {
    pub hit: bool,
    pub stuck: bool,
}

fn is_floor(i: &SlideInput, h: &ContactHit) -> (bool, i32) {
    let ang = vector_angle(i.up, h.facing, None);
    if ang <= FLOOR_ANGLE {
        return (true, ang);
    }
    let gf = inner_product(i.global, h.facing);
    (inner_product(i.global, i.up) > gf, ang)
}

/// The shared tail: mark the stuck flag on two or more distinct walls, stop
/// a small character pressed by static walls, else clamp the motion to the
/// nearest hit's distance.
fn finish(
    i: &SlideInput,
    walls: i32,
    near: Option<f32>,
    clamp_on: bool,
    motion: &mut Vec3,
    vel: &mut Vec3,
) -> SlideResult {
    let mut r = SlideResult {
        hit: false,
        stuck: false,
    };
    if walls >= 2 && !(matches!(i.character, 0 | 1) && (0x4c..=0x52).contains(&i.task_mode)) {
        r.stuck = true;
    }
    let small =
        (i.character == 0 && i.f134 < i.row0_74) || (i.character == 1 && !(i.f134 >= i.row1_74));
    if walls > 0 && small && !i.entry_has_task {
        *motion = [ZERO; 3];
        *vel = [ZERO; 3];
        r.hit = true;
        return r;
    }
    if clamp_on {
        if let Some(d) = near {
            if d < nj_scalor(*motion) {
                *motion = if nj_scalor(*motion) == ZERO {
                    [ZERO; 3]
                } else {
                    unit_vector(*motion).map(|c| c * d)
                };
                r.hit = true;
            }
        }
    }
    r
}

/// fn_800414F0 (grounded): sort this land entry's contact hits into floor
/// and wall hits; two distinct walls set the stuck flag; the motion (+0x10
/// of the query) is cut to the nearest floor hit's head distance.
/// `wall_near_in` is the caller's f26, which the routine compares against
/// before ever setting it.
pub fn slide_grounded(
    i: &SlideInput,
    hits: &[ContactHit],
    motion: &mut Vec3,
    vel: &mut Vec3,
    wall_near_in: f32,
) -> SlideResult {
    let mut floor_near = NEAR_LIMIT;
    let mut wall_near = wall_near_in;
    let (mut floors, mut walls) = (0, 0);
    let (mut floor_at, mut wall_at): (Option<usize>, Option<usize>) = (None, None);
    let mut dists = vec![];
    for (k, h) in hits.iter().enumerate() {
        let d = point_distance(h.head, i.center);
        dists.push(d);
        if is_floor(i, h).0 {
            floors += 1;
            if floor_near > d {
                floor_near = d;
                floor_at = Some(k);
            }
            continue;
        }
        let t = if nj_scalor(*motion) == ZERO {
            *motion
        } else {
            unit_vector(*motion)
        };
        if inner_product(t, h.facing) < WALL_FACING {
            let distinct = match wall_at {
                None => true,
                Some(w) => !(inner_product(h.facing, hits[w].facing) >= ONE),
            };
            if distinct {
                walls += 1;
            }
            if wall_near > d {
                wall_near = d;
                wall_at = Some(k);
            }
        }
    }
    finish(
        i,
        walls,
        floor_at.map(|k| dists[k]),
        floors > 0,
        motion,
        vel,
    )
}

/// fn_80041984 (airborne): like `slide_grounded`, but a wall is a non-floor
/// hit within 0x7000 of the up vector that the motion runs into, and the
/// motion is cut to the nearest wall hit.
pub fn slide_airborne(
    i: &SlideInput,
    hits: &[ContactHit],
    motion: &mut Vec3,
    vel: &mut Vec3,
) -> SlideResult {
    let mut near = NEAR_LIMIT;
    let mut walls = 0;
    let mut at: Option<usize> = None;
    let mut dists = vec![];
    for (k, h) in hits.iter().enumerate() {
        let d = point_distance(h.head, i.center);
        dists.push(d);
        let (floor, ang) = is_floor(i, h);
        if floor {
            continue;
        }
        let t = if nj_scalor(*motion) == ZERO {
            *motion
        } else {
            unit_vector(*motion)
        };
        if ang < WALL_ANGLE && inner_product(t, h.facing) < ZERO {
            let distinct = match at {
                None => true,
                Some(w) => !(inner_product(h.facing, hits[w].facing) >= ONE),
            };
            if distinct {
                walls += 1;
            }
            if near > d {
                near = d;
                at = Some(k);
            }
        }
    }
    if walls == 0 {
        return SlideResult {
            hit: false,
            stuck: false,
        };
    }
    finish(i, walls, at.map(|k| dists[k]), true, motion, vel)
}

/// A plane as the Ninja helpers at 0x8011B6C8.. take it: a point and a
/// normal (not necessarily unit).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plane {
    pub point: Vec3,
    pub normal: Vec3,
}

/// A line: a point and a direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Line {
    pub point: Vec3,
    pub dir: Vec3,
}

/// MSL `sqrtf` (0x8002F15C): the inline Gekko square root of positive
/// values; anything else is returned as it is.
pub fn sqrtf(x: f32) -> f32 {
    if x > ZERO {
        inline_sqrt(x)
    } else {
        x
    }
}

const PARALLEL_COS: f32 = f32::from_bits(0x3f79_999a); // r2-0x5578, 0.975

fn plain_dot(a: Vec3, b: Vec3) -> f32 {
    let z = a[2] * b[2];
    let x = a[0] * b[0];
    let y = a[1] * b[1];
    z + (x + y)
}

/// fn_8011BDB4: whether two planes are within acos(0.975) of parallel.
pub fn planes_parallel(a: &Plane, b: &Plane) -> bool {
    let lb = sqrtf(plain_dot(b.normal, b.normal));
    let la = sqrtf(plain_dot(a.normal, a.normal));
    let cos = plain_dot(a.normal, b.normal) / (la * lb);
    cos.abs() > PARALLEL_COS
}

/// fn_8011B6C8: the distance from `p` to the plane, and the foot of the
/// perpendicular when asked.
pub fn plane_distance(p: Vec3, plane: &Plane, foot: Option<&mut Vec3>) -> f32 {
    let n = plane.normal;
    let d0 = -plain_dot(n, plane.point);
    let s = d0 + plain_dot(n, p);
    let nn = plain_dot(n, n);
    if let Some(out) = foot {
        let t = -s / nn;
        *out = [p[0] + n[0] * t, p[1] + n[1] * t, p[2] + n[2] * t];
    }
    let inv = ONE / sqrtf(nn);
    s.abs() * inv
}

/// fn_8011B7B8: the line where two planes meet. Parallel planes give a zero
/// line and their distance; otherwise the return value is 0.
pub fn plane_intersection(a: &Plane, b: &Plane, line: Option<&mut Line>) -> f32 {
    if planes_parallel(a, b) {
        if let Some(l) = line {
            *l = Line {
                point: [ZERO; 3],
                dir: [ZERO; 3],
            };
        }
        return plane_distance(a.point, b, None);
    }
    let Some(l) = line else {
        return ZERO;
    };
    let (ax, ay, az) = (a.normal[0], a.normal[1], a.normal[2]);
    let (bx, by, bz) = (b.normal[0], b.normal[1], b.normal[2]);
    let dx = ay * bz - by * az;
    let dy = az * bx - bz * ax;
    let dz = ax * by - bx * ay;
    let da = plain_dot(a.normal, a.point);
    let db = plain_dot(b.normal, b.point);
    let dl = plain_dot([dx, dy, dz], b.point);
    let f13 = az * dy;
    let f9 = ax * by;
    let f12 = ay * bz;
    let f31 = az * by;
    let f10 = ay * bx;
    let f11 = ax * dy;
    let det = (((bx * f13 + (dz * f9 + dx * f12)) - dx * f31) - dz * f10) - bz * f11;
    let inv = ONE / det;
    let f6 = da * dy;
    let f1 = ay * db;
    let f25 = da * by;
    let z = ((((bx * f6) + ((dl * f9) + (dx * f1))) - dx * f25) - dl * f10) - db * f11;
    let x = ((((db * f13) + ((dz * f25) + (dl * f12))) - dl * f31) - dz * f1) - bz * f6;
    let y = ((((bx * (az * dl)) + ((dz * (ax * db)) + (dx * (da * bz)))) - dx * (az * db))
        - dz * (da * bx))
        - bz * (ax * dl);
    *l = Line {
        point: [x * inv, y * inv, z * inv],
        dir: [dx, dy, dz],
    };
    ZERO
}

const WALL_PUSH: f32 = f32::from_bits(0xc090_0000); // r2-0x7508, -4.5

/// What fn_80040ACC reads besides the hits.
#[derive(Debug, Clone, Copy)]
pub struct WallSlideInput {
    pub slide: SlideInput,
    /// The land entry's flags (0x08000000 exempts it from the small-body stop).
    pub entry_flags: u32,
    /// `last_attr` (+0x8C).
    pub last_attr: u32,
    /// Query motion (+0x10), head average (+0x60) and the best hit's motion
    /// across the plane (+0x78).
    pub motion: Vec3,
    pub head: Vec3,
    pub across: Vec3,
}

/// fn_80040ACC: slide along a wall. Among the hits that face against the
/// motion within 0x7000 of the up vector (by more than cos 0x600, or with a
/// motion component below -4.5), take the nearest; when it is nearer than
/// the motion is long, turn the query's along-plane motion (+0x6C, plus
/// +0x78) into the direction where that wall meets the plane through the
/// head point across the motion, keeping the speed of `vel`. Writes the
/// wall normal (`wall`, player work +0x70). True when it slid.
pub fn wall_slide(
    i: &WallSlideInput,
    hits: &[ContactHit],
    along: &mut Vec3,
    vel: &mut Vec3,
    wall: &mut Vec3,
) -> bool {
    let s = &i.slide;
    let mut near = NEAR_LIMIT;
    let mut walls = 0;
    let mut at: Option<usize> = None;
    let mut dists = vec![];
    for (k, h) in hits.iter().enumerate() {
        let d = point_distance(h.head, s.center);
        dists.push(d);
        let ang = vector_angle(s.up, h.facing, None);
        if ang <= FLOOR_ANGLE || !(s.up[1] >= h.facing[1]) {
            continue;
        }
        let t = if nj_scalor(i.motion) == ZERO {
            i.motion
        } else {
            unit_vector(i.motion)
        };
        if ang >= WALL_ANGLE {
            continue;
        }
        let into = inner_product(t, h.facing);
        if !(into < ZERO) {
            continue;
        }
        let raw = inner_product(i.motion, h.facing);
        if !(into < -nj_cos(0x600) || raw < WALL_PUSH) {
            continue;
        }
        walls += 1;
        if near > d {
            near = d;
            at = Some(k);
        }
    }
    if walls == 0 {
        return false;
    }
    let len = nj_scalor(i.motion);
    let small =
        (s.character == 0 && s.f134 < s.row0_74) || (s.character == 1 && !(s.f134 >= s.row1_74));
    if small && i.entry_flags & 0x0800_0000 == 0 {
        *along = [ZERO; 3];
        *vel = [ZERO; 3];
        return false;
    }
    let Some(k) = at else {
        return false;
    };
    if !(dists[k] < len) || i.last_attr & 0x100 != 0 {
        return false;
    }
    if nj_scalor(*along) == ZERO {
        *along = [ZERO; 3];
        return false;
    }
    *along = [
        along[0] + i.across[0],
        along[1] + i.across[1],
        along[2] + i.across[2],
    ];
    let h = &hits[k];
    *wall = h.facing;
    if nj_scalor(vel.map(|c| -c)) == ZERO {
        return false;
    }
    let cross = outer_product(*along, *wall);
    let cross = if nj_scalor(cross) == ZERO {
        s.up
    } else {
        unit_vector(cross)
    };
    let mut line = Line {
        point: [ZERO; 3],
        dir: [ZERO; 3],
    };
    plane_intersection(
        &Plane {
            point: h.foot,
            normal: h.facing,
        },
        &Plane {
            point: i.head,
            normal: cross,
        },
        Some(&mut line),
    );
    let u = if nj_scalor(*along) == ZERO {
        *along
    } else {
        unit_vector(*along)
    };
    if inner_product(line.dir, u) < ZERO {
        line.dir = line.dir.map(|c| -c);
    }
    let speed = nj_scalor(*vel);
    *vel = line.dir.map(|c| c * speed);
    *along = *vel;
    true
}

/// DiffAngle, SubAngle and AdjustAngle (0x8002B8E0, 0x8002B904, 0x8002B918)
/// live in `player_turn`, checked against main.dol there.
pub use crate::player_turn::{adjust_angle, diff_angle, sub_angle};

/// The player-work speeds fn_80042694 compares `spd.x` with.
#[derive(Debug, Clone, Copy)]
pub struct TiltSpeeds {
    pub spd_x: f32,
    /// PhysicsArray `jog_speed`, `run_speed` (+0xE4, +0xE8), +0xEC and +0xF4.
    pub jog: f32,
    pub run: f32,
    pub f_ec: f32,
    pub f_f4: f32,
}

/// fn_80042694: tilt the task's X and Z rotation toward the contact's
/// angles (query +0x34, +0x38). It snaps when the surface faces up less
/// than the player's up vector does, when the player is not on the ground
/// (status bit 1) or is in the 0x2000 state; otherwise it turns by at most
/// the slope's own angle (from the facing in the player's frame), capped by
/// speed, or 0x400 with flag 0x00100000.
#[allow(clippy::too_many_arguments)]
pub fn tilt_to_contact(
    stack: &mut MatrixStack,
    task_angles: &mut [i32; 3],
    status: i16,
    speeds: &TiltSpeeds,
    contact_angles: (i32, i32),
    facing: Vec3,
    up: Vec3,
    global: Vec3,
    flags: u32,
) {
    let gf = inner_product(global, facing);
    if inner_product(global, up) > gf || status & 1 == 0 || status & 0x2000 != 0 {
        task_angles[0] = contact_angles.0;
        task_angles[2] = contact_angles.1;
        return;
    }
    stack.push_unit();
    stack.rotate(Axis::Y, task_angles[1]);
    stack.rotate(Axis::X, task_angles[0].wrapping_neg());
    stack.rotate(Axis::Z, task_angles[2].wrapping_neg());
    let v = stack.calc_vector(facing);
    stack.pop();
    let h = sqrtf(v[2] * v[2] + v[0] * v[0]);
    let mut ang = ((BAMS_PER_RAD * atan2f(h, v[1]) as f64) as i32) & 0xffff;
    if ang > 0x8000 {
        ang = 0x10000 - ang;
    }
    let s = speeds;
    let step = if flags & 0x0010_0000 != 0 {
        0x400
    } else if s.spd_x >= s.f_f4 {
        ang
    } else if s.spd_x >= s.f_ec {
        ang.min(0x2000)
    } else if s.spd_x >= s.run {
        ang.min(0x1500)
    } else if s.spd_x >= s.jog {
        ang.min(0x1000)
    } else {
        0x400
    };
    task_angles[0] = adjust_angle(task_angles[0], contact_angles.0, step);
    task_angles[2] = adjust_angle(task_angles[2], contact_angles.1, step);
}

const SLIDE_MIN: f32 = f32::from_bits(0x3ecc_cccd); // r2-0x7518, 0.4
const SLIDE_DAMP: f32 = f32::from_bits(0x3f75_c28f); // r2-0x7510, 0.96

/// The motion work (`playermwp[pnum]`, 0x801E7748) fields these read.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionWork {
    /// +0: the motion vector.
    pub spd: Vec3,
    /// +0x1C: the aim angle.
    pub ang_aim: i32,
}

/// The query fields the slide helpers read and write.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlideQuery {
    /// +4 and +0x10.
    pub center: Vec3,
    pub motion: Vec3,
    /// +0x1C.
    pub up: Vec3,
    /// +0x48: the best hit's facing.
    pub facing: Vec3,
    /// +0x60: the averaged head point.
    pub head: Vec3,
    /// +0x6C: the motion along the contact plane.
    pub along: Vec3,
}

/// Whether `njOuterProduct`'s returned length is zero: `fres(frsqrte(s))`
/// where s is x*x (single) then double fused adds of y*y and z*z.
fn outer_is_zero(c: Vec3) -> bool {
    let s = (c[1] as f64).mul_add(c[1] as f64, (c[0] * c[0]) as f64);
    let s = (c[2] as f64).mul_add(c[2] as f64, s);
    gekko_fres(gekko_frsqrte(s)) == 0.0
}

/// The player-work fields fn_80040540 reads.
#[derive(Debug, Clone, Copy)]
pub struct TurnWork {
    pub status: i16,
    /// `spd` (+0x64) and PhysicsArray `jog_speed` (+0xE4).
    pub spd: Vec3,
    pub jog: f32,
    pub character: i8,
    pub f134: f32,
    pub row0_74: f32,
    pub row1_74: f32,
}

fn small_body(character: i8, f134: f32, row0_74: f32, row1_74: f32) -> bool {
    (character == 0 && f134 < row0_74) || (character == 1 && !(f134 >= row1_74))
}

/// fn_80040540: turn the task's Y angle by the angle between the motion and
/// the slide direction (query +0x6C), store it as the motion work's aim
/// (flipping it by 0x8000 when that is more than 0x6000 away from the old
/// aim), and when the slide is long enough point the motion work's vector
/// along it, keeping its speed (times 0.96 above 0.6 when grounded).
pub fn turn_to_slide(
    task_ang_y: &mut i32,
    w: &TurnWork,
    q: &mut SlideQuery,
    mw: &mut MotionWork,
    dir: Vec3,
) {
    if w.status & 3 == 0 && nj_scalor(w.spd) <= w.jog {
        return;
    }
    if outer_is_zero(outer_product(dir, q.facing)) {
        return;
    }
    if small_body(w.character, w.f134, w.row0_74, w.row1_74) {
        q.along = [ZERO; 3];
        mw.spd = [ZERO; 3];
        return;
    }
    if nj_scalor(q.along) == ZERO {
        return;
    }
    let a = unit_vector(q.motion);
    let b = unit_vector(q.along);
    *task_ang_y = task_ang_y.wrapping_sub(vector_angle(a, b, Some(q.up)));
    if diff_angle(mw.ang_aim, *task_ang_y) > 0x6000 {
        *task_ang_y = task_ang_y.wrapping_add(0x8000);
    }
    mw.ang_aim = *task_ang_y;
    if nj_scalor(q.along) >= SLIDE_MIN {
        let mut s = nj_scalor(mw.spd);
        if s > LIFT_FALLING && w.status & 3 != 0 {
            s *= SLIDE_DAMP;
        }
        mw.spd = unit_vector(q.along).map(|c| c * s);
    }
}

/// fn_80042B4C(task, vel, player, query, point, offset, ret, flags): after a
/// contact, store the best facing as the wall normal (player +0x70), turn
/// toward the slide when the approach (from the task position to `point`
/// when grounded, else against `vel`) is within 0x4000 of the facing, then
/// move the query and task to the averaged head point (less `offset`) and
/// the query motion to the slide. Returns `ret`, or 1 where the routine sets
/// it; some early exits skip the move.
#[allow(clippy::too_many_arguments)]
pub fn settle_contact(
    task_flag: i16,
    task_pos: &mut Vec3,
    task_ang_y: &mut i32,
    vel: &mut Vec3,
    w: &TurnWork,
    wall: &mut Vec3,
    q: &mut SlideQuery,
    mw: &mut MotionWork,
    point: Vec3,
    offset: Vec3,
    mut ret: i32,
    flags: u32,
    stuck: bool,
) -> i32 {
    *wall = q.facing;
    let neg = |v: Vec3| v.map(|c| -c);
    let turn = |d: Vec3, q: &mut SlideQuery, mw: &mut MotionWork, ang_y: &mut i32| {
        turn_to_slide(ang_y, w, q, mw, d);
    };
    if task_flag & 1 != 0 {
        let mut d = sub3(point, *task_pos);
        if nj_scalor(d) == ZERO {
            d = neg(*vel);
            if nj_scalor(d) == ZERO {
                return ret;
            }
        }
        let d = unit_vector(d);
        if vector_angle(d, q.facing, None) >= 0x4000 {
            return ret;
        }
        ret = 1;
        turn(d, q, mw, task_ang_y);
    } else {
        let d = neg(*vel);
        if nj_scalor(d) != ZERO {
            let d = unit_vector(d);
            let ang = vector_angle(d, q.facing, None);
            if flags & 0x1000 != 0 {
                if ang >= 0x4000 {
                    return ret;
                }
                ret = 1;
            } else {
                if ang >= 0x4000 {
                    return ret;
                }
                if stuck && task_flag & 2 != 0 {
                    q.along = [ZERO; 3];
                    *vel = [ZERO; 3];
                }
            }
            turn(d, q, mw, task_ang_y);
        }
    }
    q.center = q.head;
    *task_pos = sub3(q.head, offset);
    q.motion = q.along;
    ret
}

fn sub3(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

const WALL_SAME: f32 = f32::from_bits(0x3f7f_ff58); // r2-0x7504, 0.99999

/// Player-work speeds fn_80041C78 reads: `spd.x` (+0x64), +0xF0, and
/// PhysicsArray `jog_speed` / `run_speed` (+0xE4, +0xE8).
#[derive(Debug, Clone, Copy)]
pub struct PushSpeeds {
    pub spd_x: f32,
    pub f_f0: f32,
    pub jog: f32,
    pub run: f32,
}

/// fn_80041C78 (the grounded wall push): for each non-floor hit facing
/// against the motion, stop `vel` when it runs straight in (flag 0x00400000
/// and a fast `spd.x` return 2), cap `vel` at `run_speed` when the motion
/// pushes in by more than 4.5 (return 1), unless the entry has flag
/// 0x00080000. Two distinct walls set the stuck flag; a small body against
/// static walls stops (1); in task mode 1 two walls stop the motion and cap
/// `vel` at 0.6 * `jog_speed` (1, unless entry flag 0x1000); otherwise the
/// motion is cut to the nearest wall. Returns the routine's code.
pub fn wall_push(
    i: &SlideInput,
    entry_flags: u32,
    speeds: &PushSpeeds,
    hits: &[ContactHit],
    motion: &mut Vec3,
    vel: &mut Vec3,
) -> (i32, bool) {
    let mut code = 0;
    let mut near = NEAR_LIMIT;
    let mut walls = 0;
    let mut at: Option<usize> = None;
    let mut dists = vec![];
    for (k, h) in hits.iter().enumerate() {
        let d = point_distance(h.head, i.center);
        dists.push(d);
        if is_floor(i, h).0 {
            continue;
        }
        let t = if nj_scalor(*motion) == ZERO {
            *motion
        } else {
            unit_vector(*motion)
        };
        let into = inner_product(t, h.facing);
        if !(into < ZERO) {
            continue;
        }
        let distinct = match at {
            None => true,
            Some(w) => !(inner_product(h.facing, hits[w].facing) >= WALL_SAME),
        };
        if distinct {
            walls += 1;
        }
        if near > d {
            near = d;
            at = Some(k);
        }
        if entry_flags & 0x0008_0000 == 0 {
            if into < -nj_cos(0x600) {
                *vel = [ZERO; 3];
                if entry_flags & 0x0040_0000 != 0 && speeds.spd_x >= speeds.f_f0 {
                    code = 2;
                }
            }
            if inner_product(*motion, h.facing) < WALL_PUSH {
                let cap = speeds.run;
                if nj_scalor(*vel) > cap {
                    *vel = unit_vector(*vel).map(|c| c * cap);
                    code = 1;
                }
            }
        }
    }
    if walls == 0 {
        return (code, false);
    }
    let stuck =
        walls >= 2 && !(matches!(i.character, 0 | 1) && (0x4c..=0x52).contains(&i.task_mode));
    let len = nj_scalor(*motion);
    if !i.entry_has_task && small_body(i.character, i.f134, i.row0_74, i.row1_74) {
        *motion = [ZERO; 3];
        *vel = [ZERO; 3];
        return (1, stuck);
    }
    if i.task_mode == 1 && walls >= 2 && entry_flags & 0x1000 == 0 {
        let cap = LIFT_FALLING * speeds.jog;
        *motion = [ZERO; 3];
        if nj_scalor(*vel) > cap {
            *vel = unit_vector(*vel).map(|c| c * cap);
        }
        return (1, stuck);
    }
    if let Some(k) = at {
        if dists[k] < len {
            *motion = if nj_scalor(*motion) == ZERO {
                [ZERO; 3]
            } else {
                unit_vector(*motion).map(|c| c * dists[k])
            };
        }
    }
    (code, stuck)
}

/// fn_80040F48 (the airborne wall slide that also turns the task). Like
/// `wall_slide`, with these differences: flag 0x100 makes every hit a floor;
/// there is no 0x7000 limit; the along-plane motion is scaled to the
/// nearest wall's distance instead of gaining +0x78; a head-on approach
/// (within 0x1000 of the wall) stops the slide (2 when `spd.x` reaches
/// +0xF0) or caps `vel` at 0.2 * `jog_speed` (1); otherwise after sliding
/// the task's Y angle points along the slide (flipped by 0x8000 when more
/// than 0x6000 from the aim, which then takes it) and the return value is
/// DiffAngle's result.
#[allow(clippy::too_many_arguments)]
pub fn wall_slide_turn(
    i: &WallSlideInput,
    flags: u32,
    speeds: &PushSpeeds,
    hits: &[ContactHit],
    along: &mut Vec3,
    vel: &mut Vec3,
    aim: &mut i32,
    task_ang_y: &mut i32,
    wall: &mut Vec3,
) -> i32 {
    let s = &i.slide;
    let mut near = NEAR_LIMIT;
    let mut walls = 0;
    let mut at: Option<usize> = None;
    let mut dists = vec![];
    for (k, h) in hits.iter().enumerate() {
        let d = point_distance(h.head, s.center);
        dists.push(d);
        if is_floor(s, h).0 || flags & 0x100 != 0 {
            continue;
        }
        let t = if nj_scalor(i.motion) == ZERO {
            i.motion
        } else {
            unit_vector(i.motion)
        };
        let into = inner_product(t, h.facing);
        if !(into < ZERO) {
            continue;
        }
        let raw = inner_product(i.motion, h.facing);
        if into < -nj_cos(0x600) || raw < WALL_PUSH {
            walls += 1;
            if near > d {
                near = d;
                at = Some(k);
            }
        }
    }
    if walls == 0 {
        return 0;
    }
    let len = nj_scalor(i.motion);
    if small_body(s.character, s.f134, s.row0_74, s.row1_74) && flags & 0x0800_0000 == 0 {
        *along = [ZERO; 3];
        *vel = [ZERO; 3];
        return 0;
    }
    let Some(k) = at else {
        return 0;
    };
    if !(dists[k] < len) || i.last_attr & 0x100 != 0 {
        return 0;
    }
    if nj_scalor(*along) == ZERO {
        *along = [ZERO; 3];
        return 0;
    }
    *along = unit_vector(*along).map(|c| c * dists[k]);
    let h = &hits[k];
    *wall = h.facing;
    let back = vel.map(|c| -c);
    if nj_scalor(back) == ZERO {
        return 0;
    }
    if vector_angle(unit_vector(back), *wall, None) <= 0x1000 {
        if speeds.spd_x >= speeds.f_f0 {
            *along = [ZERO; 3];
            return 2;
        }
        if speeds.spd_x > speeds.jog {
            let cap = LIFT_RISING * speeds.jog;
            *vel = unit_vector(*vel).map(|c| c * cap);
        }
        return 1;
    }
    let cross = outer_product(*along, *wall);
    let cross = if nj_scalor(cross) == ZERO {
        s.up
    } else {
        unit_vector(cross)
    };
    let mut line = Line {
        point: [ZERO; 3],
        dir: [ZERO; 3],
    };
    plane_intersection(
        &Plane {
            point: h.foot,
            normal: *wall,
        },
        &Plane {
            point: i.head,
            normal: cross,
        },
        Some(&mut line),
    );
    let u = if nj_scalor(*along) == ZERO {
        *along
    } else {
        unit_vector(*along)
    };
    if inner_product(line.dir, u) < ZERO {
        line.dir = line.dir.map(|c| -c);
    }
    let speed = nj_scalor(*vel);
    *vel = line.dir.map(|c| c * speed);
    *along = *vel;
    *task_ang_y = ((BAMS_PER_RAD * atan2f(line.dir[2], line.dir[0]) as f64) as i32) & 0xffff;
    let r = diff_angle(*aim, *task_ang_y);
    if r > 0x6000 {
        *task_ang_y = task_ang_y.wrapping_add(0x8000);
    }
    *aim = *task_ang_y;
    r
}

const SIDE_MIN: f32 = f32::from_bits(0x3e19_999a); // r2-0x750C, 0.15

/// The query fields fn_800407A0 also reads: the averaged foot (+0x54).
/// fn_800407A0: the grounded counterpart of `turn_to_slide` (characters 6
/// and 7, the walkers, skip it). The slide direction is where the contact
/// plane (through the averaged foot) meets the vertical plane through the
/// query center across `dir`, pointed against `dir`; the along-plane motion
/// takes that direction at the length of the slide (airborne) or of the
/// motion (grounded), and so does the motion work's vector under
/// `last_attr` 0x80000. The task then turns by the angle between the
/// motion and the slide when the motion is longer than 0.2.
pub fn turn_to_ground_slide(
    task_ang_y: &mut i32,
    w: &TurnWork,
    last_attr: u32,
    q: &mut SlideQuery,
    foot: Vec3,
    mw: &mut MotionWork,
    dir: Vec3,
) {
    if matches!(w.character, 6 | 7) {
        return;
    }
    if w.status & 3 == 0 && nj_scalor(w.spd) <= w.jog {
        return;
    }
    let c = outer_product(dir, q.facing);
    if outer_is_zero(c) {
        return;
    }
    let c = if nj_scalor(c) <= SIDE_MIN {
        q.up
    } else {
        unit_vector(c)
    };
    let mut line = Line {
        point: [ZERO; 3],
        dir: [ZERO; 3],
    };
    plane_intersection(
        &Plane {
            point: foot,
            normal: q.facing,
        },
        &Plane {
            point: q.center,
            normal: c,
        },
        Some(&mut line),
    );
    let u = if nj_scalor(dir) == ZERO {
        dir
    } else {
        unit_vector(dir)
    };
    if inner_product(line.dir, u) > ZERO {
        line.dir = line.dir.map(|x| -x);
    }
    let len = if w.status & 3 == 0 {
        nj_scalor(q.along)
    } else {
        nj_scalor(q.motion)
    };
    q.along = line.dir.map(|x| len * x);
    if last_attr & 0x8_0000 != 0 {
        let s = nj_scalor(mw.spd);
        mw.spd = line.dir.map(|x| s * x);
    }
    if nj_scalor(q.motion) > LIFT_RISING {
        let a = unit_vector(q.motion);
        let b = unit_vector(q.along);
        *task_ang_y = task_ang_y.wrapping_sub(vector_angle(a, b, Some(q.up)));
    }
    if diff_angle(mw.ang_aim, *task_ang_y) > 0x6000 {
        *task_ang_y = task_ang_y.wrapping_add(0x8000);
    }
    mw.ang_aim = *task_ang_y;
}

const NEG_ONE: f32 = -1.0; // r2-0x75B0
const FAST_PUSH: f32 = 15.0; // r2-0x74E0
const NEVER_DOT: f32 = 4.0; // r2-0x74DC
const ALONG_CAP: f32 = 3.0; // r2-0x75F4
/// Task mode that settles to the second offset and stops on contact.
pub const MODE_STOP: u8 = 0x38;

/// The task fields fn_80042E0C reads and writes.
#[derive(Debug, Clone, Copy)]
pub struct SettleTask {
    /// Byte +0.
    pub mode: u8,
    /// Halfword +4.
    pub flag: i16,
    /// +8, +0xC, +0x10.
    pub angles: [i32; 3],
    /// +0x14.
    pub pos: Vec3,
}

/// The player-work fields fn_80042E0C reads and writes besides `TurnWork`.
#[derive(Debug, Clone, Copy)]
pub struct SettleWork {
    /// `spd` (+0x64): also cleared on fast contacts.
    pub spd: Vec3,
    /// +0xF0.
    pub f_f0: f32,
    /// `weight` (+0x138).
    pub weight: f32,
    /// `last_attr` (+0x8C).
    pub last_attr: u32,
    /// Word +0xC0 and halfword +0x12.
    pub w_c0: i32,
    pub h_12: i16,
    /// The wall normal (+0x70).
    pub wall: Vec3,
}

/// The rest of what fn_80042E0C reads.
#[derive(Debug, Clone, Copy)]
pub struct SettleInput {
    /// The contact point the grounded branch aims from.
    pub point: Vec3,
    /// Offsets from the head point to the task position (the second for
    /// `MODE_STOP`).
    pub offset: Vec3,
    pub offset_stop: Vec3,
    pub flags: u32,
    /// r13-0x78F4 and r13-0x7900.
    pub stuck: bool,
    pub backward_probe: bool,
    pub stage: i32,
    /// Averaged foot point (query +0x54).
    pub foot: Vec3,
}

/// fn_80042E0C: the grounded-mode counterpart of `settle_contact` with the
/// walker rules (fast pushes, stop mode 0x38, the up-facing flattening, the
/// character 4/5 shallow case). Returns the result code.
#[allow(clippy::too_many_arguments)]
pub fn settle_ground_contact(
    stack: &mut MatrixStack,
    t: &mut SettleTask,
    vel: &mut Vec3,
    w: &TurnWork,
    sw: &mut SettleWork,
    q: &mut SlideQuery,
    mw: &mut MotionWork,
    i: &SettleInput,
    mut ret: i32,
) -> i32 {
    sw.wall = q.facing;
    // fn_800407A0 reads the player's `spd` from the same work.
    let tw = TurnWork { spd: sw.spd, ..*w };
    let (last_attr, foot) = (sw.last_attr, i.foot);
    let turn =
        |t: &mut SettleTask, q: &mut SlideQuery, mw: &mut MotionWork, tw: &TurnWork, d: Vec3| {
            turn_to_ground_slide(&mut t.angles[1], tw, last_attr, q, foot, mw, d);
        };
    let fast_stop = |q: &mut SlideQuery, sw: &mut SettleWork| {
        q.along = [ZERO; 3];
        sw.spd = [ZERO; 3];
    };
    'end: {
        if t.flag & 1 != 0 {
            let d = sub3(i.point, t.pos);
            if nj_scalor(d) == ZERO {
                break 'end;
            }
            let d = unit_vector(d);
            let ang = vector_angle(d, q.facing, None);
            if i.flags & 0x1000 != 0 {
                if ang > 0x4000 {
                    return ret;
                }
                ret = 1;
                turn(t, q, mw, &tw, d);
                break 'end;
            }
            if sw.spd[0] >= FAST_PUSH {
                if i.flags & 0x0040_0000 != 0 {
                    ret = 2;
                }
                fast_stop(q, sw);
                break 'end;
            }
            if ang <= 0x1000 {
                if sw.spd[0] >= sw.f_f0 && i.flags & 0x0040_0000 != 0 {
                    ret = 2;
                    break 'end;
                }
                ret = 1;
                turn(t, q, mw, &tw, d);
                break 'end;
            }
            if ang > 0x4000 {
                break 'end;
            }
            ret = 1;
            turn(t, q, mw, &tw, d);
            break 'end;
        }
        if t.mode == MODE_STOP {
            ret = 1;
            fast_stop(q, sw);
            break 'end;
        }
        let d = if i.backward_probe {
            rotate_by_angles(stack, t.angles, [NEG_ONE, ZERO, ZERO])
        } else {
            vel.map(|c| -c)
        };
        if nj_scalor(d) == ZERO {
            break 'end;
        }
        let d = unit_vector(d);
        let ang = vector_angle(d, q.facing, None);
        let dot = inner_product(d, q.facing);
        if i.flags & 0x1000 != 0 {
            if ang > 0x4000 {
                return ret;
            }
            if i.stage == 0x2b {
                q.along[0] = ZERO;
                q.along[2] = ZERO;
                q.motion = q.along;
                q.center = q.head;
                t.pos = sub3(q.head, i.offset_stop);
                return 1;
            }
            ret = 1;
            turn(t, q, mw, &tw, d);
            break 'end;
        }
        if sw.spd[0] >= FAST_PUSH && dot >= NEVER_DOT {
            ret = 2;
            fast_stop(q, sw);
            break 'end;
        }
        if sw.spd[1] > ZERO && inner_product(q.up, q.facing) < nj_cos(0x7000) {
            ret = 1;
            let flatten = |stack: &mut MatrixStack, v: Vec3| {
                let mut l = unrotate_by_angles(stack, t.angles, v);
                l[1] = ZERO;
                rotate_by_angles(stack, t.angles, l)
            };
            q.along = flatten(stack, q.along);
            *vel = flatten(stack, *vel);
            sw.h_12 = sw.w_c0 as i16;
            break 'end;
        }
        if dot >= NEVER_DOT {
            ret = if sw.spd[0] >= sw.f_f0 { 2 } else { 1 };
            break 'end;
        }
        if matches!(w.character, 4 | 5) && ang <= 0x3400 && i.flags & 0x1080 == 0 {
            ret = 1;
            break 'end;
        }
        if ang >= 0x4000 {
            if sw.spd[1] < -sw.weight {
                break 'end;
            }
            return ret;
        }
        if w.status & 3 == 0 {
            break 'end;
        }
        if i.stuck {
            q.along = [ZERO; 3];
        }
        turn(t, q, mw, &tw, d);
    }
    q.center = q.head;
    t.pos = sub3(
        q.head,
        if t.mode == MODE_STOP {
            i.offset_stop
        } else {
            i.offset
        },
    );
    if matches!(t.mode as i8, 0xd | 0xa) && nj_scalor(q.along) > ALONG_CAP {
        q.along = unit_vector(q.along).map(|c| c * ALONG_CAP);
    }
    q.motion = q.along;
    ret
}

/// fn_80041814: whether any hit is a wall the motion runs into (not floor,
/// within 0x7000 of the up vector, facing against the motion).
pub fn any_wall_ahead(i: &SlideInput, hits: &[ContactHit], motion: Vec3) -> bool {
    hits.iter().any(|h| {
        let (floor, ang) = is_floor(i, h);
        if floor {
            return false;
        }
        let t = if nj_scalor(motion) == ZERO {
            motion
        } else {
            unit_vector(motion)
        };
        ang < WALL_ANGLE && inner_product(t, h.facing) < ZERO
    })
}

const LINE_NEAR: f32 = f32::from_bits(0x3ccc_cccd); // r2-0x55A0, 0.025

/// fn_8011B51C: the distance from `p` to the line, and the foot of the
/// perpendicular when asked. Squared distances below 0.025 give 0.
pub fn line_distance(p: Vec3, line: &Line, foot: Option<&mut Vec3>) -> f32 {
    let (q, d) = (line.point, line.dir);
    let along = d[2] * (p[2] - q[2]) + (d[0] * (p[0] - q[0]) + d[1] * (p[1] - q[1]));
    let t = along / (d[2] * d[2] + (d[0] * d[0] + d[1] * d[1]));
    let sq = match foot {
        Some(out) => {
            *out = [q[0] + d[0] * t, q[1] + d[1] * t, q[2] + d[2] * t];
            let (dz, dx, dy) = (p[2] - out[2], p[0] - out[0], p[1] - out[1]);
            dz * dz + (dx * dx + dy * dy)
        }
        None => {
            let dz = p[2] - (q[2] + d[2] * t);
            let dx = p[0] - (q[0] + d[0] * t);
            let dy = p[1] - (q[1] + d[1] * t);
            dz * dz + (dx * dx + dy * dy)
        }
    };
    if sq < LINE_NEAR {
        ZERO
    } else {
        sqrtf(sq)
    }
}
