//! Print a mounted campaign stage's SET inventory and start pose.
//!
//!     cargo run --example stage_inventory -- GAME_DIR PC_SA2_CONFIG "Hero Story Sequence" 9 [NAME...]
use sa2_asset_tools::fan_stage::Campaign;
use std::{collections::BTreeMap, fs, path::PathBuf};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let game = PathBuf::from(&a[1]);
    let cfg = PathBuf::from(&a[2]);
    let gd = game.join("resource/gd_PC");
    let exe = fs::read(game.join("sonic2app.exe")).unwrap();
    let read = |n: &str| fs::read_to_string(cfg.join(n)).unwrap();
    let campaign = Campaign::new(
        &exe,
        &read("ADVERTISE.ini"),
        &read("sa2lvl.ini"),
        &read("ObjectLists.ini"),
    )
    .unwrap();
    let load = |n: &str| -> sa2_asset_tools::Result<Option<Vec<u8>>> {
        let p = gd.join(n);
        Ok(p.is_file().then(|| fs::read(p).unwrap()))
    };
    let stage = campaign.mount(&a[3], a[4].parse().unwrap(), load).unwrap();
    println!(
        "level {} character {} start {:?} end {:?}",
        stage.dispatch.level,
        stage.dispatch.character,
        stage.entry_pose.as_ref().map(|e| (e.position, e.yaw)),
        stage.end_pose.as_ref().map(|e| e.position)
    );
    let objects = stage.object_table.unwrap();
    let mut counts: BTreeMap<(String, u32), usize> = BTreeMap::new();
    let show: Vec<&str> = a[5..].iter().map(String::as_str).collect();
    for (li, layer) in stage.layers.iter().enumerate() {
        for p in &layer.placements {
            let Some(def) = p.definition_index.and_then(|i| objects.entries.get(i)) else {
                continue;
            };
            let name = def.name.trim_end().to_string();
            if show.contains(&name.as_str()) {
                let pl = &p.placement;
                println!(
                    "{li}/{} {name} pos {:?} rot {:?} par {:?}",
                    p.placement_index, pl.position, pl.rotation, pl.parameters
                );
            }
            *counts.entry((name, def.function_va)).or_default() += 1;
        }
    }
    for ((n, va), c) in counts {
        println!("{n:12} {va:#x} {c}");
    }
}
