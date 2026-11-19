//! Common BIOS RL and region-directory format used by stationary rings and interactables.
use crate::{Error, VerifiedRom};
use std::ops::Range;
const MAX_LAYOUT_BYTES: usize = 64 * 1024;
fn invalid() -> Error {
    Error("Malformed native stage placement layout".into())
}

fn word(data: &[u8], offset: usize) -> Result<usize, Error> {
    Ok(u32::from_le_bytes(crate::bounded_range(data, offset, 4)?.try_into().unwrap()) as usize)
}

fn decode_rl(source: &[u8]) -> Result<Vec<u8>, Error> {
    let header = word(source, 0)?;
    let size = header >> 8;
    if header & 255 != 0x30 || !(12..=MAX_LAYOUT_BYTES).contains(&size) {
        return Err(invalid());
    }
    let mut output = Vec::with_capacity(size);
    let mut cursor = 4;
    while output.len() < size {
        let control = *source.get(cursor).ok_or_else(invalid)?;
        cursor += 1;
        let repeat = control & 0x80 != 0;
        let count = usize::from(control & 0x7f) + if repeat { 3 } else { 1 };
        if count > size - output.len() {
            return Err(invalid());
        }
        if repeat {
            let value = *source.get(cursor).ok_or_else(invalid)?;
            cursor += 1;
            output.resize(output.len() + count, value);
        } else {
            output.extend_from_slice(crate::bounded_range(source, cursor, count)?);
            cursor += count;
        }
    }
    Ok(output)
}

pub(crate) fn stage_stream(
    rom: &VerifiedRom<'_>,
    table: usize,
    level_id: u8,
) -> Result<Vec<u8>, Error> {
    if level_id >= 34 {
        return Err(Error("Unknown native stage ID".into()));
    }
    let pointer = word(rom.rom_bytes(), table + usize::from(level_id) * 4)?;
    let offset = pointer
        .checked_sub(0x0800_0000)
        .ok_or_else(|| Error("Stage placement table slot is empty".into()))?;
    decode_rl(rom.rom_bytes().get(offset..).ok_or_else(invalid)?)
}
pub(crate) struct RegionLayout<T> {
    pub columns: usize,
    pub rows: usize,
    pub regions: Vec<Range<usize>>,
    pub records: Vec<T>,
}
impl<T> RegionLayout<T> {
    pub fn parse<const N: usize>(
        raw: &[u8],
        make: impl Fn(i32, i32, &[u8; N]) -> T,
    ) -> Result<Self, Error> {
        if word(raw, 0)? != raw.len() << 8 {
            return Err(invalid());
        }
        let columns = word(raw, 4)?;
        let rows = word(raw, 8)?;
        if columns == 0 || rows == 0 || columns > 256 || rows > 256 {
            return Err(invalid());
        }
        let region_count = columns * rows;
        let directory_end = 12 + region_count * 4;
        crate::bounded_range(raw, 0, directory_end)?;
        let mut regions = Vec::with_capacity(region_count);
        let mut records = Vec::new();
        let mut next_list = directory_end;
        for region in 0..region_count {
            let first = records.len();
            let relative = word(raw, 12 + region * 4)?;
            if relative != 0 {
                // The index points relative to the second header word.
                let mut at = relative.checked_add(4).ok_or_else(invalid)?;
                if at != next_list {
                    return Err(invalid());
                }
                while *raw.get(at).ok_or_else(invalid)? != 255 {
                    let pair = crate::bounded_range(raw, at, N)?;
                    if pair[0] >= 32 || pair[1] >= 32 {
                        return Err(invalid());
                    }
                    let x = (region % columns) as i32 * 256 + i32::from(pair[0]) * 8;
                    let y = (region / columns) as i32 * 256 + i32::from(pair[1]) * 8;
                    records.push(make(x, y, pair.try_into().unwrap()));
                    at += N;
                }
                next_list = at + 1;
            }
            regions.push(first..records.len());
        }
        if next_list != raw.len() {
            return Err(invalid());
        }
        Ok(Self {
            columns,
            rows,
            regions,
            records,
        })
    }
}
