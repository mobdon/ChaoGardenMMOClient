//! The walker exec's adapter: maps each GSNE8P address `walker_exec`
//! calls through [`ExecSteps`] onto its Rust port, reading and writing the
//! native work areas. An address with no port yet panics with the address
//! (fails closed) rather than running a stand-in.
use crate::{
    native_work::{pw, tw, Work},
    ninja_matrix::{MatrixStack, Vec3},
    player_accel::{ground_move, GroundPlayer, MoveInput, SteerInput},
    player_analog::{get_analog, AnalogContext, AnalogPlayer},
    player_path::{follow, Path, PathPlayer},
    player_physics::PlayerParameters,
    player_slope::{slope_turn, SlopePlayer},
    player_turn::{turn_smoothly, turn_toward, TurnPlayer},
    walker_exec::ExecSteps,
    walker_frame::{
        clamp_speed, player_speed_from_world, player_world_speed, FrameInput, SpeedTask, SpeedWork,
    },
    walker_move::{
        accel, accel_air, accel_gravity, accel_hover, accel_mode43, accel_run, aim_carry,
        aim_toward, air_level, jump_assist, slope_aim, steer, turn_back, AccelWork, JumpTarget,
        JumpWork, SteerCalls, SteerInput as WalkSteer,
    },
};

/// The physics row a player work holds at +0xC0.
pub fn phys(p: &Work) -> PlayerParameters {
    let f = |o: usize| p.f(o);
    PlayerParameters {
        jump2_timer: p.i(0xC0),
        pos_error: f(0xC4),
        lim_h_spd: f(0xC8),
        lim_v_spd: f(0xCC),
        max_x_spd: f(0xD0),
        max_psh_spd: f(0xD4),
        jmp_y_spd: f(0xD8),
        nocon_speed: f(0xDC),
        slide_speed: f(0xE0),
        jog_speed: f(0xE4),
        run_speed: f(0xE8),
        rush_speed: f(0xEC),
        crash_speed: f(0xF0),
        dash_speed: f(0xF4),
        jmp_addit: f(0xF8),
        run_accel: f(0xFC),
        air_accel: f(0x100),
        slow_down: f(0x104),
        run_break: f(0x108),
        air_break: f(0x10C),
        air_resist_air: f(0x110),
        air_resist: f(0x114),
        air_resist_y: f(0x118),
        air_resist_z: f(0x11C),
        grd_frict: f(0x120),
        grd_frict_z: f(0x124),
        lim_frict: f(0x128),
        rat_bound: f(0x12C),
        rad: f(0x130),
        height: f(0x134),
        weight: f(0x138),
        eyes_height: f(0x13C),
        center_height: f(0x140),
    }
}

/// What the movement routines read from outside the three work areas.
#[derive(Clone, Default)]
pub struct MoveEnv {
    /// The gravity vector (0x801E5624) and stage number.
    pub gravity: Vec3,
    pub stage: i32,
    /// The stick state fn_8004E560 reads (angle and magnitude from the
    /// pad, the frame word, the touched kind-0x11 object's angles, raw
    /// stick Y).
    pub analog: AnalogContext,
    /// Raw stick X and Y, and the camera's position and X/Y angles.
    pub raw_stick: [i16; 2],
    pub camera_pos: Vec3,
    pub camera_angles: (i32, i32),
    /// r13-0x79A8 / r13-0x79AC: the X and Z angles to level to.
    pub level: (i32, i32),
    /// The floor normal of the dynamic collision (+0x1C of the query at
    /// player +0x90) and the query's across-plane push (+0x78).
    pub floor_up: Vec3,
    pub across: Vec3,
    /// The player's path (+0xB8), its jump targets (+0xB4) and the walker
    /// sharp-turn byte the ground step reads.
    pub path: Option<Path>,
    pub targets: Vec<JumpTarget>,
    pub walker_sharp: u8,
    /// fn_800060EC(task, 0x11) found a contact.
    pub touching_11: bool,
}

/// The turn routines on the work areas (Sonic's `player_turn`,
/// `player_slope` and `player_path`).
pub struct Turns<'a> {
    pub t: &'a mut Work,
    pub m: &'a mut Work,
    pub p: &'a mut Work,
    pub env: &'a MoveEnv,
    stack: MatrixStack,
}

