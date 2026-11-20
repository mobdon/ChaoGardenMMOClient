//! The rest of the walker exec's adapter: the collision core, the
//! pre-switch step, standing, motion, the land probes, the collision-info
//! refresh and the frame end, plus [`walker_frame`], the one call the
//! runtime makes per walker per frame.
use crate::{
    land_nodes::Entry,
    native_work::{pw, tw, Work},
    ninja_matrix::{MatrixStack, Vec3},
    player_analog::{get_analog, AnalogPlayer},
    player_motion::{motion_settle, motion_step, BlendEvent, MotionTables, MotionWork as Mtn},
    player_pre::{pre_switch, PreCcl, PreGame, PreSteps, PreWork},
    player_shadow::{shadow_update, ProbeLand, ShadowBlock},
    player_stand::{stand_check, StandGame, StandWork, VoiceTables},
    walker_act::{ActSteps, DamageWorld, LockWorld},
    walker_adapter::{call_move, MoveEnv, PtrTurns},
    walker_collide::MotionWork,
    walker_collide::PhysicsRow,
    walker_exec::{
        ccl_update, end_frame, walker_exec, CclUpdate, EndFrame, Engine, EngineCall, ExecGlobals,
        ExecSteps, PlayerHistory,
    },
    walker_ground::{
        collision_core, CclView, CollisionQuery, FloorExtras, GroundEnv, LandContext, MoveVector,
        PlayerTask, PlayerWork, RideRecord,
    },
};

fn shadow(p: &Work, o: usize) -> ShadowBlock {
    ShadowBlock {
        angle_x: p.i(o),
        angle_z: p.i(o + 4),
        ceiling_flags: p.w(o + 8),
        floor_flags: p.w(o + 0xC),
        ceiling_y: p.f(o + 0x10),
        floor_y: p.f(o + 0x14),
        last_ceiling_flags: p.w(o + 0x18),
        last_floor_flags: p.w(o + 0x1C),
    }
}

fn put_shadow(p: &mut Work, o: usize, b: &ShadowBlock) {
    p.set_i(o, b.angle_x);
    p.set_i(o + 4, b.angle_z);
    p.set_w(o + 8, b.ceiling_flags);
    p.set_w(o + 0xC, b.floor_flags);
    p.set_f(o + 0x10, b.ceiling_y);
    p.set_f(o + 0x14, b.floor_y);
    p.set_w(o + 0x18, b.last_ceiling_flags);
    p.set_w(o + 0x1C, b.last_floor_flags);
}

fn motion_work(p: &Work, scale_a: f32, scale_b: f32, has_blend: bool) -> Mtn {
    let b = 0x174;
    Mtn {
        mode: p.h(b),
        request: p.h(b + 2),
        action: p.h(b + 4),
        last_action: p.h(b + 6),
        next: p.h(b + 8),
        timer: p.h(b + 0xA),
        flags: p.h(b + 0xC),
        frame: p.f(b + 0x10),
        last_frame: p.f(b + 0x14),
        start_frame: p.f(b + 0x18),
        scale_a,
        scale_b,
        has_blend,
    }
}

fn put_motion(p: &mut Work, w: &Mtn) {
    let b = 0x174;
    p.set_h(b, w.mode);
    p.set_h(b + 2, w.request);
    p.set_h(b + 4, w.action);
    p.set_h(b + 6, w.last_action);
    p.set_h(b + 8, w.next);
    p.set_h(b + 0xA, w.timer);
    p.set_h(b + 0xC, w.flags);
    p.set_f(b + 0x10, w.frame);
    p.set_f(b + 0x14, w.last_frame);
    p.set_f(b + 0x18, w.start_frame);
}

