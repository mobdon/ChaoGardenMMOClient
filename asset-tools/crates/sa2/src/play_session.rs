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
    ninja_trig::{nj_rotate_sin_cos, nj_sin},
    play_objects::{
        collision_rows, death_zone_triangles, place_y, read_object_solid, CollisionRow,
    },
    play_world::{add, len, make_tri, sub, PlayWorld, V3},
    player_physics::PlayerParameters,
    prison_doors::{
        enemy_counts, DoorSound, ElevatorWall, Enemy, PrisonDoor, DOOR_KINDS, WALL_ROWS,
        WALL_ROW_COUNT,
    },
    prison_cam::{CamExplosion, SecurityCamera, CAM_RADIUS},
    prison_visuals::{Fluo, MslRand, Searchlight, Siren, SoundEmitter, FLUO_RADIUS, SIREN_RADIUS},
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
    /// Invisible shapes whose sizes are 10 + Scale (helper 0x11B30 /
    /// 0x16F94): (object-list name, CollisionData row in section 5).
    pub invisible_cubes: &'static [(&'static str, u32)],
    /// Expand RING_LINEAR placements into collectable rings.
    pub ring_lines: bool,
    /// Run enemy group counts and the cell doors (`DOOR_KINDS`, rules read
    /// from stg09D.rel).
    pub prison_doors: bool,
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
    // CONTCHAO (0x1FDA4) registers the same cube row at sec5+0x649E0
    // through 0x8000F8B8; its breaking is player-attack driven and not run.
    // CONTWOOD (0x2132C) the same cube at sec5+0x666C8 (breaking not run);
    // MECHA (0x53E4) a box at sec5+0x51198 whose exec only runs collision.
    collision_boxes: &[
        ("CONTIRON", 0x656d8, 1),
        ("CONTCHAO", 0x649e0, 1),
        ("CONTWOOD", 0x666c8, 1),
        ("MECHA", 0x51198, 1),
    ],
    // stg11D.rel 0x17140: CCUBE registers sec5+0x5E5B0 (form 3, attr 0x2000);
    // its size helper 0x16F94 adds Scale to 10 (sec4+0x7D0).
    invisible_cubes: &[("CCUBE", 0x5e5b0)],
    ring_lines: false,
    prison_doors: false,
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
        // ROLL (0x53FC): one form 2 row at sec5+0x43A84; its exec only
        // spins the model.
        ("ROLL", 0x43a84, 1),
        // CONTCHAO (0x18E1C): the same cube row; breaking is not run.
        ("CONTCHAO", 0x51d20, 1),
        // CONTWOOD (0x1A3A4): the same cube row; breaking is not run.
        ("CONTWOOD", 0x53a08, 1),
        // MECHA (0x527C): exec only runs its collision (0x8000F4EC).
        ("MECHA", 0x41db4, 1),
        // TANK (0x5D7C): its row at sec5+0x47848 is an upright cylinder
        // (form 1); breaking it and its two child tasks need player attacks.
        ("TANK", 0x47848, 1),
    ],
    // SPHERE 0x11B94 (sec5+0x4C020, form 0), CCYL 0x11C38 (+0x4C050,
    // form 1), CCUBE 0x11CDC (+0x4C080, form 3).
    invisible_cubes: &[("CCUBE", 0x4c080), ("SPHERE", 0x4c020), ("CCYL", 0x4c050)],
    ring_lines: true,
    prison_doors: true,
    death_zone_config: "[Prison Lane Death Zones]\ntype=deathzone\naddress=C4C6E4\n",
};
/// o_ring_M1: pickup when a player collision center is strictly within 14.
pub const RING_PICKUP_DISTANCE: f32 = 14.0;
const SPRA_VA: u32 = 0x6c4480;
const SPRB_VA: u32 = 0x6c4e90;
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
    /// Collision centre (feet plus the PhysicsArray center_height); the
    /// range test 0x800398D8 reads it.
    pub center: V3,
    /// The player task's position (twp +0x14), which CAM aims at.
    pub position: V3,
    /// The engine's "player standing on this object" answer, as an index
    /// into `Session::movers`.
    pub standing_on: Option<usize>,
    /// The engine's collision contact between the player and a goal ring
    /// (an index into `Session::goals`); the player's own collision shape
    /// and the CCL test are main-program code.
    pub touching_goal: Option<usize>,
    /// A security camera lens (an index into `Session::cams`) whose
    /// collision the engine reports hit by the player's attack this frame.
    pub hit_cam_lens: Option<usize>,
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
    /// A CAGED door started sliding open (its group count reached zero).
    DoorOpening {
        door: usize,
    },
    /// A CAGED door reached its open stop (sound 0x1021).
    DoorOpened {
        door: usize,
    },
    /// A WALL gate started to drop (no resting elevator near).
    WallDropping {
        wall: usize,
    },
    /// A security camera was destroyed (sound 0x14).
    CamDestroyed {
        cam: usize,
        explosion: CamExplosion,
    },
    GoalContact {
        layer: usize,
        index: usize,
        sound: u16,
    },
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

