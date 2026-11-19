//! Native A3VE collision sweep. Point classifiers are invoked in native order.
use super::ball::MainBall;

/// Classifiers at 0x2c238 and 0x2c3ac share the native halfword result slot.
/// A true return stops the sweep. Implementations may update ball fields.
pub trait CollisionField: Clone {
    type Error;
    fn hit_flipper(
        &mut self,
        _ball: &mut MainBall,
        _point: &mut [i16; 2],
        _angle: u16,
        _tables: &ResponseTables,
        _state: &mut ResponseState,
        _bios: &mut dyn super::math::BiosMath,
    ) -> Result<(), FlipperError> {
        Err(FlipperError::MissingRecords)
    }
    fn classify(
        &mut self,
        stage: usize,
        ball: &mut MainBall,
        point: [i16; 2],
        result: &mut u16,
    ) -> Result<bool, Self::Error>;
}

fn pair(ball: &MainBall, at: usize) -> [i16; 2] {
    let bytes = ball.as_bytes();
    [at, at + 2].map(|p| i16::from_le_bytes([bytes[p], bytes[p + 1]]))
}

// 0x2c140. A magnitude of -32768 remains negative after Thumb halfword negation.
fn sweep<F: CollisionField>(
    field: &mut F,
    ball: &mut MainBall,
    point: &mut [i16; 2],
    delta: [i16; 2],
    mut result: u16,
) -> Result<u16, F::Error> {
    ball.as_bytes_mut()[4..6].fill(0);
    let mut remaining = delta.map(i16::wrapping_abs);
    let mut preferred = 1;

    loop {
        if field.classify(0, ball, *point, &mut result)?
            || field.classify(1, ball, *point, &mut result)?
        {
            return Ok(result);
        }
        let axis = if remaining[preferred] > 0 {
            preferred
        } else {
            1 - preferred
        };
        if remaining[axis] <= 0 {
            return Ok(result);
        }
        point[axis] = point[axis].wrapping_add(delta[axis].signum());
        remaining[axis] -= 1;
        preferred = 1 - preferred;
    }
}

/// 0x2bfd4. Returns the native point and classifier result. Stage changes are
/// committed together so an invalid table read cannot partially update the ball.
pub fn probe<F: CollisionField>(
    field: &mut F,
    ball: &mut MainBall,
    scratch: u16,
) -> Result<([i16; 2], u16), F::Error> {
    let mut next = ball.clone();
    let mut staged = field.clone();
    let previous = pair(&next, 0xa4);
    let current = pair(&next, 0xa0);
    let delta = [0, 1].map(|i| current[i].wrapping_sub(previous[i]));
    let offset = [0x21, 0x22].map(|p| next.as_bytes()[p] as i8 as i16);
    let mut point = previous;
    let result;
    let adjustment;
    if offset == [0, 0] {
        result = sweep(&mut staged, &mut next, &mut point, delta, scratch)?;
        adjustment = [0, 0];
    } else {
        let first = sweep(&mut staged, &mut next, &mut point, offset, scratch)?;
        if next.as_bytes()[4] != 0 {
            result = first;
            let mut difference = [0, 1].map(|i| point[i].wrapping_sub(previous[i]) as u8);
            if difference[0] == 0 {
                difference[0] = 1;
            }
            adjustment = difference;
        } else {
            point = [0, 1].map(|i| previous[i].wrapping_add(offset[i]));
            result = sweep(&mut staged, &mut next, &mut point, delta, first)?;
            adjustment = offset.map(|v| v as u8);
        }
    }
    next.as_bytes_mut()[0x23..0x25].copy_from_slice(&adjustment);
    *ball = next;
    *field = staged;
    Ok((point, result))
}

/// Pre-resolved pointers for one layer (state +0x118, +0x158, +0x198).
pub struct TileLayer<'a> {
    pub tiles: &'a [i16],
    pub angles: &'a [u8],
    pub materials: &'a [u8],
}

#[derive(Debug, PartialEq, Eq)]
pub enum FieldError {
    DynamicTable(u16),
    MapLayer(u16),
    RomAddress(i64),
    Flipper(FlipperError),
    Layer(i8),
    Tile(i32),
    Pixel(i32),
    FlipperSample(usize),
}

/// Table IDs 0 and 4 use a fixed 64-tile stride in 0x2d70c.
/// Flipper origins come from native state +0x110/+0x112 and +0x10a.
#[derive(Clone)]
pub struct StaticField<'a> {
    pub table_id: u16,
    pub layers: &'a [TileLayer<'a>],
    pub flipper_origins_x: [i16; 2],
    pub flipper_origin_y: i16,
}

