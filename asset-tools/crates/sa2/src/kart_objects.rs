//! Kart SET dispatch records and standalone native collision models.
use crate::{city_land, duel_platform::Platform, manifest, object_table, set, Error, Result};
use serde::Serialize;
const SETS: &[(&str, &str)] = &[
    (
        "setCartBase.bin",
        "71a50fb29814ecfcc9d3efcffd7693e667c631ba2356f67980a55f3f52d5616c",
    ),
    (
        "setCartMini1.bin",
        "981f2931b15b998aa9d739cb0fbd6dcdcc417da600c150c7527109ddbd89f9ae",
    ),
    (
        "setCartMini2.bin",
        "f86aece2db49560cf935a2f88a2ba18b355d7143297f3bdad1bc50df51be80a9",
    ),
    (
        "setCartMini3.bin",
        "52bfc4506fbb2c1d74ca4afcd4565aca52ca211c21c990322bc2b3599cf92513",
    ),
    (
        "setCartRouge.bin",
        "46938046069128d919c4aa39a79d442142f5ae0593bd0559605f0ea5bc52c39e",
    ),
    (
        "setCartRouge_hd.bin",
        "0bd162aa1658a3801f1c0c6fabfed5995308ac9baa7d82ab4afd99c9c097ee10",
    ),
    (
        "setCartTails.bin",
        "16e8fed06995b564022c7621658fc8052f12103ebd1c4f162e74ab604d9aa4a5",
    ),
    (
        "setCartTails_hd.bin",
        "2062821e213e1260d00a6c058ebaf90e40871432f14d0d0437cab13b12744b0b",
    ),
];
#[derive(Serialize)]
pub struct CollisionModel {
    pub name: &'static str,
    pub source_rva: u32,
    pub model: Platform,
}
#[derive(Serialize)]
pub struct KartObjects {
    pub set_file: &'static str,
    pub table_rva: u32,
    pub definitions: Vec<object_table::ObjectDefinition>,
    /// Ordered SET records; object_id indexes definitions, not collision_models.
    pub placements: Vec<set::Placement>,
    pub collision_models: Vec<CollisionModel>,
}
pub fn read(exe: &[u8], set_bytes: &[u8]) -> Result<KartObjects> {
    let hash = manifest::sha256(set_bytes);
    let set_file = SETS
        .iter()
        .find(|(_, h)| *h == hash)
        .ok_or_else(|| Error::invalid("unknown kart SET revision"))?
        .0;
    let pe = object_table::verify_image(exe)?;
    let table_rva = 0xc0a230;
    let definitions = object_table::read_table(exe, &pe, table_rva)?;
    let placements = set::parse_set(set_bytes)?;
    if placements
        .iter()
        .any(|p| usize::from(p.object_id) >= definitions.len())
    {
        return Err(Error::invalid("unmapped kart object ID"));
    }
    let collision_models = [("JUMPDAI", 0xaf5990), ("CHECKGATE", 0xaf7e48)]
        .into_iter()
        .map(|(name, source_rva)| {
            Ok(CollisionModel {
                name,
                source_rva,
                model: city_land::win32_basic_object(exe, &pe, source_rva)?,
            })
        })
        .collect::<Result<_>>()?;
    Ok(KartObjects {
        set_file,
        table_rva,
        definitions,
        placements,
        collision_models,
    })
}
