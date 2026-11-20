//! A3VE Garden ball-puzzle callbacks, dispatched on the original task records.
use super::{
    frame::{FrameError, FrameMemory, NativeCall},
    garden_shop::Calls,
};
type Mem<'a> = &'a mut dyn FrameMemory;
type R<T = ()> = Result<T, FrameError>;
const P: u32 = 0x02000000;
const G: u32 = 0x03004250;
const K: u32 = 0x03005310;
const MODE: u32 = 0x03001ae6;
const BOARD: u32 = 0x03001ae8;
const ROUND: u32 = 0x03001ad1;
const TICKS: u32 = 0x030029f0;
fn word(m: Mem, a: u32, v: u32) -> R {
    m.write_bytes(a, &v.to_le_bytes())
}
fn parent(m: &dyn FrameMemory, t: u32) -> R<u32> {
    Ok(0x03005320 + 36 * u32::from(m.read_u8(t + 0x1b)?))
}
fn allocate(m: Mem, c: &mut impl Calls, t: u32, offset: u32) -> R<u32> {
    c.invoke(m, 0xd04, &[P + offset + 1, t, 0])
}
fn draw(m: Mem, c: &mut impl Calls, t: u32, offset: u32, layer: u32) -> R {
    let id = m.read_u8(t + 4)?;
    c.invoke(m, offset, &[id.into(), layer])?;
    Ok(())
}
fn decrement(m: Mem, a: u32) -> R<u8> {
    let value = m.read_u8(a)?.wrapping_sub(1);
    m.write_u8(a, value)?;
    Ok(value)
}
fn increment(m: Mem, a: u32) -> R<u8> {
    let value = m.read_u8(a)?.wrapping_add(1);
    m.write_u8(a, value)?;
    Ok(value)
}
fn pause(m: Mem, c: &mut impl Calls, t: u32) -> R {
    if m.read_u8(G + 15)? & 0x11 != 0 {
        return Ok(());
    }
    if m.read_u8(t + 0x1a)? != 0 {
        decrement(m, t + 0x1a)?;
    }
    let flags = m.read_u8(G + 15)?;
    let pressed = m.read_u16(K + 4)?;
    if flags & 128 == 0 {
        if pressed & 8 != 0 && m.read_u8(t + 0x1a)? == 0 {
            m.write_u8(t + 0x19, 0)?;
            m.write_u8(t + 0x1a, 4)?;
            m.write_u8(G + 15, flags | 128)?;
            c.invoke(m, 0x107a0, &[])?;
            c.invoke(m, 0x1066c, &[14])?;
        }
    } else if pressed & 11 != 0 && m.read_u8(t + 0x1a)? == 0 {
        let cancel = m.read_u16(K)? & 2 != 0;
        if cancel || m.read_u8(t + 0x19)? == 0 {
            m.write_u8(G + 15, flags & 127)?;
            c.invoke(m, 0x107d8, &[])?;
        } else {
            m.write_u8(G + 15, flags | 2)?;
            word(m, G + 4, 0)?;
        }
        m.write_u8(t + 0x1a, 4)?;
        c.invoke(m, 0x1066c, &[if cancel { 13 } else { 12 }])?;
        m.write_u16(K + 4, m.read_u16(K + 4)? & 0xfffc)?;
    } else if pressed & 64 != 0 && m.read_u8(t + 0x19)? != 0 {
        m.write_u8(t + 0x19, 0)?;
        c.invoke(m, 0x1066c, &[11])?;
    } else if pressed & 128 != 0 && m.read_u8(t + 0x19)? == 0 {
        m.write_u8(t + 0x19, 1)?;
        c.invoke(m, 0x1066c, &[11])?;
    }
    Ok(())
}
fn pause_sprite(m: Mem, c: &mut impl Calls, t: u32, off: u32) -> R {
    if m.read_u8(G + 15)? & 128 == 0 {
        return Ok(());
    }
    m.write_u16(t + 0xa, 68)?;
    m.write_u16(t + 0xe, 56)?;
    if off == 0xf7dc {
        word(m, t + 0x10, P + 0x2dc34)?;
        m.write_u16(t + 0x14, 0)?;
        m.write_u8(t + 0x18, 16)?;
    } else if matches!(off, 0xd9a8 | 0xda04) {
        word(m, t + 0x10, P + 0x2dc94)?;
        let tile = if off == 0xd9a8 {
            9 + ((m.read_u32(G + 8)? >> 2) & 4) as u16
        } else {
            41
        };
        m.write_u16(t + 0x14, tile)?;
        m.write_u8(t + 0x18, 16)?;
        if (m.read_u8(parent(m, t)? + 0x19)? != 0) == (off == 0xd9a8) {
            m.write_u16(t + 0xe, 72)?
        }
    } else {
        m.write_u16(t + 0x14, 0)?;
        let alt = m.read_u8(ROUND - 1)? != 0;
        let offset = if off == 0xda58 { 0x2dc9c } else { 0x2dccc };
        word(m, t + 0x10, P + offset + if alt { 24 } else { 0 })?;
        let selected = m.read_u8(parent(m, t)? + 0x19)? != 0;
        m.write_u8(t + 0x18, if selected == (off == 0xda58) { 16 } else { 32 })?;
    }
    draw(m, c, t, 0x1254, 0)
}
fn score(m: Mem, c: &mut impl Calls) -> R {
    let n = m.read_u16(0x03001ad2)?;
    if n == m.read_u16(0x03001ad4)? {
        return Ok(());
    }
    m.write_u16(0x03001ad4, n)?;
    let mut n = m.read_u16(0x03001ad2)?;
    let mut digits = [0u16; 3];
    for digit in digits.iter_mut().rev() {
        *digit = 1 + n % 10;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    for (i, d) in digits.into_iter().enumerate() {
        let ptr = 0x03001ad8 + 2 * i as u32;
        m.write_u16(ptr, 2 * d)?;
        m.write_u16(ptr + 6, 2 * d + 1)?;
    }
    c.invoke(m, 0x17b0, &[0x03001ad8, 0x0600d0b4, 3, 2, 0x61a0])?;
    Ok(())
}
fn setup(m: Mem, c: &mut impl Calls, t: u32, off: u32) -> R {
    match off {
        0xf6bc => {
            allocate(m, c, t, 0xf7dc)?;
            for ptr in [0xd9a8, 0xda04, 0xda58, 0xdac0] {
                let child = allocate(m, c, t, ptr)?;
                m.write_u8(child + 0x1b, m.read_u8(t + 4)?)?;
            }
            word(m, t, P + 0xd8a5)?;
        }
        0xdfbc => {
            let child = allocate(m, c, t, 0xde24)?;
            m.write_u8(child + 0x1b, m.read_u8(t + 4)?)?;
            m.write_u16(child + 0xe, 159)?;
            m.write_u8(child + 0x18, 12)?;
            m.write_u8(child + 0x1a, 0)?;
            m.write_u8(t + 0x1b, m.read_u8(child + 4)?)?;
            m.write_u16(t + 0xa, 104)?;
            m.write_u16(0x030029f6, 104)?;
            word(m, t, P + 0xdba9)?;
            m.write_u8(t + 0x19, 6)?;
            m.write_u8(t + 0x16, 2)?;
            m.write_u8(0x030029f8, 0)?;
            m.write_bytes(0x03002a00, &[0; 12])?;
        }
        _ => {
            allocate(m, c, t, 0xf85c)?;
            let child = allocate(m, c, t, 0xf888)?;
            m.write_u8(child + 0x1b, m.read_u8(t + 4)?)?;
            m.write_u16(TICKS, 0)?;
            m.write_u8(t + 0x1f, 255)?;
            m.write_u8(
                t + 0x1a,
                m.read_u8(ROUND)?.wrapping_mul(10).wrapping_add(20),
            )?;
            let speed = match m.read_u8(ROUND)? {
                0 => 116,
                1 => 96,
                2 => 76,
                _ => 57,
            };
            m.write_u8(t + 0x1c, speed)?;
            m.write_u8(t + 0x19, 30)?;
            word(m, t, P + 0xe1f1)?;
        }
    }
    Ok(())
}
fn advance_rows(m: Mem, c: &mut impl Calls, t: u32) -> R {
    if m.read_u8(G + 15)? & 0x83 != 0 {
        return Ok(());
    }
    let scroll = m.read_u16(TICKS)?;
    if scroll != 0 {
        let value = scroll.wrapping_sub(2);
        m.write_u16(TICKS, value)?;
        if value == 0 {
            m.write_u8(MODE, m.read_u8(MODE)? & !2)?;
            c.invoke(m, 0xf2dc, &[1])?;
        }
        let value = m.read_u16(TICKS)?;
        c.invoke(m, 0x1698, &[0x16, value.into()])?;
        return Ok(());
    }
    if m.read_u8(MODE)? == 0 {
        for col in 1..=10 {
            if m.read_u8(BOARD + 0x6c + col)? & 15 != 0 {
                m.write_u8(MODE, m.read_u8(MODE)? | 64)?;
                return Ok(());
            }
        }
    }
    let population = m.read_u8(MODE - 2)?;
    if population | m.read_u8(MODE - 1)? == 0 {
        m.write_u8(MODE, m.read_u8(MODE)? | 128)?;
        return Ok(());
    }
    m.write_u8(MODE - 1, 0)?;
    if m.read_u8(MODE)? & 0xfe != 0 || population == 0 || decrement(m, t + 0x19)? != 0 {
        return Ok(());
    }
    m.write_u8(t + 0x19, m.read_u8(t + 0x1c)?)?;
    for row in (2..=10).rev() {
        for col in 1..=10 {
            let pos = BOARD + 12 * row + col;
            m.write_u8(pos, m.read_u8(pos - 12)?)?;
        }
    }
    let index = increment(m, t + 0x1f)? as i8 as i32;
    let limit = i32::from(m.read_u8(t + 0x1a)?);
    if index < limit {
        for col in 0..10 {
            let i = m.read_u8(t + 0x1f)? as i8 as i32;
            m.write_u8(
                BOARD + 13 + col,
                m.read_u8(0x03001df0u32.wrapping_add_signed(i * 10) + col)?,
            )?;
        }
    } else {
        m.write_u8(t + 0x1f, (limit - 1) as u8)?;
        m.write_bytes(BOARD + 13, &[0xf0; 10])?;
    }
    c.invoke(m, 0xe888, &[])?;
    m.write_u16(TICKS, 14)?;
    c.invoke(m, 0x1698, &[0x16, 14])?;
    m.write_u8(MODE, m.read_u8(MODE)? | 2)?;
    Ok(())
}
fn move_cursor(m: Mem, t: u32, held: u16, col: u8) -> R {
    let left = held & 32 != 0;
    let flags = m.read_u8(t + 7)?;
    m.write_u8(t + 7, if left { flags & !1 } else { flags | 1 })?;
    m.write_u8(
        t + 0x19,
        if left {
            col.wrapping_sub(1)
        } else {
            col.wrapping_add(1)
        },
    )
}
fn cursor_input(m: Mem, c: &mut impl Calls, t: u32, child: u32) -> R {
    let pressed = m.read_u16(K + 4)?;
    if pressed & 3 != 0
        && m.read_u8(MODE)? & 0xfd == 0
        && m.read_u8(BOARD + 0x60 + u32::from(m.read_u8(t + 0x19)?))? == 0
    {
        let flags = m.read_u8(t + 7)?;
        m.write_u8(
            t + 7,
            if pressed & 2 != 0 {
                flags & !1
            } else {
                flags | 1
            },
        )?;
        m.write_u8(child + 0x1a, 0)?;
        m.write_u8(child + 0x19, 0)?;
        m.write_u8(t + 0x16, 4)?;
        let projectile = allocate(m, c, t, 0xe134)?;
        m.write_u8(projectile + 0x1b, m.read_u8(t + 4)?)?;
    } else {
        let held = m.read_u16(K)?;
        let col = m.read_u8(t + 0x19)?;
        if held & 0x30 != 0 && (if held & 32 != 0 { col > 1 } else { col <= 9 }) {
            move_cursor(m, t, held, col)?;
            m.write_u8(child + 0x1a, 0)?;
            m.write_u8(child + 0x19, 0)?;
            m.write_u8(t + 0x1a, 8)?;
            m.write_u8(t + 0x16, 3)?;
        }
    }
    Ok(())
}
fn cursor(m: Mem, c: &mut impl Calls, t: u32) -> R {
    if m.read_u8(G + 15)? & 0x93 != 0 {
        return Ok(());
    }
    let child = parent(m, t)?;
    let mode = m.read_u8(MODE)?;
    if (mode & 64 != 0 || mode & 0x84 == 128) && m.read_u8(t + 7)? & 128 == 0 {
        m.write_u8(t + 7, m.read_u8(t + 7)? | 128)?;
        m.write_u8(child + 0x1a, 0)?;
        m.write_u8(child + 0x19, 0)?;
        let (state, off) = if mode & 64 != 0 {
            (5, 0xf72c)
        } else {
            (6, 0xf778)
        };
        m.write_u8(t + 0x16, state)?;
        allocate(m, c, t, off)?;
        m.write_u8(G + 15, m.read_u8(G + 15)? | 16)?;
    }
    let state = m.read_u8(t + 0x16)?;
    if state <= 2 || state > 6 {
        if state == 0 || state > 6 {
            m.write_u8(0x030029f8, 0)?;
            m.write_u8(t + 0x1a, 20)?;
            increment(m, t + 0x16)?;
        }
        if state != 2 && decrement(m, t + 0x1a)? == 0 {
            m.write_u8(child + 0x19, 1)?;
            m.write_u8(child + 0x1a, 0)?;
            increment(m, t + 0x16)?;
        }
        cursor_input(m, c, t, child)?;
    } else if state == 3 {
        let x = m.read_u16(t + 0xa)?;
        m.write_u16(
            t + 0xa,
            if m.read_u8(t + 7)? & 1 != 0 {
                x.wrapping_add(2)
            } else {
                x.wrapping_sub(2)
            },
        )?;
        if decrement(m, t + 0x1a)? == 0 {
            let col = m.read_u8(t + 0x19)?;
            if col.wrapping_sub(2) <= 7 {
                let held = m.read_u16(K)?;
                if held & 0x30 != 0 {
                    move_cursor(m, t, held, col)?;
                    m.write_u8(t + 0x1a, 8)?;
                } else {
                    m.write_u8(t + 0x16, 0)?
                }
            } else {
                m.write_u8(t + 0x16, 0)?
            }
        }
    } else if state == 4 && m.read_u8(child + 0x1a)? > 4 {
        m.write_u8(child + 0x1a, 0)?;
        if increment(m, child + 0x19)? > 2 {
            m.write_u8(t + 0x16, 0)?
        }
    }
    m.write_u16(0x030029f6, m.read_u16(t + 0xa)?)?;
    let hit = m.read_u8(0x03002a00 + u32::from(m.read_u8(t + 0x19)?))? != 0;
    let flags = m.read_u8(t + 7)?;
    m.write_u8(t + 7, if hit { flags | 2 } else { flags & !2 })?;
    m.write_bytes(0x03002a01, &[0; 10])
}

fn cursor_animation(m: Mem, c: &mut impl Calls, t: u32) -> R {
    if m.read_u8(G + 15)? & 128 == 0 {
        increment(m, t + 0x1a)?;
        let p = parent(m, t)?;
        let state = m.read_u8(p + 0x16)?;
        let mut flip = false;
        let tile = match state {
            3 => {
                if m.read_u8(t + 0x1a)? > 6 {
                    m.write_u8(t + 0x1a, 0)?;
                    increment(m, t + 0x19)?;
                }
                let i = m.read_u8(t + 0x19)? & 3;
                let tile = m.read_u8(P + 0x2e117 + u32::from(i))?.wrapping_add(4);
                flip = m.read_u8(p + 7)? & 1 != 0;
                tile
            }
            4 => {
                let tile = m.read_u8(t + 0x19)?.wrapping_add(7).min(8);
                flip = m.read_u8(p + 7)? & 1 != 0;
                tile
            }
            5 => {
                if m.read_u8(t + 0x1a)? > 15 {
                    m.write_u8(t + 0x1a, 0)?;
                    if increment(m, t + 0x19)? > 2 {
                        m.write_u8(t + 0x19, 1)?
                    }
                }
                m.read_u8(t + 0x19)?.wrapping_add(12)
            }
            6 => {
                if m.read_u8(t + 0x1a)? > 20 {
                    m.write_u8(t + 0x1a, 0)?;
                    if increment(m, t + 0x19)? == 1 && m.read_u8(ROUND)? != 3 {
                        c.invoke(m, 0x1066c, &[12])?;
                    }
                    if m.read_u8(t + 0x19)? > 2 {
                        m.write_u8(t + 0x19, 2)?
                    }
                }
                m.read_u8(t + 0x19)?.wrapping_add(15)
            }
            _ if m.read_u8(0x030029f8)? != 0 => {
                decrement(m, 0x030029f8)?;
                10 + ((m.read_u32(G + 8)? >> 3) & 1) as u8
            }
            _ if m.read_u8(p + 7)? & 2 != 0 => 9,
            2 => {
                if m.read_u8(t + 0x1a)? > 30 {
                    m.write_u8(t + 0x1a, 0)?;
                    increment(m, t + 0x19)?;
                }
                let i = m.read_u8(t + 0x19)?;
                if i & 1 != 0 || i <= 3 {
                    i & 1
                } else if m.read_u8(t + 0x1a)? <= 5 {
                    2
                } else {
                    m.write_u8(t + 0x19, 0)?;
                    0
                }
            }
            _ => 3,
        };
        m.write_u16(t + 0xa, m.read_u16(p + 0xa)?)?;
        word(m, t + 0x10, P + 0x2dcfc + if flip { 16 } else { 0 })?;
        m.write_u16(t + 0x14, 0x53 + 12 * u16::from(tile))?;
    }
    draw(m, c, t, 0x1398, 0x50)
}
fn board_map(m: Mem, overlay: bool) -> R {
    let dst = if overlay { 0x03001ff0 } else { 0x030024f0 };
    let board = if overlay { 0x03001b70 } else { BOARD };
    m.write_bytes(dst, &[0; 1280])?;
    for row in 0..10 {
        for col in 0..10 {
            let tag = m.read_u8(board + 13 + row * 12 + col)?;
            let first = match tag {
                1 => 0xa209,
                2 => 0x9209,
                0x10 => 0xb20d,
                0x50 => 0xb211,
                0xa0 => 0xb215,
                0xf0 if !overlay => 0x8205,
                0x18 if overlay => 0,
                _ => continue,
            };
            let dest = dst + 4 + row * 128 + col * 4;
            for (i, delta) in [0, 2, 64, 66].into_iter().enumerate() {
                let tile = if first == 0 {
                    let frame = u32::from(m.read_u8(0x03002a30)?);
                    m.read_u16(P + 0x2e0c0 + 8 * frame + 2 * i as u32)?
                } else {
                    first + i as u16
                };
                m.write_u16(dest + delta, tile)?;
            }
        }
    }
    m.write_u8(if overlay { 0x030029f4 } else { 0x030029f5 }, 1)
}
fn detach(m: Mem, c: &mut impl Calls, stop: u8) -> R {
    let mut changed = false;
    for col in 1..=10 {
        for row in (u32::from(stop) + 1..=9).rev() {
            let cell = BOARD + row * 12 + col;
            let tag = m.read_u8(cell)?;
            if tag & 15 != 0 {
                break;
            }
            if tag == 0 || tag == 0xf0 {
                continue;
            }
            m.write_u8(cell, 0)?;
            decrement(m, MODE - 2)?;
            changed = true;
            for particle in (0..(tag >> 4)).rev() {
                let obj = c.invoke(m, 0xd04, &[P + 0xf219, 0x03005344, 1])?;
                m.write_u16(obj + 0xa, (col * 16 + 8) as u16)?;
                m.write_u16(obj + 0xe, (row * 16 - 8) as u16)?;
                word(m, obj + 0x10, P + 0x2dd5c)?;
                m.write_u16(obj + 0x14, 0x13f)?;
                m.write_u8(obj + 0x18, 0x6c)?;
                m.write_u8(obj + 0x1a, particle * 3)?;
                m.write_u16(obj + 0x20, 0)?;
                m.write_u8(obj + 0x1e, col as u8)?;
            }
        }
    }
    if changed {
        c.invoke(m, 0x1066c, &[0x12])?;
        c.invoke(m, 0xe888, &[])?;
    }
    Ok(())
}
fn signed(m: &dyn FrameMemory, a: u32) -> R<i32> {
    Ok(m.read_u16(a)? as i16 as i32)
}
fn shift_half(m: Mem, addr: u32, delta: i16) -> R<i16> {
    let n = (m.read_u16(addr)? as i16).wrapping_add(delta);
    m.write_u16(addr, n as u16)?;
    Ok(n)
}
fn shot_init(m: Mem, c: &mut impl Calls, t: u32) -> R {
    m.write_u8(MODE, m.read_u8(MODE)? | 1)?;
    let p = parent(m, t)?;
    let side = m.read_u8(p + 7)? & 1;
    m.write_u8(t + 7, side)?;
    m.write_u16(t + 0xa, m.read_u16(p + 0xa)?)?;
    m.write_u16(t + 0xe, 155)?;
    word(m, t + 0x10, P + 0x2dd1c)?;
    m.write_u16(t + 0x14, 0x12b)?;
    m.write_u8(t + 0x18, 12)?;
    m.write_u8(t + 0x19, m.read_u8(p + 0x19)?)?;
    m.write_u8(t + 0x18, if side == 0 { 0x4c } else { 0x3c })?;
    for i in 1..=3 {
        let tail = allocate(m, c, t, 0xf814)?;
        m.write_u16(tail + 0xa, m.read_u16(t + 0xa)?)?;
        m.write_u16(tail + 0xe, m.read_u16(t + 0xe)?.wrapping_add(3 * i))?;
        let graphic = m.read_u32(t + 0x10)?;
        word(m, tail + 0x10, graphic)?;
        m.write_u16(tail + 0x14, m.read_u16(t + 0x14)?.wrapping_add(4 * i))?;
        m.write_u8(tail + 0x16, 4)?;
        m.write_u8(tail + 0x18, m.read_u8(t + 0x18)?)?;
        m.write_u8(tail + 0x1d, 120 - i as u8)?;
    }
    c.invoke(m, 0x1066c, &[19])?;
    word(m, t, P + 0xe08d)?;
    c.invoke(m, 0xe08c, &[t])?;
    Ok(())
}
fn flight(m: Mem, c: &mut impl Calls, t: u32) -> R {
    if m.read_u8(G + 15)? & 128 == 0 {
        let y = shift_half(m, t + 0xe, -4)?;
        let row = (i32::from(y) + signed(m, TICKS)? + 8) >> 4;
        let col = i32::from(m.read_u8(t + 0x19)?);
        if m.read_u8(BOARD.wrapping_add_signed(12 * row + col))? != 0 {
            let hit = c.invoke(m, 0xd04, &[P + 0xe431, 0x03005344, 1])?;
            m.write_u16(hit + 0xa, m.read_u8(t + 0x19)?.into())?;
            m.write_u16(hit + 0xe, (row + 1) as u16)?;
            m.write_u8(hit + 0x19, m.read_u8(t + 7)?.wrapping_add(1))?;
            m.write_u8(hit + 0x16, 0)?;
            m.write_u8(MODE, m.read_u8(MODE)? & !1)?;
            m.write_u8(t + 0x18, 120)?;
            m.write_u16(t + 0xc, (row * 16 + 8) as u16)?;
            m.write_u8(t + 0x1a, 15)?;
            word(m, t, P + 0xe02d)?;
            c.invoke(m, 0xe02c, &[t])?;
            return Ok(());
        }
    }
    draw(m, c, t, 0x1398, 120)
}
fn shot_effect(m: Mem, c: &mut impl Calls, t: u32, trail: bool) -> R {
    let flags = m.read_u8(G + 15)?;
    if trail {
        if flags & 2 != 0 || (flags & 128 == 0 && m.read_u8(MODE)? & 1 == 0) {
            c.invoke(m, 0xed4, &[t])?;
        } else {
            if flags & 128 == 0 {
                shift_half(m, t + 0xe, -4)?;
            }
            let layer = m.read_u8(t + 0x1d)?;
            draw(m, c, t, 0x1398, layer.into())?;
        }
    } else if flags & 128 == 0 && decrement(m, t + 0x1a)? == 255 {
        c.invoke(m, 0xed4, &[t])?;
    } else {
        let y = m.read_u16(t + 0xc)?.wrapping_sub(m.read_u16(TICKS)?);
        m.write_u16(t + 0xe, y)?;
        m.write_u16(t + 0x14, if y as i16 > 120 { 0x14f } else { 0x14b })?;
        draw(m, c, t, 0x1254, 2)?;
    }
    Ok(())
}
fn remove_resolution(m: Mem, c: &mut impl Calls, t: u32) -> R {
    c.invoke(m, 0xed4, &[t])?;
    m.write_u8(MODE, m.read_u8(MODE)? & !4)
}
fn clear_overlay(m: Mem) -> R {
    m.write_bytes(0x03001ff0, &[0; 1280])?;
    m.write_u8(0x030029f4, 1)
}
fn resolve_shot(m: Mem, c: &mut impl Calls, t: u32) -> R {
    if m.read_u8(G + 15)? & 128 != 0 {
        return Ok(());
    }
    m.write_u8(MODE, m.read_u8(MODE)? | 4)?;
    match m.read_u8(t + 0x16)? {
        0 => {
            let addr = BOARD.wrapping_add_signed(signed(m, t + 0xa)? + 12 * signed(m, t + 0xe)?);
            m.write_u8(addr, m.read_u8(t + 0x19)?)?;
            c.invoke(m, 0xe888, &[])?;
            if m.read_u8(MODE - 2)? == 0 {
                return remove_resolution(m, c, t);
            }
            c.invoke(m, 0x1066c, &[17])?;
            m.write_u8(t + 0x1a, 15)?;
            increment(m, t + 0x16)?;
        }
        1 if decrement(m, t + 0x1a)? == 0 => {
            let row = m.read_u8(t + 0xe)?;
            let col = m.read_u8(t + 0xa)?;
            if c.invoke(m, 0xea5c, &[row.into(), col.into()])? as u8 == 0 {
                m.write_u8(t + 0x1a, 0)?
            } else {
                m.write_u8(t + 7, 1)?;
                m.write_u8(t + 0x1a, 31)?
            }
            increment(m, t + 0x16)?;
        }
        2 => {
            if m.read_u8(t + 7)? != 0 {
                match decrement(m, t + 0x1a)? {
                    15 | 16 => {
                        increment(m, 0x03002a30)?;
                        c.invoke(m, 0xe6d8, &[])?;
                    }
                    0 => {
                        clear_overlay(m)?;
                        c.invoke(m, 0xe888, &[])?;
                    }
                    _ => {}
                }
                let tag = m.read_u8(t + 0x19)?;
                let clock = m.read_u8(t + 0x1a)?;
                c.invoke(m, 0xeb08, &[tag.into(), clock.into()])?;
            }
            if m.read_u8(t + 0x1a)? == 0 {
                m.write_u8(0x03001bf4, 1)?;
                m.write_u8(0x03001bf8, m.read_u16(t + 0xe)? as u8)?;
                m.write_u8(0x03001bf9, m.read_u16(t + 0xa)? as u8)?;
                m.write_u8(0x03001bfa, m.read_u8(t + 0x19)?)?;
                if c.invoke(m, 0xed90, &[])? as u8 == 0 {
                    return remove_resolution(m, c, t);
                }
                c.invoke(m, 0xeef8, &[])?;
                m.write_u8(t + 0x1a, 36)?;
                increment(m, t + 0x16)?;
            }
        }
        3 if m.read_u8(t + 0x1a)? != 0 => {
            let clock = decrement(m, t + 0x1a)?;
            if clock == 0 {
                clear_overlay(m)?
            } else if [6, 12, 18, 24, 30].contains(&clock) {
                if clock == 18 {
                    c.invoke(m, 0xe888, &[])?;
                }
                increment(m, 0x03002a30)?;
                c.invoke(m, 0xe6d8, &[])?;
            }
        }
        3 => {
            if c.invoke(m, 0xf010, &[])? as u8 != 0 {
                c.invoke(m, 0xe888, &[])?;
                increment(m, t + 0x16)?;
            } else if c.invoke(m, 0xef50, &[])? as u8 != 0 {
                m.write_u8(t + 0x16, 5)?
            } else {
                c.invoke(m, 0xf2dc, &[0])?;
                remove_resolution(m, c, t)?
            }
        }
        4 if c.invoke(m, 0xf0f0, &[])? as u8 != 0 => {
            c.invoke(m, 0xe888, &[])?;
            m.write_u8(t + 0x16, 3)?;
        }
        5 => {
            if c.invoke(m, 0xed90, &[])? as u8 != 0 {
                c.invoke(m, 0xeef8, &[])?;
                m.write_u8(t + 0x1a, 36)?;
                m.write_u8(t + 0x16, 3)?;
            } else {
                c.invoke(m, 0xf2dc, &[0])?;
                remove_resolution(m, c, t)?
            }
        }
        _ => {}
    }
    Ok(())
}
fn pickup(m: Mem, c: &mut impl Calls, t: u32, falling: bool) -> R {
    if m.read_u8(G + 15)? & 128 == 0 {
        if falling {
            if m.read_u8(t + 0x1a)? != 0 {
                decrement(m, t + 0x1a)?;
            } else {
                let speed = shift_half(m, t + 0x20, 32)?;
                let y = m
                    .read_u32(t + 0xc)?
                    .wrapping_add_signed(i32::from(speed) << 8);
                word(m, t + 0xc, y)?;
                let y = signed(m, t + 0xe)?;
                if y > 167 {
                    c.invoke(m, 0xed4, &[t])?;
                    return Ok(());
                }
                if y > 135 && m.read_u8(MODE)? & 64 == 0 {
                    let x = signed(m, t + 0xa)?;
                    let player = signed(m, 0x030029f6)?;
                    if player - 12 < x && x < player + 12 {
                        c.invoke(m, 0x1066c, &[27])?;
                        m.write_u8(t + 0x1a, 0)?;
                        m.write_u8(t + 0x19, 0)?;
                        word(m, t, P + 0xf195)?;
                        shift_half(m, 0x03001ad2, 1)?;
                        m.write_u8(0x030029f8, 30)?;
                    }
                }
            }
        } else {
            if increment(m, t + 0x1a)? > 6 {
                m.write_u8(t + 0x1a, 0)?;
                increment(m, t + 0x19)?;
            }
            m.write_u8(t + 0x18, 0x7c)?;
            let frame = m.read_u8(t + 0x19)?;
            m.write_u16(t + 0x14, if frame & 1 != 0 { 0x147 } else { 0x143 })?;
            m.write_u8(t + 0x17, if frame & 2 != 0 { 16 } else { 0 })?;
            if frame > 3 {
                c.invoke(m, 0xed4, &[t])?;
                return Ok(());
            }
        }
    }
    draw(m, c, t, 0x1398, 128)?;
    if falling {
        let col = m.read_u8(t + 0x1e)? as i8;
        m.write_u8(0x03002a00u32.wrapping_add_signed(col.into()), 1)?;
    }
    m.write_u8(MODE - 1, 1)
}
fn banner_setup(m: Mem, c: &mut impl Calls, t: u32, win: bool) -> R {
    if win && m.read_u8(ROUND)? <= 2 {
        m.write_u8(t + 0x1a, 105)?;
        return word(m, t, P + 0xf8d1);
    }
    let (src, dst, width, delta, cb) = if win {
        (0x2e026, 0x0600c00c, 13, 0x517a, 0xf518)
    } else {
        (0x2dfae, 0x0600c004, 20, 0x413e, 0xf404)
    };
    c.invoke(m, 0x17b0, &[P + src, dst, width, 3, delta])?;
    c.invoke(m, 0x1698, &[0x12, 24])?;
    m.write_u8(t + 0x16, 0)?;
    m.write_u16(t + 0xe, 0)?;
    m.write_u8(t + 0x1a, 15)?;
    word(m, t, P + cb + 1)?;
    c.invoke(m, cb, &[t])?;
    Ok(())
}
fn leave(m: Mem, next: u32) -> R {
    m.write_u8(G + 15, m.read_u8(G + 15)? | 2)?;
    word(m, G + 4, next)
}
fn banner(m: Mem, c: &mut impl Calls, t: u32, win: bool) -> R {
    if m.read_u8(t + 0x1a)? != 0 {
        if decrement(m, t + 0x1a)? == 0 {
            c.invoke(m, 0x1066c, &[if win { 35 } else { 23 }])?;
        }
        return Ok(());
    }
    match m.read_u8(t + 0x16)? {
        0 => {
            let n = signed(m, t + 0xe)?;
            let d = n * n / 4;
            let mut v = (if win { d + 96 } else { 280 - d }) as i16;
            shift_half(m, t + 0xe, 1)?;
            if (win && v > 184) || (!win && v < 184) {
                v = 184;
                increment(m, t + 0x16)?;
                m.write_u16(t + 0xe, 0)?;
            }
            c.invoke(m, 0x1698, &[0x12, u32::from(v as u16)])?;
        }
        1 => {
            let v = m.read_u16(t + 0xe)?.wrapping_add(184);
            c.invoke(m, 0x1698, &[0x12, v.into()])?;
            if shift_half(m, t + 0xe, 1)? > 7 {
                c.invoke(m, 0x1698, &[0x54, 0])?;
                c.invoke(m, 0x1698, &[0x50, 129])?;
                increment(m, t + 0x16)?;
                m.write_u16(t + 0xe, 0)?;
            }
        }
        2 | 3 => {
            let state = m.read_u8(t + 0x16)?;
            let v = m.read_u16(t + 0xe)?;
            c.invoke(m, 0x1698, &[0x54, v.into()])?;
            let n = shift_half(m, t + 0xe, if state == 2 { 1 } else { -1 })?;
            if (state == 2 && n > 15) || (state == 3 && n < 0) {
                increment(m, t + 0x16)?;
                if state == 3 {
                    m.write_u16(t + 0xe, 0)?
                }
            }
        }
        _ if m.read_u16(K + 4)? & 9 != 0 || shift_half(m, t + 0xe, 1)? > 179 => leave(m, 0)?,
        _ => {}
    }
    Ok(())
}
fn fade_return(m: Mem, c: &mut impl Calls) -> R {
    for off in [0x16d8, 0x12e0, 0x1420] {
        c.invoke(m, off, &[])?;
    }
    let n = shift_half(m, G + 12, 1)?;
    m.write_u16(0x04000054, n as u16)?;
    if n > 15 {
        let mut next = m.read_u32(G + 4)?;
        if next == 0 {
            let rings = m
                .read_u32(0x03006194)?
                .wrapping_add(u32::from(m.read_u16(0x03001ad2)?));
            word(m, 0x03006194, rings)?;
            if rings > 99999 {
                word(m, 0x03006194, 99999)?
            }
            c.invoke(m, 0xe28, &[])?;
            next = P + 0x1bf9;
        }
        word(m, G, next)?;
    }
    Ok(())
}

const MARKS: u32 = 0x03001b70;
const CHANGES: u32 = 0x03001bf4;
const TRIPLES: u32 = 0x03001bf8;
const GAPS: u32 = 0x03001cb8;
const TAG: u32 = 0x03001cc3;
const TAIL: u32 = 0x03001cc4;
const HEAD: u32 = 0x03001cc5;
const QUEUE: u32 = 0x03001cc8;
const FOUND: u32 = 0x03001d48;
const CELLS: u32 = 0x03001d50;
fn malformed(offset: u32, reason: &str) -> FrameError {
    FrameError::Dependency {
        call: NativeCall {
            entry_offset: super::garden_graphics::diagnostic_address(offset),
            argument0: None,
        },
        reason: reason.into(),
    }
}
fn palette_flash(m: Mem, tag: u8, clock: u8) -> R {
    let source = P + 0x17194 + if tag == 1 { 32 } else { 0 };
    let add = u16::from(clock.wrapping_mul(2));
    for i in 0..16 {
        let color = m.read_u16(source + 2 * i)?;
        let channel = |shift: u16| (((color >> shift) & 31) + add).min(31);
        let result = channel(0) | (channel(5) << 5) | (channel(10) << 10);
        m.write_u16(0x03002a10 + 2 * i, result)?;
    }
    m.write_u8(0x03002a0c, 1)
}
fn put_point(m: Mem, count: u32, base: u32, row: u8, col: u8, wrap: bool) -> R {
    let i = u32::from(m.read_u8(count)?);
    if wrap && i >= 64 {
        return Err(malformed(0xeb9c, "native group queue index exceeds64"));
    }
    m.write_u8(base + 2 * i, row)?;
    let i = m.read_u8(count)?;
    m.write_u8(count, i.wrapping_add(1))?;
    m.write_u8(base + 2 * u32::from(i) + 1, col)?;
    if wrap {
        m.write_u8(count, m.read_u8(count)? & 63)?
    }
    Ok(())
}
fn scan_run(m: Mem, row: u8, mut col: u8, tag: u8) -> R {
    let start = MARKS + 12 * u32::from(row);
    let orig = col;
    while m.read_u8(start + u32::from(col) - 1)? == tag {
        col = col.wrapping_sub(1);
        if col == orig {
            return Err(malformed(0xeb9c, "unterminated native scanline"));
        }
    }
    let first = col;
    loop {
        for delta in [1i32, -1] {
            if m.read_u8(start.wrapping_add_signed(12 * delta) + u32::from(col))? == tag {
                put_point(
                    m,
                    TAIL,
                    QUEUE,
                    row.wrapping_add_signed(delta as i8),
                    col,
                    true,
                )?;
            }
        }
        m.write_u8(start + u32::from(col), 0)?;
        put_point(m, FOUND, CELLS, row, col, false)?;
        col = col.wrapping_add(1);
        if m.read_u8(start + u32::from(col))? != tag {
            return Ok(());
        }
        if col == first {
            return Err(malformed(0xeb9c, "unterminated native scanline"));
        }
    }
}
fn collect_group(m: Mem, c: &mut impl Calls) -> R<u32> {
    if m.read_u8(TAIL)? >= 64 || m.read_u8(HEAD)? >= 64 {
        return Err(malformed(0xecc8, "native group queue index exceeds64"));
    }
    m.write_u8(FOUND, 0)?;
    let mut steps = 0;
    while m.read_u8(TAIL)? != m.read_u8(HEAD)? {
        steps += 1;
        if steps > 4096 || m.read_u8(TAIL)? >= 64 || m.read_u8(HEAD)? >= 64 {
            return Err(malformed(
                0xecc8,
                "native group queue budget or index exceeded",
            ));
        }
        let head = m.read_u8(HEAD)?;
        let p = QUEUE + 2 * u32::from(head);
        let row = m.read_u8(p)?;
        let next = head.wrapping_add(1);
        m.write_u8(HEAD, next)?;
        let col = m.read_u8(p + 1)?;
        m.write_u8(HEAD, next & 63)?;
        if m.read_u8(MARKS + 12 * u32::from(row) + u32::from(col))? != 0 {
            let tag = m.read_u8(TAG)?;
            c.invoke(m, 0xeb9c, &[row.into(), col.into(), tag.into()])?;
        }
    }
    if m.read_u8(FOUND)? <= 9 {
        return Ok(0);
    }
    loop {
        let i = decrement(m, FOUND)?;
        if i == 255 {
            break;
        }
        let p = CELLS + 2 * u32::from(i);
        let cell = 12 * u32::from(m.read_u8(p)?) + u32::from(m.read_u8(p + 1)?);
        m.write_u8(BOARD + cell, 0x18)?;
    }
    Ok(1)
}
fn find_groups(m: Mem, c: &mut impl Calls) -> R<u32> {
    for row in 1..=9 {
        for cell in row * 12 + 1..=row * 12 + 10 {
            m.write_u8(MARKS + cell, m.read_u8(BOARD + cell)? & 15)?;
        }
    }
    let mut any = 0u8;
    loop {
        let i = decrement(m, CHANGES)?;
        if i == 255 {
            break;
        }
        let p = TRIPLES + 3 * u32::from(i);
        let row = m.read_u8(p)?;
        let col = m.read_u8(p + 1)?;
        let tag = m.read_u8(p + 2)?;
        m.write_u8(TAG, tag)?;
        if m.read_u8(MARKS + 12 * u32::from(row) + u32::from(col))? == tag {
            m.write_u8(TAIL, 1)?;
            m.write_u8(HEAD, 0)?;
            m.write_u8(QUEUE, row)?;
            m.write_u8(QUEUE + 1, col)?;
            any |= c.invoke(m, 0xecc8, &[])? as u8;
        }
    }
    if any != 0 {
        for row in 1..=10 {
            for i in row * 12 + 1..=row * 12 + 10 {
                let tag = m.read_u8(BOARD + i)?;
                if tag == 0x18 {
                    m.write_u8(BOARD + i, 0)?;
                    m.write_u8(MARKS + i, tag)?;
                } else {
                    m.write_u8(MARKS + i, 0)?
                }
            }
        }
        m.write_u8(0x03002a30, 3)?;
        c.invoke(m, 0xe6d8, &[])?;
        c.invoke(m, 0x1698, &[0x12, 0])?;
        c.invoke(m, 0x1066c, &[25])?;
    }
    Ok(any.into())
}
fn find_gaps(m: Mem) -> R {
    for col in 1..=10 {
        m.write_u8(GAPS + col, 0)?;
        for row in 1..=9 {
            if m.read_u8(BOARD + 12 * row + col)? == 0 {
                m.write_u8(GAPS + col, row as u8)?;
                break;
            }
        }
    }
    Ok(())
}
fn gather_changes(m: Mem) -> R<u32> {
    m.write_u8(CHANGES, 0)?;
    for col in 1..=10 {
        let start = m.read_u8(GAPS + col)?;
        if start == 0 {
            continue;
        }
        for row in u32::from(start)..=9 {
            let addr = BOARD + 12 * row + col;
            if m.read_u8(addr)? & 15 != 0 {
                let i = m.read_u8(CHANGES)?;
                m.write_u8(TRIPLES + 3 * u32::from(i), row as u8)?;
                let i = m.read_u8(CHANGES)?;
                m.write_u8(TRIPLES + 3 * u32::from(i) + 1, col as u8)?;
                let i = m.read_u8(CHANGES)?;
                m.write_u8(CHANGES, i.wrapping_add(1))?;
                m.write_u8(TRIPLES + 3 * u32::from(i) + 2, m.read_u8(addr)?)?;
            }
        }
    }
    Ok(m.read_u8(CHANGES)?.into())
}
fn begin_settle(m: Mem, c: &mut impl Calls) -> R<u32> {
    for row in 1..=10 {
        m.write_bytes(MARKS + 12 * row + 1, &[0; 10])?
    }
    let mut changed = false;
    for col in 1..=10 {
        let mut gap = false;
        for row in 1..=10 {
            let i = 12 * row + col;
            let tag = m.read_u8(BOARD + i)?;
            if tag == 0 {
                gap = true
            } else if gap {
                m.write_u8(MARKS + i, tag)?;
                m.write_u8(BOARD + i, 0)?;
                changed = true;
            }
        }
    }
    if changed {
        c.invoke(m, 0xe6d8, &[])?;
        m.write_u16(0x030029f2, 4)?;
        c.invoke(m, 0x1698, &[0x12, 4])?;
    }
    Ok(changed.into())
}
fn settle(m: Mem, c: &mut impl Calls) -> R<u32> {
    let n = shift_half(m, 0x030029f2, 4)?;
    if n <= 15 {
        c.invoke(m, 0x1698, &[0x12, u32::from(n as u16)])?;
        return Ok(0);
    }
    for col in 1..=10 {
        for row in (2..=10).rev() {
            let i = 12 * row + col;
            let tag = m.read_u8(MARKS + i)?;
            if tag != 0 {
                m.write_u8(BOARD + i - 12, tag)?
            }
        }
    }
    clear_overlay(m)?;
    Ok(1)
}

pub fn dispatch(m: Mem, c: &mut impl Calls, offset: u32, args: &[u32]) -> R<u32> {
    if matches!(
        offset,
        0xe02c
            | 0xe08c
            | 0xe134
            | 0xe430
            | 0xf194
            | 0xf218
            | 0xf404
            | 0xf518
            | 0xf72c
            | 0xf778
            | 0xf814
            | 0xf8d0
            | 0xf688
            | 0xf6bc
            | 0xdfbc
            | 0xe3b4
            | 0xd8a4
            | 0xd9a8
            | 0xda04
            | 0xda58
            | 0xdac0
            | 0xf7dc
            | 0xdba8
            | 0xde24
            | 0xe1f0
            | 0xf85c
            | 0xf888
    ) && args
        .first()
        .is_some_and(|t| !(0x03000000..=0x03007fdc).contains(t))
    {
        return Err(malformed(offset, "task record outside bounded IWRAM"));
    }
    match (offset, args) {
        (0xe02c, &[t]) => shot_effect(m, c, t, false)?,
        (0xe08c, &[t]) => flight(m, c, t)?,
        (0xe134, &[t]) => shot_init(m, c, t)?,
        (0xe430, &[t]) => resolve_shot(m, c, t)?,
        (0xf194 | 0xf218, &[t]) => pickup(m, c, t, offset == 0xf218)?,
        (0xf404 | 0xf518, &[t]) => banner(m, c, t, offset == 0xf518)?,
        (0xf72c | 0xf778, &[t]) => banner_setup(m, c, t, offset == 0xf778)?,
        (0xf624, []) => fade_return(m, c)?,
        (0xf814, &[t]) => shot_effect(m, c, t, true)?,
        (0xf8d0, &[t]) => {
            if decrement(m, t + 0x1a)? == 0 {
                leave(m, P + 0xf8f9)?
            }
        }
        (0xf8f8, []) => {
            c.invoke(m, 0x163c, &[P + 0x2e0b4, 3])?;
            if increment(m, ROUND)? > 3 {
                m.write_u8(ROUND, 3)?
            }
            c.invoke(m, 0xd504, &[])?;
        }
        (0xeb08, &[tag, clock]) => palette_flash(m, tag as u8, clock as u8)?,
        (0xeb9c, &[row, col, tag]) => scan_run(m, row as u8, col as u8, tag as u8)?,
        (0xecc8, []) => return collect_group(m, c),
        (0xed90, []) => return find_groups(m, c),
        (0xeef8, []) => find_gaps(m)?,
        (0xef50, []) => return gather_changes(m),
        (0xf010, []) => return begin_settle(m, c),
        (0xf0f0, []) => return settle(m, c),
        (0xf688, &[t]) => {
            let n = decrement(m, t + 0x1a)?;
            c.invoke(m, 0x1698, &[0x54, n.into()])?;
            if m.read_u8(t + 0x1a)? == 0 {
                c.invoke(m, 0xed4, &[t])?;
                m.write_u8(G + 15, m.read_u8(G + 15)? & 0xfe)?;
            }
        }
        (0xf6bc | 0xdfbc | 0xe3b4, &[t]) => setup(m, c, t, offset)?,
        (0xd8a4, &[t]) => pause(m, c, t)?,
        (0xd9a8 | 0xda04 | 0xda58 | 0xdac0 | 0xf7dc, &[t]) => pause_sprite(m, c, t, offset)?,
        (0xdb28, []) => score(m, c)?,
        (0xdba8, &[t]) => cursor(m, c, t)?,
        (0xde24, &[t]) => cursor_animation(m, c, t)?,
        (0xe1f0, &[t]) => advance_rows(m, c, t)?,
        (0xe6d8 | 0xe888, []) => board_map(m, offset == 0xe6d8)?,
        (0xf2dc, &[stop]) => detach(m, c, stop as u8)?,
        (0xf85c, &[t]) => {
            m.write_u16(t + 0xa, 210)?;
            m.write_u16(t + 0xe, 48)?;
            word(m, t + 0x10, P + 0x2dd24)?;
            m.write_u16(t + 0x14, 0x13b)?;
            m.write_u8(t + 0x18, 0x5c)?;
            draw(m, c, t, 0x1398, 0)?;
        }
        (0xf888, &[t]) => {
            let p = parent(m, t)?;
            let num = 48 * (i32::from(m.read_u8(p + 0x1f)? as i8) + 1);
            let denom = m.read_u8(p + 0x1a)?;
            let quotient = if denom == 0 {
                c.invoke(m, 0x11ae8, &[num as u32, 0])? as i32
            } else {
                num / i32::from(denom)
            };
            m.write_u16(t + 0xe, quotient.wrapping_add(48) as u16)?;
            m.write_u16(t + 0xa, 239)?;
            word(m, t + 0x10, P + 0x2dd34)?;
            m.write_u8(t + 0x18, 12)?;
            draw(m, c, t, 0x1398, 1)?;
        }
        _ => {
            return Err(FrameError::Dependency {
                call: NativeCall {
                    entry_offset: super::garden_graphics::diagnostic_address(offset),
                    argument0: args.first().copied(),
                },
                reason: "unknown puzzle callback or invalid arguments".into(),
            })
        }
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{
        garden_payload::{BoardCalls, NativeBoardCalls},
        memory::{MemoryImage, Region},
    };
    use serde_json::{json, Value};
    struct OracleBoundary(Vec<Value>, bool);
    impl Calls for OracleBoundary {
        fn invoke(&mut self, m: Mem, off: u32, args: &[u32]) -> R<u32> {
            if self.1 && matches!(off, 0xeb08 | 0xeef8 | 0xed90 | 0xef50 | 0xf010 | 0xf0f0) {
                return dispatch(m, self, off, args);
            }
            if self.1 && off == 0xea5c {
                use crate::game::garden_payload::{
                    flip_bracketed_balls, BoardEffect, BoardEffects, BoardError,
                };
                struct Effects<'a>(&'a mut OracleBoundary);
                impl BoardEffects for Effects<'_> {
                    fn invoke(
                        &mut self,
                        which: BoardEffect,
                        m: &mut dyn FrameMemory,
                    ) -> Result<(), BoardError> {
                        match which {
                            BoardEffect::RedrawMarks => {
                                self.0.invoke(m, 0xe6d8, &[])?;
                            }
                            BoardEffect::QueueBg0VerticalOffset => {
                                self.0.invoke(m, 0x1698, &[0x12, 0])?;
                            }
                        }
                        Ok(())
                    }
                }
                return Ok(flip_bracketed_balls(
                    m,
                    args[0] as u8,
                    args[1] as u8,
                    &mut Effects(self),
                )
                .unwrap()
                .into());
            }
            match off {
                0xd04 | 0xed4 | 0x1254 | 0x1398 | 0x1698 | 0x1066c | 0x107a0 | 0x107d8 | 0x17b0
                | 0xeb08 | 0xeef8 | 0x16d8 | 0x12e0 | 0x1420 | 0xe28 | 0x163c | 0xd504 | 0xea5c
                | 0xed90 | 0xf010 | 0xef50 | 0xf0f0 => {
                    self.0.push(json!([off, args]));
                    if off == 0xd04 {
                        return Ok(NativeBoardCalls.invoke(P + off, args, m).unwrap());
                    }
                    if let Some(i) = [0xea5c, 0xed90, 0xf010, 0xef50, 0xf0f0]
                        .iter()
                        .position(|entry| *entry == off)
                    {
                        return Ok(u32::from(m.read_u8(0x03007000 + i as u32)?));
                    }
                    Ok(0)
                }
                _ => dispatch(m, self, off, args),
            }
        }
    }
    fn n(val: &Value) -> u32 {
        val.as_u64().unwrap() as u32
    }
    fn verify(f: &Value, profile: &str, data: Vec<u8>) {
        for (i, case) in f["cases"].as_array().unwrap().iter().enumerate() {
            let mut m = MemoryImage::fixture(vec![
                Region {
                    base: P,
                    bytes: data.clone(),
                    writable: false,
                },
                Region {
                    base: 0x04000054,
                    bytes: vec![0; 2],
                    writable: true,
                },
                Region {
                    base: 0x03000000,
                    bytes: vec![0; 0x8000],
                    writable: true,
                },
            ]);
            for w in f["base"]
                .as_array()
                .unwrap()
                .iter()
                .chain(case["writes"].as_array().unwrap())
            {
                let value = n(&w[2]).to_le_bytes();
                m.write_bytes(n(&w[0]), &value[..n(&w[1]) as usize])
                    .unwrap();
            }

            let mut c = OracleBoundary(vec![], case["rules"].as_bool().unwrap_or(false));
            let mut returns = vec![];
            let mut executed = 0;
            for step in case["steps"].as_array().unwrap() {
                executed += 1;
                let s = step.as_array().unwrap();
                let result = dispatch(
                    &mut m,
                    &mut c,
                    n(&s[0]),
                    &s[1..].iter().map(n).collect::<Vec<_>>(),
                )
                .unwrap();
                if matches!(n(&s[0]), 0xecc8 | 0xed90 | 0xef50 | 0xf010 | 0xf0f0) {
                    returns.push(result)
                }
                if let Some(task) = case["stopWhenRemoved"].as_u64() {
                    if c.0.contains(&json!([0xed4, [task]])) {
                        break;
                    }
                }
            }
            let expected = &f["results"][profile][i];
            assert_eq!(json!(returns), expected["returns"]);
            assert_eq!(executed, n(&expected["executed"]));
            assert_eq!(
                u32::from(m.read_u16(0x04000054).unwrap()),
                n(&expected["brightness"])
            );
            assert_eq!(json!(c.0), expected["calls"], "{}", case["name"]);
            assert_eq!(
                crate::sha256(m.bytes(0x03000000, 0x7800).unwrap()),
                expected["ram"].as_str().unwrap(),
                "{}",
                case["name"]
            );
        }
    }
    #[test]
    fn native_puzzle_callbacks_match_ram_and_service_order() {
        let f: Value = serde_json::from_str(include_str!(
            "../../tests/garden-puzzle-objects-vectors.json"
        ))
        .unwrap();
        let mut synthetic = vec![0; 0x40000];
        for row in f["syntheticTables"].as_array().unwrap() {
            let start = n(&row[0]) as usize;
            for (i, value) in row[1].as_array().unwrap().iter().enumerate() {
                synthetic[start + i] = n(value) as u8
            }
        }
        verify(&f, "synthetic", synthetic);
        if let Ok(path) = std::env::var("SPP_GARDEN_PAYLOAD") {
            let mut payload = std::fs::read(path).unwrap();
            assert_eq!(
                crate::sha256(&payload),
                f["payloadSha256"].as_str().unwrap()
            );
            for src in f["sources"].as_array().unwrap() {
                let a = n(&src["offset"]) as usize;
                let end = a + n(&src["length"]) as usize;
                assert_eq!(src["space"], "garden");
                assert_eq!(
                    crate::sha256(&payload[a..end]),
                    src["sha256"].as_str().unwrap()
                );
            }
            payload.resize(0x40000, 0);
            verify(&f, "authentic", payload);
        }
    }
    #[test]
    fn corrupt_group_queue_indices_fail_before_writing_or_looping() {
        for (head, tail) in [(0, 64), (64, 0), (255, 255)] {
            let mut m = MemoryImage::fixture(vec![Region {
                base: 0x03000000,
                bytes: vec![0; 0x8000],
                writable: true,
            }]);
            m.write_u8(HEAD, head).unwrap();
            m.write_u8(TAIL, tail).unwrap();
            let before = crate::sha256(m.bytes(0x03000000, 0x8000).unwrap());
            assert!(dispatch(&mut m, &mut OracleBoundary(vec![], false), 0xecc8, &[]).is_err());
            assert_eq!(before, crate::sha256(m.bytes(0x03000000, 0x8000).unwrap()));
        }
    }
}
