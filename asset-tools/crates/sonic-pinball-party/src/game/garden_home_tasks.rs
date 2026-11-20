//! Native Tiny Garden object callbacks. Sprite records use the original local
//! tile IDs and part placement tables; the renderer consumes the ordered list.
use super::frame::{FrameError as Error, FrameMemory, NativeCall};
pub use super::garden_home::Calls;
type Mem<'a> = &'a mut dyn FrameMemory;
const BASE: u32 = 0x02000000;
const SAVE: u32 = 0x03004aa0;
const FLAGS: u32 = 0x0300425f;
const FRAME: u32 = 0x03004258;
fn fail(off: u32, t: u32, why: &str) -> Error {
    Error::Dependency {
        call: NativeCall {
            entry_offset: super::garden_graphics::diagnostic_address(off),
            argument0: Some(t),
        },
        reason: why.into(),
    }
}
fn word(m: Mem<'_>, a: u32, v: u32) -> Result<(), Error> {
    m.write_bytes(a, &v.to_le_bytes())
}
fn step(m: Mem<'_>, a: u32, v: i8) -> Result<u8, Error> {
    let n = m.read_u8(a)?.wrapping_add_signed(v);
    m.write_u8(a, n)?;
    Ok(n)
}
fn parent(m: &dyn FrameMemory, t: u32) -> Result<u32, Error> {
    let id = m.read_u8(t + 0x1b)?;
    if id > 71 {
        return Err(fail(0x6a54, t, "parent task ID outside native pool"));
    }
    Ok(0x03005320 + 36 * u32::from(id))
}
fn draw(m: Mem<'_>, t: u32, priority: u8, c: &mut dyn Calls) -> Result<(), Error> {
    let id = u32::from(m.read_u8(t + 4)?);
    c.invoke(m, 0x1398, &[id, u32::from(priority)])?;
    Ok(())
}
fn animation(m: Mem<'_>, t: u32, period: u8) -> Result<(u8, bool), Error> {
    let trigger = step(m, t + 0x22, 1)? >= period;
    if trigger {
        m.write_u8(t + 0x22, 0)?;
        step(m, t + 0x23, 1)?;
    }
    Ok((m.read_u8(t + 0x23)?, trigger))
}
fn pattern(m: &dyn FrameMemory, n: u8) -> Result<u8, Error> {
    m.read_u8(0x02020681 + u32::from(n & 3))
}
fn direction(d: u8) -> u8 {
    match d {
        2..=4 => 1,
        5..=7 => 2,
        8..=10 => 3,
        _ => 0,
    }
}
fn body(m: Mem<'_>, t: u32, p: u32, c: &mut dyn Calls) -> Result<(), Error> {
    let flags = m.read_u8(FLAGS)?;
    if flags & 128 == 0 {
        m.write_u8(t + 7, m.read_u8(p + 7)?)?;
        m.write_u16(t + 10, m.read_u16(p + 10)?)?;
        m.write_u16(t + 14, u16::from(m.read_u8(p + 0x18)?))?;
        m.write_u8(t + 0x1d, m.read_u8(p + 0x1d)?)?;
        let state = m.read_u8(p + 0x17)?;
        let off = if (1..=63).contains(&state) {
            m.read_u32(0x02006ad4 + 4 * u32::from(state - 1))?
                .wrapping_sub(BASE)
        } else {
            0x6bd0
        };
        let phase = m.read_u8(p + 0x19)?;
        let dir = direction(m.read_u8(p + 0x16)?);
        let clock = m.read_u32(FRAME)?;
        let mut orientation = 1;
        let tile = match off {
            0x6bd0 => 27,
            0x6bd4 => 40,
            0x6bd8 => {
                m.write_u8(t + 0x23, 0)?;
                m.write_u8(t + 0x22, 0)?;
                0
            }
            0x6bea => {
                let (n, _) = animation(m, t, 12)?;
                45 + pattern(m, n)?
            }
            0x6c20 | 0x6c40 | 0x6c96 | 0x6eb8 => {
                orientation = u8::from(m.read_u8(t + 7)? & 128 == 0);
                match off {
                    0x6c20 => {
                        m.write_u8(t + 0x22, 48)?;
                        36
                    }
                    0x6c40 => {
                        if step(m, t + 0x22, 1)? & 48 == 0 {
                            38
                        } else {
                            37
                        }
                    }
                    0x6c96 => {
                        if phase == 0 {
                            37
                        } else {
                            58 + ((phase - 1) & 1)
                        }
                    }
                    _ => 28,
                }
            }
            0x6c6a => {
                let n = m.read_u8(m.read_u32(0x0300150c)? + 0x1a)?;
                49 + pattern(m, ((u16::from(n) + 16) >> 5) as u8)?
            }
            0x6c8c => 39 + (phase & 1),
            0x6cba | 0x6e46 | 0x6fc4 | 0x6dee | 0x6dac => {
                let d = if off == 0x6e46 {
                    m.read_u8(p + 0x1a)?.wrapping_add(32) >> 6
                } else {
                    dir
                };
                if d == 0 {
                    orientation = 0;
                }
                let mut d = if d == 0 { 2 } else { d };
                if off == 0x6dac {
                    if d == 3 {
                        d = 1;
                    }
                    26 + d
                } else if off == 0x6dee {
                    (d - 1) * 3
                } else {
                    let (n, trigger) = animation(m, t, if off == 0x6fc4 { 10 } else { 18 })?;
                    if off != 0x6fc4 && trigger && n & 3 == 0 && flags & 3 == 0 {
                        c.invoke(m, 0x1066c, &[15])?;
                    }
                    (d - 1) * 3 + pattern(m, n)? + if off == 0x6fc4 { 9 } else { 0 }
                }
            }
            0x6d44 => {
                orientation = u8::from(dir != 0);
                let v = m.read_u8(p + 7)? & 127;
                m.write_u8(p + 7, v | if dir == 0 { 128 } else { 0 })?;
                if clock & 127 == 32 && flags & 3 == 0 {
                    c.invoke(m, 0x1066c, &[20])?;
                }
                if clock & 0x60 == 0 {
                    53
                } else {
                    54
                }
            }
            0x6e2c => 48,
            0x6e30 => {
                if phase == 0 {
                    48
                } else {
                    66
                }
            }
            0x6e3e => 0,
            0x6e42 => 44,
            0x6ecc | 0x6ede => (if off == 0x6ecc { 62 } else { 64 }) + u8::from(clock & 24 != 0),
            0x6ef0 => {
                if phase == 0 {
                    0
                } else {
                    43 + (phase & 1)
                }
            }
            0x6f0a => match phase {
                1 | 6 | 7 => 60,
                2..=5 => 61,
                _ => 0,
            },
            0x6f4c => match phase {
                3 | 5 | 7 => 30,
                4 | 6 => 31,
                _ => 29,
            },
            0x6f8c | 0x6fa6 => match phase {
                1 => {
                    if off == 0x6f8c {
                        41
                    } else {
                        32
                    }
                }
                2 | 3 => {
                    if off == 0x6f8c {
                        42
                    } else {
                        33
                    }
                }
                _ => 0,
            },
            0x6fc0 => 52,
            0x703c | 0x706a | 0x70a0 => {
                let base = match phase {
                    32..=96 => 0,
                    97..=159 => 3,
                    160..=224 => 6,
                    _ => {
                        orientation = 0;
                        3
                    }
                };
                if off == 0x706a {
                    base + 1
                } else if off == 0x70a0 {
                    let (n, _) = animation(m, t, 4)?;
                    base + pattern(m, n)?
                } else {
                    base
                }
            }
            0x7108 => {
                if m.read_u8(p + 0x1a)? == 0 {
                    m.write_u8(p + 0x1a, 1)?;
                    m.write_u8(t + 0x23, 0)?;
                    m.write_u8(t + 0x22, 0)?;
                }
                let n = m.read_u8(t + 0x23)?;
                let n = if n <= 11 { animation(m, t, 2)?.0 } else { n };
                if n > 11 && state == 60 {
                    0
                } else {
                    45 + pattern(m, n)?
                }
            }
            0x716c => {
                let (n, _) = animation(m, t, 8)?;
                if clock & 64 != 0 {
                    orientation = 0;
                }
                34 + (n & 1)
            }
            0x71a8 => {
                let (n, _) = animation(m, t, 10)?;
                if m.read_u16(p + 0x22)? as i16 >= 0 {
                    orientation = 0;
                }
                55 + pattern(m, n)?
            }
            _ => return Err(fail(off, t, "invalid body action target")),
        };
        word(
            m,
            t + 0x10,
            if orientation != 0 {
                0x0201fb20
            } else {
                0x0201fb40
            },
        )?;
        m.write_u16(t + 0x14, 420 + 9 * u16::from(tile))?;
        m.write_u8(t + 0x19, tile)?;
        m.write_u8(t + 0x1e, orientation)?;
    }
    if m.read_u8(t + 7)? & 1 == 0 {
        let z = m.read_u8(t + 0x1d)?;
        draw(m, t, z, c)?;
    }
    Ok(())
}
fn component(m: Mem<'_>, t: u32, off: u32, c: &mut dyn Calls) -> Result<(), Error> {
    let flags = m.read_u8(FLAGS)?;
    if flags & 16 != 0 {
        return Ok(());
    }
    if off == 0x8a74 {
        if m.read_u8(0x03001506)? != 0 {
            let p = parent(m, t)?;
            m.write_u16(t + 14, m.read_u16(p + 14)?.wrapping_add(1))?;
            let z = m.read_u8(p + 0x1d)?;
            draw(m, t, z, c)?;
        }
        return Ok(());
    }
    if off == 0x8ab8 && m.read_u8(0x03001506)? != 0 {
        return Ok(());
    }
    if m.read_u8(0x0300152e)? == 0 {
        c.invoke(m, 0xed4, &[t])?;
        return Ok(());
    }
    let p = parent(m, t)?;
    if off == 0x6a54 {
        return body(m, t, p, c);
    }
    if off == 0x8ab8 {
        m.write_u16(t + 10, m.read_u16(p + 10)?)?;
        m.write_u16(t + 14, u16::from(m.read_u8(p + 0x1d)?))?;
        if m.read_u8(p + 7)? & 1 == 0 {
            draw(m, t, 0, c)?;
        }
        return Ok(());
    }
    let rec = 0x0201fc00 + 5 * u32::from(m.read_u8(p + 0x19)?);
    let orientation = m.read_u8(p + 0x1e)?;
    let clock = m.read_u32(FRAME)?;
    if off == 0x7240 {
        if flags & 128 == 0 {
            m.write_u8(t + 0x17, 0)?;
            let pose = match m.read_u8(p + 7)? & 0x78 {
                64 => 1,
                32 => 2,
                16 => {
                    if clock & 16 == 0 {
                        4
                    } else {
                        3
                    }
                }
                8 => {
                    let state = m.read_u8(m.read_u32(0x03001510)? + 0x17)?;
                    if (state == 61 && orientation == 0)
                        || (state == 57
                            && (orientation == 0 || m.read_u16(p + 0x14)?.wrapping_sub(474) <= 18))
                    {
                        m.write_u8(t + 0x17, 16)?;
                    }
                    if clock & 8 == 0 {
                        6
                    } else {
                        5
                    }
                }
                _ => 0,
            };
            word(m, t + 0x10, 0x0201fb68 + 8 * pose)?;
            let mut dx = m.read_u8(rec)? as i8;
            let dy = m.read_u8(rec + 1)? as i8;
            if orientation == 0 {
                dx = dx.wrapping_neg().wrapping_sub(6);
            }
            for (d, src, dst, scale) in [(dx, 10, 8, 8192), (dy, 14, 0x20, 16384)] {
                let delta = m
                    .read_u16(p + src)?
                    .wrapping_add_signed(i16::from(d))
                    .wrapping_sub(m.read_u16(t + dst + 2)?) as i16;
                let n = m
                    .read_u32(t + dst)?
                    .wrapping_add_signed(i32::from(delta) * scale);
                word(m, t + dst, n)?;
            }
            let ang = m.read_u8(t + 0x19)?.wrapping_add(0xc0);
            let sine = m.read_u16(0x0201f2e0 + 2 * u32::from(ang))? as i16;
            m.write_u16(
                t + 14,
                m.read_u16(t + 0x22)?.wrapping_add_signed(sine / 128),
            )?;
            step(m, t + 0x19, 4)?;
        }
    } else if off == 0x73a8 {
        if flags & 128 == 0 {
            if m.read_u8(p + 7)? & 4 != 0 {
                m.write_u8(t + 7, 1)?;
            } else if m.read_u8(t + 7)? == 0 {
                let limit = if m.read_u8(t + 0x19)? & 1 == 0 {
                    180
                } else {
                    11
                };
                if step(m, t + 0x1a, 1)? >= limit {
                    m.write_u8(t + 0x1a, 0)?;
                    m.write_u8(t + 7, 1)?;
                }
                return Ok(());
            } else if step(m, t + 0x1a, 1)? > 4 {
                m.write_u8(t + 0x1a, 0)?;
                m.write_u8(t + 7, 0)?;
                step(m, t + 0x19, 1)?;
            }
        }
        let mut dx = m.read_u8(rec + 2)? as i8;
        if dx == 0 {
            return Ok(());
        }
        let dy = m.read_u8(rec + 3)? as i8;
        let mut pose = m.read_u8(rec + 4)?;
        if orientation == 0 {
            dx = dx.wrapping_neg();
            pose = u8::from(pose != 0);
        }
        m.write_u16(
            t + 10,
            m.read_u16(p + 10)?.wrapping_add_signed(i16::from(dx)),
        )?;
        m.write_u16(
            t + 14,
            m.read_u16(p + 14)?.wrapping_add_signed(i16::from(dy)),
        )?;
        word(m, t + 0x10, 0x0201fba0 + 8 * u32::from(pose))?;
        if m.read_u8(t + 7)? == 0 {
            return Ok(());
        }
    }
    if m.read_u8(p + 7)? & 1 == 0 {
        let z = m.read_u8(p + 0x1d)?;
        draw(m, t, z, c)?;
    }
    Ok(())
}
pub fn dispatch(m: Mem<'_>, c: &mut dyn Calls, off: u32, args: &[u32]) -> Result<u32, Error> {
    let [t] = *args else {
        return Err(fail(off, 0, "expected home task pointer"));
    };
    if !(0x03000000..=0x03007fdc).contains(&t) {
        return Err(fail(off, t, "home task outside IWRAM"));
    }
    match off {
        0x6a54 | 0x7240 | 0x73a8 | 0x8a74 | 0x8ab8 => component(m, t, off, c)?,
        0x35cc => {
            let idx = 2 * u32::from(m.read_u8(t + 0x1a)?);
            m.write_u16(t + 10, u16::from(m.read_u8(SAVE + 0x37 + idx)?))?;
            let y = m.read_u8(SAVE + 0x38 + idx)?;
            m.write_u8(t + 0x1d, y)?;
            m.write_u16(t + 14, u16::from(y))?;
            word(m, t + 0x10, 0x0201fd50)?;
            m.write_u16(t + 0x14, 265)?;
            m.write_u8(t + 0x18, 28)?;
            m.write_u8(t + 0x1c, 3)?;
            word(m, t, 0x02003615)?;
        }
        0x74c0 => {
            m.write_u8(t + 0x1d, 19)?;
            m.write_u16(t + 14, 19)?;
            word(m, t + 0x10, 0x0201fda0)?;
            m.write_u16(t + 0x14, 269)?;
            let other = m.read_u8(t + 0x1a)? != 0;
            m.write_u16(t + 10, if other { 124 } else { 156 })?;
            m.write_u8(t + 0x18, if other { 204 } else { 188 })?;
            m.write_u8(t + 0x1c, 4)?;
            word(m, t, 0x02008b1d)?;
        }
        0x89b0 => {
            m.write_u8(t + 7, 128)?;
            let n = m.read_u32(0x03006194)?.wrapping_add(1);
            word(m, t + 0x20, n)?;
            m.write_u8(t + 0x19, 6)?;
            m.write_u8(t + 0x1a, 1)?;
            word(m, t, 0x0200292d)?;
        }
        0x8a04 => {
            m.write_u16(t + 10, 95)?;
            m.write_u16(t + 14, 58)?;
            word(m, t + 0x10, 0x0201fd60)?;
            m.write_u16(t + 0x14, 172)?;
            m.write_u16(t + 0x22, 384)?;
            m.write_u16(t + 0x20, 271)?;
            word(m, t, 0x02003749)?;
        }
        0x89d4 => {
            m.write_u16(t + 10, 88)?;
            m.write_u16(t + 14, 0xffd0)?;
            word(m, t + 0x10, 0x0201fe30)?;
            m.write_u16(t + 0x14, 331)?;
            m.write_u8(t + 0x18, 12)?;
            m.write_u8(t + 0x1f, 0)?;
            m.write_u8(t + 0x1e, 0)?;
            word(m, t, 0x02002eb9)?;
        }
        0x218c => {
            let flags = m.read_u8(FLAGS)?;
            if flags & 127 == 0 {
                if (m.read_u8(t + 0x19)? == 0 || step(m, t + 0x19, -1)? == 0)
                    && m.read_u16(0x03005314)? & 8 != 0
                {
                    c.invoke(m, 0x107a0, &[])?;
                    c.invoke(m, 0x1066c, &[14])?;
                    m.write_u16(t + 14, 0)?;
                    m.write_u8(t + 0x19, 8)?;
                    word(m, t, 0x020021f5)?;
                    m.write_u8(FLAGS, m.read_u8(FLAGS)? | 128)?;
                } else {
                    m.write_u8(FLAGS, m.read_u8(FLAGS)? & 127)?;
                }
            }
        }
        0x88b0 => {
            let phase = u32::from(step(m, t + 0x1a, -1)?);
            c.invoke(m, 0x1698, &[84, phase])?;
            c.invoke(
                m,
                0x1698,
                &[82, phase | ((16u32.wrapping_sub(phase) & 254) << 7)],
            )?;
            if m.read_u8(t + 0x1a)? == 0 {
                c.invoke(m, 0xed4, &[t])?;
                m.write_u8(FLAGS, m.read_u8(FLAGS)? & !1)?;
            }
        }
        0x3424 => spawn_object(m, t, c)?,
        0x8b1c => {
            if m.read_u8(FLAGS)? & 16 == 0 {
                let z = m.read_u8(t + 0x1d)?;
                draw(m, t, z, c)?;
            }
        }
        0x84ec => item_frame(m, t, c)?,
        0x3eb4 => egg_frame(m, t, c)?,
        0x3e04 | 0x83e4 => {
            let egg = off == 0x3e04;
            let k = u32::from(m.read_u8(t + 0x1a)?);
            if !egg && k >= 3 {
                return Err(fail(off, t, "invalid item index"));
            }
            let a = if egg {
                SAVE + 0x1f
            } else {
                SAVE + 0x31 + 2 * k
            };
            let x = m.read_u8(a)?;
            let y = m.read_u8(a + 1)?.min(159);
            m.write_u16(t + 10, u16::from(x))?;
            m.write_u8(a + 1, y)?;
            m.write_u8(t + 0x1d, y)?;
            m.write_u16(t + 14, u16::from(y))?;
            if egg {
                word(m, t + 0x10, 0x0201fd50)?;
                m.write_u16(t + 0x14, if k == 0 { 216 } else { 220 })?;
                m.write_u8(t + 0x18, 236)?;
                m.write_u8(t + 0x1c, 1)?;
                lower_to_ground(m, t, a + 1)?;
                m.write_u8(t + 0x19, 0)?;
                m.write_u16(t + 8, 1800)?;
                m.write_u8(0x030015ad, k as u8)?;
                m.write_u8(0x030015ac, 1)?;
                word(m, t, 0x02003eb5)?;
                egg_frame(m, t, c)?;
            } else {
                let data = m.read_u32(0x0201272c + 4 * k)?;
                word(m, t + 0x10, data)?;
                m.write_u16(t + 0x14, 224)?;
                m.write_u8(t + 0x18, (m.read_u8(0x0202066f + k)? << 4) | 12)?;
                m.write_u8(t + 0x1c, k as u8 + 5)?;
                if k == 1 {
                    if m.read_u8(0x020216a0 + u32::from(y >> 3) * 22 + u32::from(x >> 3))? == 0 {
                        m.write_u8(a, 64)?;
                        m.write_u16(t + 10, 64)?;
                        m.write_u8(a + 1, 159)?;
                        m.write_u8(t + 0x1d, 159)?;
                        m.write_u16(t + 14, 159)?;
                    }
                    m.write_u16(t + 0x22, m.read_u16(t + 10)?)?;
                } else {
                    lower_to_ground(m, t, a + 1)?;
                }
                word(m, t, 0x020084ed)?;
                item_frame(m, t, c)?;
            }
        }
        0x292c => hud(m, t, c)?,
        0x3748 => cursor(m, t, c)?,
        0x3614 | 0x36ec => ground_object(m, t, c, off)?,
        0x3c24 | 0x3d58 => interact(m, t, c, off)?,
        0x85e0 | 0x8648 | 0x8d90 => remove_arc(m, t, c, off)?,
        _ => return Err(fail(off, t, "unported home task")),
    }
    Ok(0)
}
fn matrix_slot(m: Mem<'_>) -> Result<u8, Error> {
    let count = m.read_u8(0x03001140)?;
    if count > 31 {
        return Ok(255);
    }
    let head = m.read_u8(0x03001141)?;
    m.write_u8(0x03001141, head.wrapping_add(1) & 31)?;
    let id = m.read_u8(0x03001120 + u32::from(head))?;
    m.write_u8(0x03001140, count + 1)?;
    Ok(id)
}
fn lower_to_ground(m: Mem<'_>, t: u32, save_y: u32) -> Result<(), Error> {
    let y = m.read_u16(t + 14)?;
    let mut cell = (y >> 3) * 22 + (m.read_u16(t + 10)? >> 3);
    let initial = cell;
    while m.read_u8(0x020216a0 + u32::from(cell))? != 0 {
        cell = cell.wrapping_sub(22);
        m.write_u16(t + 14, m.read_u16(t + 14)?.wrapping_sub(8))?;
        if cell == initial {
            return Err(fail(0x84b0, t, "no reachable ground tile"));
        }
    }
    let y = m.read_u16(t + 14)? as u8;
    m.write_u8(t + 0x1d, y)?;
    m.write_u8(save_y, y)
}
fn item_frame(m: Mem<'_>, t: u32, c: &mut dyn Calls) -> Result<(), Error> {
    let flags = m.read_u8(FLAGS)?;
    if flags & 16 != 0 {
        return Ok(());
    }
    if flags & 128 == 0 {
        let frame = m.read_u32(FRAME)?;
        match m.read_u8(t + 0x1a)? {
            1 => {
                let x = m.read_u16(t + 0x22)? as i16;
                if x != 64 && frame.wrapping_add(57) & 127 == 0 {
                    let v = x.wrapping_add(if x > 64 { -1 } else { 1 });
                    m.write_u16(t + 0x22, v as u16)?;
                    m.write_u8(SAVE + 0x33, v as u8)?;
                }
                let y = m.read_u8(t + 0x1d)?;
                if y != 159 && frame.wrapping_add(155) & 63 == 0 {
                    let v = step(m, t + 0x1d, if y > 159 { -1 } else { 1 })?;
                    m.write_u8(SAVE + 0x34, v)?;
                }
                let trig = |phase: u8| 0x0201f2e0 + 2 * u32::from(phase);
                let vx = (m.read_u16(trig(frame as u8))? as i16) / 129;
                let vy = (m.read_u16(trig((frame as u8).wrapping_mul(2).wrapping_add(192)))?
                    as i16)
                    / 129;
                m.write_u16(t + 10, m.read_u16(t + 0x22)?.wrapping_add_signed(vx))?;
                m.write_u16(
                    t + 14,
                    u16::from(m.read_u8(t + 0x1d)?).wrapping_add_signed(vy),
                )?;
                m.write_u8(SAVE + 0xcf, 1)?;
            }
            2 => {
                word(m, 0x03001514, t)?;
                word(
                    m,
                    t + 0x10,
                    if frame & 8 != 0 {
                        0x0201fd78
                    } else {
                        0x0201fd80
                    },
                )?;
            }
            _ => {}
        }
    }
    let z = m.read_u8(t + 0x1d)?;
    draw(m, t, z, c)
}
fn egg_frame(m: Mem<'_>, t: u32, c: &mut dyn Calls) -> Result<(), Error> {
    let flags = m.read_u8(FLAGS)?;
    if flags & 16 != 0 {
        return Ok(());
    }
    if m.read_u8(SAVE + 0x59)? == 255 {
        c.invoke(m, 0x85e0, &[t])?;
        return Ok(());
    }
    if m.read_u8(t + 0x19)? > 2 && m.read_u16(t + 8)? > 60 {
        m.write_u16(t + 8, 60)?;
    }
    if flags & 0xe3 == 0 {
        if m.read_u8(0x0300152f)? != 0 && m.read_u8(SAVE + 0x58)? != 0 {
            let n = m.read_u16(t + 8)?.wrapping_sub(1);
            m.write_u16(t + 8, n)?;
            if n == 0 {
                word(m, t + 0x10, 0x0201fd58)?;
                let s = matrix_slot(m)?;
                m.write_u8(t + 0x17, s)?;
                for o in [0x1f, 0x1e, 0x19] {
                    m.write_u8(t + o, 0)?;
                }
                m.write_u8(t + 0x1c, 255)?;
                word(m, t, 0x02003f69)?;
                c.invoke(m, 0x3f68, &[t])?;
                c.invoke(m, 0x1066c, &[33])?;
                return Ok(());
            }
        } else {
            m.write_u8(t + 0x19, 0)?;
            m.write_u16(t + 8, 1800)?;
        }
    }
    let z = m.read_u8(t + 0x1d)?;
    draw(m, t, z, c)
}
fn spawn_object(m: Mem<'_>, t: u32, c: &mut dyn Calls) -> Result<(), Error> {
    if m.read_u8(FLAGS)? & 0xf3 != 0 || m.read_u8(0x0300159b)? > 15 {
        return Ok(());
    }
    let timer = m.read_u32(SAVE + 16)?.wrapping_sub(1);
    word(m, SAVE + 16, timer)?;
    if timer != 0 {
        return Ok(());
    }
    let rng = |m: &mut dyn FrameMemory| -> Result<u32, Error> {
        let seed = m
            .read_u32(0x030013a8)?
            .wrapping_mul(0x41c64e6d)
            .wrapping_add(12345);
        word(m, 0x030013a8, seed)?;
        Ok((seed >> 16) & 0x7fff)
    };
    let delay = (rng(m)? % 30 + 60) * 60;
    word(m, SAVE + 16, delay)?;
    let mut coord = None;
    for _ in 0..65536 {
        let x = rng(m)? % 160 + 8;
        let y = rng(m)? % 136 + 24;
        let tile = m.read_u16(0x02021330 + 2 * ((y >> 3) * 22 + (x >> 3)))?;
        if tile == 9 || tile == 13 {
            coord = Some((x as u8, y as u8));
            break;
        }
        rng(m)?;
    }
    let (x, y) = coord.ok_or_else(|| fail(0x3462, t, "no legal spawn location found"))?;
    let mut slot = 0;
    while slot < 16 && m.read_u8(SAVE + 0x37 + 2 * slot)? != 0 {
        slot += 1;
    }
    let child = c.invoke(m, 0xd04, &[0x02003535, t, 0])?;
    m.write_u8(SAVE + 0x37 + 2 * slot, x)?;
    m.write_u16(child + 10, u16::from(x))?;
    m.write_u8(SAVE + 0x38 + 2 * slot, y)?;
    m.write_u8(child + 0x1d, y)?;
    word(m, child + 0x10, 0x0201fd50)?;
    m.write_u16(child + 0x14, 265)?;
    m.write_u8(child + 0x16, 1)?;
    let s = matrix_slot(m)?;
    m.write_u8(child + 0x17, s)?;
    m.write_u8(child + 0x18, 28)?;
    m.write_u8(child + 0x1a, slot as u8)?;
    m.write_u8(child + 0x1f, 0)?;
    step(m, 0x0300159b, 1)?;
    Ok(())
}
const BARS: u32 = 0x03001558;
fn queue(
    m: Mem<'_>,
    c: &mut dyn Calls,
    s: u32,
    d: u32,
    w: u32,
    h: u32,
    base: u32,
) -> Result<(), Error> {
    c.invoke(m, 0x17b0, &[s, d, w, h, base])?;
    Ok(())
}
fn bar(m: Mem<'_>, c: &mut dyn Calls, i: u32) -> Result<bool, Error> {
    let a = BARS + 8 * i;
    let mut increased = false;
    if i < 2 || m.read_u8(a + 4)? != 0 {
        let value = u32::from(m.read_u8(a + 3)?);
        let current = if i < 2 {
            (value + 10) / 20
        } else {
            m.write_u8(a + 4, 0)?;
            value.div_ceil(10)
        };
        let prev = u32::from(m.read_u8(a + 5)?);
        if current != prev {
            queue(
                m,
                c,
                0x0201f6e0 + 14 * current,
                0x0600f142 + 128 * i,
                7,
                1,
                if i < 2 { 0x211a } else { 0x111a },
            )?;
            increased = current > prev;
            m.write_u8(a + 5, current as u8)?;
        }
    }
    if i >= 2 && m.read_u8(a + 2)? != 0 {
        let blank = step(m, a + 2, -1)? & 8 != 0;
        queue(
            m,
            c,
            if blank { 0 } else { a },
            0x0600f10e + 128 * i,
            1,
            1,
            if blank { 225 } else { 124 },
        )?;
    }
    Ok(increased)
}
fn number_tiles(m: Mem<'_>, c: &mut dyn Calls, t: u32) -> Result<(), Error> {
    let rings = m.read_u32(0x03006194)?;
    let mut displayed = m.read_u32(t + 0x20)?;
    let diff = rings.wrapping_sub(displayed) as i32;
    if diff == 0 {
        return Ok(());
    }
    word(m, SAVE + 0x64, rings.swap_bytes())?;
    let mut amount = diff.wrapping_abs();
    for i in 0..5 {
        let a = 0x02020654 + 4 * i;
        if amount >= i32::from(m.read_u16(a)?) {
            let s = i32::from(m.read_u16(a + 2)?);
            amount = amount.wrapping_sub(s);
            displayed = displayed.wrapping_add_signed(if diff < 0 { -s } else { s });
        }
    }
    word(m, t + 0x20, displayed)?;
    let mut digits = [0u8; 5];
    let mut number = displayed as i32;
    for d in digits.iter_mut().rev() {
        if number <= 9 {
            *d = (number as u8).wrapping_add(1);
            break;
        }
        *d = (number % 10) as u8 + 1;
        number /= 10;
    }
    for (i, d) in digits.into_iter().enumerate() {
        m.write_u16(
            0x03001590 + 2 * i as u32,
            if d == 0 {
                if i == 0 {
                    257
                } else {
                    225
                }
            } else {
                0x2141 + u16::from(d)
            },
        )?;
    }
    queue(m, c, 0x03001590, 0x0600f4c6, 5, 1, 0)
}
fn hud(m: Mem<'_>, t: u32, c: &mut dyn Calls) -> Result<(), Error> {
    if step(m, t + 0x1a, -1)? == 0 {
        m.write_u8(t + 0x1a, 6)?;
        let mut phase = step(m, t + 0x19, 1)?;
        if phase > 5 {
            phase = 0;
            m.write_u8(t + 0x19, 0)?;
        }
        queue(
            m,
            c,
            0x0201f77a + 8 * u32::from(phase),
            0x0600f482,
            2,
            2,
            0x712a,
        )?;
    }
    number_tiles(m, c, t)?;
    let selected = m.read_u8(0x0300152f)?;
    if selected != m.read_u8(t + 7)? {
        m.write_u8(t + 7, selected)?;
        if selected == 255 {
            queue(m, c, 0, 0x0600f042, 7, 2, 225)?;
        } else {
            for i in 0..7 {
                m.write_u16(
                    0x03001538 + 2 * i,
                    0x2af + u16::from(m.read_u8(SAVE + 0x6a + i)?),
                )?;
            }
            queue(m, c, 0x03001538, 0x0600f042, 7, 1, 0)?;
            let mut kind = u32::from(m.read_u8(SAVE + 0x71)?.min(19));
            if kind != 0 {
                kind += 2;
            }
            let row =
                0x0201faae + (kind / 3) * 7 + if m.read_u8(0x03004260)? != 0 { 56 } else { 0 };
            for i in 0..7 {
                let ch = m.read_u8(row + i)?;
                m.write_u16(
                    0x03001548 + 2 * i,
                    if ch == 0 {
                        0x1e68 + (kind % 3) as u16
                    } else {
                        u16::from(ch)
                    },
                )?;
            }
            queue(m, c, 0x03001548, 0x0600f082, 7, 1, 0x2af)?;
        }
        for i in 0..7 {
            let a = BARS + 8 * i;
            if selected == 255 {
                m.write_u8(a + 3, 0)?;
                m.write_u16(a, 0)?;
            } else if i < 2 {
                m.write_u8(a + 3, m.read_u8(SAVE + 0x73 + i)?)?;
            } else {
                m.write_u16(a, u16::from(m.read_u8(SAVE + 0x86 + i)?))?;
                m.write_u8(a + 3, m.read_u8(SAVE + 0x76 + i)?)?;
            }
            m.write_u8(a + 4, 1)?;
            m.write_u8(a + 2, 1)?;
            m.write_u8(a + 5, 255)?;
            bar(m, c, i)?;
        }
        return Ok(());
    }
    if selected == 255 {
        return Ok(());
    }
    let flags = m.read_u8(FLAGS)?;
    if flags & 0x95 == 0 {
        let cursor = m.read_u32(0x0300150c)?;
        let x = m.read_u16(cursor + 10)? as i16;
        if x > 181 {
            let highlight = x > 186
                && (m.read_u16(cursor + 14)? as i16) < 8
                && flags & 32 == 0
                && m.read_u8(0x03001509)? == 0;
            queue(
                m,
                c,
                0x03001538,
                0x0600f042,
                7,
                1,
                if highlight { 0xa000 } else { 0 },
            )?;
            if highlight
                && m.read_u16(0x03005314)? & 1 != 0
                && m.read_u16(0x03005310)? & 0xf0 == 0
                && m.read_u8(FLAGS)? & 2 == 0
            {
                c.invoke(m, 0x1066c, &[12])?;
                word(m, 0x03004254, 0x02008e0d)?;
                word(m, 0x03004250, 0x02008849)?;
                m.write_u8(FLAGS, m.read_u8(FLAGS)? | 3)?;
            }
        }
    }
    if m.read_u8(FLAGS)? & 0xd3 != 0 {
        return Ok(());
    }
    if m.read_u8(0x03001500)? != 0 {
        step(m, 0x03001500, -1)?;
    }
    for i in 0..7 {
        let a = BARS + 8 * i;
        let prev = m.read_u8(a + 3)?;
        if i < 2 {
            let target = m.read_u8(SAVE + 0x73 + i)?;
            if target == prev {
                continue;
            }
            step(m, a + 3, if target > prev { 1 } else { -1 })?;
        } else if m.read_u32(FRAME)? & 1 != 0 {
            let level = m.read_u16(a)?;
            if u16::from(m.read_u8(SAVE + 0x86 + i)?) > level {
                if step(m, a + 3, 1)? > 99 {
                    m.write_u8(a + 3, 0)?;
                    m.write_u16(a, level.wrapping_add(1))?;
                    m.write_u8(a + 2, 120)?;
                    c.invoke(m, 0x1066c, &[35])?;
                    m.write_u8(0x03001500, 72)?;
                }
                m.write_u8(a + 4, 1)?;
            } else {
                let target = m.read_u8(SAVE + 0x76 + i)?;
                if target != prev {
                    step(m, a + 3, if target > prev { 1 } else { -1 })?;
                    m.write_u8(a + 4, 1)?;
                }
            }
        }
        if bar(m, c, i)? && m.read_u8(0x03001500)? == 0 {
            c.invoke(m, 0x1066c, &[22])?;
        }
    }
    Ok(())
}
fn ground_object(m: Mem<'_>, t: u32, c: &mut dyn Calls, off: u32) -> Result<(), Error> {
    let flags = m.read_u8(FLAGS)?;
    if flags & 16 != 0 {
        return Ok(());
    }
    if flags & 128 == 0 {
        if off == 0x3614 {
            let phase = m.read_u8(t + 0x19)?;
            if phase == 1 {
                let a = c.invoke(m, 0xd04, &[0x020036ed, t, 0])?;
                m.write_u16(a + 10, m.read_u16(t + 10)?)?;
                let y = m.read_u16(t + 14)? as u8;
                m.write_u8(a + 0x1d, y)?;
                m.write_u16(a + 14, u16::from(y))?;
                word(m, a + 0x10, 0x0201fd68)?;
                m.write_u16(a + 0x14, 398)?;
                m.write_u8(a + 0x18, 140)?;
                m.write_u8(t + 0x1b, m.read_u8(a + 4)?)?;
                step(m, t + 0x19, 1)?;
            } else if phase == 3 || phase == 4 {
                if phase == 3 {
                    m.write_u8(parent(m, t)? + 7, 1)?;
                    step(m, t + 0x19, 1)?;
                }
                if m.read_u8(t + 7)? != 0 {
                    m.write_u8(SAVE + 0x37 + 2 * u32::from(m.read_u8(t + 0x1a)?), 0)?;
                    step(m, 0x0300159b, -1)?;
                    c.invoke(m, 0xed4, &[t])?;
                    return Ok(());
                }
            }
        } else if m.read_u8(t + 7)? != 0 && step(m, t + 0x19, 1)? > 9 {
            m.write_u8(t + 0x19, 0)?;
            let tile = m.read_u16(t + 0x14)?.wrapping_add(2);
            m.write_u16(t + 0x14, tile)?;
            if tile > 403 {
                c.invoke(m, 0xed4, &[t])?;
                return Ok(());
            }
        }
    }
    let z = m.read_u8(t + 0x1d)?;
    draw(m, t, z, c)
}
fn cursor(m: Mem<'_>, t: u32, c: &mut dyn Calls) -> Result<(), Error> {
    let flags = m.read_u8(FLAGS)?;
    let held = m.read_u8(t + 7)? != 0;
    if flags & 0x50 != 0 || (flags & 32 != 0 && !held) || m.read_u8(0x03001509)? != 0 {
        return Ok(());
    }
    m.write_u8(t + 0x18, 20)?;
    let id = m.read_u8(t + 4)?;
    c.invoke(m, 0x1254, &[u32::from(id), 1])?;
    if m.read_u8(FLAGS)? & 0x83 != 0 {
        return Ok(());
    }
    let keys = m.read_u16(0x03005310)? & 0xf0;
    if keys != 0 {
        m.write_u16(t + 0x14, if held { 176 } else { 172 })?;
        if held {
            m.write_u8(0x0300152c, 1)?;
        }
        let mut v = [0i32; 2];
        for (j, (a, max)) in [(0x22, 896), (0x20, 633)].into_iter().enumerate() {
            let n = ((u32::from(m.read_u16(t + a)?) * 261) >> 8) as u16;
            let n = n.min(max);
            m.write_u16(t + a, n)?;
            v[j] = i32::from(n);
        }
        let (a, b) = (v[0], v[1]);
        let (dx, dy) = match keys {
            16 => (a, 0),
            32 => (-a, 0),
            64 => (0, -a),
            128 => (0, a),
            0x50 => (b, -b),
            0x60 => (-b, -b),
            0x90 => (b, b),
            0xa0 => (-b, b),
            _ => (0, 0),
        };
        for (pos, delta, min, max) in [
            (8, dx, 16, if held { 160 } else { 223 }),
            (12, dy, 4, if held { 140 } else { 143 }),
        ] {
            let n = m.read_u32(t + pos)?.wrapping_add_signed(delta * 256);
            word(m, t + pos, n)?;
            let coord = (n >> 16) as i16;
            if coord < min || coord > max {
                m.write_u16(t + pos + 2, coord.clamp(min, max) as u16)?;
            }
        }
        return Ok(());
    }
    m.write_u16(t + 0x22, 384)?;
    m.write_u16(t + 0x20, 271)?;
    if held {
        m.write_u16(t + 0x14, 176)?;
        m.write_u8(0x0300152c, 1)?;
        return Ok(());
    }
    let x = m.read_u16(t + 10)? as i16;
    let y = m.read_u16(t + 14)? as i16;
    if m.read_u16(0x03005314)? & 1 == 0 || x <= i16::from(m.read_u8(0x03001528)? as i8) {
        return Ok(());
    }
    for i in (0..u32::from(m.read_u8(0x03006190)?)).rev() {
        let a = 0x03005320 + 36 * u32::from(m.read_u8(0x03005d40 + i)?);
        let k = m.read_u8(a + 0x1c)?;
        if k == 255 {
            continue;
        }
        let bounds = 0x0201fe68 + 4 * u32::from(k);
        let ax = m.read_u16(a + 10)?;
        let ay = m.read_u16(a + 14)?;
        let mut inside = true;
        for (v, pos, b) in [
            (x.wrapping_sub(8), ax, bounds),
            (y.wrapping_add(8), ay, bounds + 2),
        ] {
            let lo = (pos as i16).wrapping_add(i16::from(m.read_u8(b)? as i8));
            let hi = lo.wrapping_add(i16::from(m.read_u8(b + 1)? as i8));
            if v < lo || v > hi {
                inside = false;
            }
        }
        if !inside {
            continue;
        }
        let gid = m.read_u8(a + 4)?;
        match k {
            4 if m.read_u8(FLAGS)? & 4 == 0 => {
                c.invoke(m, 0x10804, &[0x030072e0, 1])?;
                c.invoke(m, 0x1066c, &[12])?;
                let next = if m.read_u8(a + 0x1a)? != 0 {
                    0x0200d295
                } else {
                    0x0200a8a5
                };
                word(m, 0x03004254, next)?;
                word(m, 0x03004250, 0x0200882d)?;
                m.write_u8(FLAGS, m.read_u8(FLAGS)? | 3)?;
            }
            3 => {
                c.invoke(m, 0x1066c, &[21])?;
                m.write_u16(t + 10, ax)?;
                m.write_u16(t + 14, ay.wrapping_sub(16))?;
                m.write_u8(t + 0x1d, m.read_u8(a + 0x1d)?.wrapping_add(1))?;
                m.write_u16(t + 0x14, 176)?;
                m.write_u8(t + 0x1a, 0)?;
                m.write_u8(t + 0x19, 0)?;
                m.write_u8(t + 0x1b, gid)?;
                word(m, t, 0x02003c25)?;
                m.write_u8(0x0300152c, 1)?;
            }
            1 if m.read_u8(0x0300152f)? != 0 && m.read_u8(SAVE + 0x58)? != 0 => {
                m.write_u16(t + 10, ax)?;
                m.write_u8(t + 0x1f, 17)?;
                m.write_u16(t + 14, ay.wrapping_sub(17))?;
                m.write_u8(t + 0x1d, m.read_u8(a + 0x1d)?.wrapping_add(1))?;
                m.write_u16(t + 0x14, 184)?;
                for o in [0x1e, 0x1a, 0x19] {
                    m.write_u8(t + o, 0)?;
                }
                m.write_u8(t + 0x1b, gid)?;
                word(m, t, 0x02003d59)?;
                step(m, a + 0x19, 1)?;
                m.write_u8(0x0300152c, 1)?;
            }
            1 | 2 | 5..=7 => {
                c.invoke(m, 0x1066c, &[21])?;
                m.write_u8(t + 7, 1)?;
                m.write_u16(t + 10, ax)?;
                m.write_u16(t + 14, ay.wrapping_sub(19))?;
                word(
                    m,
                    a,
                    match k {
                        1 => 0x02008cd5,
                        2 => 0x02008c9d,
                        _ => 0x02008d05,
                    },
                )?;
                m.write_u8(0x0300152c, 1)?;
                m.write_u16(0x03005314, m.read_u16(0x03005314)? & !1)?;
            }
            0 => {
                let chao = m.read_u32(0x03001510)?;
                if m.read_u8(chao + 0x1e)? & 2 == 0 {
                    continue;
                }
                m.write_u8(t + 0x1e, 0)?;
                m.write_u8(t + 0x1f, 23)?;
                m.write_u8(t + 0x1d, m.read_u8(a + 0x1d)?.wrapping_add(1))?;
                m.write_u16(t + 0x14, 184)?;
                m.write_u8(t + 0x1a, 0)?;
                m.write_u8(t + 0x19, 0)?;
                m.write_u8(t + 0x1b, gid)?;
                word(m, t, 0x02003d59)?;
                match m.read_u8(chao + 0x17)? {
                    34 => m.write_u8(chao + 0x19, 60)?,
                    23 => {
                        m.write_u8(t + 0x1f, 19)?;
                        m.write_u8(
                            t + 0x1e,
                            if m.read_u8(chao + 7)? & 128 != 0 {
                                2
                            } else {
                                254
                            },
                        )?;
                        if m.read_u8(chao + 0x19)? == 0 {
                            m.write_u8(chao + 0x19, 60)?;
                        }
                    }
                    _ => {
                        m.write_u8(chao + 7, 0)?;
                        m.write_u8(chao + 0x17, 16)?;
                    }
                }
                m.write_u16(
                    t + 10,
                    ax.wrapping_add_signed(i16::from(m.read_u8(t + 0x1e)? as i8)),
                )?;
                m.write_u16(t + 14, ay.wrapping_sub(u16::from(m.read_u8(t + 0x1f)?)))?;
                m.write_u8(0x0300152c, 1)?;
            }
            _ => continue,
        }
        break;
    }
    Ok(())
}
fn trig(m: &dyn FrameMemory, angle: u8, sine: bool) -> Result<i16, Error> {
    let i = if sine { angle.wrapping_add(192) } else { angle };
    Ok(m.read_u16(0x0201f2e0 + 2 * u32::from(i))? as i16)
}
fn interact(m: Mem<'_>, t: u32, c: &mut dyn Calls, off: u32) -> Result<(), Error> {
    m.write_u8(0x0300152c, 1)?;
    if m.read_u8(FLAGS)? & 128 == 0 {
        let p = parent(m, t)?;
        let a = m.read_u8(t + 0x1a)?;
        if off == 0x3d58 {
            if matches!(a, 0 | 66 | 129) {
                c.invoke(m, 0x1066c, &[19])?;
            }
            let x = m
                .read_u16(p + 10)?
                .wrapping_sub((trig(m, a.wrapping_mul(2), true)? / 65) as u16)
                .wrapping_add_signed(i16::from(m.read_u8(t + 0x1e)? as i8));
            let y = m
                .read_u16(p + 14)?
                .wrapping_sub((trig(m, a.wrapping_mul(4), false)? / 192) as u16)
                .wrapping_sub(i16::from(m.read_u8(t + 0x1f)? as i8) as u16);
            m.write_u16(t + 10, x)?;
            m.write_u16(t + 14, y)?;
            if step(m, t + 0x1a, 3)? > 191 {
                word(m, t, 0x02003749)?;
            }
        } else {
            let phase = m.read_u8(t + 0x19)?;
            let advance = match phase {
                0 | 3 => {
                    let advance = step(m, t + 0x1a, 1)? > 7;
                    if advance && phase == 0 {
                        step(m, p + 0x19, 1)?;
                    }
                    advance
                }
                1 | 4 => {
                    for q in [t, p] {
                        m.write_u16(q + 14, m.read_u16(q + 14)?.wrapping_sub(1))?;
                    }
                    step(m, t + 0x1a, 1)? != 0
                }
                2 | 5 => {
                    let advance = step(m, t + 0x1a, 1)? > 15;
                    if advance {
                        if phase == 2 {
                            for q in [t, p] {
                                m.write_u16(q + 14, m.read_u16(q + 14)?.wrapping_add(1))?;
                            }
                        } else {
                            c.invoke(m, 0x1066c, &[13])?;
                            step(m, p + 0x19, 1)?;
                        }
                    }
                    advance
                }
                6 => {
                    let y = u16::from(m.read_u8(p + 0x1d)?)
                        .wrapping_sub((trig(m, a, true)? >> 4) as u16);
                    m.write_u16(p + 14, y)?;
                    m.write_u16(t + 14, y.wrapping_sub(16))?;
                    let advance = step(m, t + 0x1a, 5)? > 63;
                    if advance {
                        m.write_u16(t + 0x14, 180)?;
                    }
                    advance
                }
                7 => {
                    if step(m, t + 0x1a, 1)? > 11 {
                        word(m, t, 0x02003749)?;
                        m.write_u8(p + 7, 1)?;
                    }
                    false
                }
                _ => false,
            };
            if advance {
                m.write_u8(t + 0x1a, 0)?;
                step(m, t + 0x19, 1)?;
            }
        }
    }
    m.write_u8(t + 0x18, 28)?;
    let z = m.read_u8(t + 0x1d)?;
    draw(m, t, z, c)
}
fn remove_arc(m: Mem<'_>, t: u32, c: &mut dyn Calls, entry: u32) -> Result<(), Error> {
    if entry == 0x85e0 {
        m.write_u8(t + 0x1c, 255)?;
        m.write_u8(t + 0x1d, 159)?;
        let x = i32::from(m.read_u16(t + 10)? as i16);
        m.write_u16(t + 0x22, ((x - 88) * 4) as u16)?;
        let y = m.read_u16(t + 14)?;
        let next = if (y as i16) < 104 {
            m.write_u16(t + 12, y)?;
            m.write_u16(t + 0x20, 104u16.wrapping_sub(y))?;
            m.write_u8(t + 0x1a, 0)?;
            0x8648
        } else {
            m.write_u16(t + 0x20, y.wrapping_sub(46))?;
            m.write_u8(t + 0x1a, 64)?;
            0x8d90
        };
        word(m, t, BASE + next + 1)?;
        remove_arc(m, t, c, next)?;
        c.invoke(m, 0x1066c, &[32])?;
        return Ok(());
    }
    let upper = entry == 0x8648;
    let a = step(m, t + 0x1a, if upper { 2 } else { 1 })?;
    if a > 128 {
        c.invoke(m, 0xed4, &[t])?;
        return Ok(());
    }
    let x = m
        .read_u32(t + 8)?
        .wrapping_sub((i32::from(m.read_u16(t + 0x22)? as i16) * 256) as u32);
    word(m, t + 8, x)?;
    let base = if upper { m.read_u16(t + 12)? } else { 46 };
    let dy = i32::from(trig(m, a, true)?) * i32::from(m.read_u16(t + 0x20)? as i16) / 256;
    m.write_u16(t + 14, base.wrapping_add(dy as u16))?;
    if upper && a == 64 {
        m.write_u16(t + 12, 46)?;
        m.write_u16(t + 0x20, 58)?;
    }
    let z = m.read_u8(t + 0x1d)?;
    draw(m, t, z, c)?;
    m.write_u8(0x0300152c, 1)
}

