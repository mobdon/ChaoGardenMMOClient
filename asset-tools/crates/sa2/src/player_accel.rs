//! The player's ground acceleration for one frame, as SA2B's main.dol
//! (GameCube, GSNE8P) computes it: fn_8004B568 (stick-relative),
//! fn_8004BA74 (camera-relative steering) and fn_8004C734, the ground step
//! seven character mains share. Both add gravity and air
//! resistance, turn toward the input (fn_80047714 / fn_800473B4) and add
//! the stick's push or the slow-down, with the friction limits that stop a
//! standing player sliding. `tests/player_accel_dol.rs` checks them against
//! main.dol run under `tests/dol_ppc.py`.
use crate::{
    msl_math::atan2f,
    ninja_matrix::{rotate_by_angles, unrotate_by_angles, MatrixStack},
    ninja_trig::{fctiwz, fctiwz_f64, fmadds, fnmsubs, nj_sin},
    ninja_vector::{inner_product, outer_product, scalor, unit_vector, vector_angle},
    player_analog::{get_analog, AnalogContext, AnalogPlayer},
    player_path::{follow, Path, PathPlayer},
    player_physics::PlayerParameters,
    player_slope::{slope_turn, slope_turn_strong, SlopePlayer},
    player_turn::{adjust_angle, diff_angle, turn_smoothly, turn_toward, TurnPlayer},
    walker_collide::{plane_distance, plane_intersection, Line, Plane},
};

/// sdata2 constants (r2 = 0x803B6520).
const ZERO: f32 = 0.0; // r2-0x75F8
const RUN_ACCEL_FAST: f32 = f32::from_bits(0x3ecc_cccd); // r2-0x7518, 0.4
const HALF: f32 = 0.5; // r2-0x75CC
const SHARP_TURN_ACCEL: f32 = f32::from_bits(0x3f4c_cccd); // r2-0x7560, 0.8

/// The task, motion and player work fields the ground routines read and
/// write.
#[derive(Debug, Clone, Copy, Default)]
pub struct GroundPlayer {
    /// Task work status (+4), angles (+8, +0xC, +0x10), position (+0x14).
    pub status: i16,
    pub angles: [i32; 3],
    pub pos: [f32; 3],
    /// Motion work velocity (+0), acceleration carried in (+0xC), aim yaw
    /// (+0x1C).
    pub motion: [f32; 3],
    pub base: [f32; 3],
    pub aim: i32,
    /// Player work +0x14, +0x1A, +0x30 (path distance), +0x34, +0x38,
    /// +0x3C, +0x40.
    pub nocon: i16,
    pub confuse: i16,
    pub path_dist: f32,
    pub up: f32,
    pub excess: f32,
    pub push: f32,
    pub scale: f32,
    /// Player work +0x58 (the result), +0x64 (local velocity), +0x7C (the
    /// player's own gravity direction), +0x88 (surface flags).
    pub accel: [f32; 3],
    pub speed: [f32; 3],
    pub down: [f32; 3],
    pub surface: u32,
    /// Floor normal (+0x1C of the collision info at player work +0x90).
    pub normal: [f32; 3],
    /// Player work +1 (character number) and +0x70 (the normal of a wall
    /// the player is pushing against, zero when none).
    pub character: i8,
    pub wall: [f32; 3],
}

impl GroundPlayer {
    fn analog(&self) -> AnalogPlayer {
        AnalogPlayer {
            status: self.status,
            angles: self.angles,
            nocon: self.nocon,
            confuse: self.confuse,
        }
    }

    fn turn(&self) -> TurnPlayer {
        TurnPlayer {
            status: self.status,
            nocon: self.nocon,
            confuse: self.confuse,
            up: self.up,
            scale: self.scale,
        }
    }

    fn slope(&self, phys: &PlayerParameters) -> SlopePlayer {
        SlopePlayer {
            status: self.status,
            angles: self.angles,
            nocon: self.nocon,
            surface: self.surface,
            speed: self.speed,
            jog_speed: phys.jog_speed,
            motion: self.motion,
            aim: self.aim,
        }
    }

