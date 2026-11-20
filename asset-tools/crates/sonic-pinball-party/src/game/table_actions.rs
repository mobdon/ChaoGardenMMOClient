//! Native A3VE event queue, table phase setup and three-record multiball initializer.
use super::{
    frame::{FrameError, FrameMemory, NativeCall},
    lifecycle::LifecycleCalls,
};
type M<'a> = &'a mut dyn FrameMemory;
type C<'a> = &'a mut dyn LifecycleCalls;
type R = Result<(), FrameError>;
const TABLE: u32 = 0x03003068;
const G: u32 = 0x02032980;
const F: u32 = 0x020329d0;
const A: u32 = 0x020328c0;
const H: u32 = 0x020355e0;
fn ptr(m: &dyn FrameMemory) -> Result<u32, FrameError> {
    let a = m.read_u32(0x030040b0)?;
    if a & 3 == 0
        && ((0x02000000..=0x02040000 - 0xc4).contains(&a)
            || (0x03000000..=0x03008000 - 0xc4).contains(&a))
    {
        Ok(a)
    } else {
        Err(FrameError::MissingRead {
            address: a,
            width: 4,
        })
    }
}
fn word(m: M<'_>, a: u32, v: u32) -> R {
    m.write_u16(a, v as u16)?;
    m.write_u16(a + 2, (v >> 16) as u16)
}
fn invalid(off: u32, why: &str) -> FrameError {
    FrameError::Dependency {
        call: NativeCall {
            entry_offset: off,
            argument0: None,
        },
        reason: why.into(),
    }
}
pub struct ActionCalls<'a>(pub &'a mut dyn LifecycleCalls);
impl LifecycleCalls for ActionCalls<'_> {
    fn call(&mut self, m: M<'_>, off: u32, args: &[u32]) -> R {
        match (off, args) {
            (0x2dff8, []) => setup(m, self.0),
            (0x2f9b4, &[p, kind, time]) => {
                let i = m.read_u16(0x02032ae0)?.wrapping_add(1) & 31;
                m.write_u16(0x02032ae0, i)?;
                let a = 0x020329e0 + 8 * u32::from(i);
                word(m, a, p)?;
                m.write_u16(a + 4, kind as u16)?;
                m.write_u16(a + 6, time as u16)
            }
            (0x2f634, &[v]) => m.write_u16(F, v as u16),
            (0x3de48, []) => multiball(m, self.0),
            (0x468a8, &[idx, v]) => slot(m, idx, v),
            (0x2dff8 | 0x2f9b4 | 0x2f634 | 0x3de48 | 0x468a8, _) => {
                Err(invalid(off, "unexpected native action argument count"))
            }
            _ => self.0.call(m, off, args),
        }
    }
}
fn slot(m: M<'_>, idx: u32, v: u32) -> R {
    let i = idx as u16;
    if i >= 4 {
        return Err(invalid(0x468a8, "native table has four slots"));
    }
    let a = 0x020360f0 + 2 * u32::from(i);
    m.write_u16(a, v as u16)?;
    m.write_u16(a + 16, 0)?;
    m.write_u16(a + 8, 0)
}
fn setup(m: M<'_>, c: C<'_>) -> R {
    let table = m.read_u16(TABLE)?;
    if !(1..=3).contains(&table) {
        return Ok(());
    }
    let s = ptr(m)?;
    m.write_u16(s + 0x28, 2)?;
    if table == 3 {
        m.write_u16(G + 20, 0)?;
    }
    if m.read_u16(0x02035746)? != 0 {
        if table != 2 || m.read_u16(s + 0x6e)? != 5 {
            c.call(
                m,
                0x3046c,
                &[
                    match table {
                        1 => 0x080afa4c,
                        2 => 0x080afa68,
                        _ => 0x080cec94,
                    },
                    8,
                    120,
                ],
            )?;
        }
        if table == 3 {
            m.write_u16(0x020365d4, 120)?;
        }
        c.call(m, 0x389c, &[129])?;
        if table == 3 {
            m.write_u16(0x02035746, 0)?;
        }
    }
    match table {
        1 => {
            for a in [F + 2, F + 4, F + 6, G, A + 0xa8] {
                m.write_u16(a, 0)?;
            }
            if m.read_u16(ptr(m)? + 0x6e)? == 0 {
                m.write_u16(
                    G + 4,
                    u16::from(m.read_u16(0x02035138)? == 4 || m.read_u16(0x0203513a)? == 3),
                )?;
            }
        }
        2 => {
            for a in [F + 8, F + 10, F + 12, G + 6, G + 8] {
                m.write_u16(a, 0)?;
            }
            m.write_u16(G + 10, u16::from(m.read_u16(0x03002948)? != 7))?;
            m.write_u16(G + 12, 0)?;
            for i in 0..4 {
                slot(m, i, 0)?;
            }
        }
        _ => (),
    }
    Ok(())
}
fn multiball(m: M<'_>, c: C<'_>) -> R {
    let s = ptr(m)?;
    let count = m.read_u16(H + 2)?;
    if count > 3 {
        return Err(invalid(
            0x3de48,
            "spawn count exceeds the two three-element ROM coordinate tables",
        ));
    }
    let mut points = [(0i16, 0i16); 3];
    for (i, (x, y)) in points.iter_mut().enumerate() {
        *x = m.read_u16(0x080b1a28 + i as u32 * 2)? as i16;
        *y = m.read_u16(0x080b1a2e + i as u32 * 2)? as i16;
    }
    m.write_u16(s + 0x76, m.read_u16(s + 0x76)?.wrapping_add(1))?;
    m.write_u16(H + 6, 0)?;
    m.write_u16(A + 0xa8, count)?;
    m.write_u16(A + 0xaa, count)?;
    for (i, &(x, y)) in points.iter().take(count.into()).enumerate() {
        let a = A + 56 * i as u32;
        for (off, v) in [
            (0, 1),
            (28, 0),
            (30, 0),
            (32, 0),
            (40, 0),
            (42, 1),
            (44, 0),
            (20, x as u16),
            (22, y as u16),
            (24, y as u16),
            (12, (x / 2) as u16),
            (14, (y / 2) as u16),
            (16, (x / 2) as u16),
            (18, (y / 2) as u16),
            (46, 1),
            (48, 1),
            (50, 0),
        ] {
            m.write_u16(a + off, v)?;
        }
        word(m, a + 36, 0)?;
        word(m, a + 4, (i32::from(x) << 7) as u32)?;
        word(m, a + 8, (i32::from(y) << 7) as u32)?;
    }
    m.write_u16(H + 2, 0)?;
    c.call(m, 0x389c, &[67])?;
    match m.read_u16(TABLE)? {
        1 => m.write_u16(0x020355c2, 32)?,
        2 => {
            m.write_u16(0x020355c0, 32)?;
            m.write_u16(G + 14, 0)?;
            m.write_u16(G + 16, 0)?;
        }
        _ => (),
    }
    m.write_u16(0x020355b0, 0)?;
    m.write_u16(0x020355b2, 0)?;
    m.write_u16(ptr(m)? + 0x6e, 9)?;
    c.call(m, 0x37d4, &[0, 18])?;
    let a = 0x02035fa0;
    word(m, a + 4, 10000)?;
    m.write_u16(a + 8, 0)?;
    m.write_u16(a + 10, 1)?;
    m.write_u16(a + 12, 0)?;
    m.write_u16(a, 1)?;
    if m.read_u16(ptr(m)? + 0x6e)? <= 8 {
        c.call(m, 0x3046c, &[0x080b20ec, 2, 120])?;
    }
    if m.read_u16(ptr(m)? + 0x6e)? != 9 {
        c.call(m, 0x5724, &[0x0876b39c, 256, 11])?;
    }
    if m.read_u16(TABLE)? == 2 {
        m.write_u16(H, 0)?;
    }
    m.write_u16(A + 0xac, 24)?;
    m.write_u16(A + 0xae, 0)?;
    m.write_u16(A + 0xb0, 1)?;
    c.call(m, 0x5724, &[0x0876b3dc, 256, 11])
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
            serde_json::from_str(include_str!("../../tests/table_actions-vectors.json")).unwrap();
        let cases = v["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 160);
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
            ActionCalls(&mut calls)
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
    #[test]
    fn rejects_invalid_counts_slots_pointers_and_arguments_before_writes() {
        for (off, args, s, n) in [
            (0x3de48, vec![], 0x02000000, 4),
            (0x3de48, vec![], 0x0203fffc, 3),
            (0x468a8, vec![4, 0], 0x02000000, 0),
            (0x2f9b4, vec![1, 2], 0x02000000, 0),
        ] {
            let mut m = Mem::default();
            m.put(0x030040b0, 4, s);
            m.put(H + 2, 2, n);
            let before = m.0.clone();
            let mut probe = Probe {
                calls: vec![],
                mutations: &Value::Null,
            };
            assert!(ActionCalls(&mut probe).call(&mut m, off, &args).is_err());
            assert_eq!(m.0, before);
            assert!(probe.calls.is_empty());
        }
    }
}
