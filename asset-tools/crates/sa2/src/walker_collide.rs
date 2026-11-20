//! The walker's collision helpers under fn_8003EAFC and fn_80043ED4
//! (GSNE8P main.dol), ported instruction by instruction and checked against
//! main.dol by tests/walker_collide.rs.
use crate::{
    land_contact::unit_vector, land_nodes::nj_scalor, ninja_matrix::Vec3, ninja_trig::fnmsubs,
    ninja_vector::inner_product,
};

const ZERO: f32 = 0.0; // r2-0x75F8
const LIFT_FALLING: f32 = f32::from_bits(0x3f19_999a); // r2-0x7514, 0.6
const LIFT_RISING: f32 = f32::from_bits(0x3e4c_cccd); // r2-0x7590, 0.2
const LIFT_SLOW: f32 = f32::from_bits(0x3e99_999a); // r2-0x7580, 0.3
const LIFT_STEEP_DOWN: f32 = f32::from_bits(0xbf19_999a); // r2-0x74F4, -0.6
const LIFT_STEEP_SCALE: f32 = f32::from_bits(0x3fe6_6666); // r2-0x74F0, 1.8
const LIFT_SLOPE: f32 = f32::from_bits(0x3d4c_cccd); // r2-0x74EC, 0.05
const LIFT_SLOPE_SCALE: f32 = f32::from_bits(0x3fd3_3333); // r2-0x74E8, 1.65
const LIFT_FLAT_SCALE: f32 = f32::from_bits(0x3fb3_3333); // r2-0x74E4, 1.4
const LIFT_DOWNHILL: f32 = f32::from_bits(0xbdcc_cccd); // r2-0x75A0, -0.1
const LIFT_LEVEL_Y: f32 = 0.25; // r2-0x7574
const LIFT_LEVEL_SCALE: f32 = 1.5; // r2-0x74FC
const HALF: f32 = 0.5; // r2-0x75CC

/// The inputs fn_800428F8 reads.
#[derive(Debug, Clone, Copy)]
pub struct LiftInput {
    /// The motion this frame (r3).
    pub motion: Vec3,
    /// Player work +0xC4 (PhysicsArray +4): the collision height.
    pub height: f32,
    /// `last_attr` (+0x8C).
    pub last_attr: u32,
    /// `spd.x`, `spd.y` (+0x64, +0x68).
    pub spd_x: f32,
    pub spd_y: f32,
    /// Player work halfword +0x1C (status bits).
    pub status: i16,
    /// PhysicsArray `run_speed` (+0xE8) and the speed at +0xF4.
    pub run_speed: f32,
    pub speed_f4: f32,
    /// The collision struct's up vector (+0x1C).
    pub up: Vec3,
    /// The global vector at 0x801E5624.
    pub global: Vec3,
}

/// fn_800428F8: the collision sphere's offset from the player, given the
/// offset's current y (`out_y`). Returns (0, y, 0): the routine zeroes x and
/// z and lowers y by a share of the height that depends on the floor, the
/// slope and the speed.
pub fn sphere_lift(i: &LiftInput, out_y: f32) -> Vec3 {
    let mut h = i.height;
    let out = |y: f32| [ZERO, y, ZERO];
    let ceiling = i.last_attr & 0x100 != 0;
    if ceiling && i.motion[1] <= ZERO {
        return out(fnmsubs(LIFT_FALLING, h, out_y));
    }
    if ceiling && i.motion[1] > ZERO {
        return out(fnmsubs(LIFT_RISING, h, out_y));
    }
    if !(i.spd_y <= ZERO) {
        if i.status & 0x2000 != 0 {
            return out(out_y - h);
        }
        return out(out_y);
    }
    if i.last_attr & 0x8 != 0 {
        h *= LIFT_SLOW;
    } else {
        let d = -inner_product(i.global, i.up);
        if i.status & 0x2000 != 0 {
            h *= if d <= LIFT_STEEP_DOWN {
                LIFT_STEEP_SCALE
            } else if d <= LIFT_SLOPE {
                LIFT_SLOPE_SCALE
            } else {
                LIFT_FLAT_SCALE
            };
        } else if (i.spd_x >= i.speed_f4 && i.status & 3 != 0) || d <= LIFT_DOWNHILL {
            // Unchanged.
        } else if d > LIFT_DOWNHILL && d < LIFT_RISING {
            let ty = if nj_scalor(i.motion) == ZERO {
                ZERO
            } else {
                unit_vector(i.motion)[1]
            };
            h *= if ty.abs() < LIFT_LEVEL_Y {
                LIFT_LEVEL_SCALE
            } else {
                LIFT_SLOW
            };
        } else if i.spd_x <= i.run_speed {
            h *= HALF;
        } else if i.status & 3 != 0 {
            h *= i.spd_x / i.speed_f4;
        }
    }
    out(out_y - h)
}