    /// fn_80038F40 on the player's path, which the original reads whenever
    /// status bit 0x2000 is set.
    fn follow(
        &mut self,
        stack: &mut MatrixStack,
        phys: &PlayerParameters,
        path: Option<&Path>,
        stick: f32,
    ) {
        let path = path.expect("status 0x2000 needs the player's path");
        let mut pp = PathPlayer {
            status: self.status,
            angles: self.angles,
            pos: self.pos,
            speed_x: self.speed[0],
            dash_speed: phys.dash_speed,
            path_dist: self.path_dist,
            normal: self.normal,
            aim: self.aim,
        };
        follow(stack, path, &mut pp, stick);
        (
            self.status,
            self.angles,
            self.speed[0],
            self.path_dist,
            self.aim,
        ) = (pp.status, pp.angles, pp.speed_x, pp.path_dist, pp.aim);
    }

    /// Turn on the spot: yaw toward `target` by up to 0x2000, velocity
    /// carried round (fn_8003E9E0, AdjustAngle, fn_8003EA6C).
    fn pivot(&mut self, stack: &mut MatrixStack, target: i32) {
        let world = rotate_by_angles(stack, self.angles, self.speed);
        self.angles[1] = adjust_angle(self.angles[1], target, 0x2000);
        self.speed = unrotate_by_angles(stack, self.angles, world);
    }
}

/// fn_8004B568(task work, motion work, player work). `gravity` is the
/// vector at 0x801E5624; `analog` the stick state fn_8004E560 reads.
pub fn accelerate(
    stack: &mut MatrixStack,
    p: &mut GroundPlayer,
    phys: &PlayerParameters,
    gravity: [f32; 3],
    analog: AnalogContext,
) {
    let mut acc = [0, 1, 2].map(|k| fmadds(gravity[k], phys.weight, p.base[k]));
    let input = get_analog(stack, p.analog(), analog, true, true);
    let sx = p.speed[0];
    if !input.active && sx > phys.run_speed {
        acc[0] = fmadds(phys.air_resist, sx, acc[0]);
    } else if sx > phys.rush_speed {
        acc[0] = fmadds(phys.air_resist, sx - phys.rush_speed, acc[0]);
    } else if sx < ZERO {
        acc[0] = fmadds(phys.air_resist, sx, acc[0]);
    }
    acc[1] = fmadds(phys.air_resist_y, p.speed[1], acc[1]);
    acc[2] = fmadds(phys.air_resist_z, p.speed[2], acc[2]);

    let push = if !input.active {
        p.push = ZERO;
        if p.speed[0] > ZERO {
            phys.slow_down
        } else if p.speed[0] < ZERO {
            -phys.slow_down
        } else {
            ZERO
        }
    } else {
        let ang = input.angle.unwrap_or_default();
        let mut mag = input.magnitude.unwrap_or_default();
        mag = if phys.run_speed > p.speed[0] {
            mag * phys.run_accel
        } else {
            mag * (RUN_ACCEL_FAST * phys.run_accel)
        };
        let d = diff_angle(p.angles[1], ang);
        let sx = p.speed[0];
        let turn = p.turn();
        p.aim = ang;
        if sx == ZERO && d > 0x1000 {
            // Standing: turn on the spot and don't push this frame.
            mag = ZERO;
            p.pivot(stack, ang);
        } else if sx >= HALF * (phys.jog_speed + phys.run_speed) && d > 0x1000 {
            mag = phys.slow_down;
            turn_smoothly(stack, &mut p.angles, &mut p.speed, turn, analog, ang);
        } else if sx >= phys.jog_speed && d < 0x1000 {
            turn_toward(
                stack,
                &mut p.angles,
                &mut p.speed,
                phys.dash_speed,
                p.up,
                ang,
            );
        } else if sx >= phys.jog_speed && sx <= phys.rush_speed && d > 0x2000 {
            mag *= SHARP_TURN_ACCEL;
            turn_smoothly(stack, &mut p.angles, &mut p.speed, turn, analog, ang);
        } else {
            turn_smoothly(stack, &mut p.angles, &mut p.speed, turn, analog, ang);
        }
        p.push = mag;
        mag
    };

    let sx = p.speed[0];
    let scale = p.scale;
    let limit = |acc: &[f32; 3]| scale * (acc[1] * phys.lim_frict);
    if sx == ZERO {
        acc[0] += push;
        let lim = limit(&acc);
        if acc[0] < lim && acc[0] > -lim {
            acc[0] = ZERO;
        }
    } else if push < ZERO {
        if sx > ZERO {
            let a = acc[0] + push;
            acc[0] = if a * push < ZERO { ZERO } else { a };
        } else {
            let a = limit(&acc) + (push + acc[0]);
            acc[0] = if a >= ZERO { ZERO } else { a };
        }
    } else {
        let lim = limit(&acc);
        if sx < ZERO {
            acc[0] += push;
            if acc[0] * push < ZERO {
                acc[0] = ZERO;
            }
        } else if lim > ZERO && lim < push {
            let a = push + acc[0];
            acc[0] = if a > lim {
                p.excess = a - lim;
                lim
            } else {
                a
            };
        } else {
            acc[0] += push;
        }
    }
    if p.speed[2] == ZERO {
        let lim = limit(&acc);
        if acc[2] < lim && acc[2] > -lim {
            acc[2] = ZERO;
        }
    } else {
        acc[2] = HALF * -p.speed[2];
    }
    acc[1] += ZERO;
    p.accel = acc;
}

