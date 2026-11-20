//! Iron Gate's object list, read from the Steam install's `stg11D.rel`.
//!
//! The Steam REL is the GC module linked against a different main.dol build
//! (ENGINE-MAP.md section 0): its object list (.data+0x5D4FC: count, then a
//! pointer to 0x10-byte rows) is the GC one, and its exec pointers are either
//! module-local (made native with [`Stg11Layout`]) or main.dol addresses of
//! that other build, moved to GSNE8P with [`steam_dol_to_gsne8p`].
use crate::set::ObjectDef;
use crate::stage::Stg11Layout;
use sa2_asset_tools::{city_rel::CityRel, manifest, stage_land};

/// stg11D .data offset of the object-list header `fn_8006638C` receives.
const LIST: u32 = 0x5D4FC;
const DATA: u8 = 5;
const TEXT: u8 = 1;

/// Steam-build main.dol address to GSNE8P (ENGINE-MAP.md section 0, code).
pub fn steam_dol_to_gsne8p(a: u32) -> u32 {
    const RANGES: [(u32, u32); 6] = [
        (0x8000_5000, 0),
        (0x8001_ADD4, 0x84),
        (0x8002_2BC4, 0x248),
        (0x8002_7A80, 0x268),
        (0x800D_DCAC, 0x2DC),
        (0x800E_CD78, 0x348),
    ];
    let add = RANGES
        .iter()
        .rev()
        .find(|(from, _)| a >= *from)
        .map_or(0, |r| r.1);
    a.wrapping_add(add)
}

/// Read the Steam `stg11D.rel` (checked against the catalogued digest).
pub fn steam_stg11(bytes: &[u8]) -> Result<CityRel<'_>, String> {
    let info = stage_land::catalog()
        .iter()
        .find(|i| i.filename == "stg11D.rel")
        .ok_or("stg11D.rel is not catalogued")?;
    if bytes.len() != info.size || manifest::sha256(bytes) != info.sha256 {
        return Err("unsupported stg11D.rel revision".into());
    }
    CityRel::read_identified(bytes, info.module).map_err(|e| e.to_string())
}

/// The object list of a stg11D module (`main_dol` maps its main.dol
/// pointers to GSNE8P: [`steam_dol_to_gsne8p`] for the Steam REL, identity
/// for the GC disc REL).
pub fn object_list(
    rel: &CityRel<'_>,
    layout: Stg11Layout,
    main_dol: fn(u32) -> u32,
) -> Result<Vec<ObjectDef>, String> {
    let data = rel.data(DATA).map_err(|e| e.to_string())?;
    let word = |at: u32| -> Result<[u8; 4], String> {
        data.get(at as usize..at as usize + 4)
            .and_then(|w| w.try_into().ok())
            .ok_or_else(|| format!("object list read past .data at {at:#x}"))
    };
    let count = i16::from_be_bytes([word(LIST)?[0], word(LIST)?[1]]);
    let (_, rows) = rel
        .local_pointer(DATA, LIST + 4)
        .map_err(|e| e.to_string())?
        .ok_or("object list has no rows")?;
    let mut out = Vec::new();
    for i in 0..count.max(0) as u32 {
        let at = rows + 0x10 * i;
        let [im, level, f0, f1] = word(at)?;
        let exec = match rel.reference(DATA, at + 8) {
            None => 0,
            Some(r) if r.module == rel.module_id() && r.section == TEXT && r.kind == 1 => {
                layout.text + r.addend
            }
            Some(r) if r.module == 0 && r.kind == 1 => main_dol(r.addend),
            Some(_) => return Err(format!("object {i}: unexpected exec relocation")),
        };
        let name = match rel.local_pointer(DATA, at + 12) {
            Ok(Some((s, off))) => rel
                .data(s)
                .ok()
                .and_then(|d| d.get(off as usize..))
                .map(|d| {
                    let end = d.iter().position(|&b| b == 0).unwrap_or(d.len()).min(64);
                    d[..end].iter().map(|&b| b as char).collect()
                })
                .unwrap_or_default(),
            _ => String::new(),
        };
        out.push(ObjectDef {
            im,
            level,
            flags: u16::from_be_bytes([f0, f1]),
            distance: f32::from_be_bytes(word(at + 4)?),
            exec,
            name,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steam_dol_ranges() {
        // MINIMAL's exec in both builds' stg11D object lists.
        assert_eq!(steam_dol_to_gsne8p(0x8002_5A34), 0x8002_5C7C);
        assert_eq!(steam_dol_to_gsne8p(0x8007_1BA4), 0x8007_1E0C);
        assert_eq!(steam_dol_to_gsne8p(0x8000_6000), 0x8000_6000);
    }
}
