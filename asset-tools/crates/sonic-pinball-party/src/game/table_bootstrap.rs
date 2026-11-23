//! A3VE table bootstrap variants selected by the native state stack.
use super::{
    frame::{FrameError, FrameMemory, NativeCall},
    lifecycle::LifecycleCalls,
};
type M<'a> = &'a mut dyn FrameMemory;
type C<'a> = &'a mut dyn LifecycleCalls;
type R = Result<(), FrameError>;
const TAB: u32 = 0x03003068;
const REG: u32 = 0x03000770;
const AUX: u32 = 0x03002060;
fn bad(off: u32, msg: &str) -> FrameError {
    FrameError::Dependency {
        call: NativeCall {
            entry_offset: off,
            argument0: None,
        },
        reason: msg.into(),
    }
}
fn word(m: M<'_>, a: u32, v: u32) -> R {
    m.write_u16(a, v as u16)?;
    m.write_u16(a + 2, (v >> 16) as u16)
}
fn valid(a: u32, size: u32) -> Result<u32, FrameError> {
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
    valid(m.read_u32(0x030040b0)?, 0xc4)
}
fn push(m: M<'_>, id: u32, tag: u16) -> R {
    let n = m.read_u16(AUX)?;
    if n < 10 {
        word(m, AUX + 4 + 4 * u32::from(n), id)?;
        m.write_u16(AUX + 44 + 2 * u32::from(n), tag)?;
        m.write_u16(AUX, n + 1)?;
    }
    Ok(())
}
fn callbacks(m: M<'_>, ids: &[u32]) -> R {
    m.write_u16(AUX, 0)?;
    for &id in ids {
        push(m, id, 0)?;
    }
    Ok(())
}
fn reset_message(m: M<'_>) -> R {
    m.write_u16(REG + 26, 0)?;
    m.write_u16(REG + 2, m.read_u16(REG + 2)? & !32)?;
    m.write_u16(REG + 82, m.read_u16(REG + 82)? & !4)?;
    m.write_u16(0x02032ae0, m.read_u16(0x02033f28)?)?;
    if m.read_u16(state(m)? + 0x88)? != 0 {
        m.write_u16(0x02033f2c, 0)?;
        m.write_u16(0x02034f44, 0)?;
    }
    Ok(())
}
fn screen(m: &dyn FrameMemory, k: u32) -> Result<u32, FrameError> {
    Ok(0x06000000 + ((u32::from(m.read_u16(REG + k)?) & 0x1f00) << 3))
}
pub struct BootstrapCalls<'a>(pub &'a mut dyn LifecycleCalls);
impl LifecycleCalls for BootstrapCalls<'_> {
    fn call(&mut self, m: M<'_>, off: u32, args: &[u32]) -> R {
        match (off, args) {
            (0xa668 | 0xa808 | 0xa85c | 0xaa20 | 0xa9f0, &[ctx]) => boot(m, self.0, off, ctx),
            (0x2f68, []) => m.write_u16(AUX, 0),
            (0x2f74, &[id, tag]) => push(m, id, tag as u16),
            (0x2f92c, []) => reset_message(m),
            (0xa668 | 0xa808 | 0xa85c | 0xaa20 | 0xa9f0 | 0x2f68 | 0x2f74 | 0x2f92c, _) => {
                Err(bad(off, "invalid table bootstrap arguments"))
            }
            _ => self.0.call(m, off, args),
        }
    }
}
fn boot(m: M<'_>, c: C<'_>, off: u32, ctx: u32) -> R {
    valid(ctx, 4)?;
    if off == 0xa9f0 {
        word(m, ctx, 0)?;
        word(m, 0x020319a8, 0)?;
        return callbacks(m, &[0x0800a345, 0x0800a3ed]);
    }
    if off == 0xaa20 {
        word(m, ctx, 0)?;
        word(m, 0x020319ac, 1)?;
        word(m, 0x020319a8, 0)?;
        for f in [0x1efc4, 0x29a40, 0x2a778, 0x2dff8] {
            c.call(m, f, &[])?;
        }
        m.write_u16(state(m)? + 0x28, 4)?;
        c.call(m, 0x39ea0, &[])?;
        for (k, v) in [(4, 0x8c03), (6, 0x8e0a), (8, 0x1f09)] {
            m.write_u16(REG + k, v)?;
        }
        let matched = m.read_u16(0x02031c44)? == m.read_u16(0x03002948)? && m.read_u16(TAB)? == 2;
        m.write_u16(REG + 24, if matched { 0xffe0 } else { 0xff80 })?;
        m.write_u16(REG + 16, 0)?;
        m.write_u16(state(m)? + 0x86, 0)?;
        if m.read_u16(TAB)? == 2 {
            c.call(m, 0x1f0ec, &[])?;
        }
        callbacks(m, &[0x0800a345, 0x0800a3ed])?;
        if m.read_u16(TAB)? == 2 {
            push(m, 0x0800a815, 0)?;
        }
        let special = m.read_u16(0x02031c44)? == m.read_u16(0x03002948)? && m.read_u16(TAB)? == 2;
        for id in [
            if special { 0x0800a2e9 } else { 0x0800a159 },
            0x08009bcd,
            0x080096c1,
        ] {
            push(m, id, 0)?;
        }
        return Ok(());
    }
    let variant = off == 0xa85c;
    let mut dims = [(0u16, 0u16); 6];
    for (i, item) in dims.iter_mut().enumerate() {
        let a = 0x0806a5a8 + 4 * i as u32;
        *item = (m.read_u16(a)?, m.read_u16(a + 2)?);
    }
    word(m, ctx, 0)?;
    word(m, 0x020319ac, 1)?;
    word(m, 0x020319a8, variant.into())?;
    word(m, 0x020328bc, 0)?;
    m.write_u16(0x02031ae4, 0)?;
    c.call(m, 0x1dc4, &[0])?;
    c.call(m, 0x2560, &[])?;
    c.call(m, 0x1eff8, &[])?;
    if !variant {
        word(m, 0x020319c0, 0)?;
        m.write_u16(0x02031c44, 255)?;
    }
    let idx = m.read_u16(TAB)?;
    let &(width, height) = dims
        .get(usize::from(idx))
        .ok_or_else(|| bad(off, "extent index exceeds six native rows"))?;
    m.write_u16(0x030040e4, width)?;
    m.write_u16(0x030040e0, height)?;
    if !variant {
        c.call(m, 0x22964, &[0x0806a5c0])?;
        c.call(m, 0x27ddc, &[])?;
    }
    word(m, 0x03002944, 0)?;
    m.write_u16(0x03002950, 2)?;
    if !variant {
        for f in [
            0x20f7c, 0x3a098, 0x2b824, 0x29a40, 0x2a778, 0x2dff8, 0x39ea0,
        ] {
            c.call(m, f, &[])?;
        }
    }
    c.call(m, 0x26214, &[])?;
    if variant || m.read_u16(TAB)? == 2 {
        c.call(m, 0x1f0ec, &[])?;
    }
    if m.read_u16(TAB)? <= 1 {
        let dest = screen(m, 10)?;
        c.call(m, 0x162c, &[dest, 0x08442ed0, 2048])?;
        for row in 0..32 {
            for col in (0..32).rev() {
                m.write_u16(0x02004db8 + 64 * row + 2 * col, 0)?;
            }
        }
        let dest = screen(m, 10)? + 2048;
        c.call(m, 0x162c, &[dest, 0x02004db8, 2048])?;
        m.write_u16(REG + 18, 40)?;
    } else {
        m.write_u16(REG + 18, 0)?;
    }
    m.write_u16(REG + 26, 24)?;
    if variant {
        let dest = screen(m, 8)?;
        c.call(m, 0x162c, &[dest, 0x02004db8, 2048])?;
        m.write_u16(state(m)? + 0x86, 0)?;
        reset_message(m)?;
        callbacks(
            m,
            &[0x0800a845, 0x0800a815, 0x0800a3b5, 0x08009ff1, 0x080096c1],
        )
    } else {
        callbacks(
            m,
            &[0x0800a345, 0x0800a3ed, 0x08009e71, 0x08009991, 0x080096c1],
        )
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
        let v: Value = serde_json::from_str(vectors!("table_bootstrap-vectors.json")).unwrap();
        let cases = v["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 96);
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
            BootstrapCalls(&mut calls)
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
