//! E AI (Prison Lane's flying and walking gun robots), the stage REL's E
//! EMI-family code: stg09D.rel 0x4E8EC..0x51358 on the GameCube disc
//! (offsets are the same in the PC REL).
//!
//! `EAi::exec` is the row callback 0x51034: a 12-state dispatcher. Its
//! states call the family's shared routines, which keep their REL offsets
//! as names here. Child tasks (bullets, jets, effects), sounds and the
//! game's collision and culling go through `EnemyHost`.
use super::{
    engine::GroupCounts,
    engine::{self, NoticeFrom, BAMS_PER_RADIAN},
    host::{EnemyHost, Event},
    motion::MotionRow,
    EnemyTask, EnemyWork, Rand, V3,
};
use crate::{
    msl_math::{asinf, atan2f},
    ninja_matrix::{mult_vec, mult_vec_sr, Axis, MatrixStack, Mtx, IDENTITY},
    ninja_trig::{fctiwz, fmadds, nj_cos, nj_sin},
};

/// A section-4 (or main.dol SDA2) float constant, by its bits.
fn k(bits: u32) -> f32 {
    f32::from_bits(bits)
}

/// Section-5 offsets of E AI's data in stg09D.rel.
pub mod rows {
    /// CollisionData rows (kind 5 uses the pair at +0x60).
    pub const COLLISION: u32 = 0xC3DC8;
    /// The motion table (17 rows).
    pub const MOTION: u32 = 0xC3EC8;
    pub const MOTION_ROWS: usize = 17;
    /// The models: kinds 0, 1, 4, 6, 8, 9; kind 5; kinds 2, 3, 7.
    pub const MODEL_A: u32 = 0xB7B50;
    pub const MODEL_B: u32 = 0xBE504;
    pub const MODEL_C: u32 = 0xBB144;
}

/// The REL's child-task creators E AI calls (section-1 offsets).
pub mod creators {
    pub const SETUP_GUN: u32 = 0x449CC;
    pub const SETUP_4_8: u32 = 0x43510;
    pub const SETUP_TURRET: u32 = 0x455BC;
    pub const SETUP_45EC8: u32 = 0x45EC8;
    pub const SETUP_43E5C: u32 = 0x43E5C;
    pub const JET: u32 = 0x43ED0;
    pub const SHOT: u32 = 0x44BA0;
    pub const BEAM: u32 = 0x45638;
    pub const SHOT_4: u32 = 0x4358C;
    pub const DUST: u32 = 0x3D71C;
    pub const NEAR_EFFECT: u32 = 0x45F3C;
}

/// Ninja's matrix calls as the REL makes them on the current matrix.
pub(crate) fn translate(m: &mut Mtx, v: V3) {
    // __internal_njTranslate (paired singles): per row,
    // (m2*z + m0*x) + (m3*1 + m1*y), the madds fused.
    for r in m.iter_mut() {
        let a = fmadds(r[2], v[2], r[0] * v[0]);
        let b = fmadds(r[3], 1.0, r[1] * v[1]);
        r[3] = a + b;
    }
}

/// 0x4F9E0: a point in the enemy's frame (position, then yaw) to world.
fn to_world_point(stack: &mut MatrixStack, task: &EnemyTask, v: V3) -> V3 {
    stack.push();
    *stack.current_mut() = IDENTITY;
    translate(stack.current_mut(), task.pos);
    stack.rotate(Axis::Y, task.ang[1]);
    let out = mult_vec(stack.current(), v);
    stack.pop();
    out
}

/// 0x4FA48: a direction in the enemy's frame (yaw only) to world.
fn to_world_vector(stack: &mut MatrixStack, task: &EnemyTask, v: V3) -> V3 {
    stack.push();
    *stack.current_mut() = IDENTITY;
    stack.rotate(Axis::Y, task.ang[1]);
    let out = mult_vec_sr(stack.current(), v);
    stack.pop();
    out
}

/// One E AI (or E EMI-family) enemy.
#[derive(Debug, Clone)]
pub struct EAi {
    pub task: EnemyTask,
    pub work: EnemyWork,
    /// The motion table rows (stg09D.rel section 5 +0xC3EC8).
    pub table: Vec<MotionRow>,
    /// The model the work points at (+0x1FC), a section-5 offset.
    pub model: u32,
}

