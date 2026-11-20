//! E NAMIE, stg09D.rel: row callback 0x52A1C and its routines
//! 0x51398..0x52980. Four kinds from trunc(Scale.x): kinds 0 to 2 cruise
//! around home and turn their child segments loose when they face the
//! player (state 2, until a segment sets task flag 0x8000); kind 3 fires
//! three beams every interval. It joins its switch group (set-up calls
//! fn_80018C88).
//!
//! The child segments (created by 0x53F0C, exec 0x53BB8) are REL child
//! tasks this module does not run; their creation is an `Event::Child`.
use super::{
    e_ai::{creators, translate},
    engine::{self, GroupCounts, BAMS_PER_RADIAN},
    host::{EnemyHost, Event},
    EnemyTask, EnemyWork, Rand, V3,
};
use crate::{
    msl_math::{asinf, atan2f},
    ninja_matrix::{mult_vec, mult_vec_sr, Axis, MatrixStack, IDENTITY},
    ninja_trig::{fctiwz, fmadds, nj_cos, nj_sin},
    ninja_vector::inner_product,
};

/// Section-5 offsets of E NAMIE's data in stg09D.rel.
pub mod rows {
    pub const COLLISION: u32 = 0xCA318;
    /// Models for kinds 0 to 3.
    pub const MODELS: [u32; 4] = [0xC5A5C, 0xC6DB0, 0xC849C, 0xCA2E0];
}

/// The creator of E NAMIE's child segments (section-1 offset).
pub const SEGMENTS: u32 = 0x53F0C;

fn k(bits: u32) -> f32 {
    f32::from_bits(bits)
}

/// The 0x1C-byte block at task +0x40 (set-up 0x51398).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NamieArea {
    /// +0.
    pub set: u8,
    /// +4, +8: X bounds; +0xC, +0x10: Y bounds; +0x14, +0x18: Z bounds.
    pub x: [f32; 2],
    pub y: [f32; 2],
    pub z: [f32; 2],
}

#[derive(Debug, Clone)]
pub struct ENamie {
    pub task: EnemyTask,
    pub work: EnemyWork,
    pub area: NamieArea,
    pub model: u32,
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

fn world_vector(stack: &mut MatrixStack, task: &EnemyTask, v: V3) -> V3 {
    stack.push();
    *stack.current_mut() = IDENTITY;
    stack.rotate(Axis::Y, task.ang[1]);
    let out = mult_vec_sr(stack.current(), v);
    stack.pop();
    out
}

fn child(host: &mut impl EnemyHost, creator: u32, args: Vec<V3>, ints: Vec<i32>, floats: Vec<f32>) {
    host.event(Event::Child {
        creator,
        args,
        ints,
        floats,
    });
}

impl ENamie {
    pub fn new(task: EnemyTask) -> Self {
        Self {
            task,
            work: EnemyWork::default(),
            area: NamieArea::default(),
            model: 0,
        }
    }

