//! Match an explicitly selected SA Tools level section to its PC object table.
use crate::{
    object_table::{read_table, verify_image, ObjectDefinition},
    pe::PeImage,
    set::{parse_set, Placement},
    Error, Result,
};
use serde::Serialize;
use std::collections::BTreeMap;

type Section = BTreeMap<String, String>;
pub(crate) fn sections(text: &str, keys: &[&str]) -> Result<BTreeMap<String, Section>> {
    if text.len() > 2 * 1024 * 1024 {
        return Err(Error::invalid("level configuration byte limit"));
    }
    let mut sections = BTreeMap::new();
    let mut current = None;
    for line in text.trim_start_matches('\u{feff}').lines().map(str::trim) {
        if line.is_empty() || line.starts_with([';', '#']) {
            continue;
        }
        if line.starts_with('[') {
            let name = line
                .strip_prefix('[')
                .and_then(|s| s.strip_suffix(']'))
                .filter(|s| !s.is_empty() && s.len() <= 512)
                .ok_or_else(|| Error::invalid("invalid INI section"))?;
            if sections.len() >= 4096 || sections.insert(name.to_string(), Section::new()).is_some()
            {
                return Err(Error::invalid("duplicate or excessive INI section"));
            }
            current = Some(name);
        } else if let Some((name, value)) = line.split_once('=') {
            let key = name.trim().to_ascii_lowercase();
            if let Some(section) = current.and_then(|name| sections.get_mut(name)) {
                if keys.contains(&key.as_str()) {
                    let value = value.trim();
                    if value.is_empty()
                        || value.len() > 1024
                        || section.insert(key, value.to_owned()).is_some()
                    {
                        return Err(Error::invalid("empty, duplicate or excessive INI value"));
                    }
                }
            }
        }
    }
    Ok(sections)
}

fn list_key(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

struct Mapping {
    section: String,
    rva: u32,
}

pub struct Catalog<'a> {
    exe: &'a [u8],
    image: PeImage,
    levels: BTreeMap<String, Section>,
    mappings: BTreeMap<String, Mapping>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundPlacement {
    pub placement_index: usize,
    pub source_offset: usize,
    pub placement: Placement,
    /// Index into this report's object_definitions. Absent when no matching record exists.
    pub definition_index: Option<usize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundSet {
    pub level_section: String,
    pub level_id: String,
    pub set_name: Option<String>,
    pub level_id_sections: Vec<String>,
    pub configured_object_list: Option<String>,
    pub object_list_section: Option<String>,
    pub table_rva: Option<u32>,
    pub sections_sharing_table: Vec<String>,
    pub table_gap: Option<String>,
    pub object_definitions: Vec<ObjectDefinition>,
    pub placements: Vec<BoundPlacement>,
    pub unresolved_id_counts: BTreeMap<u16, usize>,
}

impl<'a> Catalog<'a> {
    pub fn new(exe: &'a [u8], level_config: &str, object_config: &str) -> Result<Self> {
        let image = verify_image(exe)?;
        let levels = sections(level_config, &["levelid", "objectlist", "setname"])?;
        let mut mappings = BTreeMap::new();
        for (section, fields) in sections(object_config, &["type", "filename", "address"])? {
            if !fields
                .get("type")
                .is_some_and(|v| v.eq_ignore_ascii_case("objlist"))
            {
                continue;
            }
            let file = fields
                .get("filename")
                .ok_or_else(|| Error::invalid("objlist missing filename"))?;
            let address = fields
                .get("address")
                .ok_or_else(|| Error::invalid("objlist missing address"))?;
            let rva = u32::from_str_radix(address, 16)
                .map_err(|_| Error::invalid("invalid objlist RVA"))?;
            if mappings
                .insert(list_key(file), Mapping { section, rva })
                .is_some()
            {
                return Err(Error::invalid("ambiguous objlist filename"));
            }
        }
        if mappings.is_empty() {
            return Err(Error::invalid("no object list mappings"));
        }
        Ok(Self {
            exe,
            image,
            levels,
            mappings,
        })
    }

    pub fn bind(&self, set: &[u8], level_section: &str) -> Result<BoundSet> {
        let section = self.levels.get(level_section).ok_or_else(|| {
            Error::invalid("unknown level section; use its full literal INI name")
        })?;
        let id = section
            .get("levelid")
            .ok_or_else(|| Error::invalid("selected section has no LevelID"))?;
        if id.len() != 4 || !id.bytes().all(|b| b.is_ascii_digit()) {
            return Err(Error::invalid("expected four decimal LevelID digits"));
        }
        let path = section.get("objectlist");
        let mapping = path.and_then(|p| self.mappings.get(&list_key(p)));
        let (defs, gap) = match (path, mapping) {
            (_, Some(m)) => (read_table(self.exe, &self.image, m.rva)?, None),
            (Some(_), None) => (
                vec![],
                Some("ObjectList has no filename mapping in ObjectLists.ini".into()),
            ),
            (None, None) => (
                vec![],
                Some("selected level section declares no ObjectList".into()),
            ),
        };
        let mut unresolved_id_counts = BTreeMap::new();
        let mut placements = Vec::new();
        for (placement_index, placement) in parse_set(set)?.into_iter().enumerate() {
            let index = usize::from(placement.object_id);
            let definition_index = (index < defs.len()).then_some(index);
            if definition_index.is_none() {
                *unresolved_id_counts.entry(placement.object_id).or_default() += 1;
            }
            placements.push(BoundPlacement {
                placement_index,
                source_offset: 32 + 32 * placement_index,
                placement,
                definition_index,
            });
        }
        let mut level_id_sections = Vec::new();
        let mut sections_sharing_table = Vec::new();
        for (name, fields) in &self.levels {
            if fields.get("levelid") == Some(id) {
                level_id_sections.push(name.clone());
            }
            if let Some(selected) = mapping {
                if fields
                    .get("objectlist")
                    .and_then(|p| self.mappings.get(&list_key(p)))
                    .is_some_and(|m| m.rva == selected.rva)
                {
                    sections_sharing_table.push(name.clone());
                }
            }
        }
        Ok(BoundSet {
            level_section: level_section.to_owned(),
            level_id: id.clone(),
            set_name: section.get("setname").cloned(),
            level_id_sections,
            configured_object_list: path.cloned(),
            object_list_section: mapping.map(|m| m.section.clone()),
            table_rva: mapping.map(|m| m.rva),
            sections_sharing_table,
            table_gap: gap,
            object_definitions: defs,
            placements,
            unresolved_id_counts,
        })
    }
}

pub fn bind(
    set: &[u8],
    exe: &[u8],
    level_config: &str,
    object_config: &str,
    level_section: &str,
) -> Result<BoundSet> {
    Catalog::new(exe, level_config, object_config)?.bind(set, level_section)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_ambiguous_keys_and_sections() {
        for invalid in ["[a]\nLevelID=0046\nlevelid=0047", "[a]\n[a]", "[incomplete"] {
            assert!(sections(invalid, &["levelid"]).is_err());
        }
        let valid = sections(
            "\u{feff}; note\n[a]\n LevelID = 0090\n; ObjectList=wrong\n[b]\nlevelid=0090",
            &["levelid", "objectlist"],
        )
        .unwrap();
        assert_eq!(valid["a"]["levelid"], "0090");
        assert!(!valid["a"].contains_key("objectlist"));
    }
}