impl CollisionField for StaticField<'_> {
    type Error = FieldError;
    fn classify(
        &mut self,
        stage: usize,
        ball: &mut MainBall,
        point: [i16; 2],
        result: &mut u16,
    ) -> Result<bool, FieldError> {
        if !matches!(self.table_id, 0 | 4) {
            return Err(FieldError::DynamicTable(self.table_id));
        }
        if stage == 1 {
            if (point[1].wrapping_sub(self.flipper_origin_y.wrapping_mul(2)) as u16) < 96 {
                for (side, x) in self.flipper_origins_x.iter().enumerate() {
                    if (point[0].wrapping_sub(x.wrapping_mul(2)) as u16) < 96 {
                        return Err(FieldError::FlipperSample(side));
                    }
                }
            }
            return Ok(false);
        }
        let selector = ball.as_bytes()[6] as i8;
        let layer = self
            .layers
            .get(selector as usize)
            .ok_or(FieldError::Layer(selector))?;
        let [x, y] = point.map(i32::from);
        let tile_index = (y / 8) * 64 + x / 8;
        let tile = layer
            .tiles
            .get(tile_index as usize)
            .ok_or(FieldError::Tile(tile_index))?;
        let pixel = i32::from(*tile) * 64 + y % 8 * 8 + x % 8;
        let angle = *layer
            .angles
            .get(pixel as usize)
            .ok_or(FieldError::Pixel(pixel))?;
        let material = *layer
            .materials
            .get(pixel as usize)
            .ok_or(FieldError::Pixel(pixel))?;
        Ok(apply_material(ball, result, angle, material))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    #[derive(Deserialize)]
    struct Case(
        u16,
        u8,
        usize,
        bool,
        [i16; 2],
        [i16; 2],
        [i8; 2],
        u16,
        [i16; 2],
        u16,
        [u8; 2],
        [u8; 2],
    );
    #[derive(Deserialize)]
    struct Response(
        [i16; 3],
        i32,
        u8,
        u32,
        Vec<(usize, u8)>,
        u32,
        u32,
        Vec<(usize, u32)>,
        Vec<(usize, u16)>,
    );
    #[derive(Deserialize)]
    struct Reference {
        responses: Vec<Response>,
        cases: Vec<Case>,
    }

    #[test]
    fn original_static_probes() {
        check_reference(None);
    }

    #[test]
    #[ignore = "requires the exact local A3VE cartridge in A3VE_ROM"]
    fn original_static_response() {
        let rom = std::fs::read(std::env::var_os("A3VE_ROM").expect("set A3VE_ROM")).unwrap();
        check_reference(Some(&ResponseTables::from_rom(&rom).unwrap()));
    }

    fn check_reference(tables: Option<&ResponseTables>) {
        let reference: Reference =
            serde_json::from_str(include_str!("../../tests/collision-vectors.json")).unwrap();
        let tiles = [0; 512];
        let angles: [u8; 64] = std::array::from_fn(|i| (i as u8).wrapping_mul(73).wrapping_add(11));
        for (
            index,
            Case(
                table_id,
                material,
                phase,
                enabled,
                prior,
                current,
                offset,
                seed,
                point,
                result,
                kind,
                adjustment,
            ),
        ) in reference.cases.into_iter().enumerate()
        {
            let materials: [u8; 64] =
                std::array::from_fn(|i| if (i + phase) % 5 == 0 { material } else { 0 });
            let mut field = StaticField {
                table_id,
                layers: &[TileLayer {
                    tiles: &tiles,
                    angles: &angles,
                    materials: &materials,
                }],
                flipper_origins_x: [1000; 2],
                flipper_origin_y: 1000,
            };
            let mut ball = MainBall::default();
            for (i, v) in current.into_iter().chain(prior).enumerate() {
                let at = 0xa0 + i * 2;
                ball.as_bytes_mut()[at..at + 2].copy_from_slice(&v.to_le_bytes());
            }
            ball.as_bytes_mut()[0x21..0x23].copy_from_slice(&offset.map(|v| v as u8));
            ball.as_bytes_mut()[0x2a..0x6a].fill(enabled as u8);
            let original = ball.clone();
            let mut expected = ball.clone();
            expected.as_bytes_mut()[4..6].copy_from_slice(&kind);
            expected.as_bytes_mut()[0x23..0x25].copy_from_slice(&adjustment);
            assert_eq!(
                probe(&mut field, &mut ball, seed),
                Ok((point, result)),
                "case {index}"
            );
            assert_eq!(ball, expected, "case {index}");
            if let Some(t) = tables {
                let Response(
                    velocity,
                    fixed,
                    latch,
                    counter,
                    changes,
                    new_counter,
                    angle,
                    history,
                    events,
                ) = &reference.responses[index];
                let mut ball = original;
                for (i, v) in velocity.iter().enumerate() {
                    let at = 0xa8 + 2 * i;
                    ball.as_bytes_mut()[at..at + 2].copy_from_slice(&v.to_le_bytes());
                }
                ball.as_bytes_mut()[0x90..0x94].copy_from_slice(&fixed.to_le_bytes());
                ball.as_bytes_mut()[0x26] = *latch;
                let mut expected = ball.clone();
                for &(at, v) in changes {
                    expected.as_bytes_mut()[at] = v;
                }
                let mut global = ResponseState {
                    angle: 0x12345678,
                    counter: *counter,
                    history: [0; 256],
                    events: [0; 30],
                };
                let mut expected_global = global.clone();
                expected_global.angle = *angle;
                expected_global.counter = *new_counter;
                for &(at, value) in history {
                    expected_global.history[at] = value;
                }
                for &(at, value) in events {
                    expected_global.events[at] = value;
                }
                resolve(&mut field, table_id, &mut ball, seed, t, &mut global).unwrap();
                assert_eq!(ball, expected, "response case {index}");
                assert_eq!(global, expected_global, "native globals case {index}");
            }
        }
    }
}

/// Native response constants are read from the local verified cartridge.
pub struct ResponseTables {
    trig: super::math::TrigTable,
    separation: [(i8, i8, u16); 8],
    flipper_separation: [(i8, i8, u16); 4],
    kick_words: [u16; 104],
    impulses: [i16; 9],
}

impl ResponseTables {
    pub fn from_rom(rom: &[u8]) -> Result<Self, super::math::MathError> {
        let trig = super::math::TrigTable::from_rom(rom)?;
        let separation = std::array::from_fn(|i| {
            let at = 0xaf824 + i * 4;
            (
                rom[at] as i8,
                rom[at + 1] as i8,
                u16::from_le_bytes([rom[at + 2], rom[at + 3]]),
            )
        });
        let impulses = std::array::from_fn(|i| {
            let at = 0xafa2c + i * 2;
            i16::from_le_bytes([rom[at], rom[at + 1]])
        });
        Ok(Self {
            trig,
            separation,
            flipper_separation: std::array::from_fn(|i| {
                let at = 0xaf844 + i * 4;
                (rom[at] as i8, rom[at + 1] as i8, u16_at(rom, at + 2))
            }),
            kick_words: std::array::from_fn(|i| u16_at(rom, 0xaf844 + i * 2)),
            impulses,
        })
    }
}

/// Native side effects at IWRAM 0x02032770 (contact angle), 0x030045c4
/// (counter), 0x030040f0 (history) and halfwords at EWRAM 0x02034f50.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResponseState {
    pub angle: u32,
    pub counter: u32,
    pub history: [u32; 256],
    pub events: [u16; 30],
}

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn add_velocity(ball: &mut MainBall, at: usize, amount: i16) {
    let next = u16_at(ball.as_bytes(), at).wrapping_add(amount as u16);
    ball.as_bytes_mut()[at..at + 2].copy_from_slice(&next.to_le_bytes());
}

