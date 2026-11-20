//! Fixed SET ring tasks from SA2 Battle (GSNE8P). See docs/STAGE-GAMEPLAY.md.
//! The caller supplies range/scheduling, collision results and the spin gate.
//! These fixed-ring paths apply when neither player has the magnetic shield.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RingMode {
    Init,
    Active,
    Collected,
    Removed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RingHit {
    pub player: u8,
    pub item: u16,
}

#[derive(Clone, Copy, Debug)]
pub struct RingPlayer {
    pub center: [f32; 3],
    pub item: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RingEvents {
    /// Native calls, in order: AddScore(10), AddNumRing(slot, 1),
    /// AddMechHP(slot, 0.1), SE_CallRing(slot).
    pub player_reward: Option<u8>,
    /// Request the four native o_ring_0 particles at the task position.
    pub particles: bool,
    /// The ordinary Ring registers even during the pickup frame.
    pub register_collision: bool,
    /// The SET-backed task calls DeadOut on its next execution.
    pub dead_out: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct StageRing {
    pub position: [f32; 3],
    pub yaw: i32,
    pub mode: RingMode,
}

impl StageRing {
    pub const COLLISION_RADIUS: f32 = 5.0;

    pub fn new(position: [f32; 3], yaw: i32) -> Self {
        Self {
            position,
            yaw,
            mode: RingMode::Init,
        }
    }

    /// Ordinary Ring. A hit represents a set CCL hit bit and a non-null
    /// CCL_IsHitPlayer result, with the slot returned by IsThisTaskPlayer.
    pub fn step(&mut self, hit: Option<RingHit>, spin_enabled: bool) -> RingEvents {
        self.advance(hit, spin_enabled, true)
    }

    /// Alternate o_ring_M1 callback. Players denote consecutive valid task and
    /// collision-work pairs; the source stops iterating at the first absent slot.
    pub fn step_m1(
        &mut self,
        players: [Option<RingPlayer>; 2],
        active_mask: u8,
        spin_enabled: bool,
    ) -> RingEvents {
        let hit = players
            .into_iter()
            .map_while(|p| p)
            .enumerate()
            .find_map(|(slot, p)| {
                let [x, y, z] = std::array::from_fn(|i| p.center[i] - self.position[i]);
                (active_mask & (1 << slot) != 0 && (x * x + y * y + z * z).sqrt() < 14.0).then_some(
                    RingHit {
                        player: slot as u8,
                        item: p.item,
                    },
                )
            });
        self.advance(hit, spin_enabled, false)
    }

    fn advance(&mut self, hit: Option<RingHit>, spin_enabled: bool, uses_ccl: bool) -> RingEvents {
        let mut events = RingEvents::default();
        match self.mode {
            RingMode::Init => self.mode = RingMode::Active,
            RingMode::Active => {
                if let Some(hit) = hit {
                    self.mode = RingMode::Collected;
                    events.particles = true;
                    if hit.player < 2 && hit.item & 0x4000 == 0 {
                        events.player_reward = Some(hit.player);
                    }
                }
                if spin_enabled {
                    self.yaw = self.yaw.wrapping_add(0x16c);
                }
                events.register_collision = uses_ccl;
            }
            RingMode::Collected => {
                self.mode = RingMode::Removed;
                events.dead_out = true;
            }
            RingMode::Removed => {}
        }
        events
    }
}

/// Arguments to the original MakeParticle service; rendering remains with the host.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct PickupParticle {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub scale: f32,
}

/// GSNE8P o_ring_0, in native submission order. The angle service is injectable.
pub fn pickup_particles(
    position: [f32; 3],
    mut sin_cos: impl FnMut(u16) -> (f32, f32),
) -> [PickupParticle; 4] {
    std::array::from_fn(|n| {
        let (s, c) = sin_cos((n as u16) * 0x4000);
        PickupParticle {
            position: [
                position[0] + 4.0 * s,
                (position[1] + 1.8 * (4 - n) as f32) - 1.5,
                position[2] + 4.0 * c,
            ],
            velocity: [0.0, 0.02, 0.0],
            scale: 2.5,
        }
    })
}
