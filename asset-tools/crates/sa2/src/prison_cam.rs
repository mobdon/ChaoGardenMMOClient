//! Prison Lane's security camera, CAM (stg09D.rel 0x2450 set-up, 0x25DC
//! exec, 0x2818 draw, 0x2578 delete) and its lens child task (0x28C8
//! set-up, 0x2948 exec, 0x2928 delete).
//!
//! Set-up: the head angle (work +6) starts at Rotation.y; the body
//! registers CollisionData sec5+0x3A7C4 (form 0, a radius-8.5 sphere). The
//! lens child is placed at the model's child node (the NJS_OBJECT at
//! sec5+0x39CD4, child +0x2C, position (0, 0.8, 12.6)) through njTranslate
//! and njRotateY(Rotation.y), registers sec5+0x3A7F4 (form 0, radius 4,
//! attribute 0x800000) and takes an enemy work block (0x80018D28: group
//! 0xFF, so it never counts toward a door's group).
//!
//! Exec, while not destroyed: the range test 0x800398D8 against 300 picks a
//! player; the head turns toward that player's position by
//! `trunc(10430.38043493439 * atan2f(dx, dz))` (MSL atan2f 0x80111168, the
//! product in double precision), limited to 128 BAMS per frame (the word at
//! sec5+0x3A7C0). When NextAction (twp +1) is set, the camera is destroyed:
//! the explosion point is the child node through njTranslate and
//! njRotateY(Rotation.y), 10 lower, turned to the head angle, with velocity (0, 0.2, 0) and sound 0x14; the ground
//! probe 0x80067AF0 and the debris/explosion spawns (local 0xC23C, 0xB6F8)
//! are not run. The lens exec sets the parent's NextAction bit when its
//! collision status has bit 2 (a hit) set, then deletes itself; that hit
//! comes from the player's attack collision, which the host reports with
//! `SecurityCamera::lens_hit`.
//!
//! The object range check at the top of the exec (0x800655F4) is not run:
//! the session does not stream SET objects in and out.
use crate::msl_math::atan2f;
use crate::ninja_trig::nj_rotate_sin_cos;
use crate::play_world::V3;
use crate::player_range::player_in_range;
use serde::Serialize;

/// Range test radius (sec4+0x110).
pub const CAM_SIGHT: f32 = 300.0;
/// Head turn limit per frame (sec5+0x3A7C0).
pub const CAM_TURN_LIMIT: i32 = 128;
/// Body sphere (sec5+0x3A7C4) and lens sphere (sec5+0x3A7F4) radii.
pub const CAM_RADIUS: f32 = 8.5;
pub const LENS_RADIUS: f32 = 4.0;
/// The model's child node position (sec5+0x39CD4 +0x2C, then +8).
pub const LENS_OFFSET: V3 = [0.0, 0.8, 12.6];
/// Explosion drop (sec4+0x120) and rise speed (sec4+0x128).
const EXPLOSION_DROP: f32 = 10.0;
const EXPLOSION_RISE: f32 = f32::from_bits(0x3e4c_cccd);
/// BAMS per radian (sec4+0x118, a double).
const BAMS_PER_RADIAN: f64 = 10430.38043493439;
/// Sound 0x8006378C plays when the camera is destroyed.
pub const CAM_DESTROY_SOUND: u16 = 0x14;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CamExplosion {
    pub position: V3,
    /// Rotation (0, head, 0).
    pub head: u16,
    pub velocity: V3,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityCamera {
    pub layer: usize,
    pub index: usize,
    pub position: V3,
    /// Rotation.y as the task holds it (the SET loader sign-extends it).
    pub yaw: i32,
    /// Work +6: the head's absolute yaw.
    pub head: u16,
    /// Lens task position (fixed at set-up).
    pub lens: V3,
    pub lens_alive: bool,
    /// twp +1.
    pub next_action: u8,
    pub destroyed: bool,
}

/// njTranslate(position) then njRotateY(angle), applied to `p`.
fn place(p: V3, angle: i32, at: V3) -> V3 {
    let (s, c) = nj_rotate_sin_cos(angle);
    [
        c * p[0] + s * p[2] + at[0],
        p[1] + at[1],
        -s * p[0] + c * p[2] + at[2],
    ]
}

impl SecurityCamera {
    pub fn new(layer: usize, index: usize, position: V3, rotation_y: u16) -> Self {
        let yaw = i32::from(rotation_y as i16);
        Self {
            layer,
            index,
            position,
            yaw,
            head: rotation_y,
            lens: place(LENS_OFFSET, yaw, position),
            lens_alive: true,
            next_action: 0,
            destroyed: false,
        }
    }

    /// One frame. `centres` are the players' collision centres the range
    /// test reads; `positions` the player positions the head aims at.
    /// `lens_hit` is the lens collision's hit status this frame.
    pub fn step(
        &mut self,
        centres: &[Option<V3>; 2],
        positions: &[Option<V3>; 2],
        lens_hit: bool,
    ) -> Option<CamExplosion> {
        let mut out = None;
        if !self.destroyed {
            let slot = player_in_range(centres, self.position, CAM_SIGHT);
            if let Some(p) = slot.checked_sub(1).and_then(|s| positions[s as usize]) {
                let dx = p[0] - self.position[0];
                let dz = p[2] - self.position[2];
                let angle = (BAMS_PER_RADIAN * f64::from(atan2f(dx, dz))) as i32;
                let delta = (angle.wrapping_sub(i32::from(self.head)) as i16) as i32;
                let delta = if delta > 0 {
                    delta.min(CAM_TURN_LIMIT)
                } else {
                    delta.max(-CAM_TURN_LIMIT)
                };
                self.head = self.head.wrapping_add(delta as u16);
            }
            if self.next_action != 0 {
                // 0x2708: njRotateY(Rotation.y), not the head angle.
                let mut at = place(LENS_OFFSET, self.yaw, self.position);
                at[1] -= EXPLOSION_DROP;
                self.destroyed = true;
                out = Some(CamExplosion {
                    position: at,
                    head: self.head,
                    velocity: [0.0, EXPLOSION_RISE, 0.0],
                });
            }
        }
        // The lens task runs after its parent.
        if self.lens_alive && lens_hit {
            // Child Index 0: bit 1 << 0.
            self.next_action |= 1;
            self.lens_alive = false;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_turns_at_most_128_per_frame() {
        let mut c = SecurityCamera::new(0, 0, [0.0; 3], 0);
        let p = [Some([100.0, 0.0, 0.0]), None];
        c.step(&p, &p, false);
        assert_eq!(c.head, 128);
        // Out of sight: no turn.
        let far = [Some([1000.0, 0.0, 0.0]), None];
        c.step(&far, &far, false);
        assert_eq!(c.head, 128);
        // Within one step of the target it lands exactly.
        let mut c = SecurityCamera::new(0, 0, [0.0; 3], 16300);
        c.step(&p, &p, false);
        assert_eq!(
            c.head as i32,
            (BAMS_PER_RADIAN * f64::from(atan2f(100.0, 0.0))) as i32
        );
    }

    #[test]
    fn lens_hit_destroys_on_the_next_frame() {
        let mut c = SecurityCamera::new(0, 0, [0.0; 3], 0);
        assert_eq!(c.lens, [0.0, 0.8, 12.6]);
        let none = [None, None];
        assert!(c.step(&none, &none, true).is_none());
        let e = c.step(&none, &none, false).unwrap();
        assert!(c.destroyed && !c.lens_alive);
        assert_eq!(e.position[1], 0.8 - 10.0);
        assert!(c.step(&none, &none, false).is_none());
    }
}
