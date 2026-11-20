//! Native A3VE Garden shop and carried item tasks. All pointers refer to the
//! decoded payload or its IWRAM. The save and working balance are local game state.
use super::frame::{FrameError, FrameMemory, NativeCall};

pub trait Calls {
    fn invoke(
        &mut self,
        m: &mut dyn FrameMemory,
        offset: u32,
        args: &[u32],
    ) -> std::result::Result<u32, FrameError>;
}
type Mem<'a> = &'a mut dyn FrameMemory;
type Result<T = ()> = std::result::Result<T, FrameError>;
const P: u32 = 0x02000000;
const SAVE: u32 = 0x03004aa0;
const FLAGS: u32 = 0x0300425f;
const FRAME: u32 = 0x03004258;
const INPUT: u32 = 0x03005310;
const TASKS: u32 = 0x03005320;
const HAND: u32 = 0x0300150c;
const CHAO: u32 = 0x03001510;
const LOCK: u32 = 0x03001509;
const EDGE: u32 = 0x03001528;
const COUNT: u32 = 0x0300159a;
const RINGS: u32 = 0x03006194;
const GLYPHS: u32 = 0x030015d0;
const MAP: u32 = P + 0x216a0;
const PRICES: [u32; 3] = [P + 0x126d4, P + 0x126f0, P + 0x12720];
fn word(m: Mem, a: u32, v: u32) -> Result {
    m.write_bytes(a, &v.to_le_bytes())
}
fn signed(m: &dyn FrameMemory, a: u32) -> Result<i32> {
    Ok(m.read_u16(a)? as i16 as i32)
}
fn field(m: &dyn FrameMemory, p: u32, delta: u32) -> Result<u32> {
    Ok(m.read_u32(p)?.wrapping_add(delta))
}
fn position(m: &dyn FrameMemory, t: u32) -> Result<u32> {
    Ok(SAVE + 0x21 + 2 * u32::from(m.read_u8(t + 0x19)?))
}
fn menu_x(m: &dyn FrameMemory, t: u32) -> Result<i32> {
    signed(m, TASKS + 0x24 * u32::from(m.read_u8(t + 0x1b)?) + 0xa)
}
fn draw(m: Mem, c: &mut impl Calls, t: u32, layer: u32) -> Result {
    let id = m.read_u8(t + 4)?;
    c.invoke(m, 0x1398, &[id.into(), layer])?;
    Ok(())
}
// 15CC, specialized to five positions and a glyph offset of one. Leading
// blanks clear the *previous* record's continuation byte (including row - 1).
fn digits(mut value: i32) -> [u8; 5] {
    let mut out = [0; 5];
    for d in out.iter_mut().rev() {
        if value <= 9 {
            *d = (value as u8).wrapping_add(1);
            break;
        }
        *d = (value % 10 + 1) as u8;
        value /= 10;
    }
    out
}
fn initialize(m: Mem, t: u32) -> Result {
    for row in 0..22 {
        for j in (0..40).step_by(4) {
            let v = m.read_u32(P + 0x1fe08 + j)?;
            word(m, GLYPHS + 40 * row + j, v)?;
        }
    }
    for (category, count, start) in [(0, 7, 0), (1, 12, 7), (2, 3, 19)] {
        for i in 0..count {
            let price = m.read_u32(PRICES[category] + 4 * i)?;
            let row = GLYPHS + 40 * (start + i);
            for (j, d) in digits(price as i32).into_iter().rev().enumerate() {
                let glyph = row + 8 * j as u32;
                if d == 0 {
                    m.write_u8(glyph - 1, 0)?
                } else {
                    m.write_u16(glyph, u16::from(d - 1))?
                }
            }
        }
    }
    m.write_u16(t + 0xa, (-48i16) as u16)?;
    m.write_u16(t + 0xe, 0x50)?;
    word(m, t + 0x10, P + 0x1fda8)?;
    m.write_u16(t + 0x14, 0x115)?;
    m.write_u8(t + 0x18, 0xfc)?;
    m.write_u8(t + 0x1e, 0)?;
    word(m, t, P + 0x7689)
}
fn random_item(m: Mem, c: &mut impl Calls) -> Result<u8> {
    if m.read_u8(SAVE + 0x58)? == 0xff {
        return Ok(0);
    }
    let last = m.read_u8(0x03001530)?;
    if last != 0xff {
        return Ok(last);
    }
    let rare = m.read_u8(SAVE + 0x68)? == 0 && c.invoke(m, 0x1534, &[])? as i16 <= 0x1fff;
    let base = if rare { m.read_u8(SAVE + 0x1c)? } else { 0 };
    let n = c.invoke(m, 0x1534, &[])? as i16;
    let selected = if rare {
        let delta = match n {
            ..=0x40a3 => 1,
            0x40a4..=0x4147 => 0,
            0x4148..=0x4b84 => 3,
            0x4b85..=0x65c1 => 4,
            _ => 2,
        };
        let x = u16::from(base) + delta;
        ((if x > 4 { x - 5 } else { x }) as u8).wrapping_add(7)
    } else {
        match n {
            ..=0x1999 => 0,
            0x199a..=0x3332 => 2,
            0x3333..=0x4665 => 1,
            0x4666..=0x5998 => 3,
            0x5999..=0x6665 => 4,
            0x6666..=0x7332 => 6,
            _ => 5,
        }
    };
    m.write_u8(0x03001530, selected)?;
    Ok(selected)
}
fn open(m: Mem, c: &mut impl Calls, t: u32) -> Result {
    if m.read_u8(SAVE + 0x1c)? == 0xff {
        let frame = m.read_u32(FRAME)? as i32;
        m.write_u8(SAVE + 0x1c, (frame % 5) as u8)?;
    }
    for i in 0..7 {
        let child = c.invoke(m, 0xd04, &[P + 0x8b3d, t, 0])?;
        m.write_u16(child + 0xe, (0x1a + 16 * i) as u16)?;
        m.write_u8(child + 0x1a, m.read_u8(P + 0x126cd + i)?)?;
        m.write_u8(child + 0x1b, m.read_u8(t + 4)?)?;
    }
    if m.read_u8(SAVE + 0x59)? == 0xff && m.read_u8(0x03001501)? == 0 {
        let item = random_item(m, c)?;
        let child = c.invoke(m, 0xd04, &[P + 0x8b7d, t, 0])?;
        m.write_u8(child + 0x1a, item)?;
        m.write_u8(child + 0x1b, m.read_u8(t + 4)?)?;
    }
    let next = if m.read_u8(SAVE + 0x31)? == 0 {
        Some(0)
    } else if m.read_u8(SAVE + 0x33)? == 0 {
        Some(1)
    } else if m.read_u8(SAVE + 0x35)? == 0 && m.read_u32(SAVE + 0x14)? == 0 {
        Some(2)
    } else {
        None
    };
    if let Some(item) = next {
        let child = c.invoke(m, 0xd04, &[P + 0x8bc5, t, 0])?;
        m.write_u8(child + 0x1a, item)?;
        m.write_u8(child + 0x1b, m.read_u8(t + 4)?)?;
    }
    m.write_u8(SAVE + 0xce, 1)
}
fn panel(m: Mem, c: &mut impl Calls, t: u32) -> Result {
    if m.read_u8(FLAGS)? & 0x10 != 0 {
        return Ok(());
    }
    let timer = m.read_u32(SAVE + 0x14)?;
    if timer != 0 {
        word(m, SAVE + 0x14, timer - 1)?
    }
    let flags = m.read_u8(FLAGS)?;
    if flags & 0x80 == 0 {
        let x = signed(m, t + 0xa)?;
        if m.read_u8(t + 0x1e)? != 0 {
            if x < 0 {
                m.write_u16(t + 0xa, (x + 4) as u16)?
            } else if m.read_u16(INPUT + 4)? & 0x200 != 0
                || flags & 0x24 != 0
                || m.read_u8(field(m, HAND, 7)?)? != 0
            {
                m.write_u8(t + 0x1e, 0)?
            }
        } else if x > -48 {
            m.write_u16(t + 0xa, (x - 4) as u16)?
        } else if m.read_u16(INPUT + 4)? & 0x200 != 0
            && flags & 0x24 == 0
            && m.read_u8(field(m, HAND, 7)?)? == 0
        {
            m.write_u8(t + 0x1e, 1)?;
            m.write_u16(t + 0xa, (x + 4) as u16)?;
            open(m, c, t)?;
        }
    }
    let x = signed(m, t + 0xa)?;
    m.write_u8(EDGE, (x + 0x3b) as u8)?;
    if x > -48 {
        draw(m, c, t, 0xa0)?
    }
    Ok(())
}
fn purchase(m: Mem, c: &mut impl Calls, t: u32, category: usize) -> Result {
    let flags = m.read_u8(FLAGS)?;
    if flags & 0x10 != 0 {
        return Ok(());
    }
    let panel = menu_x(m, t)?;
    if panel <= -48 {
        c.invoke(m, 0xed4, &[t])?;
        return Ok(());
    }
    if panel >= 0 {
        let hand = m.read_u32(HAND)?;
        if signed(m, hand + 0xa)? <= 51
            && m.read_u8(hand + 7)? == 0
            && m.read_u8(LOCK)? == 0
            && flags & 0x80 == 0
        {
            let y = m.read_u16(hand + 0xe)?.wrapping_add(8) as i16 as i32;
            let row = signed(m, t + 0xe)?;
            if y <= row && row - 16 < y {
                let price = |m: &dyn FrameMemory| {
                    m.read_u32(PRICES[category] + 4 * u32::from(m.read_u8(t + 0x1a)?))
                };
                let funds = m.read_u32(RINGS)? >= price(m)?;
                let available = funds && (category != 0 || m.read_u8(COUNT)? <= 7);
                let confirm = m.read_u16(INPUT + 4)? & 1 != 0 && m.read_u16(INPUT)? & 0xf0 == 0;
                if available && confirm {
                    c.invoke(m, 0x1066c, &[0x19])?;
                    let id = m.read_u8(t + 0x1a)?;
                    match category {
                        0 => {
                            let mut slot = 0;
                            while slot < 8 && m.read_u8(SAVE + 0x5a + slot)? != 0xff {
                                slot += 1
                            }
                            // 7A7E falls through at slot 8 if the count and slots disagree.
                            m.write_u8(SAVE + 0x5a + slot, id)?;
                            m.write_u8(t + 0x19, slot as u8)?;
                            m.write_u8(COUNT, m.read_u8(COUNT)?.wrapping_add(1))?;
                        }
                        1 => m.write_u8(SAVE + 0x59, id)?,
                        _ if id == 1 => word(m, SAVE + 0x14, 0x34bc0)?,
                        _ => {}
                    }
                    let remaining = m.read_u32(RINGS)?.wrapping_sub(price(m)?);
                    word(m, RINGS, remaining)?;
                    m.write_u8(field(m, HAND, 7)?, 1)?;
                    m.write_u8(0x0300152c, 1)?;
                    c.invoke(m, [0x8c9c, 0x8cd4, 0x8d04][category], &[t])?;
                    return Ok(());
                }
                if !available && confirm {
                    c.invoke(m, 0x1066c, &[0x1c])?;
                }
                if available && m.read_u32(FRAME)? & 0x18 == 0 {
                    return Ok(());
                }
            }
        }
    }
    m.write_u16(t + 0xa, (panel + 8) as u16)?;
    draw(m, c, t, 0xa1)
}

