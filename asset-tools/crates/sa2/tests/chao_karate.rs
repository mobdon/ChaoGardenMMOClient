//! Chao Karate rules against the GameCube ChaoStgKarate.rel PowerPC.
//!
//! Needs the user's own disc files (never committed): SA2_MAIN_DOL,
//! SA2_CHAO_MAIN_REL, SA2_CHAO_KARATE_REL; optional SA2_PPC_PYTHON (Python
//! with Unicorn), SA2_KARATE_SEED, SA2_KARATE_RUNS. The oracle prints nothing
//! (and the test returns) when the files are missing.
use sa2_asset_tools::chao_karate::{self as k, Fighter, StateRecord};
use serde_json::Value;
use std::{env, path::PathBuf, process::Command};

fn oracle() -> Option<Value> {
    let python = env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg("-I")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/chao_karate_ppc.py"))
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
fn u(v: &Value) -> u64 {
    v.as_u64().unwrap()
}
fn i(v: &Value) -> i64 {
    v.as_i64().unwrap()
}
fn script(v: &Value) -> impl FnMut() -> i32 {
    let mut values: Vec<i32> = v
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_i64().unwrap() as i32)
        .collect();
    values.reverse();
    move || {
        values
            .pop()
            .expect("more rand() calls than the native routine made")
    }
}
fn fighter_with(work: &[(usize, f32)]) -> Fighter {
    let mut me = k::fighter([0; 7], false, false, false, &mut || 0);
    for &(off, v) in work {
        match off {
            0x20 => me.defence = v,
            0x24 => me.evasion = v,
            0x28 => me.speed = v,
            0x2C => me.power = v,
            0x34 => me.intellect = v,
            0x38 => me.zeal = v,
            0x3C => me.hp = v,
            0x40 => me.luck = v,
            0x44 => me.hp_max = v,
            0x48 => me.charge = v,
            _ => unreachable!(),
        }
    }
    me.flags = 0;
    me
}

