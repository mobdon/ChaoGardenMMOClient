//! Compare `enemy_lib` with main.dol's enemy helpers (and stg09D.rel's
//! sqrtf) run under the Unicorn DOL harness (tests/enemy_lib_ppc.py).
use sa2_asset_tools::{
    enemy_lib::{
        engine::*,
        motion::{read_table, Motion},
        EnemyTask, EnemyWork, PlayerView, Rand,
    },
    ninja_matrix::MatrixStack,
};
use serde_json::Value;
use std::{path::PathBuf, process::Command};

fn f(v: &Value) -> f32 {
    f32::from_bits(v.as_u64().unwrap() as u32)
}
fn v3(v: &Value) -> [f32; 3] {
    [f(&v[0]), f(&v[1]), f(&v[2])]
}
fn u(v: &Value) -> u32 {
    v.as_i64().unwrap() as u32
}
fn i(v: &Value) -> i32 {
    v.as_i64().unwrap() as i32
}
fn bits(v: [f32; 3]) -> Vec<u32> {
    v.map(f32::to_bits).to_vec()
}
fn words(v: &Value) -> Vec<u32> {
    v.as_array().unwrap().iter().map(u).collect()
}
fn players(v: &Value) -> [Option<PlayerView>; 2] {
    std::array::from_fn(|k| {
        (!v[k].is_null()).then(|| PlayerView {
            pos: v3(&v[k]),
            center: [0.0; 3],
        })
    })
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_STAGE_REL (GC stg09D.rel) and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn helpers_match_main_dol() {
    let python = std::env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/enemy_lib_ppc.py"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    if o.stdout.is_empty() {
        eprintln!("skipped: {}", String::from_utf8_lossy(&o.stderr));
        return;
    }
    let c: Value = serde_json::from_slice(&o.stdout).unwrap();
    let list = |k: &str| c[k].as_array().unwrap().clone();

    for x in list("diff") {
        assert_eq!(diff_angle(i(&x[0]), i(&x[1])) as u32, u(&x[2]), "diff {x}");
    }
    for x in list("adjust") {
        assert_eq!(
            adjust_angle(i(&x[0]), i(&x[1]), i(&x[2])) as u32,
            u(&x[3]),
            "adjust {x}"
        );
    }
    for x in list("fraction") {
        assert_eq!(fraction(f(&x[0])).to_bits(), u(&x[1]), "fraction {x}");
        assert_eq!(round_off(f(&x[0])).to_bits(), u(&x[2]), "round_off {x}");
    }
    for x in list("sqrt") {
        assert_eq!(sqrtf_dol(f(&x[0])).to_bits(), u(&x[1]), "sqrtf {x}");
        assert_eq!(sqrtf_rel(f(&x[0])).to_bits(), u(&x[2]), "REL sqrtf {x}");
    }
    for x in list("distance") {
        assert_eq!(
            distance(v3(&x[0]), v3(&x[1])).to_bits(),
            u(&x[2]),
            "distance {x}"
        );
    }
    for x in list("approach") {
        assert_eq!(
            approach(f(&x[0]), f(&x[1]), f(&x[2])).to_bits(),
            u(&x[3]),
            "approach {x}"
        );
    }
    for x in list("scalor") {
        assert_eq!(scalor(v3(&x[0])).to_bits(), u(&x[1]), "scalor {x}");
    }
    for x in list("unit") {
        assert_eq!(bits(unit_vector(v3(&x[0]))), words(&x[1]), "unit {x}");
    }
    for x in list("rand") {
        let mut r = Rand(u(&x[0]));
        assert_eq!(r.rand() as u32, u(&x[1]), "rand {x}");
        assert_eq!(r.0, u(&x[2]));
    }
    for x in list("leash") {
        let mut t = EnemyTask {
            pos: v3(&x[0]),
            ..Default::default()
        };
        let w = EnemyWork {
            home: v3(&x[1]),
            ..Default::default()
        };
        leash(&mut t, &w, f(&x[2]));
        assert_eq!(bits(t.pos), words(&x[3]), "leash {x}");
    }
    for x in list("face") {
        let mut t = EnemyTask {
            pos: v3(&x[0]),
            ang: [i(&x[1][0]), i(&x[1][1]), i(&x[1][2])],
            ..Default::default()
        };
        let mut w = EnemyWork {
            turn_rate: i(&x[2]),
            ..Default::default()
        };
        let left = face_player(&mut t, &mut w, v3(&x[3]));
        assert_eq!(
            (left as u32, t.ang[1] as u32, w.target_yaw as u32),
            (u(&x[4]), u(&x[5]), u(&x[6])),
            "face {x}"
        );
    }
    for x in list("notice") {
        let t = EnemyTask {
            pos: v3(&x[0]),
            ..Default::default()
        };
        let w = EnemyWork {
            home: v3(&x[1]),
            home_offset: v3(&x[2]),
            notice_range_sq: f(&x[3]),
            ..Default::default()
        };
        let p = players(&x[4]);
        let got: Vec<u32> = [NoticeFrom::HomeOffset, NoticeFrom::Home, NoticeFrom::Body]
            .iter()
            .map(|&k| player_in_range(k, &t, &w, &p, u(&x[5]) as u8, u(&x[6]) as u8) as u32)
            .collect();
        assert_eq!(got, words(&x[7]), "notice {x}");
    }
    for x in list("nearest") {
        assert_eq!(
            nearest_player(&players(&x[1]), v3(&x[0])) as u32,
            u(&x[2]),
            "nearest {x}"
        );
    }
    let mut stack = MatrixStack::new();
    for x in list("home_offset") {
        let mut t = EnemyTask {
            ang: [0, i(&x[0]), 0],
            scl: v3(&x[1]),
            ..Default::default()
        };
        let mut w = EnemyWork::default();
        home_offset(&mut stack, &mut t, &mut w);
        assert_eq!(bits(w.home_offset), words(&x[2]), "offset {x}");
        assert_eq!(t.scl[2].to_bits(), u(&x[3]), "scale {x}");
    }

    let rel = std::fs::read(std::env::var("SA2_STAGE_REL").unwrap()).unwrap();
    let table = read_table(&rel, 9, 5, 0xC3EC8, 17).unwrap();
    let base = u(&c["motion_base"]);
    let mut steps = 0;
    for run in list("motion") {
        for fr in run.as_array().unwrap() {
            let before = hex(&fr[0]);
            let mut m = decode(&before, base);
            m.step(&table);
            assert_eq!(
                m,
                decode(&hex(&fr[1]), base),
                "motion step from {before:02x?}"
            );
            steps += 1;
        }
    }
    assert!(steps >= 10_000);
}

fn hex(v: &Value) -> Vec<u8> {
    let s = v.as_str().unwrap();
    (0..s.len())
        .step_by(2)
        .map(|k| u8::from_str_radix(&s[k..k + 2], 16).unwrap())
        .collect()
}

/// The 0x30-byte motion block as main.dol lays it out.
fn decode(b: &[u8], base: u32) -> Motion {
    let w = |o: usize| u32::from_be_bytes(b[o..o + 4].try_into().unwrap());
    let p = |o: usize| (w(o) != 0).then(|| w(o) - base);
    Motion {
        frame: f32::from_bits(w(0)),
        start: f32::from_bits(w(8)),
        scale: f32::from_bits(w(0xC)),
        mode: b[0x10] as i8,
        ended: b[0x11],
        current: b[0x12] as i8,
        request: b[0x13] as i8,
        previous: b[0x14] as i8,
        next: b[0x15] as i8,
        count: i16::from_be_bytes([b[0x16], b[0x17]]),
        blend_from: p(0x20),
        blend_to: p(0x24),
        blend_frame: f32::from_bits(w(0x28)),
        blend_start: f32::from_bits(w(0x2C)),
    }
}
