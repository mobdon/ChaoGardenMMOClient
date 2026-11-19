//! Single Player Act 1 title-card clock. Kept separate from the gameplay clock.
pub mod act_label;
pub mod zone_name;
use crate::{Error, Result};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActIntro {
    tick: u16,
    held: u16,
}

impl ActIntro {
    /// Buttons already down on stage entry must not create another press.
    pub fn new(held: u16) -> Result<Self> {
        if held & !0x3ff != 0 {
            return Err(Error("invalid initial GBA button mask".into()));
        }
        Ok(Self { tick: 0, held })
    }
    pub fn tick(&self) -> u16 {
        self.tick
    }
    pub fn locked(&self) -> bool {
        self.tick < 226
    }
    /// Advance one task update. Returns true only on the movement-unlock tick.
    pub fn step(&mut self, held: u16) -> Result<bool> {
        if held & !0x3ff != 0 {
            return Err(Error("invalid GBA button mask".into()));
        }
        if !self.locked() {
            return Ok(false);
        }
        let pressed = held & !self.held;
        self.held = held;
        if pressed & 0xf3 != 0 && (36..=214).contains(&self.tick) {
            self.tick = 215;
        }
        self.tick += 1;
        Ok(!self.locked())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn skip_window_is_edge_triggered_and_excludes_start_select_and_shoulders() {
        for before in [35, 36, 214, 215] {
            for button in [1, 2, 4, 8, 16, 32, 64, 128, 256, 512] {
                let mut intro = ActIntro::default();
                for _ in 0..before {
                    intro.step(0).unwrap();
                }
                intro.step(button).unwrap();
                assert_eq!(
                    intro.tick(),
                    if (36..=214).contains(&before) && button & 0xf3 != 0 {
                        216
                    } else {
                        before + 1
                    }
                );
            }
        }
        let mut held = ActIntro::new(1).unwrap();
        for _ in 0..100 {
            held.step(1).unwrap();
        }
        assert_eq!(held.tick(), 100);
        let before = held;
        assert!(held.step(1024).is_err());
        assert_eq!(held, before);
    }
    #[test]
    fn movement_lock_matches_both_complete_retail_routes() {
        for (skip, fixture) in [
            (false, include_str!("../tests/retail-act-intro.json")),
            (true, include_str!("../tests/retail-act-intro-skip.json")),
        ] {
            let trace: Vec<(u32, i32, i32, u32, String)> = serde_json::from_str(fixture).unwrap();
            assert_eq!(trace.len(), if skip { 75 } else { 228 });
            let mut intro = ActIntro::default();
            let mut releases = 0;
            for (i, (frame, _x, _y, flags, _hash)) in trace.into_iter().enumerate() {
                assert_eq!(frame, 1657 + i as u32);
                if i > 0 {
                    releases += u32::from(intro.step(u16::from(skip && frame == 1720)).unwrap());
                }
                assert_eq!(
                    intro.locked(),
                    flags & 0x200000 != 0,
                    "retail frame {frame}"
                );
            }
            assert_eq!(releases, 1);
        }
    }
}
