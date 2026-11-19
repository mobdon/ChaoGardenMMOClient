//! Damage ring scatter and the moving ring's collision response (GSNE8P o_ring.c).
//! Host owns the wallet, task allocator, shadow query, native trig and rendering.
use crate::{Error, Result};
use serde::Serialize;

#[derive(Debug, Serialize, PartialEq)]
pub enum DamageCall {
    KillPlayer(u8),
    AfterKill,
    DebitRings(u8, i32),
    CreateDroppedRing {
        slot: u8,
        position: [f32; 3],
        yaw: i32,
    },
    Sound(u16),
}
/// Return requested calls in order. A task is initialized only when allocation succeeds.
/// The input ring balance is a snapshot within a single authoritative update.
pub fn scatter(
    slot: u8,
    base_character: u8,
    rings: u16,
    position: [f32; 3],
    random: f32,
) -> Result<Vec<DamageCall>> {
    if slot > 1 || !position.iter().all(|v| v.is_finite()) || !(0.0..=1.0).contains(&random) {
        return Err(Error::invalid("invalid damage-ring inputs"));
    }
    if rings == 0 {
        return Ok(if matches!(base_character, 6 | 7) {
            vec![]
        } else {
            vec![DamageCall::KillPlayer(slot), DamageCall::AfterKill]
        });
    }
    let count = i32::from(rings.min(20));
    let mut calls = vec![DamageCall::DebitRings(slot, -i32::from(rings))];
    for i in 0..count {
        // Original divides the integer product before adding the random phase.
        let yaw = ((random * 360. + (360 * i / count) as f32) * (65536f32 / 360.)) as i32;
        calls.push(DamageCall::CreateDroppedRing {
            slot,
            position,
            yaw,
        });
    }
    calls.push(DamageCall::Sound(0x8014));
    Ok(calls)
}

pub type Vector = [f32; 3];
/// o_ring_5 uses independent tests; Z overrides Y, and Y overrides X.
pub fn gravity_face(g: Vector) -> usize {
    let mut f = usize::from(g[0] > 0.9);
    if g[1] > 0.9 {
        f = 3;
    } else if g[1] < -0.9 {
        f = 2;
    }
    if g[2] > 0.9 {
        f = 5;
    } else if g[2] < -0.9 {
        f = 4;
    }
    f
}
fn rot_x(v: Vector, (s, c): (f32, f32)) -> Vector {
    [v[0], c * v[1] - s * v[2], s * v[1] + c * v[2]]
}
fn rot_z(v: Vector, (s, c): (f32, f32)) -> Vector {
    [c * v[0] - s * v[1], s * v[0] + c * v[1], v[2]]
}

#[derive(Debug, Clone, Copy, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Surface {
    pub coordinate: f32,
    pub pitch: i32,
    pub roll: i32,
}
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub struct Flight {
    pub position: Vector,
    pub velocity: Vector,
}
impl Flight {
    /// One moving-mode physics update after the task's lifetime and collision queries.
    /// The surface coordinate is on the gravity-selected world axis, not always Y.
    pub fn step(
        &mut self,
        g: Vector,
        ground: Surface,
        sin_cos: impl Fn(i32) -> (f32, f32),
    ) -> bool {
        let axis = gravity_face(g) / 2;
        let mut origin = self.position;
        origin[axis] = ground.coordinate;
        for (i, gravity) in g.into_iter().enumerate() {
            let v = (self.velocity[i] + 0.08 * gravity).clamp(-4., 4.);
            self.position[i] += v;
            self.velocity[i] = v * (1. - 1. / 200.);
        }
        let to_local = |v| {
            rot_x(
                rot_z(v, sin_cos(ground.roll.wrapping_neg())),
                sin_cos(ground.pitch.wrapping_neg()),
            )
        };
        let to_world = |v| rot_z(rot_x(v, sin_cos(ground.pitch)), sin_cos(ground.roll));
        let mut pos = to_local(std::array::from_fn(|i| self.position[i] - origin[i]));
        if pos[1] >= 3.44 {
            return false;
        }
        let mut vel = to_local(self.velocity);
        pos[1] = 3.44;
        vel[1] = vel[1].abs() * 0.4;
        vel[0] *= 1. - 1. / 20.;
        vel[2] *= 1. - 1. / 20.;
        let offset = to_world(pos);
        self.position = std::array::from_fn(|i| origin[i] + offset[i]);
        self.velocity = to_world(vel);
        true
    }
}

