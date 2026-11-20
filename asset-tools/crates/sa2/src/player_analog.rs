//! fn_8004E560: the player's analog-stick direction and strength, as SA2B's
//! main.dol (GameCube, GSNE8P) computes them (the PC name is `GetAnalog`).
//!
//! The input tables are engine state the caller supplies: the per-player
//! stick angle and magnitude (0x801E53F8, 8 bytes per player), the frame
//! counter that picks a confusion remap, the stage number, the raw stick Y
//! (0x801E5438 + 0x34 * player + 0x1E) and the gravity vector's Y
//! (0x801E5628). The collision lookup fn_800060EC(player task, 0x11), which
//! finds an object of collision kind 0x11 the player is touching, is a
//! boundary too: the caller passes that object's angles, or none.
//!
//! `tests/player_analog_dol.rs` checks this against main.dol.
use crate::{
    ninja_matrix::{Axis, MatrixStack},
    ninja_vector::{inner_product, vector_angle},
};

/// What fn_8004E560 reads from outside the player.
#[derive(Debug, Clone, Copy, Default)]
pub struct AnalogContext {
    /// The player's stick angle (BAMS) and magnitude.
    pub angle: i32,
    pub magnitude: f32,
    /// Word at 0x801CC1E4; bits 0x18 pick the confusion remap.
    pub frame: u32,
    /// `GetStageNum` (r13 - 0x7AA0).
    pub stage: i16,
    /// Angles (X, Y, Z) of the collision-kind-0x11 object the player touches.
    pub ground_object: Option<[i32; 3]>,
    /// Raw stick Y for this player.
    pub stick_y: i16,
    /// Gravity vector Y.
    pub gravity_y: f32,
}

/// The player fields fn_8004E560 reads.
#[derive(Debug, Clone, Copy, Default)]
pub struct AnalogPlayer {
    /// Task work status halfword (+4).
    pub status: i16,
    /// Task work angles (+8, +0xC, +0x10).
    pub angles: [i32; 3],
    /// Player work +0x14 (no-control timer) and +0x1A (confusion timer).
    pub nocon: i16,
    pub confuse: i16,
}

/// What the routine returns: whether it produced input, and the two
/// optional outputs (written only when the caller asks for them).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Analog {
    pub active: bool,
    pub angle: Option<i32>,
    pub magnitude: Option<f32>,
}

const BOGUS_STAGES: [i16; 2] = [0x2c, 0x32];
/// r2 - 0x7498 and r2 - 0x75A8.
const STEEP_UP: f32 = f32::from_bits(0x3f7d_70a4);
const GRAVITY_FLIP: f32 = f32::from_bits(0x3dcc_cccd);

/// fn_8004E560(task work, player work, &angle, &magnitude).
pub fn get_analog(
    stack: &mut MatrixStack,
    player: AnalogPlayer,
    ctx: AnalogContext,
    want_angle: bool,
    want_magnitude: bool,
) -> Analog {
    let none = Analog {
        active: false,
        angle: None,
        magnitude: None,
    };
    if player.status & 0x4000 != 0 || ctx.magnitude == 0.0 {
        return none;
    }
    if player.status & 0x2000 == 0 && player.nocon != 0 {
        return none;
    }
    let mut angle = None;
    if want_angle {
        let mut a = ctx.angle;
        if player.confuse > 0 {
            a = match ctx.frame & 0x18 {
                0x00 => a.wrapping_add(0x4000),
                0x08 => 0x8000i32.wrapping_sub(a),
                0x10 => a.wrapping_add(0x8000),
                _ => 0x4000i32.wrapping_sub(a),
            };
        }
        if !BOGUS_STAGES.contains(&ctx.stage) {
            a = steer(stack, player.angles, ctx, a);
        }
        angle = Some(a);
    }
    Analog {
        active: true,
        angle,
        magnitude: want_magnitude.then_some(ctx.magnitude),
    }
}

fn steer(stack: &mut MatrixStack, angles: [i32; 3], ctx: AnalogContext, a: i32) -> i32 {
    let Some(obj) = ctx.ground_object else {
        return if ctx.gravity_y > GRAVITY_FLIP {
            0x8000i32.wrapping_sub(a)
        } else {
            a
        };
    };
    // The object's up vector, flipped to point up.
    stack.push();
    *stack.current_mut() = crate::ninja_matrix::IDENTITY;
    stack.rotate(Axis::Y, obj[1]);
    stack.rotate(Axis::Z, obj[2]);
    let mut up = stack.calc_vector([0.0, 1.0, 0.0]);
    if up[1] < 0.0 {
        up = up.map(|v| -v);
    }
    // The player's forward and up, through its Z and X angles.
    *stack.current_mut() = crate::ninja_matrix::IDENTITY;
    stack.rotate(Axis::Z, angles[2]);
    stack.rotate(Axis::X, angles[0]);
    let forward = stack.calc_vector([1.0, 0.0, 0.0]);
    *stack.current_mut() = crate::ninja_matrix::IDENTITY;
    stack.rotate(Axis::Z, angles[2]);
    stack.rotate(Axis::X, angles[0]);
    let side = stack.calc_vector([0.0, 1.0, 0.0]);
    stack.pop();
    let near = vector_angle(forward, up, Some(side));
    let far = near.wrapping_add(0x8000);
    if up[1] > STEEP_UP {
        let y = ((ctx.stick_y as i32) as f64) as f32;
        return if y >= 0.0 { far } else { near };
    }
    stack.push();
    *stack.current_mut() = crate::ninja_matrix::IDENTITY;
    stack.rotate(Axis::Y, a);
    let stick = stack.calc_vector([1.0, 0.0, 0.0]);
    stack.pop();
    if inner_product(stick, up) >= 0.0 {
        near
    } else {
        far
    }
}