// Native 0x2cab4.
fn contact_adjust(
    ball: &mut MainBall,
    point: &mut [i16; 2],
    angle: u16,
    trig: &super::math::TrigTable,
) -> [i16; 2] {
    let bytes = ball.as_bytes_mut();
    let dx = bytes[0x23] as i8 as i16;
    let dy = bytes[0x24] as i8 as i16;
    let mut impulse = [0; 2];
    let activate = |bytes: &mut [u8]| {
        bytes[0x25] = 1;
        bytes[0x21] = dx as u8;
        bytes[0x22] = dy as u8;
    };
    if bytes[0x21] != 0 {
        point[0] = point[0].wrapping_sub(dx);
        let cosine = i32::from(trig.cosine(u32::from(angle)));
        if i32::from(dx) * cosine < 0 {
            impulse[0] = (cosine * 70 / 20000) as i16;
            activate(bytes);
        }
    }
    if dy > 0 {
        point[1] = point[1].wrapping_sub(dy);
        let factor = if bytes[0x21] == 0 { -100 } else { -75 };
        impulse[1] = (i32::from(trig.sine(u32::from(angle))) * factor / 20000) as i16;
        activate(bytes);
    }
    impulse
}

// Native 0x2cd20 (material impulse and table events).
fn material_response(
    t: &ResponseTables,
    subtype: u8,
    table: u16,
    x: i32,
    uv: &mut [i32; 2],
    events: &mut [u16; 30],
) {
    match subtype {
        2 if (-1..=1).contains(&uv[1]) => (),
        2 => {
            let left = x <= 0x77ff;
            events[if left { 0x30 / 2 } else { 0x32 / 2 }] = 1;
            if table == 2 || table == 3 {
                uv[1] -= 127;
            } else {
                uv[1] -= 170;
                uv[0] += if left { -77 } else { 77 };
            }
        }
        3 => (),
        1 | 4 | 5 | 6 | 7 | 8 => {
            let index = if subtype == 6 { 6 } else { 1 };
            uv[1] -= i32::from(t.impulses[index]) * 80 / 100;
            let event = match subtype {
                1 => 0x1c,
                4 => 0x1e,
                5 => 0x20,
                6 => 0x22,
                7 => 0x24,
                _ => 0x26,
            };
            if subtype >= 6 || table == 1 || table == 3 {
                events[event / 2] = 1;
            }
        }
        _ => uv[1] -= i32::from(t.impulses[0]),
    }
}

