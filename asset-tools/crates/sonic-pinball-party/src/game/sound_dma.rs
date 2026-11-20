//! Native 0x5F040 / 0x5F0BC sound DMA gates. These translate RAM/MMIO
//! transactions; they do not implement the mixer, DMA hardware or audible output.

const INFO_SLOT: u32 = 0x03007ff0;
const MAGIC: u32 = 0x68736d53;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SoundDmaError {
    Memory {
        address: u32,
        width: u8,
        write: bool,
    },
    InvalidInfo(u32),
    UnportedCpuSet {
        destination: u32,
        control: u32,
    },
}
impl std::fmt::Display for SoundDmaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for SoundDmaError {}
pub trait SoundDmaMemory {
    fn read_word(&self, address: u32) -> Result<u32, SoundDmaError>;
    fn write_word(&mut self, address: u32, value: u32) -> Result<(), SoundDmaError>;
    fn write_halfword(&mut self, address: u32, value: u16) -> Result<(), SoundDmaError>;
    fn write_byte(&mut self, address: u32, value: u8) -> Result<(), SoundDmaError>;
}
pub trait CpuSet {
    /// Source 0x5FEB4 / SWI 0B. This call always supplies a stack word containing
    /// zero and control 0x05000318. A modeled memory fill is not BIOS parity.
    fn zero_fill(
        &mut self,
        memory: &mut dyn SoundDmaMemory,
        destination: u32,
        control: u32,
    ) -> Result<(), SoundDmaError>;
}
pub struct RejectCpuSet;
impl CpuSet for RejectCpuSet {
    fn zero_fill(
        &mut self,
        _: &mut dyn SoundDmaMemory,
        destination: u32,
        control: u32,
    ) -> Result<(), SoundDmaError> {
        Err(SoundDmaError::UnportedCpuSet {
            destination,
            control,
        })
    }
}

fn validate(info: u32, length: u32) -> Result<(), SoundDmaError> {
    let end = info
        .checked_add(length)
        .ok_or(SoundDmaError::InvalidInfo(info))?;
    if info & 3 != 0
        || !((info >= 0x02000000 && end <= 0x02040000) || (info >= 0x03000000 && end <= 0x03008000))
    {
        return Err(SoundDmaError::InvalidInfo(info));
    }
    Ok(())
}

/// 0x5F0BC checks the exact magic word. All other words take the source's
/// decrement-by-ten path, including values that look already suspended.
pub fn gate_5f0bc(memory: &mut dyn SoundDmaMemory) -> Result<(), SoundDmaError> {
    let info = memory.read_word(INFO_SLOT)?;
    validate(info, 5)?;
    let signature = memory.read_word(info)?;
    if signature != MAGIC {
        memory.write_halfword(0x040000c6, 0xb600)?;
        memory.write_halfword(0x040000d2, 0xb600)?;
        memory.write_byte(info + 4, 0)?;
        memory.write_word(info, signature.wrapping_sub(10))?;
    }
    Ok(())
}

