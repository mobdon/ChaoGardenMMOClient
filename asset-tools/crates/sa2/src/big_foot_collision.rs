//! Per-shape local centers and the temporary task pose submitted to CCL_Entry.
use crate::{city_rel::CityRel, Error, Result};
use serde::Deserialize;
use serde_json::{json, Value};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Tick {
    mode: i8,
    flags: u16,
    reference_y: f32,
    position: [f32; 3],
    angles: [i32; 3],
    /// Native runtime vectors at section 6 offsets 180, 192 and 204.
    anchors: [[f32; 3]; 3],
    /// Engine sine returns for motion-work angle fields 0x84, 0x8c and 0x88.
    sine_results: Option<[f32; 3]>,
}
fn word(data: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_be_bytes(
        data.get(at..at + 4)
            .ok_or_else(|| Error::invalid("truncated Big Foot collision row"))?
            .try_into()
            .unwrap(),
    ))
}
fn vec3(data: &[u8], at: usize) -> Result<[f32; 3]> {
    Ok([
        f32::from_bits(word(data, at)?),
        f32::from_bits(word(data, at + 4)?),
        f32::from_bits(word(data, at + 8)?),
    ])
}
pub(super) fn run(rel: &CityRel<'_>, ticks: Vec<Tick>) -> Result<Value> {
    if ticks.len() > 2048 {
        return Err(Error::invalid("too many boss collision updates"));
    }
    let data = rel.data(5)?;
    let constants = rel.data(4)?;
    let zero = vec3(constants, 592)?;
    let extra = vec3(constants, 604)?;
    let mut bindings = vec![];
    let mut shapes = vec![];
    for index in 0..11 {
        let at = 528 + index * 20;
        let (Some((5, shape)), Some((6, anchor))) =
            (rel.local_pointer(5, at)?, rel.local_pointer(5, at + 4)?)
        else {
            return Err(Error::invalid("invalid Big Foot collision binding"));
        };
        let slot = match anchor {
            180 => 0,
            192 => 1,
            204 => 2,
            _ => return Err(Error::invalid("unknown Big Foot collision anchor")),
        };
        let off = shape as usize;
        let parameters = (0..4)
            .map(|j| word(data, off + 20 + j * 4).map(f32::from_bits))
            .collect::<Result<Vec<_>>>()?;
        shapes.push(json!({"index":index,"descriptorOffset":shape,"headerBytes":data[off..off+4],"attributes":word(data,off+4)?,"originalCenter":vec3(data,off+8)?,"parameters":parameters,"angles":[word(data,off+36)? as i32,word(data,off+40)? as i32,word(data,off+44)? as i32],"anchorOffset":anchor}));
        bindings.push((slot, vec3(data, at as usize + 8)?));
    }
    let number = |at| word(constants, at).map(f32::from_bits);
    let mut frames = vec![];
    for t in ticks {
        if !t
            .position
            .iter()
            .chain(t.anchors.iter().flatten())
            .chain([&t.reference_y])
            .all(|v| v.is_finite())
            || (t.flags & 1 == 0) != t.sine_results.is_some()
        {
            return Err(Error::invalid("invalid boss collision observations"));
        }
        let shifted = t.mode == 13 && t.reference_y <= number(616)?;
        let mut centers = Vec::new();
        for &(slot, offset) in &bindings {
            let mut c = zero;
            for i in 0..3 {
                c[i] += t.anchors[slot][i];
                c[i] += offset[i];
                if shifted {
                    c[i] += extra[i];
                }
            }
            if !c.iter().all(|n| n.is_finite()) {
                return Err(Error::invalid("collision center overflow"));
            }
            centers.push(c);
        }
        let mut pos = t.position;
        let mut ang = t.angles;
        if let Some([sx, sz, sy]) = t.sine_results {
            if ![sx, sz, sy].iter().all(|v| (-1.0..=1.0).contains(v)) {
                return Err(Error::invalid("invalid native sine result"));
            }
            ang[0] = ang[0].wrapping_add(((number(620)? * sx) * number(524)?) as i32);
            ang[2] = ang[2].wrapping_add(((number(624)? * sz) * number(524)?) as i32);
            pos[1] += number(628)? * sy;
        }
        frames.push(json!({"centers":centers,"registration":{"position":pos,"angles":ang},"afterRegistration":{"position":t.position,"angles":t.angles}}));
    }
    Ok(json!({"descriptors":shapes,"frames":frames}))
}
