//! CHAOPIPE, the pipe a walker drops a chao into (stg09D.rel 0x18A68
//! set-up, 0x18B18 exec, 0x18D5C draw, 0x18B08 delete).
//!
//! Set-up registers CollisionData sec5+0x51574 (form 6, sizes 2.5 and 4;
//! not built here) and zeroes the squash (task +0x38) and its speed (+0x3C).
//!
//! Exec, until the SET entry's flag 0x20 is set (GSNE8P 0x80065404):
//! * The range test 0x800398D8 uses radius 25 when player 0's character
//!   (0x80038108: the byte at +1 of the 0x801E7728 entry) is 6 or 7 (the
//!   walkers), else 13.
//! * With the timer (work +6) at 0: if a player is in range and that
//!   player's task Action (twp +0) is 0x2A, the timer becomes 300, the
//!   squash 0 and its speed 0.08. Otherwise, with a player in range, the
//!   exec writes 0x2A to byte +0xD of that player's 0x801E7728 entry (a
//!   request for that action, reported as `PipeEvent::RequestAction`).
//! * With the timer running: at 270 a chao of kind `(u32)Scale.x % 3`
//!   (`__cvt_fp2unsigned` 0x8010AF3C) is released (0x800260FC) at (0, 3, 2)
//!   through njTranslate(position), njRotateZ, njRotateX, njRotateY and
//!   njCalcPoint; the SET entry is marked (byte 1 = 1, flag 0x20) and sound
//!   0x100A plays. The timer then counts down by one each frame.
//!
//! Every frame the squash springs: `squash += speed; speed = 0.95 * speed -
//! 0.15 * squash`. The draw scales the pipe by (1 - squash, 1 + 0.9 *
//! squash, 1 - squash). The chao itself (0x800260FC) is main-program code
//! and is not run.
use crate::ninja_matrix::{mult_vec, rotate, Axis, IDENTITY};
use crate::play_world::V3;
use crate::player_range::player_in_range;
use serde::Serialize;

/// Player action the pipe waits for and requests (0x2A).
pub const PIPE_ACTION: u8 = 0x2a;
pub const PIPE_TIMER: u16 = 300;
pub const PIPE_RELEASE_AT: u16 = 270;
pub const PIPE_SOUND: u16 = 0x100a;
/// Release point in the pipe's frame (sec5+0x51560).
pub const PIPE_OUTLET: V3 = [0.0, 3.0, 2.0];
const WALKER_RANGE: f32 = 25.0; // sec4+0x93C
const RANGE: f32 = 13.0; // sec4+0x940
const KICK: f32 = f32::from_bits(0x3da3_d70a); // sec4+0x944, 0.08
const KEEP: f32 = f32::from_bits(0x3f73_3333); // sec4+0x948, 0.95
const PULL: f32 = f32::from_bits(0x3e19_999a); // sec4+0x94C, 0.15

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PipeEvent {
    /// Byte +0xD of the player's 0x801E7728 entry set to 0x2A.
    RequestAction { slot: u8 },
    /// A chao released (0x800260FC) with sound 0x100A.
    ChaoReleased { chao_kind: u32, position: V3 },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChaoPipe {
    pub layer: usize,
    pub index: usize,
    pub position: V3,
    pub rotation: [i32; 3],
    pub scale_x: f32,
    pub timer: u16,
    pub squash: f32,
    pub squash_speed: f32,
    /// The SET entry's flag 0x20.
    pub used: bool,
}

/// `__cvt_fp2unsigned` for the values a placement holds: truncation, 0
/// below zero.
fn to_unsigned(x: f32) -> u32 {
    if x.is_nan() || x < 0.0 {
        0
    } else {
        x as u32
    }
}

/// The release point: (0, 3, 2) through njTranslate(position) and
/// njRotateZ, X, Y.
pub fn outlet(position: V3, rotation: [i32; 3]) -> V3 {
    let mut m = IDENTITY;
    for (row, p) in m.iter_mut().zip(position) {
        row[3] = p;
    }
    rotate(&mut m, Axis::Z, rotation[2]);
    rotate(&mut m, Axis::X, rotation[0]);
    rotate(&mut m, Axis::Y, rotation[1]);
    mult_vec(&m, PIPE_OUTLET)
}

impl ChaoPipe {
    pub fn new(layer: usize, index: usize, position: V3, rotation: [u16; 3], scale: V3) -> Self {
        Self {
            layer,
            index,
            position,
            rotation: rotation.map(|r| i32::from(r as i16)),
            scale_x: scale[0],
            timer: 0,
            squash: 0.0,
            squash_speed: 0.0,
            used: false,
        }
    }

    /// One frame. `character` is player 0's character byte; `centres` the
    /// players' collision centres; `actions` their task Action bytes.
    pub fn step(
        &mut self,
        character: i8,
        centres: &[Option<V3>; 2],
        actions: &[Option<u8>; 2],
    ) -> Option<PipeEvent> {
        let mut out = None;
        if !self.used {
            let radius = if character == 7 || character == 6 {
                WALKER_RANGE
            } else {
                RANGE
            };
            let slot = player_in_range(centres, self.position, radius);
            let action = slot.checked_sub(1).and_then(|s| actions[s as usize]);
            if self.timer == 0 && slot != 0 && action == Some(PIPE_ACTION) {
                self.timer = PIPE_TIMER;
                self.squash = 0.0;
                self.squash_speed = KICK;
            } else if self.timer != 0 {
                if self.timer == PIPE_RELEASE_AT {
                    out = Some(PipeEvent::ChaoReleased {
                        chao_kind: to_unsigned(self.scale_x) % 3,
                        position: outlet(self.position, self.rotation),
                    });
                    self.used = true;
                }
                self.timer -= 1;
            } else if slot != 0 {
                out = Some(PipeEvent::RequestAction { slot: slot as u8 });
            }
        }
        self.squash += self.squash_speed;
        self.squash_speed = KEEP * self.squash_speed - PULL * self.squash;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn releases_a_chao_31_frames_after_the_action() {
        let mut p = ChaoPipe::new(0, 0, [0.0; 3], [0; 3], [4.0, 0.0, 0.0]);
        let near = [Some([0.0, 0.0, 20.0]), None];
        // A walker in range without the action: the pipe requests it.
        assert_eq!(
            p.step(6, &near, &[Some(0), None]),
            Some(PipeEvent::RequestAction { slot: 1 })
        );
        // Out of range for other characters (13).
        assert_eq!(p.step(1, &near, &[Some(0), None]), None);
        assert_eq!(p.step(6, &near, &[Some(PIPE_ACTION), None]), None);
        assert_eq!(p.timer, 300);
        let mut frames = 0;
        let released = loop {
            frames += 1;
            if let Some(e) = p.step(6, &near, &[Some(PIPE_ACTION), None]) {
                break e;
            }
        };
        // 300 is set without a count; 300 down to 270 is 31 frames.
        assert_eq!(frames, 31);
        assert_eq!(
            released,
            PipeEvent::ChaoReleased {
                chao_kind: 1,
                position: outlet([0.0; 3], [0; 3])
            }
        );
        assert!(p.used);
        assert_eq!(p.step(6, &near, &[Some(0), None]), None);
    }
}