// Native 0x2cbd4.
fn reflect(ball: &mut MainBall, angle: u16, table: u16, t: &ResponseTables, g: &mut ResponseState) {
    let turn = 0x4000u16.wrapping_sub(angle);
    g.counter = g.counter.wrapping_add(1);
    g.history[(g.counter & 255) as usize] = u32::from(turn);
    let c = i32::from(t.trig.cosine(u32::from(turn)));
    let s = i32::from(t.trig.sine(u32::from(turn)));
    let [vx, vy] = pair(ball, 0xa8).map(i32::from);
    let mut uv = [
        (vx * c + vy * s) / 20000,
        ((-vx * s + vy * c) / 20000) * (-22) / 100,
    ];
    let spin = i32::from(u16_at(ball.as_bytes(), 0xac) as i16);
    uv[0] += spin / 2;
    ball.as_bytes_mut()[0xac..0xae].copy_from_slice(&((uv[0] / 64) as i16).to_le_bytes());
    let x = i32::from_le_bytes(ball.as_bytes()[0x90..0x94].try_into().unwrap());
    material_response(t, ball.as_bytes()[5], table, x, &mut uv, &mut g.events);
    let velocity = [
        (uv[0] * c - uv[1] * s) / 20000,
        (uv[0] * s + uv[1] * c) / 20000,
    ];
    for (i, v) in velocity.into_iter().enumerate() {
        let at = 0xa8 + i * 2;
        ball.as_bytes_mut()[at..at + 2].copy_from_slice(&(v as i16).to_le_bytes());
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ResponseError<E> {
    Field(E),
    Flipper(FlipperError),
    Subtype(u8),
}

/// Native 0x2ba18 collision dispatcher. Moving flippers can request BIOS math.
/// This entry returns that dependency error without committing changes.
pub fn resolve<F: CollisionField>(
    field: &mut F,
    table_id: u16,
    ball: &mut MainBall,
    scratch: u16,
    t: &ResponseTables,
    g: &mut ResponseState,
) -> Result<(), ResponseError<F::Error>> {
    resolve_with_bios(
        field,
        table_id,
        ball,
        scratch,
        t,
        g,
        &mut super::math::RejectBiosMath,
    )
}

/// Use the same BIOS service provider as the native frame velocity limiter.
pub fn resolve_with_bios<F: CollisionField>(
    field: &mut F,
    table_id: u16,
    ball: &mut MainBall,
    scratch: u16,
    t: &ResponseTables,
    g: &mut ResponseState,
    bios: &mut dyn super::math::BiosMath,
) -> Result<(), ResponseError<F::Error>> {
    let mut next = ball.clone();
    let mut staged = field.clone();
    let (mut point, angle) =
        probe(&mut staged, &mut next, scratch).map_err(ResponseError::Field)?;
    match next.as_bytes()[4] {
        0 => {
            if (next.as_bytes()[0x21] as i8) > 0 && next.as_bytes()[0x26] != 1 {
                add_velocity(&mut next, 0xa8, -4);
                next.as_bytes_mut()[0x26] = 1;
            }
        }
        1 => {
            if next.as_bytes()[5] > 8 {
                return Err(ResponseError::Subtype(next.as_bytes()[5]));
            }
            g.angle = u32::from(angle);
            let extra = contact_adjust(&mut next, &mut point, angle, &t.trig);
            reflect(&mut next, angle, table_id, t, g);
            if let Some(&(dx, dy, _)) = t.separation.iter().find(|entry| entry.2 <= angle) {
                point[0] = point[0].wrapping_add(i16::from(dx));
                point[1] = point[1].wrapping_add(i16::from(dy));
            }
            for (i, amount) in extra.into_iter().enumerate() {
                add_velocity(&mut next, 0xa8 + i * 2, amount);
            }
            set_position(&mut next, point);
        }
        _ => {
            let mut staged_globals = g.clone();
            staged
                .hit_flipper(&mut next, &mut point, angle, t, &mut staged_globals, bios)
                .map_err(ResponseError::Flipper)?;
            if next.as_bytes()[4] != 0 {
                set_position(&mut next, point);
            }
            *g = staged_globals;
        }
    }
    *ball = next;
    *field = staged;
    Ok(())
}

/// Collision masks selected through +0x1d8 and +0x1dc, already resolved
/// against the current table's memory. Each animation plane is 96 by 96.
#[derive(Clone, Copy)]
pub struct FlipperMasks<'a> {
    pub main: &'a [u16],
    pub extra: &'a [u16],
}

pub struct FlipperLookups {
    frames: [[u8; 70]; 3],
    neighbours: [[i8; 2]; 4],
}
impl FlipperLookups {
    pub fn from_rom(rom: &[u8]) -> Result<Self, super::math::MathError> {
        crate::catalog::validate_rom(rom).map_err(|_| super::math::MathError::SourceIdentity)?;
        let frames = std::array::from_fn(|i| {
            rom[0xaf914 + i * 70..0xaf914 + (i + 1) * 70]
                .try_into()
                .unwrap()
        });
        let neighbours =
            std::array::from_fn(|i| [rom[0x6e93a0 + 2 * i] as i8, rom[0x6e93a1 + 2 * i] as i8]);
        Ok(Self { frames, neighbours })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum FlipperError {
    FrameIndex(i32),
    MaskIndex(i32),
    ClosedMask,
    MissingRecords,
    KickTable(i32),
    Bios(super::math::MathError),
    KickCycle,
}

/// Native 0x2c56c. A visit can set the sampled-frame byte even on a miss.
/// The angle slot is unchanged on a miss. Inputs correspond to the four
/// 12-byte records beginning at +0xc4.
#[allow(clippy::too_many_arguments)] // Native point, frame, side and independent memory operands.
fn sample_flipper(
    side: usize,
    table: u16,
    mut point: [i16; 2],
    phase: u16,
    records: &mut super::flippers::Flippers,
    masks: FlipperMasks<'_>,
    lut: &FlipperLookups,
    angle: &mut u16,
) -> Result<bool, FlipperError> {
    let group = if side < 2 {
        0
    } else if table == 1 {
        1
    } else if table == 2 {
        2
    } else {
        return Ok(false);
    };
    let at = side * 12;
    let selector = i32::from(records.as_bytes()[0xc8 + at] as i8) * 5 + i32::from(phase as i16);
    let frame = *lut.frames[group]
        .get(selector as usize)
        .ok_or(FlipperError::FrameIndex(selector))? as i8;
    let map = if group == 0 { masks.main } else { masks.extra };
    let plane = i32::from(frame) * 96 * 96;
    let index = |p: [i16; 2]| p[1].wrapping_mul(96).wrapping_add(p[0]) as u16;
    let read = |p: [i16; 2], shift: bool| {
        let raw = index(p);
        let pixel = if shift && group == 2 && frame <= 3 {
            raw.saturating_sub(0x600)
        } else {
            raw
        };
        let at = plane + i32::from(pixel);
        map.get(at as usize)
            .copied()
            .ok_or(FlipperError::MaskIndex(at))
    };
    if read(point, true)? & 15 == 0 {
        records.as_bytes_mut()[0xcb + at] = frame as u8;
        return Ok(false);
    }
    let dy = if (records.as_bytes()[0xce + at] as i8) < 0 {
        1
    } else {
        -1
    };
    // A complete period returns to the same packed coordinate; a closed
    // malformed mask would leave the native boundary search looping forever.
    for _ in 0..=u16::MAX {
        for [ox, oy] in lut.neighbours {
            let neighbour = [
                point[0].wrapping_add(i16::from(ox)),
                point[1].wrapping_add(i16::from(oy)),
            ];
            if read(neighbour, false)? & 15 == 0 {
                let normal = read(point, true)? & 0xfff0;
                *angle = if side % 2 == 1 {
                    0x8000u16.wrapping_sub(normal)
                } else {
                    normal
                };
                records.as_bytes_mut()[0xcb + at] = frame as u8;
                return Ok(true);
            }
        }
        point[0] = point[0].wrapping_add(1);
        point[1] = point[1].wrapping_add(dy);
    }
    Err(FlipperError::ClosedMask)
}

/// Native 0x2c3ac point classification. Overlapping masks are sampled in
/// order, with the last contact determining kind and normal.
#[allow(clippy::too_many_arguments)] // Ball prefix and mutable flipper records have separate owners.
pub fn classify_flippers(
    table: u16,
    origins: [[i16; 2]; 4],
    point: [i16; 2],
    ball: &mut MainBall,
    records: &mut super::flippers::Flippers,
    masks: FlipperMasks<'_>,
    lut: &FlipperLookups,
    angle: &mut u16,
) -> Result<bool, FlipperError> {
    let mut next = records.clone();
    let mut result = *angle;
    let mut kind = 0;
    let sides = if table == 1 || (table == 2 && u16_at(ball.as_bytes(), 0x28) == 0) {
        4
    } else {
        2
    };
    for (side, origin) in origins.iter().enumerate().take(sides) {
        if side == 2 && table == 1 {
            continue;
        }
        let mut local = [0, 1].map(|i| point[i].wrapping_sub(origin[i].wrapping_mul(2)));
        if local.iter().any(|&v| v as u16 > 95) {
            continue;
        }
        if side % 2 == 1 {
            local[0] = 95 - local[0];
        }
        if sample_flipper(
            side,
            table,
            local,
            u16::from(ball.as_bytes()[2]) + 1,
            &mut next,
            masks,
            lut,
            &mut result,
        )? {
            kind = side as u8 + 2;
        }
    }
    if kind != 0 {
        ball.as_bytes_mut()[4] = kind;
    }
    *angle = result;
    *records = next;
    Ok(kind != 0)
}

#[cfg(test)]
#[test]
#[ignore = "requires the exact local A3VE cartridge in A3VE_ROM"]
fn original_flipper_masks() {
    use serde::Deserialize;
    #[derive(Deserialize)]
    struct Case(
        u16,
        [i16; 2],
        [[i16; 2]; 4],
        u8,
        u16,
        [u8; 4],
        [i8; 4],
        bool,
        u8,
        u16,
        [u8; 4],
    );
    #[derive(Deserialize)]
    struct Dispatch(
        Vec<(usize, u8)>,
        Vec<(usize, u8)>,
        [[u32; 3]; 2],
        u32,
        u32,
        Vec<(usize, u16)>,
        Vec<(usize, u32)>,
        Vec<(u8, i64, i64, u16)>,
    );
    #[derive(Deserialize)]
    struct Reference {
        #[serde(rename = "flipperDispatch")]
        dispatch: Vec<Dispatch>,
        flippers: Vec<Case>,
        retail: Vec<serde_json::Value>,
    }
    let rom = std::fs::read(std::env::var_os("A3VE_ROM").expect("set A3VE_ROM")).unwrap();
    let lut = FlipperLookups::from_rom(&rom).unwrap();
    let regions = MapRegions::from_rom(&rom).unwrap();
    let tables = ResponseTables::from_rom(&rom).unwrap();
    let mk_mask = |bank: usize, frames: usize| -> Vec<u16> {
        (0..frames * 96 * 96)
            .map(|i| {
                let frame = i / (96 * 96);
                let y = (i / 96) % 96;
                let x = i % 96;
                let filled = (10..=80).contains(&x)
                    && (6..=78).contains(&y)
                    && !(x + 3 * y + frame + bank).is_multiple_of(11);
                (((x * 61 + y * 283 + frame * 0x140 + bank * 0x500) as u16) & 0xfff0)
                    | u16::from(filled)
            })
            .collect()
    };
    let count = |group: usize| usize::from(*lut.frames[group].iter().max().unwrap()) + 1;
    let main = mk_mask(0, count(0));
    let extra = mk_mask(1, count(1).max(count(2)));
    let reference: Reference =
        serde_json::from_str(include_str!("../../tests/collision-vectors.json")).unwrap();
    let check = |index: usize,
                 table: u16,
                 origins: [[i16; 2]; 4],
                 region_flags: [u16; 11],
                 layers: [[u32; 3]; 2],
                 masks: FlipperMasks,
                 dispatch: &Dispatch| {
        let Dispatch(input, changes, banks, counter, angle, events, history, calls) = dispatch;
        struct Replay<'a>(&'a [(u8, i64, i64, u16)]);
        impl super::math::BiosMath for Replay<'_> {
            fn sqrt(&mut self, x: u32) -> Result<u16, super::math::MathError> {
                let (&(svc, xx, _, v), tail) = self.0.split_first().expect("unexpected sqrt");
                assert_eq!((svc, xx), (8, i64::from(x)));
                self.0 = tail;
                Ok(v)
            }
            fn atan2(&mut self, x: i16, y: i16) -> Result<u16, super::math::MathError> {
                let (&(svc, xx, yy, v), tail) = self.0.split_first().expect("unexpected atan2");
                assert_eq!((svc, xx, yy), (10, i64::from(x), i64::from(y)));
                self.0 = tail;
                Ok(v)
            }
        }
        let mut initial = [0; super::flippers::STATE_BYTES];
        for &(at, v) in input {
            initial[at] = v;
        }
        let mut expected = initial;
        for &(at, v) in changes {
            expected[at] = v;
        }
        let mut ball =
            MainBall::from_bytes(initial[..super::ball::STATE_BYTES].try_into().unwrap());
        let mut field = CartridgeField {
            table_id: table,
            region_flags,
            layers,
            origins,
            flippers: super::flippers::Flippers::from_bytes(initial),
            masks,
            lookups: &lut,
            regions: &regions,
        };
        let mut g = ResponseState {
            angle: 0x12345678,
            counter: 255,
            history: [0; 256],
            events: [0; 30],
        };
        let mut expected_g = g.clone();
        expected_g.counter = *counter;
        expected_g.angle = *angle;
        for &(j, v) in history {
            expected_g.history[j] = v;
        }
        for &(j, v) in events {
            expected_g.events[j] = v;
        }
        let mut bios = Replay(calls);
        resolve_with_bios(
            &mut field, table, &mut ball, 0xbabe, &tables, &mut g, &mut bios,
        )
        .unwrap();
        assert!(bios.0.is_empty(), "case {index}");
        assert_eq!(field.layers, *banks, "banks {index}");
        assert_eq!(g, expected_g, "global {index}");
        let mut actual = *field.flippers.as_bytes();
        actual[..super::ball::STATE_BYTES].copy_from_slice(ball.as_bytes());
        assert_eq!(actual, expected, "dispatch {index}");
    };
    for (
        index,
        Case(table, point, origins, phase, flag, frames, directions, hit, kind, angle, sampled),
    ) in reference.flippers.into_iter().enumerate()
    {
        let mut ball = MainBall::default();
        ball.as_bytes_mut()[2] = phase;
        ball.as_bytes_mut()[4] = 9;
        ball.as_bytes_mut()[0x28..0x2a].copy_from_slice(&flag.to_le_bytes());
        let mut records = super::flippers::Flippers::from_bytes([0; super::flippers::STATE_BYTES]);
        for j in 0..4 {
            records.as_bytes_mut()[0xc8 + 12 * j] = frames[j];
            records.as_bytes_mut()[0xcb + 12 * j] = 0xfe;
            records.as_bytes_mut()[0xce + 12 * j] = directions[j] as u8;
        }
        let mut expected = records.clone();
        for (j, frame) in sampled.into_iter().enumerate() {
            expected.as_bytes_mut()[0xcb + 12 * j] = frame;
        }
        let mut actual = 0xbabe;
        assert_eq!(
            classify_flippers(
                table,
                origins,
                point,
                &mut ball,
                &mut records,
                FlipperMasks {
                    main: &main,
                    extra: &extra
                },
                &lut,
                &mut actual
            ),
            Ok(hit),
            "case {index}"
        );
        assert_eq!((ball.as_bytes()[4], actual), (kind, angle), "case {index}");
        assert_eq!(records, expected, "case {index}");
        check(
            index,
            table,
            origins,
            [0; 11],
            [[0x0833cb70; 3]; 2],
            FlipperMasks {
                main: &main,
                extra: &extra,
            },
            &reference.dispatch[index],
        );
    }
    let original = read_mask_planes(&rom, 0x0866a0b8, 21).unwrap();
    for (i, row) in reference.retail.into_iter().enumerate() {
        let mut fields = row.as_array().unwrap().clone();
        let dispatch: Dispatch =
            serde_json::from_value(serde_json::Value::Array(fields.split_off(4))).unwrap();
        let layers = serde_json::from_value(fields.pop().unwrap()).unwrap();
        let flags = serde_json::from_value(fields.pop().unwrap()).unwrap();
        let origins = serde_json::from_value(fields.pop().unwrap()).unwrap();
        let table = serde_json::from_value(fields.pop().unwrap()).unwrap();
        check(
            400 + i,
            table,
            origins,
            flags,
            layers,
            FlipperMasks {
                main: &original,
                extra: &original,
            },
            &dispatch,
        );
    }
}

/// Native layer pointer writes (+0x118, +0x158, +0x198) and the point used
/// for sampling. Tables 0/4 retain the installed pointers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapSelection {
    pub pointers: Option<[u32; 3]>,
    pub point: [i16; 2],
    pub stride: u16,
}

