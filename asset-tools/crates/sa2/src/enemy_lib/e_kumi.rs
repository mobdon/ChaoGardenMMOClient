//! E KUMI (Prison Lane's floating mines and gun pods), stg09D.rel: row
//! callback 0x4C408 and its routines 0x4A3A8..0x4C01C. Eight kinds from
//! trunc(Scale.x):
//!
//! * 0: bobs in place; 1: flies and explodes on touching the player;
//! * 2, 3: bob (3 flies), then sink away (state 5) and come back on a timer;
//! * 4: a gun pod that shoots bursts (state 7);
//! * 5: appears, spins and leaves like E GOLD (states 2 to 4);
//! * 6: a spring the player bounces on;
//! * 7: circles home and drops children (state 6).
//!
//! It always joins its switch group (set-up calls fn_80018C88).
use super::{
    e_ai::{creators, translate},
    engine::{self, GroupCounts, BAMS_PER_RADIAN},
    host::{EnemyHost, Event},
    motion::MotionRow,
    EnemyTask, EnemyWork, Rand, V3,
};
use crate::{
    msl_math::{asinf, atan2f},
    ninja_matrix::{mult_vec, mult_vec_sr, Axis, MatrixStack, IDENTITY},
    ninja_trig::{fctiwz, fmadds, nj_cos, nj_sin},
};

/// Section-5 offsets of E KUMI's data in stg09D.rel.
pub mod rows {
    /// CollisionData rows (kind 6 uses the pair at +0x60).
    pub const COLLISION: u32 = 0xAD1CC;
    pub const MOTION: u32 = 0xAD32C;
    pub const MOTION_ROWS: usize = 6;
    /// Models: kinds 0..3 and 5; kind 4; kind 7; kind 6.
    pub const MODEL_MINE: u32 = 0xA6A94;
    pub const MODEL_GUN: u32 = 0xA8FF8;
    pub const MODEL_CIRCLER: u32 = 0xAB1B4;
    pub const MODEL_SPRING: u32 = 0xAC33C;
}

/// The REL child-task creators only E KUMI calls.
pub mod kumi_creators {
    pub const SETUP_7: u32 = 0x427EC;
    pub const DROP: u32 = 0x4286C;
}

fn k(bits: u32) -> f32 {
    f32::from_bits(bits)
}

#[derive(Debug, Clone)]
pub struct EKumi {
    pub task: EnemyTask,
    pub work: EnemyWork,
    pub table: Vec<MotionRow>,
    /// The model the work points at (+0x1FC), a section-5 offset.
    pub model: u32,
}

fn child(host: &mut impl EnemyHost, creator: u32, args: Vec<V3>, ints: Vec<i32>) {
    host.event(Event::Child {
        creator,
        args,
        ints,
        floats: vec![],
    });
}

fn world_point(stack: &mut MatrixStack, task: &EnemyTask, v: V3) -> V3 {
    stack.push();
    *stack.current_mut() = IDENTITY;
    translate(stack.current_mut(), task.pos);
    stack.rotate(Axis::Y, task.ang[1]);
    let out = mult_vec(stack.current(), v);
    stack.pop();
    out
}

impl EKumi {
    pub fn new(task: EnemyTask, table: Vec<MotionRow>) -> Self {
        Self {
            task,
            work: EnemyWork::default(),
            table,
            model: 0,
        }
    }

