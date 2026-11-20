//! ORI, the breakable cage block the rocket hits (stg09D.rel 0x31058
//! set-up, 0x311D4 exec, 0x31180 delete).
//!
//! Set-up: a 4-byte work block whose word is the rocket link; Rotation.x is
//! cut to its low 4 bits and Rotation.z to 5 (the rocket id). It registers
//! the 20-unit cube CollisionData sec5+0x7FDF0 (form 3, centre (0, 10, 0),
//! half-extents 10), joins the rocket table (0x35A1C: 16 slots at
//! sec5+0x88204, id = Rotation.z) and the stack (0x8F04, see `solid_box`).
//!
//! Each frame it falls like SOLIDBOX (same constants at sec4+0xF00..). Then:
//! * State 0: unless the rocket has linked itself or the attack query
//!   0x800060EC(task, 0x13) reports a hit, it counts its timer up and, while
//!   its ground is unknown, probes it on every 32nd count (after the
//!   increment). On a hit it waits `trunc(max(distance(rocket, self) - 18,
//!   0))` frames (0x8011B65C) when the rocket did it, else 1; leaves the
//!   rocket table (0x35A1C's partner 0x35A70); plays sound 0x1011 (variant =
//!   the stage's break counter mod 5) and 0x14; and goes to state 1.
//! * State 1: the wait counts down first. Then on its second frame it runs
//!   the stack collapse 0x92C4 at its position; from its fourth it throws
//!   debris and sparks and goes to state 2.
//! * State 2: it deletes itself (and marks its SET entry).
//!
//! The debris (local 0xC214, the path taken while the flag at sec6+0xD30,
//! never written in this module, is 0): unless `(Rotation.x & 0xF) % 3` is 2,
//! three groups from the table at sec5+0x7FD8C (16, 4 and 4 pieces) at the
//! position plus (0, 10, 0), each piece scaled `0.7 + 1.2 * u`, with a
//! floor of -1000000 when that kind is 1, else its ground height (or its own
//! height while that is unknown). Then 14 sparks (local 0x3D56C) with `(7 +
//! 3 * u, 0.5 + u, 2.5 + 4 * u)`, drawn in the order of the last, middle
//! and first. `u = rand() * 2^-15` (MSL rand).
use crate::play_world::V3;
use crate::player_range::distance;
use crate::prison_visuals::MslRand;
use crate::solid_box::{fall_step, NO_GROUND};
use serde::Serialize;

