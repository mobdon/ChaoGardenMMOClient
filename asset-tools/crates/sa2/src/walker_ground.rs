//! The walker's land collision drivers (GSNE8P main.dol): fn_800424C8
//! (riding dynamic land), fn_80043600 (re-landing after a contact) and
//! fn_8003EAFC (the ground pass), built on `land_select`, `land_nodes`,
//! `land_contact` and the helpers in `walker_collide`. Checked against
//! main.dol by tests/walker_ground.rs on Iron Gate's land.
// Negated comparisons keep the compiled branches' NaN behaviour.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::{
    land_contact::{unit_vector, Contact, ContactHit, ContactStats},
    land_nodes::{nj_scalor, Entry, LandNodes, NjModel, NjObject, Query},
    land_select::{LandCol, Selection},
    msl_math::{atan2f, normal_angles},
    ninja_matrix::{rotate_by_angles, unrotate_by_angles, Axis, MatrixStack, Vec3},
    ninja_trig::{fmadds, fnmsubs},
    ninja_vector::{acosf, inner_product, vector_angle},
    walker_collide::{
        any_wall_ahead, apply_surface, diff_angle, plane_distance, plane_intersection,
        point_distance, settle_contact, settle_ground_contact, slide_airborne, slide_grounded,
        sphere_lift, sqrtf, sub_angle, tilt_to_contact, wall_push, wall_slide, wall_slide_turn,
        LiftInput, Line, MotionWork, PhysicsRow, Plane, PushSpeeds, SettleInput, SettleTask,
        SettleWork, SlideInput, SlideQuery, SurfaceWork, TiltSpeeds, TurnWork, WallSlideInput,
    },
};

const NEG_ONE: f32 = -1.0; // r2-0x75B0
/// Radians to BAMS (double at r2-0x75E8).
const BAMS_PER_RAD: f64 = f64::from_bits(0x40c4_5f30_b217_88bb);

const ZERO: f32 = 0.0;

/// The collision query a player keeps (player work +0x90 points at it):
/// the broadphase and contact inputs and the contact outputs.
#[derive(Debug, Clone, PartialEq)]
pub struct CollisionQuery {
    /// +0, +4, +0x10, +0x1C.
    pub radius: f32,
    pub center: Vec3,
    pub motion: Vec3,
    pub up: Vec3,
    /// +0x28, +0x2A.
    pub count: u16,
    pub best: u16,
    /// +0x34, +0x38: the best hit's angles.
    pub ang_x: i32,
    pub ang_z: i32,
    /// +0x3C, +0x48: the best hit's normal and facing.
    pub normal: Vec3,
    pub facing: Vec3,
    /// +0x54, +0x60: averaged foot and head points.
    pub foot: Vec3,
    pub head: Vec3,
    /// +0x6C, +0x78: the best hit's motion along and across its plane.
    pub along: Vec3,
    pub across: Vec3,
}

impl CollisionQuery {
    pub fn query(&self) -> Query {
        Query {
            radius: self.radius,
            center: self.center,
            motion: self.motion,
        }
    }

    /// What fn_800CEE08 writes on a hit.
    pub fn apply(&mut self, c: &Contact) {
        let b = &c.hits[c.best];
        self.count = c.hits.len() as u16;
        self.best = c.best as u16;
        self.ang_x = b.angles.0;
        self.ang_z = b.angles.1;
        self.normal = b.normal;
        self.facing = b.facing;
        self.foot = c.foot;
        self.head = c.head;
        self.along = b.along;
        self.across = b.across;
    }
}

/// The land state the drivers query: the frame's selection, the node list
/// and the static land data, plus the hit copies fn_800CEE08 leaves at
/// 0x80295D10 (kept until the next hit).
pub struct LandContext<'a> {
    pub selection: &'a mut Selection,
    pub nodes: &'a mut LandNodes,
    pub objects: &'a [NjObject],
    pub models: &'a [NjModel],
    pub cols: &'a [LandCol],
    pub col_objs: &'a [usize],
    pub stats: &'a mut ContactStats,
    pub hits: Vec<ContactHit>,
}

impl LandContext<'_> {
    /// fn_800D1020 (with fn_80023808): returns the selection list's length
    /// after the re-pick (r13-0x79A4) as the callers read it.
    pub fn broadphase(&mut self, q: &CollisionQuery) -> usize {
        self.selection.reselect(
            self.objects,
            self.models,
            self.cols,
            self.col_objs,
            q.center,
            q.radius,
        );
        self.nodes.query(&q.query(), &self.selection.list);
        self.selection.list.len()
    }

    /// fn_800CEE08 on `obj`, writing the query on a hit.
    pub fn collide(&mut self, q: &mut CollisionQuery, obj: usize, vertex_mode: bool) -> bool {
        match self.nodes.collide(&q.query(), obj, vertex_mode, self.stats) {
            Some(c) => {
                q.apply(&c);
                self.hits = c.hits;
                true
            }
            None => false,
        }
    }

    pub fn list(&self) -> &[Entry] {
        &self.selection.list
    }
}

/// A dynamic land object's ride record (task +0x3C, a second one at +0x1C
/// for player 2): an optional callback, the angle change and the motion the
/// mover made this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RideRecord {
    pub has_callback: bool,
    pub dang: [i32; 3],
    pub dpos: Vec3,
}

/// fn_800424C8: carry the player along with the land object it stands on.
/// With a callback, the task position is set to the query center less
/// `offset`, the callback moves it, and the query center follows. Then the
/// center gains the mover's motion and the task its angle change. False
/// when there is no record or the mover did not move.
pub fn ride_mover(
    record: Option<&RideRecord>,
    task_pos: &mut Vec3,
    task_angles: &mut [i32; 3],
    q: &mut CollisionQuery,
    offset: Vec3,
    callback: &mut dyn FnMut(&mut Vec3),
) -> bool {
    let Some(r) = record else {
        return false;
    };
    if r.has_callback {
        let saved = *task_pos;
        *task_pos = sub(q.center, offset);
        callback(task_pos);
        q.center = add(*task_pos, offset);
        *task_pos = saved;
    }
    q.center = add(q.center, r.dpos);
    for (a, d) in task_angles.iter_mut().zip(r.dang) {
        *a = (a.wrapping_add(d)) & 0xffff;
    }
    nj_scalor(r.dpos) != ZERO
}

fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// Game state the drivers read besides the player: the global vector at
/// 0x801E5624, the stage number, the PhysicsArray rows, the stuck flag
/// (r13-0x78F4) and the player's motion work (`playermwp[pnum]`).
pub struct GroundEnv<'a> {
    pub global: Vec3,
    pub stage: i32,
    pub rows: &'a [PhysicsRow],
    pub stuck: bool,
    pub mw: MotionWork,
    /// A dynamic land task's ride record for a player number (task +0x3C).
    pub ride_record: &'a dyn Fn(usize, i8) -> Option<RideRecord>,
    /// Whether a dynamic land task's `twp.flag` has 0x100.
    pub task_has_collision: &'a dyn Fn(usize) -> bool,
}

/// fn_8003EAFC's return value when fn_80040ACC slid the player: the game
/// returns the move vector's address there, which callers only test for
/// being nonzero and not 2.
pub const RET_SLID: i32 = 3;

