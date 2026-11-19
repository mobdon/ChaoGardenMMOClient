//! Event scene directory and frame selection for the retail BATTLE big-endian format.
use crate::{archive, Error, Result};
use serde::Serialize;

const BASE: u32 = 0x8125fe60;
const LIMIT: usize = 64 * 1024 * 1024;
fn word(b: &[u8], i: usize) -> u32 {
    u32::from_be_bytes(b[i..i + 4].try_into().unwrap())
}
fn offset(ptr: u32) -> Result<usize> {
    ptr.checked_sub(BASE)
        .map(|v| v as usize)
        .ok_or_else(|| Error::invalid("invalid BATTLE event pointer"))
}
fn table(b: &[u8], ptr: u32, count: usize, stride: usize) -> Result<&[u8]> {
    if count == 0 {
        return Ok(&[]);
    }
    let len = count
        .checked_mul(stride)
        .ok_or_else(|| Error::invalid("event array overflow"))?;
    let at = offset(ptr)?;
    b.get(
        at..at
            .checked_add(len)
            .ok_or_else(|| Error::invalid("event data overflow"))?,
    )
    .ok_or_else(|| Error::invalid("event array outside source"))
}
fn reference(b: &[u8], ptr: u32) -> Result<Option<usize>> {
    if ptr == 0 {
        return Ok(None);
    }
    table(b, ptr, 1, 4)?;
    Ok(Some(offset(ptr)?))
}

#[derive(Serialize, Debug)]
pub struct Entity {
    pub model_offset: Option<usize>,
    pub motion_index: i32,
    pub shape_motion_index: i32,
    pub gc_model_offset: Option<usize>,
    pub shadow_model_offset: Option<usize>,
    pub field_14: u32,
    pub position: [f32; 3],
    pub flags: u32,
    pub layer: u32,
}
#[derive(Serialize, Debug)]
pub struct Scene {
    pub frames: u32,
    pub entities: Vec<Entity>,
    pub camera_motions: Vec<i32>,
    pub particle_motions: Vec<i32>,
    pub big_offset: Option<usize>,
}
#[derive(Serialize, Debug)]
pub struct Event {
    pub scenes: Vec<Scene>,
}
#[derive(Serialize, Debug, PartialEq)]
pub struct Frame {
    pub global_frame: u32,
    pub scene: usize,
    pub scene_frame: u32,
}
impl Event {
    /// Scene zero supplies common entries; subsequent scene lengths partition the clip.
    /// No default/synthesized camera or player pose is assigned.
    pub fn frame(&self, global_frame: u32) -> Result<Frame> {
        let common = self
            .scenes
            .first()
            .ok_or_else(|| Error::invalid("no event scenes"))?;
        let local_frames = self.scenes[1..]
            .iter()
            .map(|s| u64::from(s.frames))
            .sum::<u64>();
        if local_frames != u64::from(common.frames) {
            return Err(Error::invalid(
                "event timeline does not match common scene length",
            ));
        }
        if global_frame >= common.frames {
            return Err(Error::invalid("frame outside event"));
        }
        let mut f = global_frame;
        for (i, s) in self.scenes.iter().enumerate().skip(1) {
            if f < s.frames {
                return Ok(Frame {
                    global_frame,
                    scene: i,
                    scene_frame: f,
                });
            }
            f -= s.frames;
        }
        Err(Error::invalid("no scene for frame"))
    }
}
pub fn read_prs(compressed: &[u8]) -> Result<Event> {
    read(&archive::decompress_prs(compressed, LIMIT)?)
}
pub fn read(b: &[u8]) -> Result<Event> {
    if b.len() < 0x2c || b.len() > LIMIT || b[0] != 0x81 || b[0x2a] != 0 || b[0x2b] > 1 {
        return Err(Error::invalid("expected a retail BATTLE event image"));
    }
    let last = word(b, 8) as usize;
    if last >= 4096 {
        return Err(Error::invalid("event scene count"));
    }
    let mut scenes = Vec::with_capacity(last + 1);
    let mut budget = 262144usize;
    for s in table(b, word(b, 0), last + 1, 32)?.as_chunks::<32>().0 {
        let counts = [
            word(s, 4) as usize,
            word(s, 12) as usize,
            word(s, 20) as usize,
        ];
        for &n in &counts {
            budget = budget
                .checked_sub(n)
                .ok_or_else(|| Error::invalid("event record budget exceeded"))?;
        }
        let mut entities = Vec::with_capacity(counts[0]);
        for row in table(b, word(s, 0), counts[0], 44)?.as_chunks::<44>().0 {
            let position = std::array::from_fn(|i| f32::from_bits(word(row, 24 + i * 4)));
            if !position.iter().all(|v| v.is_finite()) {
                return Err(Error::invalid("nonfinite event entity position"));
            }
            entities.push(Entity {
                model_offset: reference(b, word(row, 0))?,
                motion_index: word(row, 4) as i32,
                shape_motion_index: word(row, 8) as i32,
                gc_model_offset: reference(b, word(row, 12))?,
                shadow_model_offset: reference(b, word(row, 16))?,
                field_14: word(row, 20),
                position,
                flags: word(row, 36),
                layer: word(row, 40),
            });
        }
        let ids = |ptr, n| -> Result<Vec<i32>> {
            Ok(table(b, ptr, n, 4)?
                .as_chunks::<4>()
                .0
                .iter()
                .map(|x| word(x, 0) as i32)
                .collect())
        };
        let frames = word(s, 28);
        if frames > i32::MAX as u32 {
            return Err(Error::invalid("negative event frame count"));
        }
        scenes.push(Scene {
            frames,
            entities,
            camera_motions: ids(word(s, 8), counts[1])?,
            particle_motions: ids(word(s, 16), counts[2])?,
            big_offset: reference(b, word(s, 24))?,
        });
    }
    Ok(Event { scenes })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frame_boundaries_and_source_limits() {
        let mut b = vec![0u8; 0x40 + 3 * 32];
        b[..4].copy_from_slice(&(BASE + 0x40).to_be_bytes());
        b[8..12].copy_from_slice(&2u32.to_be_bytes());
        for (i, n) in [5u32, 2, 3].into_iter().enumerate() {
            let off = 0x40 + i * 32 + 28;
            b[off..off + 4].copy_from_slice(&n.to_be_bytes());
        }
        let event = read(&b).unwrap();
        assert_eq!(event.frame(1).unwrap().scene_frame, 1);
        assert_eq!(
            event.frame(2).unwrap(),
            Frame {
                global_frame: 2,
                scene: 2,
                scene_frame: 0
            }
        );
        assert!(event.frame(5).is_err());
        b[0x40 + 28..0x40 + 32].copy_from_slice(&6u32.to_be_bytes());
        assert!(read(&b).unwrap().frame(4).is_err());
        b[0x40 + 4..0x40 + 8].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(read(&b).is_err());
        for end in 0..0x40 {
            assert!(read(&b[..end]).is_err());
        }
    }
}
