//! Source-order A3VE Garden input and one iteration of its native main loop.
//! Every callback, VBlank and audio call must execute a bound port or fail.
use super::{
    frame::{FrameError, FrameMemory, NativeCall},
    garden_graphics::Calls,
};
pub const GLOBALS: u32 = 0x03004250;
pub const INPUT: u32 = 0x03005310;

fn word(m: &mut dyn FrameMemory, a: u32, v: u32) -> Result<(), FrameError> {
    m.write_u16(a, v as u16)?;
    m.write_u16(a + 2, (v >> 16) as u16)
}

/// Payload0C6C: initialize input while retaining unrelated bytes.
pub fn initialize_input(m: &mut dyn FrameMemory) -> Result<(), FrameError> {
    for offset in [6, 4, 0] {
        m.write_u16(INPUT + offset, 0)?;
    }
    m.write_u16(INPUT + 2, 0xffff)?;
    m.write_u8(INPUT + 9, 15)?;
    m.write_u8(INPUT + 8, 15)?;
    m.write_u8(INPUT + 10, 7)
}

/// Payload0BD0. Retain the native unsigned numeric comparison for repeat
/// detection, byte timer underflow, upper KEYINPUT masking and reset seam.
pub fn poll_input(m: &mut dyn FrameMemory, c: &mut dyn Calls) -> Result<(), FrameError> {
    let previous_held = m.read_u16(INPUT)?;
    let previous_raw = m.read_u16(INPUT + 2)?;
    let raw = m.read_u16(0x04000130)? | 0xfc00;
    m.write_u16(INPUT + 2, raw)?;
    m.write_u16(INPUT, !raw)?;
    if m.read_u8(GLOBALS + 15)? & 0x40 == 0 && !raw & 15 == 15 {
        for offset in [0x107a0, 0x10660, 0x11b04, 0x10c70] {
            c.call(m, offset, &[])?;
        }
        m.write_u16(0x04000208, 0)?;
        m.write_u16(0x04000200, 0)?;
        c.call(m, 0x11b0c, &[0x20])?;
    }
    // Dependencies above may mutate the current input image. Native code
    // reloads held here but retains the two original previous halfwords.
    let held = m.read_u16(INPUT)?;
    m.write_u16(INPUT + 4, previous_raw & held)?;
    if held == 0 {
        return m.write_u16(INPUT + 6, 0);
    }
    if held > previous_held {
        m.write_u16(INPUT + 6, previous_held ^ held)?;
        return m.write_u8(INPUT + 8, m.read_u8(INPUT + 9)?);
    }
    let timer = m.read_u8(INPUT + 8)?.wrapping_sub(1);
    m.write_u8(INPUT + 8, timer)?;
    if timer != 0 {
        m.write_u16(INPUT + 6, 0)
    } else {
        m.write_u16(INPUT + 6, held)?;
        m.write_u8(INPUT + 8, m.read_u8(INPUT + 10)?)
    }
}

/// Payload0168 entry before the recurring loop. Full native boot0C4 is a
/// strict dependency; no reset state, save or initialized Chao is invented.
pub fn initialize(m: &mut dyn FrameMemory, c: &mut dyn Calls) -> Result<(), FrameError> {
    c.call(m, 0xc4, &[])?;
    word(m, GLOBALS, 0x020018c9)
}

