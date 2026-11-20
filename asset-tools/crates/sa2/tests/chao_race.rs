//! Chao Race rules against the GameCube ChaoStgRace.rel / ChaoMain.rel PowerPC.
//!
//! Needs the user's own disc files (never committed): SA2_MAIN_DOL,
//! SA2_CHAO_MAIN_REL, SA2_CHAO_RACE_REL; optional SA2_PPC_PYTHON,
//! SA2_RACE_SEED, SA2_RACE_RUNS. Returns early when the oracle skips.
use sa2_asset_tools::chao_race::{self as r, Body, Progress, RunBehaviour};
use serde_json::Value;
use std::{env, path::PathBuf, process::Command};

fn oracle() -> Option<Value> {
    let python = env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg("-I")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/chao_race_ppc.py"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    if o.stdout.is_empty() {
        eprintln!("skipped: {}", String::from_utf8_lossy(&o.stderr));
        return None;
    }
    Some(serde_json::from_slice(&o.stdout).unwrap())
}

fn f(v: &Value) -> f32 {
    f32::from_bits(v.as_u64().unwrap() as u32)
}
fn v3(v: &Value) -> [f32; 3] {
    std::array::from_fn(|k| f(&v[k]))
}
fn bits(a: [f32; 3]) -> Vec<u64> {
    a.iter().map(|x| u64::from(x.to_bits())).collect()
}
fn want(v: &Value) -> Vec<u64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_u64().unwrap())
        .collect()
}

#[test]
#[ignore = "requires the user's GameCube main.dol, ChaoMain.rel and ChaoStgRace.rel and Python with Unicorn"]
fn race_rules_match_the_gamecube_powerpc() {
    let Some(doc) = oracle() else { return };
    for c in doc["move"].as_array().unwrap() {
        let mut b = Body {
            velocity: v3(&c["vel"]),
            accel: [0.0; 3],
            position: v3(&c["pos"]),
            gravity: f(&c["gravity"]),
        };
        let skill = c["skill"].as_u64().unwrap() as u16;
        let stamina = c["stamina"].as_i64().unwrap() as i32;
        let turbo = c["turbo"].as_bool().unwrap();
        let (sin, cos) = (f(&c["sin"]), f(&c["cos"]));
        let speed = match c["kind"].as_str().unwrap() {
            "run" => r::run(&mut b, skill, stamina, turbo, sin, cos),
            "fly" => r::fly(&mut b, skill, stamina, turbo, sin, cos),
            "swim" => r::swim(&mut b, skill, stamina, turbo, sin, cos),
            _ => r::climb(&mut b, skill, stamina, turbo, sin, cos),
        };
        assert_eq!(bits(b.accel), want(&c["accOut"]), "accel {c}");
        assert_eq!(bits(b.velocity), want(&c["velOut"]), "velocity {c}");
        assert_eq!(bits(b.position), want(&c["posOut"]), "position {c}");
        if matches!(c["kind"].as_str(), Some("fly" | "climb")) {
            assert_eq!(
                vec![u64::from(speed.to_bits())],
                want(&c["motionSpeed"]),
                "motion speed {c}"
            );
        }
    }
    for c in doc["behaviour"].as_array().unwrap() {
        let which = if c["which"] == "D2B4" {
            RunBehaviour::D2B4
        } else {
            RunBehaviour::D578
        };
        let mut counter = c["counter"].as_i64().unwrap() as i32;
        let mut stamina = c["stamina"].as_i64().unwrap() as i32;
        let mut values: Vec<i32> = c["rand"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_i64().unwrap() as i32)
            .collect();
        let mut used = 0;
        let trip = r::run_frame(
            which,
            c["skill"].as_u64().unwrap() as u16,
            &mut counter,
            &mut stamina,
            c["dd"].as_u64().unwrap() as u8,
            &mut || {
                used += 1;
                values.remove(0)
            },
        );
        assert_eq!(trip, c["trip"].as_bool().unwrap(), "trip {c}");
        assert_eq!(used, c["used"].as_u64().unwrap() as usize, "rand calls {c}");
        assert_eq!(
            i64::from(counter),
            c["counterOut"].as_i64().unwrap(),
            "counter {c}"
        );
        assert_eq!(
            i64::from(stamina),
            c["staminaOut"].as_i64().unwrap(),
            "stamina {c}"
        );
    }
    for c in doc["start"].as_array().unwrap() {
        let s = r::start_stamina(c[0].as_u64().unwrap() as u16);
        assert_eq!(i64::from(s), c[1].as_i64().unwrap());
        assert_eq!(i64::from(s), c[2].as_i64().unwrap());
    }
    for c in doc["progress"].as_array().unwrap() {
        let course = c["course"].as_i64().unwrap() as i32;
        let mut p = Progress::default();
        if (0..13).contains(&course) {
            p.0[course as usize] = c["old"].as_i64().unwrap() as i8;
        }
        p.record(course, c["level"].as_i64().unwrap() as i32);
        assert_eq!(
            i64::from(p.get(course)),
            c["after"].as_i64().unwrap(),
            "progress {c}"
        );
    }
    eprintln!(
        "{} movement cases, seed {}",
        doc["move"].as_array().unwrap().len(),
        doc["seed"]
    );
}

#[test]
#[ignore = "requires the user's own sonic2app.exe (SA2_GAME_PATH) and GameCube ChaoMain.rel (SA2_CHAO_MAIN_REL)"]
fn steam_course_rows_equal_the_gamecube_rows() {
    let exe =
        PathBuf::from(env::var_os("SA2_GAME_PATH").expect("SA2_GAME_PATH")).join("sonic2app.exe");
    let steam = r::read_course_rows_exe(&std::fs::read(exe).unwrap()).unwrap();
    let gc =
        r::read_course_rows_rel(&std::fs::read(env::var_os("SA2_CHAO_MAIN_REL").unwrap()).unwrap())
            .unwrap();
    assert_eq!(steam, gc);
    assert_eq!(steam.len(), 62);
}
