//! Native 64-bit score awards and four-contact progression (A3VE 0x33d40).
use super::{
    frame::{FrameError, FrameMemory, NativeCall},
    lifecycle::LifecycleCalls,
};
type M<'a> = &'a mut dyn FrameMemory;
type R = Result<(), FrameError>;
const CAP: u64 = 9_999_999_999_999_999_999;
const CONTACT: u32 = 0x02035130;
const EDGE: u32 = 0x03004560;
const LATCH: u32 = 0x02034f50;
const DISPLAY: u32 = 0x02017618;
fn error(off: u32, reason: &str) -> FrameError {
    FrameError::Dependency {
        call: NativeCall {
            entry_offset: off,
            argument0: None,
        },
        reason: reason.into(),
    }
}
fn state(m: &dyn FrameMemory, len: u32) -> Result<u32, FrameError> {
    let a = m.read_u32(0x030040b0)?;
    if a & 3 == 0
        && ((0x02000000..=0x02040000 - len).contains(&a)
            || (0x03000000..=0x03008000 - len).contains(&a))
    {
        Ok(a)
    } else {
        Err(FrameError::MissingRead {
            address: a,
            width: 4,
        })
    }
}
fn award(m: M<'_>, gain: u64) -> R {
    let p = state(m, 16)?;
    let old = u64::from(m.read_u32(p + 8)?) | (u64::from(m.read_u32(p + 12)?) << 32);
    let value = if gain > CAP.wrapping_sub(old) {
        CAP
    } else {
        old.wrapping_add(gain)
    };
    for i in 0..4 {
        m.write_u16(p + 8 + 2 * i, (value >> (16 * i)) as u16)?;
    }
    Ok(())
}
// 30D20 specialized to the caller's three/four local halfword flags. The
// resource's actual NUL-terminated length controls centering, not stage count.
pub(super) fn progress_message(m: M<'_>, src: u32, count: u16, max: u16) -> R {
    let mut n = 0u16;
    while m.read_u8(src + u32::from(n))? != 0 {
        if n >= max {
            return Err(error(0x30d20, "resource exceeds native contact flag array"));
        }
        n += 1;
    }
    m.write_u8(DISPLAY, 2)?;
    m.write_u8(DISPLAY + 60, 2)?;
    for i in 1..=61 {
        m.write_u8(DISPLAY + i, 32)?;
    }
    let start = 16 - 2 * u32::from(n);
    for i in 0..n.min(count) {
        let at = DISPLAY + start + 4 * u32::from(i);
        for j in 0..4 {
            m.write_u8(at + j, m.read_u8(src + u32::from(i))?)?;
        }
        if i != count - 1 {
            for j in 0..4 {
                m.write_u8(at + 30 + j, m.read_u8(src + u32::from(i))?)?;
            }
        }
    }
    Ok(())
}
fn clear(m: M<'_>) -> R {
    for i in 0..4 {
        m.write_u16(CONTACT + 2 * i, 0)?;
    }
    Ok(())
}
fn contacts(m: M<'_>, c: &mut dyn LifecycleCalls) -> R {
    let p = state(m, 0xac)?;
    if (m.read_u16(p + 0x9a)? as i16) > 224 {
        clear(m)?;
    }
    if m.read_u32(LATCH)? != 0 {
        m.write_u16(LATCH, 0)?;
        return m.write_u16(LATCH + 2, 0);
    }
    let first = m.read_u8(EDGE)? != 0;
    if !first && m.read_u8(EDGE + 3)? == 0 {
        for i in [1, 2] {
            if m.read_u16(CONTACT + 2 * i)? == 0 {
                m.write_u16(CONTACT + 2 * i, u16::from(m.read_u8(EDGE + i)?))?;
            }
        }
        return Ok(());
    }
    let end = if first { 0 } else { 3 };
    m.write_u16(CONTACT + 2 * end, 1)?;
    if (m.read_u16(p + 0xaa)? as i16) < 0 {
        for i in 0..4 {
            if i != end {
                m.write_u16(CONTACT + 2 * i, 0)?;
            }
        }
        return Ok(());
    }
    for i in 0..4 {
        if m.read_u16(CONTACT + 2 * i)? == 0 {
            return clear(m);
        }
    }
    c.call(m, 0x389c, &[if first { 105 } else { 106 }])?;
    m.write_u16(LATCH + if first { 2 } else { 0 }, 1)?;
    let (field, max, src, msg, timer) = if first {
        (CONTACT + 10, 3, 0x080b0984, 0x080b0988, 0x02035624)
    } else {
        (CONTACT + 8, 4, 0x080b09a8, 0x080b09b0, 0x02035622)
    };
    if m.read_u16(0x02035fa0)? == 0 {
        let n = m.read_u16(field)?;
        if n < max {
            m.write_u16(field, n + 1)?;
            progress_message(m, src, n + 1, max)?;
            c.call(m, 0x3046c, &[DISPLAY, 6, 80])?;
            if m.read_u16(field)? == max {
                m.write_u16(0x02035ffa, m.read_u16(0x02035ffa)?.wrapping_add(1))?;
                c.call(m, 0x45a18, &[])?;
            }
        }
    }
    m.write_u16(timer, 60)?;
    let alternate = m.read_u16(0x02035fa0)? != 0;
    if alternate {
        c.call(m, 0x43124, &[10000])?;
    }
    if first || !alternate {
        if m.read_u16(state(m, 0xac)? + 0x6e)? == 3 {
            if m.read_u16(field)? == max {
                m.write_u16(field, 0)?;
            }
        } else if m.read_u16(field)? == max {
            if first {
                c.call(m, 0x2f614, &[1])?;
            }
            c.call(m, 0x2f9b4, &[msg, 1, 60])?;
            if !first {
                c.call(m, 0x2f614, &[1])?;
            }
        }
    }
    let gain = 1000 * u64::from(m.read_u16(0x02036050)?);
    award(m, gain)?;
    clear(m)
}
pub struct ScoringCalls<'a>(pub &'a mut dyn LifecycleCalls);
impl LifecycleCalls for ScoringCalls<'_> {
    fn call(&mut self, m: M<'_>, off: u32, args: &[u32]) -> R {
        match (off, args) {
            (0x30c58, &[lo, hi]) => award(m, u64::from(lo) | (u64::from(hi) << 32)),
            (0x33d40, []) => contacts(m, self.0),
            (0x30c58 | 0x33d40, _) => Err(error(off, "invalid native scoring arguments")),
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
    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn matches_original_router_state_writes_and_calls() {
        let v: Value = serde_json::from_str(vectors!("scoring-vectors.json")).unwrap();
        let cases = v["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 152);
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
            ScoringCalls(&mut calls)
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
