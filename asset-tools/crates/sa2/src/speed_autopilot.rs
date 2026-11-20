//! Offline autopilot for the speed session (a verification aid, not a native
//! system). It plans over the live collision with `play_nav`, adds links for
//! springs and grind rails by simulating the controller, and then only
//! produces stick and button input; the session moves the body.
use crate::{
    play_nav::{Body, NavGrid},
    play_world::{len, sub, V3},
    speed_controller::{self, Input, Mode, SpeedPlayer, STEP_UP},
    speed_session::{SpeedSession, GOAL_RADIUS},
};
use serde::Serialize;

/// Lattice spacing. City Escape spans about 11,000 by 17,000 units.
pub const SPACING: f32 = 10.0;
/// Long links may hop this high over lips and reach this many cells.
pub const HOP: f32 = 30.0;
pub const HOP_CELLS: i32 = 3;
const LAUNCH_TAG: u32 = 10_000;
const RAIL_TAG: u32 = 20_000;
/// Speed assumed when jumping onto the low end of an uphill rail.
const RUN_IN_SPEED: f32 = 3.0;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanRecord {
    pub frame: u32,
    pub from: V3,
    /// The goal was reachable from `from`.
    pub reached: bool,
    pub waypoints: usize,
    pub links: Vec<String>,
    pub closest: V3,
    pub explored: usize,
}

#[derive(Debug, Clone, Copy)]
enum Wait {
    /// Flying after a spring or rail; replan on landing.
    Flight { since: u32, left: bool },
}

pub struct SpeedAutopilot {
    pub body: Body,
    grid: Option<NavGrid>,
    route: Vec<V3>,
    tags: Vec<u32>,
    index: usize,
    wait: Option<Wait>,
    best: (f32, u32),
    jump_frames: i32,
    stuck: u32,
    pub plans: Vec<PlanRecord>,
    /// Special links found when the grid was built (for the report).
    pub links: Vec<String>,
    /// The goal was unreachable and the route ends at the closest node.
    pub partial: bool,
    pub gave_up: bool,
    pub replans: u32,
}

/// Where a body launched with `velocity` lands under the controller with no
/// input (rails capture it as they would in play).
pub fn predict_landing(s: &SpeedSession, position: V3, velocity: V3, nocon: i32) -> Option<V3> {
    let mut pl = SpeedPlayer::new(position, velocity);
    pl.launch(velocity, nocon);
    let terrain = Probe(s);
    for f in 0..1200 {
        speed_controller::step(
            &s.physics,
            &mut pl,
            Input::default(),
            &terrain,
            &s.rails,
            &[],
        );
        if pl.grounded() && f > 2 {
            return Some(pl.position);
        }
        if pl.position[1] < s.world.min[1] - 300.0 {
            return None;
        }
    }
    None
}

struct Probe<'a>(&'a SpeedSession);
impl speed_controller::Terrain for Probe<'_> {
    fn ground(&self, p: V3, depth: f32) -> Option<crate::play_world::Ground> {
        self.0.world.ground_below(p, depth)
    }
    fn ceiling(&self, p: V3, reach: f32) -> Option<f32> {
        self.0.world.ceiling_above(p, reach)
    }
    fn push_out(&self, c: V3, r: f32) -> (V3, Option<V3>) {
        self.0.world.push_out_front(c, r, 4)
    }
}

impl SpeedAutopilot {
    pub fn new(s: &SpeedSession) -> Self {
        Self {
            body: Body {
                radius: s.physics.rad,
                height: s.physics.height,
                step: STEP_UP,
                jump: s.physics.jump_height() - 2.0,
            },
            grid: None,
            route: vec![],
            tags: vec![],
            index: 0,
            wait: None,
            best: (f32::MAX, 0),
            jump_frames: 0,
            stuck: 0,
            plans: vec![],
            links: vec![],
            partial: false,
            gave_up: false,
            replans: 0,
        }
    }