const HALF: f32 = 0.5; // r2-0x75CC
const ONE: f32 = 1.0; // r2-0x75C8
const LAND_SPEED: f32 = f32::from_bits(0x3ba3_d70a); // r2-0x7540, 0.005
const STEP_ALONG: f32 = f32::from_bits(0x4083_3333); // r2-0x753C, 4.1
const MOVE_MIN: f32 = f32::from_bits(0x38d1_b717); // r2-0x7538, 1e-4
const ANALOG_LOW: f32 = f32::from_bits(0x3f66_6666); // r2-0x7548, 0.9
const DOTP_FLAT: f32 = f32::from_bits(0x3f33_3333); // r2-0x7534, 0.7
const COLLECT_LIMIT: usize = 0x1f;

fn slide_input(
    t: &PlayerTask,
    w: &PlayerWork,
    env: &GroundEnv,
    q: &CollisionQuery,
    e: &Entry,
) -> SlideInput {
    SlideInput {
        task_mode: t.mode as i8,
        character: w.character,
        f134: w.f134,
        row0_74: env.rows[0].f74,
        row1_74: env.rows[1].f74,
        entry_has_task: e.task.is_some(),
        center: q.center,
        up: q.up,
        global: env.global,
    }
}

/// fn_800420C0 on the player.
pub fn surface(t: &PlayerTask, w: &mut PlayerWork, attr: u32, env: &GroundEnv) {
    let mut s = SurfaceWork {
        character: w.character,
        accel_row: w.accel_row,
        attr: w.attr,
        f40: w.f40,
        run_accel: w.run_accel,
        f118: w.f118,
        weight: w.weight,
        f134: w.f134,
        flags_1a8: w.flags_1a8,
        height_1b0: w.height_1b0,
    };
    apply_surface(&mut s, t.pos[1], attr, env.stage, env.rows);
    w.attr = s.attr;
    w.f40 = s.f40;
    w.run_accel = s.run_accel;
    w.f118 = s.f118;
    w.weight = s.weight;
}

/// fn_800428F8 on the player.
pub fn lift(motion: Vec3, w: &PlayerWork, q: &CollisionQuery, global: Vec3, out_y: f32) -> Vec3 {
    sphere_lift(
        &LiftInput {
            motion,
            height: w.height,
            last_attr: w.last_attr,
            spd_x: w.spd[0],
            spd_y: w.spd[1],
            status: w.status,
            run_speed: w.run,
            speed_f4: w.f_f4,
            up: q.up,
            global,
        },
        out_y,
    )
}

/// The player's up vector (into the query, with the task's X and Z
/// angles), the sphere offset and the lifted center offset in world space,
/// all under the task rotation (njRotateZ, X, then -Y).
fn frame_offsets(
    stack: &mut MatrixStack,
    t: &PlayerTask,
    vel: &MoveVector,
    w: &PlayerWork,
    q: &mut CollisionQuery,
    global: Vec3,
) -> (Vec3, Vec3) {
    stack.push_unit();
    stack.rotate(Axis::Z, t.angles[2]);
    stack.rotate(Axis::X, t.angles[0]);
    stack.rotate(Axis::Y, t.angles[1].wrapping_neg());
    q.up = stack.calc_vector([ZERO, ONE, ZERO]);
    q.ang_x = t.angles[0];
    q.ang_z = t.angles[2];
    let half = HALF * w.f134;
    let offset = stack.calc_vector([ZERO, half, ZERO]);
    let l = lift(vel.v, w, q, global, half);
    let lifted = stack.calc_vector(l);
    stack.pop();
    (offset, lifted)
}

/// The contact order fn_8003EAFC builds from the collected entries: the
/// entry whose slide test cut the motion first, then the rest (in list
/// order when riding, else reversed), with the entry landed on last frame
/// moved to the end.
pub fn contact_order(
    collected: &[Entry],
    picked: Option<usize>,
    riding: bool,
    landed_on: Option<usize>,
) -> Vec<Entry> {
    let n = collected.len();
    let mut slots: Vec<Option<Entry>> = vec![None; n];
    let mut cursor = 0;
    let mut pick = picked;
    if let Some(p) = picked {
        slots[0] = Some(collected[p]);
        cursor = 1;
    }
    let order: Vec<usize> = if riding {
        (0..n).collect()
    } else {
        (0..n).rev().collect()
    };
    for i in order {
        if pick == Some(i) {
            pick = None;
            continue;
        }
        if landed_on == Some(collected[i].obj) {
            slots[n - 1] = Some(collected[i]);
            continue;
        }
        slots[cursor] = Some(collected[i]);
        cursor += 1;
    }
    slots.into_iter().flatten().collect()
}

#[allow(clippy::too_many_arguments)]
fn settle_wall(
    land: &LandContext,
    t: &mut PlayerTask,
    vel: &mut MoveVector,
    w: &mut PlayerWork,
    q: &mut CollisionQuery,
    env: &mut GroundEnv,
    e: &Entry,
    start: Vec3,
    offset: Vec3,
    ret: i32,
) -> i32 {
    let _ = land;
    let tw = TurnWork {
        status: w.status,
        spd: w.spd,
        jog: w.jog,
        character: w.character,
        f134: w.f134,
        row0_74: env.rows[0].f74,
        row1_74: env.rows[1].f74,
    };
    let mut sq = SlideQuery {
        center: q.center,
        motion: q.motion,
        up: q.up,
        facing: q.facing,
        head: q.head,
        along: q.along,
    };
    let r = settle_contact(
        t.flag,
        &mut t.pos,
        &mut t.angles[1],
        &mut vel.v,
        &tw,
        &mut w.wall,
        &mut sq,
        &mut env.mw,
        start,
        offset,
        ret,
        e.flags,
        env.stuck,
    );
    q.center = sq.center;
    q.motion = sq.motion;
    q.along = sq.along;
    r
}

#[allow(clippy::too_many_arguments)]
fn finish_landing(
    land: &LandContext,
    t: &mut PlayerTask,
    w: &mut PlayerWork,
    q: &mut CollisionQuery,
    env: &GroundEnv,
    e: &Entry,
    slot_flags: u32,
    offset: Vec3,
    ride_callback: &mut dyn FnMut(&mut Vec3),
) {
    q.center = add(q.head, q.along);
    let plane = Plane {
        point: land.hits[q.best as usize].foot,
        normal: q.up,
    };
    let mut foot = [ZERO; 3];
    plane_distance(q.center, &plane, Some(&mut foot));
    q.center = add(offset, foot);
    let mut ride_task = e.task;
    if let Some(task) = e.task {
        w.ride = Some(task);
        let rec = (env.ride_record)(task, w.pnum);
        if !ride_mover(
            rec.as_ref(),
            &mut t.pos,
            &mut t.angles,
            q,
            offset,
            ride_callback,
        ) {
            ride_task = None;
        }
    } else {
        w.ride = None;
    }
    let newpos = sub(q.center, offset);
    let d = point_distance(t.pos, newpos);
    let keep = ride_task.is_none() && w.spd[0] == ZERO && w.spd[2] == ZERO && !(d > ONE);
    if !keep && d > MOVE_MIN {
        t.pos = newpos;
    }
    t.flag |= 1;
    surface(t, w, slot_flags, env);
}

