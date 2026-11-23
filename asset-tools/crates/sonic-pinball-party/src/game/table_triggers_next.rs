//! Native directional contact recognition in the 344E8 table event handler.
use super::{
    frame::{FrameError, FrameMemory, NativeCall},
    lifecycle::LifecycleCalls,
    scoring::ScoringCalls,
};
type M<'a> = &'a mut dyn FrameMemory;
type C<'a> = &'a mut dyn LifecycleCalls;
type R = Result<(), FrameError>;
const E: u32 = 0x03004560;
const F: u32 = 0x0203513c;
const L: u32 = 0x02034f50;
fn field(m: &dyn FrameMemory, o: u32) -> Result<u16, FrameError> {
    let a = m.read_u32(0x030040b0)?;
    if a & 3 != 0
        || !((0x02000000..=0x02040000 - 0xac).contains(&a)
            || (0x03000000..=0x03008000 - 0xac).contains(&a))
    {
        return Err(FrameError::MissingRead {
            address: a,
            width: 4,
        });
    }
    m.read_u16(a + o)
}
fn clear(m: M<'_>, start: u32, n: u32) -> R {
    for i in start..start + n {
        m.write_u16(F + 2 * i, 0)?;
    }
    Ok(())
}
fn full(m: &dyn FrameMemory, start: u32, n: u32) -> Result<bool, FrameError> {
    for i in start..start + n {
        if m.read_u16(F + 2 * i)? == 0 {
            return Ok(false);
        }
    }
    Ok(true)
}
fn endpoint(m: M<'_>, c: C<'_>, pair: bool, first: bool, alternate: bool) -> R {
    let (start, n, sound) = if pair { (4, 2, 106) } else { (0, 4, 105) };
    let speed = field(m, 0xaa)? as i16;
    let index = if first { start } else { start + n - 1 };
    m.write_u16(F + 2 * index, 1)?;
    if (first && speed < 0) || (!first && speed <= 0) {
        if first {
            clear(m, start + 1, n - 1)
        } else {
            clear(m, start, n - 1)
        }
    } else {
        let ready = full(m, start, n)?;
        if first {
            // The two-contact branch invokes 459FC even on an incomplete sequence.
            if ready && alternate {
                c.call(m, 0x389c, &[105])?;
            }
            if ready || (pair && !alternate) {
                c.call(m, 0x459fc, &[1])?;
            }
            if ready {
                if alternate || field(m, 0x6e)? == 5 {
                    c.call(m, 0x389c, &[sound])?;
                }
                m.write_u16(L + if pair { 30 } else { 28 }, 1)?;
            }
        } else if ready {
            c.call(m, 0x389c, &[105])?;
            c.call(m, 0x459fc, &[1])?;
            c.call(m, 0x389c, &[sound])?;
            if alternate && m.read_u16(0x02035fa0)? != 0 {
                c.call(m, 0x43124, &[10000])?;
            }
            m.write_u16(L + if pair { 4 } else { 2 }, 1)?;
            if !alternate {
                let gain = 1500 * u32::from(m.read_u16(0x02036050)?);
                ScoringCalls(c).call(m, 0x30c58, &[gain, 0])?;
            }
        }
        clear(m, start, n)
    }
}
fn run(m: M<'_>, c: C<'_>, alternate: bool) -> R {
    if m.read_u16(L + 2)? != 0 || m.read_u16(L + 4)? != 0 {
        m.write_u16(L + 2, 0)?;
        return m.write_u16(L + 4, 0);
    }
    if m.read_u32(L + 28)? != 0 {
        m.write_u16(L + 28, 0)?;
        return m.write_u16(L + 30, 0);
    }
    if m.read_u8(E)? != 0 {
        endpoint(m, c, false, true, alternate)?;
    } else if m.read_u8(E + 1)? != 0 {
        m.write_u16(F + 2, 1)?;
    } else if m.read_u8(E + 2)? != 0 {
        m.write_u16(F + 4, 1)?;
    } else if m.read_u8(E + 3)? != 0 {
        endpoint(m, c, false, false, alternate)?;
    }
    if m.read_u8(E + 4)? != 0 {
        endpoint(m, c, true, true, alternate)?;
    } else if m.read_u8(E + 5)? != 0 {
        endpoint(m, c, true, false, alternate)?;
    }
    Ok(())
}
pub struct DirectionalTriggerCalls<'a>(pub &'a mut dyn LifecycleCalls);
impl LifecycleCalls for DirectionalTriggerCalls<'_> {
    fn call(&mut self, m: M<'_>, off: u32, args: &[u32]) -> R {
        match (off, args) {
            (0x344e8 | 0x347dc, []) => run(m, self.0, off == 0x347dc),
            (0x344e8 | 0x347dc, _) => Err(FrameError::Dependency {
                call: NativeCall {
                    entry_offset: off,
                    argument0: args.first().copied(),
                },
                reason: "invalid directional contact arguments".into(),
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
    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn matches_original_router_state_writes_and_calls() {
        let v: Value = serde_json::from_str(vectors!("table_triggers_next-vectors.json")).unwrap();
        let cases = v["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 600);
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
            for frame in case["frames"].as_array().unwrap() {
                m.patches(frame);
                DirectionalTriggerCalls(&mut calls)
                    .call(&mut m, num(&case["offset"]), &args)
                    .unwrap();
            }
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
