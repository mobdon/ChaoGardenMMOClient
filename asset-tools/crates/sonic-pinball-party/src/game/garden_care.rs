//! A3VE Tiny Garden Chao frame. The switch at 45D4 contains internal code
//! addresses in the decoded payload; its targets share registers and epilogue.
//! Saved bytes retain their source offsets. This is local game state.
use super::frame::{FrameError, FrameMemory, NativeCall};
pub type Error = FrameError;
pub trait Calls {
    fn call(&mut self, m: &mut dyn FrameMemory, offset: u32, args: &[u32]) -> Result<u32, Error>;
}
const SAVE: u32 = 0x03004aa0;
const TIMERS: u32 = 0x03001518;
const FLAGS: u32 = 0x0300425f;
const DRAW: u32 = 0x0300152c;
const RNG: u32 = 0x030013a8;
type Mem<'a> = &'a mut dyn FrameMemory;

fn boundary(pc: u32, t: u32, why: &str) -> Error {
    Error::Dependency {
        call: NativeCall {
            entry_offset: super::garden_graphics::diagnostic_address(pc),
            argument0: Some(t),
        },
        reason: why.to_owned(),
    }
}
fn put_word(m: Mem<'_>, a: u32, v: u32) -> Result<(), Error> {
    m.write_bytes(a, &v.to_le_bytes())
}
fn step_byte(m: Mem<'_>, a: u32, delta: i8) -> Result<u8, Error> {
    let v = m.read_u8(a)?.wrapping_add_signed(delta);
    m.write_u8(a, v)?;
    Ok(v)
}
fn tick(m: Mem<'_>, a: u32) -> Result<bool, Error> {
    let v = m.read_u16(a)?.wrapping_sub(1);
    m.write_u16(a, v)?;
    Ok(v == 0)
}
fn bounded_need(m: Mem<'_>, offset: u32, increase: bool) -> Result<(), Error> {
    let value = m.read_u8(SAVE + offset)?;
    if increase && value < 200 {
        m.write_u8(SAVE + offset, value + 1)?;
    }
    if !increase && value > 0 {
        m.write_u8(SAVE + offset, value - 1)?;
    }
    Ok(())
}
fn care_timers(m: Mem<'_>) -> Result<(), Error> {
    for (offset, period, field, increase) in [
        (0, 180, 0x76, true),
        (2, 60, 0x75, true),
        (4, 180, 0x74, false),
    ] {
        if tick(m, TIMERS + offset)? {
            m.write_u16(TIMERS + offset, period)?;
            bounded_need(m, field, increase)?;
        }
    }
    let need = m.read_u8(SAVE + 0x74)?;
    if need > 189 && tick(m, TIMERS + 6)? {
        m.write_u16(TIMERS + 6, 360)?;
        bounded_need(m, 0x73, true)?;
    }
    let count = m.read_u8(0x0300159b)?;
    let delay = (4 + u16::from(need) / 20) * 60;
    let count_delay = 12u16.wrapping_sub(u16::from(count) / 2).wrapping_mul(60);
    if need > 109 && count <= 3 {
        m.write_u16(TIMERS + 8, delay)?;
        m.write_u16(TIMERS + 10, count_delay)?;
    } else {
        if need <= 109 && tick(m, TIMERS + 8)? {
            m.write_u16(TIMERS + 8, delay)?;
            bounded_need(m, 0x73, false)?;
        }
        if count > 3 && tick(m, TIMERS + 10)? {
            m.write_u16(TIMERS + 10, count_delay)?;
            bounded_need(m, 0x73, false)?;
        }
    }
    let wait = m.read_u16(TIMERS + 14)?;
    if wait != 0 {
        m.write_u16(TIMERS + 14, wait - 1)?;
    }
    if m.read_u8(SAVE + 0x73)? != 0 {
        put_word(m, SAVE + 0xd0, 216000)?;
    } else {
        let v = m.read_u32(SAVE + 0xd0)?;
        if v != 0 {
            put_word(m, SAVE + 0xd0, v - 1)?;
        }
    }
    Ok(())
}
fn normalize_once(dir: u8) -> u8 {
    let signed = dir as i8;
    if signed < 0 {
        dir.wrapping_add(12)
    } else if signed > 11 {
        dir - 12
    } else {
        dir
    }
}
fn movement(m: Mem<'_>, t: u32) -> Result<(), Error> {
    for (p, v) in [(8, 0x22), (12, 0x20)] {
        let next = m
            .read_u32(t + p)?
            .wrapping_add_signed(m.read_u16(t + v)? as i16 as i32);
        put_word(m, t + p, next)?;
    }
    for (position, velocity, lo, hi, mirror) in
        [(10, 0x22, 20, 156, 6u8), (14, 0x20, 32, 152, 12u8)]
    {
        let value = m.read_u16(t + position)? as i16;
        if value < lo || value > hi {
            let corrected = if value < lo {
                value.wrapping_add(1)
            } else {
                value.wrapping_sub(1)
            };
            m.write_u16(t + position, corrected as u16)?;
            let v = m.read_u16(t + velocity)?.wrapping_neg();
            m.write_u16(t + velocity, v)?;
            let dir = m.read_u8(t + 0x16)?;
            m.write_u8(t + 0x16, normalize_once(mirror.wrapping_sub(dir)))?;
        }
    }
    Ok(())
}
fn random(m: Mem<'_>) -> Result<u16, Error> {
    let next = m
        .read_u32(RNG)?
        .wrapping_mul(0x41c64e6d)
        .wrapping_add(12345);
    put_word(m, RNG, next)?;
    Ok(((next >> 16) & 0x7fff) as u16)
}
fn wave(m: &dyn FrameMemory, angle: u8, sine: bool) -> Result<i16, Error> {
    let index = if sine {
        angle.wrapping_add(0xc0)
    } else {
        angle
    };
    Ok(m.read_u16(0x0201f2e0 + 2 * u32::from(index))? as i16)
}
fn pick_heading(m: Mem<'_>, t: u32) -> Result<(), Error> {
    let mut dir = m.read_u8(t + 0x16)?;
    if m.read_u8(0x03001507)? != 0 {
        m.write_u8(0x03001507, 0)?;
    } else {
        dir = normalize_once(dir.wrapping_add((random(m)? % 5) as u8).wrapping_sub(2));
    }
    m.write_u8(t + 0x16, dir)?;
    let angle = m.read_u8(0x02020672u32.wrapping_add_signed(dir as i8 as i32))?;
    let vx = wave(m, angle, false)?.wrapping_mul(48);
    let vy = wave(m, angle, true)?.wrapping_mul(48);
    m.write_u16(t + 0x22, vx as u16)?;
    m.write_u16(t + 0x20, vy as u16)?;
    m.write_u16(t + 12, 0)?;
    m.write_u16(t + 8, 0)
}
fn walking(m: Mem<'_>, t: u32) -> Result<(), Error> {
    pick_heading(m, t)?;
    m.write_u8(t + 0x17, 22)
}
fn angle8(x: i16, y: i16) -> u8 {
    use super::math::{BiosMath, ReferenceBiosMath};
    (ReferenceBiosMath
        .atan2(x, y)
        .expect("numerical BIOS model always yields an angle")
        >> 8) as u8
}
fn velocity(m: Mem<'_>, t: u32, angle: u8, scale: i16) -> Result<(), Error> {
    for (v, sine) in [(0x22, false), (0x20, true)] {
        let val = wave(m, angle, sine)?.wrapping_mul(scale);
        m.write_u16(t + v, val as u16)?;
    }
    Ok(())
}
fn collide(m: Mem<'_>, t: u32) -> Result<(), Error> {
    for (p, v) in [(8, 0x22), (12, 0x20)] {
        let n = m
            .read_u32(t + p)?
            .wrapping_sub(m.read_u16(t + v)? as i16 as i32 as u32);
        put_word(m, t + p, n)?;
    }
    let x = m.read_u16(t + 10)? as i16;
    let y = m.read_u16(t + 14)? as i16;
    let angle = angle8(40i16.wrapping_sub(x), 144i16.wrapping_sub(y));
    m.write_u8(t + 0x19, angle)?;
    velocity(m, t, angle, 96)?;
    m.write_u8(t + 0x1a, 30)?;
    m.write_u16(0x03001502, 0)?;
    m.write_u16(0x03001504, 256)?;
    let dir = if x <= 71 {
        9
    } else if x <= 87 {
        10
    } else if x <= 103 && y <= 143 {
        11
    } else {
        0
    };
    m.write_u8(t + 0x16, dir)?;
    m.write_u8(0x03001507, 1)?;
    m.write_u8(t + 0x17, 55)
}

