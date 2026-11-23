//! Native camera-gated table object placement and the shared oscillation counter.
use super::{
    frame::{FrameError, FrameMemory, NativeCall},
    lifecycle::LifecycleCalls,
};
type M<'a> = &'a mut dyn FrameMemory;
type R = Result<(), FrameError>;
const ID: u32 = 0x03002948;
fn fault(a: u32) -> FrameError {
    FrameError::MissingRead {
        address: a,
        width: 4,
    }
}
fn state(m: &dyn FrameMemory) -> Result<u32, FrameError> {
    let a = m.read_u32(0x030040b0)?;
    if a & 3 == 0
        && ((0x02000000..=0x02040000 - 0x98).contains(&a)
            || (0x03000000..=0x03008000 - 0x98).contains(&a))
    {
        Ok(a)
    } else {
        Err(fault(a))
    }
}
fn y(m: &dyn FrameMemory) -> Result<i16, FrameError> {
    Ok(m.read_u16(state(m)? + 18)? as i16)
}
fn ptr(m: &dyn FrameMemory, off: u32) -> Result<u32, FrameError> {
    let id = u32::from(m.read_u16(ID)?);
    let p = m.read_u32(0x0806a43c + 4 * id)?;
    let a = p.checked_add(off).ok_or_else(|| fault(p))?;
    if !(0x08000000..=0x087ffffc).contains(&a) || a & 3 != 0 {
        return Err(fault(a));
    }
    m.read_u32(a)
}
struct Scene<'a, 'b> {
    m: M<'a>,
    calls: &'b mut dyn LifecycleCalls,
}
impl Scene<'_, '_> {
    fn draw(&mut self, pri: u32, x: i32, y: i32, source: u32, flags: u32) -> R {
        let m = &mut *self.m;
        let p = state(m)?;
        let x = (x as u16)
            .wrapping_sub(m.read_u16(p + 16)?)
            .wrapping_sub(m.read_u16(0x0203524c)?);
        let y = (y as u16).wrapping_sub(m.read_u16(p + 18)?);
        let sprite = if source >= 0x02000000 {
            source
        } else {
            ptr(m, source)?
        };
        self.calls
            .call(m, 0x2614, &[pri, x.into(), y.into(), sprite, flags, 0])
    }
    fn run(&mut self) -> R {
        match self.m.read_u16(ID)? {
            0 => {
                for (max, buf, attr, p1, x1, y1, src, p2, x2, y2) in [
                    (107, 0x02031b08, 0xd800, 2, 59, 73, 0x14c, 2, 30, 47),
                    (65, 0x02031af8, 0xd400, 3, 44, 32, 0x14c, 4, 16, 10),
                    (73, 0x02031b10, 0xd600, 3, 81, 34, 0x15c, 3, 62, 10),
                ] {
                    if y(self.m)? <= max {
                        self.m.write_u16(buf, self.m.read_u16(0x086ddf44)?)?;
                        self.m.write_u16(buf + 2, attr)?;
                        self.m.write_u16(buf + 4, self.m.read_u16(0x086ddf48)?)?;
                        self.m.write_u16(buf + 6, 65535)?;
                        self.draw(p1, x1, y1, src, 0)?;
                        self.draw(p2, x2, y2, buf, 0)?;
                    }
                }
            }
            1 => {
                if y(self.m)? <= 71 {
                    self.draw(2, 38, 17, 0x1c8, 0)?;
                }
                if (33..=207).contains(&y(self.m)?) {
                    self.draw(2, 0, 192, 0x180, 0)?;
                }
            }
            2 => {
                if y(self.m)? <= 91 {
                    self.draw(2, 27, 14, 0x1c8, 0)?;
                }
            }
            3 => {
                for (max, pri, x, yy, src, flags) in [
                    (71, 3, 40, -16, 0x1c8, 0),
                    (87, 2, 13, 8, 0x1c8, 0),
                    (79, 3, 56, 64, 0x184, 1024),
                ] {
                    if y(self.m)? <= max {
                        self.draw(pri, x, yy, src, flags)?;
                    }
                }
            }
            4 => {
                if y(self.m)? <= 87 {
                    self.draw(16, 32, 24, 0x1c8, 0)?;
                }
            }
            5 if y(self.m)? <= 71 => {
                self.draw(2, 24, 8, 0x1c8, 0)?;
            }
            id @ (6 | 7) if y(self.m)? <= if id == 6 { 91 } else { 95 } => {
                let dx = self.m.read_u32(0x020326ec)? as i32 / 8;
                let dy = self.m.read_u32(0x02032708)? as i32;
                if id == 6 {
                    self.draw(2, 12 + dx, dy.wrapping_add(14), 0x1c8, 0)?;
                } else {
                    let slot = u32::from(self.m.read_u16(0x0203250c)?) * 4;
                    self.draw(2, 25 + dx, 33i32.wrapping_sub(dy), slot, 0)?;
                }
            }
            _ => {}
        }
        if y(self.m)? > 240 {
            self.draw(0, 88, 400, 0x18c, 2048)?;
        }
        if y(self.m)? > 135 {
            self.draw(16, 56, 295, 20, 0)?;
            self.draw(16, 144, 295, 24, 0)?;
        }
        let p = state(self.m)?;
        let xx = (self.m.read_u32(p + 0x90)? >> 8) as u16;
        let yy = (self.m.read_u32(p + 0x94)? >> 8) as u16;
        let special = ((227..=234).contains(&xx) && (148..=288).contains(&yy))
            || ((149..=222).contains(&xx)
                && (54..=226).contains(&yy)
                && (self.m.read_u16(p + 40)? != 0 || self.m.read_u16(0x020329d4)? != 0));
        let pri = if special { 128 } else { 1 };
        for (lo, hi, order, x, yy, src, flags) in [
            (i16::MIN, 125, 1, 143, 62, 0xc4, 0),
            (-13, 209, pri, 232, 146, 0xc8, 0),
            (i16::MIN, 150, pri, 151, 94, 0x13c, 0),
            (51, 273, pri, 232, 210, 0x140, 0),
            (i16::MIN, 166, pri + 1, 178, 127, 0xb4, 0),
            (i16::MIN, 93, 2, 140, 62, 16, 0),
            (i16::MIN, 104, 2, 179, 73, 16, 32),
            (i16::MIN, 132, 2, 141, 101, 16, 64),
            (185, 407, 4, 232, 344, 0xa0, 0),
        ] {
            if (lo..=hi).contains(&y(self.m)?) {
                self.draw(order, x, yy, src, flags)?;
            }
        }
        let n = 0x02031af0;
        let d = 0x02031aec;
        let v = self.m.read_u32(n)?;
        if (v as i32) <= 10 && self.m.read_u32(d)? == 0 {
            word(self.m, n, v.wrapping_add(1))?;
        } else {
            word(self.m, d, 1)?;
        }
        let v = self.m.read_u32(n)?;
        if (v as i32) >= -10 && self.m.read_u32(d)? == 1 {
            word(self.m, n, v.wrapping_sub(1))
        } else {
            word(self.m, d, 0)
        }
    }
}
fn word(m: M<'_>, a: u32, v: u32) -> R {
    m.write_u16(a, v as u16)?;
    m.write_u16(a + 2, (v >> 16) as u16)
}
pub struct TableObjectCalls<'a>(pub &'a mut dyn LifecycleCalls);
impl LifecycleCalls for TableObjectCalls<'_> {
    fn call(&mut self, m: M<'_>, off: u32, args: &[u32]) -> R {
        if off != 0x1fc30 {
            return self.0.call(m, off, args);
        }
        if !args.is_empty() {
            return Err(FrameError::Dependency {
                call: NativeCall {
                    entry_offset: off,
                    argument0: args.first().copied(),
                },
                reason: "invalid table object arguments".into(),
            });
        }
        Scene { m, calls: self.0 }.run()
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
        let v: Value = serde_json::from_str(vectors!("table_objects-vectors.json")).unwrap();
        let cases = v["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 216);
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
            TableObjectCalls(&mut calls)
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
