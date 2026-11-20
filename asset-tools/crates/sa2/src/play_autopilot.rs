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
        stick: [f32; 2],
    },
}

/// Stick held during a spring flight once input is allowed: none, then the
/// eight compass directions (camera yaw 0, so +y is -Z).
const POLICIES: [[f32; 2]; 9] = [
    [0.0, 0.0],
    [1.0, 0.0],
    [-1.0, 0.0],
    [0.0, 1.0],
    [0.0, -1.0],
    [0.707, 0.707],
    [0.707, -0.707],
    [-0.707, 0.707],
    [-0.707, -0.707],
];

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

    fn plan(&mut self, s: &Session) {
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
            let board = grid.nodes_near(sp.position, 6.0, 15.0);
            let mut seen = vec![];
            for (k, stick) in POLICIES.iter().enumerate() {
                let c = Controls {
                    stick: *stick,
                    camera_yaw: 0.0,
                    jump: false,
                };
                let Some(landing) = s.predict_flight(pos, vel, nocon, c, 900) else {
                    continue;
                };
                let Some(to) = grid.nearest(landing) else {
                    continue;
                };
                if seen.contains(&to) {
                    continue;
                }
                seen.push(to);
                for &b in &board {
                    grid.add_link(b, to, 80.0, LAUNCH_TAG + (j * 16 + k) as u32);
                }
            }
        }
        let from = s.player.position;
        let r = grid.route(self.body, from, s.goals[0].position, GOAL_RADIUS, 40.0);
        let links = r
            .tags
            .iter()
            .filter(|&&t| t != 0)
            .map(|&t| {
                if t >= LAUNCH_TAG {
                    let k = (t - LAUNCH_TAG) as usize;
                    format!("spring {} steer {:?}", k / 16, POLICIES[k % 16])
                } else {
                    format!("ride mover {}", t - RIDE_TAG)
                }
            })
            .collect();
        self.plans.push(PlanRecord {
            frame: s.frame,
            from,
            reached: r.reached,
            waypoints: r.waypoints.len(),
            links,
            closest: r.closest,
            explored: r.nodes_explored,
            climbs: r
                .waypoints
                .windows(2)
                .zip(r.tags.iter().skip(1))
                .filter(|(w, &t)| t == 0 && w[1][1] - w[0][1] > STEP)
                .map(|(w, _)| (w[0], w[1]))
                .collect(),
        });
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
            Some(Wait::Launch { left, since, stick }) => {
                if !left && !s.player.grounded {
                    self.wait = Some(Wait::Launch {
                        left: true,
                        since,
                        stick,
                    });
                } else if (left && s.player.grounded) || s.frame - since > 600 {
                    self.wait = None;
                    self.plan(s);
                    return idle;
                }
                return if left {
                    Controls { stick, ..idle }
                } else {
                    idle
                };
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
                if next != 0 && s.player.grounded {
                    self.wait = Some(if next >= LAUNCH_TAG {
                        Wait::Launch {
                            left: false,
                            since: s.frame,
                            stick: POLICIES[((next - LAUNCH_TAG) % 16) as usize],
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
        let target = if boarding {
            next
        } else {
            self.route[(self.index + 1).min(self.route.len() - 1)]
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
        let climb = next[1] - p[1] > STEP - 1.0 || target[1] - p[1] > STEP - 1.0;
        // A long level link spans a gap: jump it.
        let prev = if self.index > 0 {
            self.route[self.index - 1]
        } else {
            p
        };
        let span = (next[0] - prev[0]).hypot(next[2] - prev[2]);
        let leap = span > SPACING * 1.6 && next[1] - prev[1] > -STEP;
        Controls {
            // Camera yaw 0: right = +X, forward = -Z.
            stick: [dx / l * mag, -dz / l * mag],
            camera_yaw: 0.0,
            jump: (climb || leap) && (s.player.grounded || s.player.velocity[1] > 0.0),
        }
    }
}
