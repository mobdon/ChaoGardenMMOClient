//! City Escape's stage-provided board motion defaults in stg13D.rel section 5.
use crate::city_rel::CityRel;
use crate::motion::{Keyframe, Motion, MotionChannel, MotionPart};
use crate::{manifest, pe, Error, Result};
use std::collections::BTreeMap;

const SECTION: u8 = 5;
const TABLE: usize = 0x1fd2b0;
const TABLE_SIZE: usize = 0x1c8;

#[derive(Debug)]
pub struct BoardMotionBank {
    pub slots: BTreeMap<u16, Motion>,
}

impl BoardMotionBank {
    pub fn for_action(&self, action: i16) -> Option<&Motion> {
        let index = u16::try_from(i32::from(action) + 24).ok()?;
        self.slots.get(&index)
    }
}

// Offsets are relative to .data for REL and to the image base (RVA) for PE.
enum Image<'a, 'b> {
    Bank(&'a [u8]),
    Rel(&'a CityRel<'b>),
    Pc(&'a [u8], pe::PeImage),
}

impl Image<'_, '_> {
    fn span(&self, at: u32, size: usize) -> Result<&[u8]> {
        let (bytes, offset) = match self {
            Self::Bank(bytes) => (*bytes, at as usize),
            Self::Rel(rel) => (rel.data(SECTION)?, at as usize),
            Self::Pc(bytes, image) => {
                // Windows maps the virtual tail of .data as zeroes. Some native
                // singleton key records are stored in this zero-filled range.
                if size <= 16
                    && image.sections.iter().any(|s| {
                        s.name == ".data"
                            && at.checked_sub(s.virtual_address).is_some_and(|d| {
                                d >= s.raw_size
                                    && u64::from(d) + size as u64 <= u64::from(s.virtual_size)
                            })
                    })
                {
                    return Ok(&[0u8; 16][..size]);
                }
                (*bytes, image.rva_to_offset(at, size)?)
            }
        };
        bytes
            .get(offset..)
            .and_then(|v| v.get(..size))
            .ok_or_else(|| Error::invalid("board motion outside source"))
    }
    fn u32(&self, bytes: &[u8], at: usize) -> u32 {
        let word = bytes[at..at + 4].try_into().unwrap();
        match self {
            Self::Bank(_) | Self::Rel(_) => u32::from_be_bytes(word),
            Self::Pc(..) => u32::from_le_bytes(word),
        }
    }
    fn u16(&self, bytes: &[u8], at: usize) -> u16 {
        let word = bytes[at..at + 2].try_into().unwrap();
        match self {
            Self::Bank(_) | Self::Rel(_) => u16::from_be_bytes(word),
            Self::Pc(..) => u16::from_le_bytes(word),
        }
    }
    fn pointer(&self, at: u32) -> Result<Option<u32>> {
        match self {
            Self::Bank(_) => {
                let ptr = self.u32(self.span(at, 4)?, 0);
                Ok((ptr != 0).then_some(ptr))
            }
            Self::Rel(rel) => match rel.local_pointer(SECTION, at)? {
                Some((SECTION, ptr)) => Ok(Some(ptr)),
                None => Ok(None),
                _ => Err(Error::invalid("board motion outside REL data section")),
            },
            Self::Pc(_, image) => {
                let va = self.u32(self.span(at, 4)?, 0);
                if va == 0 {
                    Ok(None)
                } else {
                    Ok(Some(va.checked_sub(image.image_base).ok_or_else(|| {
                        Error::invalid("board motion below PE image base")
                    })?))
                }
            }
        }
    }
    fn required_pointer(&self, at: u32) -> Result<u32> {
        self.pointer(at)?
            .ok_or_else(|| Error::invalid("missing board motion pointer"))
    }
}

fn channel(
    image: &Image<'_, '_>,
    descriptor: u32,
    slot: u32,
    count: u32,
    frames: u32,
) -> Result<MotionChannel> {
    let position = slot == 0;
    let kind = if position { "position" } else { "rotationBams" };
    let at = descriptor + slot * 4;
    let source_offset = match (count, image.pointer(at)?) {
        (0, None) => 0,
        (1..=4096, Some(ptr)) => ptr,
        _ => {
            return Err(Error::invalid(
                "inconsistent board motion key count and pointer",
            ))
        }
    };
    let mut keys = Vec::with_capacity(count as usize);
    let mut previous = None;
    for record in if count == 0 {
        &[]
    } else {
        image.span(source_offset, count as usize * 16)?
    }
    .as_chunks::<16>()
    .0
    {
        let frame = image.u32(record, 0) as i32;
        if frame < 0 || frame as u32 >= frames || previous.is_some_and(|v| v >= frame) {
            return Err(Error::invalid("invalid board motion key times"));
        }
        previous = Some(frame);
        let xyz = [
            image.u32(record, 4),
            image.u32(record, 8),
            image.u32(record, 12),
        ];
        let (floats, integers) = if position {
            let coords = xyz.map(f32::from_bits);
            if !coords.iter().all(|v| v.is_finite()) {
                return Err(Error::invalid("nonfinite board motion position key"));
            }
            (coords.to_vec(), Vec::new())
        } else {
            (Vec::new(), xyz.map(|v| v as i32).to_vec())
        };
        keys.push(Keyframe {
            frame,
            floats,
            integers,
            raw_words: vec![frame as u32, xyz[0], xyz[1], xyz[2]],
        });
    }
    Ok(MotionChannel {
        kind: kind.into(),
        source_offset,
        key_count: count,
        keys,
    })
}

fn motion(image: &Image<'_, '_>, header: u32, part_count: u32) -> Result<Motion> {
    let h = image.span(header, 12)?;
    let frames = image.u32(h, 4);
    if !(1..=4096).contains(&frames) || image.u16(h, 8) != 3 || image.u16(h, 10) != 2 {
        return Err(Error::invalid("unsupported board motion header"));
    }
    let descriptors = image.required_pointer(header)?;
    let mut parts = Vec::new();
    for index in 0..part_count {
        let descriptor = descriptors + 16 * index;
        let desc = image.span(descriptor, 16)?;
        parts.push(MotionPart {
            index,
            channels: vec![
                channel(image, descriptor, 0, image.u32(desc, 8), frames)?,
                channel(image, descriptor, 1, image.u32(desc, 12), frames)?,
            ],
        });
    }
    Ok(Motion {
        version: 0,
        offset: header,
        frame_count: frames,
        type_flags: 3,
        interpolation_and_slots: 2,
        short_rotation: false,
        part_count,
        labels: BTreeMap::new(),
        metadata: Vec::new(),
        parts,
        unknowns: Vec::new(),
    })
}

/// The character MTN directory stores file-relative, big-endian Ninja pointers.
pub(crate) fn bank_motion(bytes: &[u8], header: u32, parts: u16) -> Result<Motion> {
    let image = Image::Bank(bytes);
    let desc = image.required_pointer(header)?;
    let descriptors = image.span(desc, usize::from(parts) * 16)?;
    let keys: u64 = descriptors
        .chunks_exact(16)
        .map(|d| u64::from(image.u32(d, 8)) + u64::from(image.u32(d, 12)))
        .sum();
    if keys > 65536 {
        return Err(Error::invalid("character motion exceeds key budget"));
    }
    motion(&image, header, u32::from(parts))
}

pub fn read(rel: &CityRel<'_>) -> Result<BoardMotionBank> {
    read_table(&Image::Rel(rel), TABLE as u32, 145, 2)
}

pub const PC_DLL_SHA256: &str = "6ee749548ea124d22d9cbfb6f3323f6d6a9a330d9838dd2f5656be2d9b87ff59";

pub fn read_pc(bytes: &[u8]) -> Result<BoardMotionBank> {
    pc_table(bytes, 145, 2)
}
/// Stage-provided Sonic rider motions, indexed by native animation number.
pub fn rider_slots(bytes: &[u8]) -> Result<BTreeMap<u16, Motion>> {
    if bytes.starts_with(b"MZ") {
        Ok(pc_table(bytes, 121, 62)?.slots)
    } else {
        let rel = CityRel::read(bytes)?;
        Ok(read_table(&Image::Rel(&rel), TABLE as u32, 121, 62)?.slots)
    }
}
fn pc_table(bytes: &[u8], first: i16, parts: u16) -> Result<BoardMotionBank> {
    if manifest::sha256(bytes) != PC_DLL_SHA256 {
        return Err(Error::invalid("unsupported PC board data DLL"));
    }
    let pe = pe::read_pe(bytes)?;
    let table = pe
        .exports
        .iter()
        .find(|e| e.name == "header_stg13" && e.forwarded_to.is_none())
        .ok_or_else(|| Error::invalid("missing native City Escape motion list export"))?
        .rva;
    read_table(&Image::Pc(bytes, pe), table, first, parts)
}

fn read_table(image: &Image<'_, '_>, base: u32, first: i16, parts: u16) -> Result<BoardMotionBank> {
    let table = image.span(base, TABLE_SIZE)?;
    let mut slots = BTreeMap::new();
    for (i, row) in table.as_chunks::<8>().0.iter().enumerate() {
        let id = image.u16(row, 0) as i16;
        if i + 1 == TABLE_SIZE / 8 {
            if id != -1 {
                return Err(Error::invalid(
                    "missing native board motion table terminator",
                ));
            }
            break;
        }
        if !(first..first + 24).contains(&id) {
            continue;
        }
        if image.u16(row, 2) != parts {
            return Err(Error::invalid("board motion part count mismatch"));
        }
        let header = image.required_pointer(base + (i * 8 + 4) as u32)?;
        if slots
            .insert(id as u16, motion(image, header, parts.into())?)
            .is_some()
        {
            return Err(Error::invalid("duplicate board motion slot"));
        }
    }
    if slots.len() != 24 {
        return Err(Error::invalid("incomplete native board motion defaults"));
    }
    Ok(BoardMotionBank { slots })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pc_maps_complete_zero_records_only_in_data_virtual_tail() {
        let image = Image::Pc(
            &[0x5a; 32],
            pe::PeImage {
                image_base: 0,
                header_size: 0,
                exports: vec![],
                sections: vec![pe::PeSection {
                    name: ".data".into(),
                    virtual_address: 0x1000,
                    raw_offset: 0,
                    raw_size: 32,
                    virtual_size: 52,
                }],
            },
        );
        assert_eq!(image.span(0x1010, 16).unwrap(), &[0x5a; 16]);
        assert_eq!(image.span(0x1024, 16).unwrap(), &[0; 16]);
        for (start, size) in [
            (0xff0, 16),
            (0x101f, 16),
            (0x1025, 16),
            (0x1034, 1),
            (0x1020, 17),
        ] {
            assert!(image.span(start, size).is_err());
        }
    }
}
