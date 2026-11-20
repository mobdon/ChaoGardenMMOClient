//! The walker's collision helpers under fn_8003EAFC and fn_80043ED4
//! (GSNE8P main.dol), ported instruction by instruction and checked against
//! main.dol by tests/walker_collide.rs.
use crate::{
    land_contact::{unit_vector, ContactHit},
    land_nodes::nj_scalor,
    land_select::inline_sqrt,
    ninja_matrix::Vec3,
    ninja_trig::{fmadds, fnmsubs, nj_cos},
    ninja_vector::{inner_product, outer_product, vector_angle},
};

const ZERO: f32 = 0.0; // r2-0x75F8
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