pub struct MapRegions<'a> {
    rom: &'a [u8],
    base: [[[u32; 3]; 2]; 3],
    regions: [[u16; 5]; 11],
}

impl<'a> MapRegions<'a> {
    pub fn from_rom(rom: &'a [u8]) -> Result<Self, super::math::MathError> {
        crate::catalog::validate_rom(rom).map_err(|_| super::math::MathError::SourceIdentity)?;
        let base = std::array::from_fn(|table| {
            std::array::from_fn(|layer| {
                std::array::from_fn(|kind| {
                    let at = 0x6e9340 + 32 * kind + 8 * table + 4 * layer;
                    u32::from_le_bytes(rom[at..at + 4].try_into().unwrap())
                })
            })
        });
        let regions = std::array::from_fn(|index| {
            std::array::from_fn(|coordinate| u16_at(rom, 0xaebb0 + index * 10 + coordinate * 2))
        });
        Ok(Self { rom, base, regions })
    }

    /// 0x2d70c receives a decomposed point. Its caller 0x2c238 always
    /// divides each original signed coordinate by eight toward zero.
    /// Flags are eleven halfwords at 0x02032980, layer is state +0x6c,
    /// and gate is state +0x6e.
    pub fn select(
        &self,
        table: u16,
        layer: u16,
        gate: u16,
        flags: &[u16; 11],
        point: [i16; 2],
    ) -> Result<MapSelection, FieldError> {
        let mut selection = MapSelection {
            pointers: None,
            point,
            stride: if table == 2 { 65 } else { 64 },
        };
        let range = match table {
            0 | 4 => return Ok(selection),
            1 => 0..3,
            2 => 3..9,
            3 => 9..11,
            _ => return Err(FieldError::DynamicTable(table)),
        };
        let base = self.base[usize::from(table - 1)]
            .get(usize::from(layer))
            .ok_or(FieldError::MapLayer(layer))?;
        selection.pointers = Some(*base);
        let [x, y] = point.map(i32::from);
        for index in range {
            let [region_layer, left, right, top, bottom] = self.regions[index];
            if region_layer != layer
                || !(i32::from(left)..=i32::from(right)).contains(&x)
                || !(i32::from(top)..=i32::from(bottom)).contains(&y)
            {
                continue;
            }
            let flag = flags[index];
            let overlay = match index {
                0 if flag == 0 => return Ok(selection),
                0 if flag == 1 => [0x085bc118, 0x085bcea8, 0x085c2428],
                0 => [0x085c79a8, 0x085c8738, 0x085cddb8],
                1 if flag != 0 && gate == 0 => return Ok(selection),
                1 => [0x085d3438, 0x085d3498, 0x085d3998],
                2 if flag != 0 => [0x085baed8, 0x085baf98, 0x085bb858],
                3 if flag != 0 => [0x08623888, 0x08623a58, 0x08624ed8],
                4 if flag != 0 => [0x08626358, 0x08626448, 0x08627508],
                5 if flag != 0 => [0x086285c8, 0x08628708, 0x08629708],
                6 if flag != 0 => [0x08622a18, 0x08622a88, 0x08623188],
                9 if flag == 1 => [0x08652408, 0x08652578, 0x08653078],
                9 if flag > 1 => *base,
                10 if flag != 0 => [0x08653b78, 0x08653c38, 0x08654538],
                _ => return Ok(selection),
            };
            selection.pointers = Some(overlay);
            let border = if table == 1 { 0 } else { 8 };
            selection.point = [
                (x - i32::from(left) + border) as i16,
                (y - i32::from(top) + border) as i16,
            ];
            selection.stride = ((i32::from(right) - i32::from(left) + 16) / 8) as u16;
            break;
        }
        Ok(selection)
    }
}

