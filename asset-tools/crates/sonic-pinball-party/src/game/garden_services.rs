//! Native Garden task walking, background queue and sprite presentation.
//! Every layout here comes from this payload's own code, not the ROM engine.
use super::{
    frame::{FrameError, FrameMemory, NativeCall},
    garden_graphics,
};
pub trait Calls {
    fn invoke(
        &mut self,
        m: &mut dyn FrameMemory,
        offset: u32,
        args: &[u32],
    ) -> Result<u32, FrameError>;
}
fn fail(offset: u32, reason: impl ToString) -> FrameError {
    FrameError::Dependency {
        call: NativeCall {
            entry_offset: garden_graphics::diagnostic_address(offset),
            argument0: None,
        },
        reason: reason.to_string(),
    }
}
fn task(id: u8) -> Result<u32, FrameError> {
    if id > 71 {
        return Err(fail(0xeac, "invalid native task ID"));
    }
    Ok(0x03005320 + u32::from(id) * 0x24)
}
fn byte(m: &mut dyn FrameMemory, a: u32, v: u32) -> Result<(), FrameError> {
    m.write_u8(a, v as u8)
}

fn backgrounds(m: &mut dyn FrameMemory) -> Result<(), FrameError> {
    let count = m.read_u8(0x030014f8)?;
    if count > 24 {
        return Err(fail(0x1420, "background queue exceeds24 records"));
    }
    while m.read_u8(0x030014f8)? != 0 {
        let count = m.read_u8(0x030014f8)?.wrapping_sub(1);
        m.write_u8(0x030014f8, count)?;
        let a = 0x030013d8 + u32::from(count) * 12;
        let source = m.read_u32(a)?;
        let dest = m.read_u32(a + 4)?;
        let width = m.read_u8(a + 8)?;
        let height = m.read_u8(a + 9)?;
        let value = m.read_u16(a + 10)?;
        if source == 0 && width == 255 {
            // Native CpuFastSet count200 words fills each word from the
            // zero-extended halfword placed in the source stack word.
            let mut bytes = [0; 0x800];
            for word in bytes.as_chunks_mut::<4>().0 {
                word.copy_from_slice(&u32::from(value).to_le_bytes());
            }
            m.write_bytes(dest, &bytes)?;
        } else if source == 0 {
            for row in 0..u32::from(height) {
                for col in 0..u32::from(width) {
                    let a = dest
                        .checked_add(row * 64 + col * 2)
                        .ok_or_else(|| fail(0x1420, "destination overflow"))?;
                    m.write_u16(a, value)?;
                }
            }
        } else {
            garden_graphics::stamp_map(
                m,
                source,
                dest,
                u32::from(width),
                u32::from(height),
                u32::from(value),
            )
            .map_err(|e| garden_graphics::frame_error(0x1420, e))?;
        }
    }
    Ok(())
}
fn walk(m: &mut dyn FrameMemory, c: &mut dyn Calls) -> Result<(), FrameError> {
    let mut id = 0;
    for _ in 0..4096 {
        let object = task(id)?;
        let callback = m.read_u32(object)?;
        let off = callback
            .checked_sub(0x02000001)
            .filter(|o| o & 1 == 0 && (*o as usize) < crate::garden::PAYLOAD_LENGTH)
            .ok_or_else(|| fail(0xeac, "invalid Garden task callback"))?;
        c.invoke(m, off, &[object])?;
        // Task callbacks may remove/relink the current node or create new
        // nodes. Read its next link after the synchronous callback returns.
        id = m.read_u8(object + 6)?;
        if id == 0 {
            return Ok(());
        }
    }
    Err(fail(0xeac, "task dispatch budget exceeded"))
}
fn unlink(m: &mut dyn FrameMemory, object: u32) -> Result<u32, FrameError> {
    if !(0x03000000..=0x03007fdc).contains(&object) {
        return Err(fail(0xed4, "task record outside bounded IWRAM"));
    }
    let previous = m.read_u8(object + 5)?;
    let next = m.read_u8(object + 6)?;
    m.write_u8(task(previous)? + 6, next)?;
    let next = m.read_u8(object + 6)?;
    let previous = m.read_u8(object + 5)?;
    m.write_u8(task(next)? + 5, previous)?;
    let tail = m.read_u8(0x03000608)?;
    if tail > 69 {
        return Err(fail(0xed4, "invalid task free-list tail"));
    }
    m.write_u8(0x03000608, tail.wrapping_add(1))?;
    m.write_u8(0x030005c0 + u32::from(tail), m.read_u8(object + 4)?)?;
    if m.read_u8(0x03000608)? > 69 {
        m.write_u8(0x03000608, 0)?;
    }
    let result = u32::from(m.read_u8(0x03000606)?).wrapping_sub(1);
    byte(m, 0x03000606, result)?;
    Ok(0)
}
fn begin_lists(m: &mut dyn FrameMemory) -> Result<(), FrameError> {
    for layer in 0..4 {
        m.write_u8(0x03001080 + layer, layer as u8)?;
        m.write_u8(0x03001088 + layer * 2, 0)?;
    }
    m.write_u8(0x0300111c, 4)?;
    for index in 4..74 {
        m.write_u8(0x03001088 + index * 2, 0)?;
    }
    Ok(())
}
fn sorted(m: &mut dyn FrameMemory) -> Result<(), FrameError> {
    let count = m.read_u8(0x0300127a)?;
    m.write_u8(0x03006190, count)?;
    if count == 0 {
        return Ok(());
    }
    if count > 70 {
        return Err(fail(0x117c, "draw list exceeds task pool"));
    }
    for i in 0..u32::from(count) {
        let bucket = m.read_u8(0x03001148 + i * 2)?;
        let a = 0x030011d8 + u32::from(bucket);
        m.write_u8(a, m.read_u8(a)?.wrapping_add(1))?;
    }
    for i in 0..=160 {
        let a = 0x030011d8 + i;
        let sum = m.read_u8(a)?.wrapping_add(m.read_u8(a + 1)?);
        m.write_u8(a + 1, sum)?;
    }
    for i in (0..u32::from(count)).rev() {
        let entry = 0x03001148 + i * 2;
        let bucket = m.read_u8(entry)?;
        let a = 0x030011d8 + u32::from(bucket);
        let index = m.read_u8(a)?.wrapping_sub(1);
        m.write_u8(a, index)?;
        let id = m.read_u8(entry + 1)?;
        m.write_u8(0x03005d40 + u32::from(index), id)?;
    }
    for i in (0..u32::from(count)).rev() {
        let id = m.read_u8(0x03005d40 + i)?;
        let tail = m.read_u8(0x03001083)?;
        let free = m.read_u8(0x0300111c)?;
        m.write_u8(0x03001088 + u32::from(tail) * 2, free)?;
        let free = m.read_u8(0x0300111c)?;
        m.write_u8(0x03001089 + u32::from(free) * 2, id)?;
        let free = m.read_u8(0x0300111c)?;
        m.write_u8(0x0300111c, free.wrapping_add(1))?;
        m.write_u8(0x03001083, free)?;
    }
    Ok(())
}
fn compose(m: &mut dyn FrameMemory) -> Result<(), FrameError> {
    let mut output = 0x03005d90;
    let mut left = 128u16;
    for layer in 0..4 {
        let mut link = m.read_u8(0x03001088 + layer * 2)?;
        for _ in 0..128 {
            if link == 0 {
                break;
            }
            let cell = 0x03001088 + u32::from(link) * 2;
            let object = task(m.read_u8(cell + 1)?)?;
            let mut descriptor = m.read_u32(object + 0x10)?;
            loop {
                let y = (m.read_u8(descriptor + 6)? as i8 as i32)
                    .wrapping_add(i32::from(m.read_u16(object + 0xe)?))
                    as u32
                    & 255;
                let a0 =
                    (u32::from(m.read_u8(descriptor + 2)? | m.read_u8(object + 0x16)?) << 8) | y;
                m.write_u16(output, a0 as u16)?;
                let x = (m.read_u8(descriptor + 5)? as i8 as i32)
                    .wrapping_add(i32::from(m.read_u16(object + 0xa)?))
                    as u32
                    & 511;
                let a1 =
                    (u32::from(m.read_u8(descriptor + 3)? | m.read_u8(object + 0x17)?) << 8) | x;
                m.write_u16(output + 2, a1 as u16)?;
                let attr = (u32::from(m.read_u8(descriptor + 4)? | m.read_u8(object + 0x18)?) << 8)
                    | (u32::from(m.read_u16(object + 0x14)?) + u32::from(m.read_u16(descriptor)?));
                m.write_u16(output + 4, attr as u16)?;
                left -= 1;
                if left == 0 {
                    return Ok(());
                }
                output += 8;
                if m.read_u8(descriptor + 7)? == 0 {
                    break;
                }
                descriptor = descriptor
                    .checked_add(8)
                    .ok_or_else(|| fail(0x1088, "descriptor overflow"))?;
            }
            link = m.read_u8(cell)?;
        }
        if link != 0 {
            return Err(fail(0x1088, "sprite list cycle"));
        }
    }
    for _ in 0..left {
        m.write_u16(output, 0x200)?;
        output += 8;
    }
    Ok(())
}
fn snapshot(m: &mut dyn FrameMemory, restore: bool) -> Result<(), FrameError> {
    for i in 0..128 {
        if restore {
            m.write_u16(0x03005d90 + i * 8, 0x200)?;
            let value = m.read_u16(0x03001280 + i * 2)?;
            m.write_u16(0x03005d96 + i * 8, value)?;
        } else {
            let value = m.read_u16(0x03005d96 + i * 8)?;
            m.write_u16(0x03001280 + i * 2, value)?;
        }
    }
    let (src, dst) = if restore {
        (0x03001380, 0x03001120)
    } else {
        (0x03001120, 0x03001380)
    };
    for i in 0..32 {
        let value = m.read_u8(src + i)?;
        m.write_u8(dst + i, value)?;
    }
    for (live, backup) in [
        (0x03001140, 0x030013a0),
        (0x03001141, 0x030013a1),
        (0x03001142, 0x030013a2),
    ] {
        let (src, dst) = if restore {
            (backup, live)
        } else {
            (live, backup)
        };
        let value = m.read_u8(src)?;
        m.write_u8(dst, value)?;
    }
    Ok(())
}
fn task_snapshot(m: &mut dyn FrameMemory, restore: bool) -> Result<(), FrameError> {
    snapshot(m, restore)?;
    let (src, dst) = if restore {
        (0x03000610, 0x03005320)
    } else {
        (0x03005320, 0x03000610)
    };
    for i in 0..648 {
        let value = m.read_u32(src + i * 4)?;
        m.write_bytes(dst + i * 4, &value.to_le_bytes())?;
    }
    let (src, dst) = if restore {
        (0x03001030, 0x030005c0)
    } else {
        (0x030005c0, 0x03001030)
    };
    for i in 0..70 {
        let value = m.read_u8(src + i)?;
        m.write_u8(dst + i, value)?;
    }
    for (live, backup) in [
        (0x03000606, 0x03001076),
        (0x03000607, 0x03001077),
        (0x03000608, 0x03001078),
    ] {
        let (src, dst) = if restore {
            (backup, live)
        } else {
            (live, backup)
        };
        let value = m.read_u8(src)?;
        m.write_u8(dst, value)?;
    }
    if restore {
        m.write_u8(0x03006190, 0)?;
    }
    Ok(())
}
/// Exact payload entries; return words are meaningful only where the original
/// caller consumes them. All task callbacks stay synchronous strict calls.
pub fn dispatch(
    m: &mut dyn FrameMemory,
    c: &mut dyn Calls,
    off: u32,
    args: &[u32],
) -> Result<u32, FrameError> {
    match (off, args) {
        (0xdb0, []) => task_snapshot(m, false)?,
        (0xe28, []) => task_snapshot(m, true)?,
        (0xf34, []) => snapshot(m, false)?,
        (0xfd4, []) => snapshot(m, true)?,
        (0x151c, []) => return m.read_u32(0x030013a8),
        (0x1528, &[seed]) => {
            m.write_bytes(0x030013a8, &seed.to_le_bytes())?;
            return Ok(seed);
        }
        (0x1534, []) => {
            let seed = m
                .read_u32(0x030013a8)?
                .wrapping_mul(0x41c64e6d)
                .wrapping_add(12345);
            m.write_bytes(0x030013a8, &seed.to_le_bytes())?;
            return Ok((seed >> 16) & 0x7fff);
        }
        (0x17a4, []) => m.write_u8(0x030014f8, 0)?,
        (0x17b0, &[source, dest, width, height, delta]) => {
            let count = m.read_u8(0x030014f8)?;
            if count > 23 {
                return Ok(0);
            }
            let a = 0x030013d8 + u32::from(count) * 12;
            m.write_bytes(a, &source.to_le_bytes())?;
            m.write_bytes(a + 4, &dest.to_le_bytes())?;
            m.write_u8(a + 8, width as u8)?;
            m.write_u8(a + 9, height as u8)?;
            m.write_u8(0x030014f8, count + 1)?;
            m.write_u16(a + 10, delta as u16)?;
            return Ok(1);
        }
        (0x1254, &[id, layer]) => {
            let layer = u32::from(layer as u8);
            if layer > 3 {
                return Err(fail(off, "invalid sprite layer"));
            }
            let tail = m.read_u8(0x03001080 + layer)?;
            let free = m.read_u8(0x0300111c)?;
            if free > 73 {
                return Err(fail(off, "sprite link pool exhausted"));
            }
            m.write_u8(0x03001088 + u32::from(tail) * 2, free)?;
            m.write_u8(0x03001089 + u32::from(free) * 2, id as u8)?;
            m.write_u8(0x0300111c, free.wrapping_add(1))?;
            m.write_u8(0x03001080 + layer, free)?;
            // Native pop{r0};bx r0 uses R0 as its return PC. No caller
            // consumes a result for this void list-submission routine.
            return Ok(0);
        }
        (0x1420, []) => backgrounds(m)?,
        (0xeac, []) => walk(m, c)?,
        (0xed4, &[object]) => return unlink(m, object),
        (0xf30, &[object]) => return Ok(object),
        (0x13dc, [] | [_]) => begin_lists(m)?,
        (0x1374, []) => {
            for i in (0..=161).rev() {
                m.write_u8(0x030011d8 + i, 0)?;
            }
            m.write_u8(0x0300127a, 0)?;
        }
        (0x1398, &[id, y]) => {
            let y = y as u8;
            let y = if y > 208 { 0 } else { y.min(161) };
            let count = m.read_u8(0x0300127a)?;
            if count >= 70 {
                return Err(fail(off, "draw queue exceeds task pool"));
            }
            m.write_u8(0x03001148 + u32::from(count) * 2, y)?;
            m.write_u8(0x0300127a, count + 1)?;
            m.write_u8(0x03001149 + u32::from(count) * 2, id as u8)?;
        }
        (0x117c, []) => sorted(m)?,
        (0x1088, []) => compose(m)?,
        _ => {
            return Err(fail(
                off,
                format!("invalid Garden service arguments {args:#x?}"),
            ))
        }
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::memory::{MemoryImage, Region};
    use serde_json::Value;
    fn pattern(seed: u8, n: usize) -> Vec<u8> {
        (0..n)
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
    fn unhex(s: &str) -> Vec<u8> {
        s.as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect()
    }
    struct Probe {
        calls: Vec<(u32, Vec<u32>)>,
        mutate: bool,
    }
    impl Calls for Probe {
        fn invoke(
            &mut self,
            m: &mut dyn FrameMemory,
            off: u32,
            args: &[u32],
        ) -> Result<u32, FrameError> {
            assert_eq!(off, 0x35cc);
            self.calls.push((off, args.to_vec()));
            if self.mutate && args[0] == 0x03005320 + 2 * 36 {
                m.write_u8(args[0] + 6, 4)?;
            }
            Ok(args[0])
        }
    }
    #[test]
    fn original_services_match_all_synthetic_task_queue_and_oam_snapshots() {
        let f: Value =
            serde_json::from_str(include_str!("../../tests/garden-services-vectors.json")).unwrap();
        for c in f["cases"].as_array().unwrap() {
            let seed = c["seed"].as_u64().unwrap() as u8;
            let mut m = MemoryImage::fixture(vec![
                region(0x03000000, pattern(seed, 0x8000)),
                region(0x06000000, pattern(seed.wrapping_add(7), 0x20000)),
                region(0x02030000, pattern(seed.wrapping_add(19), 0x10000)),
            ]);
            for p in c["patches"].as_array().unwrap() {
                m.write_bytes(
                    p[0].as_u64().unwrap() as u32,
                    &unhex(p[1].as_str().unwrap()),
                )
                .unwrap();
            }
            let args: Vec<u32> = c["args"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u32)
                .collect();
            let off = c["entry"].as_u64().unwrap() as u32;
            let mut calls = Probe {
                calls: vec![],
                mutate: c["mutate"].as_bool().unwrap(),
            };
            let result = dispatch(&mut m, &mut calls, off, &args).unwrap();
            if let Some(expected) = c["return"].as_u64() {
                assert_eq!(result, expected as u32, "entry {off:x} return");
            }
            assert_eq!(
                crate::sha256(m.bytes(0x03000000, 0x7000).unwrap()),
                c["iwramSha256"].as_str().unwrap(),
                "entry {off:x} seed{seed}"
            );
            assert_eq!(
                crate::sha256(m.bytes(0x06000000, 0x20000).unwrap()),
                c["vramSha256"].as_str().unwrap(),
                "entry {off:x}"
            );
            assert_eq!(serde_json::to_value(calls.calls).unwrap(), c["calls"]);
        }
    }
    #[test]
    fn malformed_counts_and_callbacks_stop_bounded_without_invented_work() {
        let mut m = MemoryImage::fixture(vec![region(0x03000000, vec![0; 0x7000])]);
        m.write_u8(0x030014f8, 25).unwrap();
        let mut c = Probe {
            calls: vec![],
            mutate: false,
        };
        assert!(dispatch(&mut m, &mut c, 0x1420, &[]).is_err());
        assert_eq!(m.read_u8(0x030014f8).unwrap(), 25);
        assert!(dispatch(&mut m, &mut c, 0xeac, &[]).is_err());
        assert!(c.calls.is_empty());
    }
}