/// Ninja order used by the spring tasks: unit, rotate Z, rotate X, rotate Y,
/// calculate point. The point is rotated by Y first, then X, then Z.
pub fn rotate_zxy(v: V3, angles: [i32; 3]) -> V3 {
    let (sy, cy) = nj_rotate_sin_cos(angles[1]);
    let v = [v[0] * cy + v[2] * sy, v[1], -v[0] * sy + v[2] * cy];
    let (sx, cx) = nj_rotate_sin_cos(angles[0]);
    let v = [v[0], v[1] * cx - v[2] * sx, v[1] * sx + v[2] * cx];
    let (sz, cz) = nj_rotate_sin_cos(angles[2]);
    [v[0] * cz - v[1] * sz, v[0] * sz + v[1] * cz, v[2]]
}

/// Matrix order unit, translate, rotate Z, rotate Y, rotate X (the
/// RING_LINEAR helper 0x1B21C; its calls map to GSNE8P njRotateZ 0x801158A8,
/// njRotateY 0x80115828 and njRotateX 0x80115C60): the point is rotated by
/// X first, then Y, then Z.
pub fn rotate_xyz(v: V3, angles: [i32; 3]) -> V3 {
    let (sx, cx) = nj_rotate_sin_cos(angles[0]);
    let v = [v[0], v[1] * cx - v[2] * sx, v[1] * sx + v[2] * cx];
    let (sy, cy) = nj_rotate_sin_cos(angles[1]);
    let v = [v[0] * cy + v[2] * sy, v[1], -v[0] * sy + v[2] * cy];
    let (sz, cz) = nj_rotate_sin_cos(angles[2]);
    [v[0] * cz - v[1] * sz, v[0] * sz + v[1] * cz, v[2]]
}

/// Matrix rotate Y, Z, X (attribute 0x200 order): the point turns about X,
/// then Z, then Y.
fn rotate_yzx(v: V3, angles: [i32; 3]) -> V3 {
    let (sx, cx) = nj_rotate_sin_cos(angles[0]);
    let v = [v[0], v[1] * cx - v[2] * sx, v[1] * sx + v[2] * cx];
    let (sz, cz) = nj_rotate_sin_cos(angles[2]);
    let v = [v[0] * cz - v[1] * sz, v[0] * sz + v[1] * cz, v[2]];
    let (sy, cy) = nj_rotate_sin_cos(angles[1]);
    [v[0] * cy + v[2] * sy, v[1], -v[0] * sy + v[2] * cy]
}

