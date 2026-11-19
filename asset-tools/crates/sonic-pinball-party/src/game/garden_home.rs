//! Embedded Garden boot scheduling, home resources and task installation.
//! The native save remains opaque; hardware, audio and unknown tasks are strict.
use super::{
    frame::{FrameError, FrameMemory, NativeCall},
    garden_graphics::{self, Error, Resources},
};
pub trait Calls {
    fn invoke(
        &mut self,
        m: &mut dyn FrameMemory,
        offset: u32,
        args: &[u32],
    ) -> Result<u32, FrameError>;
}
fn word(m: &mut dyn FrameMemory, a: u32, v: u32) -> Result<(), FrameError> {
    m.write_bytes(a, &v.to_le_bytes())
}
fn fail(off: u32, e: impl ToString) -> FrameError {
    FrameError::Dependency {
        call: NativeCall {
            entry_offset: garden_graphics::diagnostic_address(off),
            argument0: None,
        },
        reason: e.to_string(),
    }
}
fn copy(m: &mut dyn FrameMemory, source: u32, dest: u32, words: u32) -> Result<(), FrameError> {
    for i in 0..words {
        let v = m.read_u32(source + i * 4)?;
        word(m, dest + i * 4, v)?;
    }
    Ok(())
}
fn resource(
    m: &mut dyn FrameMemory,
    r: &dyn Resources,
    source: u32,
    dest: u32,
    n: usize,
) -> Result<(), Error> {
    let bytes = r.read_bytes(source, n)?;
    if bytes.len() != n {
        return Err(Error::ResourceLength {
            address: source,
            expected: n,
            actual: bytes.len(),
        });
    }
    m.write_bytes(dest, &bytes)?;
    Ok(())
}
fn lz(
    m: &mut dyn FrameMemory,
    r: &mut dyn Resources,
    source: u32,
    dest: u32,
    expected: usize,
) -> Result<(), Error> {
    let bytes = r.decode_lz77(source)?;
    if bytes.len() != expected {
        return Err(Error::ResourceLength {
            address: source,
            expected,
            actual: bytes.len(),
        });
    }
    m.write_bytes(dest, &bytes)?;
    Ok(())
}
pub fn load_graphics(m: &mut dyn FrameMemory, r: &mut dyn Resources) -> Result<(), Error> {
    garden_graphics::apply_register_script(m, 0x02020608, 13)?;
    garden_graphics::apply_register_script(m, 0x0202063c, 3)?;
    m.write_bytes(0x06000000, &[0; 64])?;
    resource(m, r, 0x02022b68, 0x05000000, 512)?;
    for (source, dest, n) in [
        (0x02022d68, 0x06001c00, 9696),
        (0x020223b0, 0x060041e0, 5120),
        (0x02021858, 0x060055e0, 10784),
        (0x02020934, 0x06008000, 7680),
    ] {
        lz(m, r, source, dest, n)?;
    }
    let mut cursor = 0x06000f80;
    for row in 0..10 {
        copy(m, 0x06002980, 0x06000f80 + row * 320, 80)?;
        for _ in 0..10 {
            for i in 0..8 {
                let merged = m.read_u32(cursor)? | m.read_u32(0x06002ac0 + row * 32 + i * 4)?;
                word(m, cursor, merged)?;
                cursor += 4;
            }
        }
    }
    resource(m, r, 0x02023e50, 0x05000200, 512)?;
    lz(m, r, 0x02024390, 0x06012d80, 21088)?;
    lz(m, r, 0x02026020, 0x06011580, 6144)
}
pub fn initialize(
    m: &mut dyn FrameMemory,
    r: &mut dyn Resources,
    c: &mut dyn Calls,
) -> Result<(), Error> {
    load_graphics(m, r)?;
    c.invoke(m, 0x17a4, &[])?;
    m.write_bytes(0x0600e000, &[0; 0x2000])?;
    lz(m, r, 0x02023d84, 0x0203f800, 720)?;
    let map = if m.read_u8(0x03004260)? == 0 {
        0x0203f800
    } else {
        0x0203f968
    };
    garden_graphics::stamp_map(m, map, 0x0600f000, 9, 20, 0xe0)?;
    garden_graphics::stamp_map(m, 0x02021330, 0x0600f800, 22, 20, 0x200)?;
    for offset in [6, 2, 4, 0] {
        m.write_u8(0x030015a8 + offset, 0)?;
    }
    c.invoke(m, 0xc90, &[])?;
    allocate(m, c, 0x02003425, 0x03005344, 1)?;
    m.write_u8(0x0300159b, 0)?;
    for i in 0..16 {
        if m.read_u8(0x03004aa0 + 0x37 + i * 2)? != 0 {
            let object = allocate(m, c, 0x020035cd, 0x03005344, 1)?;
            m.write_u8(object + 0x1a, i as u8)?;
            let n = m.read_u8(0x0300159b)?.wrapping_add(1);
            m.write_u8(0x0300159b, n)?;
        }
    }
    m.write_u8(0x0300152f, m.read_u8(0x03004af8)?)?;
    let icon = allocate(m, c, 0x020074c1, 0x03005320, 0)?;
    m.write_u8(icon + 0x1a, 1)?;
    let icon = allocate(m, c, 0x020074c1, 0x03005320, 0)?;
    m.write_u8(icon + 0x1a, 0)?;
    m.write_u8(0x0300152d, 0)?;
    let chao = allocate(m, c, 0x020041e1, 0x03005320, 0)?;
    word(m, 0x03001510, chao)?;
    allocate(m, c, 0x020089b1, 0x03005320, 0)?;
    for i in 0..3 {
        if m.read_u8(0x03004aa0 + 0x31 + i * 2)? != 0 {
            let object = allocate(m, c, 0x020083e5, 0x03005320, 0)?;
            m.write_u8(object + 0x1a, i as u8)?;
        }
    }
    m.write_u8(0x0300159a, 0)?;
    for i in 0..8 {
        let item = m.read_u8(0x03004afa + i)?;
        if item != 255 {
            let object = allocate(m, c, 0x02008325, 0x03005320, 0)?;
            m.write_u8(object + 0x19, i as u8)?;
            m.write_u8(object + 0x1a, item)?;
            let n = m.read_u8(0x0300159a)?.wrapping_add(1);
            m.write_u8(0x0300159a, n)?;
        }
    }
    m.write_u8(0x03001530, 255)?;
    let special = m.read_u8(0x03004af9)?;
    if special != 255 {
        let object = allocate(m, c, 0x02003e05, 0x03005320, 0)?;
        m.write_u8(object + 0x1a, special)?;
    }
    allocate(m, c, 0x02007501, 0x03005320, 0)?;
    let cursor = allocate(m, c, 0x02008a05, 0x03005320, 0)?;
    word(m, 0x0300150c, cursor)?;
    allocate(m, c, 0x0200218d, 0x03005320, 0)?;
    allocate(m, c, 0x020089d5, 0x03005320, 0)?;
    let fade = allocate(m, c, 0x020088b1, 0x03005320, 0)?;
    m.write_u8(fade + 0x1a, 16)?;
    m.write_u8(0x03001509, 0)?;
    m.write_u16(0x0300152a, 8000)?;
    let value = m.read_u16(0x0300152a)?;
    c.invoke(m, 0x167c, &[0, u32::from(value)])?;
    c.invoke(m, 0x1066c, &[1])?;
    m.write_u8(0x0300425f, 1)?;
    word(m, 0x03004250, 0x02001d69)?;
    m.write_u8(0x03001501, 0)?;
    m.write_u8(0x03001500, 0)?;
    Ok(())
}
fn allocate(
    m: &mut dyn FrameMemory,
    c: &mut dyn Calls,
    entry: u32,
    parent: u32,
    before: u32,
) -> Result<u32, FrameError> {
    c.invoke(m, 0xd04, &[entry, parent, before])
}
pub fn dispatch(
    m: &mut dyn FrameMemory,
    r: &mut dyn Resources,
    c: &mut dyn Calls,
    off: u32,
    args: &[u32],
) -> Result<u32, FrameError> {
    match (off, args) {
        (0x18c8, []) => initialize(m, r, c).map_err(|e| garden_graphics::frame_error(off, e))?,
        (0x1c60, []) => load_graphics(m, r).map_err(|e| garden_graphics::frame_error(off, e))?,
        (0xc4, []) => {
            let language = m.read_u32(0x0200000c)?;
            m.write_u8(0x03004260, if language <= 4 { language as u8 } else { 0 })?;
            let seed = m.read_u32(0x02000010)?;
            word(m, 0x030013a8, seed)?;
            for entry in [0x19c, 0x248, 0x480, 0xc6c, 0x18a0, 0xbe24, 0xd27c] {
                c.invoke(m, entry, &[])?;
            }
            m.write_u8(0x0300425f, 0)?;
            m.write_u16(0x0300425c, 0)?;
            word(m, 0x03004258, 0)?;
            m.write_u8(0x0300425e, 0)?;
        }
        (0x19c, []) => {
            // Preserve native copy/install transactions. No copied ARM IRQ
            // instructions are executed by the Rust runtime.
            copy(m, 0x0200005c, 0x03000000, 32)?;
            word(m, 0x03007ffc, 0x03000000)?;
            for a in [0x03004a8c, 0x03004a88, 0x03004a84, 0x03004a80] {
                word(m, a, 0x02000245)?;
            }
            m.write_u16(0x04000200, 1)?;
            if m.read_u8(0x080000b2)? == 0x96 {
                let ie = m.read_u16(0x04000200)?;
                m.write_u16(0x04000200, ie | 0x2000)?;
                m.write_u16(0x03004a90, 0x2000)?;
            } else {
                m.write_u16(0x03004a90, 0)?;
            }
            m.write_u16(0x04000004, 8)?;
            m.write_u16(0x04000208, 1)?;
        }
        (0x228, &[handler]) => word(
            m,
            0x03004a84,
            if handler == 0 { 0x02000245 } else { handler },
        )?,
        (0x18a0, []) => {
            c.invoke(m, 0x105e8, &[])?;
            c.invoke(m, 0x228, &[0x020018b5])?;
        }
        (0x18b4, []) => {
            c.invoke(m, 0xff8c, &[])?;
            m.write_u8(0x0300425e, 0)?;
        }
        (0xbe24, []) => {
            m.write_u8(0x030019d4, 0)?;
            word(m, 0x030019d8, 0)?;
        }
        (0xd27c, []) => {
            m.write_u8(0x03001ab2, 0)?;
            word(m, 0x03001ab4, 0)?;
        }
        _ => return Err(fail(off, "invalid home entry arguments")),
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{
        garden_frame,
        garden_payload::{BoardCalls, NativeBoardCalls},
        memory::{MemoryImage, Region},
    };
    use serde_json::Value;
    fn pattern(seed: u8, n: usize) -> Vec<u8> {
        (0..n)
            .map(|i| seed.wrapping_add((i as u8).wrapping_mul(37)))
            .collect()
    }
    fn reg(base: u32, bytes: Vec<u8>) -> Region {
        Region {
            base,
            bytes,
            writable: true,
        }
    }
    fn unhex(s: &str) -> Vec<u8> {
        s.as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect()
    }
    struct Probe<'a> {
        resources: garden_graphics::GardenPayload<'a>,
        calls: Vec<(u32, Vec<u32>)>,
    }
    impl Calls for Probe<'_> {
        fn invoke(
            &mut self,
            m: &mut dyn FrameMemory,
            off: u32,
            args: &[u32],
        ) -> Result<u32, FrameError> {
            self.calls.push((off, args.to_vec()));
            match off {
                0x248 | 0x480 | 0x105e8 | 0x1066c => Ok(0),
                0xc6c => {
                    garden_frame::initialize_input(m)?;
                    Ok(0)
                }
                0x17a4 => {
                    m.write_u8(0x030014f8, 0)?;
                    Ok(0)
                }
                0xc90 | 0xd04 | 0x167c => NativeBoardCalls
                    .invoke(0x02000000 + off, args, m)
                    .map_err(|e| fail(off, e)),
                _ => {
                    let mut r = self.resources.clone();
                    dispatch(m, &mut r, self, off, args)
                }
            }
        }
    }
    #[test]
    #[ignore = "Requires local SPP_ROM; authentic Garden art and IRQ-copy data remain local"]
    fn home_and_boot_native_transactions_match_all_workspaces_and_allocated_tasks() {
        use std::io::Read;
        let mut rom = vec![];
        std::fs::File::open(std::env::var_os("SPP_ROM").expect("setSPP_ROM"))
            .unwrap()
            .take(0x800001)
            .read_to_end(&mut rom)
            .unwrap();
        let payload = crate::catalog::extract_garden(&rom).unwrap().bytes;
        let f: Value =
            serde_json::from_str(include_str!("../../tests/garden-home-vectors.json")).unwrap();
        for c in f["cases"].as_array().unwrap() {
            let seed = c["seed"].as_u64().unwrap() as u8;
            let mut ew = vec![0; 0x40000];
            ew[..payload.len()].copy_from_slice(&payload);
            ew[12..16].copy_from_slice(&(c["language"].as_u64().unwrap() as u32).to_le_bytes());
            ew[16..20].copy_from_slice(&(c["rng"].as_u64().unwrap() as u32).to_le_bytes());
            let mut m = MemoryImage::new(
                &rom,
                vec![
                    reg(0x02000000, ew),
                    reg(0x03000000, pattern(seed, 0x8000)),
                    reg(0x04000000, pattern(seed.wrapping_add(3), 0x10000)),
                    reg(0x05000000, pattern(seed.wrapping_add(7), 0x1000)),
                    reg(0x06000000, pattern(seed.wrapping_add(19), 0x20000)),
                ],
            )
            .unwrap();
            for p in c["patches"].as_array().unwrap() {
                m.write_bytes(
                    p[0].as_u64().unwrap() as u32,
                    &unhex(p[1].as_str().unwrap()),
                )
                .unwrap();
            }
            let mut r = garden_graphics::GardenPayload::new(&payload).unwrap();
            let mut calls = Probe {
                resources: r.clone(),
                calls: vec![],
            };
            dispatch(
                &mut m,
                &mut r,
                &mut calls,
                c["entry"].as_u64().unwrap() as u32,
                &[],
            )
            .unwrap();
            for (base, n, key) in [
                (0x03000000, 0x7000, "iwramSha256"),
                (0x02000000, 0x40000, "ewramSha256"),
                (0x04000000, 0x10000, "mmioSha256"),
                (0x05000000, 0x1000, "paletteSha256"),
                (0x06000000, 0x20000, "vramSha256"),
            ] {
                assert_eq!(
                    crate::sha256(m.bytes(base, n).unwrap()),
                    c[key].as_str().unwrap(),
                    "entry{} seed{seed} {key}",
                    c["entry"]
                );
            }
            assert_eq!(serde_json::to_value(calls.calls).unwrap(), c["calls"]);
        }
    }
}