#[test]
#[ignore = "requires the user's GameCube main.dol, ChaoMain.rel and ChaoStgKarate.rel and Python with Unicorn"]
fn karate_rules_match_the_gamecube_powerpc() {
    let Some(doc) = oracle() else { return };

    for c in doc["init"].as_array().unwrap() {
        let skills: [u16; 7] = std::array::from_fn(|n| u(&c["skills"][n]) as u16);
        let mut rng = script(&c["rand"]);
        let cpu = c["cpu"].as_bool().unwrap();
        let me = k::fighter(skills, cpu, u(&c["type"]) == 2, u(&c["d6"]) != 0, &mut rng);
        let got = [
            me.defence,
            me.evasion,
            me.speed,
            me.power,
            me.accuracy,
            me.intellect,
            me.zeal,
            me.hp,
            me.luck,
            me.hp_max,
        ]
        .map(f32::to_bits);
        let want: Vec<u32> = c["params"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| u(v) as u32)
            .collect();
        assert_eq!(got.to_vec(), want, "fighter {c}");
        assert_eq!(u64::from(me.flags), u(&c["flags"]), "fighter flags {c}");
    }

    for c in doc["hit"].as_array().unwrap() {
        let mut me = fighter_with(&[
            (0x20, f(&c["swim"])),
            (0x28, f(&c["run"])),
            (0x2C, f(&c["defPower"])),
            (0x38, f(&c["zeal"])),
            (0x3C, f(&c["hp"])),
            (0x44, f(&c["hpmax"])),
            (0x48, f(&c["charge"])),
        ]);
        let mut rec = StateRecord {
            state: 6,
            step: 0,
            frame: 0,
            flags: u(&c["twpFlags"]) as u16,
            timer: 0,
        };
        let rr: Vec<Value> = c["rand"].as_array().unwrap().clone();
        let used0 = u(&c["used0"]) as usize;
        let mut first = script(&Value::Array(rr[..used0].to_vec()));
        let hurt = k::take_hit(
            &mut me,
            &mut rec,
            f(&c["power"]),
            i(&c["kind"]) as i16,
            u(&c["phase"]) == 2,
            &mut first,
        );
        let a0 = &c["after0"];
        // Step 0 already ran step 1 once (it falls through).
        let mut step1_flags = rec.flags;
        assert_eq!(me.hp.to_bits() as u64, u(&a0["hp"]), "hp {c}");
        assert_eq!(me.zeal.to_bits() as u64, u(&a0["zeal"]), "zeal {c}");
        assert_eq!(u64::from(hurt.motion), u(&a0["motions"][0]), "motion {c}");
        assert_eq!(hurt.knocked_out, u(&a0["mgr3"]) == 2, "knock-out {c}");
        if rec.state == 6 {
            assert_eq!(u64::from(rec.flags), u(&a0["flags"]), "flags {c}");
            assert_eq!(u64::from(rec.step), u(&a0["sub"]), "step {c}");
        }
        // Second call: step 1 with or without the motion-end flag.
        if c["motionEnd"].as_bool().unwrap() {
            step1_flags |= k::state_flag::MOTION_END;
        }
        rec.flags = step1_flags;
        rec.state = 6;
        let mut rest = script(&Value::Array(rr[used0..].to_vec()));
        k::hurt_step(&mut me, &mut rec, &mut rest);
        let a1 = &c["after1"];
        assert_eq!(me.charge.to_bits() as u64, u(&a1["charge"]), "charge {c}");
        assert_eq!(u64::from(rec.state), u(&a1["state"]), "state {c}");
        assert_eq!(u64::from(rec.step), u(&a1["sub"]), "step {c}");
    }

    for c in doc["check"].as_array().unwrap() {
        let mut rec = StateRecord {
            state: u(&c["state"]) as u8,
            flags: u(&c["flags0"]) as u16,
            ..Default::default()
        };
        let mut rng = script(&c["rand"]);
        let hit = k::hit_check(
            &mut rec,
            f(&c["fly"]),
            i(&c["kind"]) as i16,
            f(&c["acc"]),
            f(&c["distance"]),
            &mut rng,
        );
        assert_eq!(u64::from(hit), u(&c["hit"]), "hit {c}");
        let want = u(&c["flags"]) as u16;
        assert_eq!(rec.flags, want, "hit flags {c}");
    }

    for c in doc["zeal"].as_array().unwrap() {
        let mut me = fighter_with(&[(0x34, f(&c["intel"])), (0x38, f(&c["zeal0"]))]);
        let mut rec = StateRecord {
            state: u(&c["state"]) as u8,
            step: 9,
            frame: u(&c["frame"]) as u8,
            flags: u(&c["flags0"]) as u16,
            timer: 0,
        };
        let mut rng = script(&c["rand"]);
        k::zeal_step(
            &mut me,
            &mut rec,
            i(&c["side"]) as i16,
            i(&c["leader"]) as i16,
            &mut rng,
        );
        assert_eq!(me.zeal.to_bits() as u64, u(&c["zeal"]), "zeal {c}");
        assert_eq!(u64::from(rec.flags), u(&c["flags"]), "flags {c}");
        assert_eq!(u64::from(rec.state), u(&c["stateAfter"]), "state {c}");
    }

    for c in doc["charge"].as_array().unwrap() {
        let mut me = fighter_with(&[
            (0x28, f(&c["run"])),
            (0x2C, f(&c["power"])),
            (0x48, f(&c["charge0"])),
        ]);
        let mut rec = StateRecord {
            state: 2,
            step: 1,
            ..Default::default()
        };
        let mut rng = script(&c["rand"]);
        k::ready_step(&mut me, &mut rec, &mut rng);
        assert_eq!(me.charge.to_bits() as u64, u(&c["charge"]), "charge {c}");
        assert_eq!(u64::from(rec.state), u(&c["state"]), "state {c}");
        assert_eq!(u64::from(rec.step), u(&c["sub"]), "step {c}");
    }

    for c in doc["attack"].as_array().unwrap() {
        let mut me = fighter_with(&[(0x38, f(&c["zeal0"])), (0x40, f(&c["luck"])), (0x48, 1.0)]);
        let mut rec = StateRecord {
            state: 4,
            step: 3,
            flags: u(&c["flags"]) as u16,
            ..Default::default()
        };
        k::attack_end(&mut me, &mut rec);
        assert_eq!(me.zeal.to_bits() as u64, u(&c["zeal"]), "zeal {c}");
        if rec.state == 2 {
            assert_eq!(me.charge.to_bits() as u64, u(&c["charge"]), "charge {c}");
        }
        assert_eq!(u64::from(rec.state), u(&c["state"]), "state {c}");
        assert_eq!(u64::from(rec.step), u(&c["sub"]), "step {c}");
        assert_eq!(u64::from(rec.timer), u(&c["timer"]), "timer {c}");
    }

    for c in doc["leader"].as_array().unwrap() {
        let v = &c["values"];
        let got = k::leader([(f(&v[0]), f(&v[1])), (f(&v[2]), f(&v[3]))]);
        assert_eq!(i64::from(got), i(&c["leader"]), "leader {c}");
    }

    let mut r = k::MslRand::default();
    for want in doc["rand"].as_array().unwrap() {
        assert_eq!(i64::from(r.rand()), i(want));
    }
    eprintln!(
        "{} cases each, seed {}",
        doc["init"].as_array().unwrap().len(),
        doc["seed"]
    );
}

