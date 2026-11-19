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
    sample_character(prs, exe, mdl, state, frame, Some("Sonic"))
}

pub fn character_pose(
    prs: &[u8],
    exe: &[u8],
    mdl: &[u8],
    state: usize,
    frame: f64,
) -> Result<serde_json::Value> {
    sample_character(prs, exe, mdl, state, frame, None)
}

// Character, motion bank, model PRS digest, motion PRS digest.
const SOURCES: &[(&str, &str, &str, &str)] = &[
    (
        "Sonic",
        "sonicmtn",
        "d8f90e6c724e709b8d3431e725a5db1d1a096bdf5118d2ecca9ad356e7862d99",
        "0feaf13cbaec452f52456c15504d2b7eaad7e8f276d8b073b11c2d4d169a9a34",
    ),
    (
        "Shadow",
        "teriosmtn",
        "d9c512a137239fbd354d3d9384c834460d64458037ccb2aefd05b51bfb2ef36c",
        "a8130a6e1349a799e6568ab8fc274a3c38a144a18b6f1fa7ce1e2823adb509bc",
    ),
    (
        "Knuckles",
        "knuckmtn",
        "33726ca2f5d59edefc9c6faefb9daa12c0caba35944033d9aed751860970a0f8",
        "13ecc493bc6878f5ec277ee44529cfe5db08bb7a0a5c838265c7b4805663640f",
    ),
    (
        "Rouge",
        "rougemtn",
        "788e77425766641a7ddb40aa10c2e57c07f074d04f0e7d84e51487073ba93277",
        "ad8d9bb4620b789fc47da766edfb36237ae5546e8f461683938a86621dd60826",
    ),
    (
        "MechEggman",
        "ewalkmtn",
        "074ea70145c5cc5aeac62db56223dbe6d814fe25231b05e0e462f1a003ba1c29",
        "e39db8799536d3233956e5861ef043cced1e9cc4289b5f33383ae7282d3d406c",
    ),
    (
        "MechTails",
        "twalkmtn",
        "e71cce13ed6d4652c6648b9e3eec3e957038e27fabedf964bf308637c7350b6f",
        "02889f50da798c833d739a4815e5de68c1685e3e825fe5ce20ab84fc9e246f58",
    ),
    (
        "Tails",
        "milesmtn",
        "3d546dd0a67b4d1c11644d28b3506525911c277fa8ccc0e9e7c16cd7c8975e80",
        "8e712f113498a87002b27cc579c3d4dbcf3265ca25982960c57b9dd0d606ba45",
    ),
    (
        "Eggman",
        "eggmtn",
        "ad3fb8c251fe5deb2b55cd680c6bb11bfd75a9fd10cb6a9623b8917ddc5cfe31",
        "c5a2868cec690b5edf3cc05240e659ede2734cc59a1d5fcdae1baf2c2b5fd031",
    ),
    (
        "SuperSonic",
        "ssonicmtn",
        "55a84c0fe8bdeb019bebbd21bc379d1cd5ef11a73db2294b45fb8e6472f910ba",
        "05e0c039e7b026ad94b8af73195ffe96e7141d794bfdfed71b2873a5765cf924",
    ),
    (
        "SuperShadow",
        "sshadowmtn",
        "7eb19b01972165404f5cec75da6ad5dbe9f687b1d213b6c50dafbbc2381972c6",
        "d6b69c1916c0967f00df41fe463e480e7123dff98a72a5187f544490b80c2f89",
    ),
];

fn sample_character(
    prs: &[u8],
    exe: &[u8],
    mdl: &[u8],
    state: usize,
    frame: f64,
    required: Option<&str>,
) -> Result<serde_json::Value> {
    let model_hash = crate::manifest::sha256(mdl);
    let &(character, own_bank, _, motion_hash) = SOURCES
        .iter()
        .find(|s| s.2 == model_hash)
        .ok_or_else(|| Error::invalid("unsupported character model archive revision"))?;
    if required.is_some_and(|c| c != character) {
        return Err(Error::invalid("unexpected character model"));
    }
    let hash = crate::manifest::sha256(prs);
    let bank_name = if hash == motion_hash {
        own_bank
    } else if hash == "288758a3c3b61f9875367816235bca67d0515f4811ee70ea398844f51c44b153" {
        "plcommtn"
    } else {
        return Err(Error::invalid("motion bank does not match character model"));
    };
    let row = crate::character_animations::row(exe, character, state)?;
    let slot = u16::try_from(row.motion).map_err(|_| Error::invalid("negative motion slot"))?;
    let model = u16::try_from(row.model).map_err(|_| Error::invalid("negative model slot"))?;
    let motion = Bank::read(prs)?.motion(slot)?;
    if !frame.is_finite() || frame < 0. || frame >= f64::from(motion.frame_count) {
        return Err(Error::invalid("frame outside stored character motion"));
    }
    let rig = crate::city_rider::archive_rig(mdl, model, "character-rig")?;
    let pose = crate::pose::evaluate_model_pose(&rig, Some(&motion), frame)?;
    Ok(
        serde_json::json!({"character":character,"state":state,"bank":bank_name,"row":row,"motionOffset":motion.offset,"pose":pose}),
    )
}
