//! A3VE table dispatch (0x2e1a0) and table 0/1 lifecycle (0x2e1e0).
use super::ball::{MainBall, ResetAuxiliary};
use super::frame::{FrameError, FrameMemory};

pub trait LifecycleCalls {
    /// Native callee offset and the consumed arguments, including stack arguments.
    fn call(
        &mut self,
        memory: &mut dyn FrameMemory,
        offset: u32,
        args: &[u32],
    ) -> Result<(), FrameError>;
}

type Memory<'a> = &'a mut dyn FrameMemory;
type Calls<'a> = &'a mut dyn LifecycleCalls;
type ResultT = Result<(), FrameError>;
const PTR: u32 = 0x030040b0;
const TABLE: u32 = 0x03003068;

/// The C4-byte native ball prefix must fit wholly in work RAM.
fn state_ptr(m: &dyn FrameMemory) -> Result<u32, FrameError> {
    let s = m.read_u32(PTR)?;
    if s & 3 == 0
        && ((0x02000000..=0x02040000 - 0xc4).contains(&s)
            || (0x03000000..=0x03008000 - 0xc4).contains(&s))
    {
        Ok(s)
    } else {
        Err(FrameError::MissingRead {
            address: s,
            width: 4,
        })
    }
}
pub fn dispatch(m: Memory<'_>, c: Calls<'_>) -> ResultT {
    state_ptr(m)?;
    match m.read_u16(TABLE)? {
        0 | 1 => table01(m, c)?,
        2 => c.call(m, 0x2e51c, &[])?,
        3 => c.call(m, 0x5c544, &[])?,
        _ => (),
    }
    let s = state_ptr(m)?;
    let count = m.read_u8(s + 6)? as i8 as i16 as u16;
    m.write_u16(s + 0x6c, count)
}
fn inc(m: Memory<'_>, a: u32) -> ResultT {
    let n = m.read_u16(a)?.wrapping_add(1);
    m.write_u16(a, n)
}
fn reset(m: Memory<'_>, c: Calls<'_>) -> ResultT {
    let table = m.read_u16(TABLE)?;
    let mut aux = ResetAuxiliary::default();
    aux.fields_ac[0] = m.read_u16(0x0203296c)?;
    if let Some(p) = MainBall::default().initialize(table, &mut aux) {
        c.call(m, 0x5724, &[p.source_address, p.argument1, p.argument2])?;
    }
    let s = state_ptr(m)?;
    let mut ball = MainBall::default();
    for (i, b) in ball.as_bytes_mut().iter_mut().enumerate() {
        *b = m.read_u8(s + i as u32)?;
    }
    let before = ball.clone();
    ball.initialize(m.read_u16(TABLE)?, &mut aux);
    for (i, (&a, &b)) in before.as_bytes().iter().zip(ball.as_bytes()).enumerate() {
        if a != b {
            m.write_u8(s + i as u32, b)?;
        }
    }
    for (a, values) in [
        (0x020327d0, &aux.trail.phase),
        (0x02032800, &aux.trail.x),
        (0x02032830, &aux.trail.y),
        (0x02032860, &aux.trail.zero),
        (0x02032890, &aux.trail.camera_y),
    ] {
        for (i, &v) in values.iter().enumerate() {
            m.write_u16(a + 2 * i as u32, v)?;
        }
    }
    for (i, v) in aux.fields_ac.into_iter().enumerate() {
        m.write_u16(0x0203296c + 2 * i as u32, v)?;
    }
    m.write_u16(0x020328b8, aux.trail.count)
}