/// sec5+0x7FD8C: pieces per debris group.
pub const DEBRIS_GROUPS: [u32; 3] = [16, 4, 4];
pub const SPARKS: u32 = 14;
pub const BREAK_SOUND: u16 = 0x1011;
pub const BREAK_VOICE: u16 = 0x14;
const ROCKET_REACH: f32 = 18.0; // sec4+0xF1C
const DUST_RANGE: f32 = 5.0; // sec4+0xF08
const UNIT: f32 = f32::from_bits(0x3800_0000);
const PIECE_BASE: f32 = f32::from_bits(0x3f33_3333); // sec4+0xF20, 0.7
const PIECE_SPAN: f32 = f32::from_bits(0x3f99_999a); // sec4+0xF14, 1.2
const LIFT: f32 = 10.0; // sec4+0xF0C

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OriEvent {
    Landed {
        position: V3,
    },
    /// Sound 0x1011 (variant `variant`) and 0x14.
    Hit {
        variant: u32,
    },
    /// 0x92C4 at the cage's position (the caller runs the collapse).
    Collapse {
        at: V3,
    },
    Piece {
        group: usize,
        at: V3,
        yaw: i32,
        scale: f32,
        floor: f32,
    },
    Spark {
        at: V3,
        a: f32,
        b: f32,
        c: f32,
    },
    Gone,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ori {
    pub layer: usize,
    pub index: usize,
    pub position: V3,
    pub rotation: [i32; 3],
    pub state: u8,
    pub timer: u16,
    pub fall: f32,
    pub speed: f32,
    pub ground: f32,
    /// task +0x4C: frames to wait before breaking.
    pub wait: i32,
    pub landing: Option<V3>,
    pub drops: u8,
    /// In the rocket table.
    pub listed: bool,
}

fn unit(rand: &mut MslRand) -> f32 {
    UNIT * (rand.rand() as i32 as f32)
}

impl Ori {
    pub fn new(layer: usize, index: usize, position: V3, rotation: [u16; 3]) -> Self {
        let r = rotation.map(|r| i32::from(r as i16));
        Self {
            layer,
            index,
            position,
            rotation: [r[0] & 0xf, r[1], r[2] & 0x1f],
            state: 0,
            timer: 0,
            fall: 0.0,
            speed: 0.0,
            ground: NO_GROUND,
            wait: 0,
            landing: None,
            drops: 0,
            listed: true,
        }
    }

    /// The rocket id (Rotation.z & 0x1F).
    pub fn rocket_id(&self) -> u32 {
        self.rotation[2] as u32
    }

    /// One frame. `rocket` is the linking rocket's position when one has hit
    /// it; `attacked` the 0x800060EC(task, 0x13) answer; `breaks` the
    /// stage's break counter (sec6+0xD34).
    pub fn step(
        &mut self,
        rocket: Option<V3>,
        attacked: bool,
        breaks: &mut u32,
        rand: &mut MslRand,
        ground: &mut dyn FnMut(V3) -> f32,
    ) -> Vec<OriEvent> {
        let mut out = vec![];
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
                out.push(OriEvent::Landed {
                    position: self.position,
                });
            }
        }
        match self.state {
            0 => {
                let linked = rocket.filter(|_| self.listed);
                if linked.is_none() && !attacked {
                    self.timer = self.timer.wrapping_add(1);
                    if self.ground == NO_GROUND && self.timer & 0x1f == 0 {
                        let [x, y, z] = self.position;
                        self.ground = ground([x, 5.0 + y, z]);
                    }
                    return out;
                }
                self.wait = match linked {
                    Some(r) => {
                        let d = distance(self.position, r) - ROCKET_REACH;
                        let d = if d < 0.0 { 0.0 } else { d };
                        d as i32
                    }
                    None => 1,
                };
                self.listed = false;
                self.timer = 0;
                self.state = 1;
                out.push(OriEvent::Hit {
                    variant: *breaks % 5,
                });
                *breaks = breaks.wrapping_add(1);
            }
            1 => {
                if self.timer == 0 && self.wait != 0 {
                    self.wait -= 1;
                    return out;
                }
                if self.timer == 1 {
                    out.push(OriEvent::Collapse { at: self.position });
                }
                let old = self.timer;
                self.timer = self.timer.wrapping_add(1);
                if old <= 2 {
                    return out;
                }
                let [x, y, z] = self.position;
                let at = [x, y + LIFT, z];
                let kind = self.rotation[0] % 3;
                if kind != 2 {
                    for (group, &n) in DEBRIS_GROUPS.iter().enumerate() {
                        for _ in 0..n {
                            let floor = if kind == 1 {
                                NO_GROUND
                            } else if self.ground == NO_GROUND {
                                self.position[1]
                            } else {
                                self.ground
                            };
                            let scale = PIECE_BASE + PIECE_SPAN * unit(rand);
                            out.push(OriEvent::Piece {
                                group,
                                at,
                                yaw: self.rotation[1],
                                scale,
                                floor,
                            });
                        }
                    }
                }
                for _ in 0..SPARKS {
                    let c = 2.5 + 4.0 * unit(rand);
                    let b = 0.5 + unit(rand);
                    let a = 7.0 + 3.0 * unit(rand);
                    out.push(OriEvent::Spark { at, a, b, c });
                }
                self.state = 2;
            }
            2 => {
                out.push(OriEvent::Gone);
                self.state = 3;
            }
            _ => {}
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rocket_hit_waits_by_distance_then_breaks() {
        let mut o = Ori::new(0, 0, [0.0; 3], [0, 0, 3]);
        let (mut breaks, mut rand) = (0u32, MslRand::default());
        let mut g = |_p: V3| NO_GROUND;
        assert!(o
            .step(None, false, &mut breaks, &mut rand, &mut g)
            .is_empty());
        let e = o.step(
            Some([20.0, 0.0, 0.0]),
            false,
            &mut breaks,
            &mut rand,
            &mut g,
        );
        assert_eq!(e, vec![OriEvent::Hit { variant: 0 }]);
        assert_eq!((o.state, o.wait, breaks), (1, 2, 1));
        let mut frames = 0;
        let mut collapsed = None;
        while o.state == 1 {
            frames += 1;
            for e in o.step(None, false, &mut breaks, &mut rand, &mut g) {
                if let OriEvent::Collapse { .. } = e {
                    collapsed = Some(frames);
                }
            }
        }
        // Two waiting frames, then timer 0, 1 (collapse), 2, 3 (debris).
        assert_eq!((collapsed, frames), (Some(4), 6));
        let e = o.step(None, false, &mut breaks, &mut rand, &mut g);
        assert_eq!(e, vec![OriEvent::Gone]);
    }
}
