//! EFFOBJ0, Prison Lane's random effect emitter (stg09D.rel 0x1CF60
//! set-up, 0x1CFB4 exec; the display callback is empty).
//!
//! Each frame, unless the byte at GSNE8P 0x801CC168 + 0x37 is set (PC
//! 0x801CBD88 + 0x37; the host reports it), the exec draws MSL `rand` to
//! decide whether to emit, with probability `((Rotation.x >> 8) & 0xFF) /
//! 255`. On an emission it draws three more: the velocity before rotation is
//! `(Scale.y * (u - 0.5), Scale.x + Scale.y * (u - 0.5), Scale.y * (u -
//! 0.5))` (y drawn first, then x, then z; `u = rand() * 2^-15`), turned by
//! njRotateY(Rotation.y) then njRotateZ(Rotation.z) through njCalcVector
//! (`ninja_matrix`). The size is `min(0.05 + Scale.z, 20)`. Rotation.x's
//! low byte picks one of 18 effect spawners (jump table sec5+0x54B1C); most
//! draw a fifth `rand` and scale the size by `0.9 + 0.2 * u`. The spawned
//! effect tasks are not run; `Emission` reports what each would get.
//!
//! The object range check at the top of the exec (0x8006538C) is not run.
use crate::ninja_matrix::{mult_vec_sr, rotate, Axis, IDENTITY};
use crate::play_world::V3;
use crate::prison_visuals::MslRand;
use serde::Serialize;

/// Spawner per kind: a stg09D.rel section-1 offset, or (kind 1) the main
/// program's 0x800E26E0 (PC address).
pub const SPAWNERS: [u32; 18] = [
    0x139a4,
    0x800e_26e0,
    0xf90c,
    0x1145c,
    0x11ed4,
    0x1253c,
    0x3c408,
    0x13354,
    0x145d0,
    0x10bd0,
    0x13dc8,
    0xe5e0,
    0x3c224,
    0x12f8c,
    0x11278,
    0xece0,
    0x1541c,
    0x137d0,
];

const UNIT: f32 = f32::from_bits(0x3800_0000); // sec4+0xA9C, 2^-15
const HALF: f32 = 0.5; // sec4+0xAA0
const SIZE_BASE: f32 = f32::from_bits(0x3d4c_cccd); // sec4+0xAA4, 0.05
const SIZE_MAX: f32 = 20.0; // sec4+0xAA8
const JITTER_BASE: f32 = f32::from_bits(0x3f66_6666); // sec4+0xAAC, 0.9
const JITTER: f32 = f32::from_bits(0x3e4c_cccd); // sec4+0xAB0, 0.2
const FLOOR_DROP: f32 = f32::from_bits(0x3dcc_cccd); // sec4+0xAB8, 0.1

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EmissionArgs {
    /// Most kinds: (position, velocity, size).
    Moving { velocity: V3, size: f32 },
    /// Kinds 9 and 10: (position, velocity, 0, Position.y - 0.1).
    Floor { velocity: V3, floor: f32 },
    /// Kind 15: the position and size only.
    Still { size: f32 },
    /// Kind 17: (position, velocity, size, 0).
    MovingZero { velocity: V3, size: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Emission {
    pub kind: u8,
    pub spawner: u32,
    pub position: V3,
    pub args: EmissionArgs,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectEmitter {
    pub layer: usize,
    pub index: usize,
    pub position: V3,
    /// Task rotation words (sign-extended by the SET loader).
    pub rotation: [i32; 3],
    pub scale: V3,
}

fn unit(rand: &mut MslRand) -> f32 {
    UNIT * (rand.rand() as i32 as f32)
}

/// The velocity transform: njRotateY then njRotateZ, then njCalcVector.
/// (The njTranslate before them only changes the translation column, which
/// njCalcVector ignores.)
pub fn turn_velocity(v: V3, rotation_y: i32, rotation_z: i32) -> V3 {
    let mut m = IDENTITY;
    rotate(&mut m, Axis::Y, rotation_y);
    rotate(&mut m, Axis::Z, rotation_z);
    mult_vec_sr(&m, v)
}

impl EffectEmitter {
    pub fn new(layer: usize, index: usize, position: V3, rotation: [u16; 3], scale: V3) -> Self {
        Self {
            layer,
            index,
            position,
            rotation: rotation.map(|r| i32::from(r as i16)),
            scale,
        }
    }

    pub fn step(&self, suppressed: bool, rand: &mut MslRand) -> Option<Emission> {
        let chance = ((self.rotation[0] >> 8) & 0xff) as f32 / 255.0;
        if suppressed {
            return None;
        }
        if unit(rand) >= chance {
            return None;
        }
        let [sx, sy, sz] = self.scale;
        let y = sx + sy * (unit(rand) - HALF);
        let x = sy * (unit(rand) - HALF);
        let z = sy * (unit(rand) - HALF);
        let velocity = turn_velocity([x, y, z], self.rotation[1], self.rotation[2]);
        let base = SIZE_BASE + sz;
        let base = if base > SIZE_MAX { SIZE_MAX } else { base };
        let kind = (self.rotation[0] & 0xff) as u8;
        let spawner = *SPAWNERS.get(kind as usize)?;
        let jittered = |rand: &mut MslRand| base * (JITTER_BASE + JITTER * unit(rand));
        let args = match kind {
            9 | 10 => EmissionArgs::Floor {
                velocity,
                floor: self.position[1] - FLOOR_DROP,
            },
            15 => EmissionArgs::Still {
                size: jittered(rand),
            },
            17 => EmissionArgs::MovingZero {
                velocity,
                size: jittered(rand),
            },
            _ => EmissionArgs::Moving {
                velocity,
                size: jittered(rand),
            },
        };
        Some(Emission {
            kind,
            spawner,
            position: self.position,
            args,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_emits_at_zero_chance_and_always_at_full() {
        let mut r = MslRand::default();
        let e = EffectEmitter::new(0, 0, [0.0; 3], [0x0003, 0, 0], [1.0, 2.0, 30.0]);
        for _ in 0..100 {
            assert!(e.step(false, &mut r).is_none());
        }
        let e = EffectEmitter::new(0, 0, [0.0; 3], [0xff03, 0, 0], [1.0, 2.0, 30.0]);
        let em = e.step(false, &mut r).unwrap();
        assert_eq!((em.kind, em.spawner), (3, 0x1145c));
        let EmissionArgs::Moving { size, .. } = em.args else {
            panic!()
        };
        // Size capped at 20 before the 0.9..1.1 jitter.
        assert!((18.0..=22.0).contains(&size));
        assert!(e.step(true, &mut r).is_none());
    }
}
