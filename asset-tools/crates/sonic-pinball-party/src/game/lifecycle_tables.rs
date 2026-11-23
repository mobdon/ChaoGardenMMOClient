//! A3VE table 2 (0x2e51c) and table 3 (0x5c544) lifecycle handlers.
use super::{
    frame::{FrameError, FrameMemory},
    lifecycle::LifecycleCalls,
};
type M<'a> = &'a mut dyn FrameMemory;
type C<'a> = &'a mut dyn LifecycleCalls;
type R = Result<(), FrameError>;
/// Adapter for the lifecycle router. Only the two table entry points are
/// consumed here; their sound, display and transfer dependencies are synchronous.
pub struct TableCalls<'a>(pub &'a mut dyn LifecycleCalls);
impl LifecycleCalls for TableCalls<'_> {
    fn call(&mut self, m: M<'_>, offset: u32, args: &[u32]) -> R {
        match (offset, args) {
            (0x2e51c, []) => table2(m, self.0),
            (0x5c544, []) => table3(m, self.0),
            _ => self.0.call(m, offset, args),
        }
    }
}
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
fn inc(m: M<'_>, a: u32) -> R {
    let v = m.read_u16(a)?.wrapping_add(1);
    m.write_u16(a, v)
}
fn pressed(m: &dyn FrameMemory) -> Result<bool, FrameError> {
    Ok(m.read_u16(0x03000516)? & m.read_u16(0x030040c2)? != 0)
}
fn optional(m: M<'_>, c: C<'_>) -> R {
    if m.read_u16(0x020355b8)? == 0 {
        c.call(m, 0x3d3a4, &[1])?;
    }
    Ok(())
}
pub fn table2(m: M<'_>, c: C<'_>) -> R {
    let s = ptr(m)?;
    let state = m.read_u16(s + 0x28)?;
    match state {
        12..=72 => {
            m.write_u16(s + 0xaa, 0)?;
            m.write_u16(s + 0xa8, 0)?;
            inc(m, s + 0x28)?;
            for (n, source) in [(61, 0x0853391c), (67, 0x08533b9c), (72, 0x0853371c)] {
                if m.read_u16(ptr(m)? + 0x28)? == n {
                    let i = m.read_u16(0x03000770)? & 7;
                    let dest = m
                        .read_u32(0x08066c54 + 4 * u32::from(i))?
                        .wrapping_add(0xe80);
                    c.call(m, 0x162c, &[dest, source, 512])?;
                }
            }
        }
        1 | 2 if m.read_u16(s + 0x10)? != 16 => inc(m, s + 0x10)?,
        1 => {
            m.write_u8(s + 0x14, 0)?;
            m.write_u16(s + 0x28, 10)?;
        }
        2 => {
            c.call(m, 0x42758, &[30])?;
            optional(m, c)?;
            c.call(m, 0x12ab4, &[8])?;
            c.call(m, 0x389c, &[128])?;
            let s = ptr(m)?;
            inc(m, s + 0x28)?;
        }
        3 => {
            m.write_u16(0x02036148, 0)?;
            m.write_u8(s + 3, 0)?;
            m.write_u8(s + 0x14, 0)?;
            if pressed(m)? {
                m.write_u16(s + 0x28, 10)?;
                c.call(m, 0x3046c, &[0x080afaf4, 10, 120])?;
                c.call(m, 0x389c, &[56])?;
            }
        }
        4 => {
            if m.read_u16(0x03002948)? != 7 {
                c.call(m, 0x42758, &[30])?;
            }
            m.write_u16(ptr(m)? + 0x28, 3)?;
            c.call(m, 0x12ab4, &[8])?;
            c.call(m, 0x389c, &[128])?;
            optional(m, c)?;
        }
        10 => {
            m.write_u16(s + 0xb8, 0)?;
            m.write_u16(0x020355e0, 0)?;
            m.write_u16(0x02035ff0, 255)?;
            m.write_u16(s + 0xaa, 0xfe00)?;
            inc(m, s + 0x28)?;
            if m.read_u16(s + 0x6e)? == 0 {
                m.write_u16(0x0203516e, 1)?;
            }
            m.write_u16(0x02036060, 0)?;
            m.write_u16(0x02036064, 0)?;
            m.write_u16(0x0203298a, u16::from(m.read_u16(0x03002948)? != 7))?;
        }
        11 if m.read_u16(s + 0x9a)? as i16 <= 303 => {
            m.write_u16(s + 0xb8, 1)?;
            inc(m, s + 0x28)?;
        }
        73 => {
            if m.read_u16(s + 0x6e)? != 5 {
                c.call(m, 0x3046c, &[0x080afb18, 8, 120])?;
            }
            m.write_u16(ptr(m)? + 0x28, 100)?;
        }
        100 => {
            m.write_u16(s + 0xb8, 0)?;
            m.write_u16(s + 0xaa, 0xfe00)?;
            m.write_u16(s + 0xa8, 0xff40)?;
            m.write_u16(0x020328b8, 150)?;
            let x = m.read_u16(s + 0x98)?;
            let y = m.read_u16(s + 0x9a)?;
            let cam = m.read_u16(s + 0x12)?;
            for i in 0..20 {
                for (a, v) in [
                    (0x020327d0, (i % 3) as u16),
                    (0x02032800, x),
                    (0x02032830, y),
                    (0x02032860, 0),
                    (0x02032890, cam),
                ] {
                    m.write_u16(a + 2 * i, v)?;
                }
            }
            inc(m, s + 0x28)?;
            c.call(m, 0x389c, &[56])?;
        }
        101 | 102 => {
            if state == 101 {
                if m.read_u16(s + 0x98)? as i16 <= 47 {
                    let v = m.read_u16(s + 0x10)?;
                    if v != 4 {
                        m.write_u16(s + 0x10, (v.wrapping_sub(1) as i16).max(4) as u16)?;
                    }
                }
                if m.read_u16(s + 0x9a)? as i16 > 48 && m.read_u16(s + 0xaa)? as i16 > 0 {
                    m.write_u16(0x02032986, 1)?;
                    inc(m, s + 0x28)?;
                }
            }
            if m.read_u16(s + 0x9a)? as i16 > 320 {
                m.write_u16(s + 0xa8, 0)?;
                m.write_u16(s + 0xaa, 0)?;
                m.write_u8(s + 6, m.read_u8(s + 6)?.wrapping_sub(1))?;
                m.write_u16(s + 0x28, 0)?;
                m.write_u8(s + 0x14, 1)?;
                m.write_u16(0x02035746, 0)?;
                if m.read_u16(s + 0x6e)? == 2 {
                    c.call(m, 0x2f634, &[1])?;
                }
                if m.read_u16(ptr(m)? + 0x6e)? == 0 && m.read_u16(0x02035fa0)? == 0 {
                    m.write_u16(0x020355e0, 1)?;
                }
                m.write_u16(0x02035170, 360)?;
            }
        }
        _ => (),
    }
    Ok(())
}
pub fn table3(m: M<'_>, c: C<'_>) -> R {
    let s = ptr(m)?;
    m.write_u8(s + 0x14, 0)?;
    match m.read_u16(s + 0x28)? {
        0 => m.write_u8(s + 0x14, 1)?,
        st @ (1 | 3) => {
            if st == 3 {
                m.write_u8(s + 3, 0)?;
                if !pressed(m)? {
                    return Ok(());
                }
            }
            m.write_u16(s + 0x28, 4)?;
            if st == 3 {
                c.call(m, 0x3920, &[0x020296a0, 0x086d333c])?;
                let i = m.read_u8(0x02037424)? % 5;
                c.call(m, 0x3046c, &[0x080cecb0 + 20 * u32::from(i), 8, 120])?;
                m.write_u16(0x020365d4, 120)?;
                m.write_u8(0x02037424, m.read_u8(0x02037424)?.wrapping_add(1))?;
            }
            m.write_u16(ptr(m)? + 0xaa, 0xff00)?;
            c.call(m, 0x389c, &[56])?;
            m.write_u16(0x02035ff0, 255)?;
        }
        2 => {
            m.write_u16(0x02032994, 0)?;
            c.call(m, 0x12ab4, &[18])?;
            let s = ptr(m)?;
            inc(m, s + 0x28)?;
        }
        4 if m.read_u16(s + 0x9a)? as i16 <= 223 => {
            m.write_u16(s + 0x28, 0)?;
            m.write_u16(0x02032994, 1)?;
        }
        _ => (),
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use crate::game::test_mem::{num, Mem};
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
        let v: Value = serde_json::from_str(vectors!("lifecycle_tables-vectors.json")).unwrap();
        let cases = v["cases"].as_array().unwrap();
        assert!(cases.len() > 200);
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
            if num(&case["table"]) == 2 {
                table2(&mut m, &mut calls).unwrap()
            } else {
                table3(&mut m, &mut calls).unwrap()
            };
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
    fn rejects_invalid_state_addresses_before_writes() {
        for s in [0xfffffff0, 0x0203fffc, 0x03007ff0, 0x02000001] {
            for table in [table2, table3] {
                let mut m = Mem::default();
                m.put(0x030040b0, 4, s);
                let original = m.0.clone();
                let mut probe = Probe {
                    calls: vec![],
                    mutations: &Value::Null,
                };
                assert!(table(&mut m, &mut probe).is_err());
                assert_eq!(m.0, original);
                assert!(probe.calls.is_empty());
            }
        }
    }
}
