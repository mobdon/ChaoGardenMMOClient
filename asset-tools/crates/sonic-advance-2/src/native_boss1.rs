//! Native Egg Hammer Tank II arithmetic and attack state (A2NE boss 1).
//! Collision acceptance, rendering, shared task order and destruction/ending
//! scenes are explicit host services. This is not an MMO combat authority.
use crate::{
    Error, VerifiedRom, bounded_range, native_goal::ScoreState, native_runtime::PseudoRng,
    native_stage::NativeStage,
};
use serde::Serialize;

pub const TABLE_OFFSET: usize = 0xd7a18;
pub const PALETTE_OFFSET: usize = 0xd7ad0;
pub const SINE_OFFSET: usize = 0x97aa4;

fn short<const N: usize>(bytes: &[u8], at: usize) -> Result<[i16; N], Error> {
    let raw = bounded_range(bytes, at, N * 2)?;
    Ok(std::array::from_fn(|i| {
        i16::from_le_bytes([raw[i * 2], raw[i * 2 + 1]])
    }))
}
fn long<const N: usize>(bytes: &[u8], at: usize) -> Result<[i32; N], Error> {
    let raw = bounded_range(bytes, at, N * 4)?;
    Ok(std::array::from_fn(|i| {
        i32::from_le_bytes(raw[i * 4..i * 4 + 4].try_into().expect("bounded word"))
    }))
}

pub struct Boss1Resources<'a> {
    sine: &'a [u8],
    pub suspension: [i16; 8],
    pub arm_follow: [i16; 8],
    pub arm_positions: [i32; 8],
    pub hammer_positions: [i32; 8],
    pub hammer_follow: [i16; 16],
    pub extension_speeds: [i16; 5],
    pub plunge_offsets: [u8; 5],
    palettes: [[u16; 16]; 2],
}
impl<'a> Boss1Resources<'a> {
    pub fn load(rom: &'a VerifiedRom<'_>) -> Result<Self, Error> {
        let bytes = rom.rom_bytes();
        let palettes = short::<32>(bytes, PALETTE_OFFSET)?;
        Ok(Self {
            sine: bounded_range(bytes, SINE_OFFSET, 0xa00)?,
            suspension: short(bytes, TABLE_OFFSET)?,
            arm_follow: short(bytes, 0xd7a28)?,
            arm_positions: long(bytes, 0xd7a38)?,
            hammer_positions: long(bytes, 0xd7a58)?,
            hammer_follow: short(bytes, 0xd7a78)?,
            extension_speeds: short(bytes, 0xd7a98)?,
            plunge_offsets: bounded_range(bytes, 0xd7aa2, 5)?
                .try_into()
                .expect("bounded five bytes"),
            palettes: std::array::from_fn(|i| std::array::from_fn(|j| palettes[i * 16 + j] as u16)),
        })
    }
    fn sin(&self, angle: u32) -> i32 {
        let at = (angle & 1023) as usize * 2;
        i32::from(i16::from_le_bytes([self.sine[at], self.sine[at + 1]]))
    }
    fn cos(&self, angle: u32) -> i32 {
        self.sin(angle.wrapping_add(256))
    }
    pub fn palette(&self, bank: usize) -> Option<&[u16; 16]> {
        self.palettes.get(bank)
    }
}

