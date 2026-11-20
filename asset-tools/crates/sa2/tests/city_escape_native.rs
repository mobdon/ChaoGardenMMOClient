//! City Escape's native gameplay inputs on the installed Steam data.
//! Run with `--ignored` and SA2_GAME_PATH and SA2_PC_CONFIG set.
use sa2_asset_tools::{
    city_escape_native::{CityEscape, MODULE_FILE},
    fan_stage::Campaign,
};
use std::{fs, path::PathBuf};

#[test]
#[ignore = "requires installed game and SA Tools 786 PC_SA2 maps"]
fn installed_city_escape_inputs() {
    let game = PathBuf::from(std::env::var_os("SA2_GAME_PATH").expect("SA2_GAME_PATH"));
    let cfg = PathBuf::from(std::env::var_os("SA2_PC_CONFIG").expect("SA2_PC_CONFIG"));
    let gd = game.join("resource/gd_PC");
    let read = |n: &str| fs::read_to_string(cfg.join(n)).unwrap();
    let exe = fs::read(game.join("sonic2app.exe")).unwrap();
    let module = fs::read(gd.join(MODULE_FILE)).unwrap();
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
    let stage = campaign.mount("Hero Story Sequence", 1, load).unwrap();
    let c = CityEscape::read(&stage, &exe, &module).unwrap();

    assert_eq!((c.character, c.physics_slot), (0, 0));
    assert_eq!(
        (c.start_position, c.start_yaw),
        ([0.0, 300.0, 340.0], 0x4000)
    );
    let p = &c.physics;
    assert_eq!((p.jump2_timer, p.jmp_y_spd, p.run_accel), (60, 1.66, 0.05));
    assert_eq!(
        (p.max_x_spd, p.dash_speed, p.weight, p.rad),
        (3.0, 5.09, 0.08, 4.0)
    );

    // 12 grind rails, all path type 1 with the shared rail callback.
    assert_eq!(c.rails.len(), 12);
    let points: usize = c.rails.iter().map(|r| r.points.len()).sum();
    assert_eq!(points, 147);

    // Two goal placements; only the Rotation.x 0 one reaches contact.
    assert_eq!(c.goals.len(), 2);
    let live: Vec<_> = c.contact_goals().collect();
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].row.position[1], -22139.0);

    assert_eq!(c.springs.len(), 10);
    assert_eq!(c.rows["SAVEPOINT"].len(), 3);
    assert_eq!(c.dash_panels.len(), 82);
    assert_eq!(c.dash_panel_program.update, 0x22c8c);
    assert_eq!(c.rows["GREEN_B"].len(), 18);
    assert!(!c.bush_solids.is_empty());
    assert!(!c.death_zones.is_empty());
}
