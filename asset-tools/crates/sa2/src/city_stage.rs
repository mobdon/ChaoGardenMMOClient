//! Mount a known PC City Escape SET against its own executable revision's object list.
use crate::{
    city_replay, manifest,
    object_table::{self, ObjectDefinition},
    set::{self, Placement},
    Error, Result,
};
use serde::Serialize;
use std::collections::BTreeMap;

/// "s" and "u" are independent source layers, not successive stages.
pub const LAYERS: [(&str, &str, bool); 5] = [
    (
        "set0013_s.bin",
        "9e7ab581b5b85dd09a4eac1d5b34afd4785b49f17575cb78e4c3c814cf05ff14",
        false,
    ),
    (
        "set0013_u.bin",
        "96cc77030033fefb76330558a444c87c0e18693294977b60b7d071a64d3dfa32",
        false,
    ),
    (
        "set0013_hd_s.bin",
        "bc585db0ea4ecdf11d69ecd8ef8b6deff48ae9bef8c713669de5746e26bf9b57",
        false,
    ),
    (
        "set0013_2p_s.bin",
        "6a69bdbe649b8a5f0929a9f8d5fab093beabdcdaa4002c4b6dc8a5688457b6af",
        true,
    ),
    (
        "set0013_2p_u.bin",
        "37028d2b7fc37c2ef8492c683d31c910443dc04f512ab27f526038030ab824e2",
        true,
    ),
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CityLayout {
    pub source_layer: &'static str,
    pub source_sha256: &'static str,
    pub executable_sha256: &'static str,
    pub object_list_rva: u32,
    pub two_player_layer: bool,
    pub object_table: Vec<ObjectDefinition>,
    pub placements: Vec<Placement>,
    pub callback_counts: BTreeMap<&'static str, usize>,
    pub unsupported_placements: usize,
}

impl CityLayout {
    pub fn load(set_bytes: &[u8], exe_bytes: &[u8]) -> Result<Self> {
        let hash = manifest::sha256(set_bytes);
        let &(source_layer, source_sha256, two_player_layer) = LAYERS
            .iter()
            .find(|(_, h, _)| *h == hash)
            .ok_or_else(|| Error::invalid("unrecognized City Escape SET revision"))?;
        let image = object_table::verify_image(exe_bytes)?;
        let object_table =
            object_table::read_table(exe_bytes, &image, object_table::CITY_LIST_RVA)?;
        let placements = set::parse_set(set_bytes)?;
        let mut callback_counts = BTreeMap::new();
        let mut unsupported_placements = 0;
        for p in &placements {
            let d = object_table
                .get(usize::from(p.object_id))
                .ok_or_else(|| Error::invalid("placement outside City Escape object list"))?;
            if let Some(name) = city_replay::callback_name(p, d) {
                *callback_counts.entry(name).or_default() += 1;
            } else {
                unsupported_placements += 1;
            }
        }
        Ok(Self {
            source_layer,
            source_sha256,
            executable_sha256: object_table::STEAM_EXE_SHA256,
            object_list_rva: object_table::CITY_LIST_RVA,
            two_player_layer,
            object_table,
            placements,
            callback_counts,
            unsupported_placements,
        })
    }

    pub fn resolve(&self, index: usize) -> Result<(&Placement, &ObjectDefinition)> {
        let p = self
            .placements
            .get(index)
            .ok_or_else(|| Error::invalid("placement index outside loaded SET layer"))?;
        let d = self
            .object_table
            .get(usize::from(p.object_id))
            .ok_or_else(|| Error::invalid("object ID outside loaded table"))?;
        Ok((p, d))
    }
}
