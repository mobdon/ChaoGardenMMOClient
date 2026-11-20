//! Local, bounded readers. Source decoding and gameplay implementation are separate.
pub mod archive;
pub mod audio;
pub mod big_foot;
pub mod board;
pub mod board_draw;
pub mod board_model;
pub mod board_motion;
mod board_replay;
pub mod city_music;
pub mod city_rel;
pub mod city_replay;
pub mod city_rider;
pub mod city_stage;
pub mod effect_emitter;
pub mod chao_pipe;
pub mod collision;
pub mod color_blend;
mod collision_record;
pub mod damage_ring;
pub mod damage_ring_stage;
pub mod death_zones;
pub mod demo;
pub mod duel_debris;
pub mod duel_director;
pub mod duel_dust;
pub mod duel_fireball;
pub mod duel_fog;
pub mod duel_gauge;
pub mod duel_platform;
pub mod duel_platform_motion;
pub mod duel_stage;
pub mod duel_stage_setup;
pub mod event;
pub mod event_camera;
pub mod glb_cli;
pub mod input;
pub mod iron_gate_movers;
mod key_pickup;
pub mod land_contact;
pub mod land_nodes;
pub mod land_probe;
pub mod land_select;
pub mod manifest;
pub mod model;
pub mod msl_math;
pub mod motion;
pub mod object_table;
pub mod ori;
pub mod pe;
pub mod physical_board;
pub mod player_physics;
pub mod player_range;
pub mod ninja_matrix;
pub mod ninja_vector;
pub mod ninja_trig;
pub mod prison_visuals;
pub mod prison_doors;
pub mod prison_cam;
pub mod play_objects;
pub mod play_session;
pub mod play_world;
pub mod pose;
pub mod ring_magnet;
pub mod rocket;
pub mod set;
pub mod spring;
pub mod solid_box;
pub mod stage;
pub mod stage_catalog;
mod stage_container;
pub mod stage_gameplay;
mod stage_key;
pub mod stage_paths;
mod stage_pipe;
pub mod stage_positions;
pub mod stage_spring_host;
pub mod story;
pub mod texture;
pub mod visual;
pub mod visual_glb;
pub mod wild_canyon;

use std::fmt;

#[derive(Debug)]
pub struct Error(String);

impl Error {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self(value.to_string())
    }
}
impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Self(value.to_string())
    }
}
pub type Result<T> = std::result::Result<T, Error>;

pub mod city_land;

pub mod checkpoint;
pub mod emerald_path;
pub mod fan_stage;
pub mod goal_ring;
pub mod kart_course;
pub mod message_bank;
pub mod mission_ranks;
pub mod stage20;
pub mod stage_fog;
pub mod stage_land;
pub mod stage_lighting;
pub mod task_coverage;
