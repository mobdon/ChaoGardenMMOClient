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

/// How a cell door kind moves its collision (stg09D.rel execs).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoorKind {
    pub name: &'static str,
    /// Section-5 CollisionData rows and their count.
    pub rows: u32,
    pub row_count: usize,
    /// Slide distance at the open stop.
    pub stop: f32,
    /// The row whose centre the exec rewrites each frame.
    pub moving_row: usize,
    /// How the slide is written: Z by Rotation.z bit 0 (CAGED), Y up
    /// (CAGE) or Y down (CAGEB, CAGEC).
    pub motion: DoorMotion,
    /// The exec skips its collision entirely once fully open (CAGEB, CAGEC).
    pub no_collision_when_open: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DoorMotion {
    SignedZ,
    Up,
    Down,
}

/// stg09D.rel cell doors. All share the exec shape of CAGED (0x2000):
/// closed until the group is clear, slide by Scale.x per frame to `stop`,
/// settle with kick 0.5, pull 0.4 and keep 0.96, snap shut if the group
/// fills again.
pub const DOOR_KINDS: [DoorKind; 4] = [
    // 0x1EB0 / 0x2000: sec5+0x38F68, stop 50 (sec4+0xE0), centre Z = +/-slide.
    DoorKind {
        name: "CAGED",
        rows: 0x38f68,
        row_count: 1,
        stop: 50.0,
        moving_row: 0,
        motion: DoorMotion::SignedZ,
        no_collision_when_open: false,
    },
    // 0xF64 / 0x10B4: sec5+0x36258, stop 48 (sec4+0x6C), centre Y = 26 + slide.
    DoorKind {
        name: "CAGE",
        rows: 0x36258,
        row_count: 1,
        stop: 48.0,
        moving_row: 0,
        motion: DoorMotion::Up,
        no_collision_when_open: false,
    },
    // 0x1480: sec5+0x36E98 (4 rows), stop 39 (sec4+0x94), row 1 centre Y
    // minus slide (0x18A4), no collision in state 2 (0x1894).
    DoorKind {
        name: "CAGEB",
        rows: 0x36e98,
        row_count: 4,
        stop: 39.0,
        moving_row: 1,
        motion: DoorMotion::Down,
        no_collision_when_open: true,
    },
    // 0x1994: sec5+0x380B8 (4 rows), stop 39 (sec4+0xB4), as CAGEB (0x1DC0).
    DoorKind {
        name: "CAGEC",
        rows: 0x380b8,
        row_count: 4,
        stop: 39.0,
        moving_row: 1,
        motion: DoorMotion::Down,
        no_collision_when_open: true,
    },
];

/// Cell door state (CAGED, CAGE, CAGEB, CAGEC).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrisonDoor {
    pub kind: DoorKind,
    pub layer: usize,
    pub index: usize,
    pub position: V3,
    pub rotation: [u16; 3],
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

/// stg09D.rel section 4 constants used by every door exec (CAGED's at
/// sec4+0xF8..0x100; the others repeat them).
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
                if self.slide >= self.kind.stop {
                    sound = Some(DoorSound::Opened);
                    self.state = 2;
                    self.velocity += SETTLE_KICK;
                } else {
                    sound = Some(DoorSound::Sliding);
                }
            }
            2 => {
                self.slide += self.velocity;
                let pull = SETTLE_PULL * -(self.slide - self.kind.stop);
                self.velocity = pull + SETTLE_KEEP * self.velocity;
                if self.slide >= self.kind.stop {
                    self.slide = self.kind.stop;
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

    /// Local Z of a CAGED box centre (its CollisionData centre Z, 0, plus
    /// the Rotation.z bit, decides the sign).
    pub fn collision_offset_z(&self) -> f32 {
        if self.positive {
            self.slide
        } else {
            -self.slide
        }
    }

    /// The door's CollisionData rows as the exec leaves them this frame;
    /// empty when the exec skips its collision.
    pub fn live_rows(
        &self,
        rows: &[crate::play_objects::CollisionRow],
    ) -> Vec<crate::play_objects::CollisionRow> {
        if self.kind.no_collision_when_open && self.state == 2 {
            return vec![];
        }
        let mut rows = rows.to_vec();
        if let Some(r) = rows.get_mut(self.kind.moving_row) {
            match self.kind.motion {
                DoorMotion::SignedZ => r.center[2] = self.collision_offset_z(),
                DoorMotion::Up => r.center[1] += self.slide,
                DoorMotion::Down => r.center[1] -= self.slide,
            }
        }
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn door(speed: f32) -> PrisonDoor {
        PrisonDoor {
            kind: DOOR_KINDS[0],
            layer: 0,
            index: 0,
            position: [0.0; 3],
            rotation: [10, 0, 0],
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
        assert!(d.slide <= 50.0 && 50.0 - d.slide < 1.0, "{}", d.slide);
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
