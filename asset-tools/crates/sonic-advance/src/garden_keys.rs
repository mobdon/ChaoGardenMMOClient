//! ASOE Tiny Garden keypad state at IWRAM 0x03003830.
#[derive(Clone, Copy, Debug)]
pub struct GardenKeys {
    held: u16,
    pub pressed: u16,
    pub repeated: u16,
    countdown: u8,
}

impl Default for GardenKeys {
    fn default() -> Self {
        Self {
            held: 0,
            pressed: 0,
            repeated: 0,
            countdown: 15,
        }
    }
}

impl GardenKeys {
    /// Accept active-high GBA keys. Returns the retail soft-reset request;
    /// reset execution belongs to the caller.
    pub fn poll(&mut self, held: u16, reset_inhibited: bool) -> bool {
        let held = held & 0x3ff;
        let old = self.held;
        self.held = held;
        self.pressed = held & !old;
        if !reset_inhibited && held & 15 == 15 {
            return true;
        }
        self.repeated = if held == 0 {
            0
        } else if held > old {
            self.countdown = 15;
            old ^ held
        } else {
            self.countdown = self.countdown.wrapping_sub(1);
            if self.countdown == 0 {
                self.countdown = 7;
                held
            } else {
                0
            }
        };
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cold_boot_menu_to_garden_input() {
        type RetailInputSample = (usize, u16, u16, u16, u16, u8, u8, u8, String);
        let samples: Vec<RetailInputSample> =
            serde_json::from_str(include_str!("../tests/retail-garden-input.json")).unwrap();
        let mut state = GardenKeys::default();
        let mut checked = 0;
        for frame in 1700..=1722 {
            let held = match frame {
                1701..=1720 => 16,
                1721 => 1,
                _ => 0,
            };
            state.poll(held, false);
            if let Some(s) = samples.iter().find(|s| s.0 == frame) {
                assert_eq!(
                    (
                        state.held,
                        !state.held,
                        state.pressed,
                        state.repeated,
                        state.countdown,
                        15,
                        7
                    ),
                    (s.1, s.2, s.3, s.4, s.5, s.6, s.7),
                    "retail frame {frame}"
                );
                checked += 1;
            }
        }
        assert_eq!(checked, 7);
    }

    #[test]
    fn matches_1024_native_polls() {
        let mut state = GardenKeys::default();
        let mut trace = Vec::new();
        for i in 0..1024 {
            let held = if i < 512 {
                (i * 73) & 1023
            } else {
                [1, 2, 3, 0, 0x201][(i / 32) % 5]
            } as u16;
            assert!(!state.poll(held, true));
            for word in [state.held, !state.held, state.pressed, state.repeated] {
                trace.extend(word.to_le_bytes());
            }
            trace.extend([state.countdown, 15, 7]);
        }
        assert_eq!(
            crate::binary::sha256(&trace),
            "eeb9da0160a29bdcd233be3a8f178dfd0e6db842d57b2ae787dcc13fad8817e2"
        );
        assert!(state.poll(15, false));
    }
}