impl EAi {
    /// A task in state 0, before its first frame.
    pub fn new(task: EnemyTask, table: Vec<MotionRow>) -> Self {
        Self {
            task,
            work: EnemyWork::default(),
            table,
            model: 0,
        }
    }

    /// 0x51034: one frame.
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
        if host.set_entry_dead() {
            host.event(Event::Free);
            return;
        }
        match self.task.mode {
            0 => return self.setup(host, stack, counts),
            1 => self.state_wait(host, rand, counts),
            2 => self.state_guard(host, stack, rand, counts),
            3 => self.state_patrol(host, stack, rand, counts),
            5 => self.state_attack(host, stack, rand, counts),
            6 => self.state_drop_wait(host, rand, counts),
            7 => self.state_drop(host, stack, rand, counts),
            8 => {
                self.state_held(host, rand, counts);
                engine::save_position(&self.task, &mut self.work);
                self.work.motion.step(&self.table);
                return;
            }
            9 => self.state_idle(host, rand, counts),
            10 => {
                self.task.mode = 11;
                return self.dead(host);
            }
            11 => return self.dead(host),
            _ => {}
        }
        if !host.game_37() {
            engine::save_position(&self.task, &mut self.work);
            let before = self.work.motion.frame;
            self.work.motion.step(&self.table);
            let m = &self.work.motion;
            if m.mode != 2 {
                let marks: &[f32] = match m.current {
                    6 => &[1.0, 3.0],
                    7 | 16 => &[2.0, 6.0],
                    _ => &[],
                };
                // A footstep when the frame passes a mark.
                if marks.iter().any(|&t| m.frame >= t && before < t) {
                    host.event(Event::Sound {
                        id: 0x400B,
                        volume: -10,
                        pos: self.task.pos,
                    });
                }
            }
            if self.task.flags & 0x2000 != 0 {
                let n = if self.task.mode == 7 || self.work.motion.current == 2 {
                    4
                } else {
                    3
                };
                host.event(Event::Child {
                    creator: creators::NEAR_EFFECT,
                    args: vec![],
                    ints: vec![n],
                    floats: vec![],
                });
            }
        }
        if self.task.mode != 6 && self.task.mode != 10 {
            host.collision_entry();
        }
    }

    fn child(
        host: &mut impl EnemyHost,
        creator: u32,
        args: Vec<V3>,
        ints: Vec<i32>,
        floats: Vec<f32>,
    ) {
        host.event(Event::Child {
            creator,
            args,
            ints,
            floats,
        });
    }

    /// 0x4E8EC: state 0, the E EMI-family set-up.
    fn setup(
        &mut self,
        host: &mut impl EnemyHost,
        stack: &mut MatrixStack,
        counts: &mut GroupCounts,
    ) {
        use creators::*;
        let t = &mut self.task;
        t.kind = fctiwz(engine::round_off(t.scl[0])) as i8;
        if !(0..10).contains(&t.kind) {
            t.kind = 0;
        }
        let mut w = if t.kind == 9 {
            // fn_80018C10: counts the group with no -1 test.
            let mut w = engine::new_work(t);
            w.group = t.ang[1] as i8;
            t.ang[1] &= 0xff00;
            let c = &mut counts[w.group as u8 as usize];
            *c = c.wrapping_add(1);
            w.flags |= 0x20;
            w
        } else {
            engine::new_counted_work(t, counts)
        };
        host.event(Event::CollisionInit {
            rows: rows::COLLISION + if t.kind == 5 { 0x60 } else { 0 },
            count: 2,
        });
        let kind = t.kind;
        let burst_and_interval = |w: &mut EnemyWork, t: &EnemyTask, min: bool| {
            w.burst = t.ang[0] as i8;
            w.interval = t.ang[2] as i16;
            if min && w.interval < 60 {
                w.interval = 60;
            }
            w.f_94 = t.scl[1];
        };
        match kind {
            0 | 1 | 6 => {
                burst_and_interval(&mut w, t, true);
                self.model = rows::MODEL_A;
                Self::child(host, SETUP_GUN, vec![], vec![], vec![]);
            }
            4 | 8 => {
                burst_and_interval(&mut w, t, true);
                self.model = rows::MODEL_A;
                Self::child(host, SETUP_GUN, vec![], vec![], vec![]);
                Self::child(host, SETUP_4_8, vec![], vec![], vec![]);
            }
            2 | 3 | 5 | 7 => {
                burst_and_interval(&mut w, t, false);
                Self::child(host, SETUP_TURRET, vec![], vec![], vec![]);
                self.model = if kind == 5 {
                    rows::MODEL_B
                } else {
                    rows::MODEL_C
                };
            }
            _ => {
                w.burst = 0;
                w.interval = 0x7fff;
                w.f_94 = 0.0;
                self.model = rows::MODEL_A;
            }
        }
        let t = &mut self.task;
        t.ang[0] = 0;
        t.ang[2] = 0;
        t.scl[1] = 2.0;
        engine::home_offset(stack, t, &mut w);
        w.f_1d8 = 6400.0;
        w.i_1d4 = 0x71c7;
        let r = 60.0 + t.scl[2];
        w.notice_range_sq = r * r;
        w.spd[0] = 1.0;
        w.turn_rate = 0x800;
        Self::child(host, SETUP_45EC8, vec![], vec![], vec![]);
        engine::set_home(t, &mut w);
        match kind {
            1 | 3 => {
                t.mode = 3;
                w.motion.request = 8;
                w.hover = -12.0;
                w.f_e0 = k(0x3f7f_f972);
                host.event(Event::TaskSound {
                    id: 0x401C,
                    volume: -15,
                });
            }
            5 => {
                t.mode = 2;
                w.motion.request = 15;
                w.hover = -6.0;
                t.b2 = 8;
            }
            6..=8 => {
                t.mode = 6;
                w.motion.request = 8;
                w.hover = -6.0;
                w.f_dc = 0.0;
            }
            9 => {
                t.mode = 9;
                w.motion.request = 3;
                w.hover = -6.0;
                w.f_dc = 0.0;
            }
            _ => {
                t.mode = 1;
                w.motion.request = 1;
                host.set_collision_radius(10.0);
            }
        }
        w.motion.step(&self.table);
        w.f_ec = 6.0;
        w.f_f0 = 1.5;
        w.flags |= 0x10;
        w.flags |= 0x4;
        if host.set_entry_dead() {
            t.mode = 11;
        }
        Self::child(host, SETUP_43E5C, vec![], vec![], vec![]);
        self.work = w;
    }

    /// 0x4EDD8: a new wander target within a fifth of the notice range of
    /// home, and the yaw toward it.
    fn pick_target(&mut self, rand: &mut Rand) {
        let w = &mut self.work;
        let r = engine::sqrtf_rel(w.notice_range_sq);
        let u = |rand: &mut Rand| k(0x3800_0000) * rand.rand() as f32;
        let dx = k(0x3e4c_cccd) * (r * (u(rand) - 0.5));
        w.wander_target[0] = w.home[0] + dx;
        w.wander_target[1] = w.home[1];
        let dz = k(0x3e4c_cccd) * (r * (u(rand) - 0.5));
        w.wander_target[2] = w.home[2] + dz;
        let a = atan2f(
            w.wander_target[0] - self.task.pos[0],
            w.wander_target[2] - self.task.pos[2],
        );
        w.target_yaw = engine::fctiwz_f64(BAMS_PER_RADIAN * a as f64).wrapping_sub(0x4000);
    }

    fn wander_timer(rand: &mut Rand) -> i32 {
        let u = k(0x3800_0000) * rand.rand() as f32;
        fctiwz(60.0 + 60.0 * (3.0 * u))
    }

    /// 0x4EF1C: turning. Back toward a wander target when outside the
    /// leash, toward the nearest player when one is near, and (kinds 1 and
    /// 3) toward new wander targets on a timer or after bumping.
    fn turn(&mut self, host: &mut impl EnemyHost, rand: &mut Rand) {
        if host.game_37() {
            return;
        }
        let flags = self.task.flags;
        let wander = if flags & 0x1000 != 0 {
            let t = self.work.wander_timer;
            self.work.wander_timer = t.wrapping_sub(1);
            t < 0
        } else if flags & 0x2000 != 0 {
            let p = host.players();
            let i = engine::nearest_player(&p, self.task.pos);
            if let Some(pl) = p.get(i).copied().flatten() {
                engine::face_player(&mut self.task, &mut self.work, pl.pos);
            }
            return;
        } else if self.task.kind == 1 || self.task.kind == 3 {
            let t = self.work.wander_timer;
            self.work.wander_timer = t.wrapping_sub(1);
            t < 0 || host.bumped()
        } else {
            return;
        };
        if wander {
            self.pick_target(rand);
            self.work.wander_timer = Self::wander_timer(rand);
        } else {
            self.task.ang[1] =
                engine::adjust_angle(self.task.ang[1], self.work.target_yaw, self.work.turn_rate);
        }
    }

    /// 0x4F138: flag 0x1000 when farther from home than the notice range.
    fn leash_flag(&mut self, host: &impl EnemyHost) {
        if host.game_37() {
            return;
        }
        if engine::home_distance_sq(&self.task, &self.work) > self.work.notice_range_sq {
            self.task.flags |= 0x1000;
        } else {
            self.task.flags &= !0x1000;
        }
    }

    /// 0x4F538 (from home) and 0x4F5C8 (from home plus offset): flag
    /// 0x2000 when a player is within the notice range (or, for kind 5,
    /// while work +0x40 is set).
    fn notice(&mut self, host: &impl EnemyHost, from: NoticeFrom) {
        let (g39, g42) = host.game_39_42();
        let near = engine::player_in_range(from, &self.task, &self.work, &host.players(), g39, g42);
        if near != 0 || (self.task.kind == 5 && self.work.link_40) {
            self.task.flags |= 0x2000;
        } else {
            self.task.flags &= !0x2000;
        }
    }

    /// 0x4F658: flying movement.
    fn fly(&mut self, host: &mut impl EnemyHost, stack: &mut MatrixStack) {
        if host.game_37() {
            return;
        }
        let w = &mut self.work;
        w.accel = if w.spd[0] < -0.5 {
            k(0x3c23_d70a)
        } else {
            k(0x3dcc_cccd)
        };
        if host.hit_player() {
            w.spd[0] = -1.0;
        } else {
            w.spd[0] += w.accel;
            if w.accel <= 0.0 && w.spd[0].abs() <= w.accel.abs() {
                w.spd[0] = 0.0;
            }
            w.spd[2] += w.side_accel;
        }
        // Two compares; NaN passes through as it does here.
        w.spd[0] = w.spd[0].clamp(-1.0, 1.0);
        let t = &mut self.task;
        let s = w.spd[2] * nj_sin(t.ang[1]);
        t.pos[0] += w.spd[0] * nj_cos(t.ang[1]) + s;
        t.pos[1] += w.spd[1];
        let c = w.spd[2] * nj_cos(t.ang[1]);
        t.pos[2] += -(w.spd[0] * nj_sin(t.ang[1])) + c;
        t.pos[1] -= nj_sin(w.bob);
        w.bob = w.bob.wrapping_add(0x300);
        super::ground::collide(stack, &mut self.task, &mut self.work, host);
        let (t, w) = (&mut self.task, &mut self.work);
        t.pos[1] += nj_sin(w.bob);
        if w.shadow.coords[2] < w.probe[1] + w.hover {
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

    /// 0x4F910: falling (state 7).
    fn fall(&mut self, host: &mut impl EnemyHost, stack: &mut MatrixStack) {
        if host.game_37() {
            return;
        }
        self.task.pos[1] += self.work.spd[1];
        super::ground::collide(stack, &mut self.task, &mut self.work, host);
        let w = &mut self.work;
        if w.motion.current != 8 {
            w.spd[1] = 0.0;
        } else {
            w.spd[1] -= k(0x3dcc_cccd);
            w.spd[1] = w.spd[1].clamp(-3.0, -1.0);
        }
    }

    /// 0x4FAA8: the three jet puffs of a flying kind.
    fn jets(&mut self, host: &mut impl EnemyHost, stack: &mut MatrixStack) {
        if host.game_37() {
            return;
        }
        let puffs: [(V3, V3); 3] = [
            (
                [-7.0, 25.0, k(0x8000_0000)],
                [k(0xbf99_999a), k(0xbe99_999a), 0.0],
            ),
            ([-6.5, 23.0, -5.0], [-1.0, k(0xbf33_3333), -0.5]),
            ([-6.5, 23.0, 5.0], [-1.0, k(0xbf33_3333), 0.5]),
        ];
        for (p, v) in puffs {
            let p = to_world_point(stack, &self.task, p);
            let v = to_world_vector(stack, &self.task, v);
            Self::child(host, creators::JET, vec![p, v], vec![0, 1], vec![]);
        }
    }

    /// 0x4FC74: turn the shot direction `dir` toward the nearest player's
    /// collision centre from `muzzle`, by at most 0x1800 in pitch and
    /// 0x800 in yaw from the enemy's facing.
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
        let pitch = engine::adjust_angle(0, pitch, 0x1800);
        let yaw = engine::adjust_angle(self.task.ang[1], yaw, 0x800);
        stack.push();
        *stack.current_mut() = IDENTITY;
        stack.rotate(Axis::Y, yaw);
        stack.rotate(Axis::Z, pitch);
        let out = mult_vec_sr(stack.current(), dir);
        stack.pop();
        out
    }

    /// 0x4FDD4: a gun shot (kinds 0, 1 and 4).
    fn shoot(&mut self, host: &mut impl EnemyHost, stack: &mut MatrixStack) {
        let muzzle = if self.task.kind == 1 {
            [k(0x41b4_cccd), k(0x41b8_cccd), 3.0]
        } else {
            [20.0, 26.5, k(0x4096_6666)]
        };
        let muzzle = to_world_point(stack, &self.task, muzzle);
        let dir = self.aim(host, stack, muzzle, [self.work.f_94, 0.0, 0.0]);
        Self::child(host, creators::SHOT, vec![dir, muzzle], vec![1], vec![]);
    }

    /// 0x4FED8: a beam (kinds 2, 3 and 5), given the burst count.
    fn beam(&mut self, host: &mut impl EnemyHost, stack: &mut MatrixStack) {
        let muzzle = if self.task.kind == 3 {
            [k(0x41a3_3333), k(0x41a3_3333), k(0x4066_6666)]
        } else {
            [k(0x41a3_3333), k(0x41b3_3333), k(0x4099_999a)]
        };
        let muzzle = to_world_point(stack, &self.task, muzzle);
        let dir = self.aim(host, stack, muzzle, [self.work.f_94, 0.0, 0.0]);
        Self::child(
            host,
            creators::BEAM,
            vec![dir, muzzle],
            vec![],
            vec![self.work.burst as i32 as f32],
        );
    }

    /// 0x4FFFC: kind 4's other shot.
    fn shoot_4(&mut self, host: &mut impl EnemyHost, stack: &mut MatrixStack) {
        let muzzle = to_world_point(stack, &self.task, [20.0, 26.5, k(0x4096_6666)]);
        let dir = self.aim(host, stack, muzzle, [self.work.f_94, 0.0, 0.0]);
        Self::child(host, creators::SHOT_4, vec![dir, muzzle], vec![], vec![]);
    }

    fn timer_tick(&mut self, host: &impl EnemyHost) {
        if !host.game_37() {
            self.task.timer = self.task.timer.wrapping_add(1);
        }
    }

    fn timer_past_interval(&self) -> bool {
        self.task.timer as i32 > self.work.interval as i32
    }

    /// State 1 (0x5013C): wait until a player comes near.
    fn state_wait(&mut self, host: &mut impl EnemyHost, rand: &mut Rand, counts: &mut GroupCounts) {
        self.notice(host, NoticeFrom::HomeOffset);
        self.timer_tick(host);
        if self.task.flags & 0x2000 != 0 {
            self.work.motion.request = 2;
            self.task.mode = 2;
            self.task.timer = (self.work.interval as i32 - 60) as u16;
            host.event(Event::Sound {
                id: 0x400C,
                volume: 0x6E,
                pos: self.task.pos,
            });
        }
        self.damage(host, rand, counts);
    }

    /// State 2 (0x501FC): face the player and fire bursts.
    fn state_guard(
        &mut self,
        host: &mut impl EnemyHost,
        stack: &mut MatrixStack,
        rand: &mut Rand,
        counts: &mut GroupCounts,
    ) {
        self.notice(host, NoticeFrom::HomeOffset);
        if self.work.motion.request == 2 {
            let r = engine::approach(host.collision_radius(), 25.0, k(0x3ecc_cccd));
            host.set_collision_radius(r);
        }
        let cur = self.work.motion.current;
        if cur == 6 || cur == 3 || (cur == 5 && self.work.motion.frame > k(0x3f93_3333)) {
            let old = self.task.ang[1];
            self.turn(host, rand);
            let cur = self.work.motion.current;
            if cur == 6 || cur == 3 {
                self.work.motion.request = if engine::diff_angle(old, self.task.ang[1]) >= 0x20 {
                    6
                } else {
                    3
                };
            }
        } else if cur == 15 || cur == 16 || cur == 13 {
            let old = self.task.ang[1];
            self.turn(host, rand);
            let cur = self.work.motion.current;
            if cur == 16 || cur == 15 {
                self.work.motion.request = if engine::diff_angle(old, self.task.ang[1]) >= 0x20 {
                    16
                } else {
                    15
                };
            }
        }
        self.damage(host, rand, counts);
        if self.task.mode == 10 || self.task.mode == 8 {
            return;
        }
        self.timer_tick(host);
        let kind = self.task.kind;
        if kind == 5 {
            if self.work.motion.current == 13 || self.task.flags & 0x800 != 0 {
                self.task.b2 = 0;
                self.work.flags |= 0x800;
            } else {
                self.task.b2 = 8;
                self.work.flags &= !0x800;
            }
        }
        if (kind == 0 && self.work.burst > 0) || kind == 4 {
            if self.task.flags & 0x400 != 0 {
                self.task.flags &= !0x400;
                self.task.timer = 0;
            }
            if self.task.flags & 0x2000 == 0 {
                self.task.timer = 0;
                self.task.shots = 0;
                return;
            }
            if !self.timer_past_interval() {
                return;
            }
            let cur = self.work.motion.current;
            if cur == 3 || cur == 6 {
                self.work.motion.request = 5;
            } else if cur == 5 {
                let f = self.work.motion.frame;
                if f >= 1.0 && f < k(0x3f93_3333) {
                    let kind_4_other = kind != 0 && {
                        let p = host.players();
                        let i = engine::nearest_player(&p, self.task.pos);
                        host.player_mode(i) != 0x26
                    };
                    if kind_4_other {
                        self.shoot_4(host, stack);
                        self.task.shots = self.work.burst as u8;
                    } else {
                        self.shoot(host, stack);
                        self.task.shots = self.task.shots.wrapping_add(1);
                    }
                    if self.task.shots as i32 >= self.work.burst as i32 {
                        self.task.shots = 0;
                        self.task.timer = 0;
                    }
                }
            }
        } else if kind == 2 || kind == 5 {
            if self.task.flags & 0x2000 == 0 {
                self.task.timer = 0;
                self.task.flags &= !0x800;
                return;
            }
            if self.task.flags & 0x800 != 0 {
                self.task.flags &= !0x800;
                return;
            }
            if !self.timer_past_interval() {
                return;
            }
            let cur = self.work.motion.current;
            if cur == 3 || cur == 6 {
                self.work.motion.request = 4;
                self.beam(host, stack);
            } else if cur == 15 || cur == 16 {
                self.work.motion.request = 11;
            } else if cur == 11 && self.work.motion.ended & 2 != 0 {
                self.beam(host, stack);
            } else if cur == 12 {
                self.work.motion.request = 13;
                self.task.timer = 0;
            } else if cur == 4 {
                self.work.motion.request = 5;
                self.task.timer = 0;
            }
        }
    }

    /// State 3 (0x508F8): fly around home until a player comes near.
    fn state_patrol(
        &mut self,
        host: &mut impl EnemyHost,
        stack: &mut MatrixStack,
        rand: &mut Rand,
        counts: &mut GroupCounts,
    ) {
        self.leash_flag(host);
        self.notice(host, NoticeFrom::Home);
        self.turn(host, rand);
        self.fly(host, stack);
        self.jets(host, stack);
        self.damage(host, rand, counts);
        if self.task.mode == 10 || self.task.mode == 8 {
            return;
        }
        self.timer_tick(host);
        if self.task.flags & 0x2000 != 0 {
            self.task.timer = 0;
            self.task.shots = 0;
            self.task.mode = 5;
        }
    }

    /// State 5 (0x509E8): fly and attack.
    fn state_attack(
        &mut self,
        host: &mut impl EnemyHost,
        stack: &mut MatrixStack,
        rand: &mut Rand,
        counts: &mut GroupCounts,
    ) {
        self.leash_flag(host);
        self.notice(host, NoticeFrom::Home);
        let kind = self.task.kind;
        if kind == 1 || (kind == 3 && self.task.flags & 0x800 == 0) {
            self.turn(host, rand);
            self.fly(host, stack);
        }
        self.jets(host, stack);
        self.damage(host, rand, counts);
        if self.task.mode == 10 || self.task.mode == 8 {
            return;
        }
        self.timer_tick(host);
        if kind == 1 && self.work.burst > 0 {
            if self.task.flags & 0x2000 == 0 {
                self.task.timer = 0;
                self.task.shots = 0;
                self.task.mode = 3;
                return;
            }
            if !self.timer_past_interval() {
                return;
            }
            let cur = self.work.motion.current;
            if cur == 8 {
                self.work.motion.request = 10;
            } else if cur == 10 {
                let f = self.work.motion.frame;
                if f >= 1.0 && f < k(0x3f94_7ae1) {
                    self.shoot(host, stack);
                    self.task.shots = self.task.shots.wrapping_add(1);
                }
                if self.task.shots as i32 >= self.work.burst as i32 {
                    self.task.timer = 0;
                    self.task.shots = 0;
                }
            }
        } else if kind == 3 {
            if self.task.flags & 0x2000 == 0 {
                self.task.timer = 0;
                self.task.flags &= !0x800;
                self.task.mode = 3;
                return;
            }
            if self.task.flags & 0x800 != 0 {
                self.task.flags &= !0x800;
                return;
            }
            if !self.timer_past_interval() {
                return;
            }
            let cur = self.work.motion.current;
            if cur == 8 {
                self.work.motion.request = 9;
                self.beam(host, stack);
            } else if cur == 9 {
                self.work.motion.request = 10;
                self.task.timer = 0;
            }
        }
    }

    /// State 6 (0x506D0): wait (kinds 6 to 8) until a player is near the
    /// enemy itself, then drop.
    fn state_drop_wait(
        &mut self,
        host: &mut impl EnemyHost,
        rand: &mut Rand,
        counts: &mut GroupCounts,
    ) {
        let (g39, g42) = host.game_39_42();
        if engine::player_in_range(
            NoticeFrom::Body,
            &self.task,
            &self.work,
            &host.players(),
            g39,
            g42,
        ) != 0
        {
            self.task.flags |= 0x2000;
            self.task.mode = 7;
        } else {
            self.task.flags &= !0x2000;
        }
        self.timer_tick(host);
        self.damage(host, rand, counts);
    }

    /// State 7 (0x5077C): drop to the ground, then become kind 0, 2 or 4
    /// in state 2.
    fn state_drop(
        &mut self,
        host: &mut impl EnemyHost,
        stack: &mut MatrixStack,
        rand: &mut Rand,
        counts: &mut GroupCounts,
    ) {
        let (g39, g42) = host.game_39_42();
        if engine::player_in_range(
            NoticeFrom::Body,
            &self.task,
            &self.work,
            &host.players(),
            g39,
            g42,
        ) != 0
        {
            self.task.flags |= 0x2000;
        } else {
            self.task.flags &= !0x2000;
        }
        self.turn(host, rand);
        self.fall(host, stack);
        self.timer_tick(host);
        let cur = self.work.motion.current;
        if cur == 3 {
            self.task.mode = 2;
            match self.task.kind {
                6 => self.task.kind = 0,
                7 => self.task.kind = 2,
                8 => self.task.kind = 4,
                _ => {}
            }
            self.task.timer = (self.work.interval as i32 - 60) as u16;
        } else if cur == 8 && self.work.flags & 0x80 != 0 {
            self.work.motion.request = 0;
            Self::child(
                host,
                creators::DUST,
                vec![self.task.pos],
                vec![8],
                vec![8.0, 4.0],
            );
            host.event(Event::Sound {
                id: 0x400B,
                volume: 0x64,
                pos: self.task.pos,
            });
        }
        self.damage(host, rand, counts);
    }

    /// State 8 (0x50C84): hit while held; back to state 2 after 6 frames.
    fn state_held(&mut self, host: &mut impl EnemyHost, rand: &mut Rand, counts: &mut GroupCounts) {
        self.notice(host, NoticeFrom::HomeOffset);
        self.turn(host, rand);
        self.timer_tick(host);
        if self.task.timer > 6 {
            self.task.mode = 2;
            self.task.timer = 0;
        }
        self.damage(host, rand, counts);
    }

    /// State 9 (0x50D2C): kind 9 does nothing but take hits.
    fn state_idle(&mut self, host: &mut impl EnemyHost, rand: &mut Rand, counts: &mut GroupCounts) {
        self.damage(host, rand, counts);
        if self.task.mode != 10 && !host.game_37() {
            self.task.timer = self.task.timer.wrapping_add(1);
        }
    }

    /// 0x500F0: states 10 and 11 (DeadOut, and kind 9 marks its set entry).
    fn dead(&mut self, host: &mut impl EnemyHost) {
        host.event(Event::Free);
        if self.task.kind == 9 {
            host.event(Event::SetEntryDead);
        }
    }

    /// fn_800167E0: was the enemy hit this frame?
    fn damaged(&mut self, host: &mut impl EnemyHost) -> bool {
        if self.task.flags & 4 != 0 {
            if host.hit_by_player() {
                self.work.flags |= 0x1000;
            }
            self.work.f_1c0 = k(0x3eb3_3333);
            true
        } else {
            host.damage_hook(self.task.pos)
        }
    }

    /// 0x4F1C4: hits. Kind 5 and enemies that can be carried react; the
    /// rest are destroyed.
    fn damage(&mut self, host: &mut impl EnemyHost, rand: &mut Rand, counts: &mut GroupCounts) {
        if !self.damaged(host) {
            return;
        }
        if !host.game_37() {
            if self.work.flags & 0x400 != 0 {
                if (0..10).contains(&self.task.kind) {
                    host.event(Event::Carried {
                        player: self.work.b_44 as i32,
                        model: self.model,
                    });
                }
            } else if self.task.b2 == 8 {
                if self.work.link_40 && !host.collision_hit() {
                    self.hold();
                    host.event(Event::ClearCollision);
                    return;
                }
                if host.collision_hit() {
                    if let Some(p) = host.hit_by_player_index() {
                        if host.hit_player() {
                            host.event(Event::KnockBack {
                                player: p,
                                speed: [-2.5, 1.0, 0.0],
                                angles: [0, self.task.ang[1].wrapping_add(0xC000), 0],
                            });
                        }
                        self.hold();
                        host.event(Event::Sound {
                            id: 0x401F,
                            volume: 10,
                            pos: self.task.pos,
                        });
                        host.event(Event::PlayerHurt {
                            player: p,
                            a: 3,
                            b: 7,
                        });
                        host.event(Event::ClearCollision);
                        return;
                    }
                }
            }
        }
        // The kill.
        self.task.mode = 10;
        host.event(Event::CollisionOff { entry: 0 });
        self.task.timer = 0;
        engine::set_home(&self.task, &mut self.work);
        host.event(Event::Debris);
        let mut points = if self.task.kind == 5 { 200 } else { 100 };
        if host.score_doubled() {
            points <<= 1;
        }
        host.event(Event::Score { points });
        self.die(host, rand, counts);
    }

    /// Hit while kind 5 can be held: state 8 with motion 14.
    fn hold(&mut self) {
        self.work.motion.request = 14;
        self.task.timer = 0;
        self.task.shots = 0;
        self.task.mode = 8;
        self.task.flags &= !4;
        self.work.flags &= !0x1000;
    }

    /// fn_800164FC: the enemy leaves its switch group, its task is flagged
    /// 0x100, and its drop starts (work flag 4: a child whose timer and
    /// kind are drawn from rand).
    fn die(&mut self, host: &mut impl EnemyHost, rand: &mut Rand, counts: &mut GroupCounts) {
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
    }
}