impl<'a> Turns<'a> {
    pub fn new(t: &'a mut Work, m: &'a mut Work, p: &'a mut Work, env: &'a MoveEnv) -> Self {
        Self {
            t,
            m,
            p,
            env,
            stack: MatrixStack::new(),
        }
    }

    fn turn_player(&self) -> TurnPlayer {
        TurnPlayer {
            status: self.t.h(tw::FLAG),
            nocon: self.p.h(0x14),
            confuse: self.p.h(0x1A),
            up: self.p.f(0x34),
            scale: self.p.f(0x40),
        }
    }

    fn analog_player(&self) -> AnalogPlayer {
        AnalogPlayer {
            status: self.t.h(tw::FLAG),
            angles: self.t.ang3(tw::ANG),
            nocon: self.p.h(0x14),
            confuse: self.p.h(0x1A),
        }
    }
}

impl SteerCalls for Turns<'_> {
    fn flagged_turn(&mut self) {
        let Some(path) = self.env.path.as_ref() else {
            panic!("fn_80038F40 needs the player's path (+0xB8)");
        };
        let mut pp = PathPlayer {
            status: self.t.h(tw::FLAG),
            angles: self.t.ang3(tw::ANG),
            pos: self.t.v3(tw::POS),
            speed_x: self.p.f(pw::SPD),
            dash_speed: self.p.f(0xF4),
            path_dist: self.p.f(0x30),
            normal: self.env.floor_up,
            aim: self.m.i(0x1C),
        };
        follow(&mut self.stack, path, &mut pp, self.env.analog.magnitude);
        self.t.set_h(tw::FLAG, pp.status);
        self.t.set_ang3(tw::ANG, pp.angles);
        self.p.set_f(pw::SPD, pp.speed_x);
        self.p.set_f(0x30, pp.path_dist);
        self.m.set_i(0x1C, pp.aim);
    }

    fn analog(&mut self) -> Option<i32> {
        let ap = self.analog_player();
        let a = get_analog(&mut self.stack, ap, self.env.analog, true, false);
        if a.active {
            a.angle
        } else {
            None
        }
    }

    fn analog_magnitude(&mut self) -> Option<(i32, f32)> {
        let ap = self.analog_player();
        let a = get_analog(&mut self.stack, ap, self.env.analog, true, true);
        if a.active {
            Some((a.angle.unwrap_or_default(), a.magnitude.unwrap_or_default()))
        } else {
            None
        }
    }

    fn slope_turn(&mut self) {
        let mut sp = SlopePlayer {
            status: self.t.h(tw::FLAG),
            angles: self.t.ang3(tw::ANG),
            nocon: self.p.h(0x14),
            surface: self.p.w(0x88),
            speed: self.p.v3(pw::SPD),
            jog_speed: self.p.f(0xE4),
            motion: self.m.v3(0),
            aim: self.m.i(0x1C),
        };
        let turn = self.turn_player();
        slope_turn(
            &mut self.stack,
            &mut sp,
            self.env.gravity,
            self.env.stage as i16,
            turn,
            self.env.analog,
        );
        self.t.set_ang3(tw::ANG, sp.angles);
        self.p.set_v3(pw::SPD, sp.speed);
        self.m.set_i(0x1C, sp.aim);
    }

    fn turn_toward(&mut self, aim: &mut i32) {
        let mut a = self.t.ang3(tw::ANG);
        let mut s = self.p.v3(pw::SPD);
        turn_toward(
            &mut self.stack,
            &mut a,
            &mut s,
            self.p.f(0xF4),
            self.p.f(0x34),
            *aim,
        );
        self.t.set_ang3(tw::ANG, a);
        self.p.set_v3(pw::SPD, s);
    }

    fn turn_smoothly(&mut self, aim: &mut i32) {
        let mut a = self.t.ang3(tw::ANG);
        let mut s = self.p.v3(pw::SPD);
        let turn = self.turn_player();
        turn_smoothly(&mut self.stack, &mut a, &mut s, turn, self.env.analog, *aim);
        self.t.set_ang3(tw::ANG, a);
        self.p.set_v3(pw::SPD, s);
    }
}

fn accel_work(p: &Work) -> AccelWork {
    AccelWork {
        spd: p.v3(pw::SPD),
        f40: p.f(0x40),
        f_d0: p.f(0xD0),
        air_resist: [p.f(0x114), p.f(0x118), p.f(0x11C)],
        decel_x: p.f(0x120),
        decel_z: p.f(0x124),
        stop_share: p.f(0x128),
        weight: p.f(0x138),
    }
}

