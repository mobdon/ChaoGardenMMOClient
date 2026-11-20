//! Runs every reader of the Steam executable and Data_DLL tables over the
//! owner's install. Data reads at the crate's existing addresses only: nothing
//! is executed, decrypted or disassembled, and the game is not started.
//!
//! Owner-local and ignored by default: set `SA2_GAME_PATH` (install root) and
//! `SA2_PC_CONFIG` (SA Tools `GameConfig/PC_SA2`), then run with
//! `--ignored --nocapture`.
use sa2_asset_tools::{
    board_model::{self, BoardModel},
    board_motion, chao_collision, character_animations, city_land, death_zones, duel_platform,
    fan_stage, finalhazard, kart_course, mission_ranks, object_table, pe, player_physics,
    stage_paths, stage_positions, story,
};
use std::{fs, path::PathBuf};

fn report(name: &str, result: Result<String, String>, failures: &mut Vec<String>) {
    match result {
        Ok(summary) => println!("ok   {name:44} {summary}"),
        Err(err) => {
            println!("FAIL {name:44} {err}");
            failures.push(format!("{name}: {err}"));
        }
    }
}

fn e(err: impl std::fmt::Display) -> String {
    err.to_string()
}

#[test]
#[ignore = "owner-local Steam install"]
fn every_executable_and_dll_table_reader_loads_the_steam_install() {
    let (Some(game), Some(config)) = (
        std::env::var_os("SA2_GAME_PATH"),
        std::env::var_os("SA2_PC_CONFIG"),
    ) else {
        eprintln!("SA2_GAME_PATH and SA2_PC_CONFIG are not set");
        return;
    };
    let game = PathBuf::from(game);
    let config = PathBuf::from(config);
    let exe = fs::read(game.join("sonic2app.exe")).expect("read sonic2app.exe");
    let dll = fs::read(game.join("resource/gd_PC/DLL/Win32/Data_DLL.dll")).expect("read Data_DLL");
    let gd = game.join("resource/gd_PC");
    let ini = |name: &str| fs::read_to_string(config.join(name)).expect("read SA Tools config");
    // Path and death-zone tables are listed across every stage configuration;
    // gather only the sections of one type, as tests/stage_path_reference.ps1 does.
    let mut sources = Vec::new();
    for entry in fs::read_dir(&config).expect("list SA Tools config") {
        let path = entry.expect("config entry").path();
        if path
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("ini"))
        {
            sources.push(fs::read_to_string(&path).unwrap_or_default());
        }
    }
    let sections_of = |kind: &str| {
        let mut out = String::from("game=SA2B\ndatafile=sonic2app.exe\n");
        for text in &sources {
            for block in text.split("\n[").skip(1) {
                let declared = block.lines().find_map(|l| l.trim().strip_prefix("type="));
                if declared.is_some_and(|t| t.trim().eq_ignore_ascii_case(kind)) {
                    out.push('[');
                    out.push_str(block.trim_end());
                    out.push('\n');
                }
            }
        }
        out
    };
    let paths_ini = sections_of("pathlist");
    let death_ini = sections_of("deathzone");
    let mut failures = Vec::new();
    let f = &mut failures;

    report(
        "exe identity (pinned SHA-256)",
        object_table::verify_image(&exe)
            .map(|_| "matches".into())
            .map_err(e),
        f,
    );
    report(
        "PE exports: Data_DLL",
        pe::read_pe(&dll)
            .map(|p| format!("{} exports", p.exports.len()))
            .map_err(e),
        f,
    );
    report(
        "object lists (ObjectLists.ini)",
        object_table::mapped_tables(&exe, &ini("ObjectLists.ini"))
            .map(|t| {
                format!(
                    "{} lists, {} objects",
                    t.len(),
                    t.values().map(|l| l.entries.len()).sum::<usize>()
                )
            })
            .map_err(e),
        f,
    );
    report(
        "stage start/end positions",
        stage_positions::read_rows(&exe)
            .map(|r| format!("{} rows", r.len()))
            .map_err(e),
        f,
    );
    report(
        "player physics",
        player_physics::read(&exe)
            .map(|r| format!("{} characters", r.len()))
            .map_err(e),
        f,
    );
    report(
        "mission ranks",
        mission_ranks::read(&exe)
            .map(|r| format!("{} rows", r.len()))
            .map_err(e),
        f,
    );
    report(
        "story sequences (ADVERTISE.ini)",
        story::read(&exe, &ini("ADVERTISE.ini"))
            .map(|s| format!("{} sequences", s.len()))
            .map_err(e),
        f,
    );
    report(
        "character animation tables",
        character_animations::read(&exe)
            .map(|t| format!("{} tables", t.len()))
            .map_err(e),
        f,
    );
    report(
        "Chao collision record",
        chao_collision(&exe).map(|_| "read".into()).map_err(e),
        f,
    );
    report(
        "stage paths",
        stage_paths::read(&exe, &paths_ini)
            .map(|t| {
                format!(
                    "{} lists, {} paths",
                    t.len(),
                    t.values().map(|l| l.paths.len()).sum::<usize>()
                )
            })
            .map_err(e),
        f,
    );
    report(
        "death zones",
        death_zones::read(&exe, &death_ini)
            .map(|t| {
                format!(
                    "{} tables, {} zones",
                    t.len(),
                    t.values().map(|l| l.zones.len()).sum::<usize>()
                )
            })
            .map_err(e),
        f,
    );
    for id in [
        BoardModel::Streetboard,
        BoardModel::ShadowBoard,
        BoardModel::AmyBoard,
        BoardModel::MetalBoard,
        BoardModel::SonicBoard,
        BoardModel::CastShadow,
        BoardModel::Modifier,
    ] {
        report(
            &format!("City Escape board model {id:?}"),
            board_model::extract(&exe, id)
                .map(|_| "read".into())
                .map_err(e),
            f,
        );
    }
    report(
        "Finalhazard models",
        finalhazard::read(&exe)
            .map(|[a, b]| format!("{} + {} joints", a.len(), b.len()))
            .map_err(e),
        f,
    );
    report(
        "Finalhazard motions",
        finalhazard::motions(&exe).map(|_| "read".into()).map_err(e),
        f,
    );
    match fs::read(gd.join("modeltex_bosslast2.prs")) {
        Ok(prs) => report(
            "Finalhazard textures",
            finalhazard::textures(&exe, &prs)
                .map(|_| "bound".into())
                .map_err(e),
            f,
        ),
        Err(err) => report("Finalhazard textures", Err(e(err)), f),
    }
    report(
        "kart profiles",
        fan_stage::kart_profiles(&exe)
            .map(|k| format!("{} karts", k.len()))
            .map_err(e),
        f,
    );
    report(
        "kart courses (exe)",
        kart_course::courses(&exe)
            .map(|c| format!("{} courses", c.len()))
            .map_err(e),
        f,
    );
    report(
        "kart course models (Data_DLL)",
        kart_course::read_course_table(&dll)
            .map(|_| "read".into())
            .map_err(e),
        f,
    );
    report(
        "duel platform (exe)",
        duel_platform::read(&exe).map(|_| "read".into()).map_err(e),
        f,
    );
    report(
        "City Escape land (Data_DLL)",
        city_land::read_win32_collision_stage(&dll)
            .map(|s| format!("{} land entries", s.entries.len()))
            .map_err(e),
        f,
    );
    report(
        "City Escape board motions (Data_DLL)",
        board_motion::read_pc(&dll)
            .map(|_| "read".into())
            .map_err(e),
        f,
    );
    report(
        "cutscene voice table",
        pe::cutscene_voice_table(&exe)
            .map(|t| format!("{} rows", t.len()))
            .map_err(e),
        f,
    );
    assert!(failures.is_empty(), "{} readers failed", failures.len());
}