    /// 0x4C408: one frame.
    pub fn exec(
        &mut self,
        host: &mut impl EnemyHost,
        stack: &mut MatrixStack,
        rand: &mut Rand,
        counts: &mut GroupCounts,
    ) {
        if self.task.mode != 0 && host.range_out() {
            return;
        }
        match self.task.mode {
            0 => return self.setup(host, stack, counts),
            1 | 7 => self.state_main(host, stack, rand, counts),
            2 => self.state_wait(host),
            3 => self.state_spin_up(host),
            4 => {
                self.state_leave(host);
                if self.task.mode != 4 {
                    host.event(Event::Hidden);
                }
            }
            5 => self.state_sunk(host, rand, counts),
            6 => self.state_circle(host, stack, rand, counts),
            8 => {
                self.task.mode = 9;
                host.event(Event::Free);
                return;
            }
            9 => {
                host.event(Event::Free);
                return;
            }
            _ => {}
        }
        if !host.game_37() {
            engine::save_position(&self.task, &mut self.work);
            let volume = match self.task.mode {
                1 | 5..=7 => Some(10),
                3 | 4 => Some(fctiwz(10.0 + 127.0 * self.work.f_ac)),
                _ => None,
            };
            if let Some(volume) = volume {
                host.event(Event::WorkSound {
                    id: 0x400D,
                    a: 1,
                    volume,
                    b: 0xA4,
                    pos: self.task.pos,
                });
            }
            self.work.motion.step(&self.table);
            if self.task.kind == 6 && self.task.timer & 0x10 != 0 {
                for z in [k(0xc0d8_f5c3), k(0x40d8_f5c3)] {
                    let p = world_point(stack, &self.task, [k(0xc056_6666), k(0xc105_1eb8), z]);
                    child(host, creators::NEAR_EFFECT, vec![p], vec![0]);
                }
            }
            if self.task.flags & 0x2000 != 0 {
                let v = if self.task.kind == 6 {
                    [11.0, 2.5, 0.0]
                } else {
                    [10.0, k(0x4093_3333), 0.0]
                };
                let p = world_point(stack, &self.task, v);
                child(host, creators::NEAR_EFFECT, vec![p], vec![1]);
            }
        }
        if self.task.mode != 8 {
            host.collision_entry();
        }
    }

