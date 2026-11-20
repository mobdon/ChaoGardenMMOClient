//! The standing step fn_80054D04 and its idle voice fn_8005118C (GSNE8P
//! main.dol), checked against main.dol by tests/player_stand.rs.
//!
//! `rand` is the engine's MSL `rand` (0x8010DC88), which the runtime owns.
//! The voice tables (0x801B3168 and 0x801B3078 in main.dol) are game data:
//! the caller loads them from the player's files.
// Negated comparisons keep the compiled branches' NaN behaviour.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::{
    enemy_lib::engine::round_off,
    land_contact::unit_vector,
    ninja_matrix::{rotate_by_angles, MatrixStack, Vec3},
    ninja_trig::{fctiwz, nj_cos},
    ninja_vector::{inner_product, vector_angle},
    walker_collide::{diff_angle, plane_distance, Plane},
    walker_exec::EngineCall,
};

/// What fn_80054D04 reads and writes.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StandWork {
    /// Player byte +0 (player number), +1 (character), +3 (voice
    /// character).
    pub pnum: i8,
    pub character: i8,
    pub voice_char: i8,
    /// +0x18: frames standing idle.
    pub idle: i16,
    /// +0x44: written 0.1 on a turn on a slope.
    pub f44: f32,
    /// +0x7C: the floor normal; +0x88 `attr`.
    pub normal: Vec3,
    pub attr: u32,
    /// +0x176: the action; +0x17C cleared when an idle starts.
    pub action: i16,
    pub h17c: i16,
}

/// The game state fields read (0x801CC168 +0x1E, +0x39 and the frame
/// counter at +0x7C).
#[derive(Debug, Clone, Copy, Default)]
pub struct StandGame {
    pub b1e: i8,
    pub b39: i8,
    pub frame: u32,
    pub stage: i32,
}

/// The voice tables: (character, voice, alternate) rows of 0x801B3168
/// and (character, stage, voice, alternate) rows of 0x801B3078.
pub struct VoiceTables<'a> {
    pub any_stage: &'a [(i16, i16, i16)],
    pub by_stage: &'a [(i8, i8, i16, i16)],
}

const RAND_SCALE: f32 = f32::from_bits(0x3800_0000); // r2-0x73D0, 1 / 32768
const IDLE_PICK: f32 = f32::from_bits(0x3ffe_b852); // r2-0x7344, 1.99
const VOICE_SPLIT: f32 = 0.5; // r2-0x7398
const TURN_SLIDE: f32 = f32::from_bits(0x3dcc_cccd); // r2-0x73B0, 0.1
const NO_VOICE_STAGE: i32 = 0x5A;

/// fn_8005118C(task, player, pick): the idle voice line for player 1.
pub fn idle_voice(
    w: &StandWork,
    game: &StandGame,
    tables: &VoiceTables,
    pick: i32,
    rand: &mut dyn FnMut() -> i32,
) -> Option<EngineCall> {
    if w.pnum != 0 || game.b1e != 0 || game.b39 != 0 || game.stage == NO_VOICE_STAGE {
        return None;
    }
    let r = RAND_SCALE * rand() as f32;
    let c = w.voice_char as i32;
    let mut voice: i16 = -1;
    if r > VOICE_SPLIT {
        if let Some(e) = tables.any_stage.iter().find(|e| e.0 as i32 == c) {
            voice = if pick & 1 != 0 { e.2 } else { e.1 };
        }
    } else if let Some(e) = tables
        .by_stage
        .iter()
        .find(|e| e.1 as i32 == game.stage && e.0 as i32 == c)
    {
        let k = fctiwz(game.frame as f32 * r);
        voice = if k & 1 != 0 { e.2 } else { e.3 };
    }
    (voice != -1).then_some(EngineCall::PlayVoice { id: voice })
}

fn in_idle(c: i8, a: i16) -> bool {
    let a = a as i32;
    let (lo, hi) = match c {
        0 => (0x58, 0x5A),
        1 => (0xB0, 0xB2),
        2 | 3 | 4 | 7 => (0x57, 0x59),
        5 => (0x8D, 0x8F),
        6 => (0x8B, 0x8D),
        _ => return false,
    };
    a >= lo && a <= hi
}

const IDLE_BASE: [i32; 8] = [0x58, 0xB0, 0x57, 0x57, 0x57, 0x8D, 0x8B, 0x57];

/// fn_80054D04(task, motion, player): pick the standing action. On flat
/// enough ground (floor within 0x11C7 of up, or `attr` bit 0x100) a
/// standing player waits and, after 900 frames, plays one of its
/// character's idles at random (with the idle voice); otherwise, on a
/// slope within 0x2000 of up, it turns (action 6) when the aim is more than
/// 0x200 off its yaw, or slides down (4) or up (5) by where the slope
/// faces. `hook` is the character 5 check through the function pointer at
/// 0x801A2A44 (when the motion table's entry 0xD8 is loaded).
#[allow(clippy::too_many_arguments)]
pub fn stand_check(
    stack: &mut MatrixStack,
    w: &mut StandWork,
    angles: [i32; 3],
    aim: i32,
    gravity: Vec3,
    game: &StandGame,
    tables: &VoiceTables,
    rand: &mut dyn FnMut() -> i32,
    hook: &mut dyn FnMut() -> bool,
) -> Vec<EngineCall> {
    let mut out = vec![];
    let a = w.action as i32;
    if a == 0x1A || ((a - 0x3F) & 0xFFFF) <= 1 || ((a - 0x10) & 0xFFFF) <= 1 || a == 0x1B {
        return out;
    }
    let c = w.character;
    if (c == 4 || c == 5) && a == 0x74 {
        return out;
    }
    let up = -inner_product(gravity, w.normal);
    if up > nj_cos(0x11C7) || w.attr & 0x100 != 0 {
        if c == 5 && hook() {
            w.action = 0xD8;
        } else if (0..=3).contains(&a) || in_idle(c, a as i16) {
            if w.action == 1 && w.idle > 0xB4 {
                w.idle = 0;
                w.action = 0;
            } else if w.idle > 0x384 {
                let r = rand() as f32;
                let pick = fctiwz(round_off((RAND_SCALE * r) * IDLE_PICK));
                w.idle = 0;
                if (0..8).contains(&(c as i32)) {
                    w.action = (IDLE_BASE[c as usize] + pick) as i16;
                }
                out.extend(idle_voice(w, game, tables, pick, rand));
                w.h17c = 0;
            }
        } else {
            w.action = 0;
        }
    } else if up > nj_cos(0x2000) {
        if diff_angle(aim, angles[1]) > 0x200 {
            w.action = 6;
            w.f44 = TURN_SLIDE;
        } else {
            let floor = Plane {
                point: [0.0; 3],
                normal: w.normal,
            };
            let fwd = rotate_by_angles(stack, angles, [1.0, 0.0, 0.0]);
            let mut f = [0.0; 3];
            plane_distance(fwd, &floor, Some(&mut f));
            let f = unit_vector(f);
            let mut g = [0.0; 3];
            plane_distance(gravity, &floor, Some(&mut g));
            let g = unit_vector(g);
            let d = vector_angle(f, g, Some(w.normal));
            w.action = if d > 0xC000 || d < 0x4000 { 5 } else { 4 };
        }
    }
    if in_idle(c, w.action) {
        if game.frame & 1 != 0 {
            w.idle = w.idle.wrapping_add(1);
        }
    } else {
        w.idle = w.idle.wrapping_add(1);
    }
    out
}