#[test]
#[ignore = "requires the user's own sonic2app.exe (SA2_GAME_PATH) and ChaoStgKarate.rel (SA2_CHAO_KARATE_REL)"]
fn steam_tables_equal_the_gamecube_tables() {
    let exe =
        PathBuf::from(env::var_os("SA2_GAME_PATH").expect("SA2_GAME_PATH")).join("sonic2app.exe");
    let steam = k::Tables::read_exe(&std::fs::read(exe).unwrap()).unwrap();
    let gc =
        k::Tables::read_rel(&std::fs::read(env::var_os("SA2_CHAO_KARATE_REL").unwrap()).unwrap())
            .unwrap();
    assert_eq!(steam, gc);
    assert_eq!(steam.descriptors.len(), 27);
}

#[test]
#[ignore = "requires the user's GameCube ChaoStgKarate.rel (SA2_CHAO_KARATE_REL) and Steam install (SA2_GAME_PATH)"]
fn steam_motion_table_equals_the_gamecube_table() {
    let prs = std::fs::read(
        PathBuf::from(env::var_os("SA2_GAME_PATH").expect("SA2_GAME_PATH"))
            .join("resource/gd_PC/ChaoStgKarate.prs"),
    )
    .unwrap();
    let steam_rel = sa2_asset_tools::archive::decompress_prs(&prs, 16 << 20).unwrap();
    let steam = k::read_motion_table(&steam_rel).unwrap();
    let gc =
        k::read_motion_table(&std::fs::read(env::var_os("SA2_CHAO_KARATE_REL").unwrap()).unwrap())
            .unwrap();
    assert_eq!(steam, gc);
    // fn_72_FCC8 attack motions 4/5/7/8 carry their hit frames.
    assert_eq!(gc[4].events, vec![4.0, 6.0]);
    assert_eq!(gc[4].motion, 579);
}