/// The body uses native single-tile sub_801F100; the hammer uses the distinct
/// six-tile sub_801E6D4 lookahead. Both probe layer1 vertically with step+8.
pub trait Boss1Terrain {
    fn single_floor(&self, x: i32, y: i32) -> Result<i32, Error>;
    fn hammer_floor(&self, x: i32, y: i32) -> Result<i32, Error>;
}
impl Boss1Terrain for NativeStage<'_> {
    fn single_floor(&self, x: i32, y: i32) -> Result<i32, Error> {
        let h = self.vertical_sample(x, y, 1)?.extent;
        let r = (y as u32 & 7) as i32;
        Ok(match h {
            0 => 8 - r,
            8 => !r,
            v if v > 0 => v - 1 - r,
            v => v + r,
        })
    }
    fn hammer_floor(&self, x: i32, y: i32) -> Result<i32, Error> {
        Ok(self.long_probe(x, y, 1, 8, true)?.distance)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Boss1Context {
    pub final_stage: bool,
    pub difficulty: u8,
    pub game_mode: u8,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Boss1Phase {
    RollIn,
    Active,
    DestructionBoundary,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[repr(u32)]
pub enum HammerState {
    Reset,
    Extend,
    Aim,
    Plunge,
    Slam,
    Hold,
    Drag,
    Retract,
}
impl TryFrom<u32> for HammerState {
    type Error = Error;
    fn try_from(value: u32) -> Result<Self, Error> {
        Ok(match value {
            0 => Self::Reset,
            1 => Self::Extend,
            2 => Self::Aim,
            3 => Self::Plunge,
            4 => Self::Slam,
            5 => Self::Hold,
            6 => Self::Drag,
            7 => Self::Retract,
            _ => return Err(Error("Unknown native hammer state".into())),
        })
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Boss1Events {
    pub unlock_player_input: bool,
    pub run_collision_services: bool,
    pub begin_destruction: bool,
    pub sound: Option<u16>,
    pub screen_shake: bool,
    pub render: bool,
    pub music_request: Option<u8>,
    pub score_awarded: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Boss1State {
    pub context: Boss1Context,
    pub phase: Boss1Phase,
    pub x: i32,
    pub y: i32,
    pub speed_x: i16,
    pub speed_y: i16,
    pub arm_positions: [i32; 8],
    pub arm_angles: [i32; 8],
    pub arm_base_sin: u32,
    pub arm_base_angle: u32,
    pub hammer_positions: [i32; 8],
    pub hammer_angles: [i32; 8],
    pub hammer_angle: i32,
    pub front_axle_y: i32,
    pub rear_axle_y: i32,
    pub hammer_state: HammerState,
    pub hammer_extension: u32,
    pub scene_timer: u32,
    pub timer: u32,
    pub health: u8,
    pub invulnerability: u8,
    pub pilot_laugh_timer: u8,
    pub pilot_variant: u8,
    pub pilot_previous_variant: i16,
}
impl Boss1State {
    /// Host creation also locks player input and seeds the shared PseudoRng with
    /// native stage time. This constructor does not mutate either global service.
    pub fn create(r: &Boss1Resources<'_>, context: Boss1Context) -> Result<Self, Error> {
        if context.game_mode > 2 {
            return Err(Error("Boss1 multiplayer execution is not ported".into()));
        }
        let health = if context.final_stage {
            if context.difficulty != 0 { 3 } else { 4 }
        } else if context.difficulty != 0 && context.game_mode != 2 {
            6
        } else {
            8
        };
        Ok(Self {
            context,
            phase: Boss1Phase::RollIn,
            x: 980 * 256,
            y: 155 * 256,
            speed_x: 1280,
            speed_y: 0,
            arm_positions: r.arm_positions,
            arm_angles: [0; 8],
            arm_base_sin: 0,
            arm_base_angle: 0,
            hammer_positions: r.hammer_positions,
            hammer_angles: [768; 8],
            hammer_angle: 768,
            front_axle_y: 4736,
            rear_axle_y: 4736,
            hammer_state: HammerState::Reset,
            hammer_extension: 2,
            scene_timer: 0,
            timer: 120,
            health,
            invulnerability: 0,
            pilot_laugh_timer: 0,
            pilot_variant: 0,
            pilot_previous_variant: -1,
        })
    }
    /// Ground motion and hammer state run before native actor collision.
    /// On error neither state nor the shared RNG is advanced.
    pub fn motion_tick(
        &mut self,
        r: &Boss1Resources<'_>,
        terrain: &impl Boss1Terrain,
        rng: &mut PseudoRng,
    ) -> Result<Boss1Events, Error> {
        let mut next = self.clone();
        let mut random = *rng;
        let events = next.motion_inner(r, terrain, &mut random)?;
        *self = next;
        *rng = random;
        Ok(events)
    }
    fn motion_inner(
        &mut self,
        r: &Boss1Resources<'_>,
        terrain: &impl Boss1Terrain,
        rng: &mut PseudoRng,
    ) -> Result<Boss1Events, Error> {
        let mut out = Boss1Events::default();
        match self.phase {
            Boss1Phase::DestructionBoundary => {
                return Err(Error("Native boss1 destruction scene is not ported".into()));
            }
            Boss1Phase::RollIn => {
                self.scene_timer = self.scene_timer.wrapping_add(1);
                if self.scene_timer < 21 {
                    self.speed_x = 0;
                    return Ok(out);
                }
                if self.scene_timer < 127 {
                    let t = self.scene_timer;
                    let d = t.wrapping_sub(61);
                    self.x = t
                        .wrapping_mul(1280)
                        .wrapping_add(1210 * 256)
                        .wrapping_add(d.wrapping_mul(d).wrapping_mul(d) >> 1)
                        .wrapping_sub(d.wrapping_mul(48).wrapping_mul(d) >> 1)
                        .wrapping_add(d >> 1) as i32;
                }
                self.x = self.x.wrapping_add(i32::from(self.speed_x));
                self.y = self.y.wrapping_add(i32::from(self.speed_y));
                if self.scene_timer > 126 {
                    out.unlock_player_input = true;
                    self.speed_x = 1280;
                    self.phase = Boss1Phase::Active;
                }
                self.body_physics(r, terrain)?;
                self.update_arm(r);
            }
            Boss1Phase::Active => {
                self.x = self.x.wrapping_add(i32::from(self.speed_x));
                self.y = self.y.wrapping_add(i32::from(self.speed_y));
                self.body_physics(r, terrain)?;
                out = self.hammer_step(r, terrain, rng)?;
                self.update_arm(r);
                out.run_collision_services = true;
            }
        }
        out.render = true;
        Ok(out)
    }
    pub fn body_physics(
        &mut self,
        r: &Boss1Resources<'_>,
        terrain: &impl Boss1Terrain,
    ) -> Result<(), Error> {
        if self.is_touching_terrain(terrain)? {
            self.on_terrain_step(r, terrain)
        } else {
            self.falling_step();
            Ok(())
        }
    }
    pub fn is_touching_terrain(&self, terrain: &impl Boss1Terrain) -> Result<bool, Error> {
        let rear = terrain.single_floor(
            self.x.wrapping_add(6144) >> 8,
            self.y.wrapping_add(self.rear_axle_y).wrapping_add(3712) >> 8,
        )?;
        Ok(rear <= 0
            || terrain.single_floor(
                self.x.wrapping_sub(5632) >> 8,
                self.y.wrapping_add(self.front_axle_y).wrapping_add(3712) >> 8,
            )? <= 0)
    }
    pub fn falling_step(&mut self) {
        self.speed_y = self.speed_y.wrapping_add(64);
        self.rear_axle_y = self.rear_axle_y.wrapping_add(288).min(6016);
        self.front_axle_y = self.front_axle_y.wrapping_add(288).min(6016);
    }
    pub fn on_terrain_step(
        &mut self,
        r: &Boss1Resources<'_>,
        terrain: &impl Boss1Terrain,
    ) -> Result<(), Error> {
        self.speed_y = 0;
        let original_y = self.y;
        let result = terrain.single_floor(
            self.x.wrapping_add(6144) >> 8,
            original_y.wrapping_add(self.rear_axle_y).wrapping_add(3712) >> 8,
        )?;
        Self::axle_physics(&mut self.y, &mut self.rear_axle_y, result, r)?;
        let result = terrain.single_floor(
            self.x.wrapping_sub(5632) >> 8,
            original_y
                .wrapping_add(self.front_axle_y)
                .wrapping_add(3712)
                >> 8,
        )?;
        Self::axle_physics(&mut self.y, &mut self.front_axle_y, result, r)
    }
    fn axle_physics(
        y: &mut i32,
        axle: &mut i32,
        result: i32,
        r: &Boss1Resources<'_>,
    ) -> Result<(), Error> {
        if result < 1 {
            *y = y.wrapping_add(result.wrapping_mul(64));
            let index = result
                .checked_neg()
                .ok_or_else(|| Error("Invalid native suspension probe".into()))?
                .min(7) as usize;
            *axle = axle.wrapping_sub(
                (index as i32)
                    .wrapping_mul(i32::from(r.suspension[index]))
                    .wrapping_sub(index as i32 * 64),
            );
        } else {
            *axle = axle.wrapping_add(288);
        }
        *y = y.wrapping_sub(4736i32.wrapping_sub(*axle).wrapping_mul(96) >> 8);
        *axle = (*axle).clamp(3712, 6016);
        Ok(())
    }
    pub fn update_arm(&mut self, r: &Boss1Resources<'_>) {
        self.arm_base_sin = self.arm_base_sin.wrapping_add(8) & 1023;
        self.arm_base_angle = (r.sin(self.arm_base_sin) >> 8) as u32;
        self.arm_angles[0] = self.arm_base_angle as i32;
        for i in 1..8 {
            self.arm_angles[i] = self.arm_angles[i].wrapping_add(
                self.arm_angles[i - 1]
                    .wrapping_sub(self.arm_angles[i])
                    .wrapping_mul(i32::from(r.arm_follow[i]))
                    .wrapping_sub(3072)
                    >> 8,
            );
        }
    }
    fn extension_value(&self, values: &[i16; 5]) -> i32 {
        let mut bit = 1;
        for value in values {
            if self.hammer_extension & bit != 0 {
                return i32::from(*value);
            }
            bit <<= 1;
        }
        bit as i32
    }
    fn follow_hammer(&mut self, r: &Boss1Resources<'_>, loose: bool) {
        self.hammer_angles[0] = self.hammer_angle;
        for i in 1..8 {
            self.hammer_angles[i] = self.hammer_angles[i].wrapping_add(
                self.hammer_angles[i - 1]
                    .wrapping_sub(self.hammer_angles[i])
                    .wrapping_mul(i32::from(r.hammer_follow[i + if loose { 8 } else { 0 }]))
                    >> 8,
            );
        }
    }
    pub fn hammer_step(
        &mut self,
        r: &Boss1Resources<'_>,
        terrain: &impl Boss1Terrain,
        rng: &mut PseudoRng,
    ) -> Result<Boss1Events, Error> {
        let mut out = Boss1Events::default();
        match self.hammer_state {
            HammerState::Reset => {
                self.hammer_angle = self.hammer_angle.wrapping_sub(8).max(768);
                self.follow_hammer(r, true);
                self.timer = self.timer.wrapping_sub(1);
                if self.timer == 0 {
                    self.timer = 10;
                    self.hammer_state = HammerState::Extend;
                    self.hammer_angles = [768; 8];
                    self.hammer_positions = r.hammer_positions;
                    self.hammer_angle = 768;
                    self.hammer_extension = if self.hammer_extension & 2 != 0 {
                        1
                    } else {
                        self.hammer_extension.wrapping_shl(1)
                    };
                }
            }
            HammerState::Extend => {
                let value = self.extension_value(&r.extension_speeds) as i16;
                let mut acc = 0i16;
                for position in &mut self.hammer_positions {
                    acc = acc.wrapping_add(value);
                    *position = position.wrapping_add(i32::from(acc));
                }
                self.timer = self.timer.wrapping_sub(1);
                if self.timer == 0 {
                    self.timer = 15;
                    self.hammer_state = HammerState::Aim;
                }
            }
            HammerState::Aim => {
                self.timer = self.timer.wrapping_sub(1);
                if self.timer == 0 {
                    self.timer = 60;
                    self.hammer_state = HammerState::Plunge;
                }
            }
            HammerState::Plunge => {
                if self.timer > 60 {
                    return Err(Error("Native boss plunge timer outside0..60".into()));
                }
                let mut value = 1u8;
                for offset in r.plunge_offsets {
                    if self.hammer_extension & u32::from(value) != 0 {
                        value = offset;
                        break;
                    }
                    value = value.wrapping_shl(1);
                }
                self.hammer_angle = self
                    .hammer_angle
                    .wrapping_sub(r.sin(60u32.wrapping_sub(self.timer) * 4) >> 9)
                    & 1023;
                self.follow_hammer(r, false);
                let angle = self.hammer_angles[7].wrapping_sub(i32::from(value)) as u32;
                let x = (self.x >> 8)
                    .wrapping_add(self.hammer_positions[7].wrapping_mul(r.cos(angle)) >> 23);
                let y = (self.y >> 8)
                    .wrapping_add(self.hammer_positions[7].wrapping_mul(r.sin(angle)) >> 23);
                let distance = terrain.hammer_floor(x, y)?;
                if distance < 1 {
                    out.sound = Some(238);
                    out.screen_shake = true;
                    self.timer = 30;
                    self.hammer_state = HammerState::Slam;
                    let angle = self.hammer_angles[7].wrapping_add(i32::from(value)) as u32;
                    let angle = vector_angle(
                        (r.sin(angle) >> 6) as i16,
                        (r.cos(angle) >> 6).wrapping_add(distance) as i16,
                    );
                    self.hammer_angles = [i32::from(angle); 8];
                    self.hammer_angle = i32::from(angle);
                } else {
                    self.timer = self.timer.wrapping_sub(1);
                    if self.timer == 0 {
                        self.timer = 30;
                        self.hammer_state = HammerState::Slam;
                    }
                }
            }
            HammerState::Slam => {
                self.timer = self.timer.wrapping_sub(1);
                if self.timer == 0 {
                    if rng.next_u32() & 3 != 0 {
                        self.timer = 68;
                        self.hammer_state = HammerState::Hold;
                    } else {
                        out.sound = Some(239);
                        self.timer = 60;
                        self.hammer_state = HammerState::Drag;
                    }
                }
            }
            HammerState::Hold => {
                self.hammer_angle = self.hammer_angle.wrapping_add(8);
                self.follow_hammer(r, true);
                let value = self.extension_value(&r.extension_speeds) * 2;
                // The native no-bit fallback remains32, not64.
                let value = if self.hammer_extension & 31 == 0 {
                    32
                } else {
                    value
                };
                if self.timer < 58 {
                    for i in 0..8 {
                        self.hammer_positions[i] = self.hammer_positions[i]
                            .wrapping_sub(value)
                            .max(r.hammer_positions[i]);
                    }
                }
                self.timer = self.timer.wrapping_sub(1);
                if self.timer == 0 {
                    self.timer = 80;
                    self.hammer_state = HammerState::Reset;
                }
            }
            HammerState::Drag => {
                let value = if self.hammer_extension & 31 == 0 {
                    32
                } else {
                    self.extension_value(&r.extension_speeds) >> 2
                };
                let mut acc = value;
                for i in 0..8 {
                    acc += value;
                    self.hammer_positions[i] = self.hammer_positions[i].wrapping_sub(acc);
                    if self.hammer_positions[i] < r.hammer_positions[i] {
                        self.hammer_positions[i] = r.hammer_positions[i];
                        self.timer = 68;
                        self.hammer_state = HammerState::Hold;
                    }
                }
                self.timer = self.timer.wrapping_sub(1);
                if self.timer == 0 {
                    self.timer = 68;
                    self.hammer_state = HammerState::Hold;
                }
            }
            HammerState::Retract => {
                self.hammer_angle = self.hammer_angle.wrapping_add(12);
                // Original BUG_FIX is disabled: it compares timer, not angle.
                if self.timer > 768 {
                    self.hammer_angle = 768;
                }
                self.follow_hammer(r, false);
                self.timer = self.timer.wrapping_sub(1);
                if self.timer == 0 {
                    self.timer = 80;
                    self.hammer_state = HammerState::Reset;
                }
            }
        }
        Ok(out)
    }
    /// Call only after native collision accepts an attack. The original helper
    /// itself does not gate invulnerability; the collision task owns that gate.
    pub fn handle_hit(&mut self, score: &mut ScoreState) -> Boss1Events {
        let mut out = Boss1Events::default();
        if self.health != 0 {
            self.health -= 1;
            out.sound = Some(if self.health & 1 != 0 { 143 } else { 235 });
            self.invulnerability = 30;
            self.pilot_variant = if self.health == 0 { 3 } else { 2 };
            self.pilot_previous_variant = -1;
            if self.health == 0 {
                score.award(1000, self.context.game_mode, true);
                out.score_awarded = 1000;
            }
        }
        if !self.context.final_stage && self.health == 4 {
            out.music_request = Some(0x11);
        }
        out
    }
    pub fn pilot_laugh(&mut self) {
        self.pilot_laugh_timer = 30;
        if self.invulnerability == 0 {
            self.pilot_variant = 1;
            self.pilot_previous_variant = -1;
        }
    }
    pub fn update_pilot(&mut self) {
        if self.invulnerability > 0 {
            self.pilot_laugh_timer = 0;
            self.invulnerability -= 1;
            if self.invulnerability > 0 {
                return;
            }
            self.pilot_variant = if self.health == 0 { 3 } else { 0 };
            self.pilot_previous_variant = -1;
        } else if self.pilot_laugh_timer > 0 {
            self.pilot_laugh_timer -= 1;
            if self.pilot_laugh_timer == 0 {
                self.pilot_variant = 0;
                self.pilot_previous_variant = -1;
            }
        }
    }
    /// Native order: motion, actor collisions, pilot timers, render/palette,
    /// then zero-health destruction. Destruction remains an explicit boundary.
    pub fn finish_active_frame(&mut self) -> Boss1Events {
        self.update_pilot();
        let begin_destruction = self.health == 0;
        if begin_destruction {
            self.phase = Boss1Phase::DestructionBoundary;
        }
        Boss1Events {
            begin_destruction,
            ..Default::default()
        }
    }
    pub fn flashing_palette<'a>(&self, r: &'a Boss1Resources<'_>) -> Option<&'a [u16; 16]> {
        (self.invulnerability > 0)
            .then(|| &r.palettes[usize::from((self.invulnerability & 4) >> 2)])
    }
    pub fn hammer_center(&self, r: &Boss1Resources<'_>) -> Option<(i32, i32)> {
        if self.invulnerability != 0
            || matches!(self.hammer_state, HammerState::Hold | HammerState::Retract)
        {
            return None;
        }
        let a = self.hammer_angles[5] as u32;
        Some((
            (self.x >> 8)
                .wrapping_add(self.hammer_positions[5].wrapping_mul(r.cos(a)) >> 23)
                .wrapping_sub(8),
            (self.y >> 8).wrapping_add(self.hammer_positions[5].wrapping_mul(r.sin(a)) >> 23),
        ))
    }
}

/// Native sub_8004418 octant ratio, preserving each s16 compound assignment.
/// This is distinct from BIOS ArcTan2 and retains narrowing at signed extremes.
pub fn vector_angle(mut x: i16, mut y: i16) -> i16 {
    if x | y == 0 {
        return -1;
    }
    let mut index = 0usize;
    if x <= 0 {
        x = x.wrapping_neg();
        index = 4;
    }
    if y <= 0 {
        y = y.wrapping_neg();
        index += 2;
    }
    let mut fraction = if x >= y {
        y = y.wrapping_mul(128);
        if x == 0 {
            y
        } else {
            (i32::from(y) / i32::from(x)) as i16
        }
    } else {
        index += 1;
        x = x.wrapping_mul(128);
        if y == 0 {
            x
        } else {
            (i32::from(x) / i32::from(y)) as i16
        }
    };
    let octant = [1, 0, 2, 3, 6, 7, 5, 4][index];
    if octant & 1 != 0 {
        fraction = 128i16.wrapping_sub(fraction);
    }
    fraction = fraction.wrapping_add(octant * 128);
    fraction & 1023
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_angle_is_not_bios_atan_and_preserves_zero() {
        assert_eq!(vector_angle(0, 0), -1);
        assert_eq!(vector_angle(1, 0), 256);
        assert_eq!(vector_angle(0, 1), 0);
        assert_eq!(vector_angle(-1, 0), 768);
    }
}
