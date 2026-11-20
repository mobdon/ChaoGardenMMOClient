//! Build a walker stage (Iron Gate or Prison Lane) from the installed game
//! files and write its native stage-rule state as JSON.
//!
//!     sa2-play GAME_DIR PC_SA2_CONFIG [--stage iron-gate|prison-lane] OUT.json
//!
//! GAME_DIR is the Steam "Sonic Adventure 2" folder; PC_SA2_CONFIG is SA Tools
//! 786's GameConfig/PC_SA2 directory. Nothing is written except OUT.json.
//!
//! The walker's movement is main-program code (encrypted on disk in the
//! Steam executable), so nothing here moves a player or opens a window.
use sa2_asset_tools::{
    fan_stage::Campaign,
    play_session::{Session, Sources, StageRules, IRON_GATE, PRISON_LANE},
};
use std::{fs, path::PathBuf, process::ExitCode};

/// (option name, story sequence, action index, rules)
const STAGES: [(&str, &str, usize, &StageRules); 2] = [
    ("iron-gate", "Dark Story Sequence", 1, &IRON_GATE),
    ("prison-lane", "Hero Story Sequence", 9, &PRISON_LANE),
];

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("sa2-play: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<String> = std::env::args().collect();
    let mut stage_name = "iron-gate".to_string();
    if let Some(i) = args.iter().position(|a| a == "--stage") {
        stage_name = args.get(i + 1).ok_or("--stage needs a name")?.clone();
        args.drain(i..i + 2);
    }
    if args.len() != 4 {
        return Err(
            "usage: sa2-play GAME_DIR PC_SA2_CONFIG [--stage iron-gate|prison-lane] OUT.json"
                .into(),
        );
    }
    let &(_, sequence, action, rules) = STAGES
        .iter()
        .find(|s| s.0 == stage_name)
        .ok_or_else(|| format!("unknown stage {stage_name}"))?;
    let game = PathBuf::from(&args[1]);
    let cfg = PathBuf::from(&args[2]);
    let gd = game.join("resource").join("gd_PC");
    let exe = fs::read(game.join("sonic2app.exe"))?;
    let read = |n: &str| fs::read_to_string(cfg.join(n));
    let campaign = Campaign::new(
        &exe,
        &read("ADVERTISE.ini")?,
        &read("sa2lvl.ini")?,
        &read("ObjectLists.ini")?,
    )?;
    let load = |n: &str| -> sa2_asset_tools::Result<Option<Vec<u8>>> {
        let f = gd.join(n);
        Ok(if f.is_file() {
            Some(fs::read(f)?)
        } else {
            None
        })
    };
    let stage = campaign.mount(sequence, action, load)?;
    let module = fs::read(gd.join(rules.movers.module_file))?;
    let s = Session::walker_stage(
        &stage,
        Sources {
            exe: &exe,
            stage_module: &module,
        },
        rules,
    )?;
    let report = serde_json::json!({
        "stage": rules.name,
        "level": s.level,
        "character": s.character,
        "start": s.start,
        "startYaw": s.start_yaw,
        "physics": s.physics,
        "moverProgram": s.movers_program,
        "movers": s.movers,
        "rings": s.rings_placed,
        "checkpoints": s.checkpoints,
        "springs": s.springs,
        "goals": s.goals,
        "slotWords": s.slot_words,
        "deathZones": s.death_zone_count(),
        "collisionTriangles": s.world.tris.len(),
        "unimplementedPlacements": s.unimplemented,
    });
    fs::write(&args[3], serde_json::to_vec_pretty(&report)?)?;
    println!(
        "{}: {} movers, {} rings, {} checkpoints, {} goal(s), {} unimplemented kinds",
        rules.name,
        s.movers.len(),
        s.rings_placed.len(),
        s.checkpoints.len(),
        s.goals.len(),
        s.unimplemented.len()
    );
    Ok(())
}
