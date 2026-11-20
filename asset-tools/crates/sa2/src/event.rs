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

/// Pointer base of the one older-layout event, `e0350.prs` (byte-identical on
/// Steam and the GameCube disc). Established from the file: at this base its
/// texture list's ten name pointers land on the names in `e0350texture.prs`.
pub const BETA_BASE: u32 = 0x812f_fe60;

/// One 32-byte entity row of the older layout. Only the first two words are
/// identified as in-file pointers; the rest are kept raw.
#[derive(Serialize, Debug)]
pub struct BetaEntity {
    pub model_offset: Option<usize>,
    pub second_offset: Option<usize>,
    pub words: [u32; 8],
}
#[derive(Serialize, Debug)]
pub struct BetaScene {
    pub frames: u32,
    pub entities: Vec<BetaEntity>,
    pub camera_motions: Vec<i32>,
    pub particle_motions: Vec<i32>,
    pub big_offset: Option<usize>,
}
#[derive(Serialize, Debug)]
pub struct BetaEvent {
    pub scenes: Vec<BetaScene>,
    pub texture_names: Vec<String>,
}

fn beta_offset(b: &[u8], ptr: u32, len: usize) -> Result<usize> {
    let at = ptr
        .checked_sub(BETA_BASE)
        .ok_or_else(|| Error::invalid("pointer below the older event base"))? as usize;
    at.checked_add(len)
        .filter(|end| *end <= b.len())
        .ok_or_else(|| Error::invalid("older event pointer outside source"))?;
    Ok(at)
}
fn beta_ref(b: &[u8], ptr: u32) -> Result<Option<usize>> {
    if ptr == 0 {
        return Ok(None);
    }
    beta_offset(b, ptr, 4).map(Some)
}

pub fn read_beta_prs(compressed: &[u8]) -> Result<BetaEvent> {
    read_beta(&archive::decompress_prs(compressed, LIMIT)?)
}

/// Read the older event layout: the retail 32-byte scene table, 32-byte entity
/// rows instead of 44, and a texture list (header word 1) of name pointers.
pub fn read_beta(b: &[u8]) -> Result<BetaEvent> {
    if b.len() < 0x30 || b.len() > LIMIT || b[0] != 0x81 {
        return Err(Error::invalid("expected an older-layout event image"));
    }
    let last = word(b, 8) as usize;
    if last >= 4096 {
        return Err(Error::invalid("event scene count"));
    }
    let scene_len = (last + 1) * 32;
    let at = beta_offset(b, word(b, 0), scene_len)?;
    let mut budget = 262_144usize;
    let mut scenes = Vec::with_capacity(last + 1);
    for s in b[at..at + scene_len].as_chunks::<32>().0 {
        let count = |i| word(s, i) as usize;
        for n in [count(4), count(12), count(20)] {
            budget = budget
                .checked_sub(n)
                .ok_or_else(|| Error::invalid("event record budget exceeded"))?;
        }
        let rows = |ptr: u32, n: usize, stride: usize| -> Result<&[u8]> {
            if n == 0 {
                return Ok(&[]);
            }
            let len = n
                .checked_mul(stride)
                .ok_or_else(|| Error::invalid("event array overflow"))?;
            let at = beta_offset(b, ptr, len)?;
            Ok(&b[at..at + len])
        };
        let mut entities = Vec::with_capacity(count(4));
        for row in rows(word(s, 0), count(4), 32)?.as_chunks::<32>().0 {
            let words = std::array::from_fn(|i| word(row, i * 4));
            entities.push(BetaEntity {
                model_offset: beta_ref(b, words[0])?,
                second_offset: beta_ref(b, words[1])?,
                words,
            });
        }
        let ids = |ptr, n| -> Result<Vec<i32>> {
            Ok(rows(ptr, n, 4)?
                .as_chunks::<4>()
                .0
                .iter()
                .map(|x| word(x, 0) as i32)
                .collect())
        };
        scenes.push(BetaScene {
            frames: word(s, 28),
            entities,
            camera_motions: ids(word(s, 8), count(12))?,
            particle_motions: ids(word(s, 16), count(20))?,
            big_offset: beta_ref(b, word(s, 24))?,
        });
    }
    // NJS_TEXLIST: {names, count}; each NJS_TEXNAME is 12 bytes, name first.
    let list = beta_offset(b, word(b, 4), 8)?;
    let n = word(b, list + 4) as usize;
    if n > 4096 {
        return Err(Error::invalid("older event texture count"));
    }
    let names = beta_offset(b, word(b, list), n * 12)?;
    let mut texture_names = Vec::with_capacity(n);
    for i in 0..n {
        let at = beta_offset(b, word(b, names + i * 12), 1)?;
        let text = &b[at..b.len().min(at + 64)];
        let end = text
            .iter()
            .position(|c| *c == 0)
            .ok_or_else(|| Error::invalid("unterminated texture name"))?;
        texture_names.push(String::from_utf8_lossy(&text[..end]).trim_end().to_string());
    }
    Ok(BetaEvent {
        scenes,
        texture_names,
    })
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
mod beta_tests {
    use super::*;
    fn put(b: &mut [u8], at: usize, v: u32) {
        b[at..at + 4].copy_from_slice(&v.to_be_bytes());
    }
    /// Synthetic older-layout image: header, one texture, two scenes with
    /// 1 and 2 entities of 32 bytes.
    fn image() -> Vec<u8> {
        let p = |off: usize| BETA_BASE + off as u32;
        let mut b = vec![0u8; 0x200];
        put(&mut b, 0, p(0x40)); // scene table
        put(&mut b, 4, p(0x80)); // texlist
        put(&mut b, 8, 1); // last scene index
        put(&mut b, 0x40 + 4, 1);
        put(&mut b, 0x40, p(0xa0));
        put(&mut b, 0x40 + 28, 4);
        put(&mut b, 0x60, p(0xc0));
        put(&mut b, 0x60 + 4, 2);
        put(&mut b, 0x60 + 28, 4);
        put(&mut b, 0xc0, p(0x100)); // model pointer
        put(&mut b, 0x80, p(0x88));
        put(&mut b, 0x84, 1);
        put(&mut b, 0x88, p(0x180));
        b[0x180..0x186].copy_from_slice(b"logo  ");
        b[0] = 0x81;
        b
    }
    #[test]
    fn reads_the_older_layout() {
        let e = read_beta(&image()).unwrap();
        assert_eq!(e.scenes.len(), 2);
        assert_eq!(e.scenes[0].entities.len(), 1);
        assert_eq!(e.scenes[1].entities.len(), 2);
        assert_eq!(e.scenes[1].entities[0].model_offset, Some(0x100));
        assert_eq!(e.scenes[1].frames, 4);
        assert_eq!(e.texture_names, ["logo"]);
    }
    #[test]
    fn older_layout_bounds_never_panic() {
        let b = image();
        for end in 0..b.len() {
            let _ = read_beta(&b[..end]);
        }
        let mut far = b.clone();
        put(&mut far, 0x60 + 4, 100_000);
        assert!(read_beta(&far).is_err());
        assert!(read(&b).is_err()); // the retail reader still refuses it
    }
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
