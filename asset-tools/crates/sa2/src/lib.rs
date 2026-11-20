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
pub mod collision;
mod collision_record;
pub mod damage_ring;
pub mod damage_ring_stage;
pub mod death_zones;
pub mod demo;
pub mod duel_platform;
pub mod event;
pub mod event_camera;
pub mod glb_cli;
pub mod input;
pub mod iron_gate_movers;
mod key_pickup;
pub mod manifest;
pub mod model;
pub mod motion;
pub mod ninja;
pub mod object_table;
pub mod pe;
pub mod physical_board;
pub mod player_physics;
pub mod play_autopilot;
pub mod play_nav;
pub mod play_objects;
pub mod play_session;
pub mod play_world;
pub mod pose;
pub mod ring_group;
pub mod ring_magnet;
pub mod set;
pub mod spring;
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
pub mod three_spring;
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

pub mod city_escape_native;
pub mod city_land;
pub mod dash_panel;

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