/// Return the original signed displacement and BIOS floor distance only if
/// the target falls inside the two heading rays and is less than 40 units away.
fn target_in_view(
    m: &dyn FrameMemory,
    t: u32,
    tx: i16,
    ty: i16,
) -> Result<Option<(i16, i16, u16)>, Error> {
    let x = m.read_u16(t + 10)? as i16;
    let y = m.read_u16(t + 14)? as i16;
    let heading = i16::from(m.read_u8(t + 0x16)?);
    let mut left = heading - 2;
    if left < 0 {
        left += 12;
    }
    let mut right = heading + 2;
    if right > 11 {
        right -= 12;
    }
    for (dir, first) in [(left, true), (right, false)] {
        let angle = (dir * 21) as u8;
        let px = x.wrapping_add(wave(m, angle, false)?);
        let py = y.wrapping_add(wave(m, angle, true)?);
        let cross = (i32::from(x) - i32::from(px))
            .wrapping_mul(i32::from(ty) - i32::from(y))
            .wrapping_add(
                (i32::from(y) - i32::from(py)).wrapping_mul(i32::from(x) - i32::from(tx)),
            );
        if first && cross > 0 || !first && cross < 0 {
            return Ok(None);
        }
    }
    let dx = tx.wrapping_sub(x);
    let dy = ty.wrapping_sub(y);
    let squared =
        (i32::from(dx) * i32::from(dx)).wrapping_add(i32::from(dy) * i32::from(dy)) as u32;
    let d = squared.isqrt() as u16;
    Ok((d <= 39).then_some((dx, dy, d)))
}
fn approach(m: Mem<'_>, t: u32, state: u8, (dx, dy, d): (i16, i16, u16)) -> Result<(), Error> {
    m.write_u8(t + 0x17, state)?;
    m.write_u8(t + 7, 64)?;
    m.write_u16(t + 0x14, ((u32::from(d) << 16) / 0x3000) as u16)?;
    let a = angle8(dx, dy);
    m.write_u8(t + 0x1a, a)?;
    m.write_u8(t + 0x19, 60)?;
    velocity(m, t, a, 48)
}
fn pose(m: Mem<'_>, t: u32, state: u8, timer: u8) -> Result<(), Error> {
    m.write_u8(t + 0x17, state)?;
    m.write_u8(t + 0x1a, timer)?;
    m.write_u8(t + 0x19, 0)
}
fn timed_state(m: Mem<'_>, t: u32, state: u8) -> Result<(), Error> {
    m.write_u8(t + 0x17, state)?;
    m.write_u16(t + 0x14, 120)
}
fn inspect_surroundings(m: Mem<'_>, t: u32) -> Result<(), Error> {
    let need = m.read_u8(SAVE + 0x74)?;
    if need <= 66 {
        for i in 0..8 {
            let contact = m.read_u8(0x03001940 + i)?;
            if contact != 0 {
                let tx = i16::from(m.read_u8(SAVE + 0x21 + 2 * i)?);
                let ty = i16::from(m.read_u8(SAVE + 0x22 + 2 * i)?);
                if let Some(target) = target_in_view(m, t, tx, ty)? {
                    approach(m, t, 36, target)?;
                    m.write_u8(t + 0x1b, i as u8)?;
                    return m.write_u8(0x03001508, contact);
                }
            }
        }
    }
    let x = i16::from(m.read_u8(SAVE + 0x35)?);
    if x != 0 {
        let y = m.read_u8(SAVE + 0x36)?;
        if y <= 143 {
            let tx = x + if (m.read_u16(t + 10)? as i16) < x {
                -8
            } else {
                8
            };
            if let Some(target) = target_in_view(m, t, tx, i16::from(y) + 4)? {
                return approach(m, t, 37, target);
            }
        }
    }
    let mut roll = i32::from(random(m)?);
    for (width, state) in [
        (0xccc, 30),
        (0x666, 27),
        (0x666, 26),
        (0x666, 25),
        (0x666, 28),
    ] {
        roll -= width;
        if roll < 0 {
            return m.write_u8(t + 0x17, state);
        }
    }
    let high = m.read_u16(SAVE + 0x92)?.swap_bytes() > 133;
    let weight = if high { 200 } else { 250 };
    let band = |n: i32| n * 32767 / 1000;
    roll = i32::from(random(m)?);
    roll -= band(weight);
    if roll < 0 {
        return match need {
            0 => timed_state(m, t, 43),
            1..=66 => pose(m, t, 45, 20),
            _ => walking(m, t),
        };
    }
    roll -= band(weight);
    if roll < 0 {
        return if m.read_u8(SAVE + 0x75)? > 149 {
            pose(m, t, 46, 15)
        } else {
            walking(m, t)
        };
    }
    roll -= band(weight);
    if roll < 0 {
        return match m.read_u8(SAVE + 0x73)? {
            190..=255 => pose(m, t, 48, 30),
            166..=189 => m.write_u8(t + 0x17, 18),
            0 => pose(m, t, 49, 30),
            1..=50 if m.read_u8(0x0300159b)? > 9 => pose(m, t, 47, 15),
            _ => walking(m, t),
        };
    }
    roll -= band(if high { 200 } else { 0 });
    if roll < 0 {
        return m.write_u8(t + 0x17, 51);
    }
    roll -= band(weight - 50);
    if roll < 0 {
        return match m.read_u8(SAVE + 0x76)? {
            133..=255 => timed_state(m, t, 44),
            100..=132 => timed_state(m, t, 50),
            _ => walking(m, t),
        };
    }
    roll = i32::from(random(m)?);
    let (state, id) = if roll < 0x1333 {
        (
            39,
            if m.read_u8(SAVE + 0xcd)? != 0 {
                101
            } else if random(m)? <= 0x3fff {
                99
            } else {
                97
            },
        )
    } else if roll < 0x1333 + 0xccc {
        (
            39,
            if m.read_u8(SAVE + 0xce)? != 0 {
                119
            } else {
                103
            },
        )
    } else if roll < 0x1333 + 0xccc + 0x1333 {
        (39, if random(m)? <= 0x3fff { 105 } else { 107 })
    } else if roll < 0x1333 + 0xccc + 0x1333 + 0x3fff {
        match need {
            0 => (41, 117),
            1..=16 => (41, 115),
            17..=33 => (41, 113),
            34..=50 => (41, 111),
            51..=66 => (39, 109),
            _ => return walking(m, t),
        }
    } else if m.read_u8(SAVE + 0xcf)? != 0 {
        (39, 129)
    } else {
        return walking(m, t);
    };
    m.write_u8(t + 0x19, id)?;
    m.write_u8(t + 0x17, state)
}
fn select_state(m: Mem<'_>, t: u32) -> Result<(), Error> {
    m.write_u8(t + 7, 0)?;
    if m.read_u32(SAVE + 0xd0)? == 0 {
        return m.write_u8(t + 0x17, 32);
    }
    let delay = (random(m)? % 3 + 3) * 60;
    m.write_u16(t + 0x14, delay)?;
    match m.read_u8(t + 0x17)? {
        23 => m.write_u8(t + 0x17, 25),
        22 if random(m)? >= 0x2666 => {
            if m.read_u8(SAVE + 0x75)? > 199 {
                m.write_u8(t + 0x1a, 0)?;
                m.write_u8(t + 0x19, 0)?;
                m.write_u8(t + 0x17, 23)
            } else {
                inspect_surroundings(m, t)
            }
        }
        _ => {
            pick_heading(m, t)?;
            m.write_u8(t + 0x17, 22)
        }
    }
}
fn writeback(m: Mem<'_>, t: u32) -> Result<(), Error> {
    let y = m.read_u16(t + 14)? as u8;
    let height = m.read_u8(t + 0x1f)?;
    m.write_u8(t + 0x18, y.wrapping_sub(height))?;
    m.write_u8(t + 0x1d, y)?;
    m.write_u8(SAVE + 0x1d, m.read_u16(t + 10)? as u8)?;
    m.write_u8(SAVE + 0x1e, y)?;
    m.write_bytes(0x03001940, &[0; 8])?;
    put_word(m, 0x03001514, 0)
}
fn map_cell(m: &dyn FrameMemory, t: u32) -> Result<u8, Error> {
    let x = i32::from(m.read_u16(t + 10)? as i16) >> 3;
    let y = i32::from(m.read_u16(t + 14)? as i16) >> 3;
    m.read_u8(0x020216a0u32.wrapping_add_signed(y * 22 + x))
}
/// State 13 consumes one signed seven-byte effect row. The saved progress
/// bytes and big-endian stat totals have different units and level caps.
fn apply_food(m: Mem<'_>, t: u32, c: &mut dyn Calls) -> Result<(), Error> {
    let item = m.read_u8(t + 0x1b)?;
    if item > 6 {
        return Err(boundary(0x4944, t, "unknown food row"));
    }
    let row = 0x0201269c + 7 * u32::from(item);
    for i in 0..2 {
        let delta = i32::from(m.read_u8(row + i)? as i8) * 20;
        let value = i32::from(m.read_u8(SAVE + 0x73 + i)?) + delta;
        m.write_u8(SAVE + 0x73 + i, value.clamp(0, 200) as u8)?;
    }
    for i in 0..5 {
        let delta = i32::from(m.read_u8(row + 2 + i)? as i8) * 10;
        let mut progress = (i32::from(m.read_u8(SAVE + 0x78 + i)?) + delta).max(0);
        if progress >= 100 {
            progress -= 100;
            let level = m.read_u8(SAVE + 0x88 + i)?;
            if level >= 99 {
                progress = 99;
            } else {
                m.write_u8(SAVE + 0x88 + i, level + 1)?;
                let total = m.read_u16(SAVE + 0x90 + i * 2)?.swap_bytes();
                let gain = 11
                    + random(m)? % 5
                    + 3 * u16::from(m.read_u8(SAVE + 0x80 + i)?)
                    + u16::from(m.read_u8(SAVE + 0x73)? > 180);
                let next = (u32::from(total) + u32::from(gain)).min(9999) as u16;
                m.write_u16(SAVE + 0x90 + i * 2, next.swap_bytes())?;
            }
        }
        m.write_u8(SAVE + 0x78 + i, progress as u8)?;
    }
    let child = c.call(m, 0xd04, &[0x020086b5, t, 0])?;
    m.write_u8(child.wrapping_add(0x1a), item)?;
    m.write_u8(child.wrapping_add(0x1b), m.read_u8(t + 4)?)?;
    m.write_u16(t + 0x14, 16)?;
    step_byte(m, t + 0x17, 1)?;
    Ok(())
}
/// States 36 and 37 approach a contact. Unlike ordinary walking they undo a
/// blocked step and select another state without reflecting the heading.
fn approach_frame(m: Mem<'_>, t: u32, food: bool) -> Result<(), Error> {
    let absent = if food {
        m.read_u8(0x03001940 + u32::from(m.read_u8(t + 0x1b)?))? == 0
    } else {
        m.read_u32(0x03001514)? == 0
    };
    if absent {
        m.write_u8(t + 7, (m.read_u8(t + 7)? & !64) | 32)?;
        m.write_u8(t + 0x17, 26)?;
        return m.write_u16(t + 0x14, 60);
    }
    let x = m.read_u32(t + 8)?;
    let y = m.read_u32(t + 12)?;
    let vx = i32::from(m.read_u16(t + 0x22)? as i16);
    let vy = i32::from(m.read_u16(t + 0x20)? as i16);
    put_word(m, t + 8, x.wrapping_add_signed(vx))?;
    put_word(m, t + 12, y.wrapping_add_signed(vy))?;
    if map_cell(m, t)? != 0 {
        put_word(m, t + 8, x)?;
        put_word(m, t + 12, y)?;
        return select_state(m, t);
    }
    if m.read_u8(t + 0x19)? != 0 && step_byte(m, t + 0x19, -1)? == 0 {
        m.write_u8(t + 7, m.read_u8(t + 7)? & !64)?;
    }
    if !tick(m, t + 0x14)? {
        return Ok(());
    }
    if food {
        m.write_u8(t + 0x17, 13)?;
        let angle = m.read_u8(t + 0x1a)?;
        m.write_u8(
            t + 7,
            if (angle.wrapping_sub(64) as i8) < 0 {
                0x90
            } else {
                0x10
            },
        )?;
        let object = 0x03005320 + 0x24 * u32::from(m.read_u8(0x03001508)?);
        m.write_u8(t + 0x1b, m.read_u8(object + 0x1a)?)?;
        m.write_u8(SAVE + 0x5a + u32::from(m.read_u8(object + 0x19)?), 255)?;
        step_byte(m, 0x0300159a, -1)?;
        put_word(m, object, 0x02000ed5)
    } else {
        m.write_u8(t + 0x17, 38)?;
        let delay = (3 + random(m)? % 3) * 60;
        m.write_u16(t + 0x14, delay)?;
        m.write_u8(t + 7, 0)?;
        let other = m.read_u32(0x03001514)?;
        if (m.read_u16(t + 10)? as i16) < (m.read_u16(other.wrapping_add(10))? as i16) {
            m.write_u8(t + 7, 128)?;
        }
        Ok(())
    }
}
fn advance(m: Mem<'_>, t: u32) -> Result<(), Error> {
    for (p, v) in [(8, 0x22), (12, 0x20)] {
        let p1 = m
            .read_u32(t + p)?
            .wrapping_add_signed(i32::from(m.read_u16(t + v)? as i16));
        put_word(m, t + p, p1)?;
    }
    Ok(())
}
fn negate(m: Mem<'_>, a: u32) -> Result<(), Error> {
    let n = m.read_u16(a)?.wrapping_neg();
    m.write_u16(a, n)
}
fn spawn_attached(m: Mem<'_>, t: u32, c: &mut dyn Calls, callback: u32) -> Result<u32, Error> {
    let child = c.call(m, 0xd04, &[callback, t, 0])?;
    m.write_u8(child + 0x1b, m.read_u8(t + 4)?)?;
    Ok(child)
}
fn splashes(m: Mem<'_>, t: u32, c: &mut dyn Calls, phase: u8) -> Result<(), Error> {
    for side in 0..2 {
        let child = c.call(m, 0xd04, &[0x02008a2d, t, 0])?;
        if side == 1 {
            m.write_u8(child + 7, 1)?;
        }
        m.write_u8(child + 0x1b, m.read_u8(t + 4)?)?;
        m.write_u8(child + 0x23, phase)?;
    }
    Ok(())
}
fn water_action(
    m: Mem<'_>,
    t: u32,
    c: &mut dyn Calls,
    entry: u32,
    paused: bool,
) -> Result<(), Error> {
    const HEIGHT: u32 = 0x03001502;
    const VERT: u32 = 0x03001504;
    const SPLASH: u32 = 0x03001506;
    m.write_u8(DRAW, 1)?;
    m.write_u8(t + 0x1e, 0)?;
    let frame = m.read_u32(0x03004258)?;
    match entry {
        0x5740 => {
            if frame & 3 == 0 {
                step_byte(m, t + 0x1f, 1)?;
            }
            if (m.read_u8(t + 0x1f)? as i8) > 15 {
                m.write_u8(t + 0x1a, 4)?;
                step_byte(m, t + 0x17, 1)?;
            }
        }
        0x5774 => {
            if paused {
                if map_cell(m, t)? != 0 {
                    m.write_u8(t + 0x16, 9)?;
                    velocity(m, t, 0xc0, 48)?;
                    m.write_u16(t + 12, 0)?;
                    m.write_u16(t + 8, 0)?;
                }
                m.write_u8(t + 0x17, 54)?;
            } else {
                movement(m, t)?;
                if tick(m, t + 0x14)? {
                    let last = step_byte(m, t + 0x1a, -1)? == 0;
                    if last && map_cell(m, t)? == 0 {
                        step_byte(m, t + 0x17, 1)?;
                    } else {
                        let delay = (3 + random(m)? % 3) * 60;
                        m.write_u16(t + 0x14, delay)?;
                        pick_heading(m, t)?;
                        if last {
                            m.write_u8(t + 0x1a, 1)?;
                        }
                    }
                }
            }
        }
        0x5908 => {
            advance(m, t)?;
            let v = m.read_u16(VERT)?;
            let h = m.read_u16(HEIGHT)?.wrapping_add(v);
            m.write_u16(HEIGHT, h)?;
            m.write_u16(VERT, v.wrapping_sub(16))?;
            let height = (h >> 8) as u8 as i8;
            m.write_u8(t + 0x1f, height as u8)?;
            if height < 0 {
                if m.read_u8(SPLASH)? == 0 {
                    m.write_u8(SPLASH, 1)?;
                    splashes(m, t, c, 0)?;
                }
                if height <= -24 {
                    m.write_u8(t + 0x1a, 0)?;
                    if m.read_u16(SAVE + 0x90)?.swap_bytes() < 100 {
                        m.write_u8(t + 7, m.read_u8(t + 7)? | 8)?;
                        m.write_u8(t + 0x17, 61)?;
                        spawn_attached(m, t, c, 0x02006799)?;
                    } else {
                        m.write_u8(t + 0x17, 62)?;
                        spawn_attached(m, t, c, 0x020068d9)?;
                        velocity(m, t, 24, 48)?;
                        if (m.read_u16(t + 10)? as i16) > 39 {
                            negate(m, t + 0x22)?;
                        }
                        if (m.read_u16(t + 14)? as i16) > 144 {
                            negate(m, t + 0x20)?;
                        }
                    }
                }
            }
        }
        0x5a3c => {
            advance(m, t)?;
            let x = m.read_u16(t + 10)? as i16;
            let y = m.read_u16(t + 14)? as i16;
            if x < 20 {
                negate(m, t + 0x22)?;
            }
            if y > 152 {
                negate(m, t + 0x20)?;
            }
            let v = m.read_u16(VERT)?;
            let h = m.read_u16(HEIGHT)?.wrapping_sub(v);
            let v = v.wrapping_add(16);
            m.write_u16(HEIGHT, h)?;
            m.write_u16(VERT, v)?;
            let height = (h >> 8) as u8 as i8;
            m.write_u8(t + 0x1f, height as u8)?;
            if height >= 0 {
                m.write_u8(SPLASH, 0)?;
            }
            if v as i16 >= 0 && height <= 0 {
                m.write_u8(t + 7, m.read_u8(t + 7)? & !4)?;
                m.write_u8(t + 0x1a, 0)?;
                m.write_u8(t + 0x1f, 0)?;
                if m.read_u8(t + 0x17)? == 57 {
                    m.write_u8(t + 0x17, 59)?;
                } else {
                    m.write_u8(t + 0x17, 60)?;
                    m.write_u8(
                        t + 0x16,
                        if x < 72 {
                            9
                        } else if x < 88 {
                            10
                        } else if x < 104 && y < 144 {
                            11
                        } else {
                            0
                        },
                    )?;
                }
                c.call(m, 0x1066c, &[15])?;
            }
        }
        0x5b3e => {
            if frame & 1 != 0 {
                return Ok(());
            }
            let height = m.read_u8(t + 0x1f)? as i8;
            if m.read_u8(t + 0x1a)? < 60 {
                if height < -8 {
                    step_byte(m, t + 0x1f, 1)?;
                } else {
                    step_byte(m, t + 0x1a, 1)?;
                }
            } else if height != (m.read_u16(HEIGHT)? >> 8) as u8 as i8 {
                step_byte(m, t + 0x1f, -1)?;
            } else {
                let angle = step_byte(m, t + 0x19, -128)?;
                velocity(m, t, angle, 112)?;
                m.write_u8(t + 7, m.read_u8(t + 7)? | 4)?;
                m.write_u8(t + 0x17, 57)?;
                splashes(m, t, c, 5)?;
            }
        }
        0x5bf8 => {
            if paused {
                velocity(m, t, 0xe0, 48)?;
                m.write_u8(t + 0x1a, 2)?;
            }
            if (m.read_u8(t + 0x1f)? as i8) < -8 && frame & 1 != 0 {
                step_byte(m, t + 0x1f, 1)?;
            }
            advance(m, t)?;
            let x = m.read_u16(t + 10)? as i16;
            let y = m.read_u16(t + 14)? as i16;
            if x < 20 {
                m.write_u16(t + 10, x.wrapping_add(1) as u16)?;
                negate(m, t + 0x22)?;
            }
            if y > 152 {
                m.write_u16(t + 14, y.wrapping_sub(1) as u16)?;
                negate(m, t + 0x20)?;
            }
            let vx = i32::from(m.read_u16(t + 0x22)? as i16);
            let vy = i32::from(m.read_u16(t + 0x20)? as i16);
            let px = i32::from((m.read_u32(t + 8)?.wrapping_add_signed(vx * 64) >> 16) as u8);
            let py = i32::from((m.read_u32(t + 12)?.wrapping_add_signed(vy * 64) >> 16) as u8);
            let cell = |x: i32, y: i32| 0x020216a0u32.wrapping_add_signed((y >> 3) * 22 + (x >> 3));
            let mut reflected = None;
            if m.read_u8(cell(px, py + if vy < 0 { -1 } else { 1 }))? == 0 {
                negate(m, t + 0x20)?;
                reflected = Some(0x20);
            }
            if m.read_u8(cell(px + if vx < 0 { -1 } else { 1 }, py))? == 0 {
                negate(m, t + 0x22)?;
                reflected = Some(0x22);
            }
            if let Some(v) = reflected {
                if step_byte(m, t + 0x1a, 1)? > 1 {
                    negate(m, t + v)?;
                    m.write_u8(t + 0x17, 63)?;
                }
            }
        }
        0x5d6e => {
            if m.read_u8(t + 0x1f)? != (m.read_u16(HEIGHT)? >> 8) as u8 {
                if frame & 1 != 0 {
                    step_byte(m, t + 0x1f, -1)?;
                }
            } else {
                m.write_u8(
                    t + 0x19,
                    if (m.read_u16(t + 0x22)? as i16) < 0 {
                        128
                    } else {
                        0
                    },
                )?;
                for v in [0x22, 0x20] {
                    let n = i32::from(m.read_u16(t + v)? as i16) * 5 / 2;
                    m.write_u16(t + v, n as u16)?;
                }
                m.write_u8(t + 0x17, 58)?;
                splashes(m, t, c, 5)?;
            }
        }
        _ => return Err(boundary(entry, t, "unknown water action")),
    }
    Ok(())
}
const TEXT_BUFFER: u32 = 0x0203f800;
/// 270C: build a bordered, localized text tilemap with the native color/font controls.
fn dialog_tiles(m: Mem<'_>, id: u8) -> Result<Option<(u8, u8)>, Error> {
    let mut index = 0u32;
    while index < 114 && m.read_u8(0x020296dc + index)? != id {
        index += 1;
    }
    if index == 114 {
        return Ok(None);
    }
    let lang = u32::from(m.read_u8(0x03004260)?);
    let invalid = |reason: &str| boundary(0x270c, id.into(), reason);
    if lang > 4 {
        return Err(invalid("dialog language outside native tables"));
    }
    let table = if lang < 2 {
        0x0202934c + index * 8 + lang * 4
    } else {
        0x0202d6dc + index * 12 + (lang - 2) * 4
    };
    const PAYLOAD_BASE: u32 = 0x02000000;
    const PAYLOAD_END: u32 = PAYLOAD_BASE + crate::garden::PAYLOAD_LENGTH as u32;
    let mut src = m.read_u32(table)?;
    if !(PAYLOAD_BASE..=PAYLOAD_END - 2).contains(&src) {
        return Err(invalid("dialog header outside native payload"));
    }
    let limit = (src + 4096).min(PAYLOAD_END);
    let mut next = |m: &dyn FrameMemory| {
        if src >= limit {
            return Err(invalid("unterminated dialog text"));
        }
        let b = m.read_u8(src)?;
        src += 1;
        Ok(b)
    };
    let w = u32::from(next(m)?);
    let h = u32::from(next(m)?);
    let stride = w + 2;
    let rows = h + 2;
    if w == 0 || stride > 255 || rows > 255 || stride * rows > 0x800 / 2 {
        return Err(invalid(
            "bordered dialog exceeds native tile buffer or dimensions",
        ));
    }
    // Publish the map only after all rows and their terminators have validated.
    let mut tiles = Vec::with_capacity((stride * rows) as usize);
    for y in 0..rows {
        for x in 0..stride {
            let tile = match (y, x) {
                (0, 0) => 0x160,
                (0, v) if v == stride - 1 => 0x163,
                (0, _) => 0x161,
                (v, 0) if v == rows - 1 => 0x167,
                (v, a) if v == rows - 1 && a == stride - 1 => 0x16a,
                (v, _) if v == rows - 1 => 0x168,
                (_, 0) => 0x164,
                (_, v) if v == stride - 1 => 0x166,
                _ => 0x165,
            };
            tiles.push(tile);
        }
    }
    let parent = m.read_u32(0x03001510)?;
    if !(0x03000000..=0x03007fdc).contains(&parent) {
        return Err(invalid("dialog Chao task outside IWRAM"));
    }
    let px = i32::from(m.read_u16(parent + 10)? as i16);
    let py = m.read_u16(parent + 14)? as i16;
    let marker = (w as i32 / 2 + (px - 88) / 8).max(0).min(w as i32 - 1) + 1;
    let marker = marker as u32 + if py > 95 { (h + 1) * stride } else { 0 };
    tiles[marker as usize] = if py > 95 { 0x169 } else { 0x162 };
    let mut palette = 0u16;
    let mut bank = 0u16;
    for y in 1..=h {
        let mut x = 0;
        loop {
            let b = next(m)?;
            match b {
                0x5f => {
                    let ctrl = next(m)?;
                    if ctrl & 128 != 0 {
                        break;
                    }
                    palette = u16::from(ctrl & 15) << 12;
                }
                0xff => bank ^= 256,
                _ => {
                    if x >= w {
                        return Err(invalid("dialog glyph exceeds row width"));
                    }
                    tiles[(y * stride + 1 + x) as usize] =
                        (0x2afu16 + u16::from(b) + bank).wrapping_add(palette);
                    x += 1;
                }
            }
        }
    }
    for (i, tile) in tiles.into_iter().enumerate() {
        m.write_u16(TEXT_BUFFER + 2 * i as u32, tile)?;
    }
    Ok(Some((stride as u8, rows as u8)))
}
fn queue_dialog(
    m: Mem<'_>,
    t: u32,
    c: &mut dyn Calls,
    id: u8,
    force_below: bool,
) -> Result<bool, Error> {
    let Some((w, h)) = dialog_tiles(m, id)? else {
        return Ok(false);
    };
    let y = i32::from(m.read_u16(t + 14)? as i16);
    let row = if !force_below && y > 95 {
        (y >> 3) - i32::from(h) - 3
    } else {
        (y >> 3) + 1
    };
    let dest = 0x0600e816u32.wrapping_add_signed(row * 64 - i32::from(w & 0xfe));
    c.call(
        m,
        0x17b0,
        &[TEXT_BUFFER, dest, u32::from(w), u32::from(h), 0xc000],
    )?;
    Ok(true)
}
fn clear_dialog(m: Mem<'_>, c: &mut dyn Calls) -> Result<(), Error> {
    c.call(m, 0x17b0, &[0, 0x0600e800, 255, 0, 0])?;
    Ok(())
}
/// Remember each of 94 texts once; index 29 is reserved, and 95 is never selected.
fn next_text(m: Mem<'_>) -> Result<Option<u8>, Error> {
    if m.read_u8(SAVE + 0xcc)? > 93 {
        for off in 0xc0..=0xcc {
            m.write_u8(SAVE + off, 0)?;
        }
    }
    m.write_u8(SAVE + 0xc3, m.read_u8(SAVE + 0xc3)? | 32)?;
    let mut idx = (random(m)? % 94) as u8;
    if idx > 28 {
        idx += 1;
    }
    let mut wraps = 0;
    loop {
        let at = SAVE + 0xc0 + u32::from(idx / 8);
        let bit = 1 << (idx & 7);
        let mask = m.read_u8(at)?;
        if mask & bit == 0 {
            m.write_u8(at, mask | bit)?;
            step_byte(m, SAVE + 0xcc, 1)?;
            return Ok(Some(idx + 57));
        }
        idx += 1;
        if idx == 95 {
            idx = 0;
            wraps += 1;
            if wraps > 2 {
                m.write_u8(SAVE + 0xcc, 94)?;
                return Ok(None);
            }
        }
    }
}
fn dialog_action(
    m: Mem<'_>,
    t: u32,
    c: &mut dyn Calls,
    entry: u32,
    paused: bool,
) -> Result<(), Error> {
    match entry {
        0x4ee4 | 0x55f0 => {
            if paused {
                m.write_u8(t + 0x17, 0)?;
                return Ok(());
            }
            let id = if entry == 0x4ee4 {
                let Some(id) = next_text(m)? else {
                    return select_state(m, t);
                };
                id
            } else {
                m.read_u8(t + 0x19)?.wrapping_add(57)
            };
            if !queue_dialog(m, t, c, id, false)? {
                if entry == 0x4ee4 {
                    return select_state(m, t);
                }
                return Err(boundary(0x5612, t, "invalid dialogue id for object"));
            }
            c.call(m, 0x1066c, &[31])?;
            m.write_u16(t + 0x14, 30)?;
            if entry == 0x55f0 {
                m.write_u8(t + 0x17, if m.read_u8(t + 0x17)? == 39 { 40 } else { 42 })?;
                return Ok(());
            }
            step_byte(m, t + 0x17, 1)?;
            return dialog_action(m, t, c, 0x508a, paused);
        }
        0x508a | 0x52e4 => {
            m.write_u8(DRAW, 1)?;
            m.write_u8(t + 0x1e, 0)?;
            if entry == 0x508a {
                m.write_u8(0x03001509, 1)?;
            }
            let timer = m.read_u16(t + 0x14)?.saturating_sub(1);
            m.write_u16(t + 0x14, timer)?;
            if m.read_u16(0x03005314)? & 1 != 0 && timer == 0 {
                clear_dialog(m, c)?;
                if entry == 0x508a {
                    m.write_u8(0x03001509, 0)?;
                } else {
                    m.write_u8(FLAGS, m.read_u8(FLAGS)? & !4)?;
                    put_word(m, SAVE + 0xd0, 216000)?;
                    m.write_u8(t + 0x16, 0)?;
                }
                select_state(m, t)?;
            }
        }
        0x50e8 => {
            m.write_u8(DRAW, 1)?;
            m.write_u8(t + 0x1e, 2)?;
            for (pos, target, positive, negative) in [(12, 87, 3, 9), (8, 88, 0, 6)] {
                let a = m.read_u16(t + pos + 2)? as i16;
                if a != target {
                    let dir = if a > target { negative } else { positive };
                    m.write_u8(t + 0x16, dir)?;
                    let p = m.read_u32(t + pos)?.wrapping_add_signed(if a > target {
                        -0x3000
                    } else {
                        0x3000
                    });
                    return put_word(m, t + pos, p);
                }
            }
            m.write_u8(t + 0x16, 3)?;
            queue_dialog(m, t, c, 0xbc, true)?;
            m.write_u16(t + 0x14, 60)?;
            for song in [2, 31] {
                c.call(m, 0x1066c, &[song])?;
            }
            m.write_u8(FLAGS, m.read_u8(FLAGS)? | 4)?;
            step_byte(m, t + 0x17, 1)?;
        }
        0x51c4 => {
            m.write_u8(DRAW, 1)?;
            m.write_u8(t + 0x1e, 0)?;
            if tick(m, t + 0x14)? {
                m.write_u8(t + 0x16, 6)?;
                m.write_u8(t + 0x19, 0)?;
                m.write_u8(t + 0x17, 34)?;
            }
        }
        0x51f4 => {
            m.write_u8(DRAW, 1)?;
            m.write_u8(t + 0x1e, 2)?;
            if m.read_u8(t + 0x19)? != 0 {
                m.write_u8(t + 0x16, 3)?;
                queue_dialog(m, t, c, 0xc0, true)?;
                clear_dialog(m, c)?;
                m.write_u16(t + 0x14, 30)?;
                m.write_u8(t + 0x17, 35)?;
                c.call(m, 0x1066c, &[1])?;
            } else {
                let nx = m.read_u32(t + 8)?.wrapping_sub(0x1400);
                put_word(m, t + 8, nx)?;
                if (m.read_u16(t + 10)? as i16) < -12 {
                    m.write_u8(0x0300152f, 255)?;
                    m.write_u8(SAVE + 0x58, 255)?;
                    put_word(m, t, 0x020041e1)?;
                    m.write_u8(FLAGS, m.read_u8(FLAGS)? & !4)?;
                    c.call(m, 0x1066c, &[1])?;
                    clear_dialog(m, c)?;
                    m.write_u8(0x030015a9, 0)?;
                    m.write_u8(0x030015a8, 1)?;
                }
            }
        }
        _ => return Err(boundary(entry, t, "unknown dialogue action")),
    }
    Ok(())
}
/// 41E0 allocates the five native component tasks and immediately runs the new
/// Chao action callback. Component callbacks update separately in the task list.
fn initialize(m: Mem<'_>, t: u32, c: &mut dyn Calls) -> Result<(), Error> {
    if m.read_u8(SAVE + 0x58)? == 255 {
        return m.write_u8(0x0300152e, 0);
    }
    m.write_u8(0x0300152e, 1)?;
    let a = c.call(m, 0xd04, &[0x02008a75, t, 0])?;
    m.write_u16(a + 10, 176)?;
    put_word(m, a + 0x10, 0x0201fbd0)?;
    m.write_u16(a + 0x14, 0)?;
    m.write_u8(a + 0x18, 12)?;
    m.write_u8(a + 0x1b, m.read_u8(t + 4)?)?;
    let b = c.call(m, 0xd04, &[0x02008ab9, t, 0])?;
    put_word(m, b + 0x10, 0x0201fb60)?;
    m.write_u16(b + 0x14, 364)?;
    m.write_u8(b + 0x16, 4)?;
    m.write_u8(b + 0x18, 28)?;
    m.write_u8(b + 0x1b, m.read_u8(t + 4)?)?;
    let left = c.call(m, 0xd04, &[0x02007241, t, 0])?;
    m.write_u16(left + 0x14, 364)?;
    m.write_u8(left + 0x18, 12)?;
    m.write_u8(left + 0x19, 1)?;
    let right = c.call(m, 0xd04, &[0x020073a9, t, 0])?;
    m.write_u16(right + 0x14, 364)?;
    m.write_u8(right + 0x18, 12)?;
    let body = c.call(m, 0xd04, &[0x02006a55, t, 0])?;
    m.write_u8(body + 0x18, 12)?;
    m.write_u8(body + 0x1b, m.read_u8(t + 4)?)?;
    m.write_u8(body + 0x1c, 0)?;
    for a in [left, right] {
        m.write_u8(a + 0x1b, m.read_u8(body + 4)?)?;
    }
    let palette = match m.read_u8(SAVE + 0x72)? {
        0 => {
            let color = m.read_u8(SAVE + 0x71)?;
            let color = if color > 18 { 1 } else { color };
            let slot = if color == 0 { 0 } else { (color + 2) % 3 };
            if slot == 0 {
                0
            } else {
                slot + 11
            }
        }
        v @ 1..=11 => v,
        _ => {
            for i in 0..16 {
                m.write_u16(
                    0x030015b0 + 2 * i,
                    m.read_u16(SAVE + 0xa0 + 2 * i)?.swap_bytes(),
                )?;
            }
            14
        }
    };
    m.write_u8(0x030015a9, palette)?;
    m.write_u8(0x030015a8, 1)?;
    if m.read_u8(SAVE + 0x1d)? == 0 {
        m.write_u8(SAVE + 0x1d, 88)?;
        m.write_u8(SAVE + 0x1e, 104)?;
    }
    let x = m.read_u8(SAVE + 0x1d)?;
    let mut y = m.read_u8(SAVE + 0x1e)?;
    let mut tile = u16::from(y >> 3) * 22 + u16::from(x >> 3);
    let start_tile = tile;
    while m.read_u8(0x020216a0 + u32::from(tile))? != 0 {
        tile = tile.wrapping_sub(22);
        y = y.wrapping_sub(8);
        m.write_u8(SAVE + 0x1e, y)?;
        if tile == start_tile {
            return Err(boundary(0x4346, t, "no reachable open spawn tile"));
        }
    }
    m.write_u16(t + 10, u16::from(x))?;
    m.write_u8(t + 0x1d, y)?;
    m.write_u16(t + 14, u16::from(y))?;
    let dir = (random(m)? % 12) as u8;
    m.write_u8(t + 0x16, dir)?;
    m.write_u8(t + 0x17, m.read_u8(0x0300152d)?)?;
    m.write_u8(t + 0x1f, 0)?;
    m.write_u16(left + 10, m.read_u16(t + 10)?.wrapping_add(1))?;
    m.write_u16(left + 0x22, m.read_u16(t + 14)?.wrapping_sub(23))?;
    let wait1 = (4 + u16::from(m.read_u8(SAVE + 0x74)?) / 20) * 60;
    let wait2 = 12u16
        .wrapping_sub(u16::from(m.read_u8(0x0300159b)?) / 2)
        .wrapping_mul(60);
    for (i, v) in [180, 60, 180, 360, wait1, wait2].into_iter().enumerate() {
        m.write_u16(TIMERS + 2 * i as u32, v)?;
    }
    m.write_u16(TIMERS + 14, 0)?;
    m.write_u8(0x03001507, 0)?;
    m.write_u8(0x03001506, 0)?;
    put_word(m, t, 0x02004419)?;
    care_frame(m, t, c)
}
fn care_frame(m: Mem<'_>, t: u32, c: &mut dyn Calls) -> Result<(), Error> {
    let flags = m.read_u8(FLAGS)?;
    if flags & 0x92 != 0 {
        return Ok(());
    }
    let paused = flags & 0x20 != 0;
    if !paused {
        care_timers(m)?;
    }
    m.write_u8(t + 0x1e, 3)?;
    let state = m.read_u8(t + 0x17)?;
    let mut entry = if state <= 63 {
        let address = m.read_u32(0x020045d4 + 4 * u32::from(state))?;
        address
            .checked_sub(0x02000000)
            .ok_or_else(|| boundary(0x45cc, t, "invalid Chao state target"))?
    } else {
        0x46d4
    };
    // Some cases continue into the next case in the same frame.
    loop {
        match entry {
            0x5740 | 0x5774 | 0x5908 | 0x5a3c | 0x5b3e | 0x5bf8 | 0x5d6e => {
                water_action(m, t, c, entry, paused)?
            }
            0x4ee4 | 0x508a | 0x50e8 | 0x51c4 | 0x51f4 | 0x52e4 | 0x55f0 => {
                dialog_action(m, t, c, entry, paused)?
            }
            0x5540 => {
                if paused {
                    m.write_u8(t + 0x17, 0)?;
                } else if m.read_u32(0x03001514)? == 0 {
                    m.write_u8(t + 0x17, 47)?;
                    m.write_u8(t + 0x1a, 15)?;
                    m.write_u8(t + 0x19, 0)?;
                } else if tick(m, t + 0x14)? {
                    let choice = random(m)? / 3276;
                    match choice {
                        0 => {
                            m.write_u8(t + 0x1a, 0)?;
                            m.write_u8(t + 0x19, 0)?;
                            m.write_u8(t + 0x17, 23)?;
                            m.write_u8(SAVE + 0x75, m.read_u8(SAVE + 0x75)?.max(90))?;
                        }
                        1..=4 => {
                            m.write_u8(t + 0x19, 119 + (choice as u8) * 2)?;
                            m.write_u8(t + 0x17, 39)?;
                        }
                        _ => select_state(m, t)?,
                    }
                }
            }
            0x4d94 => {
                m.write_u8(t + 0x1e, 2)?;
                if paused {
                    m.write_u16(t + 0x14, 1)?;
                    m.write_u8(t + 0x17, 1)?;
                } else {
                    if m.read_u8(t + 0x19)? != 0 {
                        if step_byte(m, t + 0x19, -1)? == 0 {
                            if m.read_u8(SAVE + 0x75)? > 39 {
                                let v = m.read_u8(SAVE + 0x73)?;
                                m.write_u8(SAVE + 0x73, v - v / 5)?;
                                m.write_u16(t + 0x14, 90)?;
                                m.write_u8(t + 0x17, 24)?;
                                m.write_u8(t + 7, 12)?;
                            } else {
                                select_state(m, t)?;
                            }
                        }
                    } else if m.read_u32(0x03004258)? & 3 == 0
                        && step_byte(m, SAVE + 0x75, -1)? == 0
                    {
                        select_state(m, t)?;
                    }
                    if step_byte(m, t + 0x1a, 1)? > 29 {
                        m.write_u8(t + 0x1a, 0)?;
                        spawn_attached(m, t, c, 0x020069f9)?;
                    }
                }
            }
            0x4b90 => {
                let v = m.read_u8(SAVE + 0x62)?;
                let v = if v > 3 { 0 } else { v };
                m.write_u8(SAVE + 0x62, if v == 3 { v } else { v + 1 })?;
                m.write_u16(t + 0x14, if v == 3 { 144 } else { 80 })?;
                m.write_u8(t + 0x19, 0)?;
                m.write_u8(t + 0x1a, 16)?;
                step_byte(m, t + 0x17, 1)?;
                entry = 0x4bc6;
                continue;
            }
            0x4bc6 => {
                m.write_u8(DRAW, 1)?;
                m.write_u8(t + 0x1e, 0)?;
                if step_byte(m, t + 0x1a, -1)? == 0 {
                    m.write_u8(t + 0x1a, 20)?;
                    if step_byte(m, t + 0x19, 1)? == 2 {
                        let song = if m.read_u16(t + 0x14)? > 80 { 26 } else { 36 };
                        c.call(m, 0x1066c, &[song])?;
                    }
                }
                let obj = 0x03005320 + 0x24 * u32::from(m.read_u8(t + 0x1b)?);
                let phase = m.read_u8(t + 0x19)?;
                m.write_u16(
                    obj + 14,
                    m.read_u16(t + 14)?
                        .wrapping_sub(if phase == 0 { 1 } else { 3 }),
                )?;
                let dx = if phase & 1 == 0 { 15 } else { 14 };
                m.write_u8(t + 0x23, dx)?;
                let right = m.read_u8(t + 7)? & 128 != 0;
                let x = m.read_u16(t + 10)?;
                m.write_u16(
                    obj + 10,
                    if right {
                        x.wrapping_add(u16::from(dx))
                    } else {
                        x.wrapping_sub(u16::from(dx))
                    },
                )?;
                m.write_u8(obj + 0x17, if right { 16 } else { 0 })?;
                if tick(m, t + 0x14)? {
                    put_word(m, obj, 0x020083e5)?;
                    select_state(m, t)?;
                }
            }
            0x493c => {
                apply_food(m, t, c)?;
                entry = 0x4a68;
                continue;
            }
            0x4a68 => {
                m.write_u8(DRAW, 1)?;
                m.write_u8(t + 0x1e, 0)?;
                if tick(m, t + 0x14)? {
                    m.write_u16(t + 0x14, 240)?;
                    step_byte(m, t + 0x17, 1)?;
                }
            }
            0x4ac0 => {
                m.write_u16(t + 0x14, 63)?;
                step_byte(m, t + 0x17, 1)?;
                entry = 0x4aca;
                continue;
            }
            0x4aca => {
                if paused {
                    m.write_u8(t + 0x17, 0)?;
                } else {
                    m.write_u8(t + 0x1e, 0)?;
                    if tick(m, t + 0x14)? {
                        m.write_u8(SAVE + 0x76, 0)?;
                        if m.read_u16(TIMERS + 14)? != 0 {
                            select_state(m, t)?;
                        } else {
                            let val = step_byte(m, SAVE + 0x73, 20)?.min(200);
                            m.write_u8(SAVE + 0x73, val)?;
                            m.write_u8(t + 0x17, 18)?;
                        }
                        m.write_u16(TIMERS + 14, 1800)?;
                    }
                }
            }
            0x5350 | 0x545c => {
                if paused {
                    m.write_u8(t + 0x17, 0)?;
                } else {
                    approach_frame(m, t, entry == 0x5350)?;
                }
            }
            0x46d4 => {
                m.write_u8(t + 7, 0)?;
                m.write_u16(t + 0x14, 90)?;
                m.write_u8(t + 0x17, 2)?;
                entry = 0x46e0;
                continue;
            }
            0x46e0 => {
                if !paused {
                    if tick(m, t + 0x14)? {
                        select_state(m, t)?;
                    }
                } else if m.read_u8(SAVE + 0x58)? == 255 {
                    let x = m.read_u16(t + 10)? as i16;
                    let y = m.read_u16(t + 14)? as i16;
                    m.write_u16(t + 0x22, ((i32::from(x) - 88) * 4) as u16)?;
                    if y < 104 {
                        m.write_u16(t + 12, y as u16)?;
                        m.write_u16(t + 0x20, 104u16.wrapping_sub(y as u16))?;
                        m.write_u8(t + 0x1a, 0)?;
                        m.write_u8(t + 0x17, 3)?;
                    } else {
                        m.write_u16(t + 0x20, (y as u16).wrapping_sub(46))?;
                        m.write_u8(t + 0x1a, 64)?;
                        m.write_u8(t + 0x17, 4)?;
                    }
                    c.call(m, 0x1066c, &[0x20])?;
                    m.write_u8(DRAW, 1)?;
                }
            }
            0x4756 | 0x47ac => {
                let first = entry == 0x4756;
                let angle = step_byte(m, t + 0x1a, if first { 2 } else { 1 })?;
                if angle > 128 {
                    m.write_u8(0x0300152f, 255)?;
                    return put_word(m, t, 0x020041e1);
                }
                let vx = i32::from(m.read_u16(t + 0x22)? as i16) << 8;
                let x = m.read_u32(t + 8)?.wrapping_sub(vx as u32);
                put_word(m, t + 8, x)?;
                let sin = i32::from(wave(m, angle, true)?);
                let scale = i32::from(m.read_u16(t + 0x20)? as i16);
                if first && angle <= 64 {
                    let y = m
                        .read_u16(t + 12)?
                        .wrapping_add_signed((sin * scale / 256) as i16);
                    m.write_u16(t + 14, y)?;
                } else {
                    let amplitude = if first { 58 } else { scale };
                    m.write_u8(t + 0x1f, (amplitude - sin * amplitude / 256) as u8)?;
                }
                m.write_u8(DRAW, 1)?;
            }
            0x4808 => {
                c.call(m, 0x1066c, &[0x21])?;
                m.write_u8(t + 7, 1)?;
                m.write_u16(t + 10, 88)?;
                m.write_u8(t + 0x1d, 104)?;
                m.write_u16(t + 14, 104)?;
                m.write_u8(t + 0x1a, 70)?;
                m.write_u8(t + 0x1f, 58)?;
                step_byte(m, t + 0x17, 1)?;
                entry = 0x482a;
                continue;
            }
            0x482a => {
                m.write_u8(DRAW, 1)?;
                if step_byte(m, t + 0x1a, -1)? == 0 {
                    m.write_u8(t + 7, 0)?;
                    step_byte(m, t + 0x17, 1)?;
                    entry = 0x484a;
                    continue;
                }
            }
            0x484a => {
                let v = wave(m, m.read_u8(t + 0x1a)?, false)?;
                m.write_u8(t + 0x1f, ((i32::from(v) * 58) >> 8) as u8)?;
                if step_byte(m, t + 0x1a, 2)? > 63 {
                    m.write_u8(t + 7, 64)?;
                    m.write_u8(t + 0x1a, 1)?;
                    step_byte(m, t + 0x17, 1)?;
                    entry = 0x487c;
                    continue;
                }
            }
            0x487c => {
                let v = wave(m, m.read_u8(t + 0x1a)?, true)?;
                m.write_u8(t + 0x1f, (v >> 6) as u8)?;
                if (step_byte(m, t + 0x1a, 8)? as i8) < 0 {
                    m.write_u8(t + 0x1f, 0)?;
                    m.write_u8(0x0300152f, 0)?;
                    m.write_u8(t + 0x17, 0)?;
                }
            }
            0x48ac => {
                m.write_u8(t + 7, 0)?;
                m.write_u16(t + 0x14, 90)?;
                step_byte(m, t + 0x17, 1)?;
                entry = 0x48ba;
                continue;
            }
            0x48ba | 0x4a9c => {
                m.write_u8(DRAW, 1)?;
                m.write_u8(t + 0x1e, 0)?;
                if tick(m, t + 0x14)? {
                    if entry == 0x48ba {
                        m.write_u8(t + 0x17, 0)?;
                    } else {
                        select_state(m, t)?;
                    }
                }
            }
            0x4b44 | 0x4e90 => {
                m.write_u8(t + 0x19, 0)?;
                m.write_u8(t + 0x1a, if entry == 0x4b44 { 20 } else { 60 })?;
                step_byte(m, t + 0x17, 1)?;
                entry = if entry == 0x4b44 { 0x4b52 } else { 0x4e9e };
                continue;
            }
            0x48e0 | 0x4910 | 0x4e40 | 0x4e68 | 0x5676 | 0x4b52 | 0x5698 | 0x56d0 | 0x5708
            | 0x5b08 | 0x4e9e | 0x58dc | 0x4c94 => {
                if paused {
                    m.write_u8(t + 0x17, 0)?;
                } else {
                    match entry {
                        0x4b52 | 0x5698 | 0x56d0 | 0x5708 => {
                            let (period, limit) = match entry {
                                0x56d0 => (15, 7),
                                0x5708 => (30, 3),
                                _ => (20, 5),
                            };
                            if step_byte(m, t + 0x1a, -1)? == 0 {
                                m.write_u8(t + 0x1a, period)?;
                                if step_byte(m, t + 0x19, 1)? > limit {
                                    select_state(m, t)?;
                                }
                            }
                        }
                        0x5b08 => {
                            m.write_u8(t + 0x1e, 0)?;
                            if step_byte(m, t + 0x1a, 1)? > 59 {
                                select_state(m, t)?;
                            }
                        }
                        0x58dc => {
                            if step_byte(m, t + 0x1a, -1)? == 0 {
                                m.write_u8(t + 0x17, 56)?;
                                c.call(m, 0x1066c, &[18])?;
                            }
                        }
                        _ => {
                            if entry == 0x48e0 || entry == 0x4910 {
                                m.write_u8(t + 0x1e, 2)?;
                            }
                            if entry == 0x4c94 {
                                movement(m, t)?;
                                if map_cell(m, t)? != 0 {
                                    collide(m, t)?;
                                    return writeback(m, t);
                                }
                            }
                            if entry == 0x4e9e
                                && m.read_u8(t + 0x1a)? != 0
                                && step_byte(m, t + 0x1a, -1)? == 0
                            {
                                m.write_u8(t + 0x19, 1)?;
                                m.write_u8(t + 7, m.read_u8(t + 7)? | 32)?;
                            }
                            if tick(m, t + 0x14)? {
                                if entry == 0x48e0 {
                                    m.write_u16(t + 0x14, 84)?;
                                    step_byte(m, t + 0x17, 1)?;
                                } else {
                                    select_state(m, t)?;
                                }
                            }
                        }
                    }
                }
            }
            0x5868 => {
                m.write_u8(DRAW, 1)?;
                m.write_u8(t + 0x1e, 0)?;
                if m.read_u32(0x03004258)? & 3 == 0 {
                    step_byte(m, t + 0x1f, -1)?;
                }
                if m.read_u8(t + 0x1f)? == 0 {
                    m.write_u16(t + 0x14, 15)?;
                    m.write_u8(t + 0x17, 26)?;
                    c.call(m, 0x1066c, &[15])?;
                }
            }
            0x58a0 => {
                m.write_u8(DRAW, 1)?;
                m.write_u8(t + 0x1e, 0)?;
                movement(m, t)?;
                if map_cell(m, t)? == 0 {
                    m.write_u8(t + 0x17, 53)?;
                }
            }
            _ => return Err(boundary(entry, t, "unported internal Chao action")),
        }
        break;
    }
    writeback(m, t)
}