/// World centre of a CollisionData row (GSNE8P 0x8000EAA0 / 0x8000EB90):
/// with attribute 0x20 the row centre is a world point; otherwise it is
/// moved by translate(position) and, unless attribute 0x8000 is set, the
/// task rotation (rotate Y, Z, X with attribute 0x200, else Z, X, Y).
pub fn ccl_center(position: V3, rotation: [u16; 3], row: &CollisionRow) -> V3 {
    let angles = rotation.map(|r| i32::from(r as i16));
    if row.attr & 0x20 != 0 {
        row.center
    } else if row.attr & 0x8000 != 0 {
        add(position, row.center)
    } else if row.attr & 0x200 != 0 {
        add(position, rotate_yzx(row.center, angles))
    } else {
        add(position, rotate_zxy(row.center, angles))
    }
}

/// Unit axis of an oriented shape (GSNE8P 0x8000E960): (0, 1, 0) through the
/// row's own angles and then, unless attribute 0x20 or 0x8000 is set, the
/// task rotation, both in the attribute's order (Y, Z, X with 0x200, else
/// Z, X, Y), as a vector (0x801154EC ignores translation).
pub fn ccl_axis(rotation: [u16; 3], row: &CollisionRow) -> V3 {
    let task = rotation.map(|r| i32::from(r as i16));
    let own = row.angles.map(|a| a as i32);
    let turn = |v: V3, a: [i32; 3]| {
        if row.attr & 0x200 != 0 {
            rotate_yzx(v, a)
        } else {
            rotate_zxy(v, a)
        }
    };
    let v = turn([0.0, 1.0, 0.0], own);
    if row.attr & 0x20 != 0 || row.attr & 0x8000 != 0 {
        v
    } else {
        turn(v, task)
    }
}

/// A round collision shape, kept analytic (GSNE8P CCL pair tests).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "form", rename_all = "camelCase")]
pub enum RoundShape {
    /// Form 0: radius a.
    Sphere { center: V3, radius: f32 },
    /// Form 1 (0x8000A054): upright; radius a, half height b about the centre.
    Cylinder {
        center: V3,
        radius: f32,
        half_height: f32,
    },
    /// Form 2 (0x8000987C): the segment centre +/- b * axis, radius a.
    Capsule { a: V3, b: V3, radius: f32 },
}

/// The round shape for a form 0, 1 or 2 row with sizes `size`.
pub fn ccl_round_shape(
    position: V3,
    rotation: [u16; 3],
    row: &CollisionRow,
    size: [f32; 2],
) -> Option<RoundShape> {
    let center = ccl_center(position, rotation, row);
    match row.form {
        0 => Some(RoundShape::Sphere {
            center,
            radius: size[0],
        }),
        1 => Some(RoundShape::Cylinder {
            center,
            radius: size[0],
            half_height: size[1],
        }),
        2 => {
            let axis = ccl_axis(rotation, row);
            let d = [axis[0] * size[1], axis[1] * size[1], axis[2] * size[1]];
            Some(RoundShape::Capsule {
                a: sub(center, d),
                b: add(center, d),
                radius: size[0],
            })
        }
        _ => None,
    }
}

/// World triangles of a CollisionData form 3 box, as the GameCube CCL
/// places it (GSNE8P main.dol).
///
/// * Centre (0x8000EAA0): with attribute 0x20 the row centre is used as a
///   world point; otherwise it is transformed by translate(position) and,
///   unless attribute 0x8000 is set, the task rotation: rotate Y, Z, X with
///   attribute 0x200, else rotate Z, X, Y.
/// * Orientation (box tests such as 0x800090E8): the relative vector is
///   turned by njRotateY(-(Rotation.y + row angle Y)), so the box is aligned
///   to that yaw only; X and Z rotations do not tilt it.
pub fn ccl_box_tris(
    position: V3,
    rotation: [u16; 3],
    row: &CollisionRow,
    half: V3,
) -> Vec<[V3; 3]> {
    let angles = rotation.map(|r| i32::from(r as i16));
    let center = ccl_center(position, rotation, row);
    let yaw = angles[1].wrapping_add(row.angles[1] as i32);
    let (s, c) = nj_rotate_sin_cos(yaw);
    box_tris(half, [0.0; 3])
        .into_iter()
        .map(|t| t.map(|v| add(center, [v[0] * c + v[2] * s, v[1], -v[0] * s + v[2] * c])))
        .collect()
}

