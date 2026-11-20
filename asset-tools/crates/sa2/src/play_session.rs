//! Native stage-object rules for the walker stages Iron Gate (Dark story,
//! level 11) and Prison Lane (Hero story, level 9).
//!
//! Everything here is read from the installed game files: the mounted
//! landtable and SET placements, object-list names, the PhysicsArray row of
//! the dispatched character, object collision models and death zones from
//! the Steam executable, each stage's lift/elevator/bridge rules and object
//! collision rows from its stage module (`StageRules`), the o_ring pickup
//! contract and the goal-ring initialization and contact branch.
//!
//! The session does not move a player. The walker's own movement, collision
//! response, lock-on and laser are main-program code, which is encrypted on
//! disk in the Steam executable, so a host supplies the player's collision
//! centre and the mover it stands on each frame (`PlayerSample`), and the
//! session runs the native object rules against it.
use crate::{
    death_zones,
    fan_stage::MountedStage,
    goal_ring,
    iron_gate_movers::{self as movers, Bridge, Elevator, Lift, MoverLayout, MoverProgram},
    play_objects::{collision_rows, death_zone_triangles, place_y, read_object_solid},
    play_world::{add, len, make_tri, sub, PlayWorld, V3},
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

/// Per-stage native inputs a walker session is built from.
pub struct StageRules {
    pub name: &'static str,
    pub movers: &'static MoverLayout,
    /// Basic collision model addresses (SA Tools PC_SA2/STGnn.ini).
    pub lift_collision: u32,
    pub bridge_collision: u32,
    pub elevator_collision: u32,
    /// Other solid stage objects: (object-list name, Basic model address).
    pub static_solids: &'static [(&'static str, u32)],
    /// Objects whose solidity is CollisionData rows in the stage module's
    /// section 5: (object-list name, row offset, row count).
    pub collision_boxes: &'static [(&'static str, u32, usize)],
    /// Build CCUBE placements as solid boxes (rule read from this stage's module).
    pub invisible_cubes: bool,
    /// Expand RING_LINEAR placements into collectable rings.
    pub ring_lines: bool,
    pub death_zone_config: &'static str,
}

pub const IRON_GATE: StageRules = StageRules {
    name: "Iron Gate",
    movers: &movers::IRON_GATE,
    lift_collision: LIFT_COLLISION_RVA,
    bridge_collision: BRIDGE_COLLISION_RVA,
    elevator_collision: ELEVATOR_COLLISION_RVA,
    static_solids: &[],
    // stg11D.rel 0x20714 (CONTIRON) registers this row through 0x8000F7D8.
    collision_boxes: &[("CONTIRON", 0x656d8, 1)],
    invisible_cubes: false,
    ring_lines: false,
    death_zone_config: DEATH_ZONE_CONFIG,
};

/// PC_SA2/STG09.ini (Prison Lane, Hero story, Tails' walker).
pub const PRISON_LANE: StageRules = StageRules {
    name: "Prison Lane",
    movers: &movers::PRISON_LANE,
    lift_collision: 0xc3c538,
    bridge_collision: 0xc42ea0,
    elevator_collision: 0xc39660,
    // stg09D.rel 0x2A4C: PANEL hands its Basic model to the shared
    // static-collision helper at 0x9A4C with flags 6 (bit 0 clear: the
    // placement's X and Z rotations are dropped, Y is kept).
    static_solids: &[("PANEL", 0xc40390)],
    // stg09D.rel 0x2A94 (FENCES) and 0x2AF8 (FENCEL) register these rows
    // through the helper at 0x2B5C (InitCollision 0x8000F8B8, flags 4).
    // stg09D.rel 0x1978C (CONTIRON) registers its row through 0x8000F7D8.
    collision_boxes: &[
        ("FENCES", 0x3c54c, 1),
        ("FENCEL", 0x3c57c, 3),
        ("CONTIRON", 0x52a18, 1),
    ],
    invisible_cubes: true,
    ring_lines: true,
    death_zone_config: "[Prison Lane Death Zones]\ntype=deathzone\naddress=C4C6E4\n",
};
/// o_ring_M1: pickup when a player collision center is strictly within 14.
pub const RING_PICKUP_DISTANCE: f32 = 14.0;
/// GOALRING regular shape radius (two centered spheres, 25 and 8).
pub const GOAL_RADIUS: f32 = 25.0;
const SPRA_VA: u32 = 0x6c4480;
const SPRB_VA: u32 = 0x6c4e90;
const CCUBE_VA: u32 = 0x6e54e0;
/// stg09D.rel 0x11B30 (shared by SPHERE, CCYL, CCUBE, CWALL): each of the
/// shape's a, b, c sizes is 10 (sec4+0x6B0) plus the matching Scale
/// component. CCUBE's CollisionData (sec5+0x4C080) is form 3, centre 0.
pub const CCUBE_BASE: f32 = 10.0;
const GOALRING_VA: u32 = 0x6c63c0;
const RING_LINEAR_VA: u32 = 0x6c5ed0;
/// RING_LINEAR spacing base (stg09D.rel sec4+0xA44), added to Scale.x.
pub const RING_LINE_BASE: f32 = 10.0;
const RING_VA: u32 = 0x6c0f80;
const SAVEPOINT_VA: u32 = 0x6d99a0;
const FRAME_RATE: f32 = 60.0;