/// More sdata2 constants fn_8004BA74 uses.
const UPRIGHT: f32 = f32::from_bits(0x3f75_c28f); // r2-0x7510, 0.96
const UPRIGHT_DRAG: f32 = f32::from_bits(0x3fd9_999a); // r2-0x742C, 1.7
const TILTED: f32 = f32::from_bits(0x3f35_c28f); // r2-0x7428, 0.71
const PATH_LIFT: f32 = f32::from_bits(0x3f4c_cccd); // r2-0x7560, 0.8
const PUSH_JOG: f32 = f32::from_bits(0x3f19_999a); // r2-0x7514, 0.6
const PUSH_RUN: f32 = f32::from_bits(0x3f33_3333); // r2-0x7534, 0.7
const PUSH_RUSH: f32 = f32::from_bits(0x3f66_6666); // r2-0x7548, 0.9
const RUSH_ACCEL: f32 = f32::from_bits(0x3e99_999a); // r2-0x7580, 0.3
const STEEP_DOT: f64 = f64::from_bits(0xbfeb_851e_b851_eb85); // r2-0x7488, -0.86
const BAMS_PER_RAD: f64 = f64::from_bits(0x40c4_5f30_b217_88bb); // r2-0x75E8
const SLOPE_RATE: f32 = 512.0; // r2-0x7424
const CREEP: f32 = f32::from_bits(0x3d50_e560); // r2-0x7420, 0.051
const NEG_CREEP: f32 = f32::from_bits(0xbd50_e560); // r2-0x741C

/// What fn_8004BA74 reads from outside the player.
#[derive(Debug, Clone, Copy, Default)]
pub struct SteerInput {
    /// The stick state fn_8004E560 reads.
    pub analog: AnalogContext,
    /// The gravity vector (0x801E5624) and stage number (fn_80048150).
    pub gravity: [f32; 3],
    pub stage: i16,
    /// Raw stick X and Y (0x801E5438 + 0x34 * player + 0x1C, + 0x1E).
    pub raw_stick: [i16; 2],
    /// The player's camera position (+0x18 of the camera work that
    /// 0x80175378[player] points to).
    pub camera: [f32; 3],
}

fn creeping(x: f32) -> bool {
    x < CREEP && x > NEG_CREEP
}

