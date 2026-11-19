//! Native A3VE table contact sequences 34044 and 34168.
use super::{
    frame::{FrameError, FrameMemory, NativeCall},
    lifecycle::LifecycleCalls,
    scoring::{progress_message, ScoringCalls},
};
type M<'a> = &'a mut dyn FrameMemory;
type C<'a> = &'a mut dyn LifecycleCalls;
type R = Result<(), FrameError>;
const E: u32 = 0x03004560;
const F: u32 = 0x02035130;
const L: u32 = 0x02034f50;
fn clear(m: M<'_>, off: u32, n: u32) -> R {
    for i in 0..n {
        m.write_u16(F + off + 2 * i, 0)?;
    }
    Ok(())
}
fn inc(m: M<'_>, a: u32) -> R {
    m.write_u16(a, m.read_u16(a)?.wrapping_add(1))
}
fn award(m: M<'_>, c: C<'_>, base: u32, signed: bool) -> R {
    let low = base.wrapping_mul(u32::from(m.read_u16(0x02036050)?));
    let high = if signed {
        ((low as i32) >> 31) as u32
    } else {
        0
    };
    ScoringCalls(c).call(m, 0x30c58, &[low, high])
}
fn four(m: M<'_>, c: C<'_>) -> R {
    if m.read_u32(L)? != 0 {
        m.write_u16(L, 0)?;
        return m.write_u16(L + 2, 0);
    }
    let first = m.read_u8(E)? != 0;
    if !first && m.read_u8(E + 3)? == 0 {
        for i in [1, 2] {
            if m.read_u16(F + 2 * i)? == 0 {
                m.write_u16(F + 2 * i, u16::from(m.read_u8(E + i)?))?;
            }
        }
        return Ok(());
    }
    let i = if first { 0 } else { 3 };
    m.write_u16(F + 2 * i, 1)?;
    let p = m.read_u32(0x030040b0)?;
    if p & 3 != 0
        || !((0x02000000..=0x02040000 - 0xac).contains(&p)
            || (0x03000000..=0x03008000 - 0xac).contains(&p))
    {
        return Err(FrameError::MissingRead {
            address: p,
            width: 4,
        });
    }
    if (m.read_u16(p + 0xaa)? as i16) < 0 {
        for j in 0..4 {
            if i != j {
                m.write_u16(F + 2 * j, 0)?;
            }
        }
        return Ok(());
    }
    for j in 0..4 {
        if m.read_u16(F + j * 2)? == 0 {
            return clear(m, 0, 4);
        }
    }
    c.call(m, 0x389c, &[if first { 105 } else { 106 }])?;
    m.write_u16(L + if first { 2 } else { 0 }, 1)?;
    award(m, c, 1000, true)?;
    clear(m, 0, 4)
}
fn stage(m: M<'_>, c: C<'_>, src: u32, n: u16) -> R {
    progress_message(m, src, n, 4)?;
    c.call(m, 0x3046c, &[0x02017618, 6, 80])
}
fn trigger_a(m: M<'_>, c: C<'_>) -> R {
    c.call(m, 0x459fc, &[1])?;
    inc(m, 0x02036008)?;
    c.call(m, 0x389c, &[105])?;
    if m.read_u16(0x02035fa0)? != 0 {
        c.call(m, 0x43124, &[10000])?;
    } else {
        let n = m.read_u16(F + 24)?;
        if n == 4 {
            c.call(m, 0x389c, &[105])?;
            c.call(m, 0x2f9b4, &[0x080b09ec, 1, 60])?;
        } else {
            if n < 4 {
                m.write_u16(F + 24, n + 1)?;
            }
            c.call(m, 0x389c, &[105])?;
            m.write_u16(0x02035622, 60)?;
            let count = m.read_u16(F + 24)?;
            stage(m, c, 0x080b0a0c, count)?;
            if m.read_u16(F + 24)? == 4 {
                inc(m, 0x02035ffa)?;
                c.call(m, 0x45a18, &[])?;
                c.call(m, 0x2f9b4, &[0x080b09ec, 1, 60])?;
            }
        }
    }
    m.write_u16(L + 2, 1)?;
    award(m, c, 1500, false)
}
fn trigger_b(m: M<'_>, c: C<'_>) -> R {
    c.call(m, 0x389c, &[105])?;
    c.call(m, 0x459fc, &[1])?;
    inc(m, 0x02036008)?;
    if m.read_u16(0x02035fa0)? != 0 {
        c.call(m, 0x43124, &[10000])?;
    } else {
        let old = m.read_u16(F + 26)?;
        let next = old.wrapping_add(1);
        m.write_u16(F + 26, next)?;
        if next > 4 {
            m.write_u16(F + 26, old.wrapping_sub(3))?;
        }
        m.write_u16(0x02035624, 60)?;
        c.call(m, 0x389c, &[106])?;
        if m.read_u16(0x0203516e)? != 0 {
            award(m, c, 100000, true)?;
            c.call(m, 0x3046c, &[0x080b09d8, 8, 60])?;
            c.call(m, 0x389c, &[130])?;
        } else if m.read_u16(0x02035ff6)? == 0 {
            let n = m.read_u16(F + 26)?;
            stage(m, c, 0x080b0a14, n)?;
        } else {
            m.write_u16(0x02035ff6, 1)?;
        }
        if m.read_u16(F + 26)? == 4 {
            inc(m, 0x02035ffa)?;
            c.call(m, 0x45a18, &[])?;
            m.write_u16(0x02035624, 120)?;
        }
    }
    m.write_u16(L + 4, 1)?;
    award(m, c, 1500, false)
}
fn six(m: M<'_>, c: C<'_>) -> R {
    if m.read_u16(L + 2)? != 0 || m.read_u16(L + 4)? != 0 {
        m.write_u16(L + 2, 0)?;
        return m.write_u16(L + 4, 0);
    }
    if m.read_u16(0x0203516e)? != 0 {
        m.write_u16(0x02035170, m.read_u16(0x02035170)?.wrapping_sub(1))?;
    }
    if m.read_u16(0x02035170)? == 0 {
        m.write_u16(0x0203516e, 0)?;
    }
    if m.read_u8(E)? != 0 {
        m.write_u16(F + 12, 1)?;
        clear(m, 14, 3)?;
    } else if m.read_u8(E + 1)? != 0 {
        m.write_u16(F + 14, 1)?;
    } else if m.read_u8(E + 2)? != 0 {
        m.write_u16(F + 16, 1)?;
    } else if m.read_u8(E + 3)? != 0 {
        m.write_u16(F + 18, 1)?;
        if m.read_u16(F + 12)? != 0 && m.read_u16(F + 14)? != 0 && m.read_u16(F + 16)? != 0 {
            trigger_a(m, c)?;
        }
        clear(m, 12, 4)?;
    }
    if m.read_u8(E + 4)? != 0 {
        m.write_u16(F + 20, 1)?;
        m.write_u16(F + 22, 0)?;
    } else if m.read_u8(E + 5)? != 0 {
        m.write_u16(F + 22, 1)?;
        if m.read_u16(F + 20)? != 0 {
            trigger_b(m, c)?;
        }
        clear(m, 20, 2)?;
    }
    Ok(())
}
pub struct TriggerCalls<'a>(pub &'a mut dyn LifecycleCalls);
impl LifecycleCalls for TriggerCalls<'_> {
    fn call(&mut self, m: M<'_>, off: u32, args: &[u32]) -> R {
        match (off, args) {
            (0x34044, []) => four(m, self.0),
            (0x34168, []) => six(m, self.0),
            (0x34044 | 0x34168, _) => Err(FrameError::Dependency {
                call: NativeCall {
                    entry_offset: off,
                    argument0: args.first().copied(),
                },
                reason: "invalid contact trigger arguments".into(),
            }),
            _ => self.0.call(m, off, args),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::collections::BTreeMap;
    #[derive(Clone, Default)]
    struct Mem(BTreeMap<u32, u8>);
    impl Mem {
        fn put(&mut self, a: u32, n: u32, v: u32) {
            for i in 0..n {
                self.0.insert(a + i, (v >> (8 * i)) as u8);
            }
        }
        fn load(&self, a: u32, n: u32) -> u32 {
            (0..n).fold(0, |v, i| {
                v | (u32::from(*self.0.get(&(a + i)).unwrap_or(&0)) << (8 * i))
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
    fn matches_original_router_state_writes_and_calls() {
        let v: Value =
            serde_json::from_str(include_str!("../../tests/table_triggers-vectors.json")).unwrap();
        let cases = v["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 256);
        for (i, case) in cases.iter().enumerate() {
            let mut m = Mem::default();
            m.patches(&v["common"]);
            m.patches(&case["init"]);
            let before = m.clone();
            let mut calls = Probe {
                calls: vec![],
                mutations: &case["mutations"],
            };
            let args: Vec<_> = case["args"].as_array().unwrap().iter().map(num).collect();
            TriggerCalls(&mut calls)
                .call(&mut m, num(&case["offset"]), &args)
                .unwrap();
            assert_eq!(json!(calls.calls), case["calls"], "calls {i}");
            let diff: Vec<_> =
                m.0.iter()
                    .filter(|(a, b)| before.0.get(a).copied().unwrap_or(0) != **b)
                    .map(|(a, b)| json!([a, b]))
                    .collect();
            assert_eq!(json!(diff), case["diff"], "memory case {i}");
        }
    }
}
