//! Top-down text map of a walker stage around a box: for each XZ cell, the
//! highest walkable surface within [ylo, yhi] and whether the walker fits.
//!
//!     walker_slice GAME_DIR PC_SA2_CONFIG prison-lane X0 X1 Z0 Z1 STEP YLO YHI
//!
//! Cell glyphs: ' ' no walkable surface in range, 'o' the walker fits,
//! '#' surface but the walker does not fit (wall, low ceiling).
//! Row labels are Z; columns run from X0 to X1.
use sa2_asset_tools::{
    fan_stage::Campaign,
    play_session::{Session, Sources, IRON_GATE, PRISON_LANE},
    play_world::WALKABLE_NORMAL_Y,
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
    let f: Vec<f32> = a[4..11].iter().map(|v| v.parse().unwrap()).collect();
    let (x0, x1, z0, z1, step, ylo, yhi) = (f[0], f[1], f[2], f[3], f[4], f[5], f[6]);
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
    let w = s.snapshot_world();
    let (r, h) = (s.physics.rad, s.physics.height);
    println!("rad {r} height {h}; x {x0}..{x1} step {step}");
    if a.get(11).map(String::as_str) == Some("list") {
        // Every surface (walkable or not) in range, per cell.
        let mut z = z1;
        while z >= z0 {
            let mut x = x0;
            while x <= x1 {
                let mut ys = vec![];
                w.column(x, z, |g| {
                    if g.y >= ylo && g.y <= yhi {
                        ys.push((g.y.round(), (g.normal[1] * 100.0).round() / 100.0));
                    }
                });
                ys.sort_by(|a, b| a.0.total_cmp(&b.0));
                println!("({x}, {z}): {ys:?}");
                x += step;
            }
            z -= step;
        }
        return;
    }
    let mut z = z1;
    while z >= z0 {
        let mut line = String::new();
        let mut x = x0;
        while x <= x1 {
            let mut best: Option<f32> = None;
            w.column(x, z, |g| {
                if g.normal[1] >= WALKABLE_NORMAL_Y
                    && g.y >= ylo
                    && g.y <= yhi
                    && best.is_none_or(|b| g.y > b)
                {
                    best = Some(g.y);
                }
            });
            line.push(match best {
                None => ' ',
                Some(y) => {
                    let c = [x, y + 6.0 + r, z];
                    let (corr, _) = w.push_out(c, r, 1);
                    let fits = corr == [0.0; 3] && w.ceiling_above([x, y + 0.5, z], h).is_none();
                    if fits {
                        'o'
                    } else {
                        '#'
                    }
                }
            });
            x += step;
        }
        println!("{z:8.0} |{line}|");
        z -= step;
    }
}