/// Shared nine-slot stagger used by dropped rings when selecting the next floor probe.
#[derive(Default)]
pub struct ProbeClock(u8);
impl ProbeClock {
    pub fn next_delay(&mut self) -> u8 {
        let delay = self.0;
        self.0 = (delay + 1) % 9;
        delay
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum Phase {
    Flying,
    Resting,
    Collected,
    Expired,
    Destroyed,
}
#[derive(Debug, Default, PartialEq, Serialize)]
pub struct Tick {
    pub bounce: bool,
    pub register_at: Option<Vector>,
    pub credit_slot: Option<u8>,
    pub particles: bool,
    pub destroy: bool,
}
/// External pickup and removal calls from Tobitiri modes 1, 2, 4 and 3.
#[derive(Debug, Serialize)]
pub enum PickupCall {
    AddNumRing(u8, i32),
    SECallRing(u8),
    AddMechHP(u8, f32),
    MakeParticle(crate::stage_gameplay::PickupParticle),
    DestroyTask,
}
impl Tick {
    pub fn pickup_and_removal_calls(
        &self,
        position: Vector,
        trig: impl FnMut(u16) -> (f32, f32),
    ) -> Vec<PickupCall> {
        use PickupCall::*;
        let mut calls = Vec::new();
        if let Some(slot) = self.credit_slot {
            calls.extend([AddNumRing(slot, 1), SECallRing(slot), AddMechHP(slot, 0.1)]);
        }
        if self.particles {
            calls.extend(crate::stage_gameplay::pickup_particles(position, trig).map(MakeParticle));
        }
        if self.destroy {
            calls.push(DestroyTask);
        }
        calls
    }
}
#[derive(Debug, Serialize)]
pub struct DroppedRing {
    pub flight: Flight,
    pub surface: Surface,
    pub phase: Phase,
    pub timer: u16,
    pub collision_delay: u8,
    pub age: u8,
    pub spin: f32,
    resting_shadow: f32,
    tick_even_if_paused: bool,
}
fn fallback(g: Vector, p: Vector) -> f32 {
    let face = gravity_face(g);
    p[face / 2] + if face % 2 == 1 { 100. } else { -100. }
}
impl DroppedRing {
    /// Initial flight after a successful work allocation. The two orientation
    /// angles and the sine/cosine routine come from the gravity/renderer host.
    #[allow(clippy::too_many_arguments)]
    pub fn launch(
        position: Vector,
        yaw: i32,
        g: Vector,
        gravity_pitch: i32,
        gravity_roll: i32,
        random: f32,
        paused: bool,
        clock: &mut ProbeClock,
        sin_cos: impl Fn(i32) -> (f32, f32),
    ) -> Result<Self> {
        if !(0.0..=1.).contains(&random) || !position.iter().chain(&g).all(|x| x.is_finite()) {
            return Err(Error::invalid("invalid launch observation"));
        }
        let (s, c) = sin_cos(yaw);
        Ok(Self {
            flight: Flight {
                position: std::array::from_fn(|i| position[i] - 10. * g[i]),
                velocity: rot_z(
                    rot_x([0.4 * s, 1.8, 0.4 * c], sin_cos(gravity_pitch)),
                    sin_cos(gravity_roll),
                ),
            },
            surface: Surface {
                coordinate: fallback(g, position),
                pitch: 0,
                roll: 0,
            },
            phase: Phase::Flying,
            timer: 0,
            collision_delay: clock.next_delay(),
            age: 0,
            spin: ((random * 360. * (65536f32 / 360.)) as i32) as f32,
            resting_shadow: 0.,
            tick_even_if_paused: paused,
        })
    }
    /// Initialize the model returned by CreateRingObject at the end of launch.
    pub fn initialize_model(&self, m: &mut crate::ring_models::Model) {
        m.position = self.flight.position;
        m.yaw = (self.spin * (65536f32 / 360.)) as i32;
        m.shadow_angles = [0, m.yaw, 0];
        m.shadow_y = m.position[1] + 3.44;
    }

    /// Tobitiri copies task state before its mode switch, after CheckRangeOut.
    pub fn sync_model(&self, m: &mut crate::ring_models::Model, paused: bool, range_out: bool) {
        if range_out || self.phase == Phase::Destroyed {
            return;
        }
        m.position = self.flight.position;
        m.yaw = (self.spin * (65536f32 / 360.)) as i32;
        m.shadow_angles = [self.surface.pitch, m.yaw, self.surface.roll];
        if self.phase == Phase::Flying {
            m.shadow_y = self.surface.coordinate + 0.4;
            if !self.tick_even_if_paused && paused {
                m.hidden = self.age <= 46;
            }
        } else {
            m.shadow_y = self.resting_shadow + 0.4;
            m.hidden = self.timer & 2 == 0;
        }
    }

    /// hit is the separately resolved CCL player slot and its item bits.
    /// The host supplies the selected GetShadowPosXYZ result only on native probe ticks.
    pub fn tick(
        &mut self,
        g: Vector,
        paused: bool,
        range_out: bool,
        hit: Option<(u8, u16)>,
        mut probe: impl FnMut(Vector, usize) -> Option<Surface>,
        sin_cos: impl Fn(i32) -> (f32, f32),
    ) -> Tick {
        let mut calls = Tick::default();
        if range_out || self.phase == Phase::Destroyed {
            return calls;
        }
        let active = self.tick_even_if_paused || !paused;
        match self.phase {
            Phase::Expired | Phase::Collected => {
                calls.particles = self.phase == Phase::Collected;
                calls.destroy = true;
                self.phase = Phase::Destroyed;
                return calls;
            }
            Phase::Flying | Phase::Resting => {}
            Phase::Destroyed => unreachable!(),
        }
        if active {
            self.spin += 5.;
        }
        if let Some((slot, item)) = hit {
            if self.phase == Phase::Flying {
                self.flight.position[1] += 3.44;
            }
            self.spin = 0.;
            self.flight.velocity[0] = -2.;
            self.flight.velocity[2] = -4.;
            self.phase = Phase::Collected;
            if slot < 2 && item & 0x4000 == 0 {
                calls.credit_slot = Some(slot);
            }
            return calls;
        }
        if self.phase == Phase::Resting {
            let p = self.flight.position;
            let v = self.flight.velocity;
            calls.register_at = Some([p[0] + v[0], p[1] + 3.44, p[2] + v[2]]);
            if active {
                self.timer = self.timer.wrapping_add(1);
                if self.timer >= 180 {
                    self.phase = Phase::Expired;
                }
            }
            return calls;
        }
        if self.collision_delay == 0 {
            let pos = self.flight.position;
            if let Some(s) = probe(
                std::array::from_fn(|i| pos[i] - 3.44 * g[i]),
                gravity_face(g),
            ) {
                self.surface = s;
            } else {
                self.surface.coordinate = fallback(g, pos);
            }
            self.collision_delay = 8;
        } else {
            self.collision_delay -= 1;
        }
        if active {
            self.timer = self.timer.wrapping_add(1);
            if self.timer > 240 {
                self.resting_shadow = self.surface.coordinate;
                self.phase = Phase::Resting;
                self.flight.velocity[0] = 0.;
                self.flight.velocity[2] = 0.;
                self.timer = 0;
                return calls;
            }
            calls.bounce = self.flight.step(g, self.surface, sin_cos);
            self.age += 1;
        }
        if self.age > 46 {
            self.age = 47;
            let mut p = self.flight.position;
            p[1] += 3.44;
            calls.register_at = Some(p);
        }
        calls
    }
}
