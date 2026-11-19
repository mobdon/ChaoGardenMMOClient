//! Big-endian character MTN archive directory and file-relative Ninja motion data.
use crate::{archive, board_motion, motion::Motion, Error, Result};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Entry {
    pub slot: u16,
    pub parts: u16,
    pub offset: u32,
    pub frames: u32,
    pub flags: u16,
    pub interpolation_and_slots: u16,
}

pub struct Bank {
    data: Vec<u8>,
    pub rows: Vec<Entry>,
}
impl Bank {
    pub fn read(prs: &[u8]) -> Result<Self> {
        let data = archive::decompress_prs(prs, 8 * 1024 * 1024)?;
        let mut rows = Vec::new();
        let mut end = None;
        for (i, r) in data.chunks_exact(8).take(4097).enumerate() {
            let slot = u16::from_be_bytes(r[..2].try_into().unwrap());
            if slot == u16::MAX {
                if r[2..] != [0; 6] {
                    return Err(Error::invalid("malformed MTN terminator"));
                }
                end = Some((i + 1) * 8);
                break;
            }
            let parts = u16::from_be_bytes(r[2..4].try_into().unwrap());
            let offset = u32::from_be_bytes(r[4..].try_into().unwrap());
            if !(1..=1024).contains(&parts) {
                return Err(Error::invalid("invalid MTN slot or part count"));
            }
            let h = data
                .get(offset as usize..)
                .and_then(|b| b.get(..12))
                .ok_or_else(|| Error::invalid("MTN header out of bounds"))?;
            rows.push(Entry {
                slot,
                parts,
                offset,
                frames: u32::from_be_bytes(h[4..8].try_into().unwrap()),
                flags: u16::from_be_bytes(h[8..10].try_into().unwrap()),
                interpolation_and_slots: u16::from_be_bytes(h[10..].try_into().unwrap()),
            });
        }
        let end = end.ok_or_else(|| Error::invalid("missing MTN terminator"))?;
        if rows.iter().any(|r| (r.offset as usize) < end) {
            return Err(Error::invalid("MTN header overlaps directory"));
        }
        Ok(Self { data, rows })
    }
    pub fn motion(&self, slot: u16) -> Result<Motion> {
        let mut matches = self.rows.iter().filter(|r| r.slot == slot);
        let row = matches
            .next()
            .ok_or_else(|| Error::invalid("motion slot absent in supplied bank"))?;
        if matches.next().is_some() {
            return Err(Error::invalid("ambiguous motion slot in supplied bank"));
        }
        if (row.flags, row.interpolation_and_slots) != (3, 2) {
            return Err(Error::invalid(
                "unsupported character motion channel layout",
            ));
        }
        board_motion::bank_motion(&self.data, row.offset, row.parts)
    }
}
/// Sample a Sonic animation state from an explicitly supplied character/common bank.
pub fn sonic_pose(
    prs: &[u8],
    exe: &[u8],
    mdl: &[u8],
    state: usize,
    frame: f64,
) -> Result<serde_json::Value> {
    let bank_name = match crate::manifest::sha256(prs).as_str() {
        "0feaf13cbaec452f52456c15504d2b7eaad7e8f276d8b073b11c2d4d169a9a34" => "sonicmtn",
        "288758a3c3b61f9875367816235bca67d0515f4811ee70ea398844f51c44b153" => "plcommtn",
        _ => return Err(Error::invalid("unsupported Sonic motion bank")),
    };
    let row = crate::character_animations::row(exe, "Sonic", state)?;
    let slot = u16::try_from(row.motion).map_err(|_| Error::invalid("negative motion slot"))?;
    let model = u16::try_from(row.model).map_err(|_| Error::invalid("negative model slot"))?;
    let motion = Bank::read(prs)?.motion(slot)?;
    if !frame.is_finite() || frame < 0. || frame >= f64::from(motion.frame_count) {
        return Err(Error::invalid("frame outside stored character motion"));
    }
    let rig = crate::city_rider::rig(mdl, model)?;
    let pose = crate::pose::evaluate_model_pose(&rig, Some(&motion), frame)?;
    Ok(
        serde_json::json!({"state":state,"bank":bank_name,"row":row,"motionOffset":motion.offset,"pose":pose}),
    )
}
