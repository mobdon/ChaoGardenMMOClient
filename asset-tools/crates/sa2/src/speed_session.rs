//! A playable speed-stage session for Sonic: City Escape (Hero story, level 13).
//!
//! Native inputs: the mounted landtable, SET placements and object-list rows,
//! the start row, Sonic's PhysicsArray row, the stage's grind-rail paths and
//! death zones from the Steam executable, the spring callback, the shared ring
//! pickup contract, and the goal-ring initialization and contact branch read
//! from stg13D.rel.
//!
//! Adaptations: the controller itself (`speed_controller`, see its module
//! note), dash-panel, ring-group, item-box, enemy and checkpoint contact, and
//! the camera. docs/SPEED-CONTROLLER.md lists each one.
use crate::{
    death_zones,
    fan_stage::MountedStage,
    goal_ring,
    play_objects::{death_zone_triangles, inside_volume, place_y, read_object_solid},
    play_session::{bams_sin_cos, rotate_zxy},
    play_world::{add, len, make_tri, scale, sub, Ground, PlayWorld, V3},
    speed_controller::{self, Action, Input, Rail, SpeedPhysics, SpeedPlayer, Target, Terrain},
    spring::{SpringCall, SpringKind, SpringPlayer, StageSpring},
    stage_paths, Error, Result,
};
use serde::Serialize;
use std::collections::BTreeMap;

