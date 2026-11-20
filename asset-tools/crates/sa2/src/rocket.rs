//! ROCKETMISSSILE, the rocket a walker rides into an ORI cage block
//! (stg09D.rel 0x35AC8 set-up, 0x35E10 exec, 0x35D8C delete; child task
//! 0x3756C / 0x3767C).
//!
//! Set-up: action 2; the work keeps the rocket's own pose (+0x4C position,
//! +0x44 yaw, +0x48 pitch, +0x40 roll, +0x3C hatch, +0x38 lift, +0x58
//! thrust, +0x5C "nobody on it", +0x60 the rider's slot) starting at the
//! placement. Rotation.x and .z are cut to 5 bits; Rotation.z gets 0x8000
//! added when Rotation.x has 0x10 (the "flipped" rockets). The model is a
//! dynamic land object; a child task carries its collision.
//!
//! Each frame, the stand point (+-15.1, -+2.6, 0) through njTranslate and
//! njRotateY(Rotation.y) is tested against each player in turn: within 6.5
//! (0x8011B65C) of a player whose task Status has bit 0 or 1 (on the
//! ground), the rocket is ridden (NextAction is set the first frame and
//! +0x5C becomes 0); otherwise +0x5C is 1. While the byte at GSNE8P
//! 0x801CC168 + 0x37 is set and NextAction is 0, nothing else runs.
//!
//! * Action 2: reset the pose and arm (action 3), unless (by Rotation.x & 3)
//!   mode 2's switch (bit 0 of the switch byte Rotation.z & 0x1F, PC table
//!   0x802B95E0) is off, or mode 3 has no player within 100 (0x800398D8) or
//!   an enemy group Rotation.z & 0x1F not clear (0x80018E30); and unless
//!   Rotation.z has 0x80, an ORI with that id must be listed (sec5+0x88204).
//!   Arming plays sound 0x1005.
//! * Action 3: open the hatch (+0.05 a frame to 1), then move 9 down
//!   (flipped) or up at 0.05 a frame, then lift to 7 at 0.3 a frame; once
//!   ridden, action 4.
//! * Action 4: aim at Scale (the target point): pitch atan2(dy, sqrt(dx^2 +
//!   dz^2)) and yaw -atan2(dz, dx) (both negated, yaw + 0x8000, when
//!   flipped), turning the yaw at most 0x140 BAMS a frame and the pitch only
//!   when the yaw does not move. The roll wobbles by 10 degrees of
//!   njSin(timer << 10) and the height by 0.3 * njSin(timer * 0x600). Each
//!   frame it puffs smoke (local 0x12828) and plays 0x1019. Aimed, the hatch
//!   closes (-0.05 a frame), then the thrust rises 0.07 a frame to 1:
//!   action 6 and a rumble for the rider (0x8002F8C4(slot, 5, 0x32, 0)).
//! * Action 6: fly 5.1 a frame at the target (njScalor) with exhaust (local
//!   0x1253C when more than 10.2 away) and 0x1019. Within 5.1 it arrives,
//!   links every listed ORI with its id to itself (the ORI then breaks), and
//!   explodes (0x800E26E0 with a fifth of the last step and 15); if the
//!   child's collision reports a hit on arrival it explodes without
//!   linking. Action 7.
//! * Action 7: by Rotation.x & 3, re-arm (0: action 2; 2: also clear the
//!   switch), stop for good (1: action 5) or stay (3), reset the pose and
//!   unlink.
//!
//! The particle, explosion and rumble tasks are reported, not run. The
//! child's damage byte (after 15 frames of flight) and the tail's
//! player-near flag (Status 0x100) are not modelled.
use crate::land_nodes::nj_scalor;
use crate::msl_math::{atan2f, gekko_frsqrte};
use crate::ninja_matrix::{mult_vec, mult_vec_sr, rotate, Axis, Mtx, IDENTITY};
use crate::ninja_trig::nj_sin;
use crate::play_world::V3;
use crate::player_range::{distance, player_in_range};
use crate::prison_visuals::MslRand;
use serde::Serialize;