/// fn_8003EAFC: the ground pass. Builds the player's collision sphere,
/// runs the broadphase, sorts the listed land into a contact order, then
/// lands on floors (tilting to them, sliding along walls they meet, riding
/// dynamic land) and settles against walls, and finally moves the task.
/// Returns 0, 1, 2 or `RET_SLID`. `analog` is fn_8004E560(task, player,
/// NULL, &out), giving its result and `out`; `ride_callback` is the ridden
/// mover's callback; `f26_in` is the caller's f26, which fn_800414F0
/// compares with before setting it.
#[allow(clippy::too_many_arguments)]
pub fn ground_pass(
    stack: &mut MatrixStack,
    land: &mut LandContext,
    t: &mut PlayerTask,
    vel: &mut MoveVector,
    w: &mut PlayerWork,
    q: &mut CollisionQuery,
    env: &mut GroundEnv,
    analog: &mut dyn FnMut(&PlayerTask, &PlayerWork) -> (bool, f32),
    ride_callback: &mut dyn FnMut(&mut Vec3),
    f26_in: f32,
) -> i32 {
    let old_status = w.status;
    w.status = t.flag;
    t.flag &= !1;
    let (mut offset, lifted) = frame_offsets(stack, t, vel, w, q, env.global);
    q.center = add(t.pos, lifted);
    q.radius = HALF * w.f134;
    q.motion = add(vel.v, w.v4c);
    let start = t.pos;
    if land.broadphase(q) == 0 {
        return no_land(t, vel, w, env);
    }
    // Collect up to 0x1F listed entries; note the last one whose slide test
    // cut the motion.
    let mut collected: Vec<Entry> = vec![];
    let mut picked: Option<usize> = None;
    let list: Vec<Entry> = land.list().to_vec();
    for e in &list {
        if e.flags & 0x2002 != 0 {
            continue;
        }
        if let Some(task) = e.task {
            if !(env.task_has_collision)(task) {
                continue;
            }
        }
        if land.collide(q, e.obj, false) {
            let si = slide_input(t, w, env, q, e);
            let r = if w.status & 3 != 0 {
                slide_airborne(&si, &land.hits, &mut q.motion, &mut vel.v)
            } else {
                slide_grounded(&si, &land.hits, &mut q.motion, &mut vel.v, f26_in)
            };
            env.stuck |= r.stuck;
            if r.hit {
                picked = Some(collected.len());
            }
        }
        collected.push(*e);
        if collected.len() >= COLLECT_LIMIT {
            break;
        }
    }
    if collected.is_empty() {
        return no_land(t, vel, w, env);
    }
    let mut order = contact_order(&collected, picked, w.ride.is_some(), w.landed_on);
    q.center = add(t.pos, lifted);
    q.motion = add(vel.v, w.v4c);
    w.ride = None;
    let mut ret = 0;
    let mut landed: Option<usize> = None;
    let old_grounded = old_status & 3 != 0;
    let mut k = 0usize;
    while k < order.len() {
        let e = order[k];
        if !land.collide(q, e.obj, false) {
            k += 1;
            continue;
        }
        let ang = vector_angle(q.up, q.facing, None);
        let mut is_floor = t.flag & 2 == 0
            && e.flags & 0x0040_1000 == 0
            && (ang <= 0x2c00 || {
                let gf = inner_product(env.global, q.facing);
                inner_product(env.global, q.up) > gf
            });
        if is_floor && old_grounded && w.status & 3 == 0 {
            is_floor = false;
        }
        if is_floor && inner_product(env.global, q.facing) > ZERO && w.status & 3 == 0 {
            is_floor = false;
        }
        if is_floor {
            let sy = w.spd[1];
            if !(sy <= LAND_SPEED) && (!(sy > ZERO) || w.status & 3 != 0) {
                is_floor = false;
            }
        }
        if !is_floor {
            ret = settle_wall(land, t, vel, w, q, env, &e, start, offset, ret);
            k += 1;
            continue;
        }
        // Land on this floor.
        landed = Some(e.obj);
        let wi = WallSlideInput {
            slide: slide_input(t, w, env, q, &e),
            entry_flags: e.flags,
            last_attr: w.last_attr,
            motion: q.motion,
            head: q.head,
            across: q.across,
        };
        if wall_slide(&wi, &land.hits, &mut q.along, &mut vel.v, &mut w.wall) && ret == 0 {
            ret = RET_SLID;
        }
        let (ax, az) = normal_angles(q.facing[0], q.facing[1], q.facing[2]);
        q.ang_x = ax;
        q.ang_z = az;
        let level_still = vel.v[0] == ZERO && vel.v[2] == ZERO && w.status & 1 != 0 && {
            let gu = inner_product(env.global, q.up);
            inner_product(env.global, q.facing) >= gu
        };
        if !level_still {
            if diff_angle(t.angles[2], az) > 0x4000 {
                q.ang_z = az.wrapping_sub(0x8000);
            }
            if q.facing[1] <= ZERO && diff_angle(q.ang_z, 0) < 0x4000 {
                q.ang_x = 0x8000 - ax;
            }
            let speeds = TiltSpeeds {
                spd_x: w.spd[0],
                jog: w.jog,
                run: w.run,
                f_ec: w.f_ec,
                f_f4: w.f_f4,
            };
            tilt_to_contact(
                stack,
                &mut t.angles,
                w.status,
                &speeds,
                (q.ang_x, q.ang_z),
                q.facing,
                q.up,
                env.global,
                e.flags,
            );
        }
        if ret == 0 {
            offset = frame_offsets(stack, t, vel, w, q, env.global).0;
        }
        vel.angles[0] = q.ang_x;
        vel.angles[2] = q.ang_z;
        w.floor_normal = q.facing;
        let restart = nj_scalor(q.along) > STEP_ALONG;
        if restart {
            let u = unit_vector(q.along);
            let rest = nj_scalor(q.motion) - STEP_ALONG;
            q.motion = u.map(|c| c * rest);
            q.along = u.map(|c| c * STEP_ALONG);
            let last = order.len() - 1;
            order.swap(last, k);
        } else {
            q.motion = [ZERO; 3];
        }
        // The surface flags come from the list slot, which the restart has
        // just swapped with the last entry.
        let slot_flags = order[k].flags;
        finish_landing(land, t, w, q, env, &e, slot_flags, offset, ride_callback);
        k = if restart { 0 } else { k + 1 };
    }
    if let Some(obj) = landed {
        w.landed_on = Some(obj);
        t.pos = add(t.pos, q.motion);
        return ret;
    }
    let mut stopped = false;
    if w.status & 1 != 0 && w.status & 0x4000 == 0 {
        let (on, level) = analog(t, w);
        let quiet = !on || (level <= ANALOG_LOW && w.h14 == 0);
        if quiet && w.spd[1] <= ZERO && w.spd[0] <= w.run && w.dotp > DOTP_FLAT {
            for j in 0..3 {
                t.pos[j] = fnmsubs(HALF, vel.v[j], t.pos[j]);
            }
            vel.v = [ZERO; 3];
            stopped = true;
        }
    }
    if !stopped {
        t.pos = add(t.pos, q.motion);
    }
    surface(t, w, 0, env);
    ret
}

fn no_land(t: &mut PlayerTask, vel: &MoveVector, w: &mut PlayerWork, env: &GroundEnv) -> i32 {
    t.pos = add(t.pos, w.v4c);
    t.pos = add(t.pos, vel.v);
    surface(t, w, 0, env);
    w.ride = None;
    w.landed_on = None;
    0
}

/// The player task fields the drivers use (task work).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerTask {
    /// Byte +0 (mode).
    pub mode: u8,
    /// Halfword +4.
    pub flag: i16,
    /// +8, +0xC, +0x10.
    pub angles: [i32; 3],
    /// +0x14.
    pub pos: Vec3,
}