#[cfg(test)]
#[test]
#[ignore = "requires the exact local A3VE cartridge in A3VE_ROM"]
fn original_dynamic_maps() {
    use serde::Deserialize;
    #[derive(Deserialize)]
    struct Case(
        u16,
        u16,
        u16,
        [u16; 11],
        [i16; 2],
        Option<[u32; 3]>,
        [i16; 2],
        u16,
    );
    #[derive(Deserialize)]
    struct Reference {
        dynamic: Vec<Case>,
    }
    let rom = std::fs::read(std::env::var_os("A3VE_ROM").expect("set A3VE_ROM")).unwrap();
    let regions = MapRegions::from_rom(&rom).unwrap();
    let reference: Reference =
        serde_json::from_str(include_str!("../../tests/collision-vectors.json")).unwrap();
    for (index, Case(table, layer, gate, flags, point, pointers, after, stride)) in
        reference.dynamic.into_iter().enumerate()
    {
        assert_eq!(
            regions.select(table, layer, gate, &flags, point),
            Ok(MapSelection {
                pointers,
                point: after,
                stride
            }),
            "case {index}"
        );
    }
}

fn apply_material(ball: &mut MainBall, result: &mut u16, angle: u8, material: u8) -> bool {
    let subtype = if (0xd0..=0xef).contains(&material) {
        let flag = 0x2a + 2 * usize::from(material - 0xd0);
        if ball.as_bytes()[flag..flag + 2] == [0, 0] {
            return false;
        }
        0
    } else {
        let low = material & 15;
        if !(1..=9).contains(&low) {
            return false;
        }
        low - 1
    };
    ball.as_bytes_mut()[4..6].copy_from_slice(&[1, subtype]);
    *result = u16::from(angle) << 8;
    true
}

