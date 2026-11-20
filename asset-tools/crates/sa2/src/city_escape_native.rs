//! Native gameplay inputs for City Escape (Hero story, level 13), read from
//! the installed Steam data with no adapted rules.
//!
//! This is everything a Sonic speed-stage session needs that the install
//! provides as data: Sonic's PhysicsArray row, the start row, terrain, the
//! grind-rail paths, death zones, spring tasks, dash-panel, ring-group and
//! triple-spring tasks (ported from stg13D.rel), bush-block collision and the goal-ring program, plus the raw
//! SET rows of every other placement.
//!
//! The player controller is deliberately absent. Its code is not readable
//! from the install (docs/CITY-ESCAPE-NATIVE.md), and this module adds no
//! substitute: nothing here moves a player.
use crate::{
    collision::CollisionWorld,
    dash_panel::{self, DashPanel},
    death_zones,
    fan_stage::MountedStage,
    goal_ring,
    play_objects::{death_zone_triangles, place_y, read_object_solid},
    play_world::V3,
    player_physics::PlayerParameters,
    ring_group::{self, RingGroup},
    spring::{SpringKind, StageSpring},
    stage_paths,
    three_spring::{self, ThreeSpring},
    Error, Result,
};
use serde::Serialize;
use std::collections::BTreeMap;

pub const LEVEL: u16 = 13;
pub const MODULE_FILE: &str = "stg13D.rel";
/// PC_SA2/STG13.ini [City Escape Death Zones].
pub const DEATH_ZONE_CONFIG: &str = "[City Escape Death Zones]\ntype=deathzone\naddress=CDC678\n";
/// PC_SA2/STG13.ini [City Escape Paths].
pub const PATH_CONFIG: &str = "[City Escape Paths]\ntype=pathlist\naddress=CDB35C\n";
/// The path callback every City Escape grind rail registers (path type 1).
pub const RAIL_CALLBACK_VA: u32 = 0x4980c0;
/// PC_SA2/STG13.ini [City Escape Bush Block Collision].
pub const GREEN_B_COLLISION_RVA: u32 = 0xcaa350;
const GREEN_B_VA: u32 = 0x5e2b40;
const SPRA_VA: u32 = 0x6c4480;
const SPRB_VA: u32 = 0x6c4e90;
const GOALRING_VA: u32 = 0x6c63c0;
const KASOKU_VA: u32 = 0x6cfbf0;
const RING_LINEAR_VA: u32 = 0x6c5ed0;
const RING_CIRCLE_VA: u32 = 0x6c6180;
const THREE_SPRING_VA: u32 = 0x6d9310;

/// One SET row bound to its object-list entry, unchanged.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub name: String,
    pub function_va: u32,
    pub layer: usize,
    pub index: usize,
    pub position: V3,
    pub rotation: [u16; 3],
    pub scale: [f32; 3],
}

/// A grind-rail path as stored: points, rotation words and lengths.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RailPath {
    pub tag_va: u32,
    pub total_length: f32,
    pub points: Vec<V3>,
    pub angles: Vec<[i16; 2]>,
    pub lengths: Vec<f32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Goal {
    pub row: Row,
    pub angle_x: i32,
    /// The native initialization decision for this placement.
    pub route: String,
}

#[derive(Debug, Serialize)]
pub struct Spring {
    pub row: Row,
    #[serde(skip)]
    pub task: StageSpring,
}

#[derive(Debug, Serialize)]
pub struct Panel {
    pub row: Row,
    pub task: DashPanel,
}

#[derive(Debug, Serialize)]
pub struct TripleSpring {
    pub row: Row,
    pub task: ThreeSpring,
}