/// The move vector the drivers take (r4): a vector and, at +0x18..+0x20,
/// three angles.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoveVector {
    pub v: Vec3,
    pub angles: [i32; 3],
}

/// The player work fields the drivers use (`playerwk`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerWork {
    /// Bytes +0 (player number), +1 (character), +3 (accel row).
    pub pnum: i8,
    pub character: i8,
    pub accel_row: i8,
    /// Halfwords +0x12, +0x14, +0x1C (status).
    pub h12: i16,
    pub h14: i16,
    pub status: i16,
    /// +0x34 `dotp`, +0x40.
    pub dotp: f32,
    pub f40: f32,
    /// +0x4C: an extra motion added to the query.
    pub v4c: Vec3,
    /// +0x64 `spd`, +0x70 wall normal, +0x7C `floor_normal`.
    pub spd: Vec3,
    pub wall: Vec3,
    pub floor_normal: Vec3,
    /// +0x88, +0x8C.
    pub attr: u32,
    pub last_attr: u32,
    /// +0xA0: the dynamic land task ridden; +0xB0: the land object landed on.
    pub ride: Option<usize>,
    pub landed_on: Option<usize>,
    /// +0xC0 (word), +0xC4 (collision height).
    pub w_c0: i32,
    pub height: f32,
    /// PhysicsArray copies: +0xE4 jog, +0xE8 run, +0xEC, +0xF0, +0xF4,
    /// +0xFC run_accel, +0x118, +0x134, +0x138 weight.
    pub jog: f32,
    pub run: f32,
    pub f_ec: f32,
    pub f_f0: f32,
    pub f_f4: f32,
    pub run_accel: f32,
    pub f118: f32,
    pub f134: f32,
    pub weight: f32,
    /// +0x1A8, +0x1B0.
    pub flags_1a8: u32,
    pub height_1b0: f32,
}

const FACING_FLAT: f32 = f32::from_bits(0x3f7f_ffef); // r2-0x74D8, 0.999999
const RELAND_FAST: f32 = f32::from_bits(0x40a3_3333); // r2-0x74D4, 5.1
const RELAND_LIFT: f32 = 4.0; // r2-0x74DC
/// r13-0x7EAC and r13-0x7EA8: the lift and radius used for player work
/// byte 0 == 8 while on dynamic land (initial `.sdata` values).
const LIFT_8: f32 = f32::from_bits(0x40a6_6666);
const RADIUS_8: f32 = 4.0;

/// The rotation fn_80043600 and fn_80043ED4 use for offsets: the task's
/// (njRotateZ, X, -Y), or, on dynamic land (r13-0x78F8) outside the
/// 0x2000 task state, the contact facing's tilt angles with the task's Y.
fn rotate_offset(
    stack: &mut MatrixStack,
    by_task: bool,
    angles: [i32; 3],
    facing: Vec3,
    v: Vec3,
) -> Vec3 {
    let (ax, az) = if by_task {
        (angles[0], angles[2])
    } else {
        normal_angles(facing[0], facing[1], facing[2])
    };
    stack.push_unit();
    stack.rotate(Axis::Z, az);
    stack.rotate(Axis::X, ax);
    stack.rotate(Axis::Y, angles[1].wrapping_neg());
    let out = stack.calc_vector(v);
    stack.pop();
    out
}

fn flatten_facing(q: &mut CollisionQuery, t: &PlayerTask) {
    if q.facing[2].abs() >= FACING_FLAT {
        q.normal[0] = ZERO;
        q.normal[1] = ZERO;
        q.facing[0] = ZERO;
        q.facing[1] = ZERO;
        q.ang_z = t.angles[2];
    }
}

/// fn_80043600: re-collide after a floor contact with the sphere moved to
/// the contact's head point (radius 5.1 fast or ascending onto dynamic
/// land, else 4.1, ramping with speed between jog and +0xF4) and, if the
/// re-test still finds a floor, rebuild the head point and slide from it.
/// The query is restored when it fails. `on_dynamic` is r13-0x78F8.
#[allow(clippy::too_many_arguments)]
pub fn reland(
    stack: &mut MatrixStack,
    land: &mut LandContext,
    t: &PlayerTask,
    w: &PlayerWork,
    q: &mut CollisionQuery,
    env: &GroundEnv,
    e: &Entry,
    on_dynamic: bool,
) -> bool {
    flatten_facing(q, t);
    let si = SlideInput {
        task_mode: t.mode as i8,
        character: w.character,
        f134: w.f134,
        row0_74: env.rows[0].f74,
        row1_74: env.rows[1].f74,
        entry_has_task: e.task.is_some(),
        center: q.center,
        up: q.up,
        global: env.global,
    };
    let ahead = any_wall_ahead(&si, &land.hits, q.motion);
    let saved = q.clone();
    let eight = on_dynamic && w.pnum == 8;
    let l = lift(
        env.mw.spd,
        w,
        q,
        env.global,
        if eight { LIFT_8 } else { RELAND_LIFT },
    );
    let by_task = !on_dynamic || t.flag & 0x2000 != 0;
    let lift_w = rotate_offset(stack, by_task, t.angles, q.facing, l);
    let rad = [ZERO, if eight { RADIUS_8 } else { q.radius }, ZERO];
    let rad_w = rotate_offset(stack, by_task, t.angles, q.facing, rad);
    if by_task {
        q.center = add(lift_w, add(t.pos, sub(q.head, q.center)));
    } else if e.flags & 0x100 != 0 {
        q.center = add(lift_w, sub(add(q.along, q.head), rad_w));
        q.motion = [ZERO; 3];
    } else {
        q.center = add(lift_w, sub(q.head, rad_w));
        if vector_angle(q.up, q.facing, None) >= 0x2c00 && nj_scalor(q.along) >= ONE {
            let s = nj_scalor(q.motion);
            q.motion = unit_vector(q.along).map(|c| c * s);
        }
    }
    let (sx, f4) = (w.spd[0], w.f_f4);
    q.radius = if sx >= f4 || w.last_attr & 0x100 != 0 || (w.spd[1] <= ZERO && e.flags & 0x100 != 0)
    {
        RELAND_FAST
    } else if sx >= w.jog && sx <= f4 {
        STEP_ALONG + (sx - w.jog) / (f4 - w.jog)
    } else {
        STEP_ALONG
    };
    let fail = |q: &mut CollisionQuery| {
        *q = saved.clone();
        false
    };
    if !land.collide(q, e.obj, false) {
        return fail(q);
    }
    if vector_angle(q.up, q.facing, None) > 0x2c00 {
        let gf = inner_product(env.global, q.facing);
        if !(inner_product(env.global, q.up) > gf) && e.flags & 0x100 == 0 {
            return fail(q);
        }
    }
    if inner_product(env.global, q.facing) > ZERO && w.status & 3 == 0 {
        return fail(q);
    }
    q.radius = saved.radius;
    q.motion = saved.motion;
    if ahead {
        q.along = saved.along;
        q.head = saved.head;
    } else {
        let rad = [ZERO, if eight { RADIUS_8 } else { q.radius }, ZERO];
        let rad = if by_task {
            rotate_offset(stack, true, [q.ang_x, t.angles[1], q.ang_z], q.facing, rad)
        } else {
            rotate_offset(stack, false, t.angles, q.facing, rad)
        };
        let mut out = [ZERO; 3];
        plane_distance(
            saved.head,
            &Plane {
                point: q.foot,
                normal: q.facing,
            },
            Some(&mut out),
        );
        q.head = add(rad, out);
        q.along = add(q.along, sub(q.foot, saved.foot));
    }
    q.center = saved.center;
    flatten_facing(q, t);
    true
}