/// fn_8004BA74(task work, motion work, player work): ground acceleration
/// with camera-relative steering. `path` is the player work's path (+0xB8),
/// which the original reads whenever status bit 0x2000 is set.
pub fn accelerate_steered(
    stack: &mut MatrixStack,
    p: &mut GroundPlayer,
    phys: &PlayerParameters,
    input: &SteerInput,
    path: Option<&Path>,
) {
    let mut lift = ZERO;
    let acc = [0, 1, 2].map(|k| fnmsubs(p.down[k], phys.weight, p.base[k]));
    let mut acc = unrotate_by_angles(stack, p.angles, acc);
    let analog = get_analog(stack, p.analog(), input.analog, true, true);
    let active = analog.active;
    let sx = p.speed[0];
    if !active && sx > phys.run_speed {
        acc[0] = fmadds(phys.air_resist, sx, acc[0]);
    } else if active && sx > phys.max_x_spd && p.up > UPRIGHT {
        acc[0] = fmadds(UPRIGHT_DRAG * phys.air_resist, sx - phys.max_x_spd, acc[0]);
    } else if sx > phys.max_x_spd {
        acc[0] = fmadds(phys.air_resist, sx - phys.max_x_spd, acc[0]);
    } else if sx < ZERO {
        acc[0] = fmadds(phys.air_resist, sx, acc[0]);
    }
    acc[1] = fmadds(phys.air_resist_y, p.speed[1], acc[1]);
    acc[2] = fmadds(phys.air_resist_z, p.speed[2], acc[2]);

    let push = if !active {
        let mut push = ZERO;
        if p.up < TILTED {
            let mut s = p.slope(phys);
            slope_turn(
                stack,
                &mut s,
                input.gravity,
                input.stage,
                p.turn(),
                input.analog,
            );
            (p.angles, p.speed, p.aim) = (s.angles, s.speed, s.aim);
        } else if p.speed[0] > ZERO {
            push = phys.slow_down;
        } else if p.speed[0] < ZERO {
            push = -phys.slow_down;
        }
        p.push = ZERO;
        push
    } else {
        let mut ang = analog.angle.unwrap_or_default();
        let mag = analog.magnitude.unwrap_or_default();
        if p.status & 0x2000 != 0 {
            p.follow(stack, phys, path, input.analog.magnitude);
            lift = ZERO - PATH_LIFT;
            ang = p.aim;
        }
        let mut mag = push_below_max(p.speed[0], mag, phys).unwrap_or(mag * phys.run_accel);
        let d = diff_angle(p.angles[1], ang);
        if p.speed[0] == ZERO && d > 0x1000 {
            p.aim = ang;
            mag = ZERO;
            p.pivot(stack, ang);
        } else {
            acc[2] = steer_by_camera(stack, p, phys, input, acc[2]);
            p.pivot(stack, p.aim);
        }
        p.push = mag;
        mag
    };

    settle_x(p, phys, &mut acc, push, active);
    settle_z(p, phys, &mut acc);
    acc[1] += lift;
    p.accel = acc;
}

