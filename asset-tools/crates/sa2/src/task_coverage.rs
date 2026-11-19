//! Inventory of placed native task entry points across the three campaign routes.
use crate::{city_replay, fan_stage::Campaign, Result};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub names: BTreeSet<String>,
    pub levels: BTreeSet<u16>,
    pub distinct_placements: usize,
    pub with_partial_adapter: usize,
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Coverage {
    pub story_dispatches: usize,
    pub layer_mounts: usize,
    pub distinct_layers: usize,
    pub placement_visits: usize,
    pub distinct_placements: usize,
    pub unresolved_placements: usize,
    pub without_layers: Vec<crate::fan_stage::LevelDispatch>,
    pub missing_files: BTreeSet<String>,
    pub by_function_va: BTreeMap<u32, Task>,
}
/// Count a SET row only once per native object table and filename across repeated story visits.
/// This measures placed task identities, not task executions or gameplay completion.
pub fn read(
    campaign: &Campaign,
    mut load: impl FnMut(&str) -> Result<Option<Vec<u8>>>,
) -> Result<Coverage> {
    let mut c = Coverage::default();
    let mut seen = BTreeSet::new();
    for d in campaign.levels() {
        let stage = campaign.mount(d.sequence, d.action_index, |name| {
            if !std::path::Path::new(name)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("bin"))
            {
                return Ok(None);
            }
            let bytes = load(name)?;
            if bytes.is_none() {
                c.missing_files.insert(name.into());
            }
            Ok(bytes)
        })?;
        c.story_dispatches += 1;
        if stage.layers.is_empty() {
            c.without_layers.push(d.clone());
        }
        for layer in &stage.layers {
            c.layer_mounts += 1;
            c.placement_visits += layer.placements.len();
            let fresh = seen.insert((stage.object_table.map(|t| t.head_rva), layer.filename));
            for p in &layer.placements {
                if fresh {
                    c.distinct_placements += 1;
                }
                let Some(def) = p
                    .definition_index
                    .and_then(|i| stage.object_table?.entries.get(i))
                else {
                    c.unresolved_placements += usize::from(fresh);
                    continue;
                };
                let task = c.by_function_va.entry(def.function_va).or_default();
                task.names.insert(def.name.clone());
                task.levels.insert(d.level);
                if fresh {
                    task.distinct_placements += 1;
                    if city_replay::mounted_callback(d.level, &p.placement, def).is_some() {
                        task.with_partial_adapter += 1;
                    }
                }
            }
        }
    }
    c.distinct_layers = seen.len();
    Ok(c)
}