const SLOPE_DOWN: f32 = f32::from_bits(0xbecc_cccd); // r2-0x74D0, -0.4
const HEAD_ON: f32 = f32::from_bits(0x3f05_1eb8); // r2-0x74CC, 0.52
const SLIDE_FAST: f32 = f32::from_bits(0x3fec_cccd); // r2-0x74C8, 1.85
const SLIDE_SOFT: f32 = f32::from_bits(0x3f94_7ae1); // r2-0x74BC, 1.16
const UNDERSIDE: f32 = f32::from_bits(0xbeae_147b); // r2-0x74C0, -0.34
const BEHIND: f32 = f32::from_bits(0xbe4c_cccd); // r2-0x7568, -0.2
const PUSH_POS: f32 = 3.0; // r2-0x75F4
const PUSH_VEL: f32 = 2.0; // r2-0x75FC

/// What fn_80043ED4 needs besides `GroundEnv`: r13-0x7900, and
/// fn_8004E560 as (task, player) -> (result, out) for both of its calls.
pub struct FloorExtras<'a> {
    pub probe: bool,
    pub analog: &'a mut dyn FnMut(&PlayerTask, &PlayerWork) -> (bool, f32),
}

/// The offset rotation the floor pass uses: the task's, or with land flag
/// 0x100 the contact facing's tilt with the task's Y.
fn floor_rotate(
    stack: &mut MatrixStack,
    t: &PlayerTask,
    q: &CollisionQuery,
    flag_100: bool,
    v: Vec3,
) -> Vec3 {
    rotate_offset(stack, !flag_100, t.angles, q.facing, v)
}

#[allow(clippy::too_many_arguments)]
fn wall_settle(
    stack: &mut MatrixStack,
    t: &mut PlayerTask,
    vel: &mut MoveVector,
    w: &mut PlayerWork,
    q: &mut CollisionQuery,
    env: &mut GroundEnv,
    e: &Entry,
    start: Vec3,
    offset: Vec3,
    lifted: Vec3,
    probe: bool,
    ret: i32,
) -> i32 {
    let tw = TurnWork {
        status: w.status,
        spd: w.spd,
        jog: w.jog,
        character: w.character,
        f134: w.f134,
        row0_74: env.rows[0].f74,
        row1_74: env.rows[1].f74,
    };
    let mut st = SettleTask {
        mode: t.mode,
        flag: t.flag,
        angles: t.angles,
        pos: t.pos,
    };
    let mut sw = SettleWork {
        spd: w.spd,
        f_f0: w.f_f0,
        weight: w.weight,
        last_attr: w.last_attr,
        w_c0: w.w_c0,
        h_12: w.h12,
        wall: w.wall,
    };
    let mut sq = SlideQuery {
        center: q.center,
        motion: q.motion,
        up: q.up,
        facing: q.facing,
        head: q.head,
        along: q.along,
    };
    let si = SettleInput {
        point: start,
        offset,
        offset_stop: lifted,
        flags: e.flags,
        stuck: env.stuck,
        backward_probe: probe,
        stage: env.stage,
        foot: q.foot,
    };
    let r = settle_ground_contact(
        stack,
        &mut st,
        &mut vel.v,
        &tw,
        &mut sw,
        &mut sq,
        &mut env.mw,
        &si,
        ret,
    );
    t.angles = st.angles;
    t.pos = st.pos;
    w.spd = sw.spd;
    w.wall = sw.wall;
    w.h12 = sw.h_12;
    q.center = sq.center;
    q.motion = sq.motion;
    q.along = sq.along;
    r
}