    /// 0x4A3A8: set-up.
    fn setup(
        &mut self,
        host: &mut impl EnemyHost,
        stack: &mut MatrixStack,
        counts: &mut GroupCounts,
    ) {
        let g = host.game_7c();
        let t = &mut self.task;
        let mut w = engine::new_counted_work(t, counts);
        t.kind = fctiwz(engine::round_off(t.scl[0])) as i8;
        if !(0..8).contains(&t.kind) {
            t.kind = 0;
        }
        let kind = t.kind;
        match kind {
            2 | 3 => {
                host.event(Event::CollisionInit {
                    rows: rows::COLLISION,
                    count: 2,
                });
                host.event(Event::CollisionOff { entry: 1 });
                host.event(Event::CollisionRange { range: 15.0 });
            }
            6 => {
                host.event(Event::CollisionInit {
                    rows: rows::COLLISION + 0x60,
                    count: 2,
                });
                host.event(Event::CollisionRange { range: 18.0 });
            }
            _ => {
                host.event(Event::CollisionInit {
                    rows: rows::COLLISION,
                    count: 1,
                });
                host.event(Event::CollisionRange { range: 9.0 });
            }
        }
        match kind {
            2 | 3 => {
                w.interval = fctiwz(t.scl[1]) as i16;
                let n = (g as u32).wrapping_add(t.ang[0] as u32 & 0xff);
                let m = (w.interval as i32 + 0x12e) as u32;
                t.timer = (n % m) as u16;
                t.scl[1] = 2.0;
            }
            6 => w.f_98 = (t.ang[0] & 0xff) as f32,
            4 => {
                w.burst = (t.ang[0] & 0xf) as i8;
                if t.ang[0] & 0xf0 != 0 {
                    t.flags |= 0x8000u16 as i16;
                }
                w.interval = (t.ang[2] & 0xff) as i16;
                if w.interval < 60 {
                    w.interval = 60;
                }
                t.ang[2] &= 0xff00;
                w.f_94 = t.scl[1];
                t.scl[1] = 2.0;
                child(host, creators::SETUP_GUN, vec![], vec![]);
                engine::home_offset(stack, t, &mut w);
            }
            7 => {
                w.burst = t.ang[0] as i8;
                if !(0..5).contains(&w.burst) {
                    w.burst = 0;
                }
                w.circle_yaw = t.ang[1] & 0xff00;
                w.interval = fctiwz(t.scl[1]) as i16;
                t.scl[1] = 2.0;
                child(host, kumi_creators::SETUP_7, vec![], vec![]);
            }
            5 => {
                w.f_98 = 0.0;
                t.ang[2] &= !1;
            }
            _ => {}
        }
        t.ang[0] &= 0xff00;
        w.motion.step(&self.table);
        child(host, creators::SETUP_45EC8, vec![], vec![]);
        let r = 60.0 + t.scl[2];
        w.f_1d8 = r * r;
        w.i_1d4 = 0x4000;
        let r = 60.0 + t.scl[2];
        w.notice_range_sq = r * r;
        w.spd[0] = k(0x3ecc_cccd);
        w.turn_rate = 0x180;
        if kind == 1 || kind == 3 {
            let y = host.shadow_pos(t.pos);
            w.hover = if y == -1.0e6 { -20.0 } else { y - t.pos[1] };
        }
        w.bob = t.ang[0].wrapping_add(g.wrapping_mul(t.ang[2]));
        engine::set_home(t, &mut w);
        match kind {
            0 | 2 | 4 | 6 => {
                t.pos[1] =
                    w.home[1] + nj_sin(t.ang[0].wrapping_add(g.wrapping_mul(t.ang[2]))) * t.scl[1];
            }
            5 => t.pos[1] = w.home[1] + nj_sin(t.ang[0]) * t.scl[1],
            _ => {
                if kind == 7 {
                    t.ang[1] = w.circle_yaw.wrapping_add(t.ang[2].wrapping_mul(g));
                    let a = t.ang[1].wrapping_add(0x4000);
                    t.pos[0] = w.home[0] + t.scl[2] * nj_cos(a);
                    t.pos[2] = w.home[2] - t.scl[2] * nj_sin(a);
                }
                t.pos[1] = w.home[1] + nj_sin(g.wrapping_mul(t.ang[2])) * t.scl[1];
            }
        }
        match kind {
            5 => {
                t.mode = 2;
                self.model = rows::MODEL_MINE;
                w.motion.request = 0;
                host.event(Event::CollisionOff { entry: 0 });
            }
            4 => {
                t.mode = 7;
                self.model = rows::MODEL_GUN;
                w.motion.request = 2;
            }
            7 => {
                t.mode = 6;
                self.model = rows::MODEL_CIRCLER;
                w.motion.request = 1;
            }
            6 => {
                t.mode = 1;
                self.model = rows::MODEL_SPRING;
                w.motion.request = 4;
            }
            2 => {
                if t.timer as i32 > w.interval as i32 {
                    host.event(Event::CollisionOn { entry: 1 });
                    t.mode = 5;
                    t.timer = t.timer.wrapping_sub((w.interval as i32 + 1) as u16);
                } else {
                    t.mode = 1;
                }
                self.model = rows::MODEL_MINE;
                w.motion.request = 0;
            }
            _ => {
                t.mode = 1;
                self.model = rows::MODEL_MINE;
                w.motion.request = 0;
            }
        }
        w.motion.step(&self.table);
        w.f_ec = 12.0;
        w.f_f0 = k(0x3f99_999a);
        w.flags |= 0x10;
        w.flags |= 0x4;
        if host.game_3c() == 1 {
            if self.model == rows::MODEL_MINE
                && t.ang[2] & 1 != 0
                && host.kumi_display_buffer(rows::MODEL_MINE)
            {
                t.flags |= 0x4000;
            }
            if self.model == rows::MODEL_GUN && host.kumi_display_buffer(rows::MODEL_GUN) {
                t.flags |= 0x4000;
            }
        }
        self.work = w;
    }