#[test]
#[ignore = "requires the user's GameCube main.dol, ChaoMain.rel and ChaoStgKarate.rel and Python with Unicorn"]
fn attack_motion_handlers_match_the_gamecube_powerpc() {
    let python = env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg("-I")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/chao_karate_motion_ppc.py"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    if o.stdout.is_empty() {
        return;
    }
    let doc: Value = serde_json::from_slice(&o.stdout).unwrap();
    let rows =
        k::read_motion_table(&std::fs::read(env::var_os("SA2_CHAO_KARATE_REL").unwrap()).unwrap())
            .unwrap();
    for c in doc.as_array().unwrap() {
        let motion = i(&c["motion"]) as i8;
        let f0 = &c["flags0"];
        let mut aux = k::MotionAux {
            motion,
            bits: u(&f0[0]) as u8,
            ..Default::default()
        };
        let mut rec = StateRecord {
            flags: u(&f0[1]) as u16,
            ..Default::default()
        };
        let work_flags = u(&f0[2]) as u16;
        for fr in c["frames"].as_array().unwrap() {
            let input = k::MotionInput {
                frame: f(&fr["frame"]),
                linking: fr["linking"].as_bool().unwrap(),
                stopped: fr["stopped"].as_bool().unwrap(),
                ended: false,
            };
            let mut calls = vec![];
            if matches!(motion, 4 | 7) {
                k::light_attack(&mut aux, &mut rec, &rows, input, &mut calls);
            } else {
                k::heavy_attack(&mut aux, &mut rec, &rows, input, work_flags, &mut calls);
            }
            let got: Vec<Value> = calls
                .iter()
                .filter_map(|call| match *call {
                    k::MotionCall::StopMovement => None,
                    k::MotionCall::Link { motion, link_step } => {
                        Some(serde_json::json!(["link", motion, link_step]))
                    }
                    k::MotionCall::Speed(s) => Some(serde_json::json!(["speed", s.to_bits()])),
                    k::MotionCall::Skip(m) => Some(serde_json::json!(["skip", m])),
                    k::MotionCall::Sound(s) => Some(serde_json::json!(["sound", s])),
                })
                .collect();
            assert_eq!(Value::Array(got), fr["calls"], "calls {c}");
            let want = &fr["aux"];
            assert_eq!(
                [
                    i64::from(aux.motion),
                    i64::from(aux.step),
                    i64::from(aux.event),
                    i64::from(aux.bits),
                    i64::from(aux.row)
                ],
                [
                    i(&want[0]),
                    i(&want[1]),
                    i(&want[2]),
                    i(&want[3]),
                    i(&want[4])
                ],
                "aux {fr}"
            );
            assert_eq!(u64::from(rec.flags), u(&fr["twp"]), "twp {fr}");
        }
    }
}

#[test]
#[ignore = "requires the user's GameCube main.dol, ChaoMain.rel, ChaoStgKarate.rel, the local Gekko tables and Python with Unicorn"]
fn arena_edge_and_distance_match_the_gamecube_powerpc() {
    let python = env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg("-I")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/chao_karate_ring_ppc.py"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    if o.stdout.is_empty() {
        return;
    }
    let doc: Value = serde_json::from_slice(&o.stdout).unwrap();
    let v3 = |v: &Value| -> [f32; 3] { std::array::from_fn(|n| f(&v[n])) };
    let bits3 = |a: [f32; 3]| -> Vec<u64> { a.iter().map(|x| u64::from(x.to_bits())).collect() };
    for c in doc["edge"].as_array().unwrap() {
        let mut flags = u(&c["flags"]) as u16;
        let mut rec = StateRecord::default();
        let (mut vel, mut acc) = (v3(&c["vel"]), v3(&c["acc"]));
        let edge = k::arena_edge(
            &mut flags,
            &mut rec,
            v3(&c["pos"]),
            f(&c["bodyX"]),
            &mut vel,
            &mut acc,
            i(&c["phase"]) as i8,
            c["fighting"].as_bool().unwrap(),
            f(&c["sin"]),
            f(&c["cos"]),
        );
        assert_eq!(u64::from(edge != k::Edge::None), u(&c["ret"]), "return {c}");
        assert_eq!(edge == k::Edge::RingOut, u(&c["mgr3"]) == 2, "ring-out {c}");
        assert_eq!(u64::from(flags), u(&c["flagsOut"]), "work flags {c}");
        assert_eq!(u64::from(rec.flags), u(&c["twpOut"]), "state flags {c}");
        let want = |key: &str| -> Vec<u64> { c[key].as_array().unwrap().iter().map(u).collect() };
        assert_eq!(bits3(vel), want("velOut"), "velocity {c}");
        assert_eq!(bits3(acc), want("accOut"), "accel {c}");
    }
    for c in doc["distance"].as_array().unwrap() {
        let d = k::fighter_distance(v3(&c["a"]), v3(&c["b"]));
        assert_eq!(u64::from(d.to_bits()), u(&c["d"]), "distance {c}");
    }
}