#[derive(Debug, Serialize)]
pub struct Rings {
    pub row: Row,
    pub task: RingGroup,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CityEscape {
    pub character: i8,
    pub physics_slot: usize,
    pub physics: PlayerParameters,
    /// Single-player start row: position and BAMS yaw, as stored.
    pub start_position: V3,
    pub start_yaw: u16,
    pub goal_program_local_effect: u32,
    pub initial_slot_words: [u32; 2],
    pub goals: Vec<Goal>,
    pub springs: Vec<Spring>,
    /// The verified KASOKU callback of stg13D.rel and one task per panel.
    pub dash_panel_program: dash_panel::Program,
    pub dash_panels: Vec<Panel>,
    /// The verified RING_LINEAR / RING_CIRCLE code and one task per group.
    pub ring_group_program: ring_group::Program,
    pub ring_groups: Vec<Rings>,
    /// The verified 3SPRING code and one task per placement.
    pub three_spring_program: three_spring::Program,
    pub three_springs: Vec<TripleSpring>,
    pub rails: Vec<RailPath>,
    pub death_zones: Vec<Vec<[V3; 3]>>,
    /// GREEN_B Basic collision placed by SET position and Y angle.
    pub bush_solids: Vec<[V3; 3]>,
    /// Every other placement, by object name, as raw SET rows.
    pub rows: BTreeMap<String, Vec<Row>>,
    #[serde(skip)]
    pub terrain: CollisionWorld,
}

impl CityEscape {
    /// Read City Escape from a mounted Hero story dispatch (Sonic or Shadow).
    pub fn read(stage: &MountedStage<'_>, exe: &[u8], stage_module: &[u8]) -> Result<Self> {
        if stage.dispatch.level != LEVEL {
            return Err(Error::invalid("not City Escape"));
        }
        let start = stage
            .entry_pose
            .as_ref()
            .ok_or_else(|| Error::invalid("City Escape has no start row"))?;
        let terrain = stage
            .collision_world()?
            .ok_or_else(|| Error::invalid("City Escape terrain is unavailable"))?;
        let objects = stage
            .object_table
            .ok_or_else(|| Error::invalid("City Escape object list is unavailable"))?;
        let physics_slot = match stage.dispatch.character {
            0 => 0usize,
            1 => 1,
            c => return Err(Error::invalid(format!("unsupported character {c}"))),
        };
        let physics = crate::player_physics::read(exe)?
            .swap_remove(physics_slot)
            .parameters;
        let goal_program = goal_ring::Program::read(LEVEL, stage_module)?;
        let bush = read_object_solid(exe, GREEN_B_COLLISION_RVA)?;
        let dash_panel_program = dash_panel::Program::read(MODULE_FILE, stage_module)?;
        let ring_group_program = ring_group::Program::read(MODULE_FILE, stage_module)?;
        let three_spring_program = three_spring::Program::read(MODULE_FILE, stage_module)?;

        let mut goals = vec![];
        let mut springs = vec![];
        let mut dash_panels = vec![];
        let mut ring_groups = vec![];
        let mut three_springs = vec![];
        let mut bush_solids = vec![];
        let mut rows: BTreeMap<String, Vec<Row>> = BTreeMap::new();
        for (li, layer) in stage.layers.iter().enumerate() {
            for p in &layer.placements {
                let Some(def) = p.definition_index.and_then(|i| objects.entries.get(i)) else {
                    continue;
                };
                let pl = &p.placement;
                let row = Row {
                    name: def.name.trim_end().to_string(),
                    function_va: def.function_va,
                    layer: li,
                    index: p.placement_index,
                    position: pl.position,
                    rotation: pl.rotation,
                    scale: pl.parameters,
                };
                match (def.function_va, row.name.as_str()) {
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
                            row,
                            angle_x,
                            route: format!("{route:?}"),
                        });
                    }
                    (SPRA_VA, "SPRA") | (SPRB_VA, "SPRB") => {
                        let kind = if row.name == "SPRA" {
                            SpringKind::A
                        } else {
                            SpringKind::B
                        };
                        let task = StageSpring::new(
                            kind,
                            pl.position,
                            pl.rotation.map(i32::from),
                            pl.parameters,
                        );
                        springs.push(Spring { row, task });
                    }
                    (KASOKU_VA, "KASOKU") => dash_panels.push(Panel {
                        task: DashPanel::new(pl.rotation.map(i32::from), pl.parameters),
                        row,
                    }),
                    (RING_LINEAR_VA, "RING_LINEAR") | (RING_CIRCLE_VA, "RING_CIRCLE") => {
                        let kind = if row.name == "RING_LINEAR" {
                            ring_group::Kind::Linear
                        } else {
                            ring_group::Kind::Circle
                        };
                        ring_groups.push(Rings {
                            task: RingGroup::new(
                                kind,
                                pl.position,
                                pl.rotation.map(u32::from),
                                pl.parameters,
                            ),
                            row,
                        });
                    }
                    (THREE_SPRING_VA, "3SPRING") => three_springs.push(TripleSpring {
                        task: ThreeSpring::new(
                            pl.position,
                            pl.rotation.map(i32::from),
                            pl.parameters,
                        ),
                        row,
                    }),
                    (GREEN_B_VA, "GREEN_B") => {
                        bush_solids.extend(
                            bush.iter()
                                .map(|t| t.map(|v| place_y(v, pl.rotation[1], pl.position))),
                        );
                        rows.entry(row.name.clone()).or_default().push(row);
                    }
                    _ => rows.entry(row.name.clone()).or_default().push(row),
                }
            }
        }
        let mut rails = vec![];
        for list in stage_paths::read(exe, PATH_CONFIG)?.values() {
            for path in &list.paths {
                if path.callback_va == RAIL_CALLBACK_VA {
                    rails.push(RailPath {
                        tag_va: path.tag_va,
                        total_length: path.total_length,
                        points: path.points.iter().map(|p| p.position).collect(),
                        angles: path.points.iter().map(|p| [p.angle_x, p.angle_z]).collect(),
                        lengths: path.points.iter().map(|p| p.length).collect(),
                    });
                }
            }
        }
        let mut death_zones = vec![];
        for table in death_zones::read(exe, DEATH_ZONE_CONFIG)?.values() {
            for z in &table.zones {
                death_zones.push(death_zone_triangles(z)?);
            }
        }
        Ok(Self {
            character: stage.dispatch.character,
            physics_slot,
            physics,
            start_position: start.position,
            start_yaw: start.yaw,
            goal_program_local_effect: goal_program.local_effect,
            initial_slot_words: goal_program.initial_slot_words,
            goals,
            springs,
            dash_panel_program,
            dash_panels,
            ring_group_program,
            ring_groups,
            three_spring_program,
            three_springs,
            rails,
            death_zones,
            bush_solids,
            rows,
            terrain,
        })
    }

    /// Goals whose native initialization reaches the regular contact route.
    pub fn contact_goals(&self) -> impl Iterator<Item = &Goal> {
        self.goals.iter().filter(|g| g.route.starts_with("Contact"))
    }
}
