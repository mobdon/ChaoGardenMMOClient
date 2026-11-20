//! A3VE ABI: top-level table update, table callback control, and BG message scheduling.
use super::{
    frame::{FrameError, FrameMemory, NativeCall},
    lifecycle::LifecycleCalls,
    table_presentation_frame::glyph,
};
type M<'a> = &'a mut dyn FrameMemory;
type C<'a> = &'a mut dyn LifecycleCalls;
type R = Result<(), FrameError>;
const REG: u32 = 0x03000770;
const MAP: u32 = 0x02033f40;
fn fail(off: u32, why: &str) -> FrameError {
    FrameError::Dependency {
        call: NativeCall {
            entry_offset: off,
            argument0: None,
        },
        reason: why.into(),
    }
}
fn pointer(m: &dyn FrameMemory, at: u32, size: u32) -> Result<u32, FrameError> {
    let a = m.read_u32(at)?;
    if a & 3 == 0
        && ((0x02000000..=0x02040000 - size).contains(&a)
            || (0x03000000..=0x03008000 - size).contains(&a))
    {
        Ok(a)
    } else {
        Err(FrameError::MissingRead {
            address: a,
            width: 4,
        })
    }
}
fn state(m: &dyn FrameMemory) -> Result<u32, FrameError> {
    pointer(m, 0x030040b0, 0xc4)
}
fn word(m: M<'_>, a: u32, v: u32) -> R {
    m.write_u16(a, v as u16)?;
    m.write_u16(a + 2, (v >> 16) as u16)
}
/// 60894 is the BX-r1 trampoline: callback ID remains data for the host's explicit dispatcher.
pub fn update(m: M<'_>, c: C<'_>, context: u32) -> R {
    c.call(m, 0x1dc4, &[0])?;
    c.call(m, 0x2580, &[])?;
    let depth = m.read_u16(0x03002060)?;
    if depth > 10 {
        return Err(fail(0xab78, "auxiliary stack depth exceeds ten entries"));
    }
    if depth != 0 {
        let id = m.read_u32(0x03002064 + 4 * u32::from(depth - 1))?;
        c.call(m, 0x60894, &[context, id])?;
    }
    for off in [0xa438, 0x5668, 0x2700] {
        c.call(m, off, &[])?;
    }
    Ok(())
}
pub fn control(m: M<'_>, c: C<'_>) -> R {
    let depth = m.read_u16(0x03000e00)?;
    if !(1..=10).contains(&depth) {
        return Err(fail(0xabb4, "invalid native state stack depth"));
    }
    let slot = 0x03000e2c + 2 * u32::from(depth - 1);
    let phase = m.read_u16(slot)?;
    if let Some(&off) = [0xa668, 0xa808, 0xa9f0, 0xaa20, 0xa85c].get(usize::from(phase)) {
        m.write_u16(slot, 5)?;
        word(m, 0x02004db0, 0x020199a0)?;
        return c.call(m, off, &[0x020199a0]);
    }
    let ctx = pointer(m, 0x02004db0, 4)?;
    update(m, c, ctx)?;
    if m.read_u32(0x0300296c)? == 1 && (1..=2).contains(&m.read_u16(0x03002964)?) {
        c.call(m, 0x2f92c, &[])?;
        let n = m.read_u32(0x020319c0)?;
        c.call(m, 0x5784, &[0, 32, 32u32.wrapping_sub(n)])?;
        let n = m.read_u32(0x020319c0)?.wrapping_add(1);
        word(m, 0x020319c0, n)?;
        if n as i32 > 31 {
            c.call(m, 0x2f14, &[0x08010289, 1])?;
            c.call(m, 0x2f68, &[])?;
        }
    }
    if m.read_u32(pointer(m, 0x02004db0, 4)?)? != 0 {
        let args = if m.read_u32(0x020319a4)? == 1 {
            [0x080091c5, 0]
        } else {
            [0x080081e9, 1]
        };
        c.call(m, 0x2f14, &args)?;
        c.call(m, 0x2f68, &[])?;
    }
    Ok(())
}
fn dequeue(m: M<'_>, c: C<'_>) -> Result<bool, FrameError> {
    let index = m.read_u16(0x02033f28)?;
    let end = m.read_u16(0x02032ae0)?;
    if index == end {
        return Ok(false);
    }
    let i = index.wrapping_add(1) & 31;
    m.write_u16(0x02033f28, i)?;
    let row = 0x020329e0 + 8 * u32::from(i);
    m.write_u16(0x02034f44, m.read_u16(row + 6)?)?;
    let resource = m.read_u32(row)?;
    let mode = m.read_u16(row + 4)?;
    c.call(m, 0x2ff40, &[resource, u32::from(mode)])?;
    Ok(true)
}
fn score(m: M<'_>, mut value: u64) -> Result<(u32, u16), FrameError> {
    const END: u32 = 0x020175ee;
    for i in 0..31 {
        m.write_u8(0x020175d0 + i, 32)?;
    }
    m.write_u8(END + 1, 0)?;
    let mut at = END;
    let mut length = 0u16;
    loop {
        let ch = (value % 10) as u8
            + if length != 0 && length.is_multiple_of(3) {
                0xb0
            } else {
                0x30
            };
        m.write_u8(at, ch)?;
        length += 1;
        value /= 10;
        if value == 0 {
            break;
        }
        at -= 1;
    }
    word(m, 0x020175f0, at)?;
    m.write_u16(0x02032af4, length)?;
    Ok((at, length))
}
fn icon(m: M<'_>) -> R {
    let v = m.read_u16(0x02035ff0)?;
    for (i, start) in [(0, 0x2aau16), (1, 0x2ab)] {
        for row in 0..3 {
            m.write_u16(
                MAP + 2 * (i + 32 * row),
                0xf000 | start.wrapping_add(32 * row as u16),
            )?;
        }
    }
    if [0, 1, 255].contains(&v) {
        let n = m.read_u16(state(m)? + 0x88)?;
        let base: u16 = if n == 0 {
            0x2a0
        } else if v == 0 {
            0x29e
        } else {
            0x29f
        };
        for row in 0..3 {
            m.write_u16(
                MAP + 4 + 64 * row,
                0xf000 | base.wrapping_add(n).wrapping_add(32 * row as u16),
            )?;
        }
    }
    Ok(())
}
fn map_transfer(m: M<'_>, c: C<'_>) -> R {
    let dst = 0x06000000 + ((u32::from(m.read_u16(REG + 10)?) & 0x1f00) << 3);
    c.call(m, 0x162c, &[dst, MAP, 2048])
}
pub fn presentation(m: M<'_>, c: C<'_>) -> R {
    let gate = m.read_u16(0x02033f34)?;
    if gate != 0 {
        m.write_u16(0x02033f34, gate - 1)?;
    }
    if m.read_u16(0x02032aec)? == 0 {
        return Ok(());
    }
    if m.read_u16(0x02033f2c)? == 0 {
        dequeue(m, c)?;
    }
    let kind = m.read_u16(0x02033f2c)?;
    match kind {
        0 => {
            for i in 0..30 {
                glyph(m, i, 32)?;
            }
            let s = state(m)?;
            let value = u64::from(m.read_u32(s + 8)?) | (u64::from(m.read_u32(s + 12)?) << 32);
            let (mut p, n) = score(m, value)?;
            for i in 0..20 {
                let ch = m.read_u8(p)?;
                if ch == 0 {
                    break;
                }
                if i < 20 - u32::from(n) {
                    glyph(m, 8 + i, 32)?;
                } else {
                    glyph(m, 8 + i, ch)?;
                    p += 1;
                }
            }
            icon(m)?;
            if m.read_u16(state(m)? + 0x86)? != 0 {
                m.write_u16(REG + 18, 0)?;
            }
            map_transfer(m, c)?;
        }
        1 | 2 => {
            let n = m.read_u16(0x02034f44)?;
            if n == 0 {
                if !dequeue(m, c)? {
                    m.write_u16(0x02033f2c, 0)?;
                    if kind == 2 && m.read_u16(state(m)? + 0x86)? != 0 {
                        m.write_u16(REG + 26, 0)?;
                        m.write_u16(REG + 2, m.read_u16(REG + 2)? & !32)?;
                        m.write_u16(REG + 82, m.read_u16(REG + 82)? & !4)?;
                    }
                }
            } else {
                m.write_u16(0x02034f44, n - 1)?;
                let mode = m.read_u16(0x02032ae4)?;
                c.call(m, 0x304a0, &[u32::from(mode)])?;
                if m.read_u16(state(m)? + 0x86)? != 0 {
                    m.write_u16(REG + 18, 0)?;
                }
                if kind == 1 {
                    let dst = 0x06004000 + ((u32::from(m.read_u16(REG + 10)?) & 12) << 12);
                    c.call(m, 0x162c, &[dst, 0x02032b00, 3072])?;
                    map_transfer(m, c)?;
                }
            }
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
            serde_json::from_str(include_str!("../../tests/table_frame-vectors.json")).unwrap();
        let cases = v["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 172);
        for (i, case) in cases.iter().enumerate() {
            let mut m = Mem::default();

            m.patches(&case["init"]);
            let before = m.clone();
            let mut calls = Probe {
                calls: vec![],
                mutations: &case["mutations"],
            };
            match num(&case["entry"]) {
                0xab78 => update(&mut m, &mut calls, num(&case["context"])),
                0xabb4 => control(&mut m, &mut calls),
                _ => presentation(&mut m, &mut calls),
            }
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
    fn invalid_stack_and_unimplemented_callback_stop_dispatch() {
        struct Reject;
        impl LifecycleCalls for Reject {
            fn call(&mut self, _: M<'_>, off: u32, _: &[u32]) -> R {
                Err(fail(off, "unported dependency"))
            }
        }
        let mut m = Mem::default();
        let mut calls = Probe {
            calls: vec![],
            mutations: &Value::Null,
        };
        assert!(control(&mut m, &mut calls).is_err());
        assert!(calls.calls.is_empty());
        m.put(0x03002060, 2, 11);
        assert!(update(&mut m, &mut calls, 0).is_err());
        assert_eq!(calls.calls.len(), 2);
        assert!(update(&mut m, &mut Reject, 0).is_err());
    }
    #[test]
    fn native_queue_preserves_later_requests_and_direct_preparation_state() {
        use crate::game::table_calls::TableCalls;
        let v: Value =
            serde_json::from_str(include_str!("../../tests/table_frame-vectors.json")).unwrap();
        for (idx, k) in v["sequences"].as_array().unwrap().iter().enumerate() {
            let mut m = Mem::default();
            m.patches(&k["init"]);
            let before = m.clone();
            let probe = Probe {
                calls: vec![],
                mutations: &Value::Null,
            };
            let mut calls = TableCalls::new(probe);
            for _ in 0..num(&k["steps"]) {
                if num(&k["entry"]) == 0x2ff40 {
                    calls
                        .call(&mut m, 0x2ff40, &[num(&k["desc"]), num(&k["mode"])])
                        .unwrap();
                } else {
                    calls.call(&mut m, 0x2fd68, &[]).unwrap();
                }
            }
            assert_eq!(json!(calls.backend.calls), k["calls"], "calls {idx}");
            let diff: Vec<_> =
                m.0.iter()
                    .filter(|(a, b)| before.0.get(a).copied().unwrap_or(0) != **b)
                    .map(|(a, b)| json!([a, b]))
                    .collect();
            assert_eq!(json!(diff), k["diff"], "memory {idx}");
        }
    }
}