#[test]
#[ignore = "requires the user's GameCube main.dol, ChaoMain.rel and ChaoStgKarate.rel and Python with Unicorn"]
fn approach_handler_matches_the_gamecube_powerpc() {
    let python = env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg("-I")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/chao_karate_approach_ppc.py"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    if o.stdout.is_empty() {
        return;
    }
    let doc: Value = serde_json::from_slice(&o.stdout).unwrap();
    let rows =
        k::read_motion_table(&std::fs::read(env::var_os("SA2_CHAO_KARATE_REL").unwrap()).unwrap())
            .unwrap();
    for c in doc["idle"].as_array().unwrap() {
        let mut aux = k::MotionAux {
            motion: i(&c["motion"]) as i8,
            bits: u(&c["bits0"]) as u8,
            ..Default::default()
        };
        let mut rec = StateRecord::default();
        for fr in c["frames"].as_array().unwrap() {
            let mut calls = vec![];
            let input = k::MotionInput {
                frame: 0.0,
                linking: fr["linking"].as_bool().unwrap(),
                stopped: false,
                ended: false,
            };
            k::idle(&mut aux, &mut rec, &rows, input, &mut calls);
            let got: Vec<Value> = calls
                .iter()
                .map(|call| match *call {
                    k::MotionCall::Link { motion, link_step } => {
                        serde_json::json!(["link", motion, link_step])
                    }
                    _ => unreachable!(),
                })
                .collect();
            assert_eq!(Value::Array(got), fr["calls"], "idle calls {fr}");
            assert_eq!(
                [i64::from(aux.step), i64::from(aux.bits), i64::from(aux.row)],
                [i(&fr["step"]), i(&fr["bits"]), i(&fr["row"])],
                "idle aux {fr}"
            );
            assert_eq!(
                u64::from(rec.flags & 0x8000),
                u(&fr["twp"]) & 0x8000,
                "idle end {fr}"
            );
        }
    }
    for c in doc["approach"].as_array().unwrap() {
        let mut aux = k::MotionAux {
            motion: 3,
            ..Default::default()
        };
        let mut rec = StateRecord::default();
        let mut first = true;
        for fr in c["frames"].as_array().unwrap() {
            let inp: Vec<f32> = fr["in"].as_array().unwrap().iter().map(f).collect();
            if first {
                rec.flags = 0;
                first = false;
            }
            let (mut vel, mut acc) = ([inp[0], inp[1], inp[2]], [inp[3], inp[4], inp[5]]);
            let mut calls = vec![];
            let input = k::MotionInput {
                frame: 0.0,
                linking: fr["linking"].as_bool().unwrap(),
                stopped: false,
                ended: false,
            };
            k::approach(
                &mut aux,
                &mut rec,
                &rows,
                input,
                &mut vel,
                &mut acc,
                || (f(&fr["sin"]), f(&fr["cos"])),
                || f(&fr["dist"]),
                &mut calls,
            );
            let got: Vec<Value> = calls
                .iter()
                .map(|call| match *call {
                    k::MotionCall::Link { motion, link_step } => {
                        serde_json::json!(["link", motion, link_step])
                    }
                    k::MotionCall::Sound(s) => serde_json::json!(["sound", s]),
                    _ => unreachable!(),
                })
                .collect();
            assert_eq!(Value::Array(got), fr["calls"], "calls {fr}");
            let a = &fr["aux"];
            assert_eq!(
                [
                    i64::from(aux.step),
                    i64::from(aux.timer),
                    i64::from(aux.row)
                ],
                [i(&a[0]), i(&a[1]), i(&a[2])],
                "aux {fr}"
            );
            assert_eq!(
                u64::from(rec.flags & 0x8000),
                u(&fr["twp"]) & 0x8000,
                "end {fr}"
            );
            let out: Vec<u64> = vel
                .iter()
                .chain(acc.iter())
                .map(|x| u64::from(x.to_bits()))
                .collect();
            let want: Vec<u64> = fr["out"].as_array().unwrap().iter().map(u).collect();
            assert_eq!(out, want, "movement {fr}");
        }
    }
}

