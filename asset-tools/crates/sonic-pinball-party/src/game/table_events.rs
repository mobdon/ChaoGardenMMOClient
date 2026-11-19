//! Native spatial events. Rectangle and layer-map reads use the verified A3VE ROM.
use super::{
    frame::{FrameError, FrameMemory},
    lifecycle::LifecycleCalls,
};
type M<'a> = &'a mut dyn FrameMemory;
type C<'a> = &'a mut dyn LifecycleCalls;
type R = Result<(), FrameError>;
const G: u32 = 0x02032980;
const F: u32 = 0x020329d0;
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
fn half(m: &dyn FrameMemory, a: u32) -> Result<i32, FrameError> {
    Ok(i32::from(m.read_u16(a)? as i16))
}
fn word(m: M<'_>, a: u32, v: i32) -> R {
    m.write_u16(a, v as u16)?;
    m.write_u16(a + 2, (v >> 16) as u16)
}
fn rect(
    m: &dyn FrameMemory,
    a: u32,
    x: i32,
    y: i32,
    signed: bool,
) -> Result<Option<(u32, u32)>, FrameError> {
    let bound = |off| -> Result<i32, FrameError> {
        let w = m.read_u16(a + off)?;
        Ok(if signed {
            i32::from(w as i16)
        } else {
            i32::from(w)
        })
    };
    let l = bound(0)?;
    if x < l || x > bound(2)? {
        return Ok(None);
    }
    let t = bound(4)?;
    if y < t || y > bound(6)? {
        return Ok(None);
    }
    Ok(Some(((x - l) as u32, (y - t) as u32)))
}
fn vel(m: M<'_>, s: u32, x: u16, y: u16) -> R {
    m.write_u16(s + 0xa8, x)?;
    m.write_u16(s + 0xaa, y)
}
// The original uses X twice for the half-scale mirrors and writes only A4.
fn project(m: M<'_>, s: u32) -> R {
    let x = half(m, s + 0xa0)?;
    let y = half(m, s + 0xa2)?;
    m.write_u16(s + 0xa4, y as u16)?;
    word(m, s + 0x90, x << 7)?;
    word(m, s + 0x94, y << 7)?;
    for off in [0x98, 0x9a, 0x9c, 0x9e] {
        m.write_u16(s + off, (x / 2) as u16)?;
    }
    Ok(())
}
fn enter(m: M<'_>, s: u32, table3: bool) -> R {
    m.write_u16(s + 0x8c, 0)?;
    m.write_u8(s + 0x14, 0)?;
    vel(m, s, 0, if table3 { 128 } else { 0 })?;
    m.write_u16(s + 0xa0, 2)?;
    if !table3 {
        m.write_u16(s + 0xa2, 98)?;
    }
    project(m, s)?;
    m.write_u8(s + 3, 1)
}
fn leave(m: M<'_>, s: u32, clamp: bool) -> R {
    if m.read_u16(s + 0x8c)? == 0 {
        project(m, s)?;
    }
    if clamp && half(m, s + 0x98)? < 0 {
        m.write_u16(s + 0xa0, 2)?;
        m.write_u16(s + 0xa4, 2)?;
        word(m, s + 0x90, 256)?;
    }
    m.write_u16(s + 0x8c, 1)?;
    m.write_u8(s + 3, 0)?;
    m.write_u8(s + 0x14, 1)
}
pub fn dispatch(m: M<'_>, c: C<'_>) -> R {
    let s = ptr(m)?;
    match m.read_u16(0x03003068)? {
        0 | 1 => table01(m, c, s)?,
        2 => table2(m, c, s)?,
        3 => table3(m, s)?,
        _ => (),
    }
    let s = ptr(m)?;
    m.write_u16(s + 0x6c, m.read_u8(s + 6)? as i8 as i16 as u16)
}
fn table01(m: M<'_>, c: C<'_>, s: u32) -> R {
    if m.read_u16(s + 0x28)? != 0 {
        return Ok(());
    }
    let x = half(m, s + 0x98)?;
    let y = half(m, s + 0x9a)?;
    let vx = half(m, s + 0xa8)?;
    let vy = half(m, s + 0xaa)?;
    for i in 0..6 {
        if rect(m, 0x086e93a8 + i * 8, x, y, false)?.is_none() {
            continue;
        }
        let (off, v) = match i {
            0 if vy < 0 => (6, 1),
            1 if vy > 0 => (6, 0),
            2 if vy < 0 => {
                m.write_u16(G, 1)?;
                (2, 1)
            }
            3 if vy > 0 => (2, 0),
            4 if m.read_u16(s + 0x6e)? != 10 && vy < 0 => {
                m.write_u16(F + 4, 0)?;
                m.write_u16(G, 2)?;
                m.write_u8(s + 6, 1)?;
                continue;
            }
            5 if vy > 0 => (4, 0),
            _ => continue,
        };
        m.write_u16(F + off, v)?;
        m.write_u8(s + 6, v as u8)?;
    }
    if m.read_u8(s + 6)? == 0 {
        return Ok(());
    }
    for i in 0..10 {
        if rect(m, 0x086e93d8 + i * 8, x, y, true)?.is_none() {
            continue;
        }
        let s = ptr(m)?;
        match i {
            0 if vy > 0 => m.write_u16(F + 4, 1)?,
            1 if vx > 0 => m.write_u16(F + 4, 1)?,
            2 => enter(m, s, false)?,
            3 => leave(m, s, true)?,
            4 | 5 => {
                vel(m, s, 0, 0)?;
                for a in [2, 4, 6] {
                    m.write_u16(F + a, 0)?;
                }
                m.write_u8(s + 6, 0)?;
            }
            6 => m.write_u16(G, 1)?,
            7..=9 => {
                let stage = i - 7;
                let g = 0x020355e0;
                if m.read_u32(g)? != 1 + (stage << 16)
                    || (stage == 2 && m.read_u16(0x02032968)? != 0)
                    || m.read_u8(s + 3)? != 0
                {
                    continue;
                }
                vel(m, s, 0, 0)?;
                if m.read_u16(s + 0x6e)? != 0 || m.read_u16(0x02035fa0)? != 0 {
                    m.write_u16(s + 0xb8, 1)?;
                    m.write_u16(s + 0x8c, 0)?;
                    m.write_u16(g, 2)?;
                    m.write_u16(g + 4, 0)?;
                } else {
                    m.write_u16(g + 2, m.read_u16(g + 2)?.wrapping_add(1))?;
                    if stage == 2 {
                        c.call(m, 0x3de48, &[])?;
                    } else {
                        m.write_u8(s + 3, 1)?;
                        m.write_u16(s + 0x8c, 0)?;
                        c.call(m, 0x3046c, &[0x080afb34, 8, 60])?;
                        c.call(m, 0x2f9b4, &[0x080afb48 + stage * 32, 2, 60])?;
                        c.call(m, 0x2dff8, &[])?;
                        m.write_u16(ptr(m)? + 0x28, 1)?;
                    }
                }
            }
            _ => (),
        }
    }
    Ok(())
}
fn table3(m: M<'_>, s: u32) -> R {
    if m.read_u16(s + 0x28)? != 0 {
        return Ok(());
    }
    let x = half(m, s + 0x98)?;
    let y = half(m, s + 0x9a)?;
    let vy = half(m, s + 0xaa)?;
    for i in 0..4 {
        if let Some((dx, dy)) = rect(m, 0x086e9478 + 8 * i, x, y, false)? {
            if i == 0 || i == 2 {
                let col = if i == 0 { dx } else { 36u32.wrapping_sub(dx) };
                let a = 0x086e94b8u32.wrapping_add(2u32.wrapping_mul(col.wrapping_add(37 * dy)));
                m.write_u8(s + 6, m.read_u16(a)? as u8)?;
            }
        }
    }
    if m.read_u8(s + 6)? != 1 {
        return Ok(());
    }
    for i in 0..4 {
        if rect(m, 0x086e9498 + i * 8, x, y, false)?.is_none() {
            continue;
        }
        match i {
            0 | 1 if vy > 0 => {
                vel(m, s, 0, 0)?;
                m.write_u8(s + 6, 0)?;
            }
            2 => enter(m, s, true)?,
            3 => leave(m, s, false)?,
            _ => (),
        }
    }
    Ok(())
}
fn table2(m: M<'_>, c: C<'_>, s: u32) -> R {
    let x = half(m, s + 0x98)?;
    let y = half(m, s + 0x9a)?;
    let vy = half(m, s + 0xaa)?;
    if (5..=41).contains(&x)
        && (233..=301).contains(&y)
        && m.read_u8(s + 6)? == 1
        && m.read_u16(0x020360f2)? == 0
    {
        c.call(m, 0x468a8, &[1, 1])?;
    }
    let s = ptr(m)?;
    if m.read_u16(s + 0x28)? != 0 {
        return Ok(());
    }
    let n = m.read_u16(s + 0x10)?;
    if (m.read_u8(s + 6)? != 0 || half(m, s + 0x9a)? > 208) && half(m, s + 0x98)? > 226 {
        if n != 12 {
            m.write_u16(s + 0x10, n.wrapping_add(1))?;
        }
    } else if n != 4 {
        m.write_u16(s + 0x10, (n.wrapping_sub(1) as i16).max(4) as u16)?;
    }
    for i in 0..6 {
        let Some((dx, dy)) = rect(m, 0x086e9428 + i * 8, x, y, false)? else {
            continue;
        };
        match i {
            0..=3 => {
                if i < 2 && m.read_u16(F + 12)? != 0 {
                    continue;
                }
                let base = if i < 2 { 0x080afb87 } else { 0x080b00d0 };
                let v = m.read_u8(base + dx + dy * 41)?;
                m.write_u8(s + 6, v)?;
                let bit = u16::from(v == 1);
                m.write_u16(F + 8, bit)?;
                m.write_u16(F + 10, bit)?;
                if bit != 0 {
                    m.write_u16(G + 8, 1)?;
                    m.write_u16(G + 10, u16::from(i >= 2 || m.read_u16(F)? == 0))?;
                }
            }
            4 if m.read_u16(s + 0x6e)? != 10 && vy < 0 => {
                m.write_u16(G + 6, 1)?;
                m.write_u16(F + 12, 1)?;
                m.write_u8(s + 6, 1)?;
            }
            5 if vy > 0 => {
                m.write_u16(G + 6, 0)?;
                m.write_u8(s + 6, 0)?;
                m.write_u16(F + 12, 0)?;
            }
            _ => (),
        }
    }
    for i in 0..4 {
        if rect(m, 0x086e9458 + i * 8, x, y, false)?.is_none() {
            continue;
        }
        let s = ptr(m)?;
        match i {
            0 if m.read_u8(s + 6)? != 0 && vy > 0 => m.write_u16(G + 8, 0)?,
            1 if m.read_u8(s + 6)? != 0 => {
                vel(m, s, 0, 0)?;
                m.write_u8(s + 6, 0)?;
                m.write_u16(F + 12, 0)?;
            }
            2 if m.read_u16(0x020360f2)? == 0 => c.call(m, 0x468a8, &[1, 1])?,
            3 if half(m, s + 0xaa)? < 0 && m.read_u16(0x020360f2)? != 2 => {
                m.write_u16(0x0203645e, 1)?;
                c.call(m, 0x468a8, &[1, 2])?;
            }
            _ => (),
        }
    }
    for (xs, ys, a) in [
        (66..=74, 76..=88, 0x02036458),
        (46..=59, 96..=106, 0x0203645a),
    ] {
        if xs.contains(&x) && ys.contains(&y) && m.read_u8(ptr(m)? + 6)? == 0 && m.read_u16(a)? == 0
        {
            m.write_u16(a, 1)?;
        }
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
            serde_json::from_str(include_str!("../../tests/table_events-vectors.json")).unwrap();
        let cases = v["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 354);
        for (i, case) in cases.iter().enumerate() {
            let mut m = Mem::default();
            m.patches(&v["common"]);
            m.patches(&case["init"]);
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
    #[ignore = "requires local verified A3VE_ROM"]
    fn retail_maps_and_regions_match_native_digest() {
        use sha2::{Digest, Sha256};
        let rom = std::fs::read(std::env::var_os("A3VE_ROM").unwrap()).unwrap();
        let v: Value =
            serde_json::from_str(include_str!("../../tests/table_events-vectors.json")).unwrap();
        assert_eq!(format!("{:x}", Sha256::digest(&rom)), v["romSha256"]);
        let mut bank = Mem::default();
        for (start, len) in [
            (0x6e93a8, 0x110),
            (0x6e94b8, 37 * 29 * 2),
            (0xafb87, 41 * 33),
            (0xb00d0, 41 * 33),
        ] {
            for (i, &b) in rom[start..start + len].iter().enumerate() {
                bank.put(0x08000000 + (start + i) as u32, 1, b.into());
            }
        }
        let mut digest = Sha256::new();
        let mut idx = 0;
        for (base, count) in [
            (0x086e93a8, 6),
            (0x086e93d8, 10),
            (0x086e9428, 6),
            (0x086e9458, 4),
            (0x086e9478, 4),
            (0x086e9498, 4),
        ] {
            for i in 0..count {
                let a = base + i * 8;
                let [l, r, t, b] = [0, 2, 4, 6].map(|off| half(&bank, a + off).unwrap());
                for k in 0..8 {
                    let case = &v["cases"][idx];
                    idx += 1;
                    let mut m = bank.clone();
                    m.patches(&case["init"]);
                    let x = [l, r, (l + r) >> 1, l - 1, r + 1, l, l + 1, (l + r) >> 1][k];
                    let y = [t, b, (t + b) >> 1, t, t, t + 2, t + 1, (t + b) >> 1][k];
                    m.put(0x02000098, 2, x as u32);
                    m.put(0x0200009a, 2, y as u32);
                    let before = m.clone();
                    let mut probe = Probe {
                        calls: vec![],
                        mutations: &case["mutations"],
                    };
                    dispatch(&mut m, &mut probe).unwrap();
                    let diff: Vec<_> =
                        m.0.range(..0x08000000)
                            .filter(|(a, b)| before.0.get(a).copied().unwrap_or(0) != **b)
                            .map(|(a, b)| json!([a, b]))
                            .collect();
                    digest.update(serde_json::to_vec(&json!([probe.calls, diff])).unwrap());
                    digest.update(b"\n");
                }
            }
        }
        assert_eq!(format!("{:x}", digest.finalize()), v["retailDigest"]);
    }
}
