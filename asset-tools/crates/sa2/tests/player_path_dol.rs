//! `player_path` against main.dol (tests/dol_ppc.py).
use sa2_asset_tools::{
    ninja_matrix::MatrixStack,
    player_path::{self, Nearest, Path, PathPlayer, Points, Sample},
};
use serde_json::Value;
use std::{env, path::PathBuf, process::Command};

fn u(v: &Value) -> u32 {
    v.as_u64().unwrap() as u32
}
fn f(v: &Value) -> f32 {
    f32::from_bits(u(v))
}
fn i(v: &Value) -> i32 {
    v.as_i64().unwrap() as i32
}
fn v3(v: &Value) -> [f32; 3] {
    [0, 1, 2].map(|k| f(&v[k]))
}
fn bits3(v: [f32; 3]) -> [u32; 3] {
    v.map(f32::to_bits)
}

fn path(p: &Value) -> Path {
    let recs = p["records"].as_array().unwrap();
    let points = if p["kind"] == 1 {
        Points::Angled(
            recs.iter()
                .map(|r| (i(&r[0]) as i16, i(&r[1]) as i16, f(&r[2]), v3(&r[3])))
                .collect(),
        )
    } else {
        Points::Vectored(
            recs.iter()
                .map(|r| (v3(&r[0]), f(&r[1]), v3(&r[2])))
                .collect(),
        )
    };
    Path {
        flags: i(&p["flags"]) as i16,
        count: i(&p["count"]) as i16,
        total: f(&p["total"]),
        points,
    }
}

#[test]
#[ignore = "requires the user's main.dol (SA2_MAIN_DOL) and Python with Unicorn PPC"]
fn matches_main_dol() {
    let python = env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg("-I")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/player_path_ppc.py"))
        .arg(env::var_os("SA2_MAIN_DOL").expect("SA2_MAIN_DOL"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let doc: Value = serde_json::from_slice(&o.stdout).unwrap();
    for c in doc["floor"].as_array().unwrap() {
        assert_eq!(
            player_path::floorf(f(&c[0])).to_bits(),
            u(&c[1]),
            "floorf {c}"
        );
    }
    for c in doc["wrap"].as_array().unwrap() {
        assert_eq!(
            player_path::wrap(f(&c[0]), f(&c[1])).to_bits(),
            u(&c[2]),
            "wrap {c}"
        );
    }
    for c in doc["line"].as_array().unwrap() {
        let want = c[3].as_bool().unwrap();
        let (d, foot) = player_path::line_distance(v3(&c[0]), v3(&c[1]), v3(&c[2]), want);
        assert_eq!(d.to_bits(), u(&c[4]), "line {c}");
        if want {
            assert_eq!(
                bits3(foot.unwrap()),
                [0, 1, 2].map(|k| u(&c[5][k])),
                "line foot {c}"
            );
        }
    }
    for c in doc["segment"].as_array().unwrap() {
        let (d, q) = player_path::segment_distance(v3(&c[0]), v3(&c[1]), v3(&c[2]));
        assert_eq!(d.to_bits(), u(&c[3]), "segment {c}");
        assert_eq!(
            bits3(q),
            [0, 1, 2].map(|k| u(&c[4][k])),
            "segment point {c}"
        );
    }
    for c in doc["nearest"].as_array().unwrap() {
        let p = path(&c["path"]);
        match player_path::nearest(&p, v3(&c["pos"])) {
            Nearest::Found { dist, point, along } => {
                assert_eq!(dist.to_bits(), u(&c["dist"]), "nearest {c}");
                assert_eq!(
                    bits3(point),
                    [0, 1, 2].map(|k| u(&c["point"][k])),
                    "nearest {c}"
                );
                assert_eq!(along.to_bits(), u(&c["along"]), "nearest {c}");
            }
            other => panic!("unexpected {other:?} for {c}"),
        }
    }
    for c in doc["records"].as_array().unwrap() {
        let p = path(&c["path"]);
        let s = player_path::segment(&p, i(&c["i"]));
        let raw: Vec<u32> = (0..16).map(|k| u(&c["raw"][k])).collect();
        if c["path"]["kind"] == 1 {
            let pair = |a: [i16; 2]| ((a[0] as u16 as u32) << 16) | a[1] as u16 as u32;
            assert_eq!(raw[0], pair(s.a0), "record {c}");
            assert_eq!(raw[1], s.len0.to_bits());
            assert_eq!(raw[2..5], bits3(s.p0));
            assert_eq!(raw[5], pair(s.a1), "record {c}");
            assert_eq!(raw[6], s.len1.to_bits());
            assert_eq!(raw[7..10], bits3(s.p1));
        } else {
            assert_eq!(raw[0..3], bits3(s.v0), "record {c}");
            assert_eq!(raw[3], s.len0.to_bits());
            assert_eq!(raw[4..7], bits3(s.p0));
            assert_eq!(raw[7..10], bits3(s.v1));
            assert_eq!(raw[10], s.len1.to_bits());
            assert_eq!(raw[11..14], bits3(s.p1));
        }
    }
    let mut sampled = 0;
    for c in doc["sample"].as_array().unwrap() {
        let p = path(&c["path"]);
        let mut s = Sample {
            dist: f(&c["dist"]),
            ..Default::default()
        };
        let ok = player_path::sample(&p, &mut s);
        assert_eq!(ok as u64, c["ok"].as_u64().unwrap(), "sample {c}");
        let raw: Vec<u32> = (0..17).map(|k| u(&c["raw"][k])).collect();
        assert_eq!(raw[4], s.dist.to_bits(), "sample dist {c}");
        if ok {
            sampled += 1;
            assert_eq!(raw[0..4], s.angles.map(|a| a as u32), "sample angles {c}");
            assert_eq!(raw[5..8], bits3(s.pos), "sample pos {c}");
            assert_eq!(raw[8..11], bits3(s.up0), "sample up0 {c}");
            assert_eq!(raw[11..14], bits3(s.up1), "sample up1 {c}");
            assert_eq!(raw[14..17], bits3(s.dir), "sample dir {c}");
        }
    }
    let mut stack = MatrixStack::new();
    let mut steered = 0;
    for c in doc["follow"].as_array().unwrap() {
        let p = path(&c["path"]);
        let mut pl = PathPlayer {
            status: i(&c["status"]) as i16,
            angles: [0, 1, 2].map(|k| i(&c["angles"][k])),
            pos: v3(&c["pos"]),
            speed_x: f(&c["speed"]),
            dash_speed: f(&c["dash"]),
            path_dist: f(&c["pathDist"]),
            normal: v3(&c["normal"]),
            aim: 0,
        };
        let ok = player_path::follow(&mut stack, &p, &mut pl, f(&c["stick"]));
        assert_eq!(
            pl.status as i64,
            c["outStatus"].as_i64().unwrap(),
            "follow status {c}"
        );
        assert_eq!(
            pl.angles,
            [0, 1, 2].map(|k| i(&c["outAngles"][k])),
            "follow angles {c}"
        );
        assert_eq!(pl.speed_x.to_bits(), u(&c["outSpeed"]), "follow speed {c}");
        assert_eq!(
            pl.path_dist.to_bits(),
            u(&c["outPathDist"]),
            "follow dist {c}"
        );
        assert_eq!(pl.aim, i(&c["outAim"]), "follow aim {c}");
        assert_eq!(stack.depth(), 0);
        steered += ok as usize;
    }
    eprintln!(
        "floorf/wrap/line/segment cases, 500 paths (nearest, every record, {sampled} samples) \
         and 1,500 path-follow frames ({steered} steered) matched main.dol"
    );
}