#[cfg(test)]
mod tests {
    use super::super::{
        garden_payload::{BoardCalls, NativeBoardCalls},
        garden_services,
        memory::{MemoryImage, Region},
    };
    use super::*;
    use serde_json::{json, Value};
    use std::io::Read;
    struct Record(Vec<Value>);
    impl garden_services::Calls for Record {
        fn invoke(&mut self, m: Mem<'_>, o: u32, a: &[u32]) -> Result<u32, Error> {
            Calls::invoke(self, m, o, a)
        }
    }
    impl Calls for Record {
        fn invoke(&mut self, m: Mem<'_>, o: u32, a: &[u32]) -> Result<u32, Error> {
            self.0.push(json!([o, a]));
            match o {
                0x1398 | 0xed4 | 0x17b0 | 0x1254 => garden_services::dispatch(m, self, o, a),
                0xd04 | 0x1698 => Ok(NativeBoardCalls.invoke(BASE + o, a, m).unwrap()),
                0x1066c | 0x107a0 | 0x10804 => Ok(0),
                _ => Err(fail(o, 0, "missing native test callback")),
            }
        }
    }
    fn run(payload: Option<&[u8]>) {
        let d: Value = serde_json::from_str(vectors!("garden-home-tasks-vectors.json")).unwrap();
        let u = |v: &Value| v.as_u64().unwrap() as u32;
        for case in d["cases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let e = u(&case["entry"]);
            if payload.is_none()
                && !matches!(
                    e,
                    0x8a74
                        | 0x8ab8
                        | 0x8a04
                        | 0x89d4
                        | 0x89b0
                        | 0x218c
                        | 0x35cc
                        | 0x74c0
                        | 0x88b0
                        | 0x8b1c
                )
            {
                continue;
            }
            let mut regions = vec![Region {
                base: 0x03000000,
                bytes: vec![0; 0x7000],
                writable: true,
            }];
            if let Some(p) = payload {
                regions.push(Region {
                    base: BASE,
                    bytes: p.to_vec(),
                    writable: false,
                });
            }
            let mut m = MemoryImage::fixture(regions);
            for p in d["defaults"]
                .as_array()
                .unwrap()
                .iter()
                .chain(case["patches"].as_array().unwrap())
            {
                m.write_bytes(u(&p[0]), &u(&p[2]).to_le_bytes()[..u(&p[1]) as usize])
                    .unwrap();
            }
            let mut c = Record(Vec::new());
            for (i, tick) in case["trace"].as_array().unwrap().iter().enumerate() {
                c.0.clear();
                dispatch(&mut m, &mut c, e, &[0x03005320 + 36 * 4])
                    .unwrap_or_else(|e| panic!("{name}/{i}: {e:?}"));
                assert_eq!(c.0, *tick[1].as_array().unwrap(), "{name}/{i} calls");
                assert_eq!(
                    crate::sha256(m.bytes(0x03000000, 0x7000).unwrap()),
                    tick[0],
                    "{name}/{i}"
                );
            }
        }
    }
    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn home_tasks_without_assets() {
        run(None);
    }
    #[test]
    #[ignore = "requires authenticated local SPP_ROM"]
    fn original_parts_and_home_tasks() {
        let mut rom = Vec::new();
        std::fs::File::open(std::env::var_os("SPP_ROM").unwrap())
            .unwrap()
            .take(0x800001)
            .read_to_end(&mut rom)
            .unwrap();
        crate::catalog::validate_rom(&rom).unwrap();
        let p = crate::binary::decode(&rom[0xd0970..], crate::garden::PAYLOAD_LENGTH)
            .unwrap()
            .bytes;
        run(Some(&p));
    }
}
