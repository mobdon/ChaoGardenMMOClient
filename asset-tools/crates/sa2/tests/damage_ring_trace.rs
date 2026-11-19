use sa2_asset_tools::damage_ring::*;
use std::{env, fs, path::PathBuf, process::Command};
fn trig(a: i32) -> (f32, f32) {
    ((a % 65536) as f32 * (std::f32::consts::TAU / 65536.)).sin_cos()
}
#[test]
fn damage_rules_and_face_precedence() {
    assert!(scatter(0, 7, 0, [0.; 3], 0.).unwrap().is_empty());
    assert_eq!(
        scatter(1, 0, 0, [0.; 3], 0.).unwrap(),
        [DamageCall::KillPlayer(1), DamageCall::AfterKill]
    );
    let calls = scatter(0, 0, 200, [0.; 3], 0.).unwrap();
    assert_eq!(calls[0], DamageCall::DebitRings(0, -200));
    assert_eq!(calls.len(), 22);
    assert_eq!(gravity_face([1., -1., 1.]), 5);
    assert_eq!(gravity_face([0., 0.9, 0.]), 0);
    let mut c = ProbeClock::default();
    for i in 0..20 {
        assert_eq!(c.next_delay(), i % 9);
    }
}
#[test]
#[ignore = "requires pinned native source and C compiler"]
fn compare_native_damage_and_flight() {
    let repo = PathBuf::from(env::var_os("SA2B_REFERENCE").unwrap());
    let rev = Command::new("git")
        .current_dir(&repo)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(rev.stdout).unwrap().trim(),
        "874a480379260dce01e8988d6ab13c0deddcee4c"
    );
    let source = fs::read_to_string(repo.join("src/OBJECT/o_ring.c")).unwrap();
    let start = source
        .find("void DamegeRingScatter(s32 playerIndex) {")
        .unwrap();
    let end = source[start..]
        .find("static void RingEnd(task *tp) {")
        .unwrap()
        + start;
    let tmp = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .join(".local")
        .join(format!("sa2-ring-flight-{}", std::process::id()));
    fs::create_dir_all(&tmp).unwrap();
    let src = tmp.join("oracle.c");
    let exe = tmp.join(format!("oracle{}", env::consts::EXE_SUFFIX));
    fs::write(
        &src,
        format!(
            "{}\n{}\n{}",
            include_str!("damage_ring_host.h"),
            &source[start..end],
            include_str!("damage_ring_driver.c")
        ),
    )
    .unwrap();
    let mut cc = Command::new(env::var_os("CC").unwrap_or_else(|| "cc".into()));
    if env::var_os("SA2B_ZIG_CC").is_some() {
        cc.arg("cc");
    }
    assert!(cc
        .arg("-ffp-contract=off")
        .arg(&src)
        .arg("-o")
        .arg(&exe)
        .arg("-lm")
        .status()
        .unwrap()
        .success());
    let output = Command::new(&exe).output().unwrap();
    assert!(output.status.success());
    let trace = String::from_utf8(output.stdout).unwrap();
    let mut lines = trace.lines();
    for ch in 0..8 {
        for balance in 0..26 {
            let calls = scatter(0, ch, balance, [1., 2., 3.], 0.75).unwrap();
            let (mut dead, mut after, mut debit, mut sound) = (0, 0, 0, 0);
            let mut angles = vec![];
            for c in calls {
                match c {
                    DamageCall::KillPlayer(0) => dead += 1,
                    DamageCall::AfterKill => after += 1,
                    DamageCall::DebitRings(0, n) => debit = n,
                    DamageCall::Sound(0x8014) => sound += 1,
                    DamageCall::CreateDroppedRing {
                        slot: 0,
                        position,
                        yaw,
                    } => {
                        assert_eq!(position, [1., 2., 3.]);
                        angles.push(yaw);
                    }
                    _ => panic!(),
                }
            }
            let mut v = vec![dead, after, debit, sound, angles.len() as i32];
            v.extend(angles);
            assert_eq!(
                v,
                lines
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .skip(1)
                    .map(|s| s.parse::<i32>().unwrap())
                    .collect::<Vec<_>>()
            );
        }
    }
    for scenario in 0..5 {
        let g = [0., -1., 0.];
        let mut r = DroppedRing::launch(
            [0., 4., 0.],
            0,
            g,
            0,
            0,
            0.75,
            scenario == 2,
            &mut ProbeClock::default(),
            trig,
        )
        .unwrap();
        for frame in 0..460 {
            let before = r.phase;
            let hit =
                if (matches!(scenario, 1 | 3) && frame == 50) || (scenario == 2 && frame == 250) {
                    Some((0, if scenario == 3 { 0x4000 } else { 0 }))
                } else {
                    None
                };
            let mut query = None;
            let tick = r.tick(
                g,
                frame % 11 == 0,
                frame == 2,
                hit,
                |p, face| {
                    assert_eq!(face, 2);
                    query = Some(p);
                    (frame % 3 != 0).then_some(Surface {
                        coordinate: 0.,
                        pitch: if scenario == 4 { 2048 } else { 0 },
                        roll: if scenario == 4 { 1024 } else { 0 },
                    })
                },
                trig,
            );
            let v = lines
                .next()
                .unwrap()
                .split_whitespace()
                .skip(1)
                .map(|n| n.parse::<f64>().unwrap())
                .collect::<Vec<_>>();
            let phase = if tick.destroy { before } else { r.phase };
            let mode = match phase {
                Phase::Flying => 1,
                Phase::Resting => 2,
                Phase::Expired => 3,
                Phase::Collected => 4,
                Phase::Destroyed => panic!(),
            };
            let ints = [
                scenario,
                frame,
                mode,
                r.timer.into(),
                r.age.into(),
                tick.register_at.is_some() as i32,
                tick.particles as i32,
                tick.destroy as i32,
                tick.credit_slot.map_or(-1, i32::from),
                query.is_some() as i32,
            ];
            assert_eq!(
                v[..10],
                ints.map(f64::from),
                "scenario {scenario} frame {frame}"
            );
            let actual = r
                .flight
                .position
                .into_iter()
                .chain(r.flight.velocity)
                .chain(tick.register_at.unwrap_or([0.; 3]))
                .chain(query.unwrap_or([0.; 3]))
                .chain([r.spin]);
            for (i, (a, b)) in actual.zip(&v[10..]).enumerate() {
                assert!(
                    (f64::from(a) - b).abs() < 0.002,
                    "scenario {scenario} frame {frame} field {i}: {a} vs {b}"
                );
            }
            if tick.destroy {
                break;
            }
        }
    }
    assert!(lines.next().is_none());
    fs::remove_file(src).unwrap();
    fs::remove_file(exe).unwrap();
    let pdb = tmp.join("oracle.pdb");
    if pdb.exists() {
        fs::remove_file(pdb).unwrap();
    }
    fs::remove_dir(tmp).unwrap();
}
#[test]
#[ignore = "requires verified installed City Escape REL"]
fn city_escape_floor_adapter() {
    let file = PathBuf::from(env::var_os("SA2_SET_DIR").unwrap()).join("stg13D.rel");
    let stage =
        sa2_asset_tools::stage_land::read_named(&fs::read(file).unwrap(), "objLandTable0013")
            .unwrap();
    let request = serde_json::json!({"slot":0,"base_character":0,"rings":34,"position":[4846.6666667,-10025.,5081.6666667],
      "scatter_random":0.75,"ring_index":0,"spin_random":0.75,"frames":422,"probe_depth":150.});
    let result =
        sa2_asset_tools::damage_ring_stage::replay(&stage, &serde_json::to_vec(&request).unwrap())
            .unwrap();
    let frames = result["frames"].as_array().unwrap();
    assert_eq!(frames.len(), 422);
    assert_eq!(
        frames
            .iter()
            .filter(|f| !f["staticProbe"]["hit"].is_null())
            .count(),
        27
    );
    assert_eq!(
        frames
            .iter()
            .filter(|f| f["events"]["bounce"] == true)
            .count(),
        166
    );
    assert_eq!(frames[240]["task"]["phase"], "Resting");
    assert_eq!(frames[420]["task"]["phase"], "Expired");
    assert_eq!(frames[421]["task"]["phase"], "Destroyed");
    assert!(frames.iter().all(|f| f["events"]["credit_slot"].is_null()));
}