#[test]
#[ignore = "requires the user's GameCube main.dol, ChaoMain.rel and ChaoStgKarate.rel and Python with Unicorn"]
fn hurt_motion_handler_matches_the_gamecube_powerpc() {
    let python = env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg("-I")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/chao_karate_hurt_ppc.py"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    if o.stdout.is_empty() {
        return;
    }
    let doc: Value = serde_json::from_slice(&o.stdout).unwrap();
    let rows =
        k::read_motion_table(&std::fs::read(env::var_os("SA2_CHAO_KARATE_REL").unwrap()).unwrap())
            .unwrap();
    for c in doc.as_array().unwrap() {
        let kn = &c["knock"];
        let mut aux = k::MotionAux {
            motion: i(&c["motion"]) as i8,
            knock: [f(&kn[0]), f(&kn[1]), f(&kn[2])],
            ..Default::default()
        };
        let mut rec = StateRecord {
            flags: u(&c["twp0"]) as u16,
            ..Default::default()
        };
        let work = u(&c["work"]) as u16;
        for fr in c["frames"].as_array().unwrap() {
            let inp: Vec<f32> = fr["in"].as_array().unwrap().iter().map(f).collect();
            let (mut vel, mut acc) = ([inp[0], inp[1], inp[2]], [inp[3], inp[4], inp[5]]);
            let input = k::MotionInput {
                frame: 0.0,
                linking: fr["linking"].as_bool().unwrap(),
                stopped: fr["stopped"].as_bool().unwrap(),
                ended: fr["ended"].as_bool().unwrap(),
            };
            let mut calls = vec![];
            k::hurt_motion(
                &mut aux, &mut rec, &rows, input, work, &mut vel, &mut acc, &mut calls,
            );
            let got: Vec<Value> = calls
                .iter()
                .map(|call| match *call {
                    k::MotionCall::Link { motion, link_step } => {
                        serde_json::json!(["link", motion, link_step])
                    }
                    k::MotionCall::Sound(s) => serde_json::json!(["sound", s]),
                    _ => unreachable!(),
                })
                .collect();
            assert_eq!(Value::Array(got), fr["calls"], "calls {fr}");
            let a = &fr["aux"];
            assert_eq!(
                [
                    i64::from(aux.motion),
                    i64::from(aux.step),
                    i64::from(aux.event),
                    i64::from(aux.timer),
                    i64::from(aux.row)
                ],
                [i(&a[0]), i(&a[1]), i(&a[2]), i(&a[3]), i(&a[4])],
                "aux {fr}"
            );
            assert_eq!(u64::from(rec.flags), u(&fr["twp"]), "twp {fr}");
            let out: Vec<u64> = vel
                .iter()
                .chain(acc.iter())
                .map(|x| u64::from(x.to_bits()))
                .collect();
            let want: Vec<u64> = fr["out"].as_array().unwrap().iter().map(u).collect();
            assert_eq!(out, want, "movement {fr}");
            if aux.motion == 0x28 {
                break;
            }
        }
    }
}
