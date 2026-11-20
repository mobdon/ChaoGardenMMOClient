//! A stand-in controller for Sonic and Shadow on speed stages.
//!
//! **This is not a port of the native controller.** The Steam executable's
//! x86 text is encrypted on disk, and no GameCube main.dol (where the player
//! code lives on that platform) is installed, so no build-matched player code
//! is available to port or to compare against. What is native here is the
//! input data: every tuning number comes from the character's PhysicsArray
//! row (`player_physics::read`). How those numbers are combined below is an
//! adaptation that follows the parameter names. docs/SPEED-CONTROLLER.md lists
//! each rule and where a verified port would replace it.
//!
//! Units are native: distances in stage units and speeds in units per 60 Hz
//! frame.
use crate::{
    play_world::{add, dot, len, scale, sub, Ground, V3},
    player_physics::PlayerParameters,
};
use serde::Serialize;

/// Largest step the body climbs without jumping (matches the walker session).
pub const STEP_UP: f32 = 12.0;
/// Adaptation: homing attack search radius and the cone in front of Sonic.
pub const HOMING_RANGE: f32 = 220.0;
pub const HOMING_CONE_COS: f32 = 0.35;
/// Adaptation: frames a homing attack may fly before it gives up.
pub const HOMING_FRAMES: i32 = 60;
/// Adaptation: contact distance for a homing hit, added to the body radius.
pub const HOMING_HIT: f32 = 10.0;
/// Adaptation: rail capture distances from the feet to the rail line.
pub const RAIL_CAPTURE_XZ: f32 = 7.0;
/// Feet may be up to this far above the rail line ...
pub const RAIL_CAPTURE_Y: f32 = 9.0;
/// ... or this far below it.
pub const RAIL_CAPTURE_BELOW: f32 = 2.0;
/// Adaptation: share of gravity that acts along a rail.
pub const RAIL_GRAVITY: f32 = 0.5;
/// Adaptation: frames after leaving a rail before any rail can capture again.
pub const RAIL_COOLDOWN: i32 = 20;
/// Adaptation: frames the spin dash needs to reach full charge.
pub const SPIN_CHARGE_FRAMES: i32 = 45;

/// Tuning from one PhysicsArray row (values native, see the module note).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeedPhysics {
    pub slot: usize,
    pub jump2_timer: i32,
    pub lim_h_spd: f32,
    pub lim_v_spd: f32,
    pub max_x_spd: f32,
    pub jmp_y_spd: f32,
    pub jog_speed: f32,
    pub run_speed: f32,
    pub rush_speed: f32,
    pub dash_speed: f32,
    pub jmp_addit: f32,
    pub run_accel: f32,
    pub air_accel: f32,
    pub slow_down: f32,
    pub run_break: f32,
    pub air_break: f32,
    pub air_resist: f32,
    pub air_resist_y: f32,
    pub grd_frict: f32,
    pub rad: f32,
    pub height: f32,
    pub weight: f32,
    pub center_height: f32,
}

impl SpeedPhysics {
    pub fn from_row(slot: usize, p: &PlayerParameters) -> Self {
        Self {
            slot,
            jump2_timer: p.jump2_timer,
            lim_h_spd: p.lim_h_spd,
            lim_v_spd: p.lim_v_spd,
            max_x_spd: p.max_x_spd,
            jmp_y_spd: p.jmp_y_spd,
            jog_speed: p.jog_speed,
            run_speed: p.run_speed,
            rush_speed: p.rush_speed,
            dash_speed: p.dash_speed,
            jmp_addit: p.jmp_addit,
            run_accel: p.run_accel,
            air_accel: p.air_accel,
            slow_down: p.slow_down,
            run_break: p.run_break,
            air_break: p.air_break,
            air_resist: p.air_resist,
            air_resist_y: p.air_resist_y,
            grd_frict: p.grd_frict,
            rad: p.rad,
            height: p.height,
            weight: p.weight,
            center_height: p.center_height,
        }
    }

    /// Peak height of a held jump from flat ground (as `step` integrates it).
    pub fn jump_height(&self) -> f32 {
        let (mut vy, mut h, mut hold) = (self.jmp_y_spd, 0.0, self.jump2_timer);
        while vy > 0.0 {
            h += vy;
            if hold > 0 {
                vy += self.jmp_addit;
                hold -= 1;
            }
            vy = vy - self.weight + vy * self.air_resist_y;
        }
        h
    }
}