/// 0x5F040 uses a wrapping unsigned two-value signature gate. Retain the exact
/// conditional DMA word writes before the final halfword writes and CpuSet.
pub fn gate_5f040(
    memory: &mut dyn SoundDmaMemory,
    bios: &mut dyn CpuSet,
) -> Result<(), SoundDmaError> {
    let info = memory.read_word(INFO_SLOT)?;
    validate(info, 4)?;
    let signature = memory.read_word(info)?;
    if signature.wrapping_add(0x978c92ad) > 1 {
        return Ok(());
    }
    // Native valid allocation includes the 0xC60-byte buffer at +0x350.
    validate(info, 0xfb0)?;
    memory.write_word(info, signature.wrapping_add(10))?;
    for address in [0x040000c4, 0x040000d0] {
        if memory.read_word(address)? & 0x02000000 != 0 {
            memory.write_word(address, 0x84400004)?;
        }
    }
    memory.write_halfword(0x040000c6, 0x0400)?;
    memory.write_halfword(0x040000d2, 0x0400)?;
    bios.zero_fill(memory, info + 0x350, 0x05000318)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    const INFO: u32 = 0x02002000;
    struct Memory {
        info: Vec<u8>,
        io: Vec<u8>,
        pointer: u32,
        writes: Vec<Value>,
    }
    impl Memory {
        fn write(&mut self, address: u32, bytes: &[u8]) -> Result<(), SoundDmaError> {
            let range = if let Some(i) = address.checked_sub(INFO).filter(|i| *i < 0x1000) {
                self.info.get_mut(i as usize..i as usize + bytes.len())
            } else {
                address
                    .checked_sub(0x04000000)
                    .and_then(|i| self.io.get_mut(i as usize..i as usize + bytes.len()))
            };
            range
                .ok_or(SoundDmaError::Memory {
                    address,
                    width: bytes.len() as u8,
                    write: true,
                })?
                .copy_from_slice(bytes);
            let value = bytes
                .iter()
                .enumerate()
                .fold(0u32, |v, (i, b)| v | (u32::from(*b) << (8 * i)));
            self.writes.push(json!([address, bytes.len(), value]));
            Ok(())
        }
    }
    impl SoundDmaMemory for Memory {
        fn read_word(&self, address: u32) -> Result<u32, SoundDmaError> {
            if address == INFO_SLOT {
                return Ok(self.pointer);
            }
            let bytes = if let Some(i) = address.checked_sub(INFO).filter(|i| *i < 0x1000) {
                self.info.get(i as usize..i as usize + 4)
            } else {
                address
                    .checked_sub(0x04000000)
                    .and_then(|i| self.io.get(i as usize..i as usize + 4))
            };
            Ok(u32::from_le_bytes(
                bytes
                    .ok_or(SoundDmaError::Memory {
                        address,
                        width: 4,
                        write: false,
                    })?
                    .try_into()
                    .unwrap(),
            ))
        }
        fn write_word(&mut self, a: u32, v: u32) -> Result<(), SoundDmaError> {
            self.write(a, &v.to_le_bytes())
        }
        fn write_halfword(&mut self, a: u32, v: u16) -> Result<(), SoundDmaError> {
            self.write(a, &v.to_le_bytes())
        }
        fn write_byte(&mut self, a: u32, v: u8) -> Result<(), SoundDmaError> {
            self.write(a, &[v])
        }
    }
    struct ControlledCpuSet;
    impl CpuSet for ControlledCpuSet {
        fn zero_fill(
            &mut self,
            m: &mut dyn SoundDmaMemory,
            destination: u32,
            control: u32,
        ) -> Result<(), SoundDmaError> {
            assert_eq!(control, 0x05000318);
            for i in 0..0x318 {
                m.write_word(destination + i * 4, 0)?;
            }
            Ok(())
        }
    }
    fn memory(seed: u8, signature: u32, dma1: u32, dma2: u32) -> Memory {
        let mut m = Memory {
            info: (0..0x1000)
                .map(|i| seed.wrapping_add((i as u8).wrapping_mul(37)))
                .collect(),
            io: vec![0; 0x1000],
            pointer: INFO,
            writes: Vec::new(),
        };
        m.info[..4].copy_from_slice(&signature.to_le_bytes());
        m.io[0xc4..0xc8].copy_from_slice(&dma1.to_le_bytes());
        m.io[0xd0..0xd4].copy_from_slice(&dma2.to_le_bytes());
        m
    }
    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn original_sound_dma_transactions_and_controlled_fill_match() {
        let f: Value = serde_json::from_str(vectors!("sound_dma-vectors.json")).unwrap();
        for c in f["cases"].as_array().unwrap() {
            let mut m = memory(
                c["seed"].as_u64().unwrap() as u8,
                c["signature"].as_u64().unwrap() as u32,
                c["dma1"].as_u64().unwrap() as u32,
                c["dma2"].as_u64().unwrap() as u32,
            );
            if c["entry"] == 0x5f040 {
                gate_5f040(&mut m, &mut ControlledCpuSet).unwrap();
            } else {
                gate_5f0bc(&mut m).unwrap();
            }
            assert_eq!(crate::sha256(&m.info), c["infoSha256"].as_str().unwrap());
            assert_eq!(crate::sha256(&m.io), c["ioSha256"].as_str().unwrap());
            // The controlled CpuSet's writes are a modeled BIOS seam; compare
            // only native instructions' RAM/MMIO writes against Unicorn hooks.
            m.writes.retain(|w| {
                w[0].as_u64().unwrap() < u64::from(INFO + 0x350)
                    || w[0].as_u64().unwrap() >= u64::from(INFO + 0xfb0)
            });
            assert_eq!(json!(m.writes), c["writes"]);
        }
    }
    #[test]
    fn invalid_info_and_missing_bios_are_explicit() {
        let mut m = memory(1, MAGIC, 0, 0);
        m.pointer = 0x03007ffc;
        assert_eq!(
            gate_5f0bc(&mut m),
            Err(SoundDmaError::InvalidInfo(0x03007ffc))
        );
        m.pointer = INFO;
        assert_eq!(
            gate_5f040(&mut m, &mut RejectCpuSet),
            Err(SoundDmaError::UnportedCpuSet {
                destination: INFO + 0x350,
                control: 0x05000318
            })
        );
        assert_eq!(m.read_word(INFO).unwrap(), MAGIC + 10);
    }
}
