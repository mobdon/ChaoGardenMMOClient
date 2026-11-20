//! E5: a headless 600-frame Iron Gate run on the Steam files, and the same
//! inputs giving the same run (needs SA2_GAME_PATH; nothing is committed).
use sa2_asset_tools::input::PadStatus;
use sa2_runtime::game::{Game, SteamData};
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

/// Stick forward for 300 frames, then the A button every 30th frame.
fn pads(frame: u32) -> [PadStatus; 4] {
    let mut p = [PadStatus::default(); 4];
    if frame < 300 {
        p[0].stick[1] = 100;
    } else if frame.is_multiple_of(30) {
        p[0].button = 0x0100;
    }
    p
}

fn run(d: &SteamData) -> (Game, Vec<u64>) {
    let mut g = Game::iron_gate(d).unwrap();
    let mut digests = Vec::new();
    for f in 0..600 {
        g.step(pads(f));
        if f % 100 == 99 {
            digests.push(g.digest());
        }
    }
    (g, digests)
}

#[test]
#[ignore = "requires the installed Steam game (SA2_GAME_PATH)"]
fn iron_gate_runs_600_frames_deterministically() {
    let d = data();
    let (g, a) = run(&d);
    let (_, b) = run(&d);
    assert_eq!(a, b, "same inputs, same run");
    assert_eq!(g.frames, 600);
    let spawned = g
        .stage
        .set
        .table
        .entries
        .iter()
        .filter(|e| e.task.is_some())
        .count();
    let walker =
        g.rt.events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    Event::Unported {
                        addr: 0x800B_3A8C,
                        ..
                    }
                )
            })
            .count();
    eprintln!(
        "spawned {spawned}, ring total {}, last-frame events {}, walker exec unported calls {walker}",
        g.stage.set.hud_rings,
        g.rt.events.len()
    );
    assert!(spawned > 0);
}
