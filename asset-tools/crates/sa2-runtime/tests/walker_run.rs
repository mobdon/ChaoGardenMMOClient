//! The walker adapter in the exec slot: 600 Iron Gate frames with a short
//! scripted stick, run twice (needs SA2_GAME_PATH; nothing is committed).
use sa2_asset_tools::input::PadStatus;
use sa2_asset_tools::{stage_land, stage_positions};
use sa2_runtime::game::{Game, SteamData, WALKER_EXEC};
use sa2_runtime::player::steam_physics_rows;
use sa2_runtime::walker_slot::{mech_eggman_actions, physics_rows, AdapterWalker, Land};
use sa2_runtime::Event;
use std::{fs, path::PathBuf};

fn data() -> SteamData {
    let game = PathBuf::from(std::env::var_os("SA2_GAME_PATH").expect("SA2_GAME_PATH"));
    let gd = game.join("resource/gd_PC");
    SteamData {
        exe: fs::read(game.join("sonic2app.exe")).unwrap(),
        stg11: fs::read(gd.join("stg11D.rel")).unwrap(),
        set_s: fs::read(gd.join("set0011_s.bin")).ok(),
        set_u: fs::read(gd.join("set0011_u.bin")).ok(),
    }
}

/// Stick forward (full) for frames 30..150, released afterwards.
fn pads(frame: u32) -> [PadStatus; 4] {
    let mut p = [PadStatus::default(); 4];
    if (30..150).contains(&frame) {
        p[0].stick[1] = 72;
    }
    p
}

struct Frame {
    pos: [f32; 3],
    action: i16,
    unported: usize,
}

fn run(d: &SteamData) -> Vec<Frame> {
    let mut g = Game::iron_gate(d).unwrap();
    let land = Land::from_stage(&stage_land::read(&d.stg11).unwrap());
    let rows = physics_rows(&steam_physics_rows(&d.exe).unwrap());
    let actions = mech_eggman_actions(&d.exe).unwrap();
    let gd = PathBuf::from(std::env::var_os("SA2_GAME_PATH").unwrap()).join("resource/gd_PC");
    let motions = fs::read(gd.join("ewalkmtn.prs")).unwrap();
    g.walker = Some(Box::new(AdapterWalker::new(land, rows, actions, &motions)));
    let player = g.frame.player_tasks[0].expect("player 0");
    // The single-player start row for Eggman (3, before the swap to 7), level 11.
    let start = stage_positions::for_stage(&d.exe, 11)
        .unwrap()
        .into_iter()
        .find(|r| r.character == "MechEggman" && r.kind == stage_positions::PositionKind::Start)
        .and_then(|r| r.poses.into_iter().next())
        .expect("start row");
    {
        let twp = g.rt.task_mut(player).unwrap().twp.as_mut().unwrap();
        for k in 0..3 {
            twp[0x14 + 4 * k..0x18 + 4 * k].copy_from_slice(&start.position[k].to_be_bytes());
        }
        twp[0xC..0x10].copy_from_slice(&u32::from(start.yaw).to_be_bytes());
    }
    let mut out = Vec::new();
    for f in 0..600 {
        if std::env::var_os("WALKER_TRACE").is_some() {
            eprintln!("step {f}");
        }
        g.step(pads(f));
        let t = g.rt.task(player).unwrap();
        let tw = |o: usize| f32::from_bits(t.twp_word(o));
        let pwp = t.w40.as_ref().unwrap();
        out.push(Frame {
            pos: [tw(0x14), tw(0x18), tw(0x1C)],
            action: i16::from_be_bytes([pwp[0x174 + 4], pwp[0x174 + 5]]),
            unported: g
                .rt
                .events
                .iter()
                .filter(|e| {
                    matches!(
                        e,
                        Event::Unported {
                            addr: WALKER_EXEC,
                            ..
                        }
                    )
                })
                .count(),
        });
    }
    out
}

#[test]
#[ignore = "requires the installed Steam game (SA2_GAME_PATH)"]
fn walker_adapter_runs_iron_gate() {
    let d = data();
    let a = run(&d);
    let b = run(&d);
    let same = a
        .iter()
        .zip(&b)
        .all(|(x, y)| x.pos.map(f32::to_bits) == y.pos.map(f32::to_bits) && x.action == y.action);
    for (i, f) in a.iter().enumerate() {
        if i % 50 == 0 || f.unported > 0 || f.pos.iter().any(|v| !v.is_finite()) {
            eprintln!(
                "frame {i}: pos {:?} action {} unported {}",
                f.pos, f.action, f.unported
            );
        }
    }
    assert!(same, "same inputs, same run");
}
