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
    msl_math::normal_angles,
    ninja_matrix::{Axis, MatrixStack, Vec3},
    ninja_trig::fnmsubs,
    ninja_vector::{inner_product, vector_angle},
    walker_collide::{
        apply_surface, diff_angle, plane_distance, point_distance, settle_contact, slide_airborne,
        slide_grounded, sphere_lift, tilt_to_contact, wall_slide, LiftInput, MotionWork,
        PhysicsRow, Plane, SlideInput, SlideQuery, SurfaceWork, TiltSpeeds, TurnWork,
        WallSlideInput,
    },
};

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