/// The forward acceleration's friction and push step fn_8004BA74 and
/// fn_8004C734 share (sticking still, ground grip, the lim_frict limit).
fn settle_x(
    p: &mut GroundPlayer,
    phys: &PlayerParameters,
    acc: &mut [f32; 3],
    push: f32,
    active: bool,
) {
    let sx = p.speed[0];
    let scale = p.scale;
    let limit = |acc: &[f32; 3]| scale * (acc[1] * phys.lim_frict);
    let slow_creep = |x: f32| !active && sx <= phys.jog_speed && creeping(x);
    if sx == ZERO {
        let lim = limit(acc);
        acc[0] += push;
        if !active {
            if acc[0] < lim && acc[0] > -lim {
                acc[0] = ZERO;
            }
            if creeping(acc[0]) {
                acc[0] = ZERO;
            }
        }
    } else if push < ZERO {
        if p.surface & 4 != 0 {
            let grip = if acc[1] < ZERO {
                scale * (acc[1] * phys.grd_frict)
            } else {
                ZERO
            };
            if grip > ZERO && grip < -push {
                let mut a = push + acc[0];
                if a < ZERO && a < -grip {
                    p.excess = a - grip;
                    a = -grip;
                } else if a > ZERO && a > grip {
                    p.excess = a - grip;
                    a = grip;
                }
                acc[0] = a;
            } else if slow_creep(acc[0]) {
                acc[0] = ZERO;
            } else {
                acc[0] += push;
            }
        } else if sx > ZERO {
            let a = acc[0] + push;
            acc[0] = if a * push < ZERO { ZERO } else { a };
        } else if slow_creep(acc[0]) {
            acc[0] = ZERO;
        } else {
            let a = limit(acc) + push;
            acc[0] = if a >= ZERO { ZERO } else { a };
        }
    } else {
        let lim = limit(acc);
        if sx < ZERO && creeping(acc[0]) {
            acc[0] += push;
            if acc[0] * push < ZERO {
                acc[0] = ZERO;
            }
        } else if lim > ZERO && lim < push {
            let a = push + acc[0];
            acc[0] = if a > lim {
                p.excess = a - lim;
                lim
            } else {
                a
            };
        } else if slow_creep(acc[0]) {
            acc[0] = ZERO;
        } else {
            acc[0] += push;
        }
    }
}

/// The sideways acceleration's friction step they share.
fn settle_z(p: &GroundPlayer, phys: &PlayerParameters, acc: &mut [f32; 3]) {
    let scale = p.scale;
    let limit = |acc: &[f32; 3]| scale * (acc[1] * phys.lim_frict);
    if p.speed[2] == ZERO {
        let lim = limit(acc);
        if acc[2] < lim && acc[2] > -lim {
            acc[2] = ZERO;
        }
    } else {
        let grip = if acc[1] < ZERO {
            scale * (acc[1] * phys.grd_frict_z)
        } else {
            ZERO
        };
        if p.surface & 4 != 0 {
            let mut z = ZERO + acc[2];
            if z > ZERO {
                z -= grip;
                if z < ZERO {
                    z = ZERO;
                }
            } else if z < ZERO {
                z += grip;
                if z > ZERO {
                    z = ZERO;
                }
            }
            acc[2] = z;
        } else {
            let mut k = ZERO;
            if acc[2] > ZERO {
                k = ZERO - grip;
            } else if acc[2] < ZERO {
                k = ZERO + grip;
            }
            let mut z = acc[2] + k;
            if ZERO != acc[2] && ZERO != k && z * acc[2] < ZERO {
                z = ZERO;
            }
            acc[2] = z;
        }
    }
}

/// The stick's push scaled by run_accel, by speed band below max_x_spd
/// (None at or above it).
fn push_below_max(sx: f32, mag: f32, phys: &PlayerParameters) -> Option<f32> {
    let accel = phys.run_accel;
    if sx >= phys.max_x_spd || sx.is_nan() || phys.max_x_spd.is_nan() {
        return None;
    }
    Some(if sx < phys.jog_speed {
        if mag > PUSH_JOG {
            mag * accel
        } else if sx < RUN_ACCEL_FAST * phys.jog_speed {
            (if mag < HALF { HALF } else { mag }) * accel
        } else {
            ZERO
        }
    } else if sx < phys.run_speed {
        if mag > PUSH_RUN || sx < HALF * (phys.run_speed + phys.run_speed) {
            mag * accel
        } else {
            ZERO
        }
    } else if sx < phys.rush_speed {
        if mag > PUSH_RUSH {
            mag * accel
        } else {
            mag * (RUSH_ACCEL * accel)
        }
    } else {
        mag * accel
    })
}

