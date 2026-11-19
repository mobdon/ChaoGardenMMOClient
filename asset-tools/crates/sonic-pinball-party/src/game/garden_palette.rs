//! A3VE embedded-Garden display update at payload offset 0x1D68.
//! The two palette selectors are display indices, not inferred Chao type IDs.
//! Payload offsets are independent of the cartridge's engine call addresses.
use super::{
    frame::{FrameError, FrameMemory},
    garden_graphics::Calls,
};

pub const OFFSET: usize = 0x1d68;
pub const LENGTH: usize = 0x98;
pub const FLAGS: u32 = 0x030015a8;

fn copy_bank(m: &mut dyn FrameMemory, source: u32, destination: u32) -> Result<(), FrameError> {
    // Native SWI0B count 0x10, halfword copy. These addresses are all aligned;
    // no hardware wait, DMA side effect or BIOS cycle behavior is asserted.
    for i in 0..16 {
        let color = m.read_u16(source + i * 2)?;
        m.write_u16(destination + i * 2, color)?;
    }
    Ok(())
}

/// Preserve the native dependency order and reload flags after those calls.
/// Unsupported dependencies stop synchronously; dirty flags already changed
/// before a failed copy retain their source-ordered changes.
pub fn update(m: &mut dyn FrameMemory, calls: &mut dyn Calls) -> Result<(), FrameError> {
    for offset in [0x16d8, 0x12e0, 0x1420] {
        calls.call(m, offset, &[])?;
    }
    if m.read_u8(FLAGS)? != 0 {
        m.write_u8(FLAGS, 0)?;
        m.write_u8(FLAGS + 2, 1)?;
        let index = m.read_u8(FLAGS + 1)?;
        let source = if index <= 13 {
            0x02024050 + u32::from(index) * 32
        } else {
            0x030015b0
        };
        copy_bank(m, source, 0x05000200)?;
    }
    if m.read_u8(FLAGS + 4)? != 0 {
        m.write_u8(FLAGS + 4, 0)?;
        m.write_u8(FLAGS + 6, 1)?;
        // Unlike the first selector, native code has no <=13 branch here.
        // Retain its full byte index; bounded memory supplies the source span.
        let index = m.read_u8(FLAGS + 5)?;
        copy_bank(m, 0x02024210 + u32::from(index) * 32, 0x050003c0)?;
    }
    for offset in [0x590, 0x1374, 0xeac, 0x117c, 0x1088] {
        calls.call(m, offset, &[])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{
        frame::NativeCall,
        memory::{MemoryImage, Region},
    };
    use serde_json::Value;
    fn pattern(seed: u8, length: usize) -> Vec<u8> {
        (0..length)
            .map(|i| seed.wrapping_add((i as u8).wrapping_mul(37)))
            .collect()
    }
    fn region(base: u32, bytes: Vec<u8>) -> Region {
        Region {
            base,
            bytes,
            writable: true,
        }
    }
    struct Probe {
        calls: Vec<u32>,
        mutation: Option<[u8; 2]>,
        reject: Option<u32>,
    }
    impl Calls for Probe {
        fn call(
            &mut self,
            m: &mut dyn FrameMemory,
            offset: u32,
            args: &[u32],
        ) -> Result<(), FrameError> {
            assert!(args.is_empty());
            self.calls.push(offset);
            if self.reject == Some(offset) {
                return Err(FrameError::UnportedNativeCall(NativeCall {
                    entry_offset: 0x02000000 + offset,
                    argument0: None,
                }));
            }
            if offset == 0x1420 {
                if let Some([flag, index]) = self.mutation {
                    m.write_u8(FLAGS, flag)?;
                    m.write_u8(FLAGS + 1, index)?;
                }
            }
            Ok(())
        }
    }
    #[test]
    fn native_palette_selectors_match_all_source_ordered_memory_and_calls() {
        let f: Value =
            serde_json::from_str(include_str!("../../tests/garden-palette-vectors.json")).unwrap();
        for c in f["cases"].as_array().unwrap() {
            let seed = c["seed"].as_u64().unwrap() as u8;
            let mut m = MemoryImage::fixture(vec![
                region(0x02024050, pattern(seed.wrapping_add(7), 0x21c0)),
                region(0x03000000, pattern(seed, 0x8000)),
                region(0x05000000, pattern(seed.wrapping_add(19), 0x400)),
            ]);
            for (i, v) in c["flags"].as_array().unwrap().iter().enumerate() {
                m.write_u8(FLAGS + i as u32, v.as_u64().unwrap() as u8)
                    .unwrap();
            }
            let mutation = c["mutation"]
                .as_array()
                .map(|a| [a[0].as_u64().unwrap() as u8, a[1].as_u64().unwrap() as u8]);
            let mut calls = Probe {
                calls: vec![],
                mutation,
                reject: None,
            };
            update(&mut m, &mut calls).unwrap();
            assert_eq!(serde_json::to_value(calls.calls).unwrap(), c["calls"]);
            assert_eq!(
                crate::sha256(m.bytes(0x03000000, 0x7800).unwrap()),
                c["iwramSha256"].as_str().unwrap()
            );
            assert_eq!(
                crate::sha256(m.bytes(0x05000000, 0x400).unwrap()),
                c["paletteSha256"].as_str().unwrap()
            );
        }
    }
    #[test]
    fn unsupported_dependencies_and_unbound_palette_stop_at_native_position() {
        let mut m = MemoryImage::fixture(vec![region(FLAGS, vec![1, 14, 9, 0, 1, 255, 9, 0])]);
        let mut calls = Probe {
            calls: vec![],
            mutation: None,
            reject: Some(0x16d8),
        };
        assert!(update(&mut m, &mut calls).is_err());
        assert_eq!(m.read_u8(FLAGS).unwrap(), 1);
        assert_eq!(m.read_u8(FLAGS + 2).unwrap(), 9);
        calls.reject = None;
        assert_eq!(
            update(&mut m, &mut calls),
            Err(FrameError::MissingRead {
                address: 0x030015b0,
                width: 2
            })
        );
        assert_eq!(m.read_u8(FLAGS).unwrap(), 0);
        assert_eq!(m.read_u8(FLAGS + 2).unwrap(), 1);
        assert_eq!(m.read_u8(FLAGS + 4).unwrap(), 1);
    }
}
