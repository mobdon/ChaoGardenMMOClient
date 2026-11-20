//! Native rule state of Iron Gate (Dark story, level 11).
//!
//! Everything here is read from the game files: the mounted landtable, SET
//! placements and object-list names, object collision models and death zones
//! from the Steam executable, the lift, elevator and bridge rules and the
//! iron-container collision shape from stg11D.rel, the o_ring_M1 pickup, and
//! the goal-ring initialization and contact branch.
//!
//! The walker's movement and its collision response are main-program code
//! that is not readable on this build, so nothing here moves the player. A
//! host supplies a `PlayerSample` each frame (collision centre, the mover it
//! stands on, a goal-ring CCL hit), as the engine would.
use crate::{
    death_zones,
    fan_stage::MountedStage,
    goal_ring,
    iron_gate_movers::{Bridge, Elevator, Lift, MoverProgram},
    play_objects::{death_zone_triangles, place_y, read_object_solid},
    play_world::{len, make_tri, sub, PlayWorld, V3},
    player_physics::PlayerParameters,
    spring::{SpringKind, StageSpring},
    Error, Result,
};
use serde::Serialize;

/// SA Tools PC_SA2/STG11.ini collision model RVAs in the Steam executable.
pub const LIFT_COLLISION_RVA: u32 = 0x7f0a68;
pub const BRIDGE_COLLISION_RVA: u32 = 0x7f58a8;
pub const ELEVATOR_COLLISION_RVA: u32 = 0x7fed08;
/// PC_SA2/STG11.ini [Iron Gate Death Zones].
pub const DEATH_ZONE_CONFIG: &str = "[Iron Gate Death Zones]\ntype=deathzone\naddress=8040D0\n";
/// o_ring_M1: pickup when a player collision center is strictly within 14.
pub const RING_PICKUP_DISTANCE: f32 = 14.0;
/// stg11D.rel CONTIRON (section 1 0x20714) registers CCL_INFO at section 5
/// 0x656D8: rectangle with a = b = c = 10 centred 10 above the placement.
pub const CONTAINER_HALF: f32 = 10.0;
const SPRA_VA: u32 = 0x6c4480;
const SPRB_VA: u32 = 0x6c4e90;
const CONTIRON_VA: u32 = 0x6d63f0;
const GOALRING_VA: u32 = 0x6c63c0;
const RING_VA: u32 = 0x6c0f80;
const SAVEPOINT_VA: u32 = 0x6d99a0;
const FRAME_RATE: f32 = 60.0;

/// What the engine would report about player slot 0 this frame.
#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSample {
    /// Player collision centre (the point o_ring_M1 measures from).
    pub center: V3,
    /// Index into `Session::movers` of the object the player stands on.
    pub standing_on: Option<usize>,
    /// Index into `Session::goals` that the CCL reports a player hit on.
    pub goal_hit: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Event {
    Ring {
        layer: usize,
        index: usize,
    },
    LiftStarted {
        mover: usize,
    },
    ElevatorStarted {
        mover: usize,
    },
    BridgeStarted {
        mover: usize,
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
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MoverState {
    Lift(Lift),
    Elevator(Elevator),
    Bridge(Bridge),
}

#[derive(Debug, Clone, Serialize)]
pub struct Mover {
    pub layer: usize,
    pub index: usize,
    pub state: MoverState,
    /// Placement position the collision model was baked at.
    pub origin: V3,
    #[serde(skip)]
    pub solid: PlayWorld,
}

impl Mover {
    pub fn position(&self) -> V3 {
        match &self.state {
            MoverState::Lift(l) => l.position,
            MoverState::Elevator(e) => e.position,
            MoverState::Bridge(b) => b.position,
        }
    }
    pub fn offset(&self) -> V3 {
        sub(self.position(), self.origin)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pickup {
    pub layer: usize,
    pub index: usize,
    pub position: V3,
    pub taken: bool,
}

/// A SET spring. Its ObjectSpring / ObjectSpringB callback is in
/// `spring::StageSpring`; the CCL hit that drives it is engine work, so the
/// session lists springs without running them.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Spring {
    pub layer: usize,
    pub index: usize,
    pub position: V3,
    #[serde(skip)]
    pub task: StageSpring,
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

/// Installed inputs a session is built from (all read by the caller).
pub struct Sources<'a> {
    pub exe: &'a [u8],
    pub stage_module: &'a [u8],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub level: u16,
    pub character: i8,
    /// PhysicsArray row of the dispatched character (values only).
    pub physics_slot: usize,
    #[serde(skip)]
    pub physics: PlayerParameters,
    pub movers_program: MoverProgram,
    pub goal_program_local_effect: u32,
    pub frame: u32,
    pub rings: u32,
    pub score: u32,
    pub outcome: Outcome,
    pub start: V3,
    pub start_yaw: u16,
    pub slot_words: [u32; 2],
    pub movers: Vec<Mover>,
    pub rings_placed: Vec<Pickup>,
    pub checkpoints: Vec<Pickup>,
    pub goals: Vec<Goal>,
    pub springs: Vec<Spring>,
    /// Placements whose native behavior this session does not run.
    pub unimplemented: std::collections::BTreeMap<String, usize>,
    #[serde(skip)]
    pub world: PlayWorld,
    #[serde(skip)]
    death_zones: Vec<Vec<[V3; 3]>>,
}

/// Twelve triangles of an axis-aligned box with the given half sizes.
fn box_tris(center: V3, half: V3) -> Vec<[V3; 3]> {
    let p = |x: f32, y: f32, z: f32| {
        [
            center[0] + x * half[0],
            center[1] + y * half[1],
            center[2] + z * half[2],
        ]
    };
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
            p(-1., -1., 1.),
            p(1., -1., 1.),
            p(1., 1., 1.),
            p(-1., 1., 1.),
        ],
        [
            p(1., -1., -1.),
            p(-1., -1., -1.),
            p(-1., 1., -1.),
            p(1., 1., -1.),
        ],
        [
            p(1., -1., 1.),
            p(1., -1., -1.),
            p(1., 1., -1.),
            p(1., 1., 1.),
        ],
        [
            p(-1., -1., -1.),
            p(-1., -1., 1.),
            p(-1., 1., 1.),
            p(-1., 1., -1.),
        ],
    ];
    quads
        .iter()
        .flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]])
        .collect()
}