/// RING_LINEAR (stg09D.rel 0x1B448 with helpers 0x1B1E8 and 0x1B21C): the
/// ring count n is Scale.z truncated and clamped to 1..=8, and ring i sits
/// at local (0, Scale.y * njSin(i * (0x4000 / n)), -i * (10 + Scale.x))
/// under the placement's translation and rotate Z, Y, X. The arch call
/// (PC import 0x80112050) is njSin, the start-up table lookup (GSNE8P
/// 0x80112398).
pub fn ring_line(position: V3, angles: [i32; 3], scale: V3) -> Vec<V3> {
    let n = (scale[2] as i32).clamp(1, 8);
    let step = 0x4000 / n;
    (0..n)
        .map(|i| {
            let local = [
                0.0,
                scale[1] * nj_sin(i * step),
                -(i as f32) * (RING_LINE_BASE + scale[0]),
            ];
            add(position, rotate_xyz(local, angles))
        })
        .collect()
}

#[cfg(test)]
mod ring_line_tests {
    use super::*;

    #[test]
    fn ring_line_spacing_count_and_yaw() {
        let r = ring_line([0.0, 0.0, 0.0], [0, 0x4000, 0], [15.0, 0.0, 3.0]);
        assert_eq!(r.len(), 3);
        // Local -Z turned a quarter about Y points along -X; spacing 25.
        assert!((r[1][0] + 25.0).abs() < 1e-3 && r[1][2].abs() < 1e-3);
        assert!((r[2][0] + 50.0).abs() < 1e-3 && r[2][1] == 0.0);
        assert_eq!(ring_line([0.0; 3], [0; 3], [20.0, 0.0, 0.2]).len(), 1);
        assert_eq!(ring_line([0.0; 3], [0; 3], [20.0, 0.0, 12.0]).len(), 8);
        // The arch follows njSin of i * (0x4000 / n).
        let a = ring_line([0.0; 3], [0; 3], [15.5, 4.0, 3.0]);
        assert_eq!(a[1][1], 4.0 * nj_sin(0x1555));
    }

    #[test]
    fn round_shapes_follow_the_ccl_rules() {
        let mut row = CollisionRow {
            kind: 0,
            form: 2,
            push: 0x77,
            damage: 0,
            attr: 0,
            center: [0.0, 42.0, 0.0],
            size: [3.0, 60.0, 1.0, 0.0],
            angles: [0x4000, 0, 0x4000],
        };
        // FENCEL's post: (0, 1, 0) turned by its own X then Z quarter turns.
        let Some(RoundShape::Capsule { a, b, radius }) =
            ccl_round_shape([0.0; 3], [0; 3], &row, [3.0, 60.0])
        else {
            panic!("capsule");
        };
        assert_eq!(radius, 3.0);
        let axis = sub(b, a);
        assert!((len(axis) - 120.0).abs() < 1e-3);
        // Own X then Z quarter turns lay the rail along Z, the long side of
        // FENCEL's box row.
        assert!(axis[2] > 119.9, "{axis:?}");
        row.form = 1;
        assert_eq!(
            ccl_round_shape([1.0, 2.0, 3.0], [0; 3], &row, [3.0, 60.0]),
            Some(RoundShape::Cylinder {
                center: [1.0, 44.0, 3.0],
                radius: 3.0,
                half_height: 60.0
            })
        );
    }