/// One iteration of payload0178..0192. Read the callback after input, then
/// reread the counter after callback/audio mutations. This is scheduling,
/// not proof that the callback's whole subsystem has been ported.
pub fn tick(m: &mut dyn FrameMemory, c: &mut dyn Calls) -> Result<(), FrameError> {
    c.call(m, 0x11b04, &[])?;
    poll_input(m, c)?;
    let callback = m.read_u32(GLOBALS)?;
    let offset = callback
        .checked_sub(0x02000001)
        .filter(|o| o & 1 == 0 && (*o as usize) < crate::garden::PAYLOAD_LENGTH);
    let offset = offset.ok_or_else(|| FrameError::Dependency {
        call: NativeCall {
            entry_offset: callback,
            argument0: None,
        },
        reason: "unbound Garden Thumb callback".into(),
    })?;
    c.call(m, offset, &[])?;
    c.call(m, 0x10660, &[])?;
    let counter = m.read_u32(GLOBALS + 8)?.wrapping_add(1);
    word(m, GLOBALS + 8, counter)?;
    m.write_u8(GLOBALS + 14, 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::memory::{MemoryImage, Region};
    use serde_json::Value;
    fn region(base: u32, bytes: Vec<u8>) -> Region {
        Region {
            base,
            bytes,
            writable: true,
        }
    }
    fn values(v: &Value) -> Vec<u8> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_u64().unwrap() as u8)
            .collect()
    }
    struct Probe {
        seen: Vec<(u32, Vec<u32>)>,
        mutation: Option<[u32; 2]>,
        reject: Option<u32>,
    }
    impl Calls for Probe {
        fn call(
            &mut self,
            m: &mut dyn FrameMemory,
            off: u32,
            args: &[u32],
        ) -> Result<(), FrameError> {
            self.seen.push((off, args.to_vec()));
            if self.reject == Some(off) {
                return Err(FrameError::UnportedNativeCall(NativeCall {
                    entry_offset: 0x02000000 + off,
                    argument0: args.first().copied(),
                }));
            }
            if off == 0x1d68 {
                if let Some([counter, held]) = self.mutation {
                    word(m, GLOBALS + 8, counter)?;
                    m.write_u16(INPUT, held as u16)?;
                }
            }
            Ok(())
        }
    }
    #[test]
    fn native_garden_input_and_single_loop_match_source_mutations_and_call_order() {
        let f: Value =
            serde_json::from_str(include_str!("../../tests/garden-frame-vectors.json")).unwrap();
        for c in f["cases"].as_array().unwrap() {
            let mut m = MemoryImage::fixture(vec![
                region(INPUT, values(&c["input"])),
                region(GLOBALS, values(&c["globals"])),
                region(0x04000000, vec![0x5a; 0x400]),
            ]);
            m.write_u16(0x04000130, c["raw"].as_u64().unwrap() as u16)
                .unwrap();
            let mutation = c["mutation"]
                .as_array()
                .map(|a| [a[0].as_u64().unwrap() as u32, a[1].as_u64().unwrap() as u32]);
            let mut calls = Probe {
                seen: vec![],
                mutation,
                reject: None,
            };
            match c["routine"].as_str().unwrap() {
                "initializeInput" => initialize_input(&mut m).unwrap(),
                "poll" => poll_input(&mut m, &mut calls).unwrap(),
                "initialize" => initialize(&mut m, &mut calls).unwrap(),
                "tick" => tick(&mut m, &mut calls).unwrap(),
                _ => panic!("unknown oracle routine"),
            }
            assert_eq!(m.bytes(INPUT, 12).unwrap(), values(&c["inputFinal"]));
            assert_eq!(m.bytes(GLOBALS, 20).unwrap(), values(&c["globalsFinal"]));
            assert_eq!(
                crate::sha256(m.bytes(0x04000000, 0x400).unwrap()),
                c["mmioSha256"].as_str().unwrap()
            );
            assert_eq!(serde_json::to_value(calls.seen).unwrap(), c["calls"]);
        }
    }
    #[test]
    fn unported_callback_and_vblank_preserve_native_stop_positions() {
        let mut m = MemoryImage::fixture(vec![
            region(INPUT, vec![0; 12]),
            region(GLOBALS, vec![0; 20]),
            region(0x04000000, vec![0xff; 0x400]),
        ]);
        let mut c = Probe {
            seen: vec![],
            mutation: None,
            reject: Some(0x11b04),
        };
        assert!(tick(&mut m, &mut c).is_err());
        assert_eq!(m.read_u32(GLOBALS + 8).unwrap(), 0);
        assert_eq!(m.read_u16(INPUT).unwrap(), 0);
        c.reject = None;
        assert!(tick(&mut m, &mut c).is_err());
        assert_eq!(m.read_u32(GLOBALS + 8).unwrap(), 0);
        assert_eq!(m.read_u8(GLOBALS + 14).unwrap(), 0);
        assert_eq!(c.seen.last().unwrap().0, 0x11b04);
    }
}
