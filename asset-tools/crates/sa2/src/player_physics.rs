//! Original Steam PhysicsArray. Parameter names follow player_parameter and SA Tools.
use crate::{model::Reader, object_table, Result};
use serde::Serialize;

const TABLE_VA: u32 = 0x0173_91e8;
const STRIDE: usize = 132;
const CHARACTERS: [Option<&str>; 19] = [
    Some("Sonic"),
    Some("Shadow"),
    Some("Tails"),
    Some("Eggman"),
    Some("Knuckles"),
    Some("Rouge"),
    Some("MechTails"),
    Some("MechEggman"),
    Some("Amy"),
    Some("SuperSonic"),
    Some("SuperShadow"),
    Some("B"),
    Some("MetalSonic"),
    Some("ChaoWalker"),
    Some("DarkChaoWalker"),
    Some("Tikal"),
    Some("Chaos"),
    None,
    None,
];

#[derive(Debug, Serialize)]
pub struct PlayerPhysics {
    pub slot: usize,
    pub character: Option<&'static str>,
    pub executable_sha256: &'static str,
    pub row_sha256: String,
    pub source_va: u32,
    pub parameters: PlayerParameters,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct PlayerParameters {
    pub jump2_timer: i32,
    pub pos_error: f32,
    pub lim_h_spd: f32,
    pub lim_v_spd: f32,
    pub max_x_spd: f32,
    pub max_psh_spd: f32,
    pub jmp_y_spd: f32,
    pub nocon_speed: f32,
    pub slide_speed: f32,
    pub jog_speed: f32,
    pub run_speed: f32,
    pub rush_speed: f32,
    pub crash_speed: f32,
    pub dash_speed: f32,
    pub jmp_addit: f32,
    pub run_accel: f32,
    pub air_accel: f32,
    pub slow_down: f32,
    pub run_break: f32,
    pub air_break: f32,
    pub air_resist_air: f32,
    pub air_resist: f32,
    pub air_resist_y: f32,
    pub air_resist_z: f32,
    pub grd_frict: f32,
    pub grd_frict_z: f32,
    pub lim_frict: f32,
    pub rat_bound: f32,
    pub rad: f32,
    pub height: f32,
    pub weight: f32,
    pub eyes_height: f32,
    pub center_height: f32,
}

/// Read static defaults from the pinned executable without running the game.
pub fn read(exe: &[u8]) -> Result<Vec<PlayerPhysics>> {
    let image = object_table::verify_image(exe)?;
    let at = image.va_to_offset(TABLE_VA, STRIDE * CHARACTERS.len())?;
    let rows = exe[at..at + STRIDE * CHARACTERS.len()]
        .as_chunks::<STRIDE>()
        .0
        .iter();
    rows.zip(CHARACTERS)
        .enumerate()
        .map(|(slot, (bytes, character))| {
            let r = Reader { data: bytes };
            let parameters = PlayerParameters {
                jump2_timer: r.u32(0)? as i32,
                pos_error: r.f32(4)?,
                lim_h_spd: r.f32(8)?,
                lim_v_spd: r.f32(12)?,
                max_x_spd: r.f32(16)?,
                max_psh_spd: r.f32(20)?,
                jmp_y_spd: r.f32(24)?,
                nocon_speed: r.f32(28)?,
                slide_speed: r.f32(32)?,
                jog_speed: r.f32(36)?,
                run_speed: r.f32(40)?,
                rush_speed: r.f32(44)?,
                crash_speed: r.f32(48)?,
                dash_speed: r.f32(52)?,
                jmp_addit: r.f32(56)?,
                run_accel: r.f32(60)?,
                air_accel: r.f32(64)?,
                slow_down: r.f32(68)?,
                run_break: r.f32(72)?,
                air_break: r.f32(76)?,
                air_resist_air: r.f32(80)?,
                air_resist: r.f32(84)?,
                air_resist_y: r.f32(88)?,
                air_resist_z: r.f32(92)?,
                grd_frict: r.f32(96)?,
                grd_frict_z: r.f32(100)?,
                lim_frict: r.f32(104)?,
                rat_bound: r.f32(108)?,
                rad: r.f32(112)?,
                height: r.f32(116)?,
                weight: r.f32(120)?,
                eyes_height: r.f32(124)?,
                center_height: r.f32(128)?,
            };
            Ok(PlayerPhysics {
                slot,
                character,
                executable_sha256: object_table::STEAM_EXE_SHA256,
                row_sha256: crate::manifest::sha256(bytes),
                source_va: TABLE_VA + (slot * STRIDE) as u32,
                parameters,
            })
        })
        .collect()
}