/// Buttons are held states; presses are derived from the previous frame.
#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Input {
    /// Stick, x right and y forward, each in -1..=1, relative to `camera_yaw`.
    pub stick: [f32; 2],
    /// Camera heading in radians about +Y (0 looks toward -Z).
    pub camera_yaw: f32,
    /// A: jump, and in the air the homing attack.
    pub jump: bool,
    /// B: spin (moving) or spin dash charge (standing).
    pub action: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum Mode {
    Ground,
    /// Rolling ball on the ground.
    Spin,
    /// Standing spin dash charge, in frames.
    Charge {
        frames: i32,
    },
    Air,
    /// Flying at a homing target (index into the caller's target list).
    Homing {
        target: usize,
        frames: i32,
    },
    /// On rail `rail` at arc length `s`, moving `dir` (+1/-1) along it.
    Rail {
        rail: usize,
        s: f32,
        dir: f32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Action {
    Jump,
    SpinStart,
    SpinDash { speed: f32 },
    HomingStart { target: usize },
    HomingHit { target: usize },
    AirDash,
    RailOn { rail: usize },
    RailOff { rail: usize },
    Landed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeedPlayer {
    /// Feet position.
    pub position: V3,
    pub velocity: V3,
    /// Horizontal facing (unit length).
    pub facing: V3,
    pub mode: Mode,
    pub ground_normal: V3,
    /// Ball form in the air (after a jump, spring roll or homing hit).
    pub ball: bool,
    pub jump_hold: i32,
    /// Frames of ignored stick and buttons (dash panels, springs).
    pub nocon: i32,
    air_action_used: bool,
    rail_cooldown: i32,
    prev_jump: bool,
    prev_action: bool,
}

impl SpeedPlayer {
    pub fn new(position: V3, facing: V3) -> Self {
        Self {
            position,
            velocity: [0.0; 3],
            facing: flat_unit(facing).unwrap_or([0.0, 0.0, -1.0]),
            mode: Mode::Air,
            ground_normal: [0.0, 1.0, 0.0],
            ball: false,
            jump_hold: 0,
            nocon: 0,
            air_action_used: false,
            rail_cooldown: 0,
            prev_jump: false,
            prev_action: false,
        }
    }

    pub fn grounded(&self) -> bool {
        matches!(self.mode, Mode::Ground | Mode::Spin | Mode::Charge { .. })
    }

    /// True while the body damages what it touches (ball, homing, charge).
    pub fn attacking(&self) -> bool {
        self.ball
            || matches!(
                self.mode,
                Mode::Spin | Mode::Charge { .. } | Mode::Homing { .. }
            )
    }

    pub fn on_rail(&self) -> Option<usize> {
        match self.mode {
            Mode::Rail { rail, .. } => Some(rail),
            _ => None,
        }
    }

    pub fn speed(&self) -> f32 {
        len(self.velocity)
    }

    /// Put the player on the ground facing `facing` with forward speed `speed`
    /// for `nocon` frames (dash panel).
    pub fn boost(&mut self, facing: V3, speed: f32, nocon: i32) {
        if let Some(f) = flat_unit(facing) {
            self.facing = f;
        }
        let sp = speed.max(len(self.velocity));
        if self.grounded() {
            self.mode = Mode::Ground;
            self.velocity = scale(tangent(self.facing, self.ground_normal), sp);
        } else {
            let vy = self.velocity[1];
            self.velocity = scale(self.facing, sp);
            self.velocity[1] = vy;
        }
        self.nocon = self.nocon.max(nocon);
    }

    /// An external launch (spring): leave the ground or rail with `velocity`.
    pub fn launch(&mut self, velocity: V3, nocon: i32) {
        self.velocity = velocity;
        self.mode = Mode::Air;
        self.ball = false;
        self.jump_hold = 0;
        self.air_action_used = false;
        self.nocon = nocon;
        if let Some(f) = flat_unit(velocity) {
            self.facing = f;
        }
    }

    /// The bounce after a homing or ball hit.
    pub fn bounce(&mut self, ph: &SpeedPhysics) {
        let h = [self.velocity[0], 0.0, self.velocity[2]];
        let keep = len(h).min(ph.jog_speed);
        self.velocity = add(scale(self.facing, keep), [0.0, ph.jmp_y_spd * 1.2, 0.0]);
        self.mode = Mode::Air;
        self.ball = true;
        self.jump_hold = 0;
        self.air_action_used = false;
    }
}

/// Collision the controller needs from its host.
pub trait Terrain {
    /// Highest walkable surface at or below `p` and above `p.y - depth`.
    fn ground(&self, p: V3, depth: f32) -> Option<Ground>;
    /// Lowest surface above `p.y` within `reach`.
    fn ceiling(&self, p: V3, reach: f32) -> Option<f32>;
    /// Push a sphere out of walls: (correction, contact normal).
    fn push_out(&self, center: V3, radius: f32) -> (V3, Option<V3>);
}

/// A grind rail: a native stage path polyline with cumulative arc lengths.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rail {
    pub points: Vec<V3>,
    pub arc: Vec<f32>,
}

impl Rail {
    pub fn new(points: Vec<V3>) -> Self {
        let mut arc = vec![0.0];
        for w in points.windows(2) {
            let l = len(sub(w[1], w[0]));
            arc.push(arc.last().copied().unwrap_or(0.0) + l);
        }
        Self { points, arc }
    }

    pub fn length(&self) -> f32 {
        self.arc.last().copied().unwrap_or(0.0)
    }

    /// Position and unit tangent at arc length `s` (clamped).
    pub fn at(&self, s: f32) -> (V3, V3) {
        let s = s.clamp(0.0, self.length());
        let i = match self.arc.iter().position(|&a| a > s) {
            Some(0) => 0,
            Some(i) => i - 1,
            None => self.points.len().saturating_sub(2),
        };
        let (a, b) = (self.points[i], self.points[i + 1]);
        let seg = (self.arc[i + 1] - self.arc[i]).max(1e-4);
        let t = ((s - self.arc[i]) / seg).clamp(0.0, 1.0);
        let d = sub(b, a);
        (add(a, scale(d, t)), scale(d, 1.0 / len(d).max(1e-4)))
    }

    /// Closest point on the rail to `p`: (arc length, point).
    pub fn closest(&self, p: V3) -> (f32, V3) {
        let mut best = (f32::MAX, 0.0, self.points[0]);
        for (i, w) in self.points.windows(2).enumerate() {
            let d = sub(w[1], w[0]);
            let l2 = dot(d, d).max(1e-6);
            let t = (dot(sub(p, w[0]), d) / l2).clamp(0.0, 1.0);
            let q = add(w[0], scale(d, t));
            let dist = len(sub(p, q));
            if dist < best.0 {
                best = (dist, self.arc[i] + t * l2.sqrt(), q);
            }
        }
        (best.1, best.2)
    }
}

/// What the controller may home in on this frame.
#[derive(Debug, Clone, Copy)]
pub struct Target {
    pub position: V3,
    pub alive: bool,
}

pub fn flat_unit(v: V3) -> Option<V3> {
    let h = [v[0], 0.0, v[2]];
    let l = len(h);
    (l > 1e-4).then(|| scale(h, 1.0 / l))
}

/// The facing direction projected onto the ground plane (unit length).
pub fn tangent(facing: V3, normal: V3) -> V3 {
    let t = sub(facing, scale(normal, dot(facing, normal)));
    let l = len(t);
    if l > 1e-4 {
        scale(t, 1.0 / l)
    } else {
        facing
    }
}

fn rotate_toward(from: V3, to: V3, max_angle: f32) -> V3 {
    let a = from[2].atan2(from[0]);
    let b = to[2].atan2(to[0]);
    let mut d = b - a;
    while d > std::f32::consts::PI {
        d -= std::f32::consts::TAU;
    }
    while d < -std::f32::consts::PI {
        d += std::f32::consts::TAU;
    }
    let r = a + d.clamp(-max_angle, max_angle);
    [r.cos(), 0.0, r.sin()]
}

/// The world direction and magnitude the stick asks for.
pub fn stick_world(i: &Input) -> (V3, f32) {
    let mag = i.stick[0].hypot(i.stick[1]).min(1.0);
    let (s, c) = i.camera_yaw.sin_cos();
    let fwd = [-s, 0.0, -c];
    let right = [c, 0.0, -s];
    let d = add(scale(right, i.stick[0]), scale(fwd, i.stick[1]));
    match flat_unit(d) {
        Some(u) if mag > 0.05 => (u, mag),
        _ => ([0.0; 3], 0.0),
    }
}

/// Advance one 60 Hz frame. `targets` are homing candidates; `rails` are the
/// stage's grind paths.
pub fn step(
    ph: &SpeedPhysics,
    pl: &mut SpeedPlayer,
    input: Input,
    terrain: &impl Terrain,
    rails: &[Rail],
    targets: &[Target],
) -> Vec<Action> {
    let mut out = vec![];
    let input = if pl.nocon > 0 {
        pl.nocon -= 1;
        Input {
            stick: [0.0; 2],
            jump: false,
            action: false,
            ..input
        }
    } else {
        input
    };
    let jump_press = input.jump && !pl.prev_jump;
    let action_press = input.action && !pl.prev_action;
    pl.prev_jump = input.jump;
    pl.prev_action = input.action;
    pl.rail_cooldown = (pl.rail_cooldown - 1).max(0);
    let (want, mag) = stick_world(&input);

    match pl.mode {
        Mode::Rail { rail, s, dir } => {
            grind(ph, pl, rails, rail, s, dir, jump_press, &mut out);
            if pl.on_rail().is_some() {
                return out;
            }
        }
        Mode::Homing { target, frames } => {
            let t = targets.get(target).filter(|t| t.alive);
            match t {
                Some(t) if frames > 0 => {
                    let aim = t.position;
                    let center = add(pl.position, [0.0, ph.center_height, 0.0]);
                    let d = sub(aim, center);
                    let dl = len(d).max(1e-4);
                    if dl < ph.rad + HOMING_HIT {
                        out.push(Action::HomingHit { target });
                        pl.bounce(ph);
                    } else {
                        pl.velocity = scale(d, ph.dash_speed / dl);
                        if let Some(f) = flat_unit(d) {
                            pl.facing = f;
                        }
                        pl.mode = Mode::Homing {
                            target,
                            frames: frames - 1,
                        };
                    }
                }
                _ => {
                    pl.mode = Mode::Air;
                    pl.ball = true;
                }
            }
        }
        Mode::Ground | Mode::Spin | Mode::Charge { .. } => {
            ground(
                ph,
                pl,
                want,
                mag,
                jump_press,
                input.action,
                action_press,
                &mut out,
            );
        }
        Mode::Air => air(ph, pl, want, mag, input.jump, jump_press, targets, &mut out),
    }
    integrate(ph, pl, terrain, &mut out);
    if pl.rail_cooldown == 0 && pl.on_rail().is_none() && pl.velocity[1] <= 0.5 {
        capture_rail(pl, rails, &mut out);
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn ground(
    ph: &SpeedPhysics,
    pl: &mut SpeedPlayer,
    want: V3,
    mag: f32,
    jump_press: bool,
    action_held: bool,
    action_press: bool,
    out: &mut Vec<Action>,
) {
    let n = pl.ground_normal;
    // Signed speed along the facing on the ground plane.
    let mut spd = dot(pl.velocity, tangent(pl.facing, n));
    // Gravity along the slope (weight times the slope's sine, signed by facing).
    let downhill = [n[0], 0.0, n[2]];
    let slope = ph.weight * dot(pl.facing, downhill);

    if let Mode::Charge { frames } = pl.mode {
        if action_held {
            if mag > 0.0 {
                pl.facing = want;
            }
            pl.mode = Mode::Charge {
                frames: (frames + 1).min(SPIN_CHARGE_FRAMES),
            };
            pl.velocity = [0.0; 3];
            return;
        }
        // Release: speed rises linearly with the charge up to dash_speed.
        let speed = ph.dash_speed * (frames as f32 / SPIN_CHARGE_FRAMES as f32).max(0.3);
        pl.mode = Mode::Spin;
        pl.velocity = scale(tangent(pl.facing, n), speed);
        out.push(Action::SpinDash { speed });
        return;
    }

    if jump_press {
        pl.velocity = add(
            scale(tangent(pl.facing, n), spd.max(0.0)),
            scale(n, ph.jmp_y_spd),
        );
        pl.mode = Mode::Air;
        pl.ball = true;
        pl.jump_hold = ph.jump2_timer;
        pl.air_action_used = false;
        out.push(Action::Jump);
        return;
    }

    if action_press && pl.mode == Mode::Ground {
        if spd < ph.jog_speed {
            pl.mode = Mode::Charge { frames: 0 };
            pl.velocity = [0.0; 3];
            return;
        }
        pl.mode = Mode::Spin;
        out.push(Action::SpinStart);
    }

    if pl.mode == Mode::Spin {
        // A ball keeps its speed: drag, a tenth of slow_down, and slopes.
        if mag > 0.0 {
            pl.facing = rotate_toward(pl.facing, want, 0.03);
        }
        spd += slope + spd * ph.air_resist + ph.slow_down * 0.1;
        if spd < ph.jog_speed * 0.5 {
            pl.mode = Mode::Ground;
        }
    } else if mag > 0.0 {
        let along = dot(pl.facing, want);
        if along < -0.5 && spd > ph.run_speed {
            // Stick held against the motion: brake.
            spd += ph.run_break;
        } else {
            // Turn rate falls with speed.
            let turn = (0.35 - 0.035 * spd.max(0.0)).max(0.07);
            pl.facing = rotate_toward(pl.facing, want, turn);
            if spd < ph.max_x_spd * mag {
                let accel = if spd < ph.rush_speed {
                    ph.run_accel
                } else {
                    ph.run_accel * 0.5
                };
                spd = (spd + accel * mag).min(ph.max_x_spd * mag).max(spd);
            }
        }
        spd += slope + spd * ph.air_resist;
    } else {
        // No stick: slow_down until stopped; slopes gentler than that hold.
        if slope.abs() > -ph.slow_down {
            spd += slope;
        }
        spd += spd * ph.air_resist;
        spd = if spd > 0.0 {
            (spd + ph.slow_down).max(0.0)
        } else {
            (spd - ph.slow_down).min(0.0)
        };
    }
    if spd < 0.0 {
        // Rolled back past zero: turn around.
        pl.facing = scale(pl.facing, -1.0);
        spd = -spd;
    }
    spd = spd.min(ph.lim_h_spd);
    pl.velocity = scale(tangent(pl.facing, n), spd);
}

#[allow(clippy::too_many_arguments)]
fn air(
    ph: &SpeedPhysics,
    pl: &mut SpeedPlayer,
    want: V3,
    mag: f32,
    jump_held: bool,
    jump_press: bool,
    targets: &[Target],
    out: &mut Vec<Action>,
) {
    if jump_press && pl.ball && !pl.air_action_used {
        pl.air_action_used = true;
        pl.jump_hold = 0;
        let center = add(pl.position, [0.0, ph.center_height, 0.0]);
        let mut best: Option<(f32, usize)> = None;
        for (i, t) in targets.iter().enumerate().filter(|(_, t)| t.alive) {
            let d = sub(t.position, center);
            let dl = len(d);
            let Some(h) = flat_unit(d) else { continue };
            if dl > HOMING_RANGE || dot(h, pl.facing) < HOMING_CONE_COS || d[1] > 60.0 {
                continue;
            }
            if best.is_none_or(|(b, _)| dl < b) {
                best = Some((dl, i));
            }
        }
        if let Some((_, target)) = best {
            pl.mode = Mode::Homing {
                target,
                frames: HOMING_FRAMES,
            };
            out.push(Action::HomingStart { target });
            return;
        }
        // No target: air dash forward.
        let h = len([pl.velocity[0], 0.0, pl.velocity[2]]).max(ph.dash_speed);
        pl.velocity = [pl.facing[0] * h, pl.velocity[1].max(0.0), pl.facing[2] * h];
        out.push(Action::AirDash);
        return;
    }
    let mut h = [pl.velocity[0], 0.0, pl.velocity[2]];
    if mag > 0.0 {
        let hs = len(h);
        if hs > 0.1 && dot(h, want) < -0.5 * hs {
            let u = scale(h, 1.0 / hs);
            h = scale(u, (hs + ph.air_break).max(0.0));
        } else {
            let nh = add(h, scale(want, ph.air_accel * mag));
            // Steering may turn but not add speed beyond the larger of the old
            // speed and max_x_spd.
            let cap = hs.max(ph.max_x_spd);
            let nl = len(nh);
            h = if nl > cap { scale(nh, cap / nl) } else { nh };
        }
        if let Some(f) = flat_unit(h) {
            pl.facing = rotate_toward(pl.facing, f, 0.2);
        }
    }
    h = add(h, scale(h, ph.air_resist));
    let mut vy = pl.velocity[1];
    if jump_held && pl.jump_hold > 0 && vy > 0.0 {
        vy += ph.jmp_addit;
        pl.jump_hold -= 1;
    } else {
        pl.jump_hold = 0;
    }
    vy = (vy - ph.weight + vy * ph.air_resist_y).max(-ph.lim_v_spd);
    pl.velocity = [h[0], vy, h[2]];
}

#[allow(clippy::too_many_arguments)]
fn grind(
    ph: &SpeedPhysics,
    pl: &mut SpeedPlayer,
    rails: &[Rail],
    rail: usize,
    s: f32,
    dir: f32,
    jump_press: bool,
    out: &mut Vec<Action>,
) {
    let r = &rails[rail];
    let (_, t) = r.at(s);
    let t = scale(t, dir);
    let mut spd = len(pl.velocity);
    // Gravity along the rail plus drag; a stalled grinder turns back.
    spd += -ph.weight * RAIL_GRAVITY * t[1] + spd * ph.air_resist;
    let mut dir = dir;
    if spd < 0.0 {
        dir = -dir;
        spd = -spd;
    }
    spd = spd.min(ph.lim_h_spd);
    let ns = s + dir * spd;
    let (p, nt) = r.at(ns);
    let nt = scale(nt, dir);
    if let Some(f) = flat_unit(nt) {
        pl.facing = f;
    }
    if jump_press {
        pl.position = p;
        pl.launch(add(scale(nt, spd), [0.0, ph.jmp_y_spd, 0.0]), 0);
        pl.ball = true;
        pl.jump_hold = ph.jump2_timer;
        pl.rail_cooldown = RAIL_COOLDOWN;
        out.push(Action::Jump);
        out.push(Action::RailOff { rail });
        return;
    }
    if ns <= 0.0 || ns >= r.length() {
        pl.position = p;
        pl.launch(scale(nt, spd), 0);
        pl.rail_cooldown = RAIL_COOLDOWN;
        out.push(Action::RailOff { rail });
        return;
    }
    pl.position = p;
    pl.velocity = scale(nt, spd);
    pl.mode = Mode::Rail { rail, s: ns, dir };
}

fn capture_rail(pl: &mut SpeedPlayer, rails: &[Rail], out: &mut Vec<Action>) {
    for (i, r) in rails.iter().enumerate() {
        let (s, q) = r.closest(pl.position);
        let d = sub(pl.position, q);
        if d[0].hypot(d[2]) > RAIL_CAPTURE_XZ
            || !(-RAIL_CAPTURE_BELOW..=RAIL_CAPTURE_Y).contains(&d[1])
        {
            continue;
        }
        let (_, t) = r.at(s);
        // Momentum along the rail carries over; a body arriving across it
        // keeps its speed in the nearer direction.
        let along = dot(pl.velocity, t);
        let dir = if along >= 0.0 { 1.0 } else { -1.0 };
        let spd = along.abs().max(len(pl.velocity) * 0.5).max(1.0);
        pl.position = q;
        pl.velocity = scale(t, spd * dir);
        pl.mode = Mode::Rail { rail: i, s, dir };
        pl.ball = false;
        pl.air_action_used = false;
        out.push(Action::RailOn { rail: i });
        return;
    }
}

/// Move with collision. Ground modes follow the surface; air modes fall.
fn integrate(
    ph: &SpeedPhysics,
    pl: &mut SpeedPlayer,
    terrain: &impl Terrain,
    out: &mut Vec<Action>,
) {
    if pl.on_rail().is_some() {
        return;
    }
    let was_grounded = pl.grounded();
    let mut pos = pl.position;
    let mut vel = pl.velocity;
    let steps = ((len(vel) / (ph.rad * 0.5)).ceil() as usize).clamp(1, 32);
    let f = 1.0 / steps as f32;
    let mut grounded = false;
    let mut normal = pl.ground_normal;
    for _ in 0..steps {
        let horizontal = [vel[0] * f, 0.0, vel[2] * f];
        pos[0] += horizontal[0];
        pos[2] += horizontal[2];
        // Walls: sphere above the step height. Hitting one removes the
        // velocity into it.
        let center = [pos[0], pos[1] + STEP_UP * 0.5 + ph.rad, pos[2]];
        let (d, wall) = terrain.push_out(center, ph.rad);
        pos[0] += d[0];
        pos[2] += d[2];
        if let Some(w) = wall {
            let into = dot(vel, w);
            if into < 0.0 {
                vel = sub(vel, scale(w, into));
            }
        }
        let dy = vel[1] * f;
        if dy > 0.0 && !(was_grounded && grounded) {
            let top = pos[1] + ph.height;
            if let Some(ceil) = terrain.ceiling([pos[0], top - 1.0, pos[2]], dy + 1.0) {
                pos[1] = ceil - ph.height - 0.01;
                vel[1] = 0.0;
                continue;
            }
        }
        // Stay on the ground over crests and down slopes: the snap reaches as
        // far as a 55-degree slope drops over this sub-step.
        let hmove = len(horizontal);
        let snap = if was_grounded {
            STEP_UP.min(4.0) + hmove * 1.5
        } else {
            0.0
        };
        let probe_top = pos[1] + STEP_UP.max(dy);
        let depth = STEP_UP + snap + (-dy).max(0.0);
        match terrain.ground([pos[0], probe_top, pos[2]], depth) {
            Some(g) if dy <= 0.5 && g.y >= pos[1] + dy.min(0.0) - snap - 0.01 => {
                pos[1] = g.y;
                grounded = true;
                normal = g.normal;
            }
            _ => {
                pos[1] += dy;
                grounded = false;
            }
        }
    }
    pl.position = pos;
    if grounded {
        if !was_grounded {
            // Landing keeps the momentum that lies along the ground.
            let t = sub(vel, scale(normal, dot(vel, normal)));
            let along = len(t);
            if let Some(f) = flat_unit(t) {
                if along > 0.2 {
                    pl.facing = f;
                }
            }
            pl.velocity = scale(tangent(pl.facing, normal), along);
            pl.mode = if pl.ball && along > ph.run_speed && matches!(pl.mode, Mode::Spin) {
                Mode::Spin
            } else {
                Mode::Ground
            };
            pl.ball = false;
            pl.jump_hold = 0;
            pl.air_action_used = false;
            out.push(Action::Landed);
        } else {
            // Re-project onto the new surface so speed is kept over crests.
            let sp = len(vel);
            pl.velocity = scale(tangent(pl.facing, normal), sp);
        }
        pl.ground_normal = normal;
    } else {
        pl.velocity = vel;
        if was_grounded {
            pl.mode = Mode::Air;
            pl.air_action_used = true;
        } else if matches!(pl.mode, Mode::Homing { .. }) {
            // stays homing
        } else {
            pl.mode = Mode::Air;
        }
        pl.ground_normal = [0.0, 1.0, 0.0];
    }
}

/// Flat test world and helpers for unit tests.
#[cfg(test)]
mod tests {
    use super::*;

    struct Flat(f32);
    impl Terrain for Flat {
        fn ground(&self, p: V3, depth: f32) -> Option<Ground> {
            (p[1] >= self.0 && p[1] - depth <= self.0).then_some(Ground {
                y: self.0,
                normal: [0.0, 1.0, 0.0],
                tri: 0,
            })
        }
        fn ceiling(&self, _: V3, _: f32) -> Option<f32> {
            None
        }
        fn push_out(&self, _: V3, _: f32) -> (V3, Option<V3>) {
            ([0.0; 3], None)
        }
    }

    /// A plane through the origin tilted down toward +X by `angle`.
    struct Slope(f32);
    impl Terrain for Slope {
        fn ground(&self, p: V3, depth: f32) -> Option<Ground> {
            let y = -p[0] * self.0.tan();
            let n = [self.0.sin(), self.0.cos(), 0.0];
            (p[1] >= y - 0.01 && p[1] - depth <= y).then_some(Ground {
                y,
                normal: n,
                tri: 0,
            })
        }
        fn ceiling(&self, _: V3, _: f32) -> Option<f32> {
            None
        }
        fn push_out(&self, _: V3, _: f32) -> (V3, Option<V3>) {
            ([0.0; 3], None)
        }
    }

    fn sonic() -> SpeedPhysics {
        // Sonic's PhysicsArray row 0 on the pinned Steam build.
        SpeedPhysics {
            slot: 0,
            jump2_timer: 60,
            lim_h_spd: 16.0,
            lim_v_spd: 16.0,
            max_x_spd: 3.0,
            jmp_y_spd: 1.66,
            jog_speed: 0.46,
            run_speed: 1.39,
            rush_speed: 2.3,
            dash_speed: 5.09,
            jmp_addit: 0.076,
            run_accel: 0.05,
            air_accel: 0.031,
            slow_down: -0.06,
            run_break: -0.18,
            air_break: -0.17,
            air_resist: -0.008,
            air_resist_y: -0.01,
            grd_frict: -0.1,
            rad: 4.0,
            height: 10.0,
            weight: 0.08,
            center_height: 5.4,
        }
    }

    fn run(ph: &SpeedPhysics, pl: &mut SpeedPlayer, t: &impl Terrain, i: Input, n: usize) {
        for _ in 0..n {
            step(ph, pl, i, t, &[], &[]);
        }
    }

    const FORWARD: Input = Input {
        stick: [0.0, 1.0],
        camera_yaw: 0.0,
        jump: false,
        action: false,
    };

    #[test]
    fn runs_up_to_max_x_spd_on_flat_ground() {
        let ph = sonic();
        let mut pl = SpeedPlayer::new([0.0, 0.0, 0.0], [0.0, 0.0, -1.0]);
        run(&ph, &mut pl, &Flat(0.0), Input::default(), 2);
        assert!(pl.grounded());
        run(&ph, &mut pl, &Flat(0.0), FORWARD, 600);
        let v = pl.speed();
        assert!(v > 2.5 && v <= ph.max_x_spd, "{v}");
        assert!(pl.position[2] < -1000.0);
        // Releasing the stick stops within a few seconds.
        run(&ph, &mut pl, &Flat(0.0), Input::default(), 120);
        assert!(pl.speed() < 1e-3);
    }

    #[test]
    fn held_jump_peaks_at_the_integrated_height() {
        let ph = sonic();
        let mut pl = SpeedPlayer::new([0.0, 0.0, 0.0], [0.0, 0.0, -1.0]);
        run(&ph, &mut pl, &Flat(0.0), Input::default(), 2);
        let jump = Input {
            jump: true,
            ..Input::default()
        };
        let mut peak: f32 = 0.0;
        for _ in 0..400 {
            step(&ph, &mut pl, jump, &Flat(0.0), &[], &[]);
            peak = peak.max(pl.position[1]);
        }
        assert!(pl.grounded());
        assert!(
            (peak - ph.jump_height()).abs() < 2.0,
            "{peak} {}",
            ph.jump_height()
        );
    }

    #[test]
    fn slopes_accelerate_a_spinning_ball_past_running_speed() {
        let ph = sonic();
        let slope = Slope(0.5);
        let mut pl = SpeedPlayer::new([0.0, 0.0, 0.0], [1.0, 0.0, 0.0]);
        run(&ph, &mut pl, &slope, Input::default(), 2);
        assert!(pl.grounded());
        let spin = Input {
            stick: [1.0, 0.0],
            action: true,
            ..Input::default()
        };
        run(
            &ph,
            &mut pl,
            &slope,
            Input {
                stick: [1.0, 0.0],
                ..Input::default()
            },
            60,
        );
        run(&ph, &mut pl, &slope, spin, 1);
        assert_eq!(pl.mode, Mode::Spin);
        let hold = Input {
            stick: [1.0, 0.0],
            action: true,
            ..Input::default()
        };
        run(&ph, &mut pl, &slope, hold, 240);
        assert!(pl.grounded(), "{:?}", pl.mode);
        assert!(pl.speed() > ph.max_x_spd + 0.5, "{}", pl.speed());
    }

    #[test]
    fn spin_dash_releases_at_dash_speed() {
        let ph = sonic();
        let mut pl = SpeedPlayer::new([0.0, 0.0, 0.0], [0.0, 0.0, -1.0]);
        run(&ph, &mut pl, &Flat(0.0), Input::default(), 2);
        let hold = Input {
            action: true,
            ..Input::default()
        };
        run(
            &ph,
            &mut pl,
            &Flat(0.0),
            hold,
            SPIN_CHARGE_FRAMES as usize + 5,
        );
        assert!(matches!(pl.mode, Mode::Charge { .. }));
        let a = step(&ph, &mut pl, Input::default(), &Flat(0.0), &[], &[]);
        assert!(a.iter().any(
            |a| matches!(a, Action::SpinDash { speed } if (*speed - ph.dash_speed).abs() < 1e-4)
        ));
    }

    #[test]
    fn homing_attack_reaches_a_target_ahead_and_bounces() {
        let ph = sonic();
        let mut pl = SpeedPlayer::new([0.0, 0.0, 0.0], [0.0, 0.0, -1.0]);
        run(&ph, &mut pl, &Flat(-1000.0), Input::default(), 1);
        pl.ball = true;
        let targets = [Target {
            position: [0.0, -40.0, -120.0],
            alive: true,
        }];
        let press = Input {
            jump: true,
            ..Input::default()
        };
        let a = step(&ph, &mut pl, press, &Flat(-1000.0), &[], &targets);
        assert!(a.contains(&Action::HomingStart { target: 0 }));
        let mut hit = false;
        for _ in 0..HOMING_FRAMES {
            let a = step(&ph, &mut pl, press, &Flat(-1000.0), &[], &targets);
            if a.contains(&Action::HomingHit { target: 0 }) {
                hit = true;
                break;
            }
        }
        assert!(hit);
        assert!(pl.velocity[1] > 1.5);
    }

    #[test]
    fn rails_capture_a_falling_body_and_carry_it_downhill() {
        let ph = sonic();
        let rails = [Rail::new(vec![[0.0, 0.0, 0.0], [0.0, -50.0, -200.0]])];
        let mut pl = SpeedPlayer::new([0.0, 3.0, -10.0], [0.0, 0.0, -1.0]);
        pl.velocity = [0.0, -1.0, -2.0];
        let mut on = false;
        let mut off = false;
        for _ in 0..400 {
            for a in step(&ph, &mut pl, Input::default(), &Flat(-1000.0), &rails, &[]) {
                on |= a == Action::RailOn { rail: 0 };
                off |= a == Action::RailOff { rail: 0 };
            }
            if off {
                break;
            }
        }
        assert!(on && off);
        assert!(pl.position[2] < -199.0);
        assert!(pl.speed() > 1.5, "{}", pl.speed());
    }
}