    /// 0x4AE74: flag 0x1000 when farther from home (3D) than the notice
    /// range.
    fn leash_flag(&mut self, host: &impl EnemyHost) {
        if host.game_37() {
            return;
        }
        let (t, w) = (&self.task, &self.work);
        let dx = w.home[0] - t.pos[0];
        let dy = w.home[1] - t.pos[1];
        let dz = w.home[2] - t.pos[2];
        // fn_80017AB8.
        let d = dz * dz + (dx * dx + dy * dy);
        if d > w.notice_range_sq {
            self.task.flags |= 0x1000;
        } else {
            self.task.flags &= !0x1000;
        }
    }

    /// 0x4B1BC: flag 0x2000 when a player is near (kind 4: around home
    /// plus offset; others: around the enemy).
    fn notice(&mut self, host: &impl EnemyHost) {
        let (g39, g42) = host.game_39_42();
        let from = if self.task.kind == 4 {
            engine::NoticeFrom::HomeOffset
        } else {
            engine::NoticeFrom::Body
        };
        if engine::player_in_range(from, &self.task, &self.work, &host.players(), g39, g42) != 0 {
            self.task.flags |= 0x2000;
        } else {
            self.task.flags &= !0x2000;
        }
    }

    /// 0x4ADDC: turn toward home outside the leash (fn_800179F0), else
    /// toward the nearest player when one is near.
    fn turn(&mut self, host: &impl EnemyHost) {
        if host.game_37() {
            return;
        }
        if self.task.flags & 0x1000 != 0 {
            let (t, w) = (&mut self.task, &mut self.work);
            let a = atan2f(w.home[0] - t.pos[0], w.home[2] - t.pos[2]);
            w.target_yaw = engine::fctiwz_f64(BAMS_PER_RADIAN * a as f64).wrapping_sub(0x4000);
            t.ang[1] = engine::adjust_angle(t.ang[1], w.target_yaw, w.turn_rate);
        } else if self.task.flags & 0x2000 != 0 {
            let p = host.players();
            let i = engine::nearest_player(&p, self.task.pos);
            if let Some(pl) = p.get(i).copied().flatten() {
                engine::face_player(&mut self.task, &mut self.work, pl.pos);
            }
        }
    }

    /// 0x4B250: flying kinds (1, 3 and 7 in state 1).
    fn fly(&mut self, host: &mut impl EnemyHost) {
        if host.game_37() {
            return;
        }
        let w = &mut self.work;
        w.spd[0] += w.accel;
        w.spd[2] += w.side_accel;
        if w.spd[0] > k(0x3ecc_cccd) {
            w.spd[0] = k(0x3ecc_cccd);
        } else if w.spd[0] < k(0x3dcc_cccd) {
            w.spd[0] = k(0x3dcc_cccd);
        }
        let g = host.game_7c();
        let t = &mut self.task;
        let before = nj_sin(g.wrapping_sub(1).wrapping_mul(t.ang[2]));
        let now = nj_sin(g.wrapping_mul(t.ang[2]));
        t.pos[1] += (now - before) * t.scl[1];
        let s = w.spd[2] * nj_sin(t.ang[1]);
        t.pos[0] += w.spd[0] * nj_cos(t.ang[1]) + s;
        t.pos[1] += w.spd[1];
        let c = w.spd[2] * nj_cos(t.ang[1]);
        t.pos[2] += -(w.spd[0] * nj_sin(t.ang[1])) + c;
        host.ground(&mut self.task, &mut self.work);
        let (t, w) = (&mut self.task, &mut self.work);
        if w.shadow_y < (w.probe[1] + w.hover) - 2.0 * t.scl[1] {
            w.spd[1] -= k(0x3d4c_cccd);
            if w.spd[1] < -2.0 {
                w.spd[1] = -2.0;
            }
        } else {
            w.spd[1] *= k(0x3f73_3333);
            if w.spd[1].abs() < k(0x3dcc_cccd) {
                w.spd[1] = 0.0;
            }
        }
        engine::leash(t, w, 60.0 + t.scl[2]);
    }

