//! E GOLD (the gold beetle), stg09D.rel: row callback 0x4A024 and its
//! routines 0x494AC..0x49DB4. It bobs in place facing the player, spins up
//! and flies off when its time runs out, and is worth 1000 points. It does
//! not join a switch group (its set-up calls fn_80018D28, not fn_80018C88).
use super::{
    engine::{self, GroupCounts},
    host::{EnemyHost, Event},
    motion::MotionRow,
    EnemyTask, EnemyWork, Rand,
};
use crate::{
    ninja_matrix::{mult_vec, Axis, MatrixStack, IDENTITY},
    ninja_trig::{fctiwz, fmadds, nj_sin},
};

/// Section-5 offsets of E GOLD's data in stg09D.rel.
pub mod rows {
    pub const COLLISION: u32 = 0xA4F84;
    pub const MOTION: u32 = 0xA4FCC;
    pub const MOTION_ROWS: usize = 1;
    pub const MODEL: u32 = 0xA4C00;
    pub const DEBRIS: u32 = 0xA4FB4;
}

fn k(bits: u32) -> f32 {
    f32::from_bits(bits)
}

#[derive(Debug, Clone)]
pub struct EGold {
    pub task: EnemyTask,
    pub work: EnemyWork,
    pub table: Vec<MotionRow>,
}

impl EGold {
    pub fn new(task: EnemyTask, table: Vec<MotionRow>) -> Self {
        Self {
            task,
            work: EnemyWork::default(),
            table,
        }
    }

