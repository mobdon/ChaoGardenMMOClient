//! Single-player selection task, starting after the screen's opening transition.
mod overlay;
pub mod screen;
use crate::{Error, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Character {
    Sonic,
    Tails,
    Knuckles,
    Amy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selection {
    Back,
    Confirm {
        character: Character,
        cpu_tails_sequence: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Carousel {
    age: u32,
    angle: u16,
    settled: u8,
    target: u8,
    direction: u8,
    remaining: u8,
    sequence: u8,
    exit: Option<Selection>,
}

impl Carousel {
    pub fn new(character: Character) -> Self {
        let slot = 32 + character as u8;
        Self {
            age: 0,
            angle: u16::from(slot) << 8,
            settled: slot,
            target: slot,
            direction: 0,
            remaining: 0,
            sequence: 255,
            exit: None,
        }
    }
    pub fn angle_q8(&self) -> u16 {
        self.angle & 1023
    }
    pub fn character(&self) -> Character {
        [
            Character::Sonic,
            Character::Tails,
            Character::Knuckles,
            Character::Amy,
        ][usize::from(self.angle_q8() >> 8)]
    }
    pub fn selection(&self) -> Option<Selection> {
        self.exit
    }

    /// One active task tick. Supply the GBA pressed-edge and repeated-key registers,
    /// not raw held buttons. A terminal choice is returned only on its first tick.
    pub fn step(&mut self, pressed: u16, repeated: u16) -> Result<Option<Selection>> {
        if (pressed | repeated) & !0x3ff != 0 {
            return Err(Error("invalid GBA button mask".into()));
        }
        if self.exit.is_some() {
            return Ok(None);
        }
        const KEYS: [u16; 8] = [16, 128, 16, 512, 16, 256, 16, 1];
        if pressed != 0 {
            if pressed & 64 != 0 && self.character() == Character::Sonic {
                self.sequence = 0;
            } else if self.sequence != 255 {
                self.sequence = if KEYS
                    .get(usize::from(self.sequence))
                    .is_some_and(|key| pressed & key != 0)
                {
                    self.sequence + 1
                } else {
                    255
                };
            }
        }
        let mut requested = i16::from(self.target);
        let ready = self.age > 33;
        if ready && self.target == self.settled && repeated & 3 != 0 {
            self.exit = Some(if repeated & 2 != 0 {
                Selection::Back
            } else {
                Selection::Confirm {
                    character: self.character(),
                    cpu_tails_sequence: self.sequence == 8,
                }
            });
            self.age = 0;
        } else {
            let direction = if ready && repeated & 32 != 0 {
                requested -= 1;
                2
            } else if ready && repeated & 16 != 0 {
                requested += 1;
                1
            } else if self.target > self.settled {
                1
            } else if self.target < self.settled {
                2
            } else {
                0
            };
            if self.direction == 0 && direction != 0 {
                self.direction = direction;
                self.remaining = 16;
            }
        }
        if self.remaining != 0 {
            self.angle = if self.direction == 1 {
                self.angle.wrapping_add(16)
            } else {
                self.angle.wrapping_sub(16)
            } & 0x3fff;
            self.remaining -= 1;
            if self.remaining == 0 {
                self.direction = 0;
            }
        }
        self.target = (requested & 63) as u8;
        let round_up = self.settled > self.target && self.angle & 255 != 0;
        self.settled = (((self.angle >> 8) as u8) + u8::from(round_up)) & 63;
        if self.settled > 36 && self.target > 36 && self.angle > 0x2400 {
            while self.settled > 36 {
                self.settled -= 4;
            }
            while self.target > 36 {
                self.target -= 4;
            }
            while self.angle > 0x2400 {
                self.angle -= 0x400;
            }
        } else if self.settled < 28 && self.target < 28 && self.angle < 0x1c00 {
            while self.settled < 28 {
                self.settled += 4;
            }
            while self.target < 28 {
                self.target += 4;
            }
            while self.angle < 0x1c00 {
                self.angle += 0x400;
            }
        }
        self.age = self.age.wrapping_add(1);
        Ok(self.exit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_active_tick_matches_the_retail_task() {
        let rows: Vec<[u32; 9]> =
            serde_json::from_str(include_str!("../tests/retail-character-input.json")).unwrap();
        let mut s = Carousel::new(Character::Sonic);
        assert_eq!(rows.len(), 186);
        for (n, [frame, mask, age, angle, settled, target, direction, remaining, seq]) in
            rows.into_iter().enumerate()
        {
            assert_eq!(frame, 1415 + n as u32);
            if n > 0 {
                s.step(mask as u16, mask as u16).unwrap();
            }
            assert_eq!(
                [
                    s.age,
                    u32::from(s.angle),
                    s.settled.into(),
                    s.target.into(),
                    s.direction.into(),
                    s.remaining.into(),
                    s.sequence.into()
                ],
                [age, angle, settled, target, direction, remaining, seq],
                "retail frame {frame}"
            );
        }
        assert_eq!(
            s.selection(),
            Some(Selection::Confirm {
                character: Character::Sonic,
                cpu_tails_sequence: true
            })
        );
    }
    #[test]
    fn rejects_unknown_bits_and_emits_cancel_once() {
        let mut s = Carousel::new(Character::Amy);
        let original = s.clone();
        for input in [(1024, 0), (0, 1024)] {
            assert!(s.step(input.0, input.1).is_err());
            assert_eq!(s, original);
        }
        for _ in 0..34 {
            s.step(0, 0).unwrap();
        }
        assert_eq!(s.step(3, 3).unwrap(), Some(Selection::Back));
        let terminal = s.clone();
        assert_eq!(s.step(1, 1).unwrap(), None);
        assert_eq!(s, terminal);
    }
}