/// What the host reports about the player this frame. The session reads it
/// and never moves the player.
#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSample {
    /// Collision centre (feet plus the PhysicsArray center_height).
    pub center: V3,
    /// The engine's "player standing on this object" answer, as an index
    /// into `Session::movers`.
    pub standing_on: Option<usize>,
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

/// Twelve triangles of a box with half-extents `h` centred on `c`.
fn box_tris(h: V3, c: V3) -> Vec<[V3; 3]> {
    let p = |x: f32, y: f32, z: f32| [c[0] + x * h[0], c[1] + y * h[1], c[2] + z * h[2]];
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

/// Whether a CCUBE's box is the same however its rotation is applied:
/// whether CollisionData form 3 uses the task rotation at all is not read,
/// so only boxes with no X/Z rotation and a yaw of 0 or 0x8000 (a half turn
/// maps a centred box onto itself) are built.
pub fn cube_rotation_is_exact(rotation: [u16; 3]) -> bool {
    rotation[0] == 0 && rotation[2] == 0 && rotation[1] & 0x7fff == 0
}

/// Whether a RING_LINEAR line is placed exactly from read rules alone: with
/// Scale.y 0 the trig call (0x80112050, sine or cosine) drops out, and with
/// no X/Z rotation only rotate Y (0x801154E0, the goal ring's spin call)
/// is applied.
pub fn ring_line_is_exact(rotation: [u16; 3], scale: V3) -> bool {
    rotation[0] == 0 && rotation[2] == 0 && scale[1] == 0.0
}

/// RING_LINEAR (stg09D.rel 0x1B448 with helpers 0x1B1E8 and 0x1B21C) for a
/// line that `ring_line_is_exact` accepts: the ring count is Scale.z
/// truncated and clamped to 1..=8, and ring i sits at local
/// (0, 0, -i * (10 + Scale.x)) turned by the placement's Y rotation.
pub fn ring_line(position: V3, yaw_bams: u16, scale: V3) -> Vec<V3> {
    let n = (scale[2] as i32).clamp(1, 8);
    (0..n)
        .map(|i| {
            let local = [0.0, 0.0, -(i as f32) * (RING_LINE_BASE + scale[0])];
            add(position, rotate_zxy(local, [0, i32::from(yaw_bams), 0]))
        })
        .collect()
}

#[cfg(test)]
mod ring_line_tests {
    use super::*;

    #[test]
    fn ring_line_spacing_count_and_yaw() {
        let r = ring_line([0.0, 0.0, 0.0], 0x4000, [15.0, 0.0, 3.0]);
        assert_eq!(r.len(), 3);
        // Local -Z turned a quarter about Y points along -X; spacing 25.
        assert!((r[1][0] + 25.0).abs() < 1e-3 && r[1][2].abs() < 1e-3);
        assert!((r[2][0] + 50.0).abs() < 1e-3 && r[2][1] == 0.0);
        assert_eq!(ring_line([0.0; 3], 0, [20.0, 0.0, 0.2]).len(), 1);
        assert_eq!(ring_line([0.0; 3], 0, [20.0, 0.0, 12.0]).len(), 8);
        // Lines needing the unread trig or X/Z axis calls are not built.
        assert!(!ring_line_is_exact([0, 0x4000, 0], [15.5, 4.0, 3.0]));
        assert!(!ring_line_is_exact([1, 0, 192], [20.0, 0.0, 5.0]));
        assert!(cube_rotation_is_exact([0, 0x8000, 0]));
        assert!(!cube_rotation_is_exact([0, 0x4000, 0]));
    }
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
    pub outcome: Outcome,
    /// Single-player start row (feet position and BAMS yaw).
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

impl Session {
    /// Build Iron Gate from a mounted Dark story dispatch.
    pub fn iron_gate(stage: &MountedStage<'_>, src: Sources<'_>) -> Result<Self> {
        Self::walker_stage(stage, src, &IRON_GATE)
    }

    /// Build Prison Lane from a mounted Hero story dispatch.
    pub fn prison_lane(stage: &MountedStage<'_>, src: Sources<'_>) -> Result<Self> {
        Self::walker_stage(stage, src, &PRISON_LANE)
    }

    /// The rules for a mounted level, if it is a supported walker stage.
    pub fn rules_for(level: u16) -> Option<&'static StageRules> {
        [&IRON_GATE, &PRISON_LANE]
            .into_iter()
            .find(|r| r.movers.level == level)
    }

    /// Build a supported walker stage from its mounted dispatch.
    pub fn walker_stage(
        stage: &MountedStage<'_>,
        src: Sources<'_>,
        rules: &StageRules,
    ) -> Result<Self> {
        if stage.dispatch.level != rules.movers.level {
            return Err(Error::invalid(format!(
                "level {} is not {}",
                stage.dispatch.level, rules.name
            )));
        }
        let entry = stage
            .entry_pose
            .as_ref()
            .ok_or_else(|| Error::invalid("stage has no start row"))?;
        let collision = stage
            .collision_world()?
            .ok_or_else(|| Error::invalid("stage terrain is unavailable"))?;
        let objects = stage
            .object_table
            .ok_or_else(|| Error::invalid("stage object list is unavailable"))?;
        let slot = match stage.dispatch.character {
            7 => 7usize,
            6 => 6,
            c => return Err(Error::invalid(format!("unsupported character {c}"))),
        };
        let rows = crate::player_physics::read(src.exe)?;
        let physics = Physics::from_row(slot, &rows[slot].parameters);
        let movers_program = MoverProgram::verify_layout(rules.movers, src.stage_module)?;
        let goal_program = goal_ring::Program::read(stage.dispatch.level, src.stage_module)?;
        let lift_solid = read_object_solid(src.exe, rules.lift_collision)?;
        let bridge_solid = read_object_solid(src.exe, rules.bridge_collision)?;
        let elevator_solid = read_object_solid(src.exe, rules.elevator_collision)?;
        let static_solids = rules
            .static_solids
            .iter()
            .map(|&(n, rva)| Ok((n, read_object_solid(src.exe, rva)?)))
            .collect::<Result<Vec<_>>>()?;
        let collision_boxes = rules
            .collision_boxes
            .iter()
            .map(|&(n, offset, count)| {
                Ok((
                    n,
                    collision_rows(src.stage_module, rules.movers.module_file, offset, count)?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;

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
                    (RING_LINEAR_VA, "RING_LINEAR")
                        if rules.ring_lines && ring_line_is_exact(pl.rotation, pl.parameters) =>
                    {
                        for position in ring_line(pl.position, pl.rotation[1], pl.parameters) {
                            rings_placed.push(Pickup {
                                layer: li,
                                index: p.placement_index,
                                position,
                                taken: false,
                            });
                        }
                    }
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
                    (CCUBE_VA, "CCUBE")
                        if rules.invisible_cubes && cube_rotation_is_exact(pl.rotation) =>
                    {
                        // Invisible cube collision: CollisionData form 3
                        // centred on the placement; the shared helper sets
                        // its half-extents to CCUBE_BASE + Scale. Only boxes
                        // whose shape does not depend on the unread rotation
                        // rule are built (see `cube_rotation_is_exact`).
                        let h = pl.parameters.map(|s| s + CCUBE_BASE);
                        for t in box_tris(h, [0.0; 3]) {
                            if let Some(t) = make_tri(t.map(|v| add(v, pl.position)), 1, u32::MAX) {
                                world.push(t);
                            }
                        }
                        *unimplemented
                            .entry(format!("{name} (solid only)"))
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
                    (_, n) if static_solids.iter().any(|(s, _)| *s == n) => {
                        let (_, tris) = static_solids.iter().find(|(s, _)| *s == n).unwrap();
                        for t in place(tris) {
                            if let Some(t) = make_tri(t, 1, u32::MAX) {
                                world.push(t);
                            }
                        }
                        *unimplemented
                            .entry(format!("{name} (solid only)"))
                            .or_insert(0) += 1;
                    }
                    (_, n) if collision_boxes.iter().any(|(s, _)| *s == n) => {
                        let (_, rows) = collision_boxes.iter().find(|(s, _)| *s == n).unwrap();
                        for row in rows {
                            // Form 3 boxes with no row angles, whose shape is
                            // the same however the placement rotation is
                            // applied (see `cube_rotation_is_exact`).
                            let on_axis = row.center[0] == 0.0 && row.center[2] == 0.0;
                            // A box square in XZ about the Y axis is also
                            // unchanged by quarter turns.
                            let quarter = on_axis
                                && row.size[0] == row.size[2]
                                && pl.rotation[0] == 0
                                && pl.rotation[2] == 0
                                && pl.rotation[1] & 0x3fff == 0;
                            let exact = row.form == 3
                                && row.angles == [0; 3]
                                && (quarter
                                    || (cube_rotation_is_exact(pl.rotation)
                                        && (pl.rotation[1] == 0 || on_axis)));
                            if !exact {
                                *unimplemented
                                    .entry(format!("{name} row (form {}): rule not read", row.form))
                                    .or_insert(0) += 1;
                                continue;
                            }
                            let h = [row.size[0], row.size[1], row.size[2]];
                            for t in box_tris(h, row.center) {
                                if let Some(t) =
                                    make_tri(t.map(|v| add(v, pl.position)), 1, u32::MAX)
                                {
                                    world.push(t);
                                }
                            }
                            *unimplemented
                                .entry(format!("{name} row (solid only)"))
                                .or_insert(0) += 1;
                        }
                    }
                    (CCUBE_VA, "CCUBE") if rules.invisible_cubes => {
                        *unimplemented
                            .entry(format!("{name} (rotated: rotation rule not read)"))
                            .or_insert(0) += 1;
                    }
                    (RING_LINEAR_VA, "RING_LINEAR") if rules.ring_lines => {
                        *unimplemented
                            .entry(format!(
                                "{name} (arched or tilted: trig/axis calls not read)"
                            ))
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
        let tables = death_zones::read(src.exe, rules.death_zone_config)?;
        let mut zones = vec![];
        for table in tables.values() {
            for z in &table.zones {
                zones.push(death_zone_triangles(z)?);
            }
        }
        Ok(Self {
            level: stage.dispatch.level,
            character: stage.dispatch.character,
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

    pub fn death_zone_count(&self) -> usize {
        self.death_zones.len()
    }

    /// Native death-zone volumes (what entering one does is not run here).
    pub fn death_zone_triangles(&self) -> impl Iterator<Item = &[V3; 3]> {
        self.death_zones.iter().flatten()
    }

    /// Advance one 60 Hz frame of the native object rules.
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
        for i in 0..self.movers.len() {
            let stood = standing == Some(i);
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
        }
        if let Some(p) = player {
            self.objects(p.center, &mut events);
        }
        events
    }

    fn objects(&mut self, center: V3, events: &mut Vec<Event>) {
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
        }
    }

    pub fn elapsed_seconds(&self) -> f32 {
        self.frame as f32 / FRAME_RATE
    }
}
