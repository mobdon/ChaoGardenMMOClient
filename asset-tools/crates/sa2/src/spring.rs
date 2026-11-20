//! ObjectSpring, ObjectSpringB and VacumePlayer (SA2B GSNE8P).
//! The host resolves CCL player slots and supplies native matrix/trig results.

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum SpringKind {
    A,
    B,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum SpringMode {
    Init,
    Normal,
    Touch,
    Done,
    Removed,
}

#[derive(Clone, Copy, Debug)]
pub struct SpringPlayer {
    pub position: [f32; 3],
    pub running_speed: f32,
    pub item: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub enum SpringCall {
    InitCollision(SpringKind),
    RegisterCollision,
    Sound(u16),
    Input(u8, u8),
    SetPosition(u8, [f32; 3]),
    SpringVelocity(u8, [f32; 3]),
    VelocityYRotationNoControl {
        slot: u8,
        velocity_y: f32,
        angles: [i32; 3],
        frames: i32,
    },
    Function8002fb2c(u8, i32, i32, i32),
    SetRelLocal(u8, i32),
    DeadOut,
}

#[derive(Debug, Clone)]
pub struct StageSpring {
    pub kind: SpringKind,
    pub position: [f32; 3],
    pub angles: [i32; 3],
    pub scale: [f32; 3],
    pub mode: SpringMode,
    pub slot: u8,
    pub contact_timer: u8,
    pub wait_timer: u16,
    pub phase_degrees: f32,
    pub amplitude: f32,
    pub compression: f32,
}

impl StageSpring {
    pub fn new(kind: SpringKind, position: [f32; 3], angles: [i32; 3], scale: [f32; 3]) -> Self {
        Self {
            kind,
            position,
            angles,
            scale,
            mode: SpringMode::Init,
            slot: 0,
            contact_timer: 0,
            wait_timer: 0,
            phase_degrees: 0.0,
            amplitude: 0.0,
            compression: 0.0,
        }
    }

    /// One in-range callback. For a non-player CCL hit use Some(2); it
    /// resets early contact debounce without selecting a player. Supply the
    /// already selected slot's live player state on the Touch tick.
    pub fn step(
        &mut self,
        hit: Option<u8>,
        player: Option<SpringPlayer>,
        out_point: [f32; 3],
        sin_cos: impl Fn(i32) -> (f32, f32),
    ) -> Vec<SpringCall> {
        use SpringCall::*;
        let mut calls = Vec::new();
        match self.mode {
            SpringMode::Init => {
                calls.push(InitCollision(self.kind));
                self.set_phase(180.0);
                self.scale[2] = 0.0;
                self.amplitude = 0.5;
                if self.kind == SpringKind::B {
                    self.contact_timer = 0;
                }
                self.mode = SpringMode::Normal;
            }
            SpringMode::Normal => {
                self.wait_timer = self.wait_timer.saturating_sub(1);
                const ANGLE: f32 = 32768.0 / 180.0;
                self.compression =
                    -(self.amplitude * sin_cos((self.phase_degrees * ANGLE) as i32).0).abs();
                self.set_phase(self.phase_degrees + self.scale[2]);
                if self.phase_degrees >= 290.0 && self.phase_degrees < self.scale[2] + 290.0 {
                    self.scale[2] = match self.kind {
                        SpringKind::A => 18.0,
                        SpringKind::B => 15.0,
                    };
                }
                self.amplitude =
                    sin_cos(((self.phase_degrees - 180.0) * 0.1 * ANGLE) as i32).1 * 0.6;
                if self.amplitude < 0.0 {
                    self.set_phase(180.0);
                    self.scale[2] = 0.0;
                }
                self.contact_timer = self.contact_timer.wrapping_add(1);
                if self.contact_timer > 2 {
                    if let Some(slot @ 0..=1) = hit {
                        self.compression = 0.1;
                        self.slot = slot;
                        self.scale[2] = 20.0;
                        self.set_phase(270.0);
                        self.mode = SpringMode::Touch;
                        self.contact_timer = 0;
                    }
                } else if hit.is_some() {
                    self.contact_timer = 0;
                }
                if self.mode != SpringMode::Touch {
                    calls.push(RegisterCollision);
                }
                self.contact_timer = self.contact_timer.min(50);
            }
            SpringMode::Touch => {
                if self.kind == SpringKind::A {
                    calls.push(Sound(0x1000));
                }
                self.contact_timer = 0;
                if let Some(player) = player {
                    self.launch(player, out_point, &mut calls);
                }
                if self.kind == SpringKind::B {
                    calls.push(Sound(0x1000));
                }
            }
            SpringMode::Done => {
                calls.push(DeadOut);
                self.mode = SpringMode::Removed;
            }
            SpringMode::Removed => {}
        }
        calls
    }

    fn set_phase(&mut self, phase: f32) {
        self.phase_degrees = phase;
        if self.kind == SpringKind::A {
            self.scale[0] = phase;
        }
    }

    fn launch(&mut self, player: SpringPlayer, out: [f32; 3], calls: &mut Vec<SpringCall>) {
        use SpringCall::*;
        let slot = self.slot;
        let target = std::array::from_fn(|i| self.position[i] + out[i]);
        let dx = target[0] - player.position[0];
        let dz = target[2] - player.position[2];
        let d = (dx * dx + dz * dz).sqrt();
        let slow = player.running_speed < 1.0 || self.compression != 0.1;
        if player.item & 0x4000 == 0 {
            let placement = if !slow {
                Some(target)
            } else if (2.0..14.0).contains(&d) {
                Some([
                    player.position[0] + 2.0 * (dx / d),
                    player.position[1],
                    player.position[2] + 2.0 * (dz / d),
                ])
            } else {
                None
            };
            if let Some(pos) = placement {
                calls.extend([Input(slot, 9), SetPosition(slot, pos), Input(slot, 15)]);
            }
        }
        match self.kind {
            SpringKind::A => calls.push(SpringVelocity(slot, out)),
            SpringKind::B => {
                if slow {
                    calls.push(SetPosition(
                        slot,
                        [target[0], self.position[1] + out[1] * 0.8, target[2]],
                    ));
                }
                if self.scale[0] <= 0.0 {
                    self.scale[0] = 0.0;
                }
                calls.push(VelocityYRotationNoControl {
                    slot,
                    velocity_y: self.scale[1] + 5.0,
                    angles: self.angles,
                    frames: self.scale[0] as i32,
                });
            }
        }
        self.mode = SpringMode::Normal;
        self.compression = 0.1;
        calls.extend([Function8002fb2c(slot, 4, 15, 0), SetRelLocal(slot, 1)]);
    }
}
