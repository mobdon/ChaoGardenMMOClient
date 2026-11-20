//! Reachability probe for a walker stage: plans (without driving) from the
//! start and from each checkpoint to every checkpoint, mover and the goal.
//!
//!     cargo run --release --example walker_probe -- GAME_DIR PC_SA2_CONFIG prison-lane [X Y Z ...]
use sa2_asset_tools::{
    fan_stage::Campaign,
    play_autopilot::Autopilot,
    play_session::{Session, Sources, IRON_GATE, PRISON_LANE},
};
use std::{fs, path::PathBuf};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let game = PathBuf::from(&a[1]);
    let cfg = PathBuf::from(&a[2]);
    let (seq, action, rules) = match a[3].as_str() {
        "iron-gate" => ("Dark Story Sequence", 1, &IRON_GATE),
        _ => ("Hero Story Sequence", 9, &PRISON_LANE),
    };
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
    let stage = campaign.mount(seq, action, load).unwrap();
    let module = fs::read(gd.join(rules.movers.module_file)).unwrap();
    let s = Session::walker_stage(
        &stage,
        Sources {
            exe: &exe,
            stage_module: &module,
        },
        rules,
    )
    .unwrap();
    let pilot = Autopilot::new(&s);
    let mut targets: Vec<(String, [f32; 3])> = vec![];
    for (i, c) in s.checkpoints.iter().enumerate() {
        targets.push((format!("checkpoint {i}"), c.position));
    }
    for (i, m) in s.movers.iter().enumerate() {
        targets.push((format!("mover {i} ({})", kind(&m.state)), m.position()));
    }
    targets.push(("goal".into(), s.goals[0].position));
    for k in a[4..].chunks(3) {
        targets.push((
            format!("point {k:?}"),
            [
                k[0].parse().unwrap(),
                k[1].parse().unwrap(),
                k[2].parse().unwrap(),
            ],
        ));
    }
    let mut froms = vec![("start".to_string(), s.player.position)];
    for (i, c) in s.checkpoints.iter().enumerate() {
        froms.push((format!("checkpoint {i}"), c.position));
    }
    for (fname, from) in &froms {
        println!("from {fname} {from:?}");
        for (tname, to) in &targets {
            let r = pilot.probe(&s, *from, *to, 30.0);
            println!(
                "  {tname:28} {to:?}: {} closest {:?} links {:?} climbs {}",
                if r.reached { "REACHED" } else { "no" },
                r.closest.map(|v| v.round()),
                r.links,
                r.climbs.len()
            );
        }
    }
}

fn kind(m: &sa2_asset_tools::play_session::MoverState) -> &'static str {
    use sa2_asset_tools::play_session::MoverState::*;
    match m {
        Lift(_) => "lift",
        Elevator(_) => "elevator",
        Bridge(_) => "bridge",
    }
}
