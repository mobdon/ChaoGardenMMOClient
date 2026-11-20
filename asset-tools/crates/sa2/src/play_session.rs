//! A playable single-player stage session for Iron Gate (Dark story, level 11).
//!
//! Native inputs: the mounted landtable, SET placements, object-list names,
//! the PhysicsArray row of the dispatched character, object collision models
//! and death zones from the Steam executable, Iron Gate's lift/elevator rules
//! from stg11D.rel, the shared ring pickup contract, and the goal-ring
//! initialization and contact branch.
//!
//! Adaptations (no build-matched reference exists for them; see
//! docs/PLAYABLE-IRON-GATE.md): the walker controller's integration order and
//! collision response, checkpoint contact radius, the camera, and treating
//! goal contact plus the stage-module slot word as the mission clear.
use crate::{
    death_zones,
    fan_stage::MountedStage,
    goal_ring,
    iron_gate_movers::{Bridge, Elevator, Lift, MoverProgram},
    play_objects::{death_zone_triangles, inside_volume, place_y, read_object_solid},
    play_world::{add, dot, len, make_tri, scale, sub, Ground, PlayWorld, V3},
    player_physics::PlayerParameters,
    spring::{SpringCall, SpringKind, SpringPlayer, StageSpring},
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
/// GOALRING regular shape radius (two centered spheres, 25 and 8).
pub const GOAL_RADIUS: f32 = 25.0;
/// Adaptation: horizontal checkpoint contact radius.
pub const CHECKPOINT_RADIUS: f32 = 30.0;
/// stg11D.rel spring collision (CCL_INFO at section 5 0xA3574 and 0xA35A4):
/// SPRA is a cylinder of radius 6 and height 10, SPRB a sphere of radius 14,
/// both centred on the placement. The player's own CCL shape lives in the
/// main executable; the walker's radius stands in for it.
pub const SPRA_RADIUS: f32 = 6.0;
pub const SPRA_HEIGHT: f32 = 10.0;
pub const SPRB_RADIUS: f32 = 14.0;
const SPRA_VA: u32 = 0x6c4480;
const SPRB_VA: u32 = 0x6c4e90;
const CONTIRON_VA: u32 = 0x6d63f0;
const CONTCHAO_VA: u32 = 0x6de3b0;
const KASOKU_VA: u32 = 0x6cfbf0;
const CCUBE_VA: u32 = 0x6e54e0;
/// stg11D.rel CCUBE (section 1 0x17140, helper 0x16F94): CCL rectangle whose
/// a, b, c are 10 plus SET Scale x, y, z, centred on the placement; its
/// CCL_INFO at section 5 0x5E5B0 has form 3 and push 0x77.
pub const CCUBE_BASE: f32 = 10.0;
/// stg11D.rel KASOKU (section 1 0x1B4C0): CCL_INFO at section 5 0x60CA0 is a
/// capsule (form 6) of radius 10 and half length 4.44, turned 0x4000 about X.
pub const DASH_RADIUS: f32 = 10.0;
pub const DASH_HALF_LENGTH: f32 = 4.44;
/// Defaults used when Scale.x or Scale.y is not positive (section 4 0x994, 0x998).
pub const DASH_DEFAULT_SPEED: f32 = 14.0;
pub const DASH_DEFAULT_FRAMES: f32 = 60.0;
/// stg11D.rel CONTIRON (section 1 0x20714): CCL_INFO at section 5 0x656D8 is a
/// rectangle with a = b = c = 10 centred 10 above the placement, i.e. a solid
/// 20-unit cube standing on it (Iron Gate stacks them at that pitch). Wooden
/// containers break under the walker's attacks, which live in the main
/// executable, so they are left out rather than left blocking. The iron
/// container's own fall and the Laser Blaster that breaks it are not run.
pub const CONTAINER_EDGE: f32 = 20.0;
const GOALRING_VA: u32 = 0x6c63c0;
const RING_VA: u32 = 0x6c0f80;
const SAVEPOINT_VA: u32 = 0x6d99a0;
const STEP_UP: f32 = 12.0;
const FRAME_RATE: f32 = 60.0;

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Controls {
    /// Stick, x right and y forward, each in -1..=1, relative to `camera_yaw`.
    pub stick: [f32; 2],
    /// Camera heading in radians about +Y (0 looks toward -Z).
    pub camera_yaw: f32,
    pub jump: bool,
}

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
    LiftStarted {
        mover: usize,
    },
    ElevatorStarted {
        mover: usize,
    },
    BridgeStarted {
        mover: usize,
    },
    Spring {
        spring: usize,
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
pub struct Player {
    /// Feet position.
    pub position: V3,
    pub velocity: V3,
    /// Facing heading in radians (0 faces -Z).
    pub yaw: f32,
    pub grounded: bool,
    pub ground_normal: V3,
    pub on_mover: Option<usize>,
    jump_hold: i32,
    /// Frames of ignored input after a spring (native no-control timer).
    pub nocon: i32,
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

/// A SET spring running the verified ObjectSpring / ObjectSpringB callback.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Spring {
    pub layer: usize,
    pub index: usize,
    pub position: V3,
    #[serde(skip)]
    pub task: StageSpring,
    /// Native transformed local vector (0, 5 + Scale.y, 0).
    pub out: V3,
}

impl Spring {
    /// Launch position and velocity this spring gives a slow player standing on it.
    pub fn launch(&self) -> (V3, V3, i32) {
        let t = &self.task;
        match t.kind {
            SpringKind::A => (t.position, self.out, 0),
            SpringKind::B => (
                [
                    t.position[0] + self.out[0],
                    t.position[1] + self.out[1] * 0.8,
                    t.position[2] + self.out[2],
                ],
                rotate_zxy([0.0, t.scale[1] + 5.0, 0.0], t.angles),
                (t.scale[0] as i32).max(0),
            ),
        }
    }
}

/// Twelve triangles of an axis-aligned box centred on the origin.
fn box_tris(half: V3) -> Vec<[V3; 3]> {
    cube_tris(2.0)
        .into_iter()
        .map(|t| t.map(|v| [v[0] * half[0], (v[1] - 1.0) * half[1], v[2] * half[2]]))
        .collect()
}

/// KASOKU dash panel, read from stg11D.rel but not run. On contact with a
/// grounded player it calls the engine with the player slot, local velocity
/// (speed, 0, 0), the panel's rotation and the no-control frame count, then
/// plays sound 0x1003. How the engine turns that local velocity and rotation
/// into world motion is player code in the main executable, which is not
/// readable here, so the session reports panels without applying them.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashPanel {
    pub layer: usize,
    pub index: usize,
    pub position: V3,
    pub angles: [i32; 3],
    /// Scale.x, or 14 when not positive.
    pub speed: f32,
    /// ceil(Scale.y), or 60 when not positive (the call before fctiwz is ceilf
    /// in the reference symbol map once its 0x348 displacement is applied).
    pub frames: i32,
    pub timer: i32,
}

impl DashPanel {
    pub fn launch(&self) -> (V3, i32) {
        (rotate_zxy([self.speed, 0.0, 0.0], self.angles), self.frames)
    }

    /// Capsule contact against the walker's collision centre.
    pub fn touches(&self, center: V3, radius: f32) -> bool {
        let axis = rotate_zxy([0.0, 0.0, 1.0], [0, self.angles[1], 0]);
        let d = sub(center, self.position);
        let t = dot(d, axis).clamp(-DASH_HALF_LENGTH, DASH_HALF_LENGTH);
        len(sub(d, scale(axis, t))) < DASH_RADIUS + radius
    }
}

/// Twelve triangles of a cube standing on the origin (outward winding).
fn cube_tris(edge: f32) -> Vec<[V3; 3]> {
    let h = edge * 0.5;
    let p = |x: f32, y: f32, z: f32| [x * h, (y + 1.0) * h, z * h];
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

pub fn bams_sin_cos(a: i32) -> (f32, f32) {
    (a as f32 * std::f32::consts::TAU / 65536.0).sin_cos()
}

/// Ninja order used by the spring tasks: unit, rotate Z, rotate X, rotate Y,
/// calculate point. The point is rotated by Y first, then X, then Z.
pub fn rotate_zxy(v: V3, angles: [i32; 3]) -> V3 {
    let (sy, cy) = bams_sin_cos(angles[1]);
    let v = [v[0] * cy + v[2] * sy, v[1], -v[0] * sy + v[2] * cy];
    let (sx, cx) = bams_sin_cos(angles[0]);
    let v = [v[0], v[1] * cx - v[2] * sx, v[1] * sx + v[2] * cx];
    let (sz, cz) = bams_sin_cos(angles[2]);
    [v[0] * cz - v[1] * sz, v[0] * sz + v[1] * cz, v[2]]
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Physics {
    pub slot: usize,
    pub run_accel: f32,
    pub rush_speed: f32,
    pub air_accel: f32,
    pub slow_down: f32,
    pub jmp_y_spd: f32,
    pub jmp_addit: f32,
    pub jump2_timer: i32,
    pub gravity: f32,
    pub lim_v_spd: f32,
    pub air_break: f32,

    pub rad: f32,
    pub height: f32,
    pub center_height: f32,
}

impl Physics {
    pub fn from_row(slot: usize, p: &PlayerParameters) -> Self {
        Self {
            slot,
            run_accel: p.run_accel,
            rush_speed: p.rush_speed,
            air_accel: p.air_accel,
            slow_down: p.slow_down.abs(),
            jmp_y_spd: p.jmp_y_spd,
            jmp_addit: p.jmp_addit,
            jump2_timer: p.jump2_timer,
            gravity: p.weight,
            lim_v_spd: p.lim_v_spd,
            air_break: p.air_break.abs(),

            rad: p.rad,
            height: p.height,
            center_height: p.center_height,
        }
    }
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
    pub physics: Physics,
    pub movers_program: MoverProgram,
    pub goal_program_local_effect: u32,
    pub frame: u32,
    pub rings: u32,
    pub score: u32,
    pub deaths: u32,
    pub outcome: Outcome,
    pub player: Player,
    pub start: V3,
    pub start_yaw: f32,
    pub respawn: V3,
    pub slot_words: [u32; 2],
    pub movers: Vec<Mover>,
    pub rings_placed: Vec<Pickup>,
    pub checkpoints: Vec<Pickup>,
    pub goals: Vec<Goal>,
    pub springs: Vec<Spring>,
    pub dash_panels: Vec<DashPanel>,
    /// Placements whose native behavior this session does not run.
    pub unimplemented: std::collections::BTreeMap<String, usize>,
    #[serde(skip)]
    pub world: PlayWorld,
    #[serde(skip)]
    death_zones: Vec<Vec<[V3; 3]>>,
}

impl Session {
    /// Build Iron Gate from a mounted Dark story dispatch.
    pub fn iron_gate(stage: &MountedStage<'_>, src: Sources<'_>) -> Result<Self> {
        if stage.dispatch.level != crate::iron_gate_movers::LEVEL {
            return Err(Error::invalid(
                "the playable session currently supports Iron Gate only",
            ));
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
        let rows = crate::player_physics::read(src.exe)?;
        let physics = Physics::from_row(slot, &rows[slot].parameters);
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
        let mut dash_panels = vec![];
        let mut unimplemented = std::collections::BTreeMap::new();
        for (li, layer) in stage.layers.iter().enumerate() {
            for p in &layer.placements {
                let Some(def) = p.definition_index.and_then(|i| objects.entries.get(i)) else {
                    continue;
                };
                let pl = &p.placement;
                let name = def.name.trim_end();
                let place = |tris: &[[V3; 3]]| -> Vec<[V3; 3]> {
                    tris.iter()
                        .map(|t| t.map(|v| place_y(v, pl.rotation[1], pl.position)))
                        .collect()
                };
                match (def.function_va, name) {
                    (RING_VA, "RING") => rings_placed.push(Pickup {
                        layer: li,
                        index: p.placement_index,
                        position: pl.position,
                        taken: false,
                    }),
                    (SAVEPOINT_VA, "SAVEPOINT") => checkpoints.push(Pickup {
                        layer: li,
                        index: p.placement_index,
                        position: pl.position,
                        taken: false,
                    }),
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
                    (_, "MLIFT") => {
                        let solid = PlayWorld::from_tris(
                            place(&lift_solid)
                                .into_iter()
                                .filter_map(|v| make_tri(v, 1, u32::MAX))
                                .collect(),
                        );
                        movers.push(Mover {
                            layer: li,
                            index: p.placement_index,
                            state: MoverState::Lift(Lift::new(
                                &movers_program,
                                pl.position,
                                pl.parameters[0],
                            )),
                            origin: pl.position,
                            solid,
                        });
                    }
                    (_, "ELV") => {
                        let solid = PlayWorld::from_tris(
                            place(&elevator_solid)
                                .into_iter()
                                .filter_map(|v| make_tri(v, 1, u32::MAX))
                                .collect(),
                        );
                        movers.push(Mover {
                            layer: li,
                            index: p.placement_index,
                            state: MoverState::Elevator(Elevator::new(
                                &movers_program,
                                pl.position,
                                pl.parameters,
                                pl.rotation[0],
                            )),
                            origin: pl.position,
                            solid,
                        });
                    }
                    (CCUBE_VA, "CCUBE") => {
                        let half = pl.parameters.map(|s| CCUBE_BASE + s);
                        for t in box_tris(half) {
                            if let Some(t) = make_tri(
                                t.map(|v| place_y(v, pl.rotation[1], pl.position)),
                                1,
                                u32::MAX,
                            ) {
                                world.push(t);
                            }
                        }
                    }
                    (KASOKU_VA, "KASOKU") => {
                        let [sx, sy, _] = pl.parameters;
                        let speed = if sx > 0.0 { sx } else { DASH_DEFAULT_SPEED };
                        let frames = if sy > 0.0 { sy } else { DASH_DEFAULT_FRAMES };
                        dash_panels.push(DashPanel {
                            layer: li,
                            index: p.placement_index,
                            position: pl.position,
                            angles: pl.rotation.map(i32::from),
                            speed,
                            frames: frames.ceil() as i32,
                            timer: 0,
                        });
                    }
                    (CONTIRON_VA, "CONTIRON") | (CONTCHAO_VA, "CONTCHAO") => {
                        for t in cube_tris(CONTAINER_EDGE) {
                            if let Some(t) = make_tri(
                                t.map(|v| place_y(v, pl.rotation[1], pl.position)),
                                1,
                                u32::MAX,
                            ) {
                                world.push(t);
                            }
                        }
                        *unimplemented
                            .entry(format!("{name} (solid, not breakable)"))
                            .or_insert(0) += 1;
                    }
                    (_, "BRIDGE") => {
                        let solid = PlayWorld::from_tris(
                            place(&bridge_solid)
                                .into_iter()
                                .filter_map(|v| make_tri(v, 1, u32::MAX))
                                .collect(),
                        );
                        movers.push(Mover {
                            layer: li,
                            index: p.placement_index,
                            state: MoverState::Bridge(Bridge::new(
                                &movers_program,
                                pl.position,
                                pl.rotation[1],
                            )),
                            origin: pl.position,
                            solid,
                        });
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
        let start_yaw = entry.yaw as f32 * std::f32::consts::TAU / 65536.0;
        let mut s = Self {
            level: stage.dispatch.level,
            character: stage.dispatch.character,
            physics,
            movers_program,
            goal_program_local_effect: goal_program.local_effect,
            frame: 0,
            rings: 0,
            score: 0,
            deaths: 0,
            outcome: Outcome::Playing,
            player: Player {
                position: entry.position,
                velocity: [0.0; 3],
                yaw: start_yaw,
                grounded: false,
                ground_normal: [0.0, 1.0, 0.0],
                on_mover: None,
                jump_hold: 0,
                nocon: 0,
            },
            start: entry.position,
            start_yaw,
            respawn: entry.position,
            slot_words: goal_program.initial_slot_words,
            movers,
            rings_placed,
            checkpoints,
            goals,
            springs,
            dash_panels,
            unimplemented,
            world,
            death_zones: zones,
        };
        s.settle();
        Ok(s)
    }

    /// Static terrain plus every mover's solid at its current position.
    pub fn snapshot_world(&self) -> PlayWorld {
        let mut w = self.world.clone();
        for m in &self.movers {
            let o = m.offset();
            for t in &m.solid.tris {
                if let Some(t) = make_tri(t.v.map(|v| add(v, o)), t.flags, t.mesh) {
                    w.push(t);
                }
            }
        }
        w
    }

    /// Peak height of a held jump under this controller (frames integrate as in `control`).
    pub fn jump_height(&self) -> f32 {
        let ph = &self.physics;
        let (mut vy, mut h, mut hold) = (ph.jmp_y_spd, 0.0, ph.jump2_timer);
        while vy > 0.0 {
            h += vy;
            if hold > 0 {
                vy += ph.jmp_addit;
                hold -= 1;
            }
            vy -= ph.gravity;
        }
        h
    }

    pub fn death_zone_count(&self) -> usize {
        self.death_zones.len()
    }

    pub fn death_zone_triangles(&self) -> impl Iterator<Item = &[V3; 3]> {
        self.death_zones.iter().flatten()
    }

    /// Restart as the game does after a miss once checkpoint `index` (in
    /// `checkpoints` order) has been passed: the player stands at it.
    pub fn restart_from_checkpoint(&mut self, index: usize) -> Result<()> {
        let c = self
            .checkpoints
            .get_mut(index)
            .ok_or_else(|| Error::invalid("no such checkpoint"))?;
        c.taken = true;
        self.respawn = c.position;
        self.player.position = c.position;
        self.player.velocity = [0.0; 3];
        self.player.on_mover = None;
        self.settle();
        Ok(())
    }

    /// Snap the player onto the ground below the start/respawn point.
    fn settle(&mut self) {
        let p = self.player.position;
        if let Some((g, m)) = self.ground([p[0], p[1] + 20.0, p[2]], 400.0) {
            self.player.position[1] = g.y;
            self.player.grounded = true;
            self.player.ground_normal = g.normal;
            self.player.on_mover = m;
        }
    }

    fn ground(&self, p: V3, depth: f32) -> Option<(Ground, Option<usize>)> {
        let mut best = self.world.ground_below(p, depth).map(|g| (g, None));
        for (i, m) in self.movers.iter().enumerate() {
            let o = m.offset();
            if let Some(mut g) = m.solid.ground_below(sub(p, o), depth) {
                g.y += o[1];
                // A mover flush with the floor carries the player.
                if best.is_none_or(|(b, _)| g.y >= b.y - 0.05) {
                    best = Some((g, Some(i)));
                }
            }
        }
        best
    }

    fn ceiling(&self, p: V3, reach: f32) -> Option<f32> {
        let mut best = self.world.ceiling_above(p, reach);
        for m in &self.movers {
            let o = m.offset();
            if let Some(y) = m.solid.ceiling_above(sub(p, o), reach) {
                let y = y + o[1];
                if best.is_none_or(|b| y < b) {
                    best = Some(y);
                }
            }
        }
        best
    }

    fn push_out(&self, c: V3, r: f32) -> (V3, Option<V3>) {
        let (mut total, mut normal) = self.world.push_out(c, r, 4);
        for m in &self.movers {
            let o = m.offset();
            let (d, n) = m.solid.push_out(sub(add(c, total), o), r, 2);
            if d != [0.0; 3] {
                total = add(total, d);
                normal = n;
            }
        }
        (total, normal)
    }

    pub fn player_center(&self) -> V3 {
        add(self.player.position, [0.0, self.physics.center_height, 0.0])
    }

    /// Advance one 60 Hz frame.
    pub fn step(&mut self, c: Controls) -> Vec<Event> {
        let mut events = vec![];
        if self.outcome == Outcome::Cleared {
            return events;
        }
        self.frame += 1;
        // 1. Movers update from last frame's standing state and carry the rider.
        let riding = self.player.on_mover.filter(|_| self.player.grounded);
        let elevators: Vec<(V3, bool)> = self
            .movers
            .iter()
            .filter_map(|m| match &m.state {
                MoverState::Elevator(e) => Some((e.position, e.mode == 1)),
                _ => None,
            })
            .collect();
        for i in 0..self.movers.len() {
            let before = self.movers[i].position();
            let stood = riding == Some(i);
            let started = match &mut self.movers[i].state {
                MoverState::Lift(l) => l
                    .step(&self.movers_program, stood)
                    .then_some(Event::LiftStarted { mover: i }),
                MoverState::Elevator(e) => e
                    .step(&self.movers_program, stood)
                    .then_some(Event::ElevatorStarted { mover: i }),
                MoverState::Bridge(b) => b
                    .step(&self.movers_program, &elevators)
                    .then_some(Event::BridgeStarted { mover: i }),
            };
            events.extend(started);
            if stood {
                let d = sub(self.movers[i].position(), before);
                self.player.position = add(self.player.position, d);
            }
        }
        // 2. Controller (adaptation, parameters from PhysicsArray).
        let mut pl = self.player.clone();
        self.control(&mut pl, c);
        // 3. Integrate with collision.
        self.integrate(&mut pl);
        self.player = pl;
        // 4. Objects.
        self.springs(&mut events);
        self.objects(&mut events);
        events
    }

    /// Free flight (no input) from a launch state until landing; used by the
    /// offline planner to find where a spring delivers the player.
    pub fn predict_landing(
        &self,
        position: V3,
        velocity: V3,
        nocon: i32,
        max_frames: u32,
    ) -> Option<V3> {
        self.predict_flight(position, velocity, nocon, Controls::default(), max_frames)
    }

    /// Feet positions of a free flight, one per frame, until landing.
    pub fn flight_path(
        &self,
        position: V3,
        velocity: V3,
        nocon: i32,
        controls: Controls,
        max_frames: u32,
    ) -> Vec<V3> {
        let mut pl = self.player.clone();
        pl.position = position;
        pl.velocity = velocity;
        pl.grounded = false;
        pl.on_mover = None;
        pl.nocon = nocon;
        pl.jump_hold = 0;
        let mut out = vec![];
        for f in 0..max_frames {
            self.control(&mut pl, controls);
            self.integrate(&mut pl);
            out.push(pl.position);
            if (pl.grounded && f > 2) || pl.position[1] < self.world.min[1] - 300.0 {
                break;
            }
        }
        out
    }

    /// As `predict_landing`, holding `controls` every frame once input is allowed.
    pub fn predict_flight(
        &self,
        position: V3,
        velocity: V3,
        nocon: i32,
        controls: Controls,
        max_frames: u32,
    ) -> Option<V3> {
        let mut pl = self.player.clone();
        pl.position = position;
        pl.velocity = velocity;
        pl.grounded = false;
        pl.on_mover = None;
        pl.nocon = nocon;
        pl.jump_hold = 0;
        for f in 0..max_frames {
            self.control(&mut pl, controls);
            self.integrate(&mut pl);
            if pl.grounded && f > 2 && f as i32 >= nocon {
                return Some(pl.position);
            }
            if pl.position[1] < self.world.min[1] - 300.0 {
                return None;
            }
        }
        None
    }

    fn control(&self, pl: &mut Player, c: Controls) {
        let ph = &self.physics;
        let c = if pl.nocon > 0 {
            pl.nocon -= 1;
            Controls {
                stick: [0.0; 2],
                jump: false,
                ..c
            }
        } else {
            c
        };
        let mag = (c.stick[0] * c.stick[0] + c.stick[1] * c.stick[1])
            .sqrt()
            .min(1.0);
        // Camera-relative world direction. Forward is -Z at camera yaw 0.
        let (s, co) = c.camera_yaw.sin_cos();
        let fwd = [-s, 0.0, -co];
        let right = [co, 0.0, -s];
        let mut dir = add(scale(right, c.stick[0]), scale(fwd, c.stick[1]));
        let dl = len(dir);
        if dl > 1e-4 {
            dir = scale(dir, 1.0 / dl);
        }
        let mut h = [pl.velocity[0], 0.0, pl.velocity[2]];
        let speed = len(h);
        if mag > 0.05 && dl > 1e-4 {
            let target_yaw = (-dir[0]).atan2(-dir[2]);
            let mut dy = target_yaw - pl.yaw;
            while dy > std::f32::consts::PI {
                dy -= std::f32::consts::TAU;
            }
            while dy < -std::f32::consts::PI {
                dy += std::f32::consts::TAU;
            }
            let turn = if pl.grounded { 0.25 } else { 0.08 };
            pl.yaw += dy.clamp(-turn, turn);
            let accel = if pl.grounded {
                ph.run_accel
            } else {
                ph.air_accel
            };
            let target = ph.rush_speed * mag;
            let facing = [-pl.yaw.sin(), 0.0, -pl.yaw.cos()];
            let new_speed = if speed < target {
                (speed + accel).min(target)
            } else if pl.grounded {
                (speed - ph.slow_down).max(target)
            } else {
                speed
            };
            // Ground velocity follows the facing. In the air the stick pushes
            // by air_accel, and the part of the velocity opposing it is
            // braked by air_break; speed never grows past max(current, target).
            h = if pl.grounded {
                scale(facing, new_speed)
            } else {
                let along = dot(h, dir);
                let mut v = add(h, scale(dir, ph.air_accel));
                if along < 0.0 {
                    let brake = (-along).min(ph.air_break);
                    v = add(v, scale(dir, brake));
                }
                let cap = speed.max(target);
                let vl = len(v);
                if vl > cap && vl > 1e-4 {
                    scale(v, cap / vl)
                } else {
                    v
                }
            };
        } else if pl.grounded {
            let ns = (speed - ph.slow_down).max(0.0);
            h = if speed > 1e-4 {
                scale(h, ns / speed)
            } else {
                [0.0; 3]
            };
        }

        pl.velocity[0] = h[0];
        pl.velocity[2] = h[2];
        if c.jump && pl.grounded {
            pl.velocity[1] = ph.jmp_y_spd;
            pl.grounded = false;
            pl.on_mover = None;
            pl.jump_hold = ph.jump2_timer;
        } else if !pl.grounded {
            if c.jump && pl.jump_hold > 0 && pl.velocity[1] > 0.0 {
                pl.velocity[1] += ph.jmp_addit;
                pl.jump_hold -= 1;
            } else {
                pl.jump_hold = 0;
            }
            pl.velocity[1] = (pl.velocity[1] - ph.gravity).max(-ph.lim_v_spd);
        }
    }

    fn integrate(&self, pl: &mut Player) {
        let ph = &self.physics;
        let mut pos = pl.position;
        let vel = pl.velocity;
        // Sub-step so fast moves cannot skip thin floors or walls.
        let steps = ((len(vel) / (ph.rad * 0.5)).ceil() as usize).clamp(1, 16);
        let mut grounded = false;
        let mut ground_normal = [0.0, 1.0, 0.0];
        let mut on_mover = None;
        let mut vy = vel[1];
        for _ in 0..steps {
            let f = 1.0 / steps as f32;
            pos[0] += vel[0] * f;
            pos[2] += vel[2] * f;
            // Walls: sphere above the step height.
            let center = [pos[0], pos[1] + STEP_UP * 0.5 + ph.rad, pos[2]];
            let (d, _) = self.push_out(center, ph.rad);
            pos[0] += d[0];
            pos[2] += d[2];
            // Vertical.
            if vy > 0.0 {
                let top = pos[1] + ph.height;
                if let Some(ceil) = self.ceiling([pos[0], top - 1.0, pos[2]], vy * f + 1.0) {
                    pos[1] = ceil - ph.height - 0.01;
                    vy = 0.0;
                } else {
                    pos[1] += vy * f;
                }
            } else {
                let probe_top = pos[1] + STEP_UP;
                let snap = if pl.grounded && vy <= 0.0 {
                    STEP_UP
                } else {
                    0.0
                };
                let depth = STEP_UP + snap + (-vy * f);
                match self.ground([pos[0], probe_top, pos[2]], depth) {
                    Some((g, m)) if g.y >= pos[1] + vy * f - snap - 0.01 => {
                        pos[1] = g.y;
                        grounded = true;
                        ground_normal = g.normal;
                        on_mover = m;
                        vy = 0.0;
                    }
                    _ => pos[1] += vy * f,
                }
            }
        }
        if grounded {
            vy = 0.0;
        }
        pl.position = pos;
        pl.velocity[1] = vy;
        pl.grounded = grounded;
        pl.ground_normal = ground_normal;
        pl.on_mover = if grounded { on_mover } else { None };
    }

    /// Everything that throws the player: (where to stand, start, velocity,
    /// no-control frames).
    pub fn launchers(&self) -> Vec<(V3, V3, V3, i32)> {
        self.springs
            .iter()
            .map(|sp| {
                let (pos, vel, nocon) = sp.launch();
                (sp.position, pos, vel, nocon)
            })
            .collect()
    }

    fn springs(&mut self, events: &mut Vec<Event>) {
        let center = self.player_center();
        let rad = self.physics.rad;
        let pl = &mut self.player;
        let running = (pl.velocity[0] * pl.velocity[0] + pl.velocity[2] * pl.velocity[2]).sqrt();
        for (i, sp) in self.springs.iter_mut().enumerate() {
            let d = sub(center, sp.task.position);
            let near = match sp.task.kind {
                SpringKind::A => {
                    d[0].hypot(d[2]) < SPRA_RADIUS + rad && d[1] > -rad && d[1] < SPRA_HEIGHT + rad
                }
                SpringKind::B => len(d) < SPRB_RADIUS + rad,
            };
            let player = SpringPlayer {
                position: pl.position,
                running_speed: running,
                item: 0,
            };
            let calls = sp
                .task
                .step(near.then_some(0u8), Some(player), sp.out, bams_sin_cos);
            for call in calls {
                match call {
                    SpringCall::SetPosition(0, p) => pl.position = p,
                    SpringCall::SpringVelocity(0, v) => {
                        pl.velocity = v;
                        pl.grounded = false;
                        pl.on_mover = None;
                        events.push(Event::Spring { spring: i });
                    }
                    SpringCall::VelocityYRotationNoControl {
                        slot: 0,
                        velocity_y,
                        angles,
                        frames,
                    } => {
                        pl.velocity = rotate_zxy([0.0, velocity_y, 0.0], angles);
                        pl.nocon = frames;
                        pl.grounded = false;
                        pl.on_mover = None;
                        events.push(Event::Spring { spring: i });
                    }
                    _ => {}
                }
            }
        }
    }

    fn objects(&mut self, events: &mut Vec<Event>) {
        let center = self.player_center();
        let feet = self.player.position;
        for r in self.rings_placed.iter_mut().filter(|r| !r.taken) {
            if len(sub(r.position, center)) < RING_PICKUP_DISTANCE {
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
        for c in self.checkpoints.iter_mut().filter(|c| !c.taken) {
            let d = [c.position[0] - feet[0], c.position[2] - feet[2]];
            if (d[0] * d[0] + d[1] * d[1]).sqrt() < CHECKPOINT_RADIUS
                && (c.position[1] - feet[1]).abs() < 40.0
            {
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
        // The stage task consumes slot word 2 as the goal handoff.
        if self.slot_words[0] == 2 {
            self.outcome = Outcome::Cleared;
            events.push(Event::Cleared { frame: self.frame });
            return;
        }
        let zone = self.death_zones.iter().any(|z| inside_volume(z, center));
        if zone || self.player.position[1] < self.world.min[1] - 300.0 {
            self.deaths += 1;
            events.push(Event::Death {
                frame: self.frame,
                at: self.player.position,
                zone,
            });
            self.player.position = self.respawn;
            self.player.velocity = [0.0; 3];
            self.player.on_mover = None;
            self.rings = 0;
            self.settle();
            events.push(Event::Respawn { frame: self.frame });
        }
    }

    pub fn elapsed_seconds(&self) -> f32 {
        self.frame as f32 / FRAME_RATE
    }
}