    fn build(&mut self, s: &SpeedSession) {
        let mut grid = NavGrid::build(&s.world, self.body, SPACING);
        grid.set_hops(HOP, HOP_CELLS);
        // Springs: walk onto one, land where free flight ends.
        for (j, sp) in s.springs.iter().enumerate() {
            // The launch the spring would give a still player on it.
            let Some(v) = spring_velocity(s, j) else {
                continue;
            };
            let Some(landing) = predict_landing(s, sp.position, v.0, v.1) else {
                continue;
            };
            let Some(to) = grid.nearest(landing) else {
                continue;
            };
            let from = grid.nodes_near(sp.position, 10.0, 20.0);
            self.links.push(format!(
                "spring {j}: {} boarding nodes, lands at {landing:?}",
                from.len()
            ));
            for b in from {
                grid.add_link(b, to, 80.0, LAUNCH_TAG + j as u32);
            }
        }
        // Rails: board either end (run into a low end, jump to a high one)
        // and ride to where the simulated grind delivers the body.
        for (j, r) in s.rails.iter().enumerate() {
            for (from_end, dir) in [(0usize, 1.0f32), (r.points.len() - 1, -1.0)] {
                let start = r.points[from_end];
                let (_, t) = r.at(if dir > 0.0 { 0.5 } else { r.length() - 0.5 });
                let t = [t[0] * dir, t[1] * dir, t[2] * dir];
                // Entry speed: running speed for uphill rails, a hop for downhill.
                let entry = if t[1] > 0.05 { RUN_IN_SPEED } else { 2.0 };
                let mut pl = SpeedPlayer::new(start, t);
                pl.mode = Mode::Rail {
                    rail: j,
                    s: if dir > 0.0 { 0.0 } else { r.length() },
                    dir,
                };
                pl.velocity = [t[0] * entry, t[1] * entry, t[2] * entry];
                let terrain = Probe(s);
                let mut landing = None;
                for _ in 0..2400 {
                    speed_controller::step(
                        &s.physics,
                        &mut pl,
                        Input::default(),
                        &terrain,
                        &s.rails,
                        &[],
                    );
                    if pl.grounded() {
                        landing = Some(pl.position);
                        break;
                    }
                    if pl.position[1] < s.world.min[1] - 300.0 {
                        break;
                    }
                }
                let Some(landing) = landing else {
                    self.links
                        .push(format!("rail {j} from end {from_end}: no landing"));
                    continue;
                };
                // A stalled grind slides back to where it began: no link.
                if len(sub(landing, start)) < r.length() * 0.5 {
                    self.links
                        .push(format!("rail {j} from end {from_end}: stalls"));
                    continue;
                }
                let Some(to) = grid.nearest(landing) else {
                    continue;
                };
                let below: Vec<u32> = grid
                    .nodes_near(start, 20.0, self.body.jump + 20.0)
                    .into_iter()
                    .filter(|&n| {
                        let up = start[1] - grid.node_pos(n)[1];
                        (-2.0..self.body.jump - 4.0).contains(&up)
                    })
                    .collect();
                self.links.push(format!(
                    "rail {j} from end {from_end}: {} boarding nodes, lands at {landing:?}",
                    below.len()
                ));
                for b in below {
                    grid.add_link(
                        b,
                        to,
                        120.0,
                        RAIL_TAG + j as u32 * 2 + (from_end != 0) as u32,
                    );
                }
            }
        }
        self.grid = Some(grid);
    }

    fn plan(&mut self, s: &SpeedSession) {
        if self.grid.is_none() {
            self.build(s);
        }
        let grid = self.grid.as_ref().unwrap();
        let from = s.player.position;
        let goal = s.goals[0].position;
        let mut r = grid.route(self.body, from, goal, GOAL_RADIUS, 40.0);
        self.partial = !r.reached;
        if !r.reached && len(sub(r.closest, from)) > 50.0 {
            // Go as far as the geometry allows so the controller still runs.
            let closest = r.closest;
            r = grid.route(self.body, from, closest, 4.0, 4.0);
        }
        self.replans += 1;
        self.plans.push(PlanRecord {
            frame: s.frame,
            from,
            reached: !self.partial,
            waypoints: r.waypoints.len(),
            links: r
                .tags
                .iter()
                .filter(|&&t| t != 0)
                .map(|&t| {
                    if t >= RAIL_TAG {
                        format!("rail {}", (t - RAIL_TAG) / 2)
                    } else {
                        format!("spring {}", t - LAUNCH_TAG)
                    }
                })
                .collect(),
            closest: r.closest,
            explored: r.nodes_explored,
        });
        self.gave_up = !r.reached;
        self.route = r.waypoints;
        self.tags = r.tags;
        self.index = 0;
        self.best = (f32::MAX, s.frame);
    }

