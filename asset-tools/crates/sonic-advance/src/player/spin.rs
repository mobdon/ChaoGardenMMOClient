use super::*;
use std::sync::Arc;

const RELEASE_TABLE: usize = 0x4a7084;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Spin {
    pub(super) mode: SpinMode,
    charge: i32,
    pub(super) roll_jump: bool,
    speeds: Option<Arc<[i16; 128]>>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum SpinMode {
    #[default]
    Standing,
    Crouched,
    Uncrouching,
    Charging,
    Revving,
    Rolling,
}

impl Spin {
    pub(super) fn load(&mut self, rom: &[u8]) -> Result<()> {
        // The retail release indexes the ROM directly. Rapid taps can index
        // beyond the nine named speed entries; preserve that ASOE read.
        let raw = binary::range(rom, RELEASE_TABLE, 256)?;
        if binary::sha256(raw) != "1ca89e36168b3d53d4839570a61b6e7e5f6d92550117236c02b493e8f633ce9d"
        {
            return Err(Error("ASOE spindash table mismatch".into()));
        }
        let mut entries = [0i16; 128];
        for (entry, bytes) in entries.iter_mut().zip(raw.as_chunks::<2>().0.iter()) {
            *entry = i16::from_le_bytes(*bytes);
        }
        self.speeds = Some(Arc::new(entries));
        Ok(())
    }

    fn release(&self) -> Result<i32> {
        let index = (self.charge >> 8) as usize;
        if let Some(table) = &self.speeds {
            table
                .get(index)
                .map(|&v| i32::from(v))
                .ok_or_else(|| Error("spindash charge index outside ASOE range".into()))
        } else if index <= 8 {
            Ok(1536 + 96 * index as i32)
        } else {
            Err(Error(
                "extended spindash release requires native ASOE table".into(),
            ))
        }
    }
}

impl Player {
    pub fn spindash_charge(&self) -> i32 {
        self.spin.charge
    }
    pub fn rolling(&self) -> bool {
        self.spin.mode == SpinMode::Rolling
    }
    pub(super) fn spin_charge_frame(
        &mut self,
        down: bool,
        pressed: bool,
        out: &mut FrameOutcome,
    ) -> Result<bool> {
        use SpinMode::*;
        match self.spin.mode {
            Crouched if pressed => {
                out.spin_sound = Some(109);
                self.spin.mode = Charging;
                self.spin.charge = 0;
                self.shift_radius(SPIN_HALF_HEIGHT);
            }
            Charging | Revving if !down => {
                out.spin_sound = Some(110);
                let speed = self.spin.release()?;
                self.ground_speed = if self.facing_left { -speed } else { speed };
                self.spin.mode = Rolling;
            }
            Charging | Revving => {
                self.spin.charge -= self.spin.charge >> 5;
                if pressed {
                    out.spin_sound = Some(109);
                    out.spin_pitch = Some((self.spin.charge & !127) as u16);
                    self.spin.charge = (self.spin.charge + 512).max(2048);
                    self.spin.mode = Revving;
                }
            }
            _ => return Ok(false),
        }
        self.braking = false;
        Ok(true)
    }

    pub(super) fn ground_motion(&mut self, input: Input, down: bool) {
        use SpinMode::*;
        if self.rolling() {
            if self.angle.wrapping_add(96) < 192 && self.ground_speed != 0 {
                let mut accel = (self.trig.sin(self.angle) * 60) >> 8;
                if (self.ground_speed > 0 && accel <= 0) || (self.ground_speed < 0 && accel >= 0) {
                    accel >>= 2;
                }
                self.ground_speed += accel;
            }
            if self.adhesion_timer == 0 && input.left != input.right {
                let dir = if input.left { -1 } else { 1 };
                if self.ground_speed * dir >= 0 {
                    self.facing_left = dir < 0;
                } else {
                    self.ground_speed += dir * 24;
                    if self.ground_speed * dir > 0 {
                        self.ground_speed = dir * 96;
                    }
                }
            }
            self.ground_speed = toward_zero(
                self.ground_speed,
                (GROUND_ACCELERATION * self.speed_factor()) >> 1,
            );
            if self.ground_speed == 0 {
                self.spin.mode = if down { Crouched } else { Standing };
                self.shift_radius(STANDING_HALF_HEIGHT);
            }
            return;
        }

        if self.angle.wrapping_add(96) < 192 && self.ground_speed != 0 {
            self.ground_speed += (self.trig.sin(self.angle) * 3) >> 5;
        }
        self.braking = false;
        if self.adhesion_timer == 0 {
            match (input.left, input.right) {
                (true, false) => self.accelerate_ground(-1),
                (false, true) => self.accelerate_ground(1),
                _ => {}
            }
        }
        if down {
            if self.ground_speed != 0 {
                self.shift_radius(SPIN_HALF_HEIGHT);
                self.spin.mode = Rolling;
            } else {
                self.spin.mode = Crouched;
            }
        } else if self.spin.mode == Crouched {
            self.spin.mode = Uncrouching;
        }
        if input.left || input.right {
            if self.spin.mode == Uncrouching {
                self.spin.mode = Standing;
            }
        } else {
            self.ground_speed = toward_zero(self.ground_speed, GROUND_FRICTION);
        }
    }
}