fn sprite(m: Mem, t: u32, category: usize, carried: bool) -> Result {
    let id = u32::from(m.read_u8(t + 0x1a)?);
    word(m, t + 0x10, P + 0x1fd50)?;
    let tile = match category {
        0 => 0xbc + 4 * id,
        1 if id == 0 => 0xd8,
        1 => 0xdc,
        _ => 0xe0 + 4 * id,
    };
    m.write_u16(t + 0x14, tile as u16)?;
    let bank = match category {
        1 => 14,
        0 => m.read_u8(P + 0x20668 + id)?,
        _ => m.read_u8(P + 0x2066f + id)?,
    };
    m.write_u8(t + 0x18, bank << 4 | if carried { 4 } else { 12 })
}
fn row_init(m: Mem, c: &mut impl Calls, t: u32, category: usize) -> Result {
    match category {
        1 => m.write_u16(t + 0xe, 0x8a)?,
        2 => m.write_u16(t + 0xe, 0x9a)?,
        _ => {}
    }
    sprite(m, t, category, false)?;
    let index = m.read_u8(t + 0x1a)?.wrapping_add([0, 7, 19][category]);
    c.invoke(m, 0x8c0c, &[t, index.into()])?;
    if category == 1 {
        m.write_u8(0x030015ad, m.read_u8(t + 0x1a)?)?;
        m.write_u8(0x030015ac, 1)?;
    }
    word(m, t, P + [0x7979, 0x7b01, 0x7c4d][category])
}
fn price_label(m: Mem, c: &mut impl Calls, t: u32, index: u32) -> Result {
    let child = c.invoke(m, 0xd04, &[P + 0x8c51, t, 0])?;
    m.write_u16(child + 0xe, m.read_u16(t + 0xe)?.wrapping_sub(10))?;
    word(m, child + 0x10, GLYPHS + 40 * (index & 255))?;
    m.write_u16(child + 0x14, 0x141)?;
    m.write_u8(child + 0x18, 0x1c)?;
    m.write_u8(child + 0x1b, m.read_u8(t + 0x1b)?)
}
fn label_frame(m: Mem, c: &mut impl Calls, t: u32) -> Result {
    if m.read_u8(FLAGS)? & 0x10 == 0 {
        let x = menu_x(m, t)?;
        if x <= -48 {
            c.invoke(m, 0xed4, &[t])?;
        } else {
            m.write_u16(t + 0xa, x as u16)?;
            draw(m, c, t, 0xa1)?;
        }
    }
    Ok(())
}
fn carried_position(m: &dyn FrameMemory, t: u32, category: usize) -> Result<u32> {
    match category {
        0 => position(m, t),
        1 => Ok(SAVE + 0x1f),
        _ => Ok(SAVE + 0x31 + 2 * u32::from(m.read_u8(t + 0x1a)?)),
    }
}
fn touches_chao(m: &dyn FrameMemory, t: u32) -> Result<bool> {
    if m.read_u8(SAVE + 0x58)? != 0 || m.read_u8(0x0300152f)? != 0 {
        return Ok(false);
    }
    let p = m.read_u32(CHAO)?;
    if m.read_u8(p + 0x1e)? & 1 == 0 {
        return Ok(false);
    }
    let cx = signed(m, p + 0xa)?;
    let x = signed(m, t + 0xa)?;
    if x <= cx - 11 || x >= cx + 11 {
        return Ok(false);
    }
    let cy = signed(m, p + 0xe)?;
    let y = signed(m, t + 0xe)?;
    Ok(y > cy - 18 && y < cy + 10)
}
fn release_hand(m: Mem, c: &mut impl Calls) -> Result {
    c.invoke(m, 0x1066c, &[0x16])?;
    m.write_u8(field(m, HAND, 7)?, 0)?;
    m.write_u16(field(m, HAND, 0x14)?, 0xb4)
}
fn beside_chao(m: Mem, t: u32, category: usize) -> Result<bool> {
    let p = m.read_u32(CHAO)?;
    let x = signed(m, p + 0xa)?;
    let right = x <= if signed(m, p + 0xe)? > 103 { 139 } else { 87 };
    m.write_u8(
        carried_position(m, t, category)?,
        (x + if right { 16 } else { -16 }) as u8,
    )?;
    m.write_u8(
        carried_position(m, t, category)? + 1,
        m.read_u8(field(m, CHAO, 0xe)?)?.wrapping_sub(2),
    )?;
    Ok(right)
}
fn food_contact(m: Mem, c: &mut impl Calls, t: u32) -> Result {
    release_hand(m, c)?;
    let room = 200 - i32::from(m.read_u8(SAVE + 0x74)?);
    let id = u32::from(m.read_u8(t + 0x1a)?);
    let effect = m.read_u8(P + 0x1269d + 7 * id)? as i8 as i32;
    if effect * 10 > room {
        m.write_u8(field(m, CHAO, 7)?, 8)?;
        m.write_u8(field(m, CHAO, 0x17)?, 11)?;
        m.write_u16(field(m, CHAO, 0x14)?, 16)?;
        beside_chao(m, t, 0)?;
        c.invoke(m, 0x8324, &[t])?;
    } else {
        m.write_u8(field(m, CHAO, 0x17)?, 13)?;
        m.write_u8(field(m, CHAO, 7)?, 16)?;
        m.write_u8(field(m, CHAO, 0x1b)?, m.read_u8(t + 0x1a)?)?;
        let p = m.read_u32(CHAO)?;
        if signed(m, p + 0xa)? < signed(m, t + 0xa)? {
            m.write_u8(p + 7, m.read_u8(p + 7)? | 0x80)?
        }
        m.write_u8(SAVE + 0x76, 0)?;
        m.write_u8(SAVE + 0x5a + u32::from(m.read_u8(t + 0x19)?), 0xff)?;
        m.write_u8(COUNT, m.read_u8(COUNT)?.wrapping_sub(1))?;
        c.invoke(m, 0xed4, &[t])?;
    }
    Ok(())
}
fn special_contact(m: Mem, c: &mut impl Calls, t: u32) -> Result {
    release_hand(m, c)?;
    let right = beside_chao(m, t, 2)?;
    if m.read_u8(SAVE + 0x73)? <= 49 {
        c.invoke(m, 0x83e4, &[t])?;
    } else {
        m.write_u8(field(m, CHAO, 0x17)?, 20)?;
        m.write_u8(field(m, CHAO, 0x1b)?, m.read_u8(t + 4)?)?;
        m.write_u8(field(m, CHAO, 7)?, if right { 128 } else { 0 })?;
        m.write_u8(t + 0x1d, m.read_u8(field(m, CHAO, 0x1d)?)?.wrapping_sub(1))?;
        let bank = m.read_u8(P + 0x2066f + u32::from(m.read_u8(t + 0x1a)?))?;
        m.write_u8(t + 0x18, bank << 4 | 12)?;
        m.write_u8(t + 0x1c, 0xff)?;
        word(m, t, P + 0x8ded)?;
        c.invoke(m, 0x8dec, &[t])?;
    }
    Ok(())
}
fn carried(m: Mem, c: &mut impl Calls, t: u32, category: usize) -> Result {
    if category == 2 && m.read_u8(t + 0x1a)? == 1 {
        m.write_u8(SAVE + 0xcf, 0)?
    }
    if m.read_u8(LOCK)? != 0 {
        return Ok(());
    }
    if m.read_u8(FLAGS)? & 0x80 == 0 {
        let hand = m.read_u32(HAND)?;
        m.write_u16(t + 0xa, m.read_u16(hand + 0xa)?)?;
        m.write_u8(
            carried_position(m, t, category)?,
            m.read_u16(hand + 0xa)? as u8,
        )?;
        let y = m.read_u16(field(m, HAND, 0xe)?)?.wrapping_add(19);
        m.write_u16(t + 0xe, y)?;
        m.write_u8(carried_position(m, t, category)? + 1, y as u8)?;
        if m.read_u16(INPUT + 4)? & 1 != 0
            && signed(m, field(m, HAND, 0xa)?)? > m.read_u8(EDGE)? as i8 as i32
            && m.read_u16(INPUT)? & 0xf0 == 0
        {
            if category != 1 && (category == 0 || m.read_u8(t + 0x1a)? == 0) && touches_chao(m, t)?
            {
                return if category == 0 {
                    food_contact(m, c, t)
                } else {
                    special_contact(m, c, t)
                };
            }
            if signed(m, t + 0xe)? > 159 {
                let pos = carried_position(m, t, category)?;
                m.write_u16(t + 0xe, 159)?;
                m.write_u8(pos + 1, 159)?;
            }
            let tile = (signed(m, t + 0xe)? >> 3) * 22 + (signed(m, t + 0xa)? >> 3);
            let solid = m.read_u8(MAP.wrapping_add_signed(tile))? != 0;
            let needs_solid = category == 2 && m.read_u8(t + 0x1a)? == 1;
            if solid == needs_solid {
                release_hand(m, c)?;
                c.invoke(m, [0x8324, 0x3e04, 0x83e4][category], &[t])?;
                return Ok(());
            }
            c.invoke(m, 0x1066c, &[0x1c])?;
        }
    }
    let id = m.read_u8(t + 4)?;
    c.invoke(m, 0x1254, &[id.into(), 1])?;
    Ok(())
}
fn restore_food(m: Mem, c: &mut impl Calls, t: u32) -> Result {
    let pos = position(m, t)?;
    m.write_u16(t + 0xa, m.read_u8(pos)?.into())?;
    let pos = position(m, t)? + 1;
    if m.read_u8(pos)? > 159 {
        m.write_u8(pos, 159)?
    }
    let y = m.read_u8(position(m, t)? + 1)?;
    m.write_u8(t + 0x1d, y)?;
    m.write_u16(t + 0xe, y.into())?;
    sprite(m, t, 0, false)?;
    m.write_u8(t + 0x1c, 2)?;
    let mut tile = ((u32::from(y) >> 3) * 22 + (signed(m, t + 0xa)? >> 3) as u32) as u16;
    let slot = m.read_u8(t + 0x19)?;
    // Tile indices wrap at 16 bits. After 32,768 decrements by 22 every
    // reachable index repeats, so an all-solid synthetic map cannot hang.
    let mut inspected = 0;
    while m.read_u8(MAP + u32::from(tile))? != 0 {
        if inspected == 32768 {
            return Err(error(0x8324, "no open tile in native search cycle"));
        }
        tile = tile.wrapping_sub(22);
        m.write_u16(t + 0xe, m.read_u16(t + 0xe)?.wrapping_sub(8))?;
        inspected += 1;
    }
    let y = m.read_u16(t + 0xe)? as u8;
    m.write_u8(t + 0x1d, y)?;
    m.write_u8(SAVE + 0x22 + 2 * u32::from(slot), y)?;
    word(m, t, P + 0x8d3d)?;
    c.invoke(m, 0x8d3c, &[t])?;
    Ok(())
}
fn food_frame(m: Mem, c: &mut impl Calls, t: u32) -> Result {
    if m.read_u8(FLAGS)? & 16 == 0 {
        let slot = u32::from(m.read_u8(t + 0x19)?);
        if m.read_u8(SAVE + 0x5a + slot)? == 255 {
            m.write_u8(COUNT, m.read_u8(COUNT)?.wrapping_sub(1))?;
            c.invoke(m, 0x85e0, &[t])?;
        } else {
            m.write_u8(0x03001940 + slot, m.read_u8(t + 4)?)?;
            let layer = m.read_u8(t + 0x1d)?;
            draw(m, c, t, layer.into())?;
        }
    }
    Ok(())
}
fn error(offset: u32, why: &str) -> FrameError {
    FrameError::Dependency {
        call: NativeCall {
            entry_offset: super::garden_graphics::diagnostic_address(offset),
            argument0: None,
        },
        reason: why.into(),
    }
}
pub fn dispatch(m: Mem, c: &mut impl Calls, offset: u32, args: &[u32]) -> Result<u32> {
    if args
        .first()
        .is_some_and(|t| !(0x03000000..=0x03007fdc).contains(t))
    {
        return Err(error(offset, "task record outside bounded IWRAM"));
    }
    match (offset, args) {
        (0x7500, &[t]) => initialize(m, t)?,
        (0x7688, &[t]) => panel(m, c, t)?,
        (0x7978, &[t]) => purchase(m, c, t, 0)?,
        (0x7b00, &[t]) => purchase(m, c, t, 1)?,
        (0x7c4c, &[t]) => purchase(m, c, t, 2)?,
        (0x7da0, &[t]) => carried(m, c, t, 0)?,
        (0x8010, &[t]) => carried(m, c, t, 1)?,
        (0x80e0, &[t]) => carried(m, c, t, 2)?,
        (0x8324, &[t]) => restore_food(m, c, t)?,
        (0x8b3c, &[t]) => row_init(m, c, t, 0)?,
        (0x8b7c, &[t]) => row_init(m, c, t, 1)?,
        (0x8bc4, &[t]) => row_init(m, c, t, 2)?,
        (0x8c0c, &[t, index]) => price_label(m, c, t, index)?,
        (0x8c50, &[t]) => label_frame(m, c, t)?,
        (0x8c9c | 0x8cd4 | 0x8d04, &[t]) => {
            let category = match offset {
                0x8c9c => 0,
                0x8cd4 => 1,
                _ => 2,
            };
            sprite(m, t, category, true)?;
            let successor = [0x7da0, 0x8010, 0x80e0][category];
            word(m, t, P + successor + 1)?;
            c.invoke(m, successor, &[t])?;
        }
        (0x8d3c, &[t]) => food_frame(m, c, t)?,
        _ => return Err(error(offset, "unknown shop callback or invalid arguments")),
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

    #[derive(Default)]
    struct NativeBoundaries(Vec<Value>);
    impl Calls for NativeBoundaries {
        fn invoke(&mut self, m: Mem, offset: u32, args: &[u32]) -> Result<u32> {
            if !matches!(
                offset,
                0xd04
                    | 0x1534
                    | 0x1398
                    | 0x1254
                    | 0xed4
                    | 0x1066c
                    | 0x3e04
                    | 0x83e4
                    | 0x8dec
                    | 0x85e0
            ) {
                return dispatch(m, self, offset, args);
            }
            self.0.push(json!([offset, args]));
            match offset {
                0xd04 => NativeBoardCalls
                    .invoke(P + offset, args, m)
                    .map_err(|e| error(offset, &e.to_string())),
                0x1534 => {
                    let seed = m
                        .read_u32(0x030013a8)?
                        .wrapping_mul(0x41c64e6d)
                        .wrapping_add(12345);
                    word(m, 0x030013a8, seed)?;
                    Ok((seed >> 16) & 0x7fff)
                }
                _ => Ok(0),
            }
        }
    }
    fn number(v: &Value) -> u32 {
        v.as_u64().unwrap() as u32
    }
    fn cases(profile: &str, v: &Value, payload: Vec<u8>) {
        let expected = v["results"][profile].as_array().unwrap();
        let seeds = v["base"].as_array().unwrap();
        for (case, reference) in v["cases"].as_array().unwrap().iter().zip(expected) {
            let mut m = MemoryImage::fixture(vec![
                Region {
                    base: P,
                    bytes: payload.clone(),
                    writable: false,
                },
                Region {
                    base: 0x03000000,
                    bytes: vec![0; 0x8000],
                    writable: true,
                },
            ]);
            for w in seeds.iter().chain(case["writes"].as_array().unwrap()) {
                let a = number(&w[0]);
                let data = number(&w[2]).to_le_bytes();
                m.write_bytes(a, &data[..number(&w[1]) as usize]).unwrap();
            }
            let mut boundaries = NativeBoundaries::default();
            for step in case["steps"].as_array().unwrap() {
                let s = step.as_array().unwrap();
                let args = s[1..].iter().map(number).collect::<Vec<_>>();
                dispatch(&mut m, &mut boundaries, number(&s[0]), &args).unwrap();
            }
            assert_eq!(json!(boundaries.0), reference["calls"], "{}", case["name"]);
            assert_eq!(
                crate::sha256(m.bytes(0x03000000, 0x7800).unwrap()),
                reference["ram"].as_str().unwrap(),
                "{}",
                case["name"]
            );
        }
    }
    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn native_shop_scenarios_match_memory_and_external_calls() {
        let v: Value = serde_json::from_str(vectors!("garden-shop-vectors.json")).unwrap();
        let mut data = vec![0; 0x40000];
        for region in v["syntheticTables"].as_array().unwrap() {
            let base = number(&region[0]) as usize;
            for (i, x) in region[1].as_array().unwrap().iter().enumerate() {
                data[base + i] = number(x) as u8
            }
        }
        cases("synthetic", &v, data);
        if let Ok(path) = std::env::var("SPP_GARDEN_PAYLOAD") {
            let mut data = std::fs::read(path).unwrap();
            assert_eq!(crate::sha256(&data), v["payloadSha256"].as_str().unwrap());
            for src in v["sources"].as_object().unwrap().values() {
                assert_eq!(src["space"], "garden");
                let start = number(&src["offset"]) as usize;
                let end = start + number(&src["length"]) as usize;
                assert_eq!(
                    crate::sha256(&data[start..end]),
                    src["sha256"].as_str().unwrap()
                );
            }
            data.resize(0x40000, 0);
            cases("authentic", &v, data);
        }
    }
    #[test]
    fn unavailable_purchase_audio_stops_before_debit() {
        struct Stop;
        impl Calls for Stop {
            fn invoke(&mut self, _: Mem, off: u32, _: &[u32]) -> Result<u32> {
                Err(error(off, "unbound"))
            }
        }
        let mut m = MemoryImage::fixture(vec![
            Region {
                base: P + 0x126d4,
                bytes: vec![0; 4],
                writable: false,
            },
            Region {
                base: 0x03000000,
                bytes: vec![0; 0x8000],
                writable: true,
            },
        ]);
        let task = TASKS + 0x24 * 2;
        let hand = TASKS + 0x24 * 3;
        word(&mut m, HAND, hand).unwrap();
        word(&mut m, RINGS, 9).unwrap();
        m.write_u16(task + 0xe, 8).unwrap();
        m.write_u16(INPUT + 4, 1).unwrap();
        assert!(
            matches!(dispatch(&mut m,&mut Stop,0x7978,&[task]),Err(FrameError::Dependency{call,..}) if call.entry_offset==P+0x1066c)
        );
        assert_eq!(m.read_u32(RINGS).unwrap(), 9);
        assert_eq!(m.read_u8(COUNT).unwrap(), 0);
    }
}