/// fn_80043ED4: the floor pass, the counterpart of `ground_pass` for the
/// walker states that keep to the floor. It clears the dynamic-land
/// (r13-0x78F8) and stuck (r13-0x78F4) flags, builds the sphere (tilted
/// with the floor normal when `last_attr` has 0x100), sorts the listed land
/// with fn_80041C78 / fn_800414F0, then for each contact classifies it
/// (reach, slope ahead, motion into it, wall or floor, re-landing with
/// fn_80043600), slides along floors with fn_80040F48, tilts the task,
/// rebuilds the offsets and moves onto the floor, or settles against walls
/// with fn_80042E0C. Returns the slide code.
#[allow(clippy::too_many_arguments)]
pub fn floor_pass(
    stack: &mut MatrixStack,
    land: &mut LandContext,
    t: &mut PlayerTask,
    vel: &mut MoveVector,
    w: &mut PlayerWork,
    q: &mut CollisionQuery,
    env: &mut GroundEnv,
    x: &mut FloorExtras,
    ride_callback: &mut dyn FnMut(&mut Vec3),
    f26_in: f32,
) -> i32 {
    let mut ret = 0;
    let mut on_dynamic = false;
    env.stuck = false;
    let old_status = w.status;
    w.status = t.flag;
    t.flag &= !1;
    q.up = [ZERO, ONE, ZERO];
    q.ang_x = t.angles[0];
    q.ang_z = t.angles[2];
    let half = [ZERO, HALF * w.f134, ZERO];
    let tilt = w.last_attr & 0x100 != 0;
    let fn_ = w.floor_normal;
    let rot0 = |stack: &mut MatrixStack, v: Vec3| rotate_offset(stack, !tilt, t.angles, fn_, v);
    q.up = rot0(stack, q.up);
    let mut offset = rot0(stack, half);
    let l = lift(vel.v, w, q, env.global, half[1]);
    let mut lifted = rot0(stack, l);
    q.center = add(t.pos, lifted);
    q.radius = HALF * w.f134;
    q.motion = add(vel.v, w.v4c);
    let start = t.pos;
    if land.broadphase(q) == 0 {
        return no_land(t, vel, w, env);
    }
    let mut collected: Vec<Entry> = vec![];
    let mut picked: Option<usize> = None;
    let list: Vec<Entry> = land.list().to_vec();
    for e in &list {
        if e.flags & 0x2002 != 0 {
            continue;
        }
        if w.status & 3 == 0 && e.flags & 0x0100_0000 != 0 {
            continue;
        }
        if let Some(task) = e.task {
            if !(env.task_has_collision)(task) {
                continue;
            }
        }
        if land.collide(q, e.obj, false) {
            let si = slide_input(t, w, env, q, e);
            if w.status & 3 != 0 {
                let speeds = PushSpeeds {
                    spd_x: w.spd[0],
                    f_f0: w.f_f0,
                    jog: w.jog,
                    run: w.run,
                };
                let (code, stuck) =
                    wall_push(&si, e.flags, &speeds, &land.hits, &mut q.motion, &mut vel.v);
                env.stuck |= stuck;
                if code != 0 {
                    picked = Some(collected.len());
                    ret = ret.max(code);
                }
            } else {
                let r = slide_grounded(&si, &land.hits, &mut q.motion, &mut vel.v, f26_in);
                env.stuck |= r.stuck;
                if r.hit {
                    picked = Some(collected.len());
                }
            }
        }
        collected.push(*e);
        if e.flags & 0x100 != 0 {
            on_dynamic = true;
        }
        if collected.len() >= COLLECT_LIMIT {
            break;
        }
    }
    if collected.is_empty() {
        no_land(t, vel, w, env);
        return ret;
    }
    let order = contact_order(&collected, picked, w.ride.is_some(), w.landed_on);
    q.center = add(t.pos, lifted);
    q.motion = add(vel.v, w.v4c);
    w.ride = None;
    let old_grounded = old_status & 3 != 0;
    let mut last_landed: Option<usize> = None;
    for e in &order {
        let e = *e;
        let mut ride_task = e.task;
        if !land.collide(q, e.obj, false) {
            continue;
        }
        let flag_100 = e.flags & 0x100 != 0;
        // Bit 1: within reach. Bit 2: a floor point (or flagged as one).
        // Bit 4: no steep drop ahead. Bit 8: not behind the motion.
        // Bit 0x10: an underside contact.
        let mut bits = 1u32;
        if t.flag & 1 != 0 && last_landed != Some(e.obj) {
            let d = if on_dynamic {
                sub(add(q.foot, q.along), t.pos)
            } else {
                sub(sub(add(q.head, q.along), offset), t.pos)
            };
            let s = nj_scalor(d);
            if s != ZERO {
                let d = unit_vector(d);
                let f1 = if flag_100 && d[1] > ZERO {
                    let a = (BAMS_PER_RAD * acosf(d[1]) as f64) as i32;
                    a as f32
                } else {
                    inner_product(d, q.facing)
                };
                if f1 < BEHIND || s > w.f134 {
                    bits = 0;
                }
            }
        }
        bits |= 4;
        if t.flag & 3 != 0 && t.flag & 0x2000 == 0 {
            let mut c = sub(add(q.foot, q.along), q.center);
            if on_dynamic {
                c[1] = ZERO;
            }
            if nj_scalor(c) != ZERO {
                let c = unit_vector(c);
                let dir = [if w.spd[0] >= ZERO { ONE } else { NEG_ONE }, ZERO, ZERO];
                let dv = rotate_by_angles(stack, t.angles, dir);
                if !(on_dynamic && dv[1].abs() == ONE) {
                    let dv = unit_vector(dv);
                    if inner_product(c, dv) < SLOPE_DOWN {
                        bits &= !4;
                    }
                }
            }
        }
        bits |= 8;
        if w.status & 3 != 0
            && t.flag & 0x2000 == 0
            && w.last_attr & 0x100 == 0
            && nj_scalor(vel.v) != ZERO
        {
            let m = unit_vector(vel.v);
            let h = add(lifted, sub(add(q.foot, q.along), q.center));
            let hs = nj_scalor(h);
            if hs != ZERO && hs > ONE {
                let h = unit_vector(h);
                if inner_product(h, m) < ZERO {
                    bits &= !8;
                }
            }
        }
        let mut ang = vector_angle(q.up, q.facing, None);
        if ang > 0x2c00 {
            if inner_product(env.global, q.facing) < ZERO {
                bits |= 2 | 0x10;
                ang = vector_angle(env.global.map(|c| NEG_ONE * c), q.facing, None);
            } else {
                reland(stack, land, t, w, q, env, &e, on_dynamic);
                ang = vector_angle(q.up, q.facing, None);
                bits |= 2;
            }
        } else {
            let along = q.along;
            let dvec = sub(q.foot, t.pos);
            if nj_scalor(along) <= ZERO || (w.status & 3 != 0 && inner_product(dvec, along) > ZERO)
            {
                bits |= 2;
            }
        }
        // Wall or floor?
        let wall = 'decide: {
            if t.flag & 2 != 0 || e.flags & 0x0040_1000 != 0 {
                break 'decide true;
            }
            let mut floor_ok = false;
            if ang <= 0x2c00 {
                let (on, _) = (x.analog)(t, w);
                let go_c38 =
                    on || !(inner_product(env.global, q.facing) >= ZERO) || w.status & 3 != 0;
                if go_c38 && bits & 5 == 5 {
                    floor_ok = true;
                }
            }
            if !floor_ok {
                let gf = inner_product(env.global, q.facing);
                if !(inner_product(env.global, q.up) > gf) && !(flag_100 && bits & 5 == 5) {
                    break 'decide true;
                }
            }
            if old_grounded && w.status & 3 == 0 {
                break 'decide true;
            }
            if bits & 8 == 0 {
                break 'decide true;
            }
            if inner_product(env.global, q.facing) > ZERO && w.status & 3 == 0 {
                break 'decide true;
            }
            if bits & 2 == 0 && !reland(stack, land, t, w, q, env, &e, on_dynamic) {
                break 'decide true;
            }
            false
        };
        if wall {
            ret = wall_settle(
                stack, t, vel, w, q, env, &e, start, offset, lifted, x.probe, ret,
            );
            continue;
        }
        if nj_scalor(vel.v) >= w.run {
            let m6 = t.mode == 6;
            if (!m6 && !flag_100) || (m6 && flag_100) {
                let m = unit_vector(vel.v);
                if inner_product(m, q.facing) > HEAD_ON && bits & 0x10 == 0 {
                    ret = wall_settle(
                        stack, t, vel, w, q, env, &e, start, offset, lifted, x.probe, ret,
                    );
                    continue;
                }
            }
        }
        let sy = w.spd[1];
        let land_here = sy <= ZERO || (sy > ZERO && w.status & 3 == 0) || bits & 0x10 != 0;
        if !land_here {
            ret = wall_settle(
                stack, t, vel, w, q, env, &e, start, offset, lifted, x.probe, ret,
            );
            continue;
        }
        // Land on this floor.
        last_landed = Some(e.obj);
        let wi = WallSlideInput {
            slide: slide_input(t, w, env, q, &e),
            entry_flags: 0,
            last_attr: w.last_attr,
            motion: q.motion,
            head: q.head,
            across: q.across,
        };
        let speeds = PushSpeeds {
            spd_x: w.spd[0],
            f_f0: w.f_f0,
            jog: w.jog,
            run: w.run,
        };
        let code = wall_slide_turn(
            &wi,
            e.flags,
            &speeds,
            &land.hits,
            &mut q.along,
            &mut vel.v,
            &mut vel.angles[1],
            &mut t.angles[1],
            &mut w.wall,
        );
        if code != 0 {
            if ret == 0 {
                ret = code;
            } else if code == 2 {
                ret = 2;
            }
        }
        let still = || {
            vel.v[0] == ZERO && vel.v[2] == ZERO && w.status & 1 != 0 && {
                let gu = inner_product(env.global, q.up);
                inner_product(env.global, q.facing) >= gu
            }
        };
        let speeds_t = TiltSpeeds {
            spd_x: w.spd[0],
            jog: w.jog,
            run: w.run,
            f_ec: w.f_ec,
            f_f4: w.f_f4,
        };
        if flag_100 {
            t.angles[2] = 0;
            t.angles[0] = 0;
        } else if e.flags & 0x1c != 0 {
            if !still() {
                tilt_to_contact(
                    stack,
                    &mut t.angles,
                    w.status,
                    &speeds_t,
                    (q.ang_x, q.ang_z),
                    q.facing,
                    q.up,
                    env.global,
                    e.flags,
                );
            }
        } else if bits & 0x10 != 0 {
            if w.status & 1 != 0 && diff_angle(t.angles[2], q.ang_z) > 0x4000 {
                t.angles[1] = sub_angle(0x8000, t.angles[1]);
                vel.angles[1] = t.angles[1];
            }
            t.angles[0] = q.ang_x;
            t.angles[2] = q.ang_z;
        } else if !still() {
            let (ax, az) = (q.ang_x, q.ang_z);
            if diff_angle(t.angles[2], az) > 0x4000 {
                q.ang_z = az.wrapping_sub(0x8000);
            }
            if q.facing[1] <= ZERO && diff_angle(q.ang_z, 0) < 0x4000 {
                q.ang_x = 0x8000 - ax;
            }
            if diff_angle(0xc800, q.ang_x) < 0x1000 && diff_angle(0, q.ang_z) > 0x7000 {
                q.ang_x = -0x8000 - q.ang_x;
                q.ang_z = 0x8000 - q.ang_z;
                t.angles[1] = sub_angle(0x8000, t.angles[1]);
                vel.angles[1] = t.angles[1];
            }
            tilt_to_contact(
                stack,
                &mut t.angles,
                w.status,
                &speeds_t,
                (q.ang_x, q.ang_z),
                q.facing,
                q.up,
                env.global,
                e.flags,
            );
        }
        if ret == 0 {
            q.up = [ZERO, ONE, ZERO];
            q.ang_x = t.angles[0];
            q.ang_z = t.angles[2];
            q.up = floor_rotate(stack, t, q, flag_100, q.up);
            offset = floor_rotate(stack, t, q, flag_100, half);
            let l = lift(vel.v, w, q, env.global, half[1]);
            lifted = floor_rotate(stack, t, q, flag_100, l);
        }
        vel.angles[0] = q.ang_x;
        vel.angles[2] = q.ang_z;
        w.floor_normal = q.facing;
        if flag_100 && nj_scalor(q.along) != ZERO {
            let s = nj_scalor(q.motion);
            q.along = unit_vector(q.along).map(|c| c * s);
        }
        q.motion = [ZERO; 3];
        if env.stuck {
            q.along = [ZERO; 3];
        }
        q.center = add(q.head, q.along);
        let plane = Plane {
            point: land.hits[q.best as usize].foot,
            normal: q.up,
        };
        let mut foot = [ZERO; 3];
        plane_distance(q.center, &plane, Some(&mut foot));
        q.center = add(offset, foot);
        if let Some(task) = e.task {
            w.ride = Some(task);
            let rec = (env.ride_record)(task, w.pnum);
            if !ride_mover(
                rec.as_ref(),
                &mut t.pos,
                &mut t.angles,
                q,
                offset,
                ride_callback,
            ) {
                ride_task = None;
            }
        } else {
            w.ride = None;
        }
        let newpos = sub(q.center, offset);
        let d = point_distance(t.pos, newpos);
        let moving =
            ride_task.is_some() || w.spd[0] != ZERO || w.spd[2] != ZERO || d > ONE || x.probe;
        if moving && (d > MOVE_MIN || x.probe) {
            t.pos = newpos;
        }
        let sx = w.spd[0];
        let gu = inner_product(env.global, q.up);
        if sx >= SLIDE_FAST || sx <= -SLIDE_FAST || gu < UNDERSIDE || flag_100 {
            t.flag |= 1;
            surface(t, w, e.flags, env);
            continue;
        }
        if (sx >= SLIDE_SOFT || sx <= -SLIDE_SOFT) && gu < UNDERSIDE {
            t.flag |= 1;
            surface(t, w, e.flags, env);
            continue;
        }
        surface(t, w, 0, env);
        if w.spd[0] > ZERO {
            for j in 0..3 {
                t.pos[j] = fmadds(PUSH_POS, q.normal[j], t.pos[j]);
            }
            vel.v[0] = fmadds(PUSH_VEL, q.normal[0], vel.v[0]);
            vel.v[2] = fmadds(PUSH_VEL, q.normal[2], vel.v[2]);
        }
    }
    if let Some(obj) = last_landed {
        w.landed_on = Some(obj);
        t.pos = add(t.pos, q.motion);
        return ret;
    }
    let mut stopped = false;
    if w.status & 1 != 0 && w.status & 0x4000 == 0 {
        let (on, level) = (x.analog)(t, w);
        let quiet = !on || (level <= ANALOG_LOW && w.h14 == 0);
        if quiet
            && w.spd[1] <= ZERO
            && w.spd[0] <= w.run
            && w.last_attr & 0x1c == 0
            && w.dotp > DOTP_FLAT
        {
            for j in 0..3 {
                t.pos[j] = fnmsubs(HALF, vel.v[j], t.pos[j]);
            }
            vel.v = [ZERO; 3];
            stopped = true;
        }
    }
    if !stopped {
        t.pos = add(t.pos, q.motion);
    }
    surface(t, w, 0, env);
    ret
}

