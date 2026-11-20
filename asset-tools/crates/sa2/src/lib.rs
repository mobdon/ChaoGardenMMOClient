//! Local, bounded readers. Source decoding and gameplay implementation are separate.
pub mod archive;
pub mod audio;
pub mod big_foot;
pub mod board;
pub mod board_draw;
pub mod board_model;
pub mod board_motion;
mod board_replay;
pub mod chao_pipe;
pub mod character_animations;
pub mod character_motions;
pub mod city_music;
pub mod city_rel;
pub mod city_replay;
pub mod city_rider;
pub mod city_stage;
pub mod collision;
mod collision_record;
pub mod color_blend;
pub mod csb;
pub mod effect_emitter;
pub use collision_record::chao_collision;
pub mod damage_ring;
pub mod damage_ring_stage;
pub mod death_zones;
pub mod demo;
pub mod duel_debris;
pub mod duel_director;
pub mod duel_display;
pub mod duel_dust;
pub mod duel_fireball;
pub mod duel_fog;
pub mod duel_gauge;
pub mod duel_platform;
pub mod duel_platform_motion;
pub mod duel_stage;
pub mod duel_stage_setup;
pub mod duel_title;
pub mod enemy_lib;
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
pub mod model_bank;
pub mod motion;
pub mod msl_math;
pub mod ninja_matrix;
pub mod ninja_trig;
pub mod ninja_vector;
pub mod object_table;
pub mod ori;
pub mod pe;
pub mod physical_board;
pub mod play_objects;
pub mod play_session;
pub mod play_world;
pub mod player_accel;
pub mod player_analog;
pub mod player_path;
pub mod player_physics;
pub mod player_range;
pub mod player_slope;
pub mod player_turn;
pub mod pose;
pub mod prison_cam;
pub mod prison_doors;
pub mod prison_visuals;
pub mod ring_group;
pub mod ring_magnet;
pub mod rocket;
pub mod set;
pub mod solid_box;
pub mod spring;
pub mod stage;
pub mod stage_catalog;
mod stage_container;
pub mod stage_gameplay;
mod stage_key;
pub mod stage_objects;
pub mod stage_paths;
mod stage_pipe;
pub mod stage_positions;
pub mod stage_spring_host;
pub mod story;
pub mod texture;
pub mod three_spring;
pub mod visual;
pub mod visual_glb;
pub mod walker_collide;
pub mod walker_frame;
pub mod walker_ground;
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
pub mod kart_contact;
pub mod kart_course;
pub mod kart_drift;
pub mod kart_drive_input;
pub mod kart_finish_menu;
pub mod kart_gate;
pub mod kart_guided_drive;
mod kart_lap_clock;
pub mod kart_objects;
pub mod kart_progress;
pub mod kart_ramp;
pub mod kart_scripted_motion;
pub mod kart_story_target;
pub mod kart_timeout;
pub mod kart_vehicle_init;
pub mod message_bank;
pub mod mission_ranks;
pub mod stage20;
pub mod stage_fog;
pub mod stage_land;
pub mod stage_lighting;
pub mod stage_result;
pub mod task_coverage;

pub mod kart_fall_recovery;

pub mod kart_driving_audio;

pub mod city_motion_registry;
pub mod finalhazard;
pub mod finalhazard_visual;
pub mod kart_balloon;
pub mod kart_object_range;
pub mod kart_pickup;
pub mod kart_registry;
pub mod ring_models;
mod ring_terrain;
pub mod spring_draw;
pub mod story_movie;