impl Session {
    /// Build Iron Gate from a mounted Dark story dispatch.
    pub fn iron_gate(stage: &MountedStage<'_>, src: Sources<'_>) -> Result<Self> {
        if stage.dispatch.level != crate::iron_gate_movers::LEVEL {
            return Err(Error::invalid("this session supports Iron Gate only"));
        }
        let entry = stage
            .entry_pose
            .as_ref()
            .ok_or_else(|| Error::invalid("Iron Gate has no start row"))?;
        let collision = stage
            .collision_world()?
            .ok_or_else(|| Error::invalid("Iron Gate terrain is unavailable"))?;
        let objects = stage
            .object_table
            .ok_or_else(|| Error::invalid("Iron Gate object list is unavailable"))?;
        let slot = match stage.dispatch.character {
            7 => 7usize,
            6 => 6,
            c => return Err(Error::invalid(format!("unsupported character {c}"))),
        };
        let physics = crate::player_physics::read(src.exe)?
            .into_iter()
            .nth(slot)
            .ok_or_else(|| Error::invalid("missing PhysicsArray row"))?
            .parameters;
        let movers_program = MoverProgram::verify(src.stage_module)?;
        let goal_program = goal_ring::Program::read(stage.dispatch.level, src.stage_module)?;
        let lift_solid = read_object_solid(src.exe, LIFT_COLLISION_RVA)?;
        let bridge_solid = read_object_solid(src.exe, BRIDGE_COLLISION_RVA)?;
        let elevator_solid = read_object_solid(src.exe, ELEVATOR_COLLISION_RVA)?;

        let mut world = PlayWorld::from_collision(&collision);
        let mut movers = vec![];
        let mut rings_placed = vec![];
        let mut checkpoints = vec![];
        let mut goals = vec![];
        let mut springs = vec![];
        let mut unimplemented = std::collections::BTreeMap::new();
        for (li, layer) in stage.layers.iter().enumerate() {
            for p in &layer.placements {
                let Some(def) = p.definition_index.and_then(|i| objects.entries.get(i)) else {
                    continue;
                };
                let pl = &p.placement;
                let name = def.name.trim_end();
                let solid = |tris: &[[V3; 3]]| {
                    PlayWorld::from_tris(
                        tris.iter()
                            .map(|t| t.map(|v| place_y(v, pl.rotation[1], pl.position)))
                            .filter_map(|v| make_tri(v, 1, u32::MAX))
                            .collect(),
                    )
                };
                let mut mover = |state: MoverState, solid: PlayWorld| {
                    movers.push(Mover {
                        layer: li,
                        index: p.placement_index,
                        state,
                        origin: pl.position,
                        solid,
                    })
                };
                match (def.function_va, name) {
                    (RING_VA, "RING") => rings_placed.push(Pickup {
                        layer: li,
                        index: p.placement_index,
                        position: pl.position,
                        taken: false,
                    }),
                    (SAVEPOINT_VA, "SAVEPOINT") => {
                        // Listed for reference; Checkpoint_Main is not run here.
                        checkpoints.push(Pickup {
                            layer: li,
                            index: p.placement_index,
                            position: pl.position,
                            taken: false,
                        })
                    }
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
                        springs.push(Spring {
                            layer: li,
                            index: p.placement_index,
                            position: pl.position,
                            task: StageSpring::new(kind, pl.position, angles, pl.parameters),
                        });
                    }
                    (_, "MLIFT") => mover(
                        MoverState::Lift(Lift::new(&movers_program, pl.position, pl.parameters[0])),
                        solid(&lift_solid),
                    ),
                    (_, "ELV") => mover(
                        MoverState::Elevator(Elevator::new(
                            &movers_program,
                            pl.position,
                            pl.parameters,
                            pl.rotation[0],
                        )),
                        solid(&elevator_solid),
                    ),
                    (_, "BRIDGE") => mover(
                        MoverState::Bridge(Bridge::new(
                            &movers_program,
                            pl.position,
                            pl.rotation[1],
                        )),
                        solid(&bridge_solid),
                    ),
                    (CONTIRON_VA, "CONTIRON") => {
                        // The CCL rectangle's own shape; the container's fall
                        // and breaking are not run.
                        let center = [
                            pl.position[0],
                            pl.position[1] + CONTAINER_HALF,
                            pl.position[2],
                        ];
                        for t in box_tris(center, [CONTAINER_HALF; 3]) {
                            if let Some(t) = make_tri(t, 1, u32::MAX) {
                                world.push(t);
                            }
                        }
                        *unimplemented
                            .entry("CONTIRON (shape only)".to_string())
                            .or_insert(0) += 1;
                    }
                    _ => *unimplemented.entry(name.to_string()).or_insert(0) += 1,
                }
            }
        }
        // Only goals that reach the regular contact route are live for Mission 1.
        goals.retain(|g| g.route.starts_with("Contact"));
        if goals.is_empty() {
            return Err(Error::invalid(
                "no regular goal ring reaches its contact route",
            ));
        }
        let tables = death_zones::read(src.exe, DEATH_ZONE_CONFIG)?;
        let mut zones = vec![];
        for table in tables.values() {
            for z in &table.zones {
                zones.push(death_zone_triangles(z)?);
            }
        }
        Ok(Self {
            level: stage.dispatch.level,
            character: stage.dispatch.character,
            physics_slot: slot,
            physics,
            movers_program,
            goal_program_local_effect: goal_program.local_effect,
            frame: 0,
            rings: 0,
            score: 0,
            outcome: Outcome::Playing,
            start: entry.position,
            start_yaw: entry.yaw,
            slot_words: goal_program.initial_slot_words,
            movers,
            rings_placed,
            checkpoints,
            goals,
            springs,
            unimplemented,
            world,
            death_zones: zones,
        })
    }

    pub fn death_zone_count(&self) -> usize {
        self.death_zones.len()
    }

    pub fn death_zone_triangles(&self) -> impl Iterator<Item = &[V3; 3]> {
        self.death_zones.iter().flatten()
    }

    /// Advance one 60 Hz frame of native object rules. `None` means no player
    /// sample (no standing, no contacts).
    pub fn step(&mut self, player: Option<PlayerSample>) -> Vec<Event> {
        let mut events = vec![];
        if self.outcome == Outcome::Cleared {
            return events;
        }
        self.frame += 1;
        let standing = player.and_then(|p| p.standing_on);
        let elevators: Vec<(V3, bool)> = self
            .movers
            .iter()
            .filter_map(|m| match &m.state {
                MoverState::Elevator(e) => Some((e.position, e.mode == 1)),
                _ => None,
            })
            .collect();
        for (i, m) in self.movers.iter_mut().enumerate() {
            let stood = standing == Some(i);
            let p = &self.movers_program;
            let started = match &mut m.state {
                MoverState::Lift(l) => l.step(p, stood).then_some(Event::LiftStarted { mover: i }),
                MoverState::Elevator(e) => e
                    .step(p, stood)
                    .then_some(Event::ElevatorStarted { mover: i }),
                MoverState::Bridge(b) => b
                    .step(p, &elevators)
                    .then_some(Event::BridgeStarted { mover: i }),
            };
            events.extend(started);
        }
        let Some(sample) = player else {
            return events;
        };
        for r in self.rings_placed.iter_mut().filter(|r| !r.taken) {
            if len(sub(r.position, sample.center)) < RING_PICKUP_DISTANCE {
                r.taken = true;
                // Native AddScore(10) and AddNumRing(slot, 1).
                self.score += 10;
                self.rings += 1;
                events.push(Event::Ring {
                    layer: r.layer,
                    index: r.index,
                });
            }
        }
        let local = self.goal_program_local_effect;
        for (gi, g) in self.goals.iter_mut().enumerate() {
            let hit = if sample.goal_hit == Some(gi) {
                goal_ring::Hit::Player { slot: 0 }
            } else {
                goal_ring::Hit::None
            };
            let Ok(calls) = g.contact.step(g.angle_x, 0, hit, local) else {
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
        // The stage task consumes slot word 2 as the goal handoff.
        if self.slot_words[0] == 2 {
            self.outcome = Outcome::Cleared;
            events.push(Event::Cleared { frame: self.frame });
        }
        events
    }

    pub fn elapsed_seconds(&self) -> f32 {
        self.frame as f32 / FRAME_RATE
    }
}
