//! Original display-shadow setup and register transactions, not a PPU emulator.
//! The host must implement the MMIO/VBlank effects or return a strict error.

pub const SHADOW: u32 = 0x03000770;
pub const SHADOW_HALFWORDS: usize = 42;
const IO: u32 = 0x04000000;
// 0x30FC writes these registers in source order. The affine fields are not
// linearly ordered in the native shadow, and 0x4E is intentionally absent.
const REGISTER_OFFSETS: [u32; SHADOW_HALFWORDS] = [
    0, 4, 8, 10, 12, 14, 16, 20, 24, 28, 18, 22, 26, 30, 32, 34, 36, 38, 40, 42, 44, 46, 48, 50,
    52, 54, 56, 58, 60, 62, 64, 66, 68, 70, 72, 74, 76, 80, 82, 84, 0x208, 0x200,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisplayError {
    Memory { address: u32, write: bool },
    UnportedVBlank(u32),
    WaitBudget { requested: u32, limit: u32 },
}
impl std::fmt::Display for DisplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for DisplayError {}

pub trait DisplayMemory {
    fn read_halfword(&self, address: u32) -> Result<u16, DisplayError>;
    fn write_halfword(&mut self, address: u32, value: u16) -> Result<(), DisplayError>;
}
pub trait VBlank {
    /// Original 0x0D5C. Implementations may mutate the same shadow/MMIO memory,
    /// as the source interrupt path can. Counting a call is only oracle evidence.
    fn wait(&mut self, memory: &mut dyn DisplayMemory) -> Result<(), DisplayError>;
}
pub struct RejectVBlank;
impl VBlank for RejectVBlank {
    fn wait(&mut self, _: &mut dyn DisplayMemory) -> Result<(), DisplayError> {
        Err(DisplayError::UnportedVBlank(0xd5c))
    }
}

/// 0x3010: writes exactly 84 bytes, preserving adjacent shadow storage.
pub fn initialize(memory: &mut dyn DisplayMemory) -> Result<(), DisplayError> {
    for index in 0..SHADOW_HALFWORDS {
        let value = match index {
            0 => 0x11e0,
            1 => 8,
            2 => 0x1800,
            40 => 1,
            41 => 0x2001,
            _ => 0,
        };
        memory.write_halfword(SHADOW + index as u32 * 2, value)?;
    }
    Ok(())
}

/// 0x30FC: forward the source's halfword writes, including IME before IE.
/// Register side effects and timing belong to the supplied hardware boundary.
pub fn flush(memory: &mut dyn DisplayMemory) -> Result<(), DisplayError> {
    for (index, offset) in REGISTER_OFFSETS.into_iter().enumerate() {
        let value = memory.read_halfword(SHADOW + index as u32 * 2)?;
        memory.write_halfword(IO + offset, value)?;
    }
    Ok(())
}

/// 0x3250: the live DISPCNT bit gates clearing bit 7 in the shadow and live IO.
pub fn clear_forced_blank(memory: &mut dyn DisplayMemory) -> Result<(), DisplayError> {
    if memory.read_halfword(IO)? & 0x80 != 0 {
        let value = memory.read_halfword(SHADOW)? & !0x80;
        memory.write_halfword(SHADOW, value)?;
        let value = memory.read_halfword(SHADOW)?;
        memory.write_halfword(IO, value)?;
    }
    Ok(())
}

/// 0x3278: the interrupt-enabled branch waits at 0x0D5C and does not directly
/// write DISPCNT afterwards. The other branch immediately writes the shadow.
pub fn set_forced_blank(
    memory: &mut dyn DisplayMemory,
    vblank: &mut dyn VBlank,
) -> Result<(), DisplayError> {
    if memory.read_halfword(IO)? & 0x80 == 0 {
        let value = memory.read_halfword(SHADOW)? | 0x80;
        memory.write_halfword(SHADOW, value)?;
        if memory.read_halfword(IO + 4)? & 8 != 0 && memory.read_halfword(IO + 0x200)? & 1 != 0 {
            vblank.wait(memory)?;
        } else {
            let value = memory.read_halfword(SHADOW)?;
            memory.write_halfword(IO, value)?;
        }
    }
    Ok(())
}

/// 0x3230: source unsigned decrement loop. Refuse a wait exceeding the caller's
/// explicit budget before its first effect; the source can otherwise block for
/// nearly 2^32 frames when passed a negative-looking word.
pub fn wait_frames(
    memory: &mut dyn DisplayMemory,
    vblank: &mut dyn VBlank,
    frames: u32,
    limit: u32,
) -> Result<(), DisplayError> {
    if frames > limit {
        return Err(DisplayError::WaitBudget {
            requested: frames,
            limit,
        });
    }
    for _ in 0..frames {
        vblank.wait(memory)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    struct Memory {
        shadow: [u16; SHADOW_HALFWORDS],
        io: [u16; 0x210 / 2],
        writes: Vec<Value>,
    }
    impl DisplayMemory for Memory {
        fn read_halfword(&self, address: u32) -> Result<u16, DisplayError> {
            let value = if address >= IO {
                address
                    .checked_sub(IO)
                    .and_then(|i| self.io.get(i as usize / 2))
            } else {
                address
                    .checked_sub(SHADOW)
                    .and_then(|i| self.shadow.get(i as usize / 2))
            };
            value.copied().ok_or(DisplayError::Memory {
                address,
                write: false,
            })
        }
        fn write_halfword(&mut self, address: u32, value: u16) -> Result<(), DisplayError> {
            let slot = if address >= IO {
                address
                    .checked_sub(IO)
                    .and_then(|i| self.io.get_mut(i as usize / 2))
            } else {
                address
                    .checked_sub(SHADOW)
                    .and_then(|i| self.shadow.get_mut(i as usize / 2))
            };
            *slot.ok_or(DisplayError::Memory {
                address,
                write: true,
            })? = value;
            self.writes.push(json!([address, value]));
            Ok(())
        }
    }
    struct InterceptedVBlank(u32);
    impl VBlank for InterceptedVBlank {
        fn wait(&mut self, _: &mut dyn DisplayMemory) -> Result<(), DisplayError> {
            self.0 += 1;
            Ok(())
        }
    }
    #[test]
    fn original_register_writes_and_wait_branches_match() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/display-vectors.json")).unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let mut memory = Memory {
                shadow: std::array::from_fn(|i| case["shadow"][i].as_u64().unwrap() as u16),
                io: std::array::from_fn(|i| case["io"][i].as_u64().unwrap() as u16),
                writes: Vec::new(),
            };
            let mut wait = InterceptedVBlank(0);
            match case["operation"].as_str().unwrap() {
                "initialize" => initialize(&mut memory),
                "flush" => flush(&mut memory),
                "clear" => clear_forced_blank(&mut memory),
                "set" => set_forced_blank(&mut memory, &mut wait),
                "wait" => wait_frames(
                    &mut memory,
                    &mut wait,
                    case["argument"].as_u64().unwrap() as u32,
                    256,
                ),
                _ => panic!("unknown fixture operation"),
            }
            .unwrap();
            assert_eq!(memory.writes, case["writes"].as_array().unwrap().as_slice());
            assert_eq!(wait.0, case["waits"].as_u64().unwrap() as u32);
            assert_eq!(json!(memory.shadow.as_slice()), case["afterShadow"]);
            assert_eq!(json!(memory.io.as_slice()), case["afterIo"]);
        }
    }
    #[test]
    fn missing_vblank_and_excessive_wait_stop_explicitly() {
        let mut memory = Memory {
            shadow: [0; 42],
            io: [0; 0x210 / 2],
            writes: Vec::new(),
        };
        memory.io[2] = 8;
        memory.io[0x200 / 2] = 1;
        assert_eq!(
            set_forced_blank(&mut memory, &mut RejectVBlank),
            Err(DisplayError::UnportedVBlank(0xd5c))
        );
        assert_eq!(memory.shadow[0], 0x80);
        assert_eq!(memory.io[0], 0);
        let mut wait = InterceptedVBlank(0);
        assert_eq!(
            wait_frames(&mut memory, &mut wait, u32::MAX, 256),
            Err(DisplayError::WaitBudget {
                requested: u32::MAX,
                limit: 256
            })
        );
        assert_eq!(wait.0, 0);
    }
}