/// The player's globals the frame reads: `playertp/twp/mwp/pwp` are the
/// runtime's (this player's are the work areas passed in); the gravity
/// vector (0x801E5624) and everything the routines read about the stick,
/// camera, game state and land.
pub struct PlayerGlobals<'a> {
    pub mv: MoveEnv,
    pub exec: ExecGlobals,
    pub hist: &'a mut PlayerHistory,
    pub pre: PreGame,
    pub stand: StandGame,
    pub voices: VoiceTables<'a>,
    pub motion: MotionTables<'a>,
    /// The values the motion work's +0x1C and +0x20 point at.
    pub motion_scales: (f32, f32),
    /// The motion work's blend buffer (+0x28) is set.
    pub motion_blend: bool,
    /// The collision query at player +0x90.
    pub query: CollisionQuery,
    /// The CCL view of this frame's hits (fn_800060EC and friends).
    pub ccl: CclView,
    /// The collision info bytes and range (task +0x2C, +0xC and +8).
    pub ccl_info: Option<(&'a mut Vec<u8>, &'a mut f32)>,
    /// The physics rows fn_800424C8 and the settles read.
    pub rows: &'a [PhysicsRow],
    pub stuck: bool,
    pub ride_record: &'a dyn Fn(usize, i8) -> Option<RideRecord>,
    pub task_has_collision: &'a dyn Fn(usize) -> bool,
    /// `playertwp[pnum]` position and angles, for the history ring.
    pub r13_7900: u32,
    pub game_mode_42: u8,
    /// The engine-side hooks: the CCL queries, lock-on targets, damage
    /// world and the action-step object handlers.
    pub pre_ccl: &'a mut dyn PreCcl,
    pub lock: &'a mut dyn LockWorld,
    pub damage: &'a mut dyn DamageWorld,
    pub objects: &'a mut dyn ObjectHooks,
    /// The probes' land view and fn_80023808.
    pub probe_land: ProbeLand<'a>,
    pub reselect: &'a mut dyn FnMut(Vec3, f32) -> Vec<Entry>,
    /// What fn_8003D8D0's blend asks the renderer.
    pub blend_events: Vec<BlendEvent>,
}

/// The routines the walker's action step reaches that are not ported here:
/// holding objects (fn_8004F11C, fn_800542A0), the object button handlers,
/// the path direction fn_80038A70 and the debug fly fn_800AE054.
pub trait ObjectHooks {
    fn hold_flags(&mut self, t: &Work, p: &Work) -> u32;
    fn release_hold(&mut self, t: &mut Work, m: &mut Work, p: &mut Work);
    fn button_action(&mut self, addr: u32, t: &mut Work, m: &mut Work, p: &mut Work);
    fn ride_acc(&mut self, p: &Work) -> Option<Vec3>;
    fn set_velocity(&mut self, pnum: i8, v: Vec3);
    fn set_ccl_centers(&mut self, a: Vec3, b: Vec3);
    fn step_motion(&mut self, p: &Work) -> (u32, f32, f32);
    /// Any other address with no port: the runtime decides (fail closed).
    fn unported(&mut self, addr: u32, t: &mut Work, m: &mut Work, p: &mut Work) -> i32;
}

/// The caller's f26 fn_800414F0 reads in the ground slide: the walker exec
/// never sets it, so it is the value left by the task loop.
const F26_LEFT: f32 = 0.0;

struct Frame<'g, 'a> {
    g: &'g mut PlayerGlobals<'a>,
    land: &'g mut LandContext<'a>,
    stack: MatrixStack,
    eng: &'g mut dyn Engine,
}