fn ground_player(t: &Work, m: &Work, p: &Work, env: &MoveEnv) -> GroundPlayer {
    GroundPlayer {
        status: t.h(tw::FLAG),
        angles: t.ang3(tw::ANG),
        pos: t.v3(tw::POS),
        motion: m.v3(0),
        base: m.v3(0xC),
        aim: m.i(0x1C),
        nocon: p.h(0x14),
        confuse: p.h(0x1A),
        path_dist: p.f(0x30),
        up: p.f(0x34),
        excess: p.f(0x38),
        push: p.f(0x3C),
        scale: p.f(0x40),
        accel: p.v3(0x58),
        speed: p.v3(pw::SPD),
        down: p.v3(0x7C),
        surface: p.w(0x88),
        normal: env.floor_up,
        character: p.b(pw::CHAR),
        wall: p.v3(0x70),
    }
}

fn put_ground(t: &mut Work, m: &mut Work, p: &mut Work, g: &GroundPlayer) {
    t.set_h(tw::FLAG, g.status);
    t.set_ang3(tw::ANG, g.angles);
    m.set_i(0x1C, g.aim);
    p.set_f(0x30, g.path_dist);
    p.set_f(0x38, g.excess);
    p.set_f(0x3C, g.push);
    p.set_v3(0x58, g.accel);
    p.set_v3(pw::SPD, g.speed);
}

/// The movement routines of the exec (everything but the collision, motion
/// and frame-end steps), on the work areas.
pub fn call_move(
    addr: u32,
    stack: &mut MatrixStack,
    t: &mut Work,
    m: &mut Work,
    p: &mut Work,
    env: &mut MoveEnv,
) -> Option<i32> {
    let flag = t.h(tw::FLAG);
    let angles = t.ang3(tw::ANG);
    let r = match addr {
        0x8004_C734 => {
            let mut g = ground_player(t, m, p, env);
            let input = MoveInput {
                steer: SteerInput {
                    analog: env.analog,
                    gravity: env.gravity,
                    stage: env.stage as i16,
                    raw_stick: env.raw_stick,
                    camera: env.camera_pos,
                },
                walker_sharp: env.walker_sharp,
            };
            ground_move(stack, &mut g, &phys(p), &input, env.path.as_ref());
            put_ground(t, m, p, &g);
            0
        }
        0x8004_7A1C => {
            let mut a = angles;
            let mut pos = t.v3(tw::POS);
            let mut spd = p.v3(pw::SPD);
            air_level(
                stack,
                flag,
                &mut a,
                &mut pos,
                &mut spd,
                p.w(0x24),
                p.f(0xF4),
                p.f(0x134),
                env.level,
            );
            t.set_ang3(tw::ANG, a);
            t.set_v3(tw::POS, pos);
            p.set_v3(pw::SPD, spd);
            0
        }
        0x8004_8ACC => {
            let w = accel_work(p);
            if let Some(a) = accel(
                stack,
                flag,
                angles,
                &w,
                m.v3(0xC),
                env.gravity,
                &mut env.across,
            ) {
                p.set_v3(0x58, a);
            }
            0
        }
        0x8004_8724 => {
            if let Some(a) = accel_gravity(stack, flag, angles, p.f(0x138), m.v3(0xC), env.gravity)
            {
                p.set_v3(0x58, a);
            }
            0
        }
        0x8004_8834 => {
            let w = accel_work(p);
            if let Some(a) = accel_mode43(
                stack,
                flag,
                angles,
                &w,
                p.f(0x34),
                p.f(0xE8),
                m.v3(0xC),
                env.gravity,
                &mut env.across,
            ) {
                p.set_v3(0x58, a);
            }
            0
        }
        _ => return call_turning(addr, stack, t, m, p, env),
    };
    Some(r)
}

