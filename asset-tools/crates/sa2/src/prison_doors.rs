//! Enemy groups and Prison Lane's sliding prison doors (CAGED), from the
//! GameCube main program and stg09D.rel.
//!
//! The "switch" a door asks about (PC import 0x80018E30, identical in
//! GSNE8P main.dol) is a live-enemy counter: a byte per group at 0x801E4AB8
//! that is 1 when the group's count is zero. Enemy set-up 0x80018C88 /
//! 0x80018C10 stores the low byte of the task's Rotation.y as the group,
//! keeps only its high byte as the yaw, and increments the count; the
//! destroy paths (0x80018B88, 0x80016598) decrement it.
//!
//! Which Prison Lane enemies count (stg09D.rel): E KUMI (0x4C848) calls the
//! counting set-up unless its kind, Scale.x truncated (0x80111FD8 is
//! truncation, 0x80111240 floorf on |x|) and forced to 0 outside 0..=4, is
//! 4; E EMI (0x4E900) always counts (kinds 9 and others take the two
//! counting entries); E AI (0x513D4), E NAMIE (0x54080) and E GOLD
//! (0x4A3D4) always count. The `_F` and `_N` variants share these callbacks.
use crate::play_world::V3;
use serde::Serialize;

/// One SET enemy that takes part in a group count.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Enemy {
    pub layer: usize,
    pub index: usize,
    pub name: String,
    pub position: V3,
    pub group: u8,
    pub counted: bool,
    pub alive: bool,
}

/// Whether a SET enemy of this object-list name increments its group count.
/// None for enemies whose rule is not read.
pub fn enemy_counts(name: &str, scale: V3) -> Option<bool> {
    let base = name
        .strip_suffix("_F")
        .or_else(|| name.strip_suffix("_N"))
        .unwrap_or(name);
    match base {
        "E KUMI" => {
            let kind = scale[0] as i32;
            let kind = if (0..5).contains(&kind) { kind } else { 0 };
            Some(kind != 4)
        }
        "E EMI" | "E AI" | "E NAMIE" | "E GOLD" => Some(true),
        _ => None,
    }
}

/// CAGED door state (stg09D.rel 0x1EB0 set-up, 0x2000 exec).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrisonDoor {
    pub layer: usize,
    pub index: usize,
    pub position: V3,
    pub yaw: u16,
    /// Rotation.x low byte: the enemy group that must be cleared.
    pub group: u8,
    /// Scale.x: opening speed per frame.
    pub speed: f32,
    /// Rotation.z bit 0: the collision box slides toward +local Z.
    pub positive: bool,
    /// 0 closed, 1 opening, 2 open (settling).
    pub state: u8,
    /// Task work +0: slide distance.
    pub slide: f32,
    /// Task work +4: settle velocity.
    pub velocity: f32,
}

/// stg09D.rel section 4 constants used by the exec.
pub const OPEN: f32 = 50.0; // sec4+0xE0
pub const SETTLE_KICK: f32 = 0.5; // sec4+0xF8
pub const SETTLE_PULL: f32 = 0.4; // sec4+0xFC
pub const SETTLE_KEEP: f32 = 0.96; // sec4+0x100

/// Door events that carry the exec's sound calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoorSound {
    /// 0x1020 while sliding open.
    Sliding,
    /// 0x1021 when fully open.
    Opened,
}

impl PrisonDoor {
    /// One exec frame. `clear` is the switch query for this door's group.
    pub fn step(&mut self, clear: bool) -> Option<DoorSound> {
        let mut sound = None;
        match self.state {
            0 => {
                if clear {
                    self.state = 1;
                }
            }
            1 => {
                self.slide += self.speed;
                if self.slide >= OPEN {
                    sound = Some(DoorSound::Opened);
                    self.state = 2;
                    self.velocity += SETTLE_KICK;
                } else {
                    sound = Some(DoorSound::Sliding);
                }
            }
            2 => {
                self.slide += self.velocity;
                let pull = SETTLE_PULL * -(self.slide - OPEN);
                self.velocity = pull + SETTLE_KEEP * self.velocity;
                if self.slide >= OPEN {
                    self.slide = OPEN;
                    self.velocity = -self.velocity.abs();
                }
            }
            _ => {}
        }
        // A group that fills again snaps the door shut.
        if self.state != 0 && !clear {
            self.state = 0;
            self.velocity = 0.0;
            self.slide = 0.0;
        }
        sound
    }

    /// Local Z of the door's collision box centre (its CollisionData centre
    /// Z, 0, plus the Rotation.z bit, decides the sign).
    pub fn collision_offset_z(&self) -> f32 {
        if self.positive {
            self.slide
        } else {
            -self.slide
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn door(speed: f32) -> PrisonDoor {
        PrisonDoor {
            layer: 0,
            index: 0,
            position: [0.0; 3],
            yaw: 0,
            group: 10,
            speed,
            positive: false,
            state: 0,
            slide: 0.0,
            velocity: 0.0,
        }
    }

    #[test]
    fn door_opens_after_its_group_clears_and_settles_at_fifty() {
        let mut d = door(0.5);
        assert_eq!(d.step(false), None);
        assert_eq!(d.state, 0);
        assert_eq!(d.step(true), None);
        assert_eq!(d.state, 1);
        let mut frames = 0;
        while d.state == 1 {
            d.step(true);
            frames += 1;
        }
        assert_eq!(frames, 100);
        for _ in 0..600 {
            d.step(true);
        }
        assert!(d.slide <= OPEN && OPEN - d.slide < 1.0, "{}", d.slide);
        assert_eq!(d.collision_offset_z(), -d.slide);
        // An enemy joining the group shuts it at once.
        d.step(false);
        assert_eq!((d.state, d.slide, d.velocity), (0, 0.0, 0.0));
    }

    #[test]
    fn kumi_kind_four_does_not_count() {
        assert_eq!(enemy_counts("E KUMI", [4.0, 0.0, 0.0]), Some(false));
        assert_eq!(enemy_counts("E KUMI_F", [4.9, 0.0, 0.0]), Some(false));
        assert_eq!(enemy_counts("E KUMI", [7.0, 0.0, 0.0]), Some(true));
        assert_eq!(enemy_counts("E AI_N", [0.0; 3]), Some(true));
        assert_eq!(enemy_counts("E KYOKO", [0.0; 3]), None);
    }
}