impl Frame<'_, '_> {
    fn collide(&mut self, t: &mut Work, m: &mut Work, p: &mut Work) -> i32 {
        let mut task = PlayerTask {
            mode: t.ub(tw::MODE),
            flag: t.h(tw::FLAG),
            angles: t.ang3(tw::ANG),
            pos: t.v3(tw::POS),
        };
        let mut vel = MoveVector {
            v: m.v3(0),
            angles: m.ang3(0x18),
        };
        let mut w = PlayerWork {
            pnum: p.b(0),
            character: p.b(1),
            accel_row: p.b(3),
            h12: p.h(0x12),
            h14: p.h(0x14),
            status: p.h(0x1C),
            dotp: p.f(0x34),
            f40: p.f(0x40),
            v4c: p.v3(0x4C),
            spd: p.v3(pw::SPD),
            wall: p.v3(0x70),
            floor_normal: p.v3(0x7C),
            attr: p.w(0x88),
            last_attr: p.w(0x8C),
            ride: (p.w(0xA0) != 0).then_some(p.w(0xA0) as usize),
            landed_on: (p.w(0xB0) != 0).then_some(p.w(0xB0) as usize),
            w_c0: p.i(0xC0),
            height: p.f(0xC4),
            jog: p.f(0xE4),
            run: p.f(0xE8),
            f_ec: p.f(0xEC),
            f_f0: p.f(0xF0),
            f_f4: p.f(0xF4),
            run_accel: p.f(0xFC),
            f118: p.f(0x118),
            f134: p.f(0x134),
            weight: p.f(0x138),
            flags_1a8: p.w(0x1A8),
            height_1b0: p.f(0x1B0),
        };
        let mut env = GroundEnv {
            global: self.g.mv.gravity,
            stage: self.g.mv.stage,
            rows: self.g.rows,
            stuck: self.g.stuck,
            mw: MotionWork {
                spd: m.v3(0),
                ang_aim: m.i(0x1C),
            },
            ride_record: self.g.ride_record,
            task_has_collision: self.g.task_has_collision,
        };
        let analog_ctx = self.g.mv.analog;
        let mut analog = |pt: &PlayerTask, pw_: &PlayerWork| {
            let mut s = MatrixStack::new();
            let a = get_analog(
                &mut s,
                AnalogPlayer {
                    status: pt.flag,
                    angles: pt.angles,
                    nocon: pw_.h14,
                    confuse: 0,
                },
                analog_ctx,
                false,
                true,
            );
            (a.active, a.magnitude.unwrap_or_default())
        };
        let mut x = FloorExtras {
            probe: false,
            analog: &mut analog,
        };
        let ccl = self.g.ccl.clone();
        let r = collision_core(
            &mut self.stack,
            self.land,
            &mut task,
            &mut vel,
            &mut w,
            &mut self.g.query,
            &mut env,
            &ccl,
            &mut x,
            &mut |_| {},
            F26_LEFT,
        );
        t.set_h(tw::FLAG, task.flag);
        t.set_ang3(tw::ANG, task.angles);
        t.set_v3(tw::POS, task.pos);
        m.set_v3(0, vel.v);
        m.set_ang3(0x18, vel.angles);
        p.set_f(0x34, w.dotp);
        p.set_v3(pw::SPD, w.spd);
        p.set_v3(0x70, w.wall);
        p.set_v3(0x7C, w.floor_normal);
        p.set_w(0x88, w.attr);
        p.set_w(0x8C, w.last_attr);
        p.set_w(0xB0, w.landed_on.unwrap_or(0) as u32);
        p.set_h(0x12, w.h12);
        p.set_h(0x14, w.h14);
        self.g.mv.floor_up = self.g.query.up;
        self.g.mv.across = self.g.query.across;
        r
    }

    fn stand(&mut self, t: &mut Work, m: &mut Work, p: &mut Work) {
        let mut w = StandWork {
            pnum: p.b(0),
            character: p.b(1),
            voice_char: p.b(3),
            idle: p.h(0x18),
            f44: p.f(0x44),
            normal: p.v3(0x7C),
            attr: p.w(0x88),
            action: p.h(pw::ACTION),
            h17c: p.h(0x17C),
        };
        let eng = &mut *self.eng;
        let mut rand = || eng.rand();
        let mut hook = || false;
        let calls = stand_check(
            &mut self.stack,
            &mut w,
            t.ang3(tw::ANG),
            m.i(0x1C),
            self.g.mv.gravity,
            &self.g.stand,
            &self.g.voices,
            &mut rand,
            &mut hook,
        );
        for c in calls {
            self.eng.call(c);
        }
        p.set_h(0x18, w.idle);
        p.set_f(0x44, w.f44);
        p.set_h(pw::ACTION, w.action);
        p.set_h(0x17C, w.h17c);
    }

    fn motion(&mut self, p: &mut Work, settle: bool) {
        let mut w = motion_work(
            p,
            self.g.motion_scales.0,
            self.g.motion_scales.1,
            self.g.motion_blend,
        );
        let ev = if settle {
            motion_settle(&mut w, &self.g.motion)
        } else {
            motion_step(&mut w, &self.g.motion)
        };
        self.g.motion_blend = w.has_blend;
        self.g.blend_events.extend(ev);
        put_motion(p, &w);
    }