const STAND_X: f32 = 15.1;
const STAND_Y: f32 = 2.6;
const STAND_RANGE: f32 = 6.5;
const ARM_RANGE: f32 = 100.0;
const STEP: f32 = 0.05;
const SINK: f32 = 9.0;
const LIFT_STEP: f32 = f32::from_bits(0x3e99_999a); // 0.3
const LIFT_TOP: f32 = 7.0;
const TURN: i32 = 0x140;
const BAMS_PER_DEGREE: f32 = f32::from_bits(0x4336_0b61);
const BAMS_PER_RADIAN: f64 = 10430.38043493439;
const THRUST_STEP: f32 = f32::from_bits(0x3d8f_5c29); // 0.07
const SPEED: f32 = f32::from_bits(0x40a3_3333); // 5.1
const EXHAUST_RANGE: f32 = f32::from_bits(0x4123_3333); // 10.2
const UNIT: f32 = f32::from_bits(0x3800_0000);
/// sec5+0x88284.
const SMOKE_VELOCITY: V3 = [-2.7, 0.0, 0.0];
const NOZZLE: V3 = [-0.2, 0.0, 0.0];

pub const ARM_SOUND: u16 = 0x1005;
pub const ENGINE_SOUND: u16 = 0x1019;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RocketEvent {
    Armed,
    Smoke {
        at: V3,
        velocity: V3,
        size: f32,
        height: f32,
    },
    Exhaust {
        at: V3,
        velocity: V3,
        size: f32,
    },
    /// 0x1019 at the nozzle.
    Engine {
        at: V3,
    },
    Rumble {
        slot: u8,
    },
    /// Linked to the ORIs with this id (they break).
    Linked {
        id: u32,
    },
    Explosion {
        at: V3,
        velocity: V3,
    },
    /// Mode 2 cleared its switch.
    SwitchOff {
        id: u32,
    },
}

/// What the rocket reads from the rest of the game each frame.
pub struct RocketInputs<'a> {
    /// Each player's task position and whether its Status has bit 0 or 1.
    pub players: [Option<(V3, bool)>; 2],
    /// Collision centres for the range test.
    pub centres: [Option<V3>; 2],
    pub suppressed: bool,
    pub switch_on: &'a dyn Fn(u32) -> bool,
    pub group_clear: &'a dyn Fn(u32) -> bool,
    /// An ORI with this id is listed.
    pub ori_listed: &'a dyn Fn(u32) -> bool,
    /// The child's collision reports a hit.
    pub child_hit: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rocket {
    pub layer: usize,
    pub index: usize,
    pub placement: V3,
    pub rotation: [i32; 3],
    /// Scale: the target point.
    pub target: V3,
    pub action: u8,
    pub next_action: u8,
    pub timer: u16,
    pub position: V3,
    pub yaw: i32,
    pub pitch: i32,
    pub roll: i32,
    pub hatch: f32,
    pub lift: f32,
    pub thrust: f32,
    pub unridden: f32,
    pub slot: u8,
}

/// Local 0xBEDC: `x * rsqrt(x)` with three Newton steps in double, the
/// products and difference rounded separately.
pub fn local_sqrtf(x: f32) -> f32 {
    if x <= 0.0 {
        return x;
    }
    let x64 = f64::from(x);
    let mut g = gekko_frsqrte(x64);
    for _ in 0..3 {
        let half = 0.5 * g;
        let t = 3.0 - x64 * (g * g);
        g = half * t;
    }
    (x64 * g) as f32
}

fn translated(p: V3) -> Mtx {
    let mut m = IDENTITY;
    for (row, v) in m.iter_mut().zip(p) {
        row[3] = v;
    }
    m
}

fn unit(rand: &mut MslRand) -> f32 {
    UNIT * (rand.rand() as i32 as f32)
}

fn clamp_turn(d: i32) -> i32 {
    let d = d as i16 as i32;
    if d > 0 {
        d.min(TURN)
    } else {
        d.max(-TURN)
    }
}

impl Rocket {
    pub fn new(layer: usize, index: usize, position: V3, rotation: [u16; 3], scale: V3) -> Self {
        let r = rotation.map(|r| i32::from(r as i16));
        let rx = r[0] & 0x1f;
        let rz = (r[2] & 0x1f) + if rx & 0x10 != 0 { 0x8000 } else { 0 };
        let mut s = Self {
            layer,
            index,
            placement: position,
            rotation: [rx, r[1], rz],
            target: scale,
            action: 2,
            next_action: 0,
            timer: 0,
            position,
            yaw: r[1],
            pitch: 0,
            roll: 0,
            hatch: 0.0,
            lift: 0.0,
            thrust: 0.0,
            unridden: 0.0,
            slot: 0,
        };
        s.reset_pose();
        s
    }

    fn flipped(&self) -> bool {
        self.rotation[0] & 0x10 != 0
    }

    pub fn id(&self) -> u32 {
        (self.rotation[2] & 0x1f) as u32
    }