    pub fn controls(&mut self, s: &SpeedSession) -> Input {
        let idle = Input::default();
        if self.gave_up {
            return idle;
        }
        if let Some(Wait::Flight { since, left }) = self.wait {
            let flying = !s.player.grounded();
            if !left && flying {
                self.wait = Some(Wait::Flight { since, left: true });
            } else if (left && !flying) || s.frame - since > 1200 {
                self.wait = None;
                self.plan(s);
            }
            if self.wait.is_some() {
                // Hold the jump so a rail board rises fully.
                let rising = self.jump_frames > 0;
                self.jump_frames -= 1;
                return Input {
                    jump: rising,
                    ..idle
                };
            }
        }
        if s.player.on_rail().is_some() {
            // Ride it out, then replan from wherever it delivers the body.
            self.wait = Some(Wait::Flight {
                since: s.frame,
                left: true,
            });
            return idle;
        }

        if self.route.is_empty() {
            self.plan(s);
            if self.gave_up {
                return idle;
            }
        }
        let p = s.player.position;
        let speed = s.player.speed();
        // Advance past reached waypoints (faster bodies pass them sooner).
        let reach = 6.0 + speed * 2.0;
        while self.index < self.route.len() {
            let w = self.route[self.index];
            let h = (w[0] - p[0]).hypot(w[2] - p[2]);
            if h < reach && (w[1] - p[1]).abs() < STEP_UP + 6.0 + speed * 2.0 {
                let next = self.tags.get(self.index + 1).copied().unwrap_or(0);
                if next != 0 && s.player.grounded() {
                    // Special link: springs fire on contact; rails need a jump.
                    if next >= RAIL_TAG {
                        let j = ((next - RAIL_TAG) / 2) as usize;
                        let end = if (next - RAIL_TAG).is_multiple_of(2) {
                            s.rails[j].points[0]
                        } else {
                            *s.rails[j].points.last().unwrap()
                        };
                        let d = sub(end, p);
                        let l = d[0].hypot(d[2]);
                        if l > speed_controller::RAIL_CAPTURE_XZ - 1.0 && h < 30.0 {
                            // Get under the rail end, then jump onto it.
                            return steer(d, 0.8, false);
                        }
                        self.jump_frames = 40;
                    }
                    self.wait = Some(Wait::Flight {
                        since: s.frame,
                        left: false,
                    });
                    self.index += 1;
                    let target = self.route[self.index.min(self.route.len() - 1)];
                    return steer(sub(target, p), 1.0, next >= RAIL_TAG);
                }
                self.index += 1;
            } else {
                break;
            }
        }
        if self.index >= self.route.len() {
            // A partial route ends where the lattice stops: report and stop.
            self.gave_up = self.partial;
            self.route.clear();
            return idle;
        }
        // Progress watchdog: replan if nothing improves for four seconds.
        let remaining = self.route.len() - self.index;
        let w = self.route[self.index];
        let d = (w[0] - p[0]).hypot(w[2] - p[2]) + remaining as f32 * SPACING;
        if d < self.best.0 - 1.0 {
            self.best = (d, s.frame);
        } else if s.frame - self.best.1 > 240 {
            self.route.clear();
            return idle;
        }
        // Aim at a waypoint further ahead at speed, but never past a special
        // link or a climb.
        let ahead = ((speed * 3.0 / SPACING) as usize).clamp(1, 8);
        let mut k = self.index;
        while k + 1 < self.route.len()
            && k < self.index + ahead
            && self.tags.get(k + 1).copied().unwrap_or(0) == 0
            && self.route[k + 1][1] - self.route[k][1] < STEP_UP
        {
            k += 1;
        }
        let target = self.route[k];
        let next = self.route[self.index];
        let climb = next[1] - p[1] > STEP_UP - 1.0;
        let prev = if self.index > 0 {
            self.route[self.index - 1]
        } else {
            p
        };
        let span = (next[0] - prev[0]).hypot(next[2] - prev[2]);
        // A long level or rising link spans a gap: jump it. Lips on drops are
        // handled by the stuck rule below.
        let leap = span > SPACING * 1.6 && next[1] - prev[1] > -STEP_UP;
        // Blocked by a lip the lattice cannot see: jump it.
        if s.player.grounded() && speed < 0.5 {
            self.stuck += 1;
        } else {
            self.stuck = 0;
        }
        let jump = climb || leap || self.stuck > 20;
        if self.stuck > 20 {
            self.stuck = 0;
        }
        if jump && s.player.grounded() {
            self.jump_frames = 30;
        }
        let holding = self.jump_frames > 0 && !s.player.grounded();
        self.jump_frames -= 1;
        // Ease off when the next turn is sharp at speed.
        let dir = sub(target, p);
        let turn = {
            let f = s.player.facing;
            let l = dir[0].hypot(dir[2]).max(1e-3);
            (f[0] * dir[0] + f[2] * dir[2]) / l
        };
        let mag = if speed > 3.5 && turn < 0.3 { 0.2 } else { 1.0 };
        steer(dir, mag, (jump && s.player.grounded()) || holding)
    }

    pub fn route_len(&self) -> usize {
        self.route.len()
    }
}

/// Stick input toward a world direction with the camera at yaw 0
/// (right = +X, forward = -Z).
fn steer(d: V3, mag: f32, jump: bool) -> Input {
    let l = d[0].hypot(d[2]).max(1e-3);
    Input {
        stick: [d[0] / l * mag, -d[2] / l * mag],
        camera_yaw: 0.0,
        jump,
        action: false,
    }
}

/// The velocity and no-control frames spring `j` gives a still player.
fn spring_velocity(s: &SpeedSession, j: usize) -> Option<(V3, i32)> {
    use crate::spring::{SpringCall, SpringPlayer};
    let sp = &s.springs[j];
    let mut task = sp.task_clone();
    let player = SpringPlayer {
        position: sp.position,
        running_speed: 0.0,
        item: 0,
    };
    for _ in 0..8 {
        for c in task.step(
            Some(0),
            Some(player),
            sp.out_point(),
            crate::play_session::bams_sin_cos,
        ) {
            match c {
                SpringCall::SpringVelocity(0, v) => return Some((v, 0)),
                SpringCall::VelocityYRotationNoControl {
                    slot: 0,
                    velocity_y,
                    angles,
                    frames,
                } => {
                    return Some((
                        crate::play_session::rotate_zxy([0.0, velocity_y, 0.0], angles),
                        frames,
                    ))
                }
                _ => {}
            }
        }
    }
    None
}
