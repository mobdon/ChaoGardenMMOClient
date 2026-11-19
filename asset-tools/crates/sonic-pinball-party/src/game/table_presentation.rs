//! A3VE background message setup, including original font tiles and descriptor tilemaps.
use super::{
    frame::{FrameError, FrameMemory, NativeCall},
    lifecycle::LifecycleCalls,
};
type M<'a> = &'a mut dyn FrameMemory;
type C<'a> = &'a mut dyn LifecycleCalls;
type R = Result<(), FrameError>;
const T: u32 = 0x02032b00;
const MAP: u32 = 0x02033f40;
const REG: u32 = 0x03000770;
fn bad(msg: &str) -> FrameError {
    FrameError::Dependency {
        call: NativeCall {
            entry_offset: 0x3046c,
            argument0: None,
        },
        reason: msg.into(),
    }
}
fn plus(a: u32, b: u32) -> Result<u32, FrameError> {
    a.checked_add(b)
        .ok_or_else(|| bad("native address overflow"))
}
fn wh(m: M<'_>, a: u32, v: u32) -> R {
    m.write_u16(a, v as u16)?;
    m.write_u16(plus(a, 2)?, (v >> 16) as u16)
}
fn copy(m: M<'_>, dst: u32, src: u32, len: u32) -> R {
    for i in (0..len.div_ceil(2)).rev() {
        let k = i * 2;
        m.write_u16(plus(dst, k)?, m.read_u16(plus(src, k)?)?)?;
    }
    Ok(())
}
fn tiles(m: M<'_>, i: u32, start: u16) -> R {
    for row in 0..3 {
        m.write_u16(MAP + 2 * (i + 32 * row), 0xf000 | (start + 32 * row as u16))?;
    }
    Ok(())
}
fn reset(m: M<'_>, sequential: bool) -> R {
    for i in 0..30 {
        tiles(m, i, if sequential { 512 + i as u16 } else { 0x2ac })?;
    }
    Ok(())
}
fn enabled(m: &dyn FrameMemory) -> Result<bool, FrameError> {
    let s = m.read_u32(0x030040b0)?;
    if s & 3 != 0
        || !((0x02000000..=0x02040000 - 0xc4).contains(&s)
            || (0x03000000..=0x03008000 - 0xc4).contains(&s))
    {
        return Err(bad("invalid native state pointer"));
    }
    Ok(m.read_u16(s + 0x86)? != 0)
}
pub struct PresentationCalls<'a>(pub &'a mut dyn LifecycleCalls);
impl LifecycleCalls for PresentationCalls<'_> {
    fn call(&mut self, m: M<'_>, off: u32, args: &[u32]) -> R {
        match (off, args) {
            (0x3046c, &[p, mode, time]) => start(m, self.0, p, mode as u16, time as u16),
            (0x2ff40, &[p, mode]) => prepare(m, self.0, p, mode as u16),
            (0x3046c | 0x2ff40, _) => Err(bad("unexpected message setup argument count")),
            _ => self.0.call(m, off, args),
        }
    }
}
pub fn start(m: M<'_>, c: C<'_>, p: u32, mode: u16, time: u16) -> R {
    if m.read_u16(0x02033f34)? != 0 {
        return Ok(());
    }
    m.write_u16(0x02032ae0, m.read_u16(0x02033f28)?)?;
    m.write_u16(0x02034f44, time)?;
    prepare(m, c, p, mode)
}
/// Native 0x2ff40: prepare the current descriptor without changing queue cursors,
/// consulting the immediate-request gate, or replacing the active duration.
pub fn prepare(m: M<'_>, c: C<'_>, p: u32, mode: u16) -> R {
    m.write_u16(0x02032ae4, mode)?;
    if mode == 0 {
        m.write_u16(0x02033f2c, 0)?;
        return reset(m, false);
    }
    m.write_u16(0x02032ae8, 0)?;
    c.call(m, 0x5784, &[15, 1, 32])?;
    match mode {
        1 | 2 | 5 | 6 | 7 => {
            m.write_u16(0x02033f2c, 1)?;
            wh(m, 0x02032af0, p)?;
            let head = m.read_u8(p)?;
            let (p, font) = if (1..=2).contains(&head) {
                let f = u16::from(head - 1);
                m.write_u16(0x02032ae8, f)?;
                let p = plus(p, 1)?;
                wh(m, 0x02032af0, p)?;
                (p, f)
            } else {
                (p, 0)
            };
            if mode == 7 {
                for i in 0..30 {
                    for row in 0..3 {
                        copy(m, T + 1024 * row + 32 * i, 0x0855a5d8 + 1024 * row, 32)?;
                    }
                }
            } else {
                let mut i = 0u32;
                while i < 30 {
                    let ch = m.read_u8(plus(p, i)?)?;
                    if !(0x21..=0x5c).contains(&ch) {
                        i += 1;
                        continue;
                    }
                    let entry = 0x080b061c + 8 * u32::from(font);
                    let width = u32::from(m.read_u16(entry + 4)?);
                    let cols = width / 8;
                    if cols == 0 {
                        return Err(bad("zero-width native font would not advance"));
                    }
                    let source = m.read_u32(entry)?;
                    for j in 0..cols.min(30 - i) {
                        let raw = if font == 0 {
                            u32::from(ch - 32) * 32
                        } else {
                            u32::from(ch)
                                .wrapping_sub(65)
                                .wrapping_mul(4 * width)
                                .wrapping_add(j * 32)
                        } as u16;
                        let off = u32::from((raw & 1023) | ((raw & 0xfc00).wrapping_mul(3)));
                        for row in 0..3 {
                            let col = if font == 0 { i } else { i + j };
                            copy(
                                m,
                                T + 1024 * row + 32 * col,
                                plus(source, off + 1024 * row)?,
                                32,
                            )?;
                        }
                    }
                    i += cols;
                }
                for i in 0..30 {
                    let ch = m.read_u8(plus(p, i)?)?;
                    let v = match ch {
                        48..=57 => 0x270 + u16::from(ch),
                        176..=185 => 0x1fd + u16::from(ch),
                        33..=92 => 512 + i as u16,
                        _ => 0x2ac,
                    };
                    tiles(m, i, v)?;
                }
            }
            if enabled(m)? {
                m.write_u16(REG + 26, 0)?;
                m.write_u16(REG + 2, m.read_u16(REG + 2)? & !32)?;
                m.write_u16(REG + 82, m.read_u16(REG + 82)? & !4)?;
            }
        }
        8..=10 => {
            if mode == 10 {
                wh(m, 0x02033f30, p)?;
                let params = m.read_u32(plus(p, 16)?)?;
                let effect = m.read_u8(plus(params, 6)?)?;
                c.call(m, 0x5784, &[15, 1, u32::from(effect)])?;
            }
            m.write_u16(0x02033f2c, 2)?;
            wh(m, 0x02033f30, p)?;
            let height = u32::from(m.read_u16(plus(p, 12)?)?);
            if height != 0 {
                let width = u32::from(m.read_u16(plus(p, 10)?)?);
                if width.checked_mul(height).is_none_or(|n| n > 160) || height > 64 {
                    return Err(bad("native BG tile staging area exceeded"));
                }
                let stride = u32::from(m.read_u16(plus(p, 6)?)?);
                let offset = u32::from(m.read_u16(plus(p, 4)?)?);
                let src = m.read_u32(p)?;
                for i in 0..height {
                    copy(
                        m,
                        T + i * width * 32,
                        plus(src, (i * stride + offset) * 32)?,
                        width * 32,
                    )?;
                }
            }
            let dst = 0x06004000 + ((u32::from(m.read_u16(REG + 10)?) & 12) << 12);
            c.call(m, 0x162c, &[dst, T, 5120])?;
            if enabled(m)? {
                let p = m.read_u32(0x02033f30)?;
                let param = m.read_u32(plus(p, 16)?)?;
                m.write_u16(REG + 26, m.read_u16(param)?)?;
                c.call(m, 0xf84, &[])?;
            }
        }
        _ => (),
    }
    m.write_u16(0x02033f38, 0)?;
    m.write_u16(0x02034f40, 0)?;
    match mode {
        2 => reset(m, false)?,
        7 => reset(m, true)?,
        8..=10 => {
            let mut pad = 0;
            let mut bottom = 0;
            let mut blank = 0x2ac;
            if mode != 8 {
                let p = m.read_u32(0x02033f30)?;
                pad = u32::from(m.read_u16(plus(p, 20)?)?);
                bottom = u32::from(m.read_u16(plus(p, 22)?)?);
                blank = m.read_u16(plus(p, 24)?)?;
            }
            reset(m, false)?;
            let p = m.read_u32(0x02033f30)?;
            let height = u32::from(m.read_u16(plus(p, 12)?)?);
            let n = height + pad + bottom;
            if n > 64 {
                return Err(bad("native BG tilemap rows exceeded"));
            }
            if n != 0 {
                let left = u32::from(m.read_u16(plus(p, 8)?)?);
                let width = u32::from(m.read_u16(plus(p, 10)?)?);
                for row in 0..n {
                    for col in 0..32 {
                        let v = if (left..left + width).contains(&col)
                            && (pad..n - bottom).contains(&row)
                        {
                            (512 + (row - pad) * width + col - left) as u16
                        } else {
                            blank
                        };
                        m.write_u16(MAP + 2 * ((64 - n + row) * 32 + col), v | 0xf000)?;
                    }
                }
            }
            let dst = 0x06000000 + ((u32::from(m.read_u16(REG + 10)?) & 0x1f00) << 3);
            c.call(m, 0x162c, &[dst, MAP, 4096])?;
            m.write_u16(REG + 10, (m.read_u16(REG + 10)? & 0x3fff) | 0x8000)?;
        }
        _ => (),
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::collections::BTreeMap;
    #[derive(Clone, Default)]
    struct Mem(BTreeMap<u32, u8>, Option<std::sync::Arc<[u8]>>);
    impl Mem {
        fn put(&mut self, a: u32, n: u32, v: u32) {
            for i in 0..n {
                self.0.insert(a + i, (v >> (8 * i)) as u8);
            }
        }
        fn load(&self, a: u32, n: u32) -> u32 {
            (0..n).fold(0, |v, i| {
                v | (u32::from(
                    self.0
                        .get(&(a + i))
                        .copied()
                        .or_else(|| {
                            self.1
                                .as_ref()?
                                .get((a + i).checked_sub(0x08000000)? as usize)
                                .copied()
                        })
                        .unwrap_or(0),
                ) << (8 * i))
            })
        }
        fn patches(&mut self, v: &Value) {
            for p in v.as_array().into_iter().flatten() {
                self.put(num(&p[0]), num(&p[1]), num(&p[2]));
            }
        }
    }
    impl FrameMemory for Mem {
        fn read_u8(&self, a: u32) -> Result<u8, FrameError> {
            Ok(self.load(a, 1) as u8)
        }
        fn read_u16(&self, a: u32) -> Result<u16, FrameError> {
            Ok(self.load(a, 2) as u16)
        }
        fn read_u32(&self, a: u32) -> Result<u32, FrameError> {
            Ok(self.load(a, 4))
        }
        fn write_u8(&mut self, a: u32, v: u8) -> R {
            self.put(a, 1, v.into());
            Ok(())
        }
        fn write_u16(&mut self, a: u32, v: u16) -> R {
            self.put(a, 2, v.into());
            Ok(())
        }
    }
    fn num(v: &Value) -> u32 {
        v.as_u64().unwrap() as u32
    }
    struct Probe<'a> {
        calls: Vec<Value>,
        mutations: &'a Value,
    }
    impl LifecycleCalls for Probe<'_> {
        fn call(&mut self, m: M<'_>, off: u32, args: &[u32]) -> R {
            self.calls.push(json!([off, args]));
            for p in self.mutations[off.to_string()]
                .as_array()
                .into_iter()
                .flatten()
            {
                for i in 0..num(&p[1]) {
                    m.write_u8(num(&p[0]) + i, (num(&p[2]) >> (8 * i)) as u8)?;
                }
            }
            Ok(())
        }
    }
    #[test]
    fn all_native_modes_match_bytes_and_graphics_calls() {
        let v: Value =
            serde_json::from_str(include_str!("../../tests/table_presentation-vectors.json"))
                .unwrap();
        let mut bank = Mem::default();
        bank.patches(&v["common"]);
        for (a, n) in [(0x08100000, 65536 + 2048), (0x0855a5d8, 2080)] {
            for i in 0..n {
                bank.put(a + i, 1, (i * 73 + 19) & 255);
            }
        }
        run(&bank, &v, "cases");
    }
    #[test]
    #[ignore = "requires a local verified A3VE_ROM"]
    fn retail_table_messages_match_native() {
        use sha2::{Digest, Sha256};
        let v: Value =
            serde_json::from_str(include_str!("../../tests/table_presentation-vectors.json"))
                .unwrap();
        let rom = std::fs::read(std::env::var_os("A3VE_ROM").unwrap()).unwrap();
        assert_eq!(format!("{:x}", Sha256::digest(&rom)), v["romSha256"]);
        run(&Mem(BTreeMap::new(), Some(rom.into())), &v, "retail");
    }
    fn run(bank: &Mem, v: &Value, key: &str) {
        use sha2::{Digest, Sha256};
        for (i, case) in v[key].as_array().unwrap().iter().enumerate() {
            let mut m = bank.clone();
            m.patches(&case["init"]);
            let mut probe = Probe {
                calls: vec![],
                mutations: &Value::Null,
            };
            let args: Vec<_> = case["args"].as_array().unwrap().iter().map(num).collect();
            PresentationCalls(&mut probe)
                .call(&mut m, 0x3046c, &args)
                .unwrap();
            assert_eq!(json!(probe.calls), case["calls"], "calls {i}");
            let mut h = Sha256::new();
            for r in v["ranges"].as_array().unwrap() {
                h.update(
                    (0..num(&r[1]))
                        .map(|j| m.read_u8(num(&r[0]) + j).unwrap())
                        .collect::<Vec<_>>(),
                );
            }
            assert_eq!(format!("{:x}", h.finalize()), case["sha"], "memory {i}");
        }
    }
}
