//! Prison Lane's visual stage objects, as their stg09D.rel callbacks keep
//! their state (the drawing itself is not part of this crate).
//!
//! * SIREN (0x5A30 set-up, 0x5B20 exec, 0x5BB4 draw): a spinning light. Set-up
//!   picks a random start angle, `trunc(182.04445 * (360 * (rand() *
//!   2^-15)))` kept to 16 bits, and registers a radius-5 sphere
//!   (CollisionData sec5+0x45858, form 0). Each frame the angle grows by the
//!   word at sec5+0x45854 (1120); the draw turns the light by it about Y.
//! * SEARCHLIGHT (0x56A8 / 0x5708 / 0x57A0): each frame a 16-bit phase grows
//!   by Rotation.x; the draw tilts the lamp about X by
//!   `trunc(182.04445 * (0.5 * (45 * (1 + njSin(phase)))))`, 0 to 45 degrees.
//! * SE (0x55AC / 0x560C): a sound emitter that plays sound 9 every frame
//!   when Rotation.x bit 0 is set, else sound 0xD, at its position.
//! * SIGNA (0x58D8): draw only; it keeps no state.
//!
//! `rand` is MSL's (PC import 0x8010D940, GSNE8P 0x8010DC88):
//! `seed = seed * 1103515245 + 12345`, result `(seed >> 16) & 0x7FFF`; the
//! seed starts at 1 (SDA r13-0x7C20). The game shares one seed across every
//! caller, so a session's draws follow its own call order.
use crate::ninja_trig::nj_sin;
use crate::play_world::V3;
use serde::Serialize;

/// MSL `rand`.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct MslRand {
    pub seed: u32,
}

impl Default for MslRand {
    fn default() -> Self {
        Self { seed: 1 }
    }
}

impl MslRand {
    pub fn rand(&mut self) -> u32 {
        self.seed = self.seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        (self.seed >> 16) & 0x7fff
    }
}

const BAMS_PER_DEGREE: f32 = f32::from_bits(0x4336_0b61);
/// SIREN spin per frame (sec5+0x45854).
pub const SIREN_STEP: u16 = 1120;
pub const SIREN_RADIUS: f32 = 5.0;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Siren {
    pub layer: usize,
    pub index: usize,
    pub position: V3,
    pub angle: u16,
}

impl Siren {
    pub fn new(layer: usize, index: usize, position: V3, rand: &mut MslRand) -> Self {
        let r = rand.rand() as i32 as f32;
        let degrees = 360.0 * (f32::from_bits(0x3800_0000) * r);
        let angle = (BAMS_PER_DEGREE * degrees) as i32 as u16;
        Self {
            layer,
            index,
            position,
            angle,
        }
    }

    pub fn step(&mut self) {
        self.angle = self.angle.wrapping_add(SIREN_STEP);
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Searchlight {
    pub layer: usize,
    pub index: usize,
    pub position: V3,
    pub yaw: u16,
    /// Rotation.x: phase step per frame.
    pub speed: u32,
    pub phase: u16,
}

impl Searchlight {
    pub fn step(&mut self) {
        self.phase = (u32::from(self.phase).wrapping_add(self.speed)) as u16;
    }

    /// The lamp's tilt about X (BAMS) the draw applies.
    pub fn tilt(&self) -> i32 {
        let s = nj_sin(i32::from(self.phase));
        (BAMS_PER_DEGREE * (0.5 * (45.0 * (1.0 + s)))) as i32
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoundEmitter {
    pub layer: usize,
    pub index: usize,
    pub position: V3,
    /// Sound played every frame.
    pub sound: u16,
}

impl SoundEmitter {
    pub fn new(layer: usize, index: usize, position: V3, rotation_x: u16) -> Self {
        Self {
            layer,
            index,
            position,
            sound: if rotation_x & 1 != 0 { 9 } else { 0xd },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn msl_rand_sequence() {
        let mut r = MslRand::default();
        // ANSI C reference LCG from seed 1.
        assert_eq!([r.rand(), r.rand(), r.rand()], [16838, 5758, 10113]);
    }

    #[test]
    fn searchlight_sweeps_zero_to_forty_five_degrees() {
        let mut s = Searchlight {
            layer: 0,
            index: 0,
            position: [0.0; 3],
            yaw: 0,
            speed: 0x4000,
            phase: 0,
        };
        assert_eq!(s.tilt(), (BAMS_PER_DEGREE * 22.5) as i32);
        s.step();
        assert_eq!(s.tilt(), (BAMS_PER_DEGREE * 45.0) as i32);
        s.step();
        s.step();
        assert_eq!(s.tilt(), 0);
    }

    #[test]
    fn siren_spins_from_a_random_start() {
        let mut r = MslRand::default();
        let mut s = Siren::new(0, 0, [0.0; 3], &mut r);
        // 16838 / 32768 of a turn.
        assert_eq!(
            s.angle,
            (BAMS_PER_DEGREE * (360.0 * (16838.0 / 32768.0))) as i32 as u16
        );
        let a = s.angle;
        s.step();
        assert_eq!(s.angle, a.wrapping_add(1120));
    }
}