    fn reset_pose(&mut self) {
        self.position = self.placement;
        self.pitch = 0;
        self.yaw = self.rotation[1];
        self.roll = 0;
        self.lift = 0.0;
        self.hatch = 0.0;
    }

    /// Where a rider stands: (+-15.1, -+2.6, 0) through njTranslate and
    /// njRotateY(Rotation.y).
    pub fn stand_point(&self) -> V3 {
        let stand = if self.flipped() {
            [STAND_X, -STAND_Y, 0.0]
        } else {
            [-STAND_X, STAND_Y, 0.0]
        };
        let mut m = translated(self.placement);
        rotate(&mut m, Axis::Y, self.rotation[1]);
        mult_vec(&m, stand)
    }

    fn nozzle_matrix(&self) -> Mtx {
        let mut m = translated(self.position);
        rotate(&mut m, Axis::Y, self.yaw);
        rotate(&mut m, Axis::Z, self.pitch);
        m
    }

    pub fn step(&mut self, input: &RocketInputs<'_>, rand: &mut MslRand) -> Vec<RocketEvent> {
        let mut out = vec![];
        let stand = self.stand_point();

        let mut ridden = false;
        for (slot, p) in input.players.iter().enumerate() {
            self.slot = slot as u8;
            if let Some((pos, grounded)) = p {
                if distance(*pos, stand) < STAND_RANGE && *grounded {
                    ridden = true;
                    break;
                }
            }
        }
        if ridden {
            if self.next_action == 0 && self.unridden > 0.0 {
                self.next_action = 1;
            }
            self.unridden = 0.0;
        } else {
            self.unridden = 1.0;
        }
        if input.suppressed && self.next_action == 0 {
            return out;
        }
        match self.action {
            2 => {
                self.reset_pose();
                self.action = 3;
                let id = self.id();
                match self.rotation[0] & 3 {
                    2 if !(input.switch_on)(id) => self.action = 2,
                    3 if player_in_range(&input.centres, self.placement, ARM_RANGE) == 0
                        || !(input.group_clear)(id) =>
                    {
                        self.action = 2
                    }
                    _ => {}
                }
                if self.rotation[2] & 0x80 == 0 && !(input.ori_listed)(id) {
                    self.action = 2;
                }
                if self.action == 3 {
                    out.push(RocketEvent::Armed);
                }
            }
            3 => self.arm(),
            4 => self.aim(rand, &mut out),
            6 => self.fly(input, rand, &mut out),
            7 => {
                let id = self.id();
                match self.rotation[0] & 3 {
                    0 => self.action = 2,
                    2 => {
                        self.action = 2;
                        out.push(RocketEvent::SwitchOff { id });
                    }
                    1 => self.action = 5,
                    _ => {}
                }
                self.reset_pose();
                self.thrust = 0.0;
                self.next_action = 0;
            }
            _ => {}
        }
        out
    }

    fn arm(&mut self) {
        self.hatch += STEP;
        if self.hatch < 1.0 {
            return;
        }
        self.hatch = 1.0;
        if self.flipped() {
            self.position[1] -= STEP;
            let goal = self.placement[1] - SINK;
            if self.position[1] > goal {
                return;
            }
            self.position[1] = goal;
        } else {
            self.position[1] += STEP;
            let goal = SINK + self.placement[1];
            if self.position[1] < goal {
                return;
            }
            self.position[1] = goal;
        }
        self.lift += LIFT_STEP;
        if self.lift < LIFT_TOP {
            return;
        }
        self.lift = LIFT_TOP;
        if self.unridden == 0.0 {
            self.thrust = 0.0;
            self.action = 4;
            self.timer = 0;
        }
    }