/// Complete point classifiers using native cartridge addresses. The mutable
/// bindings mirror the two layer slots and persist through the substeps.
#[derive(Clone)]
pub struct CartridgeField<'a> {
    pub table_id: u16,
    pub region_flags: [u16; 11],
    pub layers: [[u32; 3]; 2],
    pub origins: [[i16; 2]; 4],
    pub flippers: super::flippers::Flippers,
    pub masks: FlipperMasks<'a>,
    pub lookups: &'a FlipperLookups,
    pub regions: &'a MapRegions<'a>,
}

impl CollisionField for CartridgeField<'_> {
    type Error = FieldError;
    fn hit_flipper(
        &mut self,
        ball: &mut MainBall,
        point: &mut [i16; 2],
        angle: u16,
        tables: &ResponseTables,
        state: &mut ResponseState,
        bios: &mut dyn super::math::BiosMath,
    ) -> Result<(), FlipperError> {
        self.flipper_response(ball, point, angle, tables, state, bios)
    }
    fn classify(
        &mut self,
        stage: usize,
        ball: &mut MainBall,
        point: [i16; 2],
        result: &mut u16,
    ) -> Result<bool, FieldError> {
        if stage == 1 {
            return classify_flippers(
                self.table_id,
                self.origins,
                point,
                ball,
                &mut self.flippers,
                self.masks,
                self.lookups,
                result,
            )
            .map_err(FieldError::Flipper);
        }
        let layer = u16_at(ball.as_bytes(), 0x6c);
        let selection = self.regions.select(
            self.table_id,
            layer,
            u16_at(ball.as_bytes(), 0x6e),
            &self.region_flags,
            point,
        )?;
        if let Some(pointers) = selection.pointers {
            *self
                .layers
                .get_mut(usize::from(layer))
                .ok_or(FieldError::MapLayer(layer))? = pointers;
        }
        let active = ball.as_bytes()[6] as i8;
        let [tiles, angles, materials] = *self
            .layers
            .get(active as usize)
            .ok_or(FieldError::Layer(active))?;
        let byte = |base: u32, offset: i32| {
            let addr = i64::from(base) + i64::from(offset);
            self.regions
                .rom
                .get((addr - 0x08000000) as usize)
                .copied()
                .ok_or(FieldError::RomAddress(addr))
        };
        let [x, y] = selection.point.map(i32::from);
        let index = (y / 8 * i32::from(selection.stride) + x / 8) * 2;
        let tile = i16::from_le_bytes([byte(tiles, index)?, byte(tiles, index + 1)?]);
        let pixel = i32::from(tile) * 64 + (y % 8) * 8 + x % 8;
        Ok(apply_material(
            ball,
            result,
            byte(angles, pixel)?,
            byte(materials, pixel)?,
        ))
    }
}

fn set_position(ball: &mut MainBall, point: [i16; 2]) {
    for (i, v) in point.into_iter().enumerate() {
        let at = 0xa0 + i * 2;
        ball.as_bytes_mut()[at..at + 2].copy_from_slice(&v.to_le_bytes());
        let at = 0x90 + i * 4;
        ball.as_bytes_mut()[at..at + 4].copy_from_slice(&(i32::from(v) << 7).to_le_bytes());
    }
}