    /// 0x4B494: bob in place with the frame counter.
    fn bob(&mut self, host: &impl EnemyHost) {
        let g = host.game_7c();
        let (t, w) = (&mut self.task, &mut self.work);
        let s = nj_sin(t.ang[0].wrapping_add(g.wrapping_mul(t.ang[2])));
        t.pos[1] = w.home[1] + s * t.scl[1];
        w.bob = w.bob.wrapping_add(t.ang[2]);
    }

    /// 0x4B50C: bob in place with its own step count (kind 5).
    fn bob_steps(&mut self) {
        let (t, w) = (&mut self.task, &mut self.work);
        let s = nj_sin(t.ang[0].wrapping_add(w.i_1cc.wrapping_mul(t.ang[2])));
        w.i_1cc = w.i_1cc.wrapping_add(1);
        t.pos[1] = w.home[1] + s * t.scl[1];
        w.bob = w.bob.wrapping_add(t.ang[2]);
    }

    /// 0x4B5C8: like E AI's aim, but the pitch starts from -0xC00.
    fn aim(&self, host: &impl EnemyHost, stack: &mut MatrixStack, muzzle: V3, dir: V3) -> V3 {
        let p = host.players();
        let i = engine::nearest_player(&p, self.task.pos);
        let target = p
            .get(i)
            .copied()
            .flatten()
            .map(|p| p.center)
            .unwrap_or([0.0; 3]);
        let d = engine::distance(muzzle, target);
        let (pitch, yaw) = if d != 0.0 {
            let pitch =
                engine::fctiwz_f64(BAMS_PER_RADIAN * asinf((target[1] - muzzle[1]) / d) as f64);
            let a = atan2f(muzzle[2] - target[2], target[0] - muzzle[0]);
            (pitch, engine::fctiwz_f64(BAMS_PER_RADIAN * a as f64))
        } else {
            (0, self.task.ang[1])
        };
        let pitch = engine::adjust_angle(-0xC00, pitch, 0x1800);
        let yaw = engine::adjust_angle(self.task.ang[1], yaw, 0x800);
        stack.push();
        *stack.current_mut() = IDENTITY;
        stack.rotate(Axis::Y, yaw);
        stack.rotate(Axis::Z, pitch);
        let out = mult_vec_sr(stack.current(), dir);
        stack.pop();
        out
    }

    fn tick(&mut self, host: &impl EnemyHost) {
        if !host.game_37() {
            self.task.timer = self.task.timer.wrapping_add(1);
        }
    }

    /// States 1 and 7 (0x4B728).
    fn state_main(
        &mut self,
        host: &mut impl EnemyHost,
        stack: &mut MatrixStack,
        rand: &mut Rand,
        counts: &mut GroupCounts,
    ) {
        self.leash_flag(host);
        self.notice(host);
        self.turn(host);
        match self.task.kind {
            1 | 3 | 7 => self.fly(host),
            5 => self.bob_steps(),
            _ => self.bob(host),
        }
        self.damage(host, rand, counts);
        if self.task.mode == 8 {
            return;
        }
        self.tick(host);
        match self.task.kind {
            2 | 3 => {
                if self.task.timer as i32 > self.work.interval as i32 {
                    self.task.mode = 5;
                    self.task.timer = 0;
                    host.event(Event::CollisionOn { entry: 1 });
                }
            }
            4 if self.work.burst > 0 => {
                if self.task.flags & 0x2000 == 0 {
                    self.task.timer = 0;
                    self.task.shots = 0;
                    return;
                }
                if self.task.timer as i32 <= self.work.interval as i32 {
                    return;
                }
                if self.work.motion.current != 3 {
                    self.work.motion.request = 3;
                    return;
                }
                let f = self.work.motion.frame;
                if f >= 1.0 && f < k(0x3f99_999a) {
                    let muzzle = world_point(stack, &self.task, [7.5, k(0xc113_3333), 0.0]);
                    let dir = self.aim(host, stack, muzzle, [self.work.f_94, 0.0, 0.0]);
                    let unflagged = (self.task.flags as u16 & 0x8000 == 0) as i32;
                    child(host, creators::SHOT, vec![dir, muzzle], vec![unflagged]);
                    self.task.shots = self.task.shots.wrapping_add(1);
                }
                if self.task.shots as i32 >= self.work.burst as i32 {
                    self.task.timer = 0;
                    self.task.shots = 0;
                }
            }
            _ => {}
        }
    }