    fn aim(&mut self, rand: &mut MslRand, out: &mut Vec<RocketEvent>) {
        let d = [
            -self.position[0] + self.target[0],
            -self.position[1] + self.target[1],
            -self.position[2] + self.target[2],
        ];
        let flat = local_sqrtf(d[2] * d[2] + d[0] * d[0]);
        let angle = |y: f32, x: f32| (BAMS_PER_RADIAN * f64::from(atan2f(y, x))) as i32;
        let (pitch, yaw) = if self.flipped() {
            (
                angle(d[1], flat).wrapping_neg(),
                angle(d[2], d[0]).wrapping_neg().wrapping_add(0x8000),
            )
        } else {
            (angle(d[1], flat), angle(d[2], d[0]).wrapping_neg())
        };
        let dyaw = clamp_turn(yaw.wrapping_sub(self.yaw));
        let dpitch = if dyaw == 0 {
            clamp_turn(pitch.wrapping_sub(self.pitch))
        } else {
            0
        };
        self.yaw = self.yaw.wrapping_add(dyaw);
        self.pitch = self.pitch.wrapping_add(dpitch);
        let t = i32::from(self.timer);
        self.roll = (BAMS_PER_DEGREE * (10.0 * nj_sin(t << 10))) as i32;
        let wobble = LIFT_STEP * nj_sin(t * 0x600);
        self.timer = self.timer.wrapping_add(1);
        let base = if self.flipped() {
            self.placement[1] - SINK
        } else {
            SINK + self.placement[1]
        };
        self.position[1] = base + wobble;
        let m = self.nozzle_matrix();
        let at = mult_vec(&m, NOZZLE);
        let mut velocity = mult_vec_sr(&m, SMOKE_VELOCITY);
        out.push(RocketEvent::Engine { at });
        for v in velocity.iter_mut() {
            *v += 0.8 * (unit(rand) - 0.5);
        }
        let size = 3.5 + 2.0 * unit(rand);
        out.push(RocketEvent::Smoke {
            at,
            velocity,
            size,
            height: size + (self.placement[1] + 1.5),
        });
        if self.yaw as u16 == yaw as u16 && self.pitch as u16 == pitch as u16 {
            self.hatch -= STEP;
            if self.hatch > 0.0 {
                return;
            }
            self.thrust += THRUST_STEP;
            self.hatch = 0.0;
            if self.thrust < 1.0 {
                return;
            }
            self.thrust = 1.0;
            self.timer = 0;
            self.action = 6;
            out.push(RocketEvent::Rumble { slot: self.slot });
        }
    }

    fn fly(&mut self, input: &RocketInputs<'_>, rand: &mut MslRand, out: &mut Vec<RocketEvent>) {
        let mut d = [
            -self.position[0] + self.target[0],
            -self.position[1] + self.target[1],
            -self.position[2] + self.target[2],
        ];
        let len = nj_scalor(d);
        // The child's own +0x4C, which nothing writes, is never above 15.
        if len >= SPEED {
            let k = SPEED / len;
            d = d.map(|c| c * k);
            for (p, c) in self.position.iter_mut().zip(d) {
                *p += c;
            }
        } else if input.child_hit {
            self.action = 7;
        } else {
            for (p, c) in self.position.iter_mut().zip(d) {
                *p += c;
            }
            out.push(RocketEvent::Linked { id: self.id() });
            self.action = 7;
        }
        if self.action == 7 {
            out.push(RocketEvent::Explosion {
                at: self.position,
                velocity: d.map(|c| 0.2 * c),
            });
        }
        let velocity = d.map(|c| c * 0.5);
        let m = self.nozzle_matrix();
        let at = mult_vec(&m, NOZZLE);
        out.push(RocketEvent::Engine { at });
        if len > EXHAUST_RANGE {
            let mut v = velocity;
            for c in v.iter_mut() {
                *c += LIFT_STEP * (unit(rand) - 0.5);
            }
            let size = 3.0 + 2.0 * unit(rand);
            out.push(RocketEvent::Exhaust {
                at,
                velocity: v,
                size,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_sqrt_is_close() {
        for x in [1.0f32, 2.0, 100.0, 12345.0] {
            assert!((local_sqrtf(x) - x.sqrt()).abs() <= x.sqrt() * 1e-6);
        }
        assert_eq!(local_sqrtf(-1.0), -1.0);
    }

    #[test]
    fn rocket_arms_aims_flies_and_links() {
        let mut r = Rocket::new(0, 0, [0.0; 3], [0, 0, 3], [200.0, 0.0, 0.0]);
        let mut rand = MslRand::default();
        let yes = |_| true;
        let rider = [Some(([-15.1, 2.6, 0.0], true)), None];
        let input = RocketInputs {
            players: rider,
            centres: [Some([0.0; 3]), None],
            suppressed: false,
            switch_on: &yes,
            group_clear: &yes,
            ori_listed: &yes,
            child_hit: false,
        };
        let mut linked = false;
        for _ in 0..2000 {
            for e in r.step(&input, &mut rand) {
                linked |= matches!(e, RocketEvent::Linked { id: 3 });
            }
            if linked {
                break;
            }
        }
        assert!(linked);
        assert_eq!(r.action, 7);
        r.step(&input, &mut rand);
        // Mode 0 re-arms from the placement.
        assert_eq!((r.action, r.position), (2, [0.0; 3]));
    }
}
