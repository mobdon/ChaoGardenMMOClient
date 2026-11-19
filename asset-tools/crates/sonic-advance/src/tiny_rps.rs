//! ASOE Tiny Garden's three-lane rock/scissors/paper shooting game.
use crate::{catalog, Result};

pub const FIRE: u16 = 1;
pub const LEFT: u16 = 0x20;
pub const RIGHT: u16 = 0x10;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Hand {
    Rock,
    Scissors,
    Paper,
}
impl Hand {
    fn from_index(n: u32) -> Self {
        [Self::Rock, Self::Scissors, Self::Paper][n as usize]
    }
    pub fn against(self, other: Self) -> Hit {
        match (3 + self as u8 - other as u8) % 3 {
            0 => Hit::Tie,
            2 => Hit::Win,
            _ => Hit::Lose,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Win,
    Tie,
    Lose,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    Time,
    OutOfShots,
    UnresolvedReload,
}
#[derive(Clone, Copy, Debug)]
pub struct Target {
    pub hand: Hand,
    pub phase: u8,
    pub age: u8,
    pub progress: i16,
}
impl Target {
    pub fn position(self) -> Option<(i16, i16)> {
        match self.progress {
            p if p < 0 => None,
            p @ 0..64 => Some((10, p + 8)),
            p @ 64..196 => Some((p - 54, 72)),
            p @ 196..260 => Some((142, 268 - p)),
            p => Some((402 - p, 8)),
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Lane {
    pub hand: Hand,
    pub phase: u8,
    pub age: u8,
    pub y: i16,
}
impl Lane {
    fn set_phase(&mut self, phase: u8) {
        self.phase = phase;
        self.age = 0;
    }
}
#[derive(Clone, Copy)]
struct Rand(u32);
impl Rand {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(0x41c64e6d).wrapping_add(0x3039);
        (self.0 >> 16) & 0x7fff
    }
    fn hand(&mut self) -> Hand {
        Hand::from_index(self.next() / 0x2aab)
    }
}
// The background Chao's decisions consume the very same seed as the hands.
struct Spectator {
    x: i16,
    delay: i16,
    action: u32,
}
impl Spectator {
    fn step(&mut self, rand: &mut Rand) {
        match self.action {
            0 => self.x -= 1,
            1 => self.x += 1,
            _ => {}
        }
        self.delay -= 1;
        if self.delay != 0 {
            return;
        }
        self.action = rand.next() / 0x1556;
        if self.action <= 1 {
            let target = loop {
                let n = (rand.next() / 456) as i16 + 44;
                if n != self.x {
                    break n;
                }
            };
            self.action = u32::from(target > self.x);
            self.delay = (target - self.x).abs();
        } else {
            self.delay = (rand.next() / 182) as i16 + 60;
        }
    }
}
pub struct Round {
    shape: [u8; 47],
    rand: Rand,
    spectator: Spectator,
    targets: [Target; 10],
    lanes: [Lane; 3],
    selected: usize,
    keys: crate::garden_keys::GardenKeys,
    seconds: u8,
    subframe: u8,
    lives: u8,
    wave: u8,
    speed: i16,
    remaining: u8,
    points: u8,
    end: Option<End>,
}
impl Round {
    /// Resolve the native collision profile from the fingerprinted US program.
    pub fn from_rom(rom: &[u8], seed: u32) -> Result<Self> {
        let payload = catalog::garden(rom)?;
        let shape = payload.bytes[0x11f84..0x11fb3]
            .try_into()
            .expect("verified fixed payload");
        let mut rand = Rand(seed);
        let targets = std::array::from_fn(|i| Target {
            hand: rand.hand(),
            phase: 0,
            age: 0,
            progress: -2 - 32 * i as i16,
        });
        let lanes = std::array::from_fn(|_| Lane {
            hand: rand.hand(),
            phase: 0,
            age: 0,
            y: 116,
        });
        Ok(Self {
            shape,
            rand,
            spectator: Spectator {
                x: 80,
                delay: 60,
                action: 2,
            },
            targets,
            lanes,
            selected: 1,
            keys: Default::default(),
            seconds: 30,
            subframe: 0,
            lives: 5,
            wave: 0,
            speed: 2,
            remaining: 10,
            points: 0,
            end: None,
        })
    }
    pub fn targets(&self) -> &[Target; 10] {
        &self.targets
    }
    pub fn lanes(&self) -> &[Lane; 3] {
        &self.lanes
    }
    pub fn selected(&self) -> usize {
        self.selected
    }
    pub fn time(&self) -> (u8, u8) {
        (self.seconds, self.subframe)
    }
    pub fn lives(&self) -> u8 {
        self.lives
    }
    pub fn points(&self) -> u8 {
        self.points
    }
    /// Local result-screen counter once outstanding winning hits finish.
    pub fn result_points(&self) -> Option<u8> {
        if !matches!(self.end, Some(End::Time | End::OutOfShots)) {
            return None;
        }
        let pending = self.targets.iter().filter(|t| t.phase == 2).count() as u16;
        Some((u16::from(self.points) + pending * (u16::from(self.wave) / 2 + 1)).min(99) as u8)
    }
    pub fn wave(&self) -> u8 {
        self.wave
    }
    pub fn end(&self) -> Option<End> {
        self.end
    }
    pub fn seed(&self) -> u32 {
        self.rand.0
    }
    pub fn step(&mut self, held: u16) {
        if self.end.is_some() {
            return;
        }
        self.keys.poll(held, true);
        let pressed = self.keys.pressed;
        self.spectator.step(&mut self.rand);
        self.move_targets();
        for i in 0..3 {
            self.update_lane(i);
            if self.end.is_some() {
                return;
            }
        }
        if pressed & LEFT != 0 {
            self.selected = self.selected.saturating_sub(1);
        }
        if pressed & RIGHT != 0 {
            self.selected = (self.selected + 1).min(2);
        }
        if pressed & FIRE != 0 && self.lanes[self.selected].phase == 0 {
            self.lanes[self.selected].phase = 1;
        }
        if self.lives == 0 {
            self.end = Some(End::OutOfShots);
        } else if self.subframe != 0 {
            self.subframe -= 1;
        } else if self.seconds != 0 {
            self.seconds -= 1;
            self.subframe = 59;
        } else if self.remaining != 0 {
            self.end = Some(End::Time);
        }
    }
    fn move_targets(&mut self) {
        for target in &mut self.targets {
            let age = target.age;
            target.age = age.wrapping_add(1);
            match target.phase {
                0 => {
                    target.progress += self.speed;
                    if target.progress > 392 {
                        target.progress -= 392;
                    }
                }
                2 | 3 if age == 30 => {
                    if target.phase == 2 {
                        self.points = (self.points + self.wave / 2 + 1).min(99);
                    }
                    target.phase = 4;
                }
                _ => {}
            }
        }
        if self.targets.iter().all(|t| t.phase == 4) {
            self.wave = self.wave.wrapping_add(1);
            self.remaining = 10;
            for (i, t) in self.targets.iter_mut().enumerate() {
                t.phase = 0;
                t.hand = self.rand.hand();
                t.progress = -32 * i as i16 - self.speed;
            }
            self.seconds = self.seconds.saturating_add(10);
            if self.seconds >= 99 {
                self.seconds = 99;
                self.subframe = 0;
            }
        }
        self.speed = if self.remaining <= 1 { 4 } else { 2 };
    }
    fn spend_life(&mut self, i: usize) {
        self.lives -= 1;
        self.lanes[i].set_phase(if self.lives > 2 { 7 } else { 9 });
    }
    fn update_lane(&mut self, i: usize) {
        let lane = &mut self.lanes[i];
        let age = lane.age;
        lane.age = age.wrapping_add(1);
        match lane.phase {
            1 => {
                lane.y -= 4;
                if lane.y < -32 {
                    self.spend_life(i);
                    return;
                }
                let x = 12 + 64 * i as i16;
                for target in &mut self.targets {
                    if target.phase != 0 {
                        continue;
                    }
                    let Some((tx, ty)) = target.position() else {
                        continue;
                    };
                    let dy = ty - lane.y + 25;
                    let Some(&width) = usize::try_from(dy).ok().and_then(|n| self.shape.get(n))
                    else {
                        continue;
                    };
                    if !(-i16::from(width)..i16::from(width)).contains(&(tx - x)) {
                        continue;
                    }
                    match lane.hand.against(target.hand) {
                        Hit::Lose => lane.set_phase(5),
                        hit => {
                            lane.set_phase(2);
                            target.phase = if hit == Hit::Win { 2 } else { 3 };
                            self.remaining -= 1;
                        }
                    }
                    target.age = 0;
                    break;
                }
            }
            2 if age == 30 => lane.set_phase(3),
            3 if age == 20 => lane.set_phase(4),
            4 if age == 20 => lane.set_phase(7),
            5 | 6 if age == 30 => self.spend_life(i),
            7 if age == 20 => lane.set_phase(8),
            8 if age == 10 => {
                let mut hand = self.rand.hand();
                if self.rand.next() < 0x2000 {
                    let mut available = [Hand::Rock; 10];
                    let mut count = 0;
                    for target in &self.targets {
                        if target.phase == 0 {
                            available[count] = target.hand;
                            count += 1;
                        }
                    }
                    if count > 0 {
                        let divisor = (0x7ffe + count as u32) / count as u32;
                        let index = (self.rand.next() / divisor) as usize;
                        // For a maximal draw and certain counts the original addresses
                        // an unwritten stack byte beyond its freshly built candidate list.
                        let Some(&picked) = available[..count].get(index) else {
                            self.end = Some(End::UnresolvedReload);
                            return;
                        };
                        hand = Hand::from_index((picked as u32 + 2) % 3);
                    }
                }
                lane.hand = hand;
            }
            8 if age == 20 => {
                lane.phase = 0;
                lane.y = 116;
            }
            _ => {}
        }
    }
}