    fn pre(&mut self, t: &mut Work, m: &mut Work, p: &mut Work) {
        let mut w = PreWork {
            mode: t.b(tw::MODE),
            task_b1: t.ub(tw::SMODE),
            flag: t.h(tw::FLAG),
            task_h6: t.h(tw::WTIMER),
            angles: t.ang3(tw::ANG),
            pos: t.v3(tw::POS),
            info_attr: self
                .g
                .ccl_info
                .as_ref()
                .map_or(0, |(b, _)| u32::from_be_bytes([b[4], b[5], b[6], b[7]])),
            mw_spd: m.v3(0),
            mw_ang: m.ang3(0x18),
            pnum: p.b(0),
            character: p.b(1),
            p_bc: p.ub(0xC),
            p_w28: p.i(0x28),
            attr: p.w(0x88),
            p_w9c: p.w(0x9C),
            p_wa0: p.w(0xA0),
            ride: (p.w(0xA4) != 0).then_some(p.w(0xA4) as usize),
            p_h10: p.h(pw::STATUS),
            p_h18: p.h(0x18),
            action: p.h(pw::ACTION),
            spd: p.v3(pw::SPD),
            f138: p.f(0x138),
            here: shadow(p, 0x1A0),
            pwp_h180_4: false,
        };
        struct Steps<'s, 'g, 'a> {
            f: &'s mut Frame<'g, 'a>,
            t: Work,
            m: Work,
            p: Work,
        }
        impl PreSteps for Steps<'_, '_, '_> {
            fn settle(&mut self, _: &mut PreWork) {
                self.f.motion(&mut self.p, true);
            }
            fn stand(&mut self, w: &mut PreWork) {
                self.t.set_b(tw::MODE, w.mode);
                self.p.set_h(0x18, w.p_h18);
                let (mut t, mut m, mut p) = (self.t.clone(), self.m.clone(), self.p.clone());
                self.f.stand(&mut t, &mut m, &mut p);
                w.action = p.h(pw::ACTION);
                self.p = p;
            }
            fn motion(&mut self, _: &mut PreWork) {
                self.f.motion(&mut self.p, false);
            }
        }
        let g_pre = self.g.pre;
        let ccl: *mut dyn PreCcl = self.g.pre_ccl as *mut dyn PreCcl;
        let mut stack = MatrixStack::new();
        let mut st = Steps {
            f: self,
            t: t.clone(),
            m: m.clone(),
            p: p.clone(),
        };
        // SAFETY: `pre_ccl` is a separate object from the frame's other
        // fields that `Steps` touches.
        pre_switch(&mut stack, &mut w, &g_pre, unsafe { &mut *ccl }, &mut st);
        *p = st.p;
        t.set_b(tw::MODE, w.mode);
        t.set_h(tw::FLAG, w.flag);
        t.set_h(tw::WTIMER, w.task_h6);
        t.set_ang3(tw::ANG, w.angles);
        t.set_v3(tw::POS, w.pos);
        m.set_v3(0, w.mw_spd);
        m.set_ang3(0x18, w.mw_ang);
        p.set_b(0xC, w.p_bc as i8);
        p.set_w(0x88, w.attr);
        p.set_w(0x9C, w.p_w9c);
        p.set_w(0xA0, w.p_wa0);
        p.set_w(0xA4, w.ride.unwrap_or(0) as u32);
        p.set_h(pw::STATUS, w.p_h10);
        p.set_h(0x18, w.p_h18);
        p.set_h(pw::ACTION, w.action);
        p.set_v3(pw::SPD, w.spd);
        put_shadow(p, 0x1A0, &w.here);
    }

    fn probes(&mut self, t: &Work, p: &mut Work) {
        let mut here = shadow(p, 0x1A0);
        let mut ahead = shadow(p, 0x1C4);
        let stick = {
            let mut s = MatrixStack::new();
            let a = get_analog(
                &mut s,
                AnalogPlayer {
                    status: t.h(tw::FLAG),
                    angles: t.ang3(tw::ANG),
                    nocon: p.h(0x14),
                    confuse: p.h(0x1A),
                },
                self.g.mv.analog,
                true,
                false,
            );
            if a.active {
                a.angle
            } else {
                None
            }
        };
        if let Err(u) = shadow_update(
            &mut self.stack,
            self.g.reselect,
            &self.g.probe_land,
            t.ang3(tw::ANG),
            t.v3(tw::POS),
            p.f(0x130),
            p.f(0x134),
            stick,
            &mut here,
            &mut ahead,
        ) {
            panic!("unported routine {:#x}", u.0);
        }
        put_shadow(p, 0x1A0, &here);
        put_shadow(p, 0x1C4, &ahead);
    }

    fn ccl(&mut self, t: &Work, p: &Work) {
        let u = CclUpdate {
            mode: t.ub(tw::MODE),
            task_b2: t.ub(2),
            pos: t.v3(tw::POS),
            p_h10: p.h(pw::STATUS),
            p_f21c: p.f(0x21C),
            p_1e8: [p.w(0x1E8), p.w(0x1EC), p.w(0x1F0)],
        };
        let info = self
            .g
            .ccl_info
            .as_mut()
            .map(|(b, r)| (&mut b[..], &mut **r));
        ccl_update(info, &u);
    }

    fn end(&mut self, t: &mut Work, m: &mut Work, p: &mut Work) {
        let mut w = EndFrame {
            task_flag: t.h(tw::FLAG),
            task_b1: t.ub(tw::SMODE),
            angles: t.ang3(tw::ANG),
            pos: t.v3(tw::POS),
            mw_acc: m.v3(0xC),
            pnum: p.ub(0),
            p_b1: p.ub(1),
            p_b3: p.ub(3),
            p_bd: p.ub(0xD),
            p_h10: p.h(0x10),
            p_h14: p.h(0x14),
            air_timer: p.h(0x16),
            p_h1a: p.h(0x1A),
            p_h1e: p.h(0x1E),
            p_w24: p.w(0x24),
            p_wa8: p.w(0xA8),
            p_wac: p.w(0xAC),
            p_w1b8: p.w(0x1B8),
            p_f38: p.f(0x38),
            p_f4c: [0x4C, 0x50, 0x54, 0x58, 0x5C, 0x60].map(|o| p.f(o)),
            head: p.f(0x13C),
            water: p.f(0x1B0),
            ccl_b2: self.g.ccl_info.as_ref().map_or(0, |(b, _)| b[2]),
        };
        let calls = end_frame(
            &mut self.stack,
            &mut w,
            self.g.hist,
            t.v3(tw::POS),
            t.ang3(tw::ANG),
            self.g.game_mode_42,
            &mut self.g.r13_7900,
        );
        for c in calls {
            self.eng.call(c);
        }
        t.set_h(tw::FLAG, w.task_flag);
        t.set_b(tw::SMODE, w.task_b1 as i8);
        m.set_v3(0xC, w.mw_acc);
        for (k, o) in [0x4C, 0x50, 0x54, 0x58, 0x5C, 0x60].iter().enumerate() {
            p.set_f(*o, w.p_f4c[k]);
        }
        p.set_f(0x38, w.p_f38);
        p.set_h(0x14, w.p_h14);
        p.set_h(0x16, w.air_timer);
        p.set_h(0x1A, w.p_h1a);
        p.set_h(0x1E, w.p_h1e);
        p.set_w(0xA8, w.p_wa8);
        p.set_w(0xAC, w.p_wac);
        p.set_b(0xD, w.p_bd as i8);
        if let Some((b, _)) = self.g.ccl_info.as_mut() {
            b[2] = w.ccl_b2;
        }
    }
}

