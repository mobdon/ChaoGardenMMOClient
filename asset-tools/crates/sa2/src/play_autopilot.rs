//! Headless driver for offline verification: plans over the live session's
//! geometry (terrain plus movers where they currently are, plus ride and
//! spring links) and produces ordinary stick/jump controls. It never moves
//! the player directly, so a clear proves the session itself carries a body
//! from spawn to the goal.
use crate::{
    play_nav::{Body, NavGrid},
    play_session::{Controls, MoverState, Session, GOAL_RADIUS},
    play_world::{add, make_tri, V3},
};
use serde::Serialize;

const SPACING: f32 = 8.0;
const STEP: f32 = 12.0;
/// Largest rise the autopilot walks up without jumping.
const WALK_RISE: f32 = 6.0;
const RIDE_TAG: u32 = 1;
const LAUNCH_TAG: u32 = 10_000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanRecord {
    pub frame: u32,
    pub from: V3,
    pub reached: bool,
    pub waypoints: usize,
    /// Rides and launches the plan uses, in order.
    pub links: Vec<String>,
    pub closest: V3,
    pub explored: usize,
    /// Upward gains above the plain step, as (from, to).
    pub climbs: Vec<(V3, V3)>,
}

#[derive(Debug, Clone, Copy)]
enum Wait {
    Ride {
        mover: usize,
        started: bool,
        since: u32,
    },
    Launch {
        left: bool,
        since: u32,
    },
}

fn record(s: &Session, from: V3, r: &crate::play_nav::Route) -> PlanRecord {
    PlanRecord {
        frame: s.frame,
        from,
        reached: r.reached,
        waypoints: r.waypoints.len(),
        links: r
            .tags
            .iter()
            .filter(|&&t| t != 0)
            .map(|&t| {
                if t >= LAUNCH_TAG {
                    format!("spring {}", t - LAUNCH_TAG)
                } else {
                    format!("ride mover {}", t - RIDE_TAG)
                }
            })
            .collect(),
        closest: r.closest,
        explored: r.nodes_explored,
        climbs: r
            .waypoints
            .windows(2)
            .zip(r.tags.iter().skip(1))
            .filter(|(w, &t)| t == 0 && w[1][1] - w[0][1] > STEP)
            .map(|(w, _)| (w[0], w[1]))
            .collect(),
    }
}

pub struct Autopilot {
    pub body: Body,
    route: Vec<V3>,
    tags: Vec<u32>,
    index: usize,
    wait: Option<Wait>,
    best: (f32, u32),
    pub plans: Vec<PlanRecord>,
    pub gave_up: bool,
}

impl Autopilot {
    pub fn new(s: &Session) -> Self {
        Self {
            body: Body {
                radius: s.physics.rad,
                height: s.physics.height,
                step: STEP,
                jump: std::env::var("SA2_PLAY_NAV_JUMP")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(s.jump_height() - 1.5),
            },
            route: vec![],
            tags: vec![],
            index: 0,
            wait: None,
            best: (f32::MAX, 0),
            plans: vec![],
            gave_up: false,
        }
    }

    /// Plan from `from` to `to` over the session as it is now, without
    /// driving. A diagnostic for finding where a stage's route breaks.
    pub fn probe(&self, s: &Session, from: V3, to: V3, radius: f32) -> PlanRecord {
        let grid = self.grid(s);
        let r = grid.route(self.body, from, to, radius, 40.0);
        record(s, from, &r)
    }

