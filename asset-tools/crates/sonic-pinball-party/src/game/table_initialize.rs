//! Native A3VE board display setup, tile map clearing and palette work state.
use super::{
    frame::{FrameError, FrameMemory, NativeCall},
    lifecycle::LifecycleCalls,
};
type M<'a> = &'a mut dyn FrameMemory;
type R = Result<(), FrameError>;
const REG: u32 = 0x03000770;
const PAL: u32 = 0x02001c50;
const MAP: u32 = 0x02014540;
fn word(m: M<'_>, a: u32, v: u32) -> R {
    m.write_u16(a, v as u16)?;
    m.write_u16(a + 2, (v >> 16) as u16)
}
fn scroll(m: M<'_>) -> R {
    for (a, v) in [(0x02031adc, 8), (0x02031ae0, 0), (0x02031ad4, 8)] {
        word(m, a, v)?;
    }
    m.write_u16(REG + 20, 8)?;
    m.write_u16(REG + 22, 8)?;
    m.write_u16(0x02031ae4, 0)
}
fn palette(m: M<'_>, src: u32) -> R {
    m.write_u16(0x020319a0, 0)?;
    for (o, v) in [(0, src), (8, 0x08067120), (4, 0x08067520)] {
        word(m, PAL + o, v)?;
    }
    for i in 0..32 {
        m.write_u8(PAL + 12 + i, 255)?;
        m.write_u8(PAL + 44 + i, 0)?;
    }
    Ok(())
}
fn set_range(m: M<'_>, start: u32, len: u32, value: u32) -> R {
    let start = u32::from(start as u8);
    let len = u32::from(len as u8);
    if len != 0 && start + len > 32 {
        return Err(FrameError::Dependency {
            call: NativeCall {
                entry_offset: 0x5784,
                argument0: Some(start),
            },
            reason: "palette effect range exceeds 32 banks".into(),
        });
    }
    for i in 0..len {
        m.write_u8(PAL + 44 + start + i, value as u8)?;
    }
    Ok(())
}
pub struct InitializeCalls<'a>(pub &'a mut dyn LifecycleCalls);
impl LifecycleCalls for InitializeCalls<'_> {
    fn call(&mut self, m: M<'_>, off: u32, args: &[u32]) -> R {
        match (off, args) {
            (0x1efc4, []) => scroll(m),
            (0x5618, &[src]) => palette(m, src),
            (0x5784, &[start, len, v]) => set_range(m, start, len, v),
            (0x1eff8, []) => {
                let ctrl = m.read_u16(REG)? & 0xe0ff;
                m.write_u16(REG, ctrl)?;
                m.write_u16(REG, m.read_u16(REG)? | 0x1f00)?;
                for (o, v) in [
                    (24, 0),
                    (26, 24),
                    (16, 0),
                    (18, 0),
                    (22, 0),
                    (14, 0),
                    (20, 0),
                    (12, 0),
                    (4, 0x8c03),
                    (6, 0x8e0a),
                    (8, 0x1f09),
                    (10, 0x9d08),
                    (74, 0x1f48),
                    (76, 0x071e),
                    (72, 0),
                ] {
                    m.write_u16(REG + o, v)?;
                }
                self.0.call(m, 0x30fc, &[])?;
                palette(m, 0x030000f0)?;
                set_range(m, 0, 32, 0)?;
                scroll(m)?;
                for row in 0..32 {
                    for col in (0..32).rev() {
                        m.write_u16(MAP + 64 * row + 2 * col, 0)?;
                    }
                }
                let dest = 0x06000000 | ((u32::from(m.read_u16(REG + 8)?) & 0x1f00) << 3);
                self.0.call(m, 0x162c, &[dest, MAP, 2048])
            }
            (0x1efc4 | 0x5618 | 0x5784 | 0x1eff8, _) => Err(FrameError::Dependency {
                call: NativeCall {
                    entry_offset: off,
                    argument0: args.first().copied(),
                },
                reason: "invalid board initializer arguments".into(),
            }),
            _ => self.0.call(m, off, args),
        }
    }
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
    #[test]
    fn rejects_bad_palette_ranges_before_writing() {
        for (start, len) in [(31, 2), (255, 2), (34, 1), (0, 33)] {
            let mut m = Mem::default();
            assert!(set_range(&mut m, start, len, 255).is_err());
            assert!(m.0.is_empty());
        }
    }
    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn matches_original_router_state_writes_and_calls() {
        let v: Value = serde_json::from_str(vectors!("table_initialize-vectors.json")).unwrap();
        let cases = v["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 32);
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
            InitializeCalls(&mut calls)
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
