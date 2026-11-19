//! Kart jump-panel initialization after its native work allocation succeeds.
use crate::{city_rel::CityRel, manifest, Error, Result};
use serde::{Deserialize, Serialize};
const HASH: &str = "71826645bae2a4064cd5ad4402facaf4da6c5caee0be7e19894ed7ceb2c2ee6c";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskPose {
    pub position: [f32; 3],
    pub yaw: i32,
}
#[derive(Serialize)]
pub struct RampInit {
    pub callback_offset: u32,
    pub template_offset: u32,
    /// Big-endian source words expressed as numbers; attachment word is a section-5 offset.
    pub node_words: [u32; 14],
    pub attachment_section: u8,
    pub first_call_address: u32,
    pub first_call_flags: u32,
    pub second_call_address: u32,
    pub collision_info_section: u8,
    pub collision_info_offset: u32,
    pub collision_info_count: u32,
    pub collision_info_kind: u32,
    pub task_word_28: u32,
}
pub fn initialize(bytes: &[u8], pose: TaskPose) -> Result<RampInit> {
    if manifest::sha256(bytes) != HASH {
        return Err(Error::invalid("unsupported CartD.rel revision"));
    }
    if !pose.position.iter().all(|v| v.is_finite()) {
        return Err(Error::invalid("nonfinite task position"));
    }
    let rel = CityRel::read_identified(bytes, 80)?;
    let err = || Error::invalid("unexpected kart jump-panel relocation");
    // Object list slot 1: callback and independent name pointer.
    let (1, callback_offset) = rel.local_pointer(5, 0x15f0)?.ok_or_else(err)? else {
        return Err(err());
    };
    let (s, n) = rel.local_pointer(5, 0x15f4)?.ok_or_else(err)?;
    if callback_offset != 0xbc98 || !rel.data(s)?[n as usize..].starts_with(b"JUMP PANEL\0") {
        return Err(err());
    }
    let pair = |at: u32| -> Result<(u8, u32)> {
        let hi = rel.reference(1, at + 2).ok_or_else(err)?;
        let lo = rel.reference(1, at + 6).ok_or_else(err)?;
        if hi.module != 80
            || lo.module != 80
            || hi.kind != 6
            || lo.kind != 4
            || hi.section != lo.section
            || hi.addend != lo.addend
        {
            return Err(err());
        }
        Ok((hi.section, hi.addend))
    };
    let imported = |at| -> Result<u32> {
        let r = rel.reference(1, at).ok_or_else(err)?;
        if r.module != 0 || r.kind != 10 {
            return Err(err());
        }
        Ok(r.addend)
    };
    let (s, template_offset) = pair(0xbd08)?;
    let data = rel.data(s)?;
    let words = &data[template_offset as usize..template_offset as usize + 56];
    let mut node_words =
        std::array::from_fn(|i| u32::from_be_bytes(words[i * 4..i * 4 + 4].try_into().unwrap()));
    let (attachment_section, attachment) =
        rel.local_pointer(s, template_offset + 4)?.ok_or_else(err)?;
    node_words[1] = attachment;
    node_words[0] &= !3;
    node_words[2..5].copy_from_slice(&pose.position.map(f32::to_bits));
    node_words[5..8].copy_from_slice(&[0, pose.yaw as u32, 0]);
    let (collision_info_section, collision_info_offset) = pair(0xbdcc)?;
    let (sec, off) = pair(0xbde0)?;
    let task_word_28 = u32::from_be_bytes(
        rel.data(sec)?[off as usize..off as usize + 4]
            .try_into()
            .unwrap(),
    );
    Ok(RampInit {
        callback_offset,
        template_offset,
        node_words,
        attachment_section,
        first_call_address: imported(0xbdc4)?,
        first_call_flags: 0x20000001,
        second_call_address: imported(0xbddc)?,
        collision_info_section,
        collision_info_offset,
        collision_info_count: 1,
        collision_info_kind: 4,
        task_word_28,
    })
}