fn call_turning(
    addr: u32,
    stack: &mut MatrixStack,
    t: &mut Work,
    m: &mut Work,
    p: &mut Work,
    env: &mut MoveEnv,
) -> Option<i32> {
    let env_c = env.clone();
    let flag = t.h(tw::FLAG);
    let angles = t.ang3(tw::ANG);
    let w = accel_work(p);
    let (run_break, air_x, run_speed, f100, f10c) =
        (p.f(0x108), p.f(0x110), p.f(0xE8), p.f(0x100), p.f(0x10C));
    let (mw_acc, mw_y, mut aim) = (m.v3(0xC), m.f(4), m.i(0x1C));
    let r = match addr {
        0x8004_8EC4 => {
            let a = {
                let mut tu = Turns::new(t, m, p, &env_c);
                let mut aim2 = aim;
                accel_run(
                    stack,
                    flag,
                    angles,
                    &w,
                    run_break,
                    mw_acc,
                    env.gravity,
                    &mut || tu.turn_smoothly(&mut aim2),
                )
            };
            if let Some(a) = a {
                p.set_v3(0x58, a);
            }
            0
        }
        0x8004_B330 => {
            let a = {
                let mut tu = Turns::new(t, m, p, &env_c);
                let held = tu.analog_magnitude();
                let mut once = Some(held);
                let mut turn_aim = None;
                let a = accel_air(
                    stack,
                    angles,
                    &w,
                    air_x,
                    run_speed,
                    f100,
                    f10c,
                    mw_acc,
                    mw_y,
                    &mut aim,
                    env.gravity,
                    &mut || once.take().flatten(),
                    &mut |a| turn_aim = Some(a),
                );
                if let Some(mut a2) = turn_aim {
                    tu.turn_smoothly(&mut a2);
                }
                a
            };
            m.set_i(0x1C, aim);
            p.set_v3(0x58, a);
            0
        }
        0x8004_B168 => {
            let a = {
                let mut tu = Turns::new(t, m, p, &env_c);
                accel_hover(
                    stack,
                    angles,
                    w.spd,
                    [air_x, w.air_resist[1], w.air_resist[2]],
                    run_speed,
                    f100,
                    f10c,
                    mw_acc,
                    &mut aim,
                    &mut tu,
                )
            };
            m.set_i(0x1C, aim);
            p.set_v3(0x58, a);
            0
        }
        0x8004_8400 => {
            let mut tu = Turns::new(t, m, p, &env_c);
            aim_toward(flag, angles[1], &mut aim, &mut tu);
            drop(tu);
            m.set_i(0x1C, aim);
            0
        }
        0x8004_8370 => {
            let mode2 = t.ub(2);
            let mut a = angles;
            let mut spd = p.v3(pw::SPD);
            let dotp = p.f(0x34);
            {
                let mut tu = Turns::new(t, m, p, &env_c);
                aim_carry(
                    stack, flag, mode2, &mut a, dotp, &mut spd, &mut aim, &mut tu,
                );
            }
            t.set_ang3(tw::ANG, a);
            p.set_v3(pw::SPD, spd);
            m.set_i(0x1C, aim);
            0
        }
        0x8004_7F14 => {
            let spd = m.v3(0);
            let (t14, attr, fe4) = (p.h(0x14), p.w(0x88), p.f(0xE4));
            let c = {
                let mut tu = Turns::new(t, m, p, &env_c);
                slope_aim(
                    stack,
                    flag,
                    angles,
                    spd,
                    &mut aim,
                    t14,
                    attr,
                    fe4,
                    env.gravity,
                    &mut tu,
                )
            };
            m.set_i(0x1C, aim);
            c.to_bits() as i32
        }
        0x8004_8490 => {
            let inp = WalkSteer {
                task_flag: flag,
                angles,
                attr: p.w(0x88),
                normal: p.v3(0x7C),
                stick: (env.raw_stick[0], env.raw_stick[1]),
                camera: env.camera_angles,
            };
            {
                let mut tu = Turns::new(t, m, p, &env_c);
                steer(stack, &inp, &mut aim, &mut tu);
            }
            m.set_i(0x1C, aim);
            0
        }
        0x8004_E490 => {
            let touching = env.touching_11;
            let mut tu = Turns::new(t, m, p, &env_c);
            turn_back(flag, angles[1], &mut tu, &mut |_| touching) as i32
        }
        0x8003_6CE4 => {
            let inp = WalkSteer {
                task_flag: flag,
                angles,
                attr: p.w(0x88),
                normal: p.v3(0x7C),
                stick: (env.raw_stick[0], env.raw_stick[1]),
                camera: env.camera_angles,
            };
            let jw = JumpWork {
                task_pos: t.v3(tw::POS),
                mw_spd: m.v3(0),
                air_resist: [p.f(0x110), p.f(0x118), p.f(0x11C)],
                f_cc: p.f(0xCC),
                f_e8: p.f(0xE8),
                run_accel: p.f(0xFC),
                weight: p.f(0x138),
            };
            let mut yaw = angles[1];
            let mut acc = m.v3(0xC);
            let mut targets = std::mem::take(&mut env.targets);
            let mut a43 = None;
            let r = {
                let mut tu = Turns::new(t, m, p, &env_c);
                jump_assist(
                    stack,
                    &inp,
                    &mut yaw,
                    &mut aim,
                    &mut acc,
                    &jw,
                    &mut targets,
                    &mut tu,
                    &mut |_, y, a| a43 = Some((y, a)),
                )
            };
            env.targets = targets;
            if r {
                t.set_i(tw::ANG + 4, yaw);
                m.set_v3(0xC, acc);
                if a43.is_some() {
                    call_move(0x8004_8834, stack, t, m, p, env);
                }
            }
            r as i32
        }
        0x8004_6950 => {
            let v = clamp_speed(m.v3(0), p.f(0xC8), p.f(0xCC));
            m.set_v3(0, v);
            0
        }
        0x8004_6DE4 => {
            let mut w = SpeedWork {
                spd: p.v3(pw::SPD),
                acc: p.v3(0x58),
                dotp: p.f(0x34),
                attr: p.w(0x88),
                floor_normal: p.v3(0x7C),
                jog_speed: p.f(0xE4),
                run_speed: p.f(0xE8),
                run_break: p.f(0x108),
                lim_h_spd: p.f(0xC8),
                lim_v_spd: p.f(0xCC),
                flags_1a8: p.w(0x1A8),
                height_1b0: p.f(0x1B0),
            };
            let task = SpeedTask {
                mode: t.ub(tw::MODE),
                angles,
                pos_y: t.f(tw::POS + 4),
            };
            let mut world = m.v3(0);
            let held = {
                let mut tu = Turns::new(t, m, p, &env_c);
                tu.analog().is_some()
            };
            player_world_speed(stack, &mut w, &task, &mut world, |_| held);
            p.set_v3(pw::SPD, w.spd);
            m.set_v3(0, world);
            0
        }
        0x8003_E668 => {
            let i = FrameInput {
                task_flag: flag,
                task_angles: angles,
                world: m.v3(0),
                vector_angles: m.ang3(0x18),
                attr: p.w(0x88),
                last_attr: p.w(0x8C),
                floor_normal: p.v3(0x7C),
                run_accel: p.f(0xFC),
                weight: p.f(0x138),
                global: env.gravity,
                stage: env.stage,
            };
            let o = player_speed_from_world(stack, &i);
            p.set_v3(pw::SPD, o.spd);
            p.set_f(0x34, o.dotp);
            t.set_ang3(tw::ANG, o.task_angles);
            m.set_ang3(0x18, o.vector_angles);
            0
        }
        _ => return None,
    };
    Some(r)
}

