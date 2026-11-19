use crate::{Error, Result};
/// A big-endian CCL_INFO as supplied by the stage to CCL_Init. Do not
/// interpret the attribute bits or overwrite the stage's source record.
#[derive(Debug, serde::Serialize)]
pub struct CollisionRecord {
    pub kind: i8,
    pub form: u8,
    pub push: i8,
    pub damage: i8,
    pub attr: u32,
    pub center: [f32; 3],
    pub parameters: [f32; 4],
    pub angles: [i32; 3],
}
pub(crate) fn collision_record(data: &[u8], offset: u32) -> Result<CollisionRecord> {
    let raw = data
        .get(offset as usize..)
        .and_then(|v| v.get(..48))
        .ok_or_else(|| Error::invalid("truncated stage collision record"))?;
    let word = |i: usize| u32::from_be_bytes(raw[i..i + 4].try_into().unwrap());
    let f = |i| f32::from_bits(word(i));
    Ok(CollisionRecord {
        kind: raw[0] as i8,
        form: raw[1],
        push: raw[2] as i8,
        damage: raw[3] as i8,
        attr: word(4),
        center: std::array::from_fn(|i| f(8 + 4 * i)),
        parameters: std::array::from_fn(|i| f(20 + 4 * i)),
        angles: std::array::from_fn(|i| word(36 + 4 * i) as i32),
    })
}