    /// 0x4A024: one frame.
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
            0 => return self.setup(host),
            1 => self.state_bob(host, rand, counts),
            2 => self.state_wait(host),
            3 => self.state_spin_up(host),
            4 => {
                self.state_leave(host);
                if self.task.mode != 4 {
                    host.event(Event::Hidden);
                }
            }
            5 => {
                self.task.mode = 6;
                host.event(Event::Free);
                return;
            }
            6 => {
                host.event(Event::Free);
                return;
            }
            _ => {}
        }
        if !host.game_37() {
            engine::save_position(&self.task, &mut self.work);
            let volume = match self.task.mode {
                1 => Some(10),
                3 | 4 => Some(fctiwz(10.0 + 127.0 * self.work.f_ac)),
                _ => None,
            };
            if let Some(volume) = volume {
                host.event(Event::WorkSound {
                    id: 0x400D,
                    a: 1,
                    volume,
                    b: 5,
                    pos: self.task.pos,
                });
            }
            self.work.motion.step(&self.table);
            if self.task.flags & 0x1000 != 0 {
                stack.push();
                *stack.current_mut() = IDENTITY;
                super::e_ai::translate(stack.current_mut(), self.task.pos);
                stack.rotate(Axis::Y, self.task.ang[1]);
                let p = mult_vec(stack.current(), [10.0, k(0x4093_3333), 0.0]);
                host.event(Event::Child {
                    creator: super::e_ai::creators::NEAR_EFFECT,
                    args: vec![p],
                    ints: vec![1],
                    floats: vec![],
                });
                stack.pop();
            }
        }
        if self.task.mode != 5 {
            host.collision_entry();
        }
    }

    /// 0x494AC: set-up.
    fn setup(&mut self, host: &mut impl EnemyHost) {
        let game_7c = host.game_7c();
        let t = &mut self.task;
        let mut w = engine::new_work(t);
        t.kind = 0;
        host.event(Event::CollisionInit {
            rows: rows::COLLISION,
            count: 1,
        });
        w.interval = ((t.ang[0] & 0xff) * 10) as i16;
        t.ang[2] &= 0xff00;
        w.f_98 = 0.0;
        t.ang[0] &= 0xff00;
        w.motion.step(&self.table);
        host.event(Event::Child {
            creator: super::e_ai::creators::SETUP_45EC8,
            args: vec![],
            ints: vec![],
            floats: vec![],
        });
        let r = 60.0 + t.scl[2];
        w.f_1d8 = r * r;
        w.i_1d4 = 0x4000;
        let r = 60.0 + t.scl[2];
        w.notice_range_sq = r * r;
        w.spd[0] = k(0x3ecc_cccd);
        w.turn_rate = 0x180;
        w.bob = t.ang[0].wrapping_add(game_7c.wrapping_mul(t.ang[2]));
        engine::set_home(t, &mut w);
        t.mode = 2;
        w.motion.request = 0;
        host.event(Event::CollisionOff { entry: 0 });
        w.motion.step(&self.table);
        w.f_ec = 12.0;
        w.f_f0 = k(0x3f99_999a);
        w.flags |= 0x10;
        w.flags |= 0x4;
        self.work = w;
    }

    /// fn_80018A4C sets or clears the task's flag 0x1000.
    fn view(&mut self, host: &impl EnemyHost) {
        let (g39, g42) = host.game_39_42();
        if engine::player_in_view(&self.task, &self.work, &host.players(), g39, g42) != 0 {
            self.task.flags |= 0x1000;
        } else {
            self.task.flags &= !0x1000;
        }
    }

    /// 0x496B4: face the nearest player while flag 0x1000 is set.
    fn face(&mut self, host: &impl EnemyHost) {
        if host.game_37() || self.task.flags & 0x1000 == 0 {
            return;
        }
        let p = host.players();
        let i = engine::nearest_player(&p, self.task.pos);
        if let Some(pl) = p.get(i).copied().flatten() {
            engine::face_player(&mut self.task, &mut self.work, pl.pos);
        }
    }

    /// 0x498AC: bob around home by Scale.y.
    fn bob(&mut self) {
        let (t, w) = (&mut self.task, &mut self.work);
        let s = nj_sin(t.ang[0].wrapping_add(w.i_1cc.wrapping_mul(t.ang[2])));
        w.i_1cc = w.i_1cc.wrapping_add(1);
        t.pos[1] = w.home[1] + s * t.scl[1];
        w.bob = w.bob.wrapping_add(t.ang[2]);
    }

    /// State 1 (0x49968).
    fn state_bob(&mut self, host: &mut impl EnemyHost, rand: &mut Rand, counts: &mut GroupCounts) {
        self.view(host);
        self.face(host);
        self.bob();
        self.damage(host, rand, counts);
        if self.task.mode == 5 {
            return;
        }
        if !host.game_37() {
            self.task.timer = self.task.timer.wrapping_add(1);
        }
        if self.task.timer as i32 > self.work.interval as i32 {
            self.task.mode = 4;
            self.task.timer = 0;
            self.task.flags &= !0x1000;
        }
    }

    /// State 2 (0x49A3C): wait until a player is in view.
    fn state_wait(&mut self, host: &mut impl EnemyHost) {
        self.view(host);
        if self.task.flags & 0x1000 != 0 {
            self.task.mode = 3;
            self.work.f_ac = -1.0;
            self.work.scale = [1.0; 3];
            self.task.flags &= !0x1000;
            self.work.f_ec = 0.0;
            host.event(Event::Sound {
                id: 0x400F,
                volume: 0x7F,
                pos: self.task.pos,
            });
        }
        self.face(host);
    }

    fn spin(&mut self, sign: f32) {
        let a = self.task.ang[1] as f32;
        let d = 8192.0 * -self.work.f_ac;
        self.task.ang[1] = fctiwz(if sign > 0.0 { a + d } else { a - d });
    }

    /// State 3 (0x49B04): spin up, then appear (state 1).
    fn state_spin_up(&mut self, host: &mut impl EnemyHost) {
        self.spin(1.0);
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

    /// State 4 (0x49C4C): spin up and leave (state 5).
    fn state_leave(&mut self, host: &mut impl EnemyHost) {
        self.spin(-1.0);
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
            self.task.mode = 5;
            host.event(Event::CollisionOff { entry: 0 });
        }
    }

    /// 0x4973C: hits destroy it for 1000 points.
    fn damage(&mut self, host: &mut impl EnemyHost, rand: &mut Rand, counts: &mut GroupCounts) {
        let hit = if self.task.flags & 4 != 0 {
            if host.hit_by_player() {
                self.work.flags |= 0x1000;
            }
            self.work.f_1c0 = k(0x3eb3_3333);
            true
        } else {
            host.damage_hook(self.task.pos)
        };
        if !hit {
            return;
        }
        self.task.mode = 5;
        host.event(Event::CollisionOff { entry: 0 });
        self.task.timer = 0;
        engine::set_home(&self.task, &mut self.work);
        host.event(Event::Debris);
        let mut points = 1000;
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
                model: rows::MODEL,
            });
        }
    }
}