/// The other object a CCL (object collision) contact found (fn_8004E924):
/// its hit info entry and its collision work's halfword +0.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CclHit {
    /// Info entry bytes +0, +1, +3 and float +0x14.
    pub b0: u8,
    pub b1: i8,
    pub b3: u8,
    pub radius: f32,
    /// The other collision work's halfword +0 (4 for a round shape).
    pub shape: u16,
}

/// The player's collision work (twp +0x2C) as 0x80045880 reads it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CclView {
    /// Halfwords +2 and +4.
    pub h2: i16,
    pub flags: u16,
    /// +0x90: the push vector the CCL pass left.
    pub push: Vec3,
    /// The first own info entry's byte +3 and float +0x14.
    pub self_b3: u8,
    pub self_radius: f32,
    /// The hit fn_8004E924 returns (the last entry whose other object's info
    /// has bit 0 in byte +2 and whose collision work has flag 4).
    pub hit: Option<CclHit>,
    /// Word +4 of the info entry +0xA2 selects in +0xA4's info array.
    pub pushed_info_w4: u32,
    /// fn_8004F11C(task, player): interaction flags with the touched object.
    pub interaction: u32,
    /// The carrier (object type 0x12) behind +0xA4, +0x9C, +0x34 when flag 1
    /// is set: its Y angle.
    pub carrier_ang_y: Option<i32>,
}

const FAST_HIT: f32 = 15.0; // r2-0x74E0
const HEAD_ON_PUSH: f32 = f32::from_bits(0xbf7a_e148); // r2-0x74B4, -0.98
const AIR_DROP: f32 = -0.5; // r2-0x74B0

/// fn_8004614C: slide along an object's push plane: the move vector turns to
/// where that plane meets the floor plane, keeping its length, and the task
/// and the move vector's aim turn by the same angle.
pub fn slide_on_push(
    t: &mut PlayerTask,
    vel: &mut MoveVector,
    w: &mut PlayerWork,
    ccl: &CclView,
    stage: i32,
) {
    if t.flag & 3 == 0
        || nj_scalor(vel.v) < ONE
        || w.spd[1] > ZERO
        || ccl.flags & 0x10 == 0
        || t.flag & 4 != 0
        || ccl.flags & 4 == 0
    {
        return;
    }
    let mut p = ccl.push;
    if stage == 0x5a {
        p[1] = ZERO;
    }
    if nj_scalor(p) == ZERO {
        return;
    }
    w.wall = unit_vector(p);
    let mut line = Line {
        point: [ZERO; 3],
        dir: [ZERO; 3],
    };
    plane_intersection(
        &Plane {
            point: [ZERO; 3],
            normal: w.wall,
        },
        &Plane {
            point: [ZERO; 3],
            normal: w.floor_normal,
        },
        Some(&mut line),
    );
    let u = if nj_scalor(vel.v) == ZERO {
        vel.v
    } else {
        unit_vector(vel.v)
    };
    if inner_product(line.dir, u) < ZERO {
        line.dir = line.dir.map(|c| -c);
    }
    let s = nj_scalor(vel.v);
    let old = vel.v;
    vel.v = line.dir.map(|c| s * c);
    let a = unit_vector(old);
    let b = unit_vector(vel.v);
    t.angles[1] = t.angles[1].wrapping_sub(vector_angle(a, b, Some(w.floor_normal)));
    if diff_angle(vel.angles[1], t.angles[1]) > 0x6000 {
        t.angles[1] = t.angles[1].wrapping_add(0x8000);
    }
    vel.angles[1] = t.angles[1];
}