    #[test]
    fn ccl_boxes_turn_with_yaw_and_ignore_tilt() {
        let row = CollisionRow {
            kind: 0,
            form: 3,
            push: 0x77,
            damage: 0,
            attr: 0,
            center: [0.0, 10.0, 0.0],
            size: [1.0, 10.0, 30.0, 0.0],
            angles: [0; 3],
        };
        let span = |tris: &[[V3; 3]], k: usize| {
            let vs = tris.iter().flatten();
            let lo = vs.clone().map(|v| v[k]).fold(f32::MAX, f32::min);
            let hi = vs.map(|v| v[k]).fold(f32::MIN, f32::max);
            hi - lo
        };
        let flat = ccl_box_tris([0.0; 3], [0, 0, 0], &row, [1.0, 10.0, 30.0]);
        assert!((span(&flat, 2) - 60.0).abs() < 1e-3);
        // A quarter yaw swaps the long side to X.
        let turned = ccl_box_tris([0.0; 3], [0, 0x4000, 0], &row, [1.0, 10.0, 30.0]);
        assert!((span(&turned, 0) - 60.0).abs() < 1e-3);
        // A Z tilt moves the centre but does not tilt the box.
        let tilted = ccl_box_tris([0.0; 3], [0, 0, 0x4000], &row, [1.0, 10.0, 30.0]);
        assert!((span(&tilted, 1) - 20.0).abs() < 1e-3);
        let cx = tilted.iter().flatten().map(|v| v[0]).sum::<f32>() / 36.0;
        assert!((cx + 10.0).abs() < 1e-3, "{cx}");
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
    /// Single-player start row (feet position and BAMS yaw).
    pub start: V3,
    pub start_yaw: u16,
    pub slot_words: [u32; 2],
    pub movers: Vec<Mover>,
    pub rings_placed: Vec<Pickup>,
    pub checkpoints: Vec<Pickup>,
    pub goals: Vec<Goal>,
    pub springs: Vec<Spring>,
    /// Enemies that take part in group counts.
    pub enemies: Vec<Enemy>,
    /// Live-enemy count per group (the byte table at 0x801E4AB8).
    #[serde(skip)]
    pub group_counts: [u8; 256],
    pub doors: Vec<PrisonDoor>,
    pub walls: Vec<ElevatorWall>,
    #[serde(skip)]
    wall_rows: Vec<CollisionRow>,
    pub sirens: Vec<Siren>,
    pub cams: Vec<SecurityCamera>,
    pub fluos: Vec<Fluo>,
    pub searchlights: Vec<Searchlight>,
    pub sound_emitters: Vec<SoundEmitter>,
    /// MSL rand state the visual objects draw from (seed 1).
    pub rand: MslRand,
    /// Round collision shapes (forms 0-2), kept analytic.
    pub shapes: Vec<RoundShape>,
    #[serde(skip)]
    door_rows: std::collections::BTreeMap<&'static str, Vec<CollisionRow>>,
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
        let sized_rows = rules
            .invisible_cubes
            .iter()
            .map(|&(n, offset)| {
                Ok((
                    n,
                    collision_rows(src.stage_module, rules.movers.module_file, offset, 1)?
                        .remove(0),
                ))
            })
            .collect::<Result<Vec<(&str, CollisionRow)>>>()?;
        let mut shapes = vec![];
        let mut door_rows = std::collections::BTreeMap::new();
        let wall_rows = if rules.prison_doors {
            collision_rows(
                src.stage_module,
                rules.movers.module_file,
                WALL_ROWS,
                WALL_ROW_COUNT,
            )?
        } else {
            vec![]
        };
        if rules.prison_doors {
            for k in DOOR_KINDS {
                let rows = collision_rows(
                    src.stage_module,
                    rules.movers.module_file,
                    k.rows,
                    k.row_count,
                )?;
                door_rows.insert(k.name, rows);
            }
        }
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
        let mut enemies: Vec<Enemy> = vec![];
        let mut doors = vec![];
        let mut walls = vec![];
        let mut sirens = vec![];
        let mut cams = vec![];
        let mut fluos = vec![];
        let mut searchlights = vec![];
        let mut sound_emitters = vec![];
        let mut rand = MslRand::default();
        let mut unimplemented = std::collections::BTreeMap::new();
        for (li, layer) in stage.layers.iter().enumerate() {
            for p in &layer.placements {
                let Some(def) = p.definition_index.and_then(|i| objects.entries.get(i)) else {
                    continue;
                };
                let pl = &p.placement;
                let name = def.name.trim_end();
                // Collision objects keep the placement's Y rotation only
                // (stg09D.rel 0x4930, 0x30CC, 0x510 and helper 0x9A4C zero
                // X and Z), applied through njRotateY.
                let place_at = |tris: &[[V3; 3]], at: V3| -> Vec<[V3; 3]> {
                    tris.iter()
                        .map(|t| t.map(|v| place_y(v, pl.rotation[1], at)))
                        .collect()
                };
                let place = |tris: &[[V3; 3]]| place_at(tris, pl.position);
                let solid = |tris: Vec<[V3; 3]>| {
                    PlayWorld::from_tris(
                        tris.into_iter()
                            .filter_map(|v| make_tri(v, 1, u32::MAX))
                            .collect(),
                    )
                };
                match (def.function_va, name) {
                    (RING_VA, "RING") => rings_placed.push(Pickup {
                        layer: li,
                        index: p.placement_index,
                        position: pl.position,
                        taken: false,
                    }),
                    (RING_LINEAR_VA, "RING_LINEAR") if rules.ring_lines => {
                        let angles = pl.rotation.map(|r| i32::from(r as i16));
                        for position in ring_line(pl.position, angles, pl.parameters) {
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
                        let angles = pl.rotation.map(|r| i32::from(r as i16));
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
                    (_, "SIREN") if rules.prison_doors => {
                        // stg09D.rel 0x5A30: radius-5 sphere at sec5+0x45858.
                        sirens.push(Siren::new(li, p.placement_index, pl.position, &mut rand));
                        shapes.push(RoundShape::Sphere {
                            center: pl.position,
                            radius: SIREN_RADIUS,
                        });
                    }
                    (_, "CAM") if rules.prison_doors => {
                        // stg09D.rel 0x2450: the body's radius-8.5 sphere
                        // (sec5+0x3A7C4); the lens sphere stays on the camera.
                        cams.push(SecurityCamera::new(
                            li,
                            p.placement_index,
                            pl.position,
                            pl.rotation[1],
                        ));
                        shapes.push(RoundShape::Sphere {
                            center: pl.position,
                            radius: CAM_RADIUS,
                        });
                    }
                    (_, "SEARCHLIGHT") if rules.prison_doors => searchlights.push(Searchlight {
                        layer: li,
                        index: p.placement_index,
                        position: pl.position,
                        yaw: pl.rotation[1],
                        speed: u32::from(pl.rotation[0]),
                        phase: 0,
                    }),
                    (_, "SE") if rules.prison_doors => sound_emitters.push(SoundEmitter::new(
                        li,
                        p.placement_index,
                        pl.position,
                        pl.rotation[0],
                    )),
                    (_, "FLUO") if rules.prison_doors => {
                        // stg09D.rel 0x418C: radius-5 sphere at sec5+0x40504.
                        fluos.push(Fluo::new(li, p.placement_index, pl.position));
                        shapes.push(RoundShape::Sphere {
                            center: pl.position,
                            radius: FLUO_RADIUS,
                        });
                    }
                    (_, "SIGNA") if rules.prison_doors => {
                        // stg09D.rel 0x58D8: draw only, no state.
                    }
                    (_, "WALL") if rules.prison_doors => walls.push(ElevatorWall::new(
                        li,
                        p.placement_index,
                        pl.position,
                        pl.rotation,
                        pl.parameters[0],
                    )),
                    (_, n) if rules.prison_doors && DOOR_KINDS.iter().any(|k| k.name == n) => doors
                        .push(PrisonDoor {
                            kind: *DOOR_KINDS.iter().find(|k| k.name == n).unwrap(),
                            layer: li,
                            index: p.placement_index,
                            position: pl.position,
                            rotation: pl.rotation,
                            group: (pl.rotation[0] & 0xff) as u8,
                            speed: pl.parameters[0],
                            positive: pl.rotation[2] & 1 != 0,
                            state: 0,
                            slide: 0.0,
                            velocity: 0.0,
                        }),
                    (_, n) if rules.prison_doors && enemy_counts(n, pl.parameters).is_some() => {
                        enemies.push(Enemy {
                            layer: li,
                            index: p.placement_index,
                            name: n.to_string(),
                            position: pl.position,
                            group: (pl.rotation[1] & 0xff) as u8,
                            counted: enemy_counts(n, pl.parameters) == Some(true),
                            alive: true,
                        });
                        *unimplemented
                            .entry(format!("{name} (group count only)"))
                            .or_insert(0) += 1;
                    }
                    (_, "MLIFT") => {
                        let solid = solid(place(&lift_solid));
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
                        // The dynamic collision object sits
                        // `elevator_collision_drop` below the placement.
                        let at = [
                            pl.position[0],
                            pl.position[1] - movers_program.elevator_collision_drop,
                            pl.position[2],
                        ];
                        let solid = solid(place_at(&elevator_solid, at));
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
                    (_, n) if sized_rows.iter().any(|(s, _)| *s == n) => {
                        // Invisible collision: the CollisionData row with
                        // sizes CCUBE_BASE + Scale (0x11B30).
                        let (_, row) = sized_rows.iter().find(|(s, _)| *s == n).unwrap();
                        let h = pl.parameters.map(|s| s + CCUBE_BASE);
                        if row.form == 3 {
                            for t in ccl_box_tris(pl.position, pl.rotation, row, h) {
                                if let Some(t) = make_tri(t, 1, u32::MAX) {
                                    world.push(t);
                                }
                            }
                        } else if let Some(s) =
                            ccl_round_shape(pl.position, pl.rotation, row, [h[0], h[1]])
                        {
                            shapes.push(s);
                        }
                        *unimplemented
                            .entry(format!("{name} (solid only)"))
                            .or_insert(0) += 1;
                    }
                    (_, "BRIDGE") => {
                        let bridge = Bridge::new(&movers_program, pl.position, pl.rotation[1]);
                        let solid = solid(place(&bridge_solid));
                        movers.push(Mover {
                            layer: li,
                            index: p.placement_index,
                            state: MoverState::Bridge(bridge),
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
                            // Form 3 boxes follow `ccl_box_tris`; forms 0-2
                            // are kept as round shapes.
                            if row.form != 3 {
                                match ccl_round_shape(
                                    pl.position,
                                    pl.rotation,
                                    row,
                                    [row.size[0], row.size[1]],
                                ) {
                                    Some(s) => {
                                        shapes.push(s);
                                        *unimplemented
                                            .entry(format!("{name} row (solid only)"))
                                            .or_insert(0) += 1;
                                    }
                                    None => {
                                        *unimplemented
                                            .entry(format!(
                                                "{name} row (form {}): rule not read",
                                                row.form
                                            ))
                                            .or_insert(0) += 1;
                                    }
                                }
                                continue;
                            }
                            let h = [row.size[0], row.size[1], row.size[2]];
                            for t in ccl_box_tris(pl.position, pl.rotation, row, h) {
                                if let Some(t) = make_tri(t, 1, u32::MAX) {
                                    world.push(t);
                                }
                            }
                            *unimplemented
                                .entry(format!("{name} row (solid only)"))
                                .or_insert(0) += 1;
                        }
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
            group_counts: {
                let mut c = [0u8; 256];
                for e in enemies.iter().filter(|e| e.counted) {
                    c[e.group as usize] = c[e.group as usize].wrapping_add(1);
                }
                c
            },
            enemies,
            doors,
            door_rows,
            walls,
            wall_rows,
            sirens,
            cams,
            fluos,
            searchlights,
            sound_emitters,
            rand,
            shapes,
        })
    }

    /// The switch query 0x80018E30: true when the group has no live
    /// counted enemy.
    pub fn group_clear(&self, group: u8) -> bool {
        self.group_counts[group as usize] == 0
    }

    /// Report an enemy destroyed (by the walker's attacks, which are
    /// main-program code). Decrements its group count as 0x80018B88 does.
    pub fn destroy_enemy(&mut self, i: usize) -> bool {
        let Some(e) = self.enemies.get_mut(i).filter(|e| e.alive) else {
            return false;
        };
        e.alive = false;
        if e.counted {
            let c = &mut self.group_counts[e.group as usize];
            *c = c.wrapping_sub(1);
        }
        true
    }

    /// Static terrain plus every mover's solid at its current position.
    /// A cell door's collision boxes: its CollisionData rows with the centre
    /// its exec writes this frame, placed by the CCL box rule.
    pub fn door_solid(&self, i: usize) -> Vec<[V3; 3]> {
        let Some(d) = self.doors.get(i) else {
            return vec![];
        };
        let rows = self
            .door_rows
            .get(d.kind.name)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        d.live_rows(rows)
            .iter()
            .filter(|r| r.form == 3)
            .flat_map(|r| {
                ccl_box_tris(d.position, d.rotation, r, [r.size[0], r.size[1], r.size[2]])
            })
            .collect()
    }

    /// A WALL gate's collision boxes with its slide applied to row 1.
    pub fn wall_solid(&self, i: usize) -> Vec<[V3; 3]> {
        let Some(w) = self.walls.get(i) else {
            return vec![];
        };
        w.live_rows(&self.wall_rows)
            .iter()
            .filter(|r| r.form == 3)
            .flat_map(|r| {
                ccl_box_tris(w.position, w.rotation, r, [r.size[0], r.size[1], r.size[2]])
            })
            .collect()
    }

    pub fn snapshot_world(&self) -> PlayWorld {
        let mut w = self.world.clone();
        for i in 0..self.walls.len() {
            for t in self.wall_solid(i) {
                if let Some(t) = make_tri(t, 1, u32::MAX) {
                    w.push(t);
                }
            }
        }
        for i in 0..self.doors.len() {
            for t in self.door_solid(i) {
                if let Some(t) = make_tri(t, 1, u32::MAX) {
                    w.push(t);
                }
            }
        }
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
        for s in &mut self.sirens {
            s.step();
        }
        let centres = [player.map(|p| p.center), None];
        let positions = [player.map(|p| p.position), None];
        for (i, c) in self.cams.iter_mut().enumerate() {
            let hit = player.and_then(|p| p.hit_cam_lens) == Some(i);
            if let Some(explosion) = c.step(&centres, &positions, hit) {
                events.push(Event::CamDestroyed { cam: i, explosion });
            }
        }
        for f in &mut self.fluos {
            f.step(&mut self.rand);
        }
        for s in &mut self.searchlights {
            s.step();
        }
        for (i, w) in self.walls.iter_mut().enumerate() {
            if w.step(&elevators) {
                events.push(Event::WallDropping { wall: i });
            }
        }
        for (i, d) in self.doors.iter_mut().enumerate() {
            let was = d.state;
            let clear = self.group_counts[d.group as usize] == 0;
            if d.step(clear) == Some(DoorSound::Opened) {
                events.push(Event::DoorOpened { door: i });
            }
            if was == 0 && d.state == 1 {
                events.push(Event::DoorOpening { door: i });
            }
        }
        if let Some(p) = player {
            self.objects(p, &mut events);
        }
        events
    }

    fn objects(&mut self, player: PlayerSample, events: &mut Vec<Event>) {
        let center = player.center;
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
        let local = self.goal_program_local_effect;
        for (i, g) in self.goals.iter_mut().enumerate() {
            if player.touching_goal != Some(i) {
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
    }

    pub fn elapsed_seconds(&self) -> f32 {
        self.frame as f32 / FRAME_RATE
    }
}