fn flash(m: Memory<'_>, c: Calls<'_>) -> ResultT {
    let n = m.read_u16(0x020329a0)?.wrapping_add(1) & 15;
    m.write_u16(0x020329a0, n)?;
    if n > 8 {
        let s = state_ptr(m)?;
        let y = m.read_u16(s + 0x12)?;
        if y.wrapping_sub(238) <= 166 {
            let x = 234u16
                .wrapping_sub(m.read_u16(0x0203524c)?)
                .wrapping_sub(m.read_u16(s + 0x10)?);
            let index = m.read_u16(0x03002948)?;
            let obj = m.read_u32(0x0806a43c + 4 * u32::from(index))?;
            let sprite = m.read_u32(obj.wrapping_add(0xa4))?;
            c.call(
                m,
                0x2614,
                &[0, x.into(), 397u16.wrapping_sub(y).into(), sprite, 0, 0],
            )?;
        }
    }
    Ok(())
}
pub fn table01(m: Memory<'_>, c: Calls<'_>) -> ResultT {
    let s = state_ptr(m)?;
    let state = m.read_u16(s + 0x28)?;
    match state {
        1 | 2 if m.read_u16(s + 0x10)? != 8 => inc(m, s + 0x10)?,
        1 => {
            if m.read_u16(s + 0xa2)? as i16 > 751 && m.read_u8(s + 3)? != 0 {
                m.write_u8(s + 3, 0)?;
                reset(m, c)?;
            }
            let s = state_ptr(m)?;
            m.write_u8(s + 0x14, 0)?;
            if m.read_u8(s + 3)? == 0 {
                m.write_u16(s + 0x28, 10)?;
                if m.read_u16(0x02035746)? == 0 {
                    c.call(m, 0x389c, &[56])?;
                }
            }
            flash(m, c)?;
        }
        2 | 4 => {
            c.call(m, 0x42758, &[30])?;
            let s = state_ptr(m)?;
            if state == 2 {
                if m.read_u32(s + 8)? | m.read_u32(s + 12)? == 0 {
                    m.write_u16(0x0203513a, 2)?;
                }
                c.call(m, 0x12ab4, &[0])?;
                let s = state_ptr(m)?;
                inc(m, s + 0x28)?;
            } else {
                m.write_u16(s + 0x28, 3)?;
                c.call(m, 0x12ab4, &[0])?;
            }
            c.call(m, 0x389c, &[128])?;
            if m.read_u16(0x020355b8)? == 0 {
                c.call(m, 0x3d3a4, &[1])?;
            }
        }
        3 => {
            m.write_u16(0x02036148, 0)?;
            m.write_u8(s + 3, 0)?;
            m.write_u8(s + 0x14, 0)?;
            if m.read_u16(0x03000516)? & m.read_u16(0x030040c2)? != 0 {
                m.write_u16(s + 0x28, 10)?;
                if m.read_u16(0x03002948)? == 7 {
                    c.call(m, 0x3046c, &[0x080afad4, 8, 180])?;
                } else {
                    let n = m.read_u16(0x03002960)?;
                    if n < 4 {
                        c.call(m, 0x3046c, &[0x080afa84 + 20 * u32::from(n), 8, 120])?;
                    }
                }
                c.call(m, 0x389c, &[56])?;
            }
            flash(m, c)?;
        }
        5 if m.read_u16(s + 0x9a)? as i16 > 375 => {
            m.write_u16(s + 0x28, 10)?;
            c.call(m, 0x389c, &[56])?;
        }
        10 => {
            if m.read_u16(s + 0x6e)? == 0 {
                m.write_u16(0x0203516e, 1)?;
            }
            m.write_u16(0x02032980, 0)?;
            m.write_u16(s + 0xb8, 0)?;
            m.write_u16(s + 0xaa, 0xfe00)?;
            inc(m, s + 0x28)?;
            m.write_u16(0x02035ff0, 255)?;
            m.write_u16(0x020355e6, 0)?;
            m.write_u16(0x02036060, 0)?;
            m.write_u16(0x02036064, 0)?;
        }
        11 if m.read_u16(s + 0x9a)? as i16 <= 31 => inc(m, s + 0x28)?,
        12 => {
            let n = m.read_u16(s + 0x10)?;
            if n != 0 {
                m.write_u16(s + 0x10, n - 1)?;
            }
            if m.read_u16(s + 0x9a)? as i16 > 96 {
                m.write_u16(0x02032980, 1)?;
                inc(m, s + 0x28)?;
            }
        }
        13 => {
            if m.read_u16(s + 0x9a)? as i16 > 307 {
                m.write_u16(s + 0xa8, 0)?;
                m.write_u16(s + 0xaa, 0)?;
                let n = m.read_u8(s + 6)?.wrapping_sub(1);
                m.write_u8(s + 6, n)?;
                m.write_u16(s + 0x28, 0)?;
                m.write_u8(s + 0x14, 1)?;
                m.write_u16(0x02035746, 0)?;
            }
            m.write_u16(0x02035170, 360)?;
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
        fn write_u8(&mut self, a: u32, v: u8) -> ResultT {
            self.put(a, 1, v.into());
            Ok(())
        }
        fn write_u16(&mut self, a: u32, v: u16) -> ResultT {
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
        fn call(&mut self, m: Memory<'_>, off: u32, args: &[u32]) -> ResultT {
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
        let v: Value = serde_json::from_str(vectors!("lifecycle-vectors.json")).unwrap();
        let cases = v["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 192);
        for (i, case) in cases.iter().enumerate() {
            let mut m = Mem::default();
            m.patches(&case["init"]);
            for (a, v) in case["reads"].as_object().unwrap() {
                m.put(a.parse().unwrap(), 4, num(v));
            }
            let before = m.clone();
            let mut calls = Probe {
                calls: vec![],
                mutations: &case["mutations"],
            };
            dispatch(&mut m, &mut calls).unwrap();
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
    fn invalid_state_pointer_does_not_write_or_call() {
        for s in [0xfffffff0, 0x0203fffc, 0x03007ff0, 0x02000001] {
            let mut m = Mem::default();
            m.put(PTR, 4, s);
            m.put(TABLE, 2, 2);
            let before = m.0.clone();
            let mut probe = Probe {
                calls: vec![],
                mutations: &Value::Null,
            };
            assert!(dispatch(&mut m, &mut probe).is_err());
            assert_eq!(m.0, before);
            assert!(probe.calls.is_empty());
        }
    }
}