/// 0x80045880: the walker's collision core. It clears the wall normal,
/// moves `attr` to `last_attr`, reacts to an object contact (CCL: stopping
/// on fast or head-on hits, sliding round a round object, along a push
/// plane, or around a carrier), runs the ground pass (task flag 0x100 or
/// mode 0x34) or the floor pass, and when the task is airborne afterwards
/// (with flag 2) resets the floor normal, drops the task by half a unit
/// along its up axis and, unless rising, flattens the move vector in the
/// task frame. Returns the first nonzero code.
#[allow(clippy::too_many_arguments)]
pub fn collision_core(
    stack: &mut MatrixStack,
    land: &mut LandContext,
    t: &mut PlayerTask,
    vel: &mut MoveVector,
    w: &mut PlayerWork,
    q: &mut CollisionQuery,
    env: &mut GroundEnv,
    ccl: &CclView,
    x: &mut FloorExtras,
    ride_callback: &mut dyn FnMut(&mut Vec3),
    f26_in: f32,
) -> i32 {
    let mut ret = 0;
    w.wall = [ZERO; 3];
    w.last_attr = w.attr;
    w.attr = 0;
    let hit = if ccl.flags & 0x14 != 0x14 || t.flag & 4 != 0 {
        None
    } else {
        ccl.hit
    };
    if let Some(h) = hit {
        let mut p = ccl.push;
        if env.stage == 0x5a {
            p[1] = ZERO;
        }
        p = if nj_scalor(p) == ZERO {
            [ZERO; 3]
        } else {
            unit_vector(p)
        };
        w.wall = p;
        let v = if nj_scalor(vel.v) == ZERO {
            [ZERO; 3]
        } else {
            unit_vector(vel.v)
        };
        let d = inner_product(v, p);
        let sx = w.spd[0];
        let gated = ccl.flags & 0x14 == 0x14
            && ccl.h2 > 0
            && t.flag & 4 == 0
            && (ccl.pushed_info_w4 & 0x2000 == 0 || t.flag & 0x800 != 0);
        if sx >= FAST_HIT && h.b0 != 0x10 {
            ret = 2;
            w.spd = [ZERO; 3];
            vel.v = [ZERO; 3];
        } else if sx >= w.f_f0 && d < HEAD_ON_PUSH && h.b0 != 0x10 {
            ret = 2;
            vel.v[0] = ZERO;
            vel.v[2] = ZERO;
        } else if sx >= w.run && d < ZERO {
            ret = 1;
            if matches!(h.b1, 0 | 1)
                && h.radius > ccl.self_radius
                && h.shape == 4
                && (h.b3 & 3) <= ((ccl.self_b3 >> 2) & 3)
            {
                let sum = ccl.self_radius + h.radius;
                let r = sqrtf(v[0] * v[0] + v[2] * v[2]) / sum;
                let mut turn = (BAMS_PER_RAD * acosf(r) as f64) as i32;
                let pa = ((BAMS_PER_RAD * atan2f(p[2], p[0]) as f64) as i32).wrapping_add(0x8000);
                let va = (BAMS_PER_RAD * atan2f(v[2], v[0]) as f64) as i32;
                if sub_angle(pa, va) < 0 {
                    turn = 0x10000 - turn;
                }
                stack.push_unit();
                stack.rotate(Axis::Y, turn.wrapping_neg());
                let r = stack.calc_vector(vel.v);
                stack.pop();
                t.angles[1] = t.angles[1].wrapping_add(turn);
                vel.v[0] = r[0];
                vel.v[2] = r[2];
            } else if (h.b1 as u8).wrapping_sub(3) <= 2 {
                if gated {
                    vel.v[0] = ZERO;
                    vel.v[2] = ZERO;
                } else {
                    slide_on_push(t, vel, w, ccl, env.stage);
                }
            }
        } else {
            ret = 1;
            if d > ZERO {
                if ccl.interaction & 0x400 != 0 {
                    vel.v[0] = -w.spd[0] * p[0];
                    vel.v[2] = -w.spd[0] * p[2];
                } else if gated {
                    let mut a = v;
                    a[1] = ZERO;
                    let mut b = p;
                    b[1] = ZERO;
                    if nj_scalor(a) == ZERO || nj_scalor(b) == ZERO {
                        vel.v[0] = ZERO;
                        vel.v[2] = ZERO;
                    } else {
                        let s = add(unit_vector(a), unit_vector(b));
                        if nj_scalor(s) == ZERO {
                            vel.v[0] = ZERO;
                            vel.v[2] = ZERO;
                        } else {
                            let s = unit_vector(s);
                            vel.v[0] = w.spd[0] * s[0];
                            vel.v[2] = w.spd[0] * s[2];
                        }
                    }
                } else {
                    slide_on_push(t, vel, w, ccl, env.stage);
                }
            }
        }
    } else if ccl.flags & 1 != 0 {
        if let Some(carrier) = ccl.carrier_ang_y {
            if t.flag & 1 != 0
                && !(w.spd[1] > ZERO)
                && (w.spd[0] >= w.run || t.flag & 0x100 != 0)
                && diff_angle(0x4000 - t.angles[1], carrier) < 0x3000
            {
                ret = 3;
                w.spd = [ZERO; 3];
                vel.v = [ZERO; 3];
            }
        }
    }
    let post = t.flag & 2 != 0;
    let r = if t.flag & 0x100 != 0 || t.mode == 0x34 {
        ground_pass(
            stack,
            land,
            t,
            vel,
            w,
            q,
            env,
            x.analog,
            ride_callback,
            f26_in,
        )
    } else {
        floor_pass(stack, land, t, vel, w, q, env, x, ride_callback, f26_in)
    };
    if ret == 0 {
        ret = r;
    }
    if !post || t.flag & 1 != 0 {
        return ret;
    }
    stack.push_unit();
    stack.rotate(Axis::Z, t.angles[2]);
    stack.rotate(Axis::X, t.angles[0]);
    stack.rotate(Axis::Y, t.angles[1].wrapping_neg());
    w.floor_normal = stack.calc_vector([ZERO, ONE, ZERO]);
    let drop = stack.calc_vector([ZERO, AIR_DROP, ZERO]);
    stack.pop();
    t.pos[1] += drop[1];
    if w.spd[1] <= ZERO {
        let mut l = unrotate_by_angles(stack, t.angles, vel.v);
        l[1] = ZERO;
        w.spd = l;
        vel.v = rotate_by_angles(stack, t.angles, l);
    } else {
        t.flag &= !2;
    }
    ret
}
