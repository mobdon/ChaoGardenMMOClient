//! Local-only readers for the fingerprinted Sonic Advance USA ASOE revision 0.
#![forbid(unsafe_code)]

pub mod act_intro;
pub mod animation;
pub mod binary;
pub mod catalog;
pub mod character_select;
pub mod entities;
pub mod flash;
pub mod garden;
pub mod graphics;
pub mod hud;
pub mod items;
pub mod music;
pub mod player;
pub mod posts;
pub mod rings;
pub mod runtime;
pub mod springs;
pub mod sprite;
pub mod stage;
pub mod tables;
pub mod tiny_game;
pub mod tiny_rps;
pub mod title;
pub mod transfer;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
pub mod garden_cartridge;
pub mod garden_keys;
pub mod garden_screen;