    /// State 2 (0x4BCA8): kind 5 waits until a player is near.
    fn state_wait(&mut self, host: &mut impl EnemyHost) {
        self.notice(host);
        self.leash_flag(host);
        if self.task.flags & 0x2000 != 0 {
            self.task.mode = 3;
            self.work.f_ac = -1.0;
            self.work.scale = [1.0; 3];
            self.task.flags &= !0x2000;
            host.event(Event::Sound {
                id: 0x400F,
                volume: 0x7F,
                pos: self.task.pos,
            });
        }
        self.turn(host);
    }

    fn spin(&mut self, add: bool) {
        let a = self.task.ang[1] as f32;
        let d = 8192.0 * -self.work.f_ac;
        self.task.ang[1] = fctiwz(if add { a + d } else { a - d });
    }

    /// State 3 (0x4BD6C).
    fn state_spin_up(&mut self, host: &mut impl EnemyHost) {
        self.spin(true);
        let w = &mut self.work;
        w.f_98 = engine::approach(w.f_98, 1.0, k(0x3dcc_cccd));
        w.f_ec = 12.0 * w.f_98;
        if w.f_ac < 0.0 {
            w.f_ac = engine::approach(w.f_ac, 0.0, k(0x3d4c_cccd));
        } else if w.scale[0] > 0.0 {
            w.scale[0] = engine::approach(w.scale[0], 0.0, k(0x3dcc_cccd));
            w.scale[1] = w.scale[0];
            w.scale[2] = w.scale[0];
        } else {
            self.task.mode = 1;
            self.task.timer = 0;
            host.event(Event::CollisionOn { entry: 0 });
        }
    }

    /// State 4 (0x4BEB4).
    fn state_leave(&mut self, host: &mut impl EnemyHost) {
        self.spin(false);
        let w = &mut self.work;
        if w.f_ac <= -0.5 {
            w.f_98 = engine::approach(w.f_98, 0.0, k(0x3dcc_cccd));
            w.f_ec = 12.0 * w.f_98;
        }
        if w.scale[0] < 1.0 {
            w.scale[0] = engine::approach(w.scale[0], 1.0, k(0x3dcc_cccd));
            w.scale[1] = w.scale[0];
            w.scale[2] = w.scale[0];
        } else if w.f_ac > -1.0 {
            w.f_ac = engine::approach(w.f_ac, -1.0, k(0x3d4c_cccd));
        } else {
            self.task.mode = 8;
            host.event(Event::CollisionOff { entry: 0 });
        }
    }

    /// State 5 (0x4B9E8): kinds 2 and 3 sunk; back to state 1 after 300
    /// frames (when they have an interval).
    fn state_sunk(&mut self, host: &mut impl EnemyHost, rand: &mut Rand, counts: &mut GroupCounts) {
        self.bob(host);
        self.damage(host, rand, counts);
        if self.task.mode == 8 {
            return;
        }
        if !host.game_37() {
            self.task.timer = self.task.timer.wrapping_add(1);
            self.work.b_46 = self.work.b_46.wrapping_add(1);
            if self.work.b_46 >= 9 {
                self.work.b_46 = 0;
            }
            self.work.h_4e = self.work.h_4e.wrapping_add(1);
            if self.task.timer & 1 != 0 {
                host.event(Event::Burrow { pos: self.task.pos });
            }
            host.event(Event::WorkSound {
                id: 0x400E,
                a: 1,
                volume: 0xF,
                b: 0x3C,
                pos: self.task.pos,
            });
        }
        if self.work.interval != 0 && self.task.timer > 300 {
            self.task.mode = 1;
            self.task.timer = 0;
            host.event(Event::CollisionOff { entry: 1 });
        }
    }

