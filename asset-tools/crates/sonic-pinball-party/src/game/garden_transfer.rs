//! A3VE embedded Garden's local JoyBus endpoint. The two 0x800-byte buffers
//! are opaque native records. The hardware-facing register sequence is modeled
//! through FrameMemory; cable timing and physical peer interoperability are unverified.
//! This port does not authenticate peers or import records into the MMO.
use super::{
    frame::{FrameError, FrameMemory, NativeCall},
    garden_home::Calls,
};
type Mem<'a> = &'a mut dyn FrameMemory;
type R<T = ()> = Result<T, FrameError>;
const PHASE: u32 = 0x030052fc;
const ACTIVE: u32 = 0x0300530c;
const MODE: u32 = 0x03005308;
const RX_FLAG: u32 = 0x030052f8;
const TX_FLAG: u32 = 0x03005304;
const SIZE: u32 = 0x03005300;
const RX_BUF: u32 = 0x03004270;
const TX_BUF: u32 = 0x03004af8;
const CAP: u32 = 0x800;
const IDLE: u32 = 0x030005b0;
const TIME: u32 = IDLE + 1;
const PTR: u32 = IDLE + 4;
const COUNT: u32 = IDLE + 8;
const INDEX: u32 = IDLE + 10;
const LAST: u32 = IDLE + 12;
const IE: u32 = 0x04000200;
const IF: u32 = IE + 2;
const IME: u32 = IE + 8;
const RCNT: u32 = 0x04000134;
const CNT: u32 = 0x04000140;
const RX: u32 = CNT + 16;
const TX: u32 = CNT + 20;
const STAT: u32 = CNT + 24;
const KEYS: u32 = 0x03005310;
const FLAGS: u32 = 0x0300425f;
const CHAO: u32 = 0x6f616843;
fn fail(off: u32, s: &str) -> FrameError {
    FrameError::Dependency {
        call: NativeCall {
            entry_offset: super::garden_graphics::diagnostic_address(off),
            argument0: None,
        },
        reason: s.into(),
    }
}
fn word(m: Mem, a: u32, v: u32) -> R {
    m.write_bytes(a, &v.to_le_bytes())
}
fn link(m: Mem) -> R {
    m.write_u16(RCNT, 0xc000)?;
    m.write_u16(STAT, 0)?;
    m.read_u32(RX)?;
    word(m, TX, 0)?;
    m.write_u16(CNT, 0x47)
}
fn clear(m: Mem) -> R {
    m.write_u8(PHASE, 0)?;
    m.write_u8(ACTIVE, 0)?;
    m.write_u8(TIME, 10)?;
    m.write_u8(MODE, 0)
}
fn init(m: Mem) -> R {
    m.write_u16(IME, 0)?;
    let ie = m.read_u16(IE)?;
    m.write_u16(IE, ie & !0x80)?;
    link(m)?;
    let mut irq = Vec::with_capacity(0x520);
    for i in 0..0x148 {
        irq.extend(m.read_u32(0x020006b8 + i * 4)?.to_le_bytes());
    }
    m.write_bytes(0x03000088, &irq)?;
    word(m, 0x03004a80, 0x03000089)?;
    clear(m)?;
    m.write_u8(TX_FLAG, 0)?;
    m.write_u8(RX_FLAG, m.read_u8(TX_FLAG)?)?;
    m.write_u8(IDLE, 0)?;
    word(m, LAST, 0)?;
    for (dest, id) in [(IDLE - 8, 0x4f5341), (IDLE - 4, 0x425347)] {
        let region = m.read_u8(0x080000af)?;
        word(m, dest, u32::from(region) << 24 | id)?;
    }
    m.write_u16(IF, 0x80)?;
    let ie = m.read_u16(IE)?;
    m.write_u16(IE, ie | 0x80)?;
    m.write_u16(IME, 1)
}
fn maintain(m: Mem) -> R {
    m.write_u16(IME, 0)?;
    if m.read_u8(TIME)? <= 9 {
        m.write_u8(TIME, m.read_u8(TIME)? + 1)?;
    } else if m.read_u8(ACTIVE)? == 0 && m.read_u8(IDLE)? <= 10 {
        m.write_u8(IDLE, m.read_u8(IDLE)? + 1)?;
    } else {
        let saved = m.read_u16(IME)?;
        m.write_u16(IME, 0)?;
        m.write_u16(RCNT, 0x8000)?;
        link(m)?;
        clear(m)?;
        m.write_u8(IDLE, 0)?;
        word(m, LAST, 0)?;
        m.write_u16(IF, 0x80)?;
        m.write_u16(IME, saved)?;
    }
    m.write_u16(IME, 1)
}
fn begin(m: Mem, v: u32, send: bool) -> R {
    let count = ((v >> 16) as u16).swap_bytes();
    if u32::from(count) * 2 > CAP {
        return Err(fail(
            if send { 0xb64 } else { 0xb0c },
            "declared transfer exceeds native record buffer",
        ));
    }
    m.write_u8(TIME, 0)?;
    word(m, PTR, if send { TX_BUF } else { RX_BUF })?;
    m.write_u16(COUNT, count)?;
    m.write_u16(INDEX, 0)?;
    if !send {
        word(m, SIZE, u32::from(count) * 2)?;
    }
    m.write_u8(if send { TX_FLAG } else { RX_FLAG }, 1)
}
fn checked_slot(m: &dyn FrameMemory, base: u32, off: u32) -> R<(u32, u16, u16)> {
    let count = m.read_u16(COUNT)?;
    let index = m.read_u16(INDEX)?;
    let ptr = m.read_u32(PTR)?;
    if count == 0
        || u32::from(count) > CAP / 2
        || index >= count
        || ptr != base + u32::from(index) * 2
    {
        return Err(fail(off, "transfer cursor outside declared record"));
    }
    Ok((ptr, index, count))
}
fn key_ack(m: &dyn FrameMemory) -> R<u32> {
    Ok((u32::from(m.read_u16(KEYS)?.swap_bytes()) << 16) | 0x4b43)
}
fn finish(m: Mem, send: bool) -> R<u32> {
    m.write_u8(TIME, 0)?;
    let ack = key_ack(m)?;
    m.write_u8(PHASE, 5)?;
    m.write_u8(if send { TX_FLAG } else { RX_FLAG }, 2)?;
    Ok(ack)
}
fn receive(m: Mem, v: u32) -> R<u32> {
    let (mut ptr, index, count) = checked_slot(m, RX_BUF, 0x988)?;
    if index + 1 == count && v == CHAO {
        return finish(m, false);
    }
    let next = (v as u16).swap_bytes();
    if u32::from(next) == u32::from(index) + 1 {
        if next >= count {
            return Err(fail(0x988, "receive index beyond declared record"));
        }
        ptr += 2;
        m.write_u16(INDEX, next)?;
        word(m, PTR, ptr)?;
    } else if next != index {
        return Ok(0);
    }
    m.write_u8(TIME, 0)?;
    m.write_bytes(ptr, &(v >> 16).to_le_bytes()[..2])?;
    Ok(v)
}
fn send(m: Mem, v: u32) -> R<u32> {
    let (mut ptr, mut index, count) = checked_slot(m, TX_BUF, 0xa40)?;
    if v == m.read_u32(LAST)? {
        if index + 1 == count {
            return finish(m, true);
        }
        m.write_u8(TIME, 0)?;
        index += 1;
        ptr += 2;
        m.write_u16(INDEX, index)?;
        word(m, PTR, ptr)?;
    }
    Ok(u32::from(m.read_u8(ptr)?) << 16
        | u32::from(m.read_u8(ptr + 1)?) << 24
        | u32::from(index.swap_bytes()))
}
fn greeting(m: Mem) -> R {
    m.read_u32(RX)?;
    let magic = m.read_u32(IDLE - 8)?;
    word(m, TX, magic)?;
    m.write_u16(STAT, 0x20)
}
fn phase(m: Mem, v: u8) -> R {
    m.write_u16(IME, 0)?;
    m.write_u8(PHASE, v)?;
    m.write_u16(IME, 1)
}
fn irq(m: Mem) -> R {
    let flags = m.read_u16(CNT)?;
    let p = m.read_u8(PHASE)?;
    let mut reply = None;
    match p {
        0 | 10..=255 => {
            if flags & 1 != 0 {
                greeting(m)?;
                m.write_u8(PHASE, m.read_u8(PHASE)?.wrapping_add(1))?;
            }
        }
        1 => {
            if flags & 4 != 0 {
                m.write_u8(PHASE, 2)?;
            } else if flags & 1 != 0 {
                greeting(m)?;
            } else {
                m.write_u8(PHASE, 0)?;
            }
        }
        2 => {
            if flags & 2 != 0 {
                if m.read_u32(RX)? == m.read_u32(IDLE - 4)? {
                    m.write_u16(STAT, 0x30)?;
                    m.write_u8(PHASE, 3)?;
                } else {
                    m.write_u8(PHASE, 0)?;
                }
            } else if flags & 1 != 0 {
                greeting(m)?;
                m.write_u8(PHASE, 1)?;
            } else {
                m.write_u8(PHASE, 0)?;
            }
        }
        3 => m.write_u8(PHASE, 4)?,
        4..=9 => {
            if flags & 4 != 0 {
                let last = m.read_u32(LAST)?;
                word(m, TX, last)?;
            } else if flags & 2 != 0 {
                let v = m.read_u32(RX)?;
                reply = Some(match p {
                    4 => {
                        if v == CHAO {
                            m.write_u8(TIME, 0)?;
                            m.write_u8(ACTIVE, 1)?;
                            v
                        } else {
                            0
                        }
                    }
                    5 => {
                        if v == CHAO || v == 0x4f414843 {
                            m.write_u8(TIME, 0)?;
                            m.write_u8(MODE, u8::from(v != CHAO))?;
                            key_ack(m)?
                        } else if v as u8 == 0x2a {
                            begin(m, v, false)?;
                            m.write_u8(PHASE, 6)?;
                            v
                        } else if v as u8 == 0x22 {
                            begin(m, v, true)?;
                            m.write_u8(PHASE, 8)?;
                            v
                        } else {
                            0
                        }
                    }
                    6 => {
                        if v as u16 == 0 {
                            let r = receive(m, v)?;
                            m.write_u8(PHASE, 7)?;
                            r
                        } else if v as u8 == 0x2a {
                            begin(m, v, false)?;
                            v
                        } else {
                            0
                        }
                    }
                    7 => receive(m, v)?,
                    8 => {
                        if v as u8 == 0x22 {
                            begin(m, v, true)?;
                            v
                        } else if v as u8 == 0x26 {
                            let r = send(m, v)?;
                            m.write_u8(PHASE, 9)?;
                            r
                        } else {
                            0
                        }
                    }
                    _ => send(m, v)?,
                });
            }
        }
    }
    if let Some(v) = reply {
        word(m, TX, v)?;
        word(m, LAST, v)?;
    }
    m.write_u8(IDLE, 0)?;
    m.write_u16(CNT, flags)
}
const MAP: u32 = 0x0203f800;
const PANEL: u32 = 0x0600e800;
const HIDE: u32 = 0x0300152c;
// Both message builders draw a bordered tilemap using the language table and
// the native per-line color / font controls. Dimensions are returned as values,
// so the caller does not need a fabricated emulated stack pointer.
fn dialog(m: Mem, id: u8, wide: bool) -> R<(u8, u8)> {
    let off = if wide { 0x2510 } else { 0x2364 };
    let mut row = 0;
    while row < 5 && m.read_u8(0x02026f2c + row)? != id {
        row += 1;
    }
    let lang = u32::from(m.read_u8(FLAGS + 1)?);
    if row >= 5 || lang > 4 {
        return Err(fail(off, "unbound native dialog id or language"));
    }
    let mut src = m.read_u32(if lang < 2 {
        0x02026f04 + 4 * (row * 2 + lang)
    } else {
        0x02029a48 + 4 * (row * 3 + lang - 2)
    })?;
    let w = u32::from(m.read_u8(src)?);
    let h = u32::from(m.read_u8(src + 1)?);
    src += 2;
    let stride = w + if wide { 3 } else { 2 };
    let rows = h + if wide { 4 } else { 2 };
    if stride > 255 || rows > 255 || rows * stride > 1024 {
        return Err(fail(off, "native dialog exceeds tile buffer"));
    }
    for y in 0..rows {
        let (left, mid, right) = if wide {
            if y == 0 {
                (0x1f1, 0x1f2, 0x1f3)
            } else if y == 1 {
                (0x1f4, 0x1f5, 0x1f6)
            } else if y == rows - 1 {
                (0x1fb, 0x1fc, 0x1fd)
            } else if y == rows - 2 {
                (0x1f9, 0x1fa, 0x1f6)
            } else {
                (0x1f7, 0x1f8, 0x1f6)
            }
        } else if y == 0 {
            (0x1ed, 0x1ee, 0x5ed)
        } else if y == rows - 1 {
            (0x9ed, 0x9ee, 0xded)
        } else {
            (0x1ef, 0x1f0, 0x5ef)
        };
        for x in 0..stride {
            m.write_u16(
                MAP + 2 * (y * stride + x),
                if x == 0 {
                    left
                } else if x == stride - 1 {
                    right
                } else {
                    mid
                },
            )?;
        }
    }
    let mut palette = 0u16;
    let mut bank = 0u16;
    let start = src;
    for y in 0..h {
        let mut col = 0;
        loop {
            if src - start >= 4096 {
                return Err(fail(off, "unterminated native dialog text"));
            }
            let b = m.read_u8(src)?;
            src += 1;
            match b {
                0x5f => {
                    let v = m.read_u8(src)?;
                    src += 1;
                    if v & 128 != 0 {
                        break;
                    }
                    palette = u16::from(v & 15) << 12;
                }
                0xff => bank ^= 0x100,
                _ => {
                    if col >= w {
                        return Err(fail(off, "dialog row wider than supplied width"));
                    }
                    m.write_u16(
                        MAP + 2 * ((y + if wide { 2 } else { 1 }) * stride + 1 + col),
                        (0x2af + u16::from(b) + bank).wrapping_add(palette),
                    )?;
                    col += 1;
                }
            }
        }
    }
    Ok((stride as u8, rows as u8))
}
fn paint(m: Mem, c: &mut dyn Calls, id: u8, wide: bool) -> R {
    let (w, h) = dialog(m, id, wide)?;
    let (w, h) = (u32::from(w), u32::from(h));
    let x = if wide { (w - 1) & 0xfe } else { w & 0xfe };
    let y = if wide { (h + 6) & 0xfe } else { h & 0xfe };
    let dst = (PANEL + 30)
        .wrapping_add(20u32.wrapping_sub(y).wrapping_mul(32))
        .wrapping_sub(x);
    c.invoke(
        m,
        0x17b0,
        &[MAP, dst, w, h, if wide { 0xd000 } else { 0x5000 }],
    )?;
    if wide {
        let y = ((20 - y as i32) >> 1) + h as i32;
        c.invoke(
            m,
            0x17b0,
            &[
                0x0201f9fa,
                (PANEL + 30).wrapping_add_signed(y * 64),
                4,
                5,
                0xd1f1,
            ],
        )?;
    }
    Ok(())
}
fn erase(m: Mem, c: &mut dyn Calls) -> R {
    c.invoke(m, 0x17b0, &[0, PANEL, 255, 0, 0])?;
    Ok(())
}
fn clear_flags(m: Mem) -> R {
    m.write_u16(IME, 0)?;
    m.write_u8(TX_FLAG, 0)?;
    m.write_u8(RX_FLAG, m.read_u8(TX_FLAG)?)?;
    m.write_u16(IME, 1)
}
fn commit_received(m: Mem, c: &mut dyn Calls) -> R {
    let size = m.read_u32(SIZE)?;
    if size > CAP || size & 1 != 0 {
        return Err(fail(0x2eb8, "invalid completed transfer length"));
    }
    let old = m.read_u8(TX_BUF)?;
    let mut bytes = Vec::with_capacity(size as usize);
    for i in (0..size).step_by(2) {
        bytes.extend(m.read_u16(RX_BUF + i)?.to_le_bytes());
    }
    if !bytes.is_empty() {
        m.write_bytes(TX_BUF, &bytes)?;
    }
    m.write_u8(RX_FLAG, 0)?;
    m.write_u16(IME, 1)?;
    m.write_u8(TX_BUF + 0x10, 0)?;
    let balance = m.read_u32(TX_BUF + 12)?.swap_bytes();
    word(m, 0x03006194, balance)?;
    if old != 0 && m.read_u8(TX_BUF)? == 0 {
        m.write_u8(0x0300152d, 5)?;
        m.write_u8(0x03004abd, 0x58)?;
        m.write_u8(0x03004abe, 0x68)?;
        word(m, 0x03004b70, 0x34bc0)?;
    }
    c.invoke(m, 0x3ac, &[])?;
    Ok(())
}
fn controller(m: Mem, c: &mut dyn Calls, t: u32) -> R {
    let flags = m.read_u8(FLAGS)?;
    let mut force_draw = false;
    if flags & 0x93 == 0 {
        m.write_u16(IME, 0)?;
        m.read_u8(PHASE)?;
        let active = m.read_u8(ACTIVE)?;
        let mode = m.read_u8(MODE)?;
        let rx = m.read_u8(RX_FLAG)?;
        let tx = m.read_u8(TX_FLAG)?;
        m.write_u16(IME, 1)?;
        if active != 0 {
            if m.read_u8(t + 0x19)? != 0 {
                if m.read_u16(KEYS + 4)? & 1 != 0 {
                    m.write_u8(t + 0x19, 0)?;
                    m.write_u16(t + 14, 15)?;
                    erase(m, c)?;
                    clear_flags(m)?;
                }
            } else {
                if flags & 0x40 != 0 {
                    if mode != 0 && m.read_u8(t + 0x1f)? == 0 {
                        m.write_u8(t + 0x1e, 0)?;
                        m.write_u8(t + 0x1f, 1)?;
                        paint(m, c, 0x38, true)?;
                        erase(m, c)?;
                        let pal = m.read_u16(HIDE - 2)? & 0xe2ff;
                        c.invoke(m, 0x1698, &[0, u32::from(pal)])?;
                    }
                    if mode == 0 && m.read_u8(t + 0x1e)? == 0 {
                        m.write_u8(t + 0x1e, 1)?;
                        m.write_u8(t + 0x1f, 0)?;
                        paint(m, c, 0x35, false)?;
                        erase(m, c)?;
                        let pal = m.read_u16(HIDE - 2)?;
                        c.invoke(m, 0x1698, &[0, u32::from(pal)])?;
                    }
                    m.write_u16(IME, 0)?;
                    if m.read_u8(TX_FLAG)? == 2 {
                        m.write_u8(TX_FLAG, 0)?;
                    }
                    if m.read_u8(RX_FLAG)? == 2 {
                        commit_received(m, c)?;
                    }
                    m.write_u16(IME, 1)?;
                }
                let y = m.read_u16(t + 14)? as i16;
                if y <= 15 {
                    if y <= -48 {
                        for i in 0..6 {
                            let p = c.invoke(m, 0xd04, &[0x0200338d, t, 0])?;
                            word(m, p + 16, 0x0201fe60)?;
                            m.write_u16(p + 20, 0x14b)?;
                            m.write_u8(p + 26, i * 42)?;
                            m.write_u8(p + 27, m.read_u8(t + 4)?)?;
                        }
                        c.invoke(m, 0x1066c, &[0x22])?;
                    }
                    let f = m.read_u8(FLAGS)? | 0x20;
                    m.write_u8(FLAGS, f)?;
                    let y = m.read_u16(t + 14)?;
                    m.write_u16(t + 14, y.wrapping_add(1))?;
                    if y.wrapping_add(1) as i16 > 15 {
                        if m.read_u8(HIDE)? != 0 {
                            m.write_u16(t + 14, y)?;
                        } else {
                            m.write_u8(FLAGS, f | 0x40)?;
                            m.write_u8(t + 0x1e, 1)?;
                            paint(m, c, 0x35, false)?;
                            c.invoke(m, 0x107a0, &[])?;
                            c.invoke(m, 0x1066c, &[14])?;
                            phase(m, 5)?;
                        }
                    }
                }
            }
        } else if rx == 1 || tx == 1 {
            if m.read_u8(t + 0x19)? == 0 {
                m.write_u8(t + 0x19, 1)?;
                paint(m, c, 0x37, false)?;
                c.invoke(m, 0x1066c, &[14])?;
            } else if m.read_u16(KEYS + 4)? & 1 != 0 {
                m.write_u8(t + 0x1e, 0)?;
                m.write_u8(t + 0x19, 0)?;
                m.write_u8(FLAGS, flags & !0x40)?;
                erase(m, c)?;
                c.invoke(m, 0x106e4, &[1])?;
                clear_flags(m)?;
            }
            force_draw = true;
        } else {
            if m.read_u16(t + 0x1e)? != 0 {
                if m.read_u8(t + 0x1e)? != 0 {
                    m.write_u8(t + 0x1e, 0)?;
                } else {
                    m.write_u8(t + 0x1f, 0)?;
                    let p = m.read_u16(HIDE - 2)?;
                    c.invoke(m, 0x1698, &[0, p.into()])?;
                }
                m.write_u8(FLAGS, m.read_u8(FLAGS)? & !0x40)?;
                erase(m, c)?;
                c.invoke(m, 0x106e4, &[1])?;
            }
            let y = m.read_u16(t + 14)? as i16;
            if y > -48 && (y <= 15 || m.read_u8(HIDE)? == 0) {
                m.write_u16(t + 14, (y - 1) as u16)?;
                if y - 1 <= -48 {
                    m.write_u8(FLAGS, m.read_u8(FLAGS)? & !0x20)?;
                }
            }
        }
    }
    if force_draw || m.read_u16(t + 14)? as i16 > -48 {
        let slot = m.read_u8(t + 4)?;
        c.invoke(m, 0x1398, &[slot.into(), 0xa0])?;
    }
    m.write_u8(HIDE, 0)
}
fn orbit(m: Mem, c: &mut dyn Calls, t: u32) -> R {
    if m.read_u8(FLAGS)? & 32 == 0 {
        c.invoke(m, 0xed4, &[t])?;
        return Ok(());
    }
    m.write_u8(
        t + 0x18,
        if m.read_u32(FLAGS - 7)? & 0x28 != 0 {
            0xac
        } else {
            0x8c
        },
    )?;
    let angle = m.read_u8(t + 26)?.wrapping_add(1);
    m.write_u8(t + 26, angle)?;
    let parent = 0x03005320 + 36 * u32::from(m.read_u8(t + 27)?);
    let s = m.read_u16(0x0201f2e0 + 2 * u32::from(angle))? as i16;
    let x = m
        .read_u32(parent + 8)?
        .wrapping_add_signed(i32::from(s) * 5120);
    word(m, t + 8, x)?;
    let k = m.read_u16(0x0201f2e0 + 2 * u32::from(angle.wrapping_add(192)))? as i16;
    let y = (i32::from(m.read_u16(parent + 14)? as i16) + 23)
        .wrapping_shl(16)
        .wrapping_add(i32::from(k) << 11);
    word(m, t + 12, y as u32)?;
    let slot = m.read_u8(t + 4)?;
    c.invoke(
        m,
        0x1398,
        &[slot.into(), if angle & 128 != 0 { 0x9f } else { 0xa1 }],
    )?;
    Ok(())
}
pub fn dispatch(m: Mem, c: &mut dyn Calls, off: u32, args: &[u32]) -> R<u32> {
    if matches!(off, 0x2eb8 | 0x338c)
        && args
            .first()
            .is_some_and(|t| !(0x03000000..=0x03007fdc).contains(t))
    {
        return Err(fail(off, "task record outside bounded IWRAM"));
    }
    match (off, args) {
        (0x2eb8, &[t]) => controller(m, c, t)?,
        (0x338c, &[t]) => orbit(m, c, t)?,
        (off @ (0x2364 | 0x2510), &[id, w, h]) => {
            let (x, y) = dialog(m, id as u8, off == 0x2510)?;
            m.write_u8(w, x)?;
            m.write_u8(h, y)?;
        }
        (0x480, []) => init(m)?,
        (0x590, []) => maintain(m)?,
        (0x6b8, []) => irq(m)?,
        (0x988, &[v]) => return receive(m, v),
        (0xa40, &[v]) => return send(m, v),
        (0xaf0, &[p]) => {
            phase(m, p as u8)?;
            return Ok(1);
        }
        (0xb0c, &[v]) => begin(m, v, false)?,
        (0xb64, &[v]) => begin(m, v, true)?,
        (0xbb0, []) => {
            greeting(m)?;
            return Ok(0x20);
        }
        _ => return Err(fail(off, "unknown Garden transfer entry or arguments")),
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::super::{
        garden_save,
        memory::{MemoryImage, Region},
    };
    use super::*;
    use serde_json::{json, Value};
    use std::cell::RefCell;
    const P: u32 = 0x02000000;
    const I: u32 = 0x03000000;
    const J: u32 = 0x04000000;
    fn num(v: &Value) -> u32 {
        v.as_u64().unwrap() as u32
    }
    struct Bus {
        mem: MemoryImage<'static>,
        trace: RefCell<Vec<u8>>,
        cart: u8,
    }
    impl Bus {
        fn record(&self, write: bool, a: u32, w: u8, v: u32) {
            if (J..J + 0x1000).contains(&a) {
                let mut t = self.trace.borrow_mut();
                t.extend([u8::from(write), w]);
                t.extend(a.to_le_bytes());
                t.extend(v.to_le_bytes());
            }
        }
        fn poke(&mut self, a: u32, n: u32, v: u32) {
            self.mem
                .write_bytes(a, &v.to_le_bytes()[..n as usize])
                .unwrap();
        }
    }
    impl FrameMemory for Bus {
        fn read_u8(&self, a: u32) -> R<u8> {
            if a == 0x080000af {
                return Ok(self.cart);
            }
            let v = FrameMemory::read_u8(&self.mem, a)?;
            self.record(false, a, 1, v.into());
            Ok(v)
        }
        fn read_u16(&self, a: u32) -> R<u16> {
            let v = FrameMemory::read_u16(&self.mem, a)?;
            self.record(false, a, 2, v.into());
            Ok(v)
        }
        fn read_u32(&self, a: u32) -> R<u32> {
            let v = FrameMemory::read_u32(&self.mem, a)?;
            self.record(false, a, 4, v);
            Ok(v)
        }
        fn write_u8(&mut self, a: u32, v: u8) -> R {
            self.record(true, a, 1, v.into());
            FrameMemory::write_u8(&mut self.mem, a, v)
        }
        fn write_u16(&mut self, a: u32, v: u16) -> R {
            self.record(true, a, 2, v.into());
            FrameMemory::write_u16(&mut self.mem, a, v)
        }
        fn write_bytes(&mut self, a: u32, b: &[u8]) -> R {
            if (J..J + 0x1000).contains(&a) {
                self.record(true, a, 4, u32::from_le_bytes(b.try_into().unwrap()));
            }
            FrameMemory::write_bytes(&mut self.mem, a, b)
        }
    }
    #[derive(Default)]
    struct Boundaries {
        log: Vec<Value>,
        flash: Vec<String>,
        allocated: u32,
    }
    impl garden_save::Calls for Boundaries {
        fn call(&mut self, m: Mem, c: garden_save::Call) -> Result<u32, garden_save::Error> {
            self.log.push(json!([c.entry_offset(), c.arguments()]));
            if let garden_save::Call::ProgramFlash { .. } = c {
                let record = (0..garden_save::RECORD_LENGTH as u32)
                    .map(|i| m.read_u8(garden_save::SAVE + i))
                    .collect::<R<Vec<_>>>()?;
                assert!(crate::garden::read_record(&record).is_ok());
                self.flash.push(crate::sha256(&record));
            }
            Ok(0)
        }
    }
    impl Calls for Boundaries {
        fn invoke(&mut self, m: Mem, off: u32, args: &[u32]) -> R<u32> {
            self.log.push(json!([off, args]));
            match off {
                0x3ac => {
                    garden_save::write(m, self).map_err(|e| fail(off, &e.to_string()))?;
                }
                0xd04 => {
                    self.allocated += 1;
                    return Ok(I + 0x5320 + 36 * (5 + self.allocated));
                }
                0x1398 | 0x17b0 | 0xed4 | 0x1698 | 0x1066c | 0x106e4 | 0x107a0 => {}
                _ => return Err(fail(off, "unexpected native call")),
            }
            Ok(0)
        }
    }
    fn region(a: u32, b: Vec<u8>, w: bool) -> Region {
        Region {
            base: a,
            bytes: b,
            writable: w,
        }
    }
    fn run(v: &Value, profile: &str, data: Vec<u8>, cart: u8) {
        let mut count = 0;
        for (case, expected) in v["cases"]
            .as_array()
            .unwrap()
            .iter()
            .zip(v["results"][profile].as_array().unwrap())
        {
            count += 1;
            let mut bus = Bus {
                mem: MemoryImage::fixture(vec![
                    region(P, data.clone(), true),
                    region(I, (0..0x8000).map(|i| (i * 13 + 7) as u8).collect(), true),
                    region(J, vec![0; 0x1000], true),
                ]),
                cart,
                trace: RefCell::default(),
            };
            for w in v["base"]
                .as_array()
                .unwrap()
                .iter()
                .chain(case["writes"].as_array().unwrap())
            {
                bus.poke(num(&w[0]), num(&w[1]), num(&w[2]));
            }
            let mut c = Boundaries::default();
            let send_irq = |b: &mut Bus, c: &mut Boundaries, f: u32, val: u32| {
                b.poke(CNT, 2, f);
                b.poke(
                    RX,
                    4,
                    if val == 0x45425347 {
                        (u32::from(cart) << 24) | 0x425347
                    } else {
                        val
                    },
                );
                dispatch(b, c, 0x6b8, &[]).unwrap();
            };
            for step in case["steps"].as_array().unwrap() {
                let a = step.as_array().unwrap().iter().map(num).collect::<Vec<_>>();
                match a.as_slice() {
                    [0, off, args @ ..] => {
                        dispatch(&mut bus, &mut c, *off, args)
                            .unwrap_or_else(|e| panic!("{}: {e}", case["name"]));
                    }
                    [1, addr, n, val] => bus.poke(*addr, *n, *val),
                    [2, f, val] => send_irq(&mut bus, &mut c, *f, *val),
                    [3, n] => {
                        for i in 0..*n {
                            send_irq(
                                &mut bus,
                                &mut c,
                                2,
                                (((i * 23 + 0x100) & 65535) << 16) | ((i & 255) << 8) | (i >> 8),
                            );
                        }
                    }
                    _ => panic!("unknown native fixture step"),
                }
            }
            assert_eq!(json!(c.log), expected["calls"], "{} calls", case["name"]);
            assert_eq!(json!(c.flash), expected["flash"], "{} flash", case["name"]);
            assert_eq!(
                crate::sha256(&bus.trace.into_inner()),
                expected["mmio"],
                "{} MMIO",
                case["name"]
            );
            assert_eq!(
                crate::sha256(bus.mem.bytes(MAP, 0x800).unwrap()),
                expected["map"],
                "{} tilemap",
                case["name"]
            );
            assert_eq!(
                crate::sha256(bus.mem.bytes(I, 0x7800).unwrap()),
                expected["ram"],
                "{} RAM",
                case["name"]
            );
        }
        assert_eq!(count, v["cases"].as_array().unwrap().len());
    }
    #[test]
    fn native_link_matches_opaque_records_registers_and_flash() {
        let v: Value =
            serde_json::from_str(include_str!("../../tests/garden-transfer-vectors.json")).unwrap();
        let mut data = vec![0; 0x40000];
        for (i, b) in data[0x6b8..0x6b8 + 0x520].iter_mut().enumerate() {
            *b = (i * 11 + 1) as u8;
        }
        for r in v["syntheticTables"].as_array().unwrap() {
            let p = num(&r[0]) as usize;
            for (i, b) in r[1].as_array().unwrap().iter().enumerate() {
                data[p + i] = num(b) as u8;
            }
        }
        run(&v, "synthetic", data, 0xbc);
        if let Some(path) = std::env::var_os("SPP_GARDEN_PAYLOAD") {
            let mut payload = std::fs::read(path).unwrap();
            assert_eq!(crate::sha256(&payload), v["payloadSha256"]);
            for s in v["sources"].as_array().unwrap() {
                let start = num(&s["offset"]) as usize;
                assert_eq!(
                    crate::sha256(&payload[start..start + num(&s["length"]) as usize]),
                    s["sha256"]
                );
            }
            payload.resize(0x40000, 0);
            run(&v, "authentic", payload, num(&v["region"]) as u8);
        }
    }
    #[test]
    fn invalid_remote_span_and_corrupt_cursor_cannot_overwrite_other_records() {
        let mut m = MemoryImage::fixture(vec![region(I, vec![0x5a; 0x8000], true)]);
        let before = crate::sha256(m.bytes(I, 0x8000).unwrap());
        assert!(begin(&mut m, 0x0104002a, false).is_err());
        assert_eq!(crate::sha256(m.bytes(I, 0x8000).unwrap()), before);
        begin(&mut m, 0x0100002a, false).unwrap();
        assert!(receive(&mut m, 0xabcd0100).is_err());
        word(&mut m, PTR, TX_BUF).unwrap();
        assert!(receive(&mut m, 0xabcd0000).is_err());
        assert_eq!(m.bytes(TX_BUF, 2).unwrap(), [0x5a; 2]);
    }
}