/// The aim from the raw stick, turned about the player's gravity axis from
/// the camera's side, then eased downhill. Returns acc.z (above dash speed
/// the sine of the turn is added to it).
fn steer_by_camera(
    stack: &mut MatrixStack,
    p: &mut GroundPlayer,
    phys: &PlayerParameters,
    input: &SteerInput,
    mut acc_z: f32,
) -> f32 {
    let [x, y] = input.raw_stick.map(|v| v as f32);
    let stick = fctiwz_f64(BAMS_PER_RAD * atan2f(-x, -y) as f64);
    let mut foot = [ZERO; 3];
    plane_distance(
        input.camera,
        &Plane {
            point: [ZERO; 3],
            normal: p.down,
        },
        Some(&mut foot),
    );
    let (cam, _) = unit_vector(foot);
    stack.push_unit();
    stack.rotate_axis(p.down, stick);
    let want = stack.calc_vector(cam);
    stack.pop();
    let fwd = rotate_by_angles(stack, p.angles, [1.0, ZERO, ZERO]);
    let mut turn = vector_angle(want, fwd, Some(p.down));
    let same_side = inner_product(outer_product(fwd, cam), outer_product(want, cam));
    if same_side >= ZERO || same_side.is_nan() {
        let sx = p.speed[0];
        if sx > phys.dash_speed {
            acc_z += nj_sin(turn);
        } else {
            let step = if sx > phys.rush_speed {
                0x100
            } else if sx > phys.run_speed {
                0x200
            } else if sx > phys.jog_speed {
                0x400
            } else {
                0x800
            };
            turn = adjust_angle(0, turn, step);
        }
    }
    p.aim = p.angles[1].wrapping_add(turn) & 0xffff;

    if scalor(p.motion) > phys.jog_speed {
        let (v, _) = unit_vector(p.motion);
        let dot = inner_product(input.gravity, v) as f64;
        if dot >= STEEP_DOT || dot.is_nan() {
            let mut g = unrotate_by_angles(stack, p.angles, input.gravity);
            if g[0] < ZERO {
                g[0] = -g[0];
            }
            let a = atan2f(g[2], g[0]);
            let target = p.angles[1].wrapping_add(fctiwz_f64(BAMS_PER_RAD * a as f64));
            if g[2] < ZERO {
                g[2] = -g[2];
            }
            p.aim = adjust_angle(p.aim, target, fctiwz(SLOPE_RATE * g[2]));
        }
    }
    acc_z
}

/// More sdata2 constants fn_8004C734 uses.
const FLAT: f32 = f32::from_bits(0x3dcc_cccd); // r2-0x75A8, 0.1
const BANKED: f32 = f32::from_bits(0x3f19_999a); // r2-0x7514, 0.6
const CLIMB_SPEED: f32 = f32::from_bits(0x3f94_7ae1); // r2-0x74BC, 1.16
const STEEP: [f32; 3] = [
    f32::from_bits(0xbecc_cccd), // r2-0x74D0, -0.4
    f32::from_bits(0xbe99_999a), // r2-0x7418, -0.3
    f32::from_bits(0xbdcc_cccd), // r2-0x75A0, -0.1
];
const STEEP_WEIGHT: [f32; 3] = [
    5.0,                         // r2-0x75F0
    f32::from_bits(0x3f4c_cccd), // r2-0x7560, 0.8
    f32::from_bits(0x3ecc_cccd), // r2-0x7518, 0.4
];
const WALL: f32 = 0.5; // r2-0x75CC
const WALL_GRAVITY: f32 = f32::from_bits(0x4087_3333); // r2-0x7414, 4.225
const SIDE: f32 = f32::from_bits(0x3f33_3333); // r2-0x7534, 0.7
const SIDE_GRAVITY: f32 = 2.0; // r2-0x75FC
const SLANT: f32 = f32::from_bits(0x3f5e_b852); // r2-0x749C, 0.87
const SLANT_GRAVITY: f32 = f32::from_bits(0x3fb3_3333); // r2-0x74E4, 1.4
const STRONG_SLOPE: f32 = f32::from_bits(0x3f70_a3d7); // r2-0x7410, 0.94
const TIPPED_DAMP: f32 = 10.0; // r2-0x7600