pub const LEVEL: u16 = 13;
pub const MODULE_FILE: &str = "stg13D.rel";
/// PC_SA2/STG13.ini.
pub const DEATH_ZONE_CONFIG: &str = "[City Escape Death Zones]\ntype=deathzone\naddress=CDC678\n";
pub const PATH_CONFIG: &str = "[City Escape Paths]\ntype=pathlist\naddress=CDB35C\n";
/// The path callback every City Escape grind rail registers (path type 1).
pub const RAIL_CALLBACK_VA: u32 = 0x4980c0;
/// o_ring_M1: pickup when the collision centre is strictly within 14.
pub const RING_PICKUP_DISTANCE: f32 = 14.0;
pub const GOAL_RADIUS: f32 = 25.0;
/// Adaptations (shapes not read): contact distances added to the body radius.
pub const SPRING_CONTACT: f32 = 10.0;
pub const PANEL_CONTACT: f32 = 12.0;
pub const TARGET_CONTACT: f32 = 10.0;
pub const CHECKPOINT_RADIUS: f32 = 30.0;
/// Adaptation: frames of invulnerability after an enemy knocks rings loose.
pub const HURT_FRAMES: i32 = 120;
const RING_VA: u32 = 0x6c0f80;
const RING_LINEAR_VA: u32 = 0x6c5ed0;
const RING_CIRCLE_VA: u32 = 0x6c6180;
const SPRA_VA: u32 = 0x6c4480;
const SPRB_VA: u32 = 0x6c4e90;
const KASOKU_VA: u32 = 0x6cfbf0;
const SAVEPOINT_VA: u32 = 0x6d99a0;
const GOALRING_VA: u32 = 0x6c63c0;
const ITEMBOXAIR_VA: u32 = 0x6c8ef0;
const CONTIRON_VA: u32 = 0x6d63f0;
const CONTCHAO_VA: u32 = 0x6de3b0;
const CONTWOOD_VA: u32 = 0x6cd7f0;
/// PC_SA2/STG13.ini [City Escape Bush Block Collision].
pub const GREEN_B_COLLISION_RVA: u32 = 0xcaa350;
const GREEN_B_VA: u32 = 0x5e2b40;
/// Adaptation (as in the Iron Gate session): containers as solid 20-unit cubes.
pub const CONTAINER_EDGE: f32 = 20.0;
const ENEMIES: [(&str, u32); 3] = [
    ("E AI", 0x4ff990),
    ("E KUMI", 0x501530),
    ("E GOLD", 0x506510),
];
const FRAME_RATE: f32 = 60.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Event {
    Ring {
        layer: usize,
        index: usize,
    },
    Checkpoint {
        layer: usize,
        index: usize,
    },
    Spring {
        spring: usize,
    },
    DashPanel {
        panel: usize,
    },
    Destroyed {
        target: usize,
    },
    Hurt {
        target: usize,
        rings_lost: u32,
    },
    Controller {
        action: Action,
    },
    Death {
        frame: u32,
        at: V3,
        zone: bool,
    },
    Respawn {
        frame: u32,
    },
    GoalContact {
        layer: usize,
        index: usize,
        sound: u16,
    },
    Cleared {
        frame: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    Playing,
    Cleared,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pickup {
    pub layer: usize,
    pub index: usize,
    pub position: V3,
    pub taken: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Spring {
    pub layer: usize,
    pub index: usize,
    pub position: V3,
    #[serde(skip)]
    task: StageSpring,
    out: V3,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Panel {
    pub layer: usize,
    pub index: usize,
    pub position: V3,
    /// Unit direction: local +Z turned by the placement's Y angle.
    pub direction: V3,
    /// Scale.x and Scale.y of the placement.
    pub speed: f32,
    pub nocon: i32,
    cooldown: i32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomingTarget {
    pub name: String,
    pub layer: usize,
    pub index: usize,
    pub position: V3,
    pub enemy: bool,
    pub alive: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Goal {
    pub layer: usize,
    pub index: usize,
    pub position: V3,
    pub angle_x: i32,
    pub route: String,
    #[serde(skip)]
    contact: goal_ring::Contact,
}

impl Spring {
    /// A fresh copy of the spring task (for offline launch prediction).
    pub fn task_clone(&self) -> StageSpring {
        self.task.clone()
    }
    /// The launch direction point the spring task is stepped with.
    pub fn out_point(&self) -> V3 {
        self.out
    }
}

/// Installed inputs a session is built from (all read by the caller).
pub struct Sources<'a> {
    pub exe: &'a [u8],
    pub stage_module: &'a [u8],
}

struct World<'a>(&'a PlayWorld);
impl Terrain for World<'_> {
    fn ground(&self, p: V3, depth: f32) -> Option<Ground> {
        self.0.ground_below(p, depth)
    }
    fn ceiling(&self, p: V3, reach: f32) -> Option<f32> {
        self.0.ceiling_above(p, reach)
    }
    fn push_out(&self, c: V3, r: f32) -> (V3, Option<V3>) {
        // Adaptation: walls collide from their front side only.
        self.0.push_out_front(c, r, 4)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeedSession {
    pub level: u16,
    pub character: i8,
    pub physics: SpeedPhysics,
    pub goal_program_local_effect: u32,
    pub frame: u32,
    pub rings: u32,
    pub score: u32,
    pub deaths: u32,
    pub outcome: Outcome,
    pub player: SpeedPlayer,
    pub start: V3,
    pub start_facing: V3,
    pub respawn: V3,
    pub slot_words: [u32; 2],
    pub rings_placed: Vec<Pickup>,
    pub checkpoints: Vec<Pickup>,
    pub goals: Vec<Goal>,
    pub springs: Vec<Spring>,
    pub panels: Vec<Panel>,
    pub targets: Vec<HomingTarget>,
    pub rails: Vec<Rail>,
    hurt: i32,
    /// Placements whose native behavior this session does not run.
    pub unimplemented: BTreeMap<String, usize>,
    #[serde(skip)]
    pub world: PlayWorld,
    #[serde(skip)]
    death_zones: Vec<Vec<[V3; 3]>>,
}

/// The facing of a native start row. Adaptation: the row's angle `a` (BAMS)
/// is read as the direction (cos a, 0, sin a), which points City Escape's
/// start down the opening street.
pub fn start_facing(yaw: u16) -> V3 {
    let (s, c) = bams_sin_cos(yaw as i32);
    [c, 0.0, s]
}

/// A placement's local +Z turned by its Y angle (dash panels point this way).
pub fn yaw_forward(yaw: u16) -> V3 {
    let (s, c) = bams_sin_cos(yaw as i16 as i32);
    [s, 0.0, c]
}

/// Twelve triangles of a box with half extents `h` centred on the origin.
pub fn box_tris(h: V3) -> Vec<[V3; 3]> {
    let p = |x: f32, y: f32, z: f32| [x * h[0], y * h[1], z * h[2]];
    let quads = [
        [
            p(-1., 1., -1.),
            p(-1., 1., 1.),
            p(1., 1., 1.),
            p(1., 1., -1.),
        ],
        [
            p(-1., -1., -1.),
            p(1., -1., -1.),
            p(1., -1., 1.),
            p(-1., -1., 1.),
        ],
        [
            p(1., -1., -1.),
            p(1., 1., -1.),
            p(1., 1., 1.),
            p(1., -1., 1.),
        ],
        [
            p(-1., -1., -1.),
            p(-1., -1., 1.),
            p(-1., 1., 1.),
            p(-1., 1., -1.),
        ],
        [
            p(-1., -1., 1.),
            p(1., -1., 1.),
            p(1., 1., 1.),
            p(-1., 1., 1.),
        ],
        [
            p(-1., -1., -1.),
            p(-1., 1., -1.),
            p(1., 1., -1.),
            p(1., -1., -1.),
        ],
    ];
    quads
        .iter()
        .flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]])
        .collect()
}

impl SpeedSession {
    /// Build City Escape from a mounted Hero story dispatch (Sonic or Shadow).
    pub fn city_escape(stage: &MountedStage<'_>, src: Sources<'_>) -> Result<Self> {
        if stage.dispatch.level != LEVEL {
            return Err(Error::invalid(
                "the speed session currently supports City Escape only",
            ));
        }
        let entry = stage
            .entry_pose
            .as_ref()
            .ok_or_else(|| Error::invalid("City Escape has no start row"))?;
        let collision = stage
            .collision_world()?
            .ok_or_else(|| Error::invalid("City Escape terrain is unavailable"))?;
        let objects = stage
            .object_table
            .ok_or_else(|| Error::invalid("City Escape object list is unavailable"))?;
        let slot = match stage.dispatch.character {
            0 => 0usize,
            1 => 1,
            c => return Err(Error::invalid(format!("unsupported character {c}"))),
        };
        let rows = crate::player_physics::read(src.exe)?;
        let physics = SpeedPhysics::from_row(slot, &rows[slot].parameters);
        let goal_program = goal_ring::Program::read(LEVEL, src.stage_module)?;
        let bush_solid = read_object_solid(src.exe, GREEN_B_COLLISION_RVA)?;
        let mut world = PlayWorld::from_collision(&collision);
        let mut solid = |tris: Vec<[V3; 3]>| {
            for t in tris {
                if let Some(t) = make_tri(t, 1, u32::MAX) {
                    world.push(t);
                }
            }
        };

        let mut rings_placed = vec![];
        let mut checkpoints = vec![];
        let mut goals = vec![];
        let mut springs = vec![];
        let mut panels = vec![];
        let mut targets = vec![];
        let mut unimplemented = BTreeMap::new();
        for (li, layer) in stage.layers.iter().enumerate() {
            for p in &layer.placements {
                let Some(def) = p.definition_index.and_then(|i| objects.entries.get(i)) else {
                    continue;
                };
                let pl = &p.placement;
                let name = def.name.trim_end();
                let ring = |position| Pickup {
                    layer: li,
                    index: p.placement_index,
                    position,
                    taken: false,
                };
                match (def.function_va, name) {
                    (RING_VA, "RING") => rings_placed.push(ring(pl.position)),
                    (RING_LINEAR_VA, "RING_LINEAR") => {
                        // Adaptation: Scale.z rings, Scale.x apart, along local +Z.
                        let n = (pl.parameters[2].round() as i32).clamp(1, 64);
                        let step =
                            rotate_zxy([0.0, 0.0, pl.parameters[0]], pl.rotation.map(i32::from));
                        for k in 0..n {
                            rings_placed.push(ring(add(pl.position, scale(step, k as f32))));
                        }
                    }
                    (RING_CIRCLE_VA, "RING_CIRCLE") => {
                        // Adaptation: Scale.z rings on a circle of radius Scale.y.
                        let n = (pl.parameters[2].round() as i32).clamp(1, 64);
                        for k in 0..n {
                            let a = k as f32 * std::f32::consts::TAU / n as f32;
                            let local =
                                [a.cos() * pl.parameters[1], a.sin() * pl.parameters[1], 0.0];
                            let off = rotate_zxy(local, pl.rotation.map(i32::from));
                            rings_placed.push(ring(add(pl.position, off)));
                        }
                    }
                    (SAVEPOINT_VA, "SAVEPOINT") => checkpoints.push(ring(pl.position)),
                    (GOALRING_VA, "GOALRING") => {
                        let angle_x = pl.rotation[0] as i16 as i32;
                        let route = goal_ring::initialize(
                            angle_x,
                            stage.dispatch.character,
                            0,
                            None,
                            false,
                            true,
                        );
                        goals.push(Goal {
                            layer: li,
                            index: p.placement_index,
                            position: pl.position,
                            angle_x,
                            route: format!("{route:?}"),
                            contact: Default::default(),
                        });
                    }
                    (SPRA_VA, "SPRA") | (SPRB_VA, "SPRB") => {
                        let kind = if name == "SPRA" {
                            SpringKind::A
                        } else {
                            SpringKind::B
                        };
                        let angles = pl.rotation.map(i32::from);
                        let task = StageSpring::new(kind, pl.position, angles, pl.parameters);
                        let out = rotate_zxy([0.0, 5.0 + pl.parameters[1], 0.0], angles);
                        springs.push(Spring {
                            layer: li,
                            index: p.placement_index,
                            position: pl.position,
                            task,
                            out,
                        });
                    }
                    (KASOKU_VA, "KASOKU") => panels.push(Panel {
                        layer: li,
                        index: p.placement_index,
                        position: pl.position,
                        direction: yaw_forward(pl.rotation[1]),
                        speed: pl.parameters[0],
                        nocon: pl.parameters[1].round() as i32,
                        cooldown: 0,
                    }),
                    (ITEMBOXAIR_VA, "ITEMBOXAIR") => targets.push(HomingTarget {
                        name: name.into(),
                        layer: li,
                        index: p.placement_index,
                        position: pl.position,
                        enemy: false,
                        alive: true,
                    }),
                    (GREEN_B_VA, "GREEN_B") => solid(
                        bush_solid
                            .iter()
                            .map(|t| t.map(|v| place_y(v, pl.rotation[1], pl.position)))
                            .collect(),
                    ),
                    (CONTIRON_VA, "CONTIRON") | (CONTCHAO_VA, "CONTCHAO") => {
                        let e = CONTAINER_EDGE * 0.5;
                        solid(
                            box_tris([e; 3])
                                .into_iter()
                                .map(|t| {
                                    t.map(|v| {
                                        place_y([v[0], v[1] + e, v[2]], pl.rotation[1], pl.position)
                                    })
                                })
                                .collect(),
                        );
                        *unimplemented
                            .entry(format!("{name} (solid only)"))
                            .or_insert(0) += 1;
                    }
                    (CONTWOOD_VA, "CONTWOOD") => targets.push(HomingTarget {
                        name: name.into(),
                        layer: li,
                        index: p.placement_index,
                        position: add(pl.position, [0.0, CONTAINER_EDGE * 0.5, 0.0]),
                        enemy: false,
                        alive: true,
                    }),
                    (va, n) if ENEMIES.contains(&(n, va)) => targets.push(HomingTarget {
                        name: name.into(),
                        layer: li,
                        index: p.placement_index,
                        // Adaptation: the body centre sits 8 above the placement.
                        position: add(pl.position, [0.0, 8.0, 0.0]),
                        enemy: true,
                        alive: true,
                    }),
                    _ => *unimplemented.entry(name.to_string()).or_insert(0) += 1,
                }
            }
        }
        goals.retain(|g| g.route.starts_with("Contact"));
        if goals.is_empty() {
            return Err(Error::invalid(
                "no regular goal ring reaches its contact route",
            ));
        }
        let mut rails = vec![];
        for list in stage_paths::read(src.exe, PATH_CONFIG)?.values() {
            for path in &list.paths {
                if path.callback_va == RAIL_CALLBACK_VA && path.points.len() >= 2 {
                    rails.push(Rail::new(path.points.iter().map(|p| p.position).collect()));
                }
            }
        }
        let mut zones = vec![];
        for table in death_zones::read(src.exe, DEATH_ZONE_CONFIG)?.values() {
            for z in &table.zones {
                zones.push(death_zone_triangles(z)?);
            }
        }
        let facing = start_facing(entry.yaw);
        let mut s = Self {
            level: LEVEL,
            character: stage.dispatch.character,
            physics,
            goal_program_local_effect: goal_program.local_effect,
            frame: 0,
            rings: 0,
            score: 0,
            deaths: 0,
            outcome: Outcome::Playing,
            player: SpeedPlayer::new(entry.position, facing),
            start: entry.position,
            start_facing: facing,
            respawn: entry.position,
            slot_words: goal_program.initial_slot_words,
            rings_placed,
            checkpoints,
            goals,
            springs,
            panels,
            targets,
            rails,
            hurt: 0,
            unimplemented,
            world,
            death_zones: zones,
        };
        s.settle();
        Ok(s)
    }

    pub fn death_zone_count(&self) -> usize {
        self.death_zones.len()
    }

    pub fn player_center(&self) -> V3 {
        add(self.player.position, [0.0, self.physics.center_height, 0.0])
    }

    /// Restart at checkpoint `index` (in `checkpoints` order) after a miss.
    pub fn restart_from_checkpoint(&mut self, index: usize) -> Result<()> {
        let c = self
            .checkpoints
            .get_mut(index)
            .ok_or_else(|| Error::invalid("no such checkpoint"))?;
        c.taken = true;
        self.respawn = c.position;
        self.respawn_player();
        Ok(())
    }

    fn respawn_player(&mut self) {
        let facing = self.player.facing;
        self.player = SpeedPlayer::new(self.respawn, facing);
        self.settle();
    }

    fn settle(&mut self) {
        let p = self.player.position;
        if let Some(g) = self.world.ground_below([p[0], p[1] + 20.0, p[2]], 400.0) {
            self.player.position[1] = g.y;
        }
        // One idle frame lands the body.
        let ph = self.physics.clone();
        speed_controller::step(
            &ph,
            &mut self.player,
            Input::default(),
            &World(&self.world),
            &[],
            &[],
        );
    }

    /// Advance one 60 Hz frame.
    pub fn step(&mut self, input: Input) -> Vec<Event> {
        let mut events = vec![];
        if self.outcome == Outcome::Cleared {
            return events;
        }
        self.frame += 1;
        let targets: Vec<Target> = self
            .targets
            .iter()
            .map(|t| Target {
                position: t.position,
                alive: t.alive,
            })
            .collect();
        let ph = self.physics.clone();
        let actions = speed_controller::step(
            &ph,
            &mut self.player,
            input,
            &World(&self.world),
            &self.rails,
            &targets,
        );
        for a in actions {
            if let Action::HomingHit { target } = a {
                self.destroy(target, &mut events);
            }
            events.push(Event::Controller { action: a });
        }
        self.springs(&mut events);
        self.panels(&mut events);
        self.contacts(&mut events);
        self.objects(&mut events);
        events
    }

    fn destroy(&mut self, target: usize, events: &mut Vec<Event>) {
        let t = &mut self.targets[target];
        if t.alive {
            t.alive = false;
            // Adaptation: enemies score 100; item boxes give nothing here.
            if t.enemy {
                self.score += 100;
            }
            events.push(Event::Destroyed { target });
        }
    }

    fn springs(&mut self, events: &mut Vec<Event>) {
        let center = self.player_center();
        let rad = self.physics.rad;
        let pl = &mut self.player;
        let running = len([pl.velocity[0], 0.0, pl.velocity[2]]);
        for (i, sp) in self.springs.iter_mut().enumerate() {
            let near = len(sub(sp.task.position, center)) < SPRING_CONTACT + rad;
            let player = SpringPlayer {
                position: pl.position,
                running_speed: running,
                item: 0,
            };
            for call in sp
                .task
                .step(near.then_some(0u8), Some(player), sp.out, bams_sin_cos)
            {
                match call {
                    SpringCall::SetPosition(0, p) => pl.position = p,
                    SpringCall::SpringVelocity(0, v) => {
                        pl.launch(v, 0);
                        events.push(Event::Spring { spring: i });
                    }
                    SpringCall::VelocityYRotationNoControl {
                        slot: 0,
                        velocity_y,
                        angles,
                        frames,
                    } => {
                        pl.launch(rotate_zxy([0.0, velocity_y, 0.0], angles), frames);
                        events.push(Event::Spring { spring: i });
                    }
                    _ => {}
                }
            }
        }
    }

    fn panels(&mut self, events: &mut Vec<Event>) {
        let feet = self.player.position;
        let rad = self.physics.rad;
        for (i, p) in self.panels.iter_mut().enumerate() {
            p.cooldown = (p.cooldown - 1).max(0);
            let d = sub(p.position, feet);
            if p.cooldown == 0 && d[0].hypot(d[2]) < PANEL_CONTACT + rad && d[1].abs() < 12.0 {
                // Adaptation: the panel faces the player along its direction at
                // Scale.x units per frame and locks the stick for Scale.y frames.
                self.player.boost(p.direction, p.speed, p.nocon);
                p.cooldown = 30;
                events.push(Event::DashPanel { panel: i });
            }
        }
    }

    fn contacts(&mut self, events: &mut Vec<Event>) {
        self.hurt = (self.hurt - 1).max(0);
        let center = self.player_center();
        let reach = self.physics.rad + TARGET_CONTACT;
        let mut hit = None;
        for (i, t) in self.targets.iter().enumerate().filter(|(_, t)| t.alive) {
            if len(sub(t.position, center)) < reach {
                hit = Some(i);
                break;
            }
        }
        let Some(i) = hit else { return };
        if self.player.attacking() {
            self.destroy(i, events);
            if !self.player.grounded() {
                let ph = self.physics.clone();
                self.player.bounce(&ph);
            }
        } else if self.targets[i].enemy && self.hurt == 0 {
            // Adaptation: an enemy touched outside ball form takes every ring
            // and knocks the player back; with none it is a miss.
            let lost = self.rings;
            events.push(Event::Hurt {
                target: i,
                rings_lost: lost,
            });
            if lost == 0 {
                self.miss(false, events);
                return;
            }
            self.rings = 0;
            self.hurt = HURT_FRAMES;
            let back = scale(self.player.facing, -1.5);
            self.player.launch([back[0], 2.0, back[2]], 30);
        }
    }

    fn miss(&mut self, zone: bool, events: &mut Vec<Event>) {
        self.deaths += 1;
        events.push(Event::Death {
            frame: self.frame,
            at: self.player.position,
            zone,
        });
        self.rings = 0;
        self.respawn_player();
        events.push(Event::Respawn { frame: self.frame });
    }

    fn objects(&mut self, events: &mut Vec<Event>) {
        let center = self.player_center();
        let feet = self.player.position;
        for r in self.rings_placed.iter_mut().filter(|r| !r.taken) {
            if len(sub(r.position, center)) < RING_PICKUP_DISTANCE {
                r.taken = true;
                self.score += 10;
                self.rings += 1;
                events.push(Event::Ring {
                    layer: r.layer,
                    index: r.index,
                });
            }
        }
        for c in self.checkpoints.iter_mut().filter(|c| !c.taken) {
            let d = sub(c.position, feet);
            if d[0].hypot(d[2]) < CHECKPOINT_RADIUS && d[1].abs() < 40.0 {
                c.taken = true;
                self.respawn = c.position;
                events.push(Event::Checkpoint {
                    layer: c.layer,
                    index: c.index,
                });
            }
        }
        let rad = self.physics.rad;
        let local = self.goal_program_local_effect;
        for g in &mut self.goals {
            if len(sub(g.position, center)) >= GOAL_RADIUS + rad {
                continue;
            }
            let Ok(calls) = g
                .contact
                .step(g.angle_x, 0, goal_ring::Hit::Player { slot: 0 }, local)
            else {
                continue;
            };
            let sound = calls.iter().find_map(|c| match c {
                goal_ring::Call::Sound { code, .. } => Some(*code),
                _ => None,
            });
            if let Some(sound) = sound {
                goal_ring::apply_slot_writes(&mut self.slot_words, &calls, local);
                events.push(Event::GoalContact {
                    layer: g.layer,
                    index: g.index,
                    sound,
                });
            }
        }
        if self.slot_words[0] == 2 {
            self.outcome = Outcome::Cleared;
            events.push(Event::Cleared { frame: self.frame });
            return;
        }
        let zone = self.death_zones.iter().any(|z| inside_volume(z, center));
        if zone || self.player.position[1] < self.world.min[1] - 300.0 {
            self.miss(zone, events);
        }
    }

    pub fn elapsed_seconds(&self) -> f32 {
        self.frame as f32 / FRAME_RATE
    }
}