    fn grid(&self, s: &Session) -> NavGrid {
        let mut world = s.snapshot_world();
        // A bridge extends once an elevator comes to rest near it, so plan
        // with it extended wherever an elevator can arrive in range.
        let rests: Vec<V3> = s
            .movers
            .iter()
            .filter_map(|m| match &m.state {
                MoverState::Elevator(e) => Some([e.origin, e.target]),
                _ => None,
            })
            .flatten()
            .collect();
        for m in &s.movers {
            let MoverState::Bridge(b) = &m.state else {
                continue;
            };
            let near = rests.iter().any(|r| {
                let d = [
                    r[0] - b.extended[0],
                    r[1] - b.extended[1],
                    r[2] - b.extended[2],
                ];
                (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
                    < s.movers_program.bridge_elevator_range
            });
            if near && !b.is_extended {
                let o = [
                    b.extended[0] - m.origin[0],
                    b.extended[1] - m.origin[1],
                    b.extended[2] - m.origin[2],
                ];
                for t in &m.solid.tris {
                    if let Some(t) = make_tri(t.v.map(|v| add(v, o)), t.flags, t.mesh) {
                        world.push(t);
                    }
                }
            }
        }
        let mut grid = NavGrid::build(&world, self.body, SPACING);
        // Ride links: board an idle mover, arrive where its top will be.
        for (i, m) in s.movers.iter().enumerate() {
            let shift = match &m.state {
                MoverState::Lift(l) if !l.raised && !l.moving => [
                    l.top[0] - l.base[0],
                    l.top[1] - l.base[1],
                    l.top[2] - l.base[2],
                ],
                MoverState::Elevator(e) if e.mode == 0 && !e.at_target => [
                    e.target[0] - e.position[0],
                    e.target[1] - e.position[1],
                    e.target[2] - e.position[2],
                ],
                _ => continue,
            };
            let o = m.offset();
            // The deck: walkable triangles at the solid's top height.
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for t in m.solid.tris.iter().filter(|t| t.n[1].abs() > 0.6) {
                if t.v.iter().all(|v| v[1] > m.solid.max[1] - 0.5) {
                    for v in &t.v {
                        for k in 0..3 {
                            lo[k] = lo[k].min(v[k] + o[k]);
                            hi[k] = hi[k].max(v[k] + o[k]);
                        }
                    }
                }
            }
            if lo[0] > hi[0] {
                continue;
            }
            let top = [(lo[0] + hi[0]) * 0.5, hi[1], (lo[2] + hi[2]) * 0.5];
            let half = ((hi[0] - lo[0]).min(hi[2] - lo[2]) * 0.5 - 6.0).max(4.0);
            let board = grid.nodes_near(top, half, 4.0);
            if board.is_empty() {
                continue;
            }
            let dest = grid.add_node([top[0] + shift[0], top[1] + shift[1], top[2] + shift[2]]);
            // The whole arrived deck is walkable, so it can join what lies beside it.
            let mut x = lo[0] + 4.0;
            while x <= hi[0] - 4.0 {
                let mut z = lo[2] + 4.0;
                while z <= hi[2] - 4.0 {
                    grid.add_node([x + shift[0], top[1] + shift[1], z + shift[2]]);
                    z += SPACING;
                }
                x += SPACING;
            }
            let cost = (shift[0].hypot(shift[2]).hypot(shift[1])) * 0.5 + 100.0;
            for b in board {
                grid.add_link(b, dest, cost, RIDE_TAG + i as u32);
            }
        }
        // Launch links: walk onto a spring, land where free flight ends.
        for (j, sp) in s.springs.iter().enumerate() {
            let (pos, vel, nocon) = sp.launch();
            let Some(landing) = s.predict_landing(pos, vel, nocon, 900) else {
                continue;
            };
            let Some(to) = grid.nearest(landing) else {
                continue;
            };
            for b in grid.nodes_near(sp.position, 6.0, 15.0) {
                grid.add_link(b, to, 80.0, LAUNCH_TAG + j as u32);
            }
        }
        grid
    }

    fn plan(&mut self, s: &Session) {
        let grid = self.grid(s);
        let from = s.player.position;
        let r = grid.route(self.body, from, s.goals[0].position, GOAL_RADIUS, 40.0);
        self.plans.push(record(s, from, &r));
        self.gave_up = !r.reached;
        self.route = r.waypoints;
        self.tags = r.tags;
        self.index = 0;
        self.best = (f32::MAX, s.frame);
    }

    pub fn controls(&mut self, s: &Session) -> Controls {
        let idle = Controls::default();
        if self.gave_up {
            return idle;
        }
        match self.wait {
            Some(Wait::Ride {
                mover,
                started,
                since,
            }) => {
                let moving = match &s.movers[mover].state {
                    MoverState::Lift(l) => l.moving,
                    MoverState::Elevator(e) => e.mode == 1,
                    MoverState::Bridge(b) => b.moving,
                };
                if moving {
                    self.wait = Some(Wait::Ride {
                        mover,
                        started: true,
                        since: s.frame,
                    });
                    return idle;
                }
                // Wait for the start, then let dependent movers (bridges) settle.
                if (!started && s.frame - since < 400) || (started && s.frame - since < 160) {
                    return idle;
                }
                self.wait = None;
                self.plan(s);
                return idle;
            }
            Some(Wait::Launch { left, since }) => {
                if !left && !s.player.grounded {
                    self.wait = Some(Wait::Launch { left: true, since });
                } else if (left && s.player.grounded) || s.frame - since > 300 {
                    self.wait = None;
                    self.plan(s);
                }
                return idle;
            }
            None => {}
        }
        if self.route.is_empty() {
            self.plan(s);
            if self.gave_up {
                return idle;
            }
        }
        let p = s.player.position;
        // Advance past reached waypoints; stop at a boarding point.
        while self.index < self.route.len() {
            let w = self.route[self.index];
            let h = (w[0] - p[0]).hypot(w[2] - p[2]);
            if h < 5.0 && (w[1] - p[1]).abs() < STEP + 2.0 {
                let next = self.tags.get(self.index + 1).copied().unwrap_or(0);
                if next != 0 && !s.player.grounded {
                    // Over the boarding point mid-jump: land on it rather
                    // than skipping the ride.
                    break;
                }
                if next != 0 {
                    self.wait = Some(if next >= LAUNCH_TAG {
                        Wait::Launch {
                            left: false,
                            since: s.frame,
                        }
                    } else {
                        Wait::Ride {
                            mover: (next - RIDE_TAG) as usize,
                            started: false,
                            since: s.frame,
                        }
                    });
                    self.route.clear();
                    return idle;
                }
                self.index += 1;
            } else {
                break;
            }
        }
        if self.index >= self.route.len() {
            self.route.clear();
            return idle;
        }
        // Progress watchdog.
        let remaining = self.route.len() - self.index;
        let d = (self.route[self.index][0] - p[0]).hypot(self.route[self.index][2] - p[2])
            + remaining as f32 * SPACING;
        if d < self.best.0 - 1.0 {
            self.best = (d, s.frame);
        } else if s.frame - self.best.1 > 240 {
            self.route.clear();
            return idle;
        }
        let next = self.route[self.index];
        let boarding = self.tags.get(self.index + 1).is_some_and(|&t| t != 0);
        let ahead = self.route[(self.index + 1).min(self.route.len() - 1)];
        // Look one waypoint ahead to smooth the path, unless the shortcut
        // leaves the floor (cutting a ledge corner).
        let floor = |q: V3| {
            s.floor_below([q[0], q[1] + STEP, q[2]], STEP * 3.0)
                .is_some_and(|y| y > q[1] - STEP)
        };
        let shortcut_ok = (1..=4).all(|k| {
            let t = k as f32 / 4.0;
            let q = [
                p[0] + (ahead[0] - p[0]) * t,
                p[1].min(ahead[1]),
                p[2] + (ahead[2] - p[2]) * t,
            ];
            floor(q)
        });
        let target = if boarding || !shortcut_ok {
            next
        } else {
            ahead
        };
        let dx = target[0] - p[0];
        let dz = target[2] - p[2];
        let l = dx.hypot(dz).max(1e-3);
        // Slow down onto boarding points so the body stops on them.
        let mag = if boarding {
            (l / 12.0).clamp(0.25, 1.0)
        } else {
            1.0
        };
        // The session's wall sphere starts half a step above the feet, so a
        // rise taller than that stops a walking body: jump it.
        let climb = next[1] - p[1] > WALK_RISE || target[1] - p[1] > WALK_RISE;
        // A long level link spans a gap: jump it.
        let prev = if self.index > 0 {
            self.route[self.index - 1]
        } else {
            p
        };
        let span = (next[0] - prev[0]).hypot(next[2] - prev[2]);
        // Only take off on the near side: once past the gap (an overshoot
        // that has to walk back to the waypoint) there is nothing to clear.
        let near_side =
            (p[0] - prev[0]).hypot(p[2] - prev[2]) < (p[0] - next[0]).hypot(p[2] - next[2]);
        // A long link only needs a jump where the floor actually breaks.
        let gap = (1..4).any(|k| {
            let t = k as f32 / 4.0;
            !floor([
                prev[0] + (next[0] - prev[0]) * t,
                prev[1].min(next[1]),
                prev[2] + (next[2] - prev[2]) * t,
            ])
        });
        let leap = span > SPACING * 1.6 && next[1] - prev[1] > -STEP && near_side && gap;
        // Take off only once turned toward the target: the walker turns at
        // a limited rate and a jump keeps its ground velocity.
        let facing = [-s.player.yaw.sin(), -s.player.yaw.cos()];
        let aligned = facing[0] * dx / l + facing[1] * dz / l > 0.8;
        Controls {
            // Camera yaw 0: right = +X, forward = -Z.
            stick: [dx / l * mag, -dz / l * mag],
            camera_yaw: 0.0,
            // A level gap only needs a tap (holding adds height and so
            // range, which overshoots small decks); a climb holds the jump.
            jump: ((climb || leap) && s.player.grounded && aligned)
                || (climb && !s.player.grounded && s.player.velocity[1] > 0.0),
        }
    }
}
