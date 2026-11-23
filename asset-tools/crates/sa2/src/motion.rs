//! Native SAANIM keyframes without resampling or retargeting.
use crate::model::{metadata, DecodeBudget, MetadataChunk, Reader, Words};
use crate::{Error, Result};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Motion {
    pub version: u8,
    pub offset: u32,
    pub frame_count: u32,
    pub type_flags: u16,
    pub interpolation_and_slots: u16,
    pub short_rotation: bool,
    pub part_count: u32,
    pub labels: BTreeMap<u32, String>,
    pub metadata: Vec<MetadataChunk>,
    pub parts: Vec<MotionPart>,
    pub unknowns: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MotionPart {
    pub index: u32,
    pub channels: Vec<MotionChannel>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MotionChannel {
    pub kind: String,
    pub source_offset: u32,
    pub key_count: u32,
    pub keys: Vec<Keyframe>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Keyframe {
    pub frame: i32,
    pub floats: Vec<f32>,
    pub integers: Vec<i32>,
    pub raw_words: Vec<u32>,
}

pub fn read_motion(data: &[u8]) -> Result<Motion> {
    let all = Reader { data };
    let header = all.span(0, 20)?;
    if &header[..7] != b"SAANIM\0" {
        return Err(Error::invalid("unsupported motion magic"));
    }
    let version = header[7];
    if version == 0 || version > 2 {
        return Err(Error::invalid(
            "SAANIM requires supported v1/v2 explicit part count",
        ));
    }
    let offset = all.u32(8)?;
    let metadata_offset = all.u32(12)? as usize;
    let part_bits = all.u32(16)?;
    let part_count = part_bits & 0x7fff_ffff;
    if part_count == 0 || part_count > 65_536 {
        return Err(Error::invalid("motion part count outside bounds"));
    }
    let short_rotation = part_bits & 0x8000_0000 != 0;
    let (metadata, mut labels) = if version == 2 {
        metadata(data, 2, true)?
    } else {
        (Vec::new(), BTreeMap::new())
    };
    if version == 1 && metadata_offset != 0 {
        labels.insert(offset, all.string(metadata_offset, data.len())?);
    }
    let limit = if metadata_offset != 0 {
        metadata_offset
    } else {
        data.len()
    };
    let r = Reader {
        data: data
            .get(..limit)
            .ok_or_else(|| Error::invalid("motion metadata bounds"))?,
    };
    let mut motion = read_motion_in(&r, offset, part_count, short_rotation)?;
    motion.version = version;
    motion.labels = labels;
    motion.metadata = metadata;
    Ok(motion)
}

/// One native motion header at `offset` of `r`, for a model of `part_count` nodes.
pub(crate) fn read_motion_in<R: Words>(
    r: &R,
    offset: u32,
    part_count: u32,
    short_rotation: bool,
) -> Result<Motion> {
    let at = offset as usize;
    let frame_count = r.u32(at + 4)?;
    if frame_count == 0 || frame_count > 1_000_000 {
        return Err(Error::invalid("motion frame count outside bounds"));
    }
    let type_flags = r.u16(at + 8)?;
    if type_flags == 0 || type_flags & 0xc000 != 0 {
        return Err(Error::invalid("unsupported motion channel flags"));
    }
    let interpolation_and_slots = r.u16(at + 10)?;
    let count = type_flags.count_ones() as usize;
    let slots = (interpolation_and_slots & 15) as usize;
    if slots != count {
        return Err(Error::invalid(
            "motion descriptor slots differ from channel flags",
        ));
    }
    let descriptor_offset = r.u32(at)? as usize;
    if descriptor_offset == 0 {
        return Err(Error::invalid("null motion descriptor table"));
    }
    let stride = count * 8;
    r.span(descriptor_offset, stride * part_count as usize)?;
    let kinds = [
        "position",
        "rotationBams",
        "scale",
        "vector",
        "vertexPointer",
        "normalPointer",
        "target",
        "rollBams",
        "angleBams",
        "colorArgb",
        "intensity",
        "spotRaw",
        "point",
        "quaternionWxyz",
    ];
    let mut budget = DecodeBudget::new();
    budget.charge(
        part_count as usize,
        std::mem::size_of::<MotionPart>() * 2
            + count * (std::mem::size_of::<MotionChannel>() * 2 + 32),
    )?;
    let mut parts = Vec::with_capacity(part_count as usize);
    let mut total_keys = 0usize;
    let mut unknowns = vec!["Frame timing is native frame numbers; playback rate, interpolation evaluation, motion-state selection and retargeting are not inferred".into()];
    for i in 0..part_count {
        let base = descriptor_offset + stride * i as usize;
        let mut channel_index = 0;
        let mut channels = Vec::new();
        for (bit, name) in kinds.iter().enumerate() {
            if type_flags & (1 << bit) == 0 {
                continue;
            }
            let source_offset = r.u32(base + channel_index * 4)?;
            let key_count = r.u32(base + count * 4 + channel_index * 4)?;
            channel_index += 1;
            if key_count > 1_000_000 || (source_offset == 0 && key_count != 0) {
                return Err(Error::invalid("invalid motion key descriptor"));
            }
            total_keys = total_keys
                .checked_add(key_count as usize)
                .ok_or_else(|| Error::invalid("motion key overflow"))?;
            if total_keys > 2_000_000 {
                return Err(Error::invalid("motion total key limit exceeded"));
            }
            let key_size = match bit {
                0 | 2 | 3 | 6 => 16,
                1 => {
                    if short_rotation {
                        8
                    } else {
                        16
                    }
                }
                4 | 5 | 7..=10 => 8,
                11 | 13 => 20,
                12 => 12,
                _ => unreachable!(),
            };
            if key_count != 0 {
                r.span(source_offset as usize, key_count as usize * key_size)?;
            }
            budget.charge(
                key_count as usize,
                std::mem::size_of::<Keyframe>() * 2 + key_size * 3,
            )?;
            let mut keys = Vec::with_capacity(key_count as usize);
            for k in 0..key_count as usize {
                let p = source_offset as usize + k * key_size;
                let frame = if bit == 1 && short_rotation {
                    r.u16(p)? as i16 as i32
                } else {
                    r.u32(p)? as i32
                };
                if frame < 0 || frame as u32 > frame_count {
                    return Err(Error::invalid("motion frame index outside duration"));
                }
                let mut floats = Vec::new();
                let mut integers = Vec::new();
                match bit {
                    0 | 2 | 3 | 6 => floats.extend(r.vec3(p + 4)?),
                    1 if short_rotation => {
                        for n in 0..3 {
                            integers.push(r.u16(p + 2 + n * 2)? as i16 as i32);
                        }
                    }
                    1 => {
                        for n in 0..3 {
                            integers.push(r.u32(p + 4 + n * 4)? as i32);
                        }
                    }
                    4 | 5 => {
                        let pointer = r.u32(p + 4)?;
                        if pointer != 0 {
                            r.span(pointer as usize, 12)?;
                        }
                        integers.push(pointer as i32);
                    }
                    7..=9 => integers.push(r.u32(p + 4)? as i32),
                    10 => floats.push(r.f32(p + 4)?),
                    11 => {
                        for n in 0..4 {
                            integers.push(r.u32(p + 4 + n * 4)? as i32);
                        }
                    }
                    12 => {
                        floats.push(r.f32(p + 4)?);
                        floats.push(r.f32(p + 8)?);
                    }
                    13 => {
                        for n in 0..4 {
                            floats.push(r.f32(p + 4 + n * 4)?);
                        }
                    }
                    _ => unreachable!(),
                }
                let raw_words = (0..key_size / 4)
                    .map(|j| r.u32(p + j * 4))
                    .collect::<Result<Vec<_>>>()?;
                keys.push(Keyframe {
                    frame,
                    floats,
                    integers,
                    raw_words,
                });
            }
            if key_count != 0 && matches!(bit, 4 | 5 | 11) {
                let note = if bit == 11 {
                    "Spotlight value words remain raw"
                } else {
                    "Morph pointer targets retain source references; vertex cardinality requires a matching model and is not guessed"
                };
                if !unknowns.iter().any(|x| x == note) {
                    unknowns.push(note.into());
                }
            }
            channels.push(MotionChannel {
                kind: (*name).into(),
                source_offset,
                key_count,
                keys,
            });
        }
        parts.push(MotionPart { index: i, channels });
    }
    Ok(Motion {
        version: 0,
        offset,
        frame_count,
        type_flags,
        interpolation_and_slots,
        short_rotation,
        part_count,
        labels: BTreeMap::new(),
        metadata: Vec::new(),
        parts,
        unknowns,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(short: bool) -> Vec<u8> {
        let mut b = vec![0; 64];
        b[..8].copy_from_slice(b"SAANIM\0\x02");
        b[8..12].copy_from_slice(&20u32.to_le_bytes());
        b[16..20].copy_from_slice(&(1u32 | if short { 0x8000_0000 } else { 0 }).to_le_bytes());
        b[20..24].copy_from_slice(&32u32.to_le_bytes());
        b[24..28].copy_from_slice(&10u32.to_le_bytes());
        b[28..30].copy_from_slice(&2u16.to_le_bytes());
        b[30..32].copy_from_slice(&1u16.to_le_bytes());
        b[32..36].copy_from_slice(&40u32.to_le_bytes());
        b[36..40].copy_from_slice(&1u32.to_le_bytes());
        if short {
            b[42..44].copy_from_slice(&(-123i16).to_le_bytes());
        } else {
            b[44..48].copy_from_slice(&(-456i32).to_le_bytes());
        }
        b
    }
    #[test]
    fn native_rotation_width_preserved() {
        assert_eq!(
            read_motion(&fixture(true)).unwrap().parts[0].channels[0].keys[0].integers[0],
            -123
        );
        assert_eq!(
            read_motion(&fixture(false)).unwrap().parts[0].channels[0].keys[0].integers[0],
            -456
        );
    }
    #[test]
    fn rejects_pointer_and_key_count_overflow() {
        let mut b = fixture(false);
        b[32..36].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(read_motion(&b).is_err());
        let mut b = fixture(false);
        b[36..40].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(read_motion(&b).is_err());
    }
    #[test]
    fn rejects_bad_frames_and_slots() {
        let mut b = fixture(false);
        b[40..44].copy_from_slice(&11u32.to_le_bytes());
        assert!(read_motion(&b).is_err());
        let mut b = fixture(false);
        b[30..32].copy_from_slice(&2u16.to_le_bytes());
        assert!(read_motion(&b).is_err());
    }
}