/// What fn_8004C734 reads from outside the player besides `SteerInput`.
#[derive(Debug, Clone, Copy, Default)]
pub struct MoveInput {
    pub steer: SteerInput,
    /// For characters 6 and 7 (the walkers): byte +0x35C of the object at
    /// +0x40 of the player's task (0x801E7788[player]); 1 turns sharply.
    pub walker_sharp: u8,
}

/// fn_8004C734(task work, motion work, player work): the ground movement
/// step seven character mains share. Surface flag 0x100000 hands over to
/// fn_8004BA74 and 0x100 to fn_8004B568.
pub fn ground_move(
    stack: &mut MatrixStack,
    p: &mut GroundPlayer,
    phys: &PlayerParameters,
    input: &MoveInput,
    path: Option<&Path>,
) {
    let steer = &input.steer;
    if p.surface & 0x10_0000 != 0 {
        return accelerate_steered(stack, p, phys, steer, path);
    }
    if p.surface & 0x100 != 0 {
        return accelerate(stack, p, phys, steer.gravity, steer.analog);
    }
    let mut lift = ZERO;
    let acc = [0, 1, 2].map(|k| fmadds(steer.gravity[k], phys.weight, p.base[k]));
    let mut acc = unrotate_by_angles(stack, p.angles, acc);
    let analog = get_analog(stack, p.analog(), steer.analog, true, true);
    let active = analog.active;
    let side = outer_product(p.normal, p.motion);

    // Extra gravity on steep ground, extra grip on walls.
    let (sx, up, w) = (p.speed[0], p.up, phys.weight);
    if up < FLAT && side[1].abs() > BANKED && sx > CLIMB_SPEED {
        acc = [ZERO; 3];
        acc[1] -= w;
    } else if let Some(k) = (0..3).find(|&k| up < STEEP[k] && sx > CLIMB_SPEED) {
        acc[1] = fnmsubs(STEEP_WEIGHT[k], w, acc[1]);
    } else if up < WALL && sx < phys.run_speed && sx > -phys.run_speed {
        acc[0] *= WALL_GRAVITY;
        acc[2] *= WALL_GRAVITY;
    } else if up < SIDE && sx < phys.run_speed && sx > -phys.run_speed {
        acc[2] *= SIDE_GRAVITY;
    } else if up < SLANT && sx < phys.jog_speed && sx > -phys.run_speed {
        acc[2] *= SLANT_GRAVITY;
    }

    let sx = p.speed[0];
    if !active && sx > phys.run_speed {
        acc[0] = fmadds(phys.air_resist, sx, acc[0]);
    } else if active && sx > phys.max_x_spd && p.up > UPRIGHT {
        acc[0] = fmadds(UPRIGHT_DRAG * phys.air_resist, sx - phys.max_x_spd, acc[0]);
    } else if sx > phys.max_x_spd {
        acc[0] = fmadds(phys.air_resist, sx - phys.max_x_spd, acc[0]);
    } else if sx < ZERO {
        acc[0] = fmadds(phys.air_resist, sx, acc[0]);
    }
    acc[1] = fmadds(phys.air_resist_y, p.speed[1], acc[1]);
    acc[2] = fmadds(phys.air_resist_z, p.speed[2], acc[2]);

    let push = if !active {
        let mut push = ZERO;
        if p.up < TILTED {
            let mut s = p.slope(phys);
            slope_turn(
                stack,
                &mut s,
                steer.gravity,
                steer.stage,
                p.turn(),
                steer.analog,
            );
            (p.angles, p.speed, p.aim) = (s.angles, s.speed, s.aim);
        } else if p.speed[0] > ZERO {
            push = phys.slow_down;
        } else if p.speed[0] < ZERO {
            push = -phys.slow_down;
        }
        p.push = ZERO;
        push
    } else {
        let mut ang = analog.angle.unwrap_or_default();
        let mag = analog.magnitude.unwrap_or_default();
        if p.status & 0x2000 != 0 {
            p.follow(stack, phys, path, steer.analog.magnitude);
            ang = p.aim;
            lift = if p.status & 0x2000 != 0 {
                ZERO - PATH_LIFT
            } else {
                ZERO
            };
        } else if p.up < STRONG_SLOPE && p.surface & 0x8_0000 != 0 {
            let mut s = p.slope(phys);
            slope_turn_strong(stack, &mut s, steer.gravity);
            (p.angles, p.speed, p.aim) = (s.angles, s.speed, s.aim);
        }
        let sx = p.speed[0];
        let mut mag = push_below_max(sx, mag, phys).unwrap_or({
            if sx >= phys.max_x_spd && p.up < ZERO {
                mag * phys.run_accel
            } else {
                mag * (RUN_ACCEL_FAST * phys.run_accel)
            }
        });
        ang = ang.wrapping_sub(along_wall(stack, p, ang));

        let d = diff_angle(p.angles[1], ang);
        let sx = p.speed[0];
        let jog = phys.jog_speed;
        let walker = p.character == 6 || p.character == 7;
        p.aim = ang;
        let toward = |stack: &mut MatrixStack, p: &mut GroundPlayer| {
            turn_toward(
                stack,
                &mut p.angles,
                &mut p.speed,
                phys.dash_speed,
                p.up,
                ang,
            )
        };
        let smooth = |stack: &mut MatrixStack, p: &mut GroundPlayer| {
            let turn = p.turn();
            turn_smoothly(stack, &mut p.angles, &mut p.speed, turn, steer.analog, ang)
        };
        let sharp = walker && input.walker_sharp == 1;
        if sx == ZERO && d > 0x1000 {
            if walker {
                toward(stack, p);
            } else {
                mag = ZERO;
                p.pivot(stack, ang);
            }
        } else if sx >= HALF * (jog + phys.run_speed) && d > 0x1000 {
            mag = phys.slow_down;
            if walker {
                toward(stack, p);
            } else {
                smooth(stack, p);
            }
        } else if (sx >= jog && d < 0x1000) || (sx >= phys.dash_speed && p.surface & 0x8_0000 != 0)
        {
            toward(stack, p);
        } else {
            if sx >= jog && sx <= phys.rush_speed && d > 0x2000 {
                mag *= SHARP_TURN_ACCEL;
            }
            if sharp {
                toward(stack, p);
            } else {
                smooth(stack, p);
            }
        }
        p.push = mag;
        mag
    };

    let sx = p.speed[0];
    if p.up < TILTED && sx < phys.jog_speed && sx > -phys.jog_speed && !active {
        acc[0] *= TIPPED_DAMP;
        acc[2] *= TIPPED_DAMP;
    } else {
        settle_x(p, phys, &mut acc, push, active);
    }
    settle_z(p, phys, &mut acc);
    acc[1] += lift;
    p.accel = acc;
}

/// How far to turn the input yaw `ang` so the player runs along a wall it is
/// pushing into: the angle between the stick's direction and the line where
/// the planes normal to player work +0x70 and +0x7C meet.
fn along_wall(stack: &mut MatrixStack, p: &GroundPlayer, ang: i32) -> i32 {
    let len = scalor(p.wall);
    if len <= ZERO || len.is_nan() {
        return 0;
    }
    let dir = rotate_by_angles(stack, [p.angles[0], ang, p.angles[2]], [1.0, ZERO, ZERO]);
    let dot = inner_product(dir, p.wall);
    if dot >= ZERO || dot.is_nan() {
        return 0;
    }
    let mut line = Line {
        point: [ZERO; 3],
        dir: [ZERO; 3],
    };
    plane_intersection(
        &Plane {
            point: [ZERO; 3],
            normal: p.wall,
        },
        &Plane {
            point: [ZERO; 3],
            normal: p.down,
        },
        Some(&mut line),
    );
    if inner_product(line.dir, dir) < ZERO {
        line.dir = line.dir.map(|v| -v);
    }
    vector_angle(dir, line.dir, Some(p.down))
}