impl CartridgeField<'_> {
    fn flipper_response(
        &mut self,
        ball: &mut MainBall,
        point: &mut [i16; 2],
        angle: u16,
        t: &ResponseTables,
        g: &mut ResponseState,
        bios: &mut dyn super::math::BiosMath,
    ) -> Result<(), FlipperError> {
        let side = usize::from(ball.as_bytes()[4]) - 2;
        if side >= 4 {
            return Err(FlipperError::MissingRecords);
        }
        if ball.as_bytes()[6] != 0 {
            ball.as_bytes_mut()[4] = 0;
            return Ok(());
        }
        g.events[0x34 / 2 + side] = 1;
        let extra = contact_adjust(ball, point, angle, &t.trig);
        let origin = self.origins[side].map(|v| v.wrapping_mul(2));
        let mirrored = side % 2 == 1;
        let mut local = [0, 1].map(|i| point[i].wrapping_sub(origin[i]));
        if self.flippers.as_bytes()[0xca + 12 * side] == 0 {
            if side >= 2 && angle == 0 {
                g.angle = g.angle.wrapping_add(1);
            }
            reflect(ball, angle, self.table_id, t, g);
            self.flippers.as_bytes_mut()[0xca + 12 * side] = 1;
        }
        let mut velocity = pair(ball, 0xa8);
        if mirrored {
            local[0] = 95i16.wrapping_sub(local[0]);
            velocity[0] = velocity[0].wrapping_neg();
        }
        let resting = self.kick(side, &mut local, &mut velocity, t, bios)?;
        if resting {
            ball.as_bytes_mut()[4] = 6;
            if let Some(&(dx, dy, _)) = t.flipper_separation.iter().find(|entry| entry.2 <= angle) {
                local[0] = local[0].wrapping_add(if mirrored {
                    -i16::from(dx)
                } else {
                    i16::from(dx)
                });
                local[1] = local[1].wrapping_add(i16::from(dy));
            }
        }
        if mirrored {
            local[0] = 95i16.wrapping_sub(local[0]);
            velocity[0] = velocity[0].wrapping_neg();
        }
        for i in 0..2 {
            point[i] = local[i].wrapping_add(origin[i]);
            let at = 0xa8 + 2 * i;
            ball.as_bytes_mut()[at..at + 2]
                .copy_from_slice(&velocity[i].wrapping_add(extra[i]).to_le_bytes());
        }
        Ok(())
    }

    // Native 0x2d3f0 (main pair) and 0x2cf7c (extra pair).
    fn kick(
        &mut self,
        side: usize,
        point: &mut [i16; 2],
        v: &mut [i16; 2],
        t: &ResponseTables,
        bios: &mut dyn super::math::BiosMath,
    ) -> Result<bool, FlipperError> {
        let at = 12 * side;
        let group = if side < 2 {
            0
        } else if self.table_id == 1 {
            1
        } else {
            2
        };
        let direction = self.flippers.as_bytes()[0xce + at] as i8;
        let rotation = u16_at(self.flippers.as_bytes(), 0xc4 + at)
            .wrapping_sub(u16_at(self.flippers.as_bytes(), 0xc6 + at))
            as i16;
        if rotation.wrapping_mul(i16::from(direction)) <= 0 {
            return Ok(true);
        }
        let frame = self.flippers.as_bytes()[0xc8 + at] as i8;
        let rising = frame <= 7;
        let mut fixed = point.map(|p| p.wrapping_mul(128));
        let mut visited = std::collections::BTreeSet::new();
        loop {
            let sample = self.flippers.as_bytes()[0xcb + at] as i8;
            if !visited.insert((fixed, *v, sample)) {
                return Err(FlipperError::KickCycle);
            }
            let base = i32::from(sample);
            let offset = match (group, rising) {
                (0, true) => -9,
                (0, false) => -7,
                (1, true) => -5,
                (1, false) => -3,
                (2, true) => -1,
                _ => 1,
            };
            let word = |i: i32, secondary: bool| {
                let index = 8 + group * 32 + i * 2 + i32::from(secondary);
                t.kick_words
                    .get(index as usize)
                    .copied()
                    .ok_or(FlipperError::KickTable(index))
            };
            let angle =
                word(base + offset, !rising)?.wrapping_add(if rising { 0x4000 } else { 0xc000 });
            let dx = i32::from(fixed[0].wrapping_sub(0xb00));
            let dy = i32::from(fixed[1].wrapping_sub(0x1800));
            let squared = dx
                .wrapping_mul(dx)
                .wrapping_add(dy.wrapping_mul(dy))
                .wrapping_sub(0x190000)
                .wrapping_mul(4) as u32;
            let speed = i32::from(bios.sqrt(squared).map_err(FlipperError::Bios)?) / 20;
            let vector = |a: u16, xdiv: i32, ydiv: i32| {
                [
                    (i32::from(t.trig.cosine(u32::from(a))) * speed / xdiv) as i16,
                    (-i32::from(t.trig.sine(u32::from(a))) * speed / ydiv) as i16,
                ]
            };
            if side < 2 {
                let adjusted = angle
                    .wrapping_add(2000)
                    .wrapping_sub((25 * speed / 4) as u16);
                *v = vector(adjusted, 10000, 10000);
            } else {
                let mut axis = vector(angle, 20000, 20000);
                if !rising {
                    loop {
                        axis = axis.map(|a| a / 2);
                        if axis.iter().all(|a| (-79..=79).contains(a)) {
                            break;
                        }
                    }
                }
                let other = vector(word(base, false)?, 20000, 20000);
                let x = axis[0].wrapping_add((i32::from(other[0]) * 164 / 256) as i16);
                let y = axis[1].wrapping_add(other[1] / 4);
                let a = bios
                    .atan2(x, y.wrapping_neg())
                    .map_err(FlipperError::Bios)?;
                let force = vector(a, 15000, 10000);
                *v = [0, 1].map(|i| ((i32::from(v[i]) + i32::from(force[i])) / 2) as i16);
            }
            fixed = [0, 1].map(|i| fixed[i].wrapping_add(v[i]));
            *point = fixed.map(|p| p / 128);
            if point.iter().any(|p| *p as u16 > 95) {
                return Ok(false);
            }
            let mut hit = false;
            for phase in 1..=4 {
                if sample_flipper(
                    side,
                    self.table_id,
                    *point,
                    phase,
                    &mut self.flippers,
                    self.masks,
                    self.lookups,
                    &mut 0,
                )? {
                    hit = true;
                    break;
                }
            }
            if !hit {
                return Ok(false);
            }
        }
    }
}

/// Read original 96x96 mask planes from a caller-resolved native pointer.
/// 0x2a778 installs 0x0866a0b8 for the main flipper tables.
pub fn read_mask_planes(rom: &[u8], pointer: u32, count: usize) -> Result<Vec<u16>, FieldError> {
    crate::catalog::validate_rom(rom).map_err(|_| FieldError::RomAddress(i64::from(pointer)))?;
    let offset = pointer
        .checked_sub(0x08000000)
        .ok_or(FieldError::RomAddress(i64::from(pointer)))? as usize;
    let end = count
        .checked_mul(96 * 96 * 2)
        .and_then(|size| offset.checked_add(size))
        .ok_or(FieldError::RomAddress(i64::from(pointer)))?;
    let bytes = rom
        .get(offset..end)
        .ok_or(FieldError::RomAddress(i64::from(pointer)))?;
    Ok(bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect())
}