impl ExecSteps for Frame<'_, '_> {
    fn call(&mut self, addr: u32, t: &mut Work, m: &mut Work, p: &mut Work) -> i32 {
        if let Some(r) = call_move(addr, &mut self.stack, t, m, p, &mut self.g.mv) {
            return r;
        }
        match addr {
            0x8004_5880 => self.collide(t, m, p),
            0x8005_6450 => {
                self.pre(t, m, p);
                0
            }
            0x8005_4D04 => {
                self.stand(t, m, p);
                0
            }
            0x8003_D8D0 => {
                self.motion(p, false);
                0
            }
            0x8006_7600 => {
                self.probes(t, p);
                0
            }
            0x800A_C698 => {
                self.ccl(t, p);
                0
            }
            0x8004_F4D0 => {
                self.end(t, m, p);
                0
            }
            0x800A_9B84 => {
                self.eng.call(EngineCall::Deferred {
                    addr,
                    arg: p.b(0) as i32,
                });
                0
            }
            _ => self.g.objects.unported(addr, t, m, p),
        }
    }
    fn set_velocity(&mut self, pnum: i8, v: Vec3) {
        self.g.objects.set_velocity(pnum, v)
    }
    fn analog(&mut self, t: &Work, p: &Work) -> Option<(i32, f32)> {
        let mut s = MatrixStack::new();
        let a = get_analog(
            &mut s,
            AnalogPlayer {
                status: t.h(tw::FLAG),
                angles: t.ang3(tw::ANG),
                nocon: p.h(0x14),
                confuse: p.h(0x1A),
            },
            self.g.mv.analog,
            true,
            true,
        );
        a.active
            .then(|| (a.angle.unwrap_or_default(), a.magnitude.unwrap_or_default()))
    }
    fn step_motion(&mut self, p: &Work) -> (u32, f32, f32) {
        self.g.objects.step_motion(p)
    }
    fn set_ccl_centers(&mut self, a: Vec3, b: Vec3) {
        self.g.objects.set_ccl_centers(a, b)
    }
}

