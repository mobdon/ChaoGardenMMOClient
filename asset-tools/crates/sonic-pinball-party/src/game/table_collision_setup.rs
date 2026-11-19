//! A3VE collision region reset and collision map pointer installation.
use super::{
    frame::{FrameError, FrameMemory, NativeCall},
    lifecycle::LifecycleCalls,
};
type M<'a> = &'a mut dyn FrameMemory;
type R = Result<(), FrameError>;
const TABLE: u32 = 0x03003068;
fn state(m: &dyn FrameMemory) -> Result<u32, FrameError> {
    let p = m.read_u32(0x030040b0)?;
    if p & 3 == 0
        && ((0x02000000..=0x02040000 - 0x1a0).contains(&p)
            || (0x03000000..=0x03008000 - 0x1a0).contains(&p))
    {
        Ok(p)
    } else {
        Err(FrameError::MissingRead {
            address: p,
            width: 4,
        })
    }
}
fn reset(m: M<'_>) -> R {
    let p = state(m)?;
    m.write_u8(p + 6, 0)?;
    for i in 0..32 {
        let p = state(m)?;
        m.write_u16(p + 0x2a + 2 * i, 0)?;
    }
    let indices: &[u32] = match m.read_u16(TABLE)? {
        1 => &[0, 1, 2],
        3 => &[9, 10],
        _ => &[],
    };
    for &i in indices {
        m.write_u16(0x02032980 + 2 * i, 0)?;
    }
    Ok(())
}
fn bind(m: M<'_>, selected: bool) -> R {
    if selected {
        let id = m.read_u16(TABLE)?;
        if id > 4 {
            return Err(FrameError::Dependency {
                call: NativeCall {
                    entry_offset: 0x2b824,
                    argument0: None,
                },
                reason: "unsupported native collision pointer row".into(),
            });
        }
        reset(m)?;
    }
    let p = state(m)?;
    m.write_u8(p + 6, u8::from(selected))?;
    for layer in 0..2 {
        let p = state(m)?;
        for (base, field) in [
            (0x086e9340u32, 0x118),
            (0x086e9360, 0x158),
            (0x086e9380, 0x198),
        ] {
            // The native index is one-based. ID zero reads the preceding pair.
            let row = if selected {
                u32::from(m.read_u16(TABLE)?).wrapping_sub(1)
            } else {
                3
            };
            let source = base.wrapping_add(row.wrapping_mul(8)) + 4 * layer;
            let value = m.read_u32(source)?;
            let a = p + field + 4 * layer;
            m.write_u16(a, value as u16)?;
            m.write_u16(a + 2, (value >> 16) as u16)?;
        }
    }
    Ok(())
}
pub struct CollisionSetupCalls<'a>(pub &'a mut dyn LifecycleCalls);
impl LifecycleCalls for CollisionSetupCalls<'_> {
    fn call(&mut self, m: M<'_>, off: u32, args: &[u32]) -> R {
        match (off, args) {
            (0x2b824, []) => bind(m, true),
            (0x2b8ac, []) => bind(m, false),
            (0x2b920, []) => reset(m),
            (0x2b824 | 0x2b8ac | 0x2b920, _) => Err(FrameError::Dependency {
                call: NativeCall {
                    entry_offset: off,
                    argument0: args.first().copied(),
                },
                reason: "invalid collision setup arguments".into(),
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
    fn rejects_unknown_selected_row_before_reset() {
        let mut m = Mem::default();
        m.put(TABLE, 2, 5);
        m.put(0x030040b0, 4, 0x02001000);
        let before = m.0.clone();
        assert!(bind(&mut m, true).is_err());
        assert_eq!(m.0, before);
    }
    #[test]
    fn matches_original_router_state_writes_and_calls() {
        let v: Value = serde_json::from_str(include_str!(
            "../../tests/table_collision_setup-vectors.json"
        ))
        .unwrap();
        let cases = v["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 24);
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
            CollisionSetupCalls(&mut calls)
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
