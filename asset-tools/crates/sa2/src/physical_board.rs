//! City Escape's physical board object, separate from the input trigger in `board`.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardRider {
    pub position: [f32; 3],
    pub angles: [i32; 3],
    pub speed: [f32; 3],
    pub action: i16,
    pub motion_mode: i16,
    pub frame: f32,
}

#[derive(Clone, Copy, Serialize)]
pub struct PhysicalBoard {
    pub slot: u8,
    pub mode: u8,
    pub position: [f32; 3],
    pub angles: [i32; 3],
    pub speed: [f32; 3],
    pub cached_action: i32,
    pub cached_frame: f32,
}

impl PhysicalBoard {
    /// The constructor installs callbacks only if native work allocation succeeds.
    /// For a fresh SET task, motion work and mode start at zero.
    pub fn bind(rotation: [u16; 3], position: [f32; 3], allocation_ok: bool) -> Option<Self> {
        allocation_ok.then(|| Self {
            slot: (rotation[0] & 1) as u8,
            mode: 0,
            position,
            angles: rotation.map(|v| i32::from(v as i16)),
            speed: [0.0; 3],
            cached_action: 0,
            cached_frame: 0.0,
        })
    }

    /// Executes up to the detached engine call. True requests the native detached
    /// motion routine followed by its range check; the resulting state is external.
    pub fn tick(&mut self, player: Option<BoardRider>) -> bool {
        let Some(rider) = player else { return false };
        if !(0x79..=0x90).contains(&rider.action) {
            if self.mode == 1 {
                self.mode = 2;
            }
            return self.mode == 2;
        }
        self.mode = 1;
        self.position = rider.position;
        self.angles = [
            rider.angles[0],
            0x8000_i32.wrapping_sub(rider.angles[1]),
            rider.angles[2],
        ];
        self.speed = rider.speed;
        if rider.motion_mode > 2 {
            self.cached_action = rider.action.into();
            self.cached_frame = rider.frame;
        }
        false
    }
}