/// The action step's hooks over the same frame.
struct Act<'s, 'g, 'a>(&'s mut Frame<'g, 'a>);

impl ActSteps for Act<'_, '_, '_> {
    fn stand(&mut self, t: &mut Work, m: &mut Work, p: &mut Work) {
        self.0.stand(t, m, p)
    }
    fn analog(&mut self, t: &Work, p: &Work) -> bool {
        ExecSteps::analog(self.0, t, p).is_some()
    }
    fn hold_flags(&mut self, t: &Work, p: &Work) -> u32 {
        self.0.g.objects.hold_flags(t, p)
    }
    fn release_hold(&mut self, t: &mut Work, m: &mut Work, p: &mut Work) {
        self.0.g.objects.release_hold(t, m, p)
    }
    fn button_action(&mut self, addr: u32, t: &mut Work, m: &mut Work, p: &mut Work) {
        self.0.g.objects.button_action(addr, t, m, p)
    }
    fn damage(&mut self, t: &mut Work, p: &mut Work) -> bool {
        let mut m = Work::new(0x40);
        let g = self.0.g.exec.act.clone();
        crate::walker_act::damage_step(t, &mut m, p, &g, self.0.g.damage, self.0.eng)
    }
    fn touching(&mut self, _: i32) -> bool {
        self.0.g.mv.touching_11
    }
    fn ride_acc(&mut self, p: &Work) -> Option<Vec3> {
        self.0.g.objects.ride_acc(p)
    }
}

/// The land context the frame's collision reads (the runtime's `LandCtx`).
pub type LandCtx<'a> = LandContext<'a>;

/// One walker frame: the exec 0x800B3A8C on the native work areas.
pub fn walker_frame<'a>(
    t: &mut Work,
    m: &mut Work,
    p: &mut Work,
    g: &mut PlayerGlobals<'a>,
    land: &mut LandContext<'a>,
    eng: &mut dyn Engine,
) {
    let mut stack = MatrixStack::new();
    let exec_g = g.exec.clone();
    let mv = g.mv.clone();
    let hist = g.hist.clone();
    let lock: *mut dyn LockWorld = g.lock as *mut dyn LockWorld;
    let mut f = Frame {
        g,
        land,
        stack: MatrixStack::new(),
        eng,
    };
    let fp: *mut Frame = &mut f;
    let (tp, mp, pp) = (t as *mut Work, m as *mut Work, p as *mut Work);
    // SAFETY: the exec calls the step, action and engine hooks one at a
    // time; none of them is re-entered while another runs.
    unsafe {
        let mut act = Act(&mut *fp);
        let mut sc = PtrTurns::new(tp, mp, pp, &mv);
        walker_exec(
            &mut stack,
            &mut *tp,
            &mut *mp,
            &mut *pp,
            &exec_g,
            &hist,
            &mut *fp,
            &mut act,
            &mut sc,
            &mut *lock,
            (*fp).eng,
        );
    }
}