    /// 0x52A1C: one frame.
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
            0 => {
                self.setup(host, counts);
                if !host.set_entry_broken() {
                    let n = match self.task.kind {
                        0 | 3 => 3,
                        1 => 6,
                        _ => 9,
                    };
                    child(host, SEGMENTS, vec![], vec![n], vec![]);
                }
                return;
            }
            1 => self.state_cruise(host, stack, rand, counts),
            2 => self.state_release(host, stack, rand, counts),
            3 => {
                self.task.mode = 4;
                host.event(Event::Free);
                return;
            }
            4 => {
                host.event(Event::Free);
                return;
            }
            _ => {}
        }
        if !host.game_37() {
            engine::save_position(&self.task, &mut self.work);
            if self.task.flags & 0x2000 != 0 {
                let p = world_point(stack, &self.task, [k(0x402c_cccd), -1.5, 0.0]);
                child(host, creators::NEAR_EFFECT, vec![p], vec![2], vec![]);
            }
            if self.task.mode != 3 {
                host.event(Event::WorkSound {
                    id: 0x4011,
                    a: 1,
                    volume: 0x32,
                    b: 0x50,
                    pos: self.task.pos,
                });
            }
        }
        if self.task.mode != 3 {
            host.collision_entry();
        }
    }

    /// 0x51398: set-up.
    fn setup(&mut self, host: &mut impl EnemyHost, counts: &mut GroupCounts) {
        let t = &mut self.task;
        let mut w = engine::new_counted_work(t, counts);
        t.kind = fctiwz(engine::round_off(t.scl[0])) as i8;
        if !(0..4).contains(&t.kind) {
            t.kind = 0;
        }
        host.event(Event::CollisionInit {
            rows: rows::COLLISION,
            count: 2,
        });
        let kind = t.kind;
        self.model = rows::MODELS[kind as usize];
        if kind == 3 {
            w.burst = t.ang[0] as i8;
            w.interval = t.ang[2] as i16;
            w.f_94 = t.scl[1];
            child(host, creators::SETUP_TURRET, vec![], vec![], vec![]);
            w.f_98 = 60.0;
        } else {
            w.f_98 = 60.0 + t.scl[1];
        }
        if kind != 3 && t.ang[0] != 0 && t.ang[2] != 0 {
            let a = &mut self.area;
            a.set = 1;
            let lo = |v: i32| 2.0 * (v & 0xff) as f32;
            let hi = |v: i32| 2.0 * (v >> 8) as f32;
            let cx = t.pos[0] + hi(t.ang[0]);
            a.x = [lo(t.ang[0]) + cx, cx - lo(t.ang[0])];
            let cz = t.pos[2] + hi(t.ang[2]);
            a.z = [lo(t.ang[2]) + cz, cz - lo(t.ang[2])];
            a.y = [
                t.pos[1] + 200.0 * engine::fraction(t.scl[1]).abs(),
                t.pos[1] + 200.0 * -engine::fraction(t.scl[2]).abs(),
            ];
        }
        t.ang[0] = 0;
        t.ang[2] = 0;
        t.scl[1] = 2.0;
        let r = 60.0 + t.scl[2];
        w.f_1d8 = r * r;
        w.i_1d4 = 0x71c7;
        let r = 60.0 + t.scl[2];
        w.notice_range_sq = r * r;
        w.spd[0] = k(0x3ecc_cccd);
        w.turn_rate = 0x300;
        child(host, creators::SETUP_45EC8, vec![], vec![], vec![]);
        engine::set_home(t, &mut w);
        t.mode = 1;
        if engine::fraction(t.scl[0]) == 0.0 {
            t.shots = 1;
            w.f_ec = 12.0;
            w.f_f0 = k(0x3fe6_6666);
        }
        w.flags |= 0x10;
        w.flags |= 0x4;
        child(host, creators::SETUP_43E5C, vec![], vec![], vec![]);
        child(host, 0x47468, vec![], vec![], vec![]);
        self.work = w;
    }

    /// 0x51A84: past the range (+0x98) from home, flag 0x1000 for 1.5 to
    /// 3.5 seconds of turning back.
    fn leash_flag(&mut self, host: &impl EnemyHost, rand: &mut Rand) {
        if host.game_37() {
            return;
        }
        let (t, w) = (&self.task, &self.work);
        let dx = w.home[0] - t.pos[0];
        let dy = w.home[1] - t.pos[1];
        let dz = w.home[2] - t.pos[2];
        let d = dz * dz + (dx * dx + dy * dy);
        if d > w.f_98 * w.f_98 {
            self.task.flags |= 0x1000;
            let u = k(0x3800_0000) * rand.rand() as f32;
            self.work.h_4e = fctiwz(60.0 * (1.5 + 2.0 * u)) as i16;
        } else {
            let c = self.work.h_4e;
            self.work.h_4e = c.wrapping_sub(1);
            if c <= 0 {
                self.task.flags &= !0x1000;
                self.work.h_4e = 0;
            }
        }
    }

    /// 0x51EA8: flag 0x2000 when a player is near home (fn_800188B0).
    fn notice(&mut self, host: &impl EnemyHost) {
        let (g39, g42) = host.game_39_42();
        if engine::player_in_range(
            engine::NoticeFrom::Home,
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
    }

    /// 0x51968: turn home outside the leash, else toward a near player.
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

    /// 0x51EFC: cruise at home's height (when the range is at least 10).
    fn cruise(&mut self, host: &impl EnemyHost) {
        if host.game_37() {
            return;
        }
        let (t, w) = (&mut self.task, &mut self.work);
        if w.f_98.abs() < 10.0 {
            t.pos[1] = w.home[1];
        } else {
            w.spd[0] += w.accel;
            w.spd[2] += w.side_accel;
            if w.spd[0] > k(0x3ecc_cccd) {
                w.spd[0] = k(0x3ecc_cccd);
            } else if w.spd[0] < k(0x3dcc_cccd) {
                w.spd[0] = k(0x3dcc_cccd);
            }
            let s = w.spd[2] * nj_sin(t.ang[1]);
            t.pos[0] += w.spd[0] * nj_cos(t.ang[1]) + s;
            t.pos[1] = w.home[1];
            let c = w.spd[2] * nj_cos(t.ang[1]);
            t.pos[2] += -(w.spd[0] * nj_sin(t.ang[1])) + c;
        }
        engine::leash(t, w, w.f_98);
    }

    /// 0x51A08: bob 2 units around home's height.
    fn bob(&mut self, host: &impl EnemyHost) {
        if host.game_37() {
            return;
        }
        let (t, w) = (&mut self.task, &mut self.work);
        let s = nj_sin(w.bob);
        w.bob = w.bob.wrapping_add(0x200);
        t.pos[1] = w.home[1] + 2.0 * s;
    }

    /// The jet puffs while moving: one at the tail, three more for kinds 0
    /// and 3.
    fn jets(&mut self, host: &mut impl EnemyHost, stack: &mut MatrixStack, tail: f32, dir: f32) {
        let p = world_point(stack, &self.task, [tail, 0.0, 0.0]);
        let v = world_vector(stack, &self.task, [dir, 0.0, 0.0]);
        child(host, creators::JET, vec![p, v], vec![0, 0], vec![]);
        if self.task.kind == 0 || self.task.kind == 3 {
            for q in [
                [-15.0, 14.0, 0.0],
                [-15.0, -7.0, -13.0],
                [-15.0, -7.0, 13.0],
            ] {
                let p = world_point(stack, &self.task, q);
                child(host, creators::JET, vec![p, v], vec![0, 1], vec![]);
            }
        }
    }

    /// 0x5204C: like E AI's aim (njCalcPoint on the rotation).
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
        let out = mult_vec(stack.current(), dir);
        stack.pop();
        out
    }

    /// 0x521AC: kind 3's three beams (muzzles at stg09D.rel s5+0xCA5F4).
    fn beams(&mut self, host: &mut impl EnemyHost, stack: &mut MatrixStack) {
        const MUZZLES: [[u32; 3]; 3] = [
            [0x4000_0000, 0xc15e_6666, 0],
            [0x4000_0000, 0x40e0_0000, 0x4141_999a],
            [0x4000_0000, 0x40e0_0000, 0xc141_999a],
        ];
        for m in MUZZLES {
            let muzzle = world_point(stack, &self.task, m.map(k));
            let dir = self.aim(host, stack, muzzle, [self.work.f_94, 0.0, 0.0]);
            child(
                host,
                creators::BEAM,
                vec![dir, muzzle],
                vec![],
                vec![self.work.burst as i32 as f32],
            );
        }
    }

    /// State 1 (0x522F4).
    fn state_cruise(
        &mut self,
        host: &mut impl EnemyHost,
        stack: &mut MatrixStack,
        rand: &mut Rand,
        counts: &mut GroupCounts,
    ) {
        self.leash_flag(host, rand);
        self.notice(host);
        self.turn(host);
        if self.task.flags & 0x2000 == 0 || host.set_entry_broken() {
            self.cruise(host);
        }
        self.bob(host);
        if self.work.f_98.abs() >= 10.0 && !host.game_37() {
            let dir = if self.task.flags & 0x3000 == 0 {
                k(0xbf33_3333)
            } else {
                k(0xbe99_999a)
            };
            self.jets(host, stack, -30.0, dir);
        }
        self.damage(host, rand, counts);
        if self.task.mode == 3 {
            return;
        }
        if !host.game_37() {
            self.task.timer = self.task.timer.wrapping_add(1);
        }
        if self.task.kind == 3 {
            if self.task.flags & 0x2000 == 0 {
                self.task.timer = 0;
            } else if self.task.flags & 0x800 != 0 {
                self.task.flags &= !0x800;
            } else if self.task.timer as i32 > self.work.interval as i32 {
                self.task.mode = 2;
                self.task.timer = 0;
                self.beams(host, stack);
            }
        } else if !host.set_entry_broken() && self.task.flags & 0x2000 != 0 {
            let p = host.players();
            let i = engine::nearest_player(&p, self.task.pos);
            let pl = p
                .get(i)
                .copied()
                .flatten()
                .map(|p| p.pos)
                .unwrap_or([0.0; 3]);
            let mut d = [
                pl[0] - self.task.pos[0],
                pl[1] - self.task.pos[1],
                pl[2] - self.task.pos[2],
            ];
            d[1] = 0.0;
            let d = engine::unit_vector(d);
            let f = world_vector(stack, &self.task, [1.0, 0.0, 0.0]);
            if inner_product(d, f) > k(0x3f67_ef9e) {
                self.task.timer = 0;
                self.task.mode = 2;
                host.event(Event::SetEntryBroken);
            }
        }
    }

    /// State 2 (0x52708).
    fn state_release(
        &mut self,
        host: &mut impl EnemyHost,
        stack: &mut MatrixStack,
        rand: &mut Rand,
        counts: &mut GroupCounts,
    ) {
        self.damage(host, rand, counts);
        if self.task.mode == 3 || host.game_37() {
            return;
        }
        self.task.timer = self.task.timer.wrapping_add(1);
        if self.work.f_98.abs() >= 10.0 {
            self.jets(host, stack, -27.0, k(0xbe99_999a));
        }
        if self.task.kind == 3 {
            if self.task.flags & 0x800 != 0 {
                self.task.flags &= !0x800;
            } else if self.task.timer as i32 > self.work.interval as i32 {
                self.task.mode = 1;
                self.task.timer = 0;
            }
        } else if self.task.flags as u16 & 0x8000 != 0 {
            self.task.flags &= 0x7fff;
            self.task.mode = 1;
            self.task.timer = 0;
        }
    }

    /// 0x51BA8: hits (or task flag 0x4000) destroy it, unless the hit
    /// came from a held object (partner byte 2 is 6) while nothing holds
    /// this one.
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
        if !hit && self.task.flags & 0x4000 == 0 {
            return;
        }
        if host.hit_by_held_object() && self.work.b_44 == -1 {
            self.task.flags &= !4;
            self.work.flags &= !0x1000;
            return;
        }
        self.task.mode = 3;
        host.event(Event::CollisionOff { entry: 0 });
        self.task.timer = 0;
        engine::set_home(&self.task, &mut self.work);
        host.event(Event::Debris);
        let mut points = match self.task.kind {
            1 => 200,
            2 => 300,
            _ => 100,
        };
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
        self.task.flags |= 0x4000;
        if self.work.flags & 0x400 != 0 {
            host.event(Event::Carried {
                player: self.work.b_44 as i32,
                model: self.model,
            });
        }
    }
}