/// All offsets are even and relative to the decoded EWRAM payload.
pub fn dispatch(m: Mem<'_>, offset: u32, args: &[u32], c: &mut dyn Calls) -> Result<u32, Error> {
    let [t] = *args else {
        return Err(boundary(offset, 0, "expected one Chao task pointer"));
    };
    if !(0x03000000..=0x03007fdc).contains(&t) {
        return Err(boundary(offset, t, "Chao task outside IWRAM"));
    }
    match offset {
        0x41e0 => initialize(m, t, c)?,
        0x4418 => care_frame(m, t, c)?,
        0x5e58 => movement(m, t)?,
        0x5efc => select_state(m, t)?,
        0x6570 => pick_heading(m, t)?,
        _ => return Err(boundary(offset, t, "unported Chao entry")),
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::super::memory::{MemoryImage, Region};
    use super::*;
    use serde_json::{json, Value};
    use std::io::Read;
    const RAM: u32 = 0x03000000;
    const TASK: u32 = RAM + 0x5400;
    #[test]
    fn dialog_rejects_invalid_records_without_publishing_partial_tiles() {
        let base = 0x02000000;
        let end = base + crate::garden::PAYLOAD_LENGTH as u32;
        let text = base + 0x1000;
        let id = 42;
        let mut m = MemoryImage::fixture(vec![
            Region {
                base,
                bytes: vec![0; 0x40000],
                writable: true,
            },
            Region {
                base: RAM,
                bytes: vec![0; 0x8000],
                writable: true,
            },
        ]);
        m.write_u8(0x020296dc, id).unwrap();
        put_word(&mut m, 0x03001510, TASK).unwrap();
        let untouched = [0xa5; 0x800];
        m.write_bytes(TEXT_BUFFER, &untouched).unwrap();
        let mut controls = vec![0xff; 4096];
        controls[..2].copy_from_slice(&[1, 1]);
        for (lang, ptr, data, expected) in [
            (5, text, &[][..], "language"),
            (0, u32::MAX, &[][..], "header"),
            (0, end - 1, &[][..], "header"),
            (0, text, &[31, 30][..], "dimensions"),
            (0, text, &[254, 1][..], "dimensions"),
            (0, text, &[0, 1][..], "dimensions"),
            (0, text, &[1, 1, 1, 2, 0x5f, 0x80][..], "glyph"),
            (0, end - 3, &[1, 1, 0x5f][..], "unterminated"),
            (0, text, controls.as_slice(), "unterminated"),
        ] {
            m.write_u8(0x03004260, lang).unwrap();
            put_word(&mut m, 0x0202934c, ptr).unwrap();
            if !data.is_empty() {
                m.write_bytes(ptr, data).unwrap();
            }
            assert!(matches!(dialog_tiles(&mut m, id),
                Err(Error::Dependency { call, reason })
                    if call.entry_offset == 0x0200270c && reason.contains(expected)));
            assert_eq!(m.bytes(TEXT_BUFFER, 0x800).unwrap(), untouched);
        }
        // A bordered 32×32 map exactly fills the 2 KiB scratch area.
        let mut exact = vec![30, 30];
        for _ in 0..30 {
            exact.extend([0x5f, 0x80]);
        }
        m.write_bytes(text, &exact).unwrap();
        assert_eq!(dialog_tiles(&mut m, id).unwrap(), Some((32, 32)));
        assert_eq!(m.read_u16(TEXT_BUFFER + 0x7fe).unwrap(), 0x16a);
    }
    fn fixture() -> Value {
        serde_json::from_str(include_str!("../../tests/garden-care-vectors.json")).unwrap()
    }
    #[derive(Default)]
    struct SongLog(Vec<Value>);
    impl super::super::garden_services::Calls for SongLog {
        fn invoke(
            &mut self,
            m: &mut dyn FrameMemory,
            off: u32,
            args: &[u32],
        ) -> Result<u32, Error> {
            self.call(m, off, args)
        }
    }
    impl Calls for SongLog {
        fn call(
            &mut self,
            m: &mut dyn FrameMemory,
            offset: u32,
            args: &[u32],
        ) -> Result<u32, Error> {
            assert!(matches!(offset, 0x1066c | 0xd04 | 0x17b0));
            self.0.push(json!([offset, args]));
            if offset == 0x17b0 {
                return super::super::garden_services::dispatch(m, self, offset, args);
            }
            if offset == 0xd04 {
                use super::super::garden_payload::{BoardCalls, NativeBoardCalls};
                return Ok(NativeBoardCalls.invoke(0x02000d04, args, m).unwrap());
            }
            Ok(0)
        }
    }
    fn n(v: &Value) -> u32 {
        v.as_u64().unwrap().try_into().unwrap()
    }
    fn run(payload: Option<&[u8]>) {
        let f = fixture();
        for case in f["cases"].as_array().unwrap() {
            let offset = n(&case["entry"]);
            let name = case["name"].as_str().unwrap();
            if payload.is_none() && offset != 0x5e58 && !name.starts_with("mask-") {
                continue;
            }
            let mut regions = vec![Region {
                base: RAM,
                writable: true,
                bytes: vec![0; 0x7000],
            }];
            if let Some(bytes) = payload {
                regions.push(Region {
                    base: 0x02000000,
                    writable: false,
                    bytes: bytes.to_vec(),
                });
            }
            regions.push(Region {
                base: TEXT_BUFFER,
                writable: true,
                bytes: vec![0; 0x800],
            });
            let mut m = MemoryImage::fixture(regions);
            for w in f["defaults"]
                .as_array()
                .unwrap()
                .iter()
                .chain(case["patches"].as_array().unwrap())
            {
                m.write_bytes(n(&w[0]), &n(&w[2]).to_le_bytes()[..n(&w[1]) as usize])
                    .unwrap();
            }
            let mut c = SongLog::default();
            for (i, tick) in case["trace"].as_array().unwrap().iter().enumerate() {
                c.0.clear();
                let stop = match dispatch(&mut m, offset, &[TASK], &mut c) {
                    Ok(0) => Value::Null,
                    Err(Error::Dependency { call, .. }) => json!(call.entry_offset - 0x02000000),
                    other => panic!("{name}/{i}: {other:?}"),
                };
                assert_eq!(stop, tick["stop"], "{name}/{i}");
                assert_eq!(c.0, *tick["calls"].as_array().unwrap(), "{name}/{i}");
                assert_eq!(
                    crate::sha256(m.bytes(TEXT_BUFFER, 0x800).unwrap()),
                    tick["tiles"],
                    "{name}/{i} dialog tilemap"
                );
                assert_eq!(
                    crate::sha256(m.bytes(RAM, 0x7000).unwrap()),
                    tick["sha256"],
                    "{name}/{i}"
                );
            }
        }
    }
    #[test]
    fn movement_and_masked_frames_match_original_without_assets() {
        run(None);
    }
    #[test]
    #[ignore = "requires local SPP_ROM for original state table, collision map, and trigonometry"]
    fn care_frames_match_native_state_matrix() {
        let mut rom = Vec::new();
        std::fs::File::open(std::env::var_os("SPP_ROM").expect("set SPP_ROM"))
            .unwrap()
            .take(0x800001)
            .read_to_end(&mut rom)
            .unwrap();
        crate::catalog::validate_rom(&rom).unwrap();
        let bytes = crate::binary::decode(&rom[0xd0970..], crate::garden::PAYLOAD_LENGTH)
            .unwrap()
            .bytes;
        crate::garden::verify_payload_layout(&bytes).unwrap();
        run(Some(&bytes));
    }
}
