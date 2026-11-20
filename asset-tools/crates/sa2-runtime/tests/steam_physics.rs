//! The Steam PhysicsArray, converted to main.dol's row layout, against the
//! GameCube rows the load oracle reads from main.dol. Rows that differ are
//! reported (the Steam data is what the port uses). Skips without
//! SA2_GAME_PATH and SA2_MAIN_DOL.
mod common;
use sa2_runtime::player::steam_physics_rows;

#[test]
fn steam_rows_against_main_dol() {
    let Some(game) = std::env::var_os("SA2_GAME_PATH") else {
        eprintln!("skipped: SA2_GAME_PATH is not set");
        return;
    };
    let Some(out) = common::oracle("load_ppc.py") else {
        return;
    };
    let exe = std::fs::read(std::path::Path::new(&game).join("sonic2app.exe")).expect("exe");
    let steam = steam_physics_rows(&exe).expect("rows");
    let gc = out["physics"].as_array().expect("rows");
    let mut same = 0;
    for (row, g) in gc.iter().enumerate() {
        let hex: String = steam[row].iter().map(|b| format!("{b:02x}")).collect();
        if hex == g.as_str().expect("hex") {
            same += 1;
        } else {
            eprintln!("row {row}: Steam and GameCube PhysicsArray differ");
        }
    }
    eprintln!("{same} of {} rows identical", gc.len());
    assert_eq!(steam.len(), 19);
}