    /// State 6 (0x4BB28): kind 7 circles home and drops a child every
    /// interval.
    fn state_circle(
        &mut self,
        host: &mut impl EnemyHost,
        stack: &mut MatrixStack,
        rand: &mut Rand,
        counts: &mut GroupCounts,
    ) {
        self.bob(host);
        if !host.game_37() {
            let (t, w) = (&mut self.task, &mut self.work);
            let a = t.ang[1].wrapping_add(0x4000);
            t.pos[0] = w.home[0] + t.scl[2] * nj_cos(a);
            t.pos[2] = w.home[2] - t.scl[2] * nj_sin(a);
            t.ang[1] = w
                .circle_yaw
                .wrapping_add(t.ang[2].wrapping_mul(host.game_7c()));
        }
        self.damage(host, rand, counts);
        if self.task.mode == 8 {
            return;
        }
        self.tick(host);
        if self.task.timer as i32 > self.work.interval as i32 {
            let p = world_point(stack, &self.task, [0.0, -6.0, 0.0]);
            child(
                host,
                kumi_creators::DROP,
                vec![p],
                vec![0, self.work.burst as i32, 1],
            );
            self.task.timer = 0;
        }
    }

    /// 0x4AF00: hits. Kind 1 also explodes on touching a player; kind 6
    /// bounces a player that touches its spring.
    fn damage(&mut self, host: &mut impl EnemyHost, rand: &mut Rand, counts: &mut GroupCounts) {
        let touched = self.task.kind == 1 && host.hit_player();
        if self.task.kind == 6 {
            if let Some(p) = host.spring_touch() {
                host.event(Event::Spring {
                    player: p,
                    speed: self.work.f_98,
                });
                self.work.motion.request = 5;
                host.event(Event::PlayerHurt {
                    player: p,
                    a: 4,
                    b: 15,
                });
                return;
            }
        }
        let hit = if self.task.flags & 4 != 0 {
            if host.hit_by_player() {
                self.work.flags |= 0x1000;
            }
            self.work.f_1c0 = k(0x3eb3_3333);
            true
        } else {
            host.damage_hook(self.task.pos)
        };
        if !hit && !touched {
            return;
        }
        self.task.mode = 8;
        host.event(Event::CollisionOff { entry: 0 });
        self.task.timer = 0;
        engine::set_home(&self.task, &mut self.work);
        host.event(Event::Debris);
        let mut points = 100;
        if host.score_doubled() {
            points <<= 1;
        }
        host.event(Event::Score { points });
        engine::leave_group(&mut self.work, counts);
        self.task.flags |= 0x100;
        if self.work.flags & 0x1c != 0 {
            let animal = (self.work.flags & 4 != 0).then(|| {
                let u = k(0x3800_0000) * rand.rand() as f32;
                let timer = fctiwz(fmadds(8.0, u, 4.0)) as i16;
                let u = k(0x3800_0000) * rand.rand() as f32;
                (timer, fctiwz(fmadds(2.0, u, 1.0)) as i8)
            });
            host.event(Event::Drop {
                flags: self.work.flags & 0x1c,
                animal,
            });
        }
        if self.work.flags & 0x400 != 0 {
            host.event(Event::Carried {
                player: self.work.b_44 as i32,
                model: self.model,
            });
        }
    }
}
