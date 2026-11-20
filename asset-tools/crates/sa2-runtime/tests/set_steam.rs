//! Iron Gate's SET data and object list from the Steam install (needs
//! SA2_GAME_PATH; with SA2_STG11D_REL the object list is also compared with
//! the GC disc module's). Nothing from the game is committed.
use sa2_asset_tools::city_rel::CityRel;
use sa2_runtime::objects::{object_list, steam_dol_to_gsne8p, steam_stg11};
use sa2_runtime::set::{clear_entries, join_files, ring_total, set_exec, setup_entries, SET_EXEC};
use sa2_runtime::stage::{SetState, Stg11Layout};
use sa2_runtime::Runtime;
use std::{fs, path::PathBuf};

const LAYOUT: Stg11Layout = Stg11Layout {
    text: 0x8120_0100,
    rodata: 0x8126_0000,
    data: 0x8127_0000,
    bss: 0x8135_0000,
};

#[test]
#[ignore = "requires the installed Steam game (SA2_GAME_PATH)"]
fn iron_gate_set_spawns_from_steam_files() {
    let game = PathBuf::from(std::env::var_os("SA2_GAME_PATH").expect("SA2_GAME_PATH"));
    let gd = game.join("resource/gd_PC");
    let rel_bytes = fs::read(gd.join("stg11D.rel")).unwrap();
    let rel = steam_stg11(&rel_bytes).unwrap();
    let defs = object_list(&rel, LAYOUT, steam_dol_to_gsne8p).unwrap();
    assert_eq!(defs.len(), 92);
    assert_eq!(defs[0].name.trim_end(), "RING");
    assert_eq!(defs[38].exec, 0x8002_5C7C, "MINIMAL is main.dol's");

    if let Some(gc) = std::env::var_os("SA2_STG11D_REL") {
        let gc = fs::read(gc).unwrap();
        let gc = CityRel::read_identified(&gc, 11).unwrap();
        assert_eq!(object_list(&gc, LAYOUT, |a| a).unwrap(), defs);
    }

    let read = |n: &str| fs::read(gd.join(n)).ok();
    let (s, u) = (read("set0011_s.bin"), read("set0011_u.bin"));
    let mut st = SetState::default();
    st.table.rows = join_files(s.as_deref(), u.as_deref()).unwrap();
    st.table.defs = defs;
    let mut rt = Runtime::new();
    clear_entries(&mut rt, &mut st.table, &mut st.ada14);
    setup_entries(&mut rt, &mut st.table, &mut st.ada14);
    let mut g = vec![0u8; 0xF0];
    let rings = ring_total(&st.table, &mut g);
    assert!(rings > 0);
    let first = st.table.rows[0].pos;
    st.table.fallback = Some(first);
    let set = rt.create_elemental_task(2, 0, SET_EXEC, 0).unwrap();
    rt.task_mut(set).unwrap().twp.as_mut().unwrap()[0] = 1;
    set_exec(&mut rt, &mut st, set);
    let spawned = st.table.entries.iter().filter(|e| e.task.is_some()).count();
    eprintln!(
        "rows {} entries {} rings {} spawned near row 0: {}",
        st.table.rows.len(),
        st.ada14,
        rings,
        spawned
    );
    assert!(spawned > 0);
    assert!(spawned < st.table.rows.len());
}
