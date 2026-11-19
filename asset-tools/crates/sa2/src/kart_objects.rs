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

/// Installed G RING SET tasks enter 0xbab0 in mode zero. Each creates individual
/// CART RING tasks via 0xa7b8 and then releases its SET entry.
pub fn ring_groups(module: &[u8], exe: &[u8], set_bytes: &[u8]) -> Result<serde_json::Value> {
    use serde_json::json;
    crate::kart_progress::verify(module)?;
    let rel = crate::city_rel::CityRel::read_identified(module, 80)?;
    let objects = read(exe, set_bytes)?;
    let invalid = || Error::invalid("mismatched Kart ring task registration");
    let d = rel.data(5)?;
    for (id, row, text, name) in [
        (8, 0x1660, 0xa824, "CART RING"),
        (12, 0x16a0, 0xbab0, "G RING"),
    ] {
        if rel.local_pointer(5, row)? != Some((1, text)) {
            return Err(invalid());
        }
        let Some((5, offset)) = rel.local_pointer(5, row + 4)? else {
            return Err(invalid());
        };
        let label =
            std::ffi::CStr::from_bytes_until_nul(&d[offset as usize..]).map_err(|_| invalid())?;
        if label.to_bytes() != name.as_bytes()
            || objects.definitions.get(id).map(|e| e.name.as_str()) != Some(name)
        {
            return Err(invalid());
        }
    }
    let mut groups = Vec::new();
    let mut total = 0usize;
    for (index, p) in objects.placements.iter().enumerate() {
        if p.object_id != 12 {
            continue;
        }
        let mut position = p.position;
        let count = i32::from(p.rotation[0] as i16);
        let mut children = Vec::new();
        for _ in 0..count {
            children.push(position);
            for (coord, delta) in position.iter_mut().zip(p.parameters) {
                *coord += delta;
            }
        }
        total += children.len();
        groups.push(json!({"placement":index,"setOffset":32+32*index,"origin":p.position,"step":p.parameters,
            "count":count,"positions":children}));
    }
    Ok(
        json!({"setFile":objects.set_file,"setSha256":manifest::sha256(set_bytes),
        "groupCallback":0xbab0,"child":{"callback":0xa824,"mode":0,"loadArgs":[2,2],"nameOffset":0x2ed418},
        "parentRelease":0x80065148u32,"ringCount":total,"groups":groups}),
    )
}