/// An [`ExecSteps`] over the movement routines; the collision, motion and
/// frame-end steps go to `rest` (by address).
pub struct MoveSteps<'a> {
    pub stack: MatrixStack,
    pub env: MoveEnv,
    pub rest: &'a mut dyn ExecSteps,
}

impl ExecSteps for MoveSteps<'_> {
    fn call(&mut self, addr: u32, t: &mut Work, m: &mut Work, p: &mut Work) -> i32 {
        match call_move(addr, &mut self.stack, t, m, p, &mut self.env) {
            Some(r) => r,
            None => self.rest.call(addr, t, m, p),
        }
    }
    fn set_velocity(&mut self, pnum: i8, v: Vec3) {
        self.rest.set_velocity(pnum, v)
    }
    fn analog(&mut self, t: &Work, p: &Work) -> Option<(i32, f32)> {
        let a = get_analog(
            &mut self.stack,
            AnalogPlayer {
                status: t.h(tw::FLAG),
                angles: t.ang3(tw::ANG),
                nocon: p.h(0x14),
                confuse: p.h(0x1A),
            },
            self.env.analog,
            true,
            true,
        );
        a.active
            .then(|| (a.angle.unwrap_or_default(), a.magnitude.unwrap_or_default()))
    }
    fn step_motion(&mut self, p: &Work) -> (u32, f32, f32) {
        self.rest.step_motion(p)
    }
    fn set_ccl_centers(&mut self, a: Vec3, b: Vec3) {
        self.rest.set_ccl_centers(a, b)
    }
}
