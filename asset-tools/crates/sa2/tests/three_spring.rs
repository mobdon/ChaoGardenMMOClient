//! The triple spring (3SPRING) against the installed City Escape module.
use sa2_asset_tools::three_spring::{Call, Engine, Program, ThreeSpring};
use serde_json::{json, Value};
use std::{env, fs, path::PathBuf, process::Command};

fn gd() -> PathBuf {
    PathBuf::from(env::var_os("SA2_GAME_PATH").expect("SA2_GAME_PATH")).join("resource/gd_PC")
}

#[test]
#[ignore = "requires the installed City Escape module"]
fn reads_city_escape() {
    let p = Program::read("stg13D.rel", &fs::read(gd().join("stg13D.rel")).unwrap()).unwrap();
    assert_eq!(p.init, 0x22764);
    assert_eq!(p.velocity, [0.0, 5.0, 0.0]);
    assert_eq!(
        (p.z_target, p.z_pull.to_bits(), p.lift),
        (8.0, 0x3e99999a, 5.0)
    );
    assert_eq!(
        (p.nocon_frames, p.sound, p.rumble, p.cooldown),
        (30, 0x1000, [4, 15, 0], 10)
    );
    assert_eq!((p.work_size, p.collision_flag), (12, 0x40));
    assert_eq!(
        (p.kick, p.damp.to_bits(), p.spring),
        (1.0, 0x3f733333, 0.25)
    );
}

struct Scripted<'a>(&'a Value);

impl Engine for Scripted<'_> {
    fn clipped(&mut self) -> bool {
        self.0["clipped"].as_bool().unwrap()
    }
    fn allocate(&mut self) -> bool {
        self.0["alloc"].as_bool().unwrap()
    }
    fn hit(&mut self) -> bool {
        self.0["hit"].as_bool().unwrap()
    }
    fn hit_slot(&mut self) -> i8 {
        self.0["slot"].as_i64().unwrap() as i8
    }
    fn player_position(&mut self, _: i8) -> [f32; 3] {
        [0, 1, 2].map(|i| f32::from_bits(self.0["player"][i].as_u64().unwrap() as u32))
    }
    fn calc_point(&mut self, l: [f32; 3]) -> [f32; 3] {
        [l[0] + 3.0, l[1] * 2.0, l[2] - 1.0]
    }
}

fn call_json(c: &Call) -> Value {
    let b = |v: [f32; 3]| v.map(f32::to_bits);
    match *c {
        Call::Allocate { size } => json!({"call": "allocate", "size": size}),
        Call::InitCollision { .. } => json!({"call": "initCollision"}),
        Call::FreeWork => json!({"call": "freeWork"}),
        Call::EntryCollision => json!({"call": "entryCollision"}),
        Call::PlayerPosition { slot } => json!({"call": "playerPosition", "slot": slot}),
        Call::PushMatrix => json!({"call": "pushMatrix"}),
        Call::UnitMatrix => json!({"call": "unitMatrix"}),
        Call::Translate { position } => json!({"call": "translate", "position": b(position)}),
        Call::Rotate { import, angle } => {
            json!({"call": "rotate", "import": import, "angle": angle})
        }
        Call::InvertMatrix => json!({"call": "invertMatrix"}),
        Call::CalcPoint { local } => json!({"call": "calcPoint", "local": b(local)}),
        Call::PopMatrix => json!({"call": "popMatrix"}),
        Call::SetPosition { slot, position } => {
            json!({"call": "setPosition", "slot": slot, "position": b(position)})
        }
        Call::SetVelocity {
            slot,
            velocity,
            angles,
            frames,
        } => {
            json!({"call": "setVelocity", "slot": slot, "velocity": b(velocity), "angles": angles, "frames": frames})
        }
        Call::Sound(code) => json!({"call": "sound", "code": code}),
        Call::Rumble { slot, args } => json!({"call": "rumble", "slot": slot, "args": args}),
    }
}

fn state(s: &ThreeSpring, calls: &[Call], flags: u16) -> Value {
    json!({
        "calls": calls.iter().map(call_json).collect::<Vec<_>>(),
        "work": s.work.as_ref().map(|w| json!({"bounce": w.bounce.map(f32::to_bits), "cooldown": w.cooldown})),
        "colliFlags": flags,
    })
}

#[test]
#[ignore = "requires the installed City Escape module and Python with Unicorn PPC"]
fn matches_installed_powerpc() {
    let python = env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let rel =
        PathBuf::from(env::var_os("SA2_GC_REL_DIR").expect("SA2_GC_REL_DIR")).join("stg13D.rel");
    let o = Command::new(python)
        .arg("-I")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/three_spring_ppc.py"))
        .arg(&rel)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let doc: Value = serde_json::from_slice(&o.stdout).unwrap();
    let p = Program::read("stg13D.rel", &fs::read(&rel).unwrap()).unwrap();
    assert_eq!(doc["init"].as_u64(), Some(p.init as u64));
    let (mut runs, mut launches) = (0, 0);
    for (ci, case) in doc["cases"].as_array().unwrap().iter().enumerate() {
        let f = |k: &str| [0, 1, 2].map(|i| f32::from_bits(case[k][i].as_u64().unwrap() as u32));
        let angles = [0, 1, 2].map(|i| case["angles"][i].as_i64().unwrap() as i32);
        let mut s = ThreeSpring::new(f("position"), angles, f("scale"));
        let mut flags = 0u16;
        let native = case["native"].as_array().unwrap();
        for (ri, (r, n)) in case["runs"]
            .as_array()
            .unwrap()
            .iter()
            .zip(native)
            .enumerate()
        {
            let calls = s.step(&p, &mut Scripted(r));
            for c in &calls {
                if let Call::InitCollision { flag } = c {
                    flags |= flag;
                }
            }
            launches += calls
                .iter()
                .filter(|c| matches!(c, Call::SetVelocity { .. }))
                .count();
            assert_eq!(state(&s, &calls, flags), *n, "case {ci} run {ri}: {r}");
            runs += 1;
        }
        if s.work.is_some() {
            let calls = s.destroy();
            assert_eq!(
                state(&s, &calls, flags),
                *native.last().unwrap(),
                "case {ci} destructor"
            );
        }
    }
    assert!(launches > 50);
    eprintln!(
        "{runs} 3SPRING updates ({launches} launches) and destructors matched installed PowerPC"
    );
}
