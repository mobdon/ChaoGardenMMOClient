//! SOLIDBOX, Prison Lane's stackable solid box (stg09D.rel 0x3AD5C set-up,
//! 0x3AE5C exec, 0x3AE24 delete), and the stack registry it shares with ORI
//! and the containers.
//!
//! Registry (0x8F04 add, 0x9154 remove): a 256-slot table at sec6+0x224.
//! Adding a SET-placed object lowers it by 20 times its SET entry's byte 1
//! (how many times it had dropped before it was unloaded). 0x92C4 is the
//! collapse the containers (CONTIRON 0x193EC, CONTCHAO 0x19E30, CONTWOOD
//! 0x1A9F8) and ORI (0x31588) call at their own position when they go: every
//! registered object higher than that point plus 11 and within 8
//! horizontally (squared distance below 64) gets 20 more to fall
//! (twp +0x24) and its SET byte 1 grows by one.
//!
//! SOLIDBOX: a 20-unit cube (CollisionData sec5+0x8E484, form 3, centre
//! (0, 10, 0), half-extents 10). Set-up zeroes Rotation.x and .z, the fall
//! (+0x24) and its speed (+0x28), and marks the ground height unknown
//! (task +0x40 = -1000000). Each frame:
//! * While there is fall left, the box moves by its speed, or by the rest of
//!   the fall if that is less (`step = speed`, or `-fall` when `fall <
//!   -speed`), then `speed = 0.99 * speed - 0.08`. A child task (0x96B0)
//!   marks where it will land (`landing`).
//! * When the fall is used up while the box still has speed, both reset to
//!   0, and if the box is within 5 of its ground height (`fabs`, 0x1478) it
//!   raises dust (0x3D230 with 10, 10, 8, 1.2, 0.5; `SolidBoxEvent::Landed`).
//! * While the ground height is unknown, every 32nd frame (timer & 0x1F ==
//!   0) it probes 5 above its position with fn_80067AF0
//!   (`land_probe::ground_height`), which the host supplies.
use crate::play_world::V3;
use serde::Serialize;

pub const STACK_SLOTS: usize = 0x100;
/// sec4+0x428: the height one stacked object drops.
pub const STACK_STEP: f32 = 20.0;
/// sec4+0x42C / +0x430: the collapse's height margin and squared reach.
const COLLAPSE_ABOVE: f32 = 11.0;
const COLLAPSE_REACH_SQ: f32 = 64.0;
pub const NO_GROUND: f32 = -1_000_000.0;
const KEEP: f32 = f32::from_bits(0x3f7d_70a4); // sec4+0x11C8, 0.99
const GRAVITY: f32 = f32::from_bits(0x3da3_d70a); // sec4+0x11CC, 0.08
const DUST_RANGE: f32 = 5.0; // sec4+0x11D0
const PROBE_LIFT: f32 = 5.0; // sec4+0x11D0

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SolidBoxEvent {
    /// Landed within 5 of the ground: dust (0x3D230) at the box.
    Landed { position: V3 },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SolidBox {
    pub layer: usize,
    pub index: usize,
    pub position: V3,
    pub yaw: u16,
    /// twp +0x24: how far it still has to fall.
    pub fall: f32,
    /// twp +0x28.
    pub speed: f32,
    /// task +0x40: the probed ground height.
    pub ground: f32,
    /// twp +6.
    pub timer: u16,
    /// The 0x96B0 child's position while falling.
    pub landing: Option<V3>,
    /// The SET entry's byte 1 (drops so far).
    pub drops: u8,
}

/// One frame of a stacked object's fall (SOLIDBOX 0x3AEA0, ORI 0x31230):
/// move by the speed, or by the rest of the fall when that is less, then
/// `speed = 0.99 * speed - 0.08`.
pub fn fall_step(y: &mut f32, fall: &mut f32, speed: &mut f32) {
    let step = if *fall < -*speed { -*fall } else { *speed };
    *y += step;
    *fall += step;
    *speed = KEEP * *speed - GRAVITY;
}

impl SolidBox {
    pub fn new(layer: usize, index: usize, position: V3, yaw: u16) -> Self {
        Self {
            layer,
            index,
            position,
            yaw,
            fall: 0.0,
            speed: 0.0,
            ground: NO_GROUND,
            timer: 0,
            landing: None,
            drops: 0,
        }
    }

    /// One frame. `ground` is fn_80067AF0 at a point (the host's land
    /// queries), `NO_GROUND` when nothing is below.
    pub fn step(&mut self, ground: &mut dyn FnMut(V3) -> f32) -> Option<SolidBoxEvent> {
        let mut out = None;
        if self.fall > 0.0 {
            fall_step(&mut self.position[1], &mut self.fall, &mut self.speed);
            let [x, y, z] = self.position;
            self.landing = Some([x, y - self.fall, z]);
        } else {
            self.landing = None;
        }
        if self.fall <= 0.0 && self.speed != 0.0 {
            self.fall = 0.0;
            self.speed = 0.0;
            if (self.position[1] - self.ground).abs() < DUST_RANGE {
                out = Some(SolidBoxEvent::Landed {
                    position: self.position,
                });
            }
        }
        if self.ground == NO_GROUND && self.timer & 0x1f == 0 {
            let [x, y, z] = self.position;
            self.ground = ground([x, PROBE_LIFT + y, z]);
        }
        self.timer = self.timer.wrapping_add(1);
        out
    }
}

/// 0x92C4 over a set of stacked objects: `(position, fall, drops)` of each
/// registered one. Returns how many were told to fall.
pub fn collapse_above(stack: &mut [(&mut V3, &mut f32, &mut u8)], at: V3) -> usize {
    let top = COLLAPSE_ABOVE + at[1];
    let mut n = 0;
    for (p, fall, drops) in stack.iter_mut() {
        if p[1] <= top {
            continue;
        }
        let dx = p[0] - at[0];
        let dz = p[2] - at[2];
        if dx * dx + dz * dz >= COLLAPSE_REACH_SQ {
            continue;
        }
        **fall += STACK_STEP;
        **drops = drops.wrapping_add(1);
        n += 1;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_box_falls_twenty_and_lands() {
        let mut b = SolidBox::new(0, 0, [0.0, 40.0, 0.0], 0);
        let mut g = |_p: V3| 20.0;
        assert!(b.step(&mut g).is_none());
        assert_eq!(b.ground, 20.0);
        let mut below = [0.0f32, 20.0, 0.0];
        let (mut fall, mut drops) = (b.fall, b.drops);
        {
            let mut p = b.position;
            let mut stack = [(&mut p, &mut fall, &mut drops)];
            assert_eq!(collapse_above(&mut stack, below), 1);
            // The box below is gone; a second collapse at a far point misses.
            below[0] = 100.0;
            assert_eq!(collapse_above(&mut stack, below), 0);
        }
        b.fall = fall;
        b.drops = drops;
        assert_eq!((b.fall, b.drops), (20.0, 1));
        let mut frames = 0;
        let landed = loop {
            frames += 1;
            if let Some(e) = b.step(&mut g) {
                break e;
            }
            assert!(frames < 100);
        };
        // y and the fall are stepped separately in single precision, so the
        // box ends a few ulps off 20, as in the game.
        assert!((b.position[1] - 20.0).abs() < 1e-4);
        assert_eq!(
            landed,
            SolidBoxEvent::Landed {
                position: b.position
            }
        );
        // The landing marker goes on the next frame.
        assert!(b.landing.is_some());
        assert!(b.step(&mut g).is_none());
        assert!(b.landing.is_none());
    }
}
