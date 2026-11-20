//! A3VE 0x304a0: advance the background message display by one call.
use super::{
    frame::{FrameError, FrameMemory, NativeCall},
    lifecycle::LifecycleCalls,
    math::signed_divide,
};
type M<'a> = &'a mut dyn FrameMemory;
type C<'a> = &'a mut dyn LifecycleCalls;
type R = Result<(), FrameError>;
const TIMER: u32 = 0x02033f38;
const FADE: u32 = 0x02034f40;
const MAP: u32 = 0x02033f40;
const BUFFER: u32 = 0x02032b00;
fn addr(a: u32, k: u32) -> Result<u32, FrameError> {
    a.checked_add(k).ok_or(FrameError::MissingRead {
        address: a,
        width: 1,
    })
}
pub(super) fn glyph(m: M<'_>, column: u32, ch: u8) -> R {
    let base = match ch {
        48..=57 => 0x270 + u16::from(ch),
        176..=185 => 0x1fd + u16::from(ch),
        33..=92 => 512 + column as u16,
        _ => 0x2ac,
    };
    for row in 0..3 {
        m.write_u16(
            MAP + 2 * (column + row * 32),
            0xf000 | (base + 32 * row as u16),
        )?;
    }
    Ok(())
}
fn text(m: M<'_>, col: u32, src: u32) -> R {
    let p = m.read_u32(0x02032af0)?;
    let ch = m.read_u8(addr(p, src)?)?;
    glyph(m, col, ch)
}
fn row(m: M<'_>, show: bool, shift: u32) -> R {
    for i in 0..30 {
        if show {
            text(m, i, i + shift)?
        } else {
            glyph(m, i, 32)?;
        }
    }
    Ok(())
}
fn clear(m: M<'_>) -> R {
    m.write_u16(0x02032ae4, 0)?;
    m.write_u16(0x02033f2c, 0)?;
    row(m, false, 0)
}
fn increment(m: M<'_>, a: u32) -> R {
    m.write_u16(a, m.read_u16(a)?.wrapping_add(1))
}
fn scroll(m: M<'_>, n: u16) -> R {
    if n < 120 {
        for line in 0..8 {
            for tile in 0..30 {
                for plane in 0..3 {
                    let a = BUFFER + plane * 1024 + tile * 32 + line * 4;
                    for (dst, src) in [(0, 1), (1, 2), (2, 3), (3, 32)] {
                        m.write_u8(a + dst, m.read_u8(a + src)?)?;
                    }
                }
            }
        }
    }
    let ch = if n < 120 {
        let p = m.read_u32(0x02032af0)?;
        m.read_u8(addr(p, u32::from(n) / 4)?)?
    } else {
        32
    };
    let src = match ch {
        48..=57 => 0x0855a458 + (u32::from(ch) - 48) * 32,
        65..=92 => 0x0876c13c + (u32::from(ch) - 65) * 32,
        33..=64 => (0x0876b51ci32 + (i32::from(ch) - 65) * 32) as u32,
        _ => 0x0855a5d8,
    };
    for line in 0..8 {
        for plane in 0..3 {
            let k = line * 4 + plane * 1024;
            m.write_u8(0x02032ea3 + k, m.read_u8(src + k + u32::from(n & 3))?)?;
        }
    }
    if n < 120 {
        increment(m, TIMER)?;
    }
    Ok(())
}
fn half(m: &dyn FrameMemory, p: u32, k: u32) -> Result<i32, FrameError> {
    Ok(i32::from(m.read_u16(addr(p, k)?)? as i16))
}
fn interp(m: &dyn FrameMemory, p: u32, i: u32, counter: u32) -> Result<i32, FrameError> {
    let a = half(m, p, i)?;
    let b = half(m, p, i + 2)?;
    let n = i32::from(m.read_u16(counter)?);
    Ok(a.wrapping_add(signed_divide((b - a).wrapping_mul(n), half(m, p, i + 4)?)))
}
pub fn advance(m: M<'_>, c: C<'_>, mode: u16) -> R {
    match mode {
        1 => row(m, true, 0)?,
        2 | 3 => {
            let n = m.read_u16(TIMER)?;
            if n & 1 == 0 {
                for i in 0..15 {
                    let pair = if mode == 2 {
                        [14 - i, 15 + i]
                    } else {
                        [30 - i, i]
                    };
                    let show = (i < u32::from(n / 2)) == (mode == 2);
                    for col in pair {
                        if show {
                            text(m, col, col)?;
                        } else {
                            glyph(m, col, 32)?;
                        }
                    }
                }
            }
            if mode == 2 {
                if n <= 29 {
                    increment(m, TIMER)?;
                }
            } else {
                increment(m, TIMER)?;
                if m.read_u16(TIMER)? > 30 {
                    clear(m)?;
                }
            }
        }
        4 => row(m, false, 0)?,
        5 | 6 => {
            let on = m.read_u16(TIMER)? % 20 <= 14;
            row(m, on || mode == 6, if !on && mode == 6 { 30 } else { 0 })?;
            increment(m, TIMER)?;
        }
        7 => {
            let n = m.read_u16(TIMER)?;
            scroll(m, n)?;
        }
        8..=10 => {
            let desc = m.read_u32(0x02033f30)?;
            let p = m.read_u32(addr(desc, 16)?)?;
            let s = m.read_u32(0x030040b0)?;
            if s & 3 != 0
                || !((0x02000000..=0x02040000 - 0xc4).contains(&s)
                    || (0x03000000..=0x03008000 - 0xc4).contains(&s))
            {
                return Err(FrameError::MissingRead {
                    address: s,
                    width: 4,
                });
            }
            if m.read_u16(s + 0x86)? != 0 {
                let v = interp(m, p, 0, TIMER)?;
                m.write_u16(0x0300078a, (v as u16) & 511)?;
                if mode == 10 {
                    let v = interp(m, p, 6, FADE)? as u8;
                    c.call(m, 0x5784, &[15, 1, v.into()])?;
                }
            }
            if i32::from(m.read_u16(TIMER)?) < half(m, p, 4)? {
                increment(m, TIMER)?;
            }
            if mode == 10 && i32::from(m.read_u16(FADE)?) < half(m, p, 10)? {
                increment(m, FADE)?;
            }
        }
        _ => clear(m)?,
    }
    Ok(())
}
pub struct AnimationCalls<'a>(pub &'a mut dyn LifecycleCalls);
impl LifecycleCalls for AnimationCalls<'_> {
    fn call(&mut self, m: M<'_>, off: u32, args: &[u32]) -> R {
        if off != 0x304a0 {
            return self.0.call(m, off, args);
        }
        let &[mode] = args else {
            return Err(FrameError::Dependency {
                call: NativeCall {
                    entry_offset: off,
                    argument0: None,
                },
                reason: "expected one mode argument".into(),
            });
        };
        advance(m, self.0, mode as u16)
    }
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
    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn all_native_modes_match_bytes_and_graphics_calls() {
        let v: Value =
            serde_json::from_str(vectors!("table_presentation_frame-vectors.json")).unwrap();
        let mut bank = Mem::default();

        for (a, n) in [(0x0855a458, 0x1050), (0x0876b000, 0x2500)] {
            for i in 0..n {
                bank.put(a + i, 1, (i * 37 + 13) & 255);
            }
        }
        run(&bank, &v, "cases");
    }
    #[test]
    #[ignore = "requires a local verified A3VE_ROM"]
    fn retail_table_messages_match_native() {
        use sha2::{Digest, Sha256};
        let v: Value =
            serde_json::from_str(vectors!("table_presentation_frame-vectors.json")).unwrap();
        let rom = std::fs::read(std::env::var_os("A3VE_ROM").unwrap()).unwrap();
        assert_eq!(format!("{:x}", Sha256::digest(&rom)), v["romSha256"]);
        run(&Mem(BTreeMap::new(), Some(rom.into())), &v, "retail");
    }
    fn run(bank: &Mem, v: &Value, key: &str) {
        use sha2::{Digest, Sha256};
        for (i, case) in v[key].as_array().unwrap().iter().enumerate() {
            let mut m = bank.clone();
            for (a, n) in [(0x02032b00, 3072), (0x02033f40, 192)] {
                for j in 0..n {
                    m.put(a + j, 1, (j * 43 + 19) & 255);
                }
            }
            m.patches(&case["init"]);
            let patch = if case["mutate"].as_bool().unwrap() {
                json!({"22404":[[0x02033f38,2,0],[0x02034f40,2,0]]})
            } else {
                Value::Null
            };
            let mut probe = Probe {
                calls: vec![],
                mutations: &patch,
            };
            for _ in 0..num(&case["steps"]) {
                AnimationCalls(&mut probe)
                    .call(&mut m, 0x304a0, &[num(&case["mode"]) + 65536])
                    .unwrap();
            }
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
