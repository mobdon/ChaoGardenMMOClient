//! RING_LINEAR / RING_CIRCLE against the installed stage modules.
use sa2_asset_tools::ring_group::{Call, Engine, Kind, Program, RingGroup};
use serde_json::{json, Value};
use std::{collections::VecDeque, env, fs, path::PathBuf, process::Command};

fn gd() -> PathBuf {
    PathBuf::from(env::var_os("SA2_GAME_PATH").expect("SA2_GAME_PATH")).join("resource/gd_PC")
}

#[test]
#[ignore = "requires the installed stage modules"]
fn reads_city_escape_and_every_module_with_the_same_code() {
    let p = Program::read("stg13D.rel", &fs::read(gd().join("stg13D.rel")).unwrap()).unwrap();
    assert_eq!((p.linear_init, p.circle_init), (0x27f08, 0x2837c));
    assert_eq!(p.spacing_base, 10.0);
    assert_eq!(
        (p.degrees, p.one, p.linear_span, p.work_size),
        (360.0, 1.0, 0x4000, 0xa0)
    );
    assert_eq!(p.bams_per_degree.to_bits(), 0x43360b61);
    let mut found = 0;
    for e in fs::read_dir(gd()).unwrap() {
        let path = e.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap().to_string();
        if name.starts_with("stg") && name.ends_with("D.rel") {
            found += usize::from(Program::read(&name, &fs::read(&path).unwrap()).is_ok());
        }
    }
    eprintln!("{found} modules carry the same ring-group code at the same offsets");
    assert!(found >= 1);
}

/// The oracle's deterministic stand-ins for the engine services.
struct Scripted {
    clipped: VecDeque<bool>,
    spawn: VecDeque<bool>,
    alloc: bool,
    state: Option<i8>,
    has_set: bool,
    next: u32,
}

impl Engine for Scripted {
    fn clipped(&mut self) -> bool {
        self.clipped.pop_front().unwrap()
    }
    fn allocate(&mut self) -> bool {
        self.alloc
    }
    fn set_state(&mut self) -> Option<i8> {
        self.state
    }
    fn sine(&mut self, a: i32) -> f32 {
        a.rem_euclid(4096) as f32 / 4096.0 - 0.5
    }
    fn cosine(&mut self, a: i32) -> f32 {
        (a + 1000).rem_euclid(2048) as f32 / 2048.0
    }
    fn calc_point(&mut self, l: [f32; 3]) -> [f32; 3] {
        [l[0] + 1.5, l[1] * 2.0, l[2] - 0.25]
    }
    fn spawn_ring(&mut self) -> Option<u32> {
        self.next += 1;
        self.spawn.pop_front().unwrap().then_some(self.next)
    }
    fn has_set_entry(&self) -> bool {
        self.has_set
    }
}

fn calls_json(calls: &[Call]) -> Vec<Value> {
    let mut out = vec![];
    for c in calls {
        out.push(match *c {
            Call::Allocate { size } => json!({"call": "allocate", "size": size}),
            Call::ReadSetState => json!({"call": "readSetState"}),
            Call::PushMatrix => json!({"call": "pushMatrix"}),
            Call::UnitMatrix => json!({"call": "unitMatrix"}),
            Call::Translate { position } => {
                json!({"call": "translate", "position": position.map(f32::to_bits)})
            }
            Call::Rotate { import, angle } => {
                json!({"call": "rotate", "import": import, "angle": angle})
            }
            Call::Sine { angle } => json!({"call": "sine", "angle": angle}),
            Call::Cosine { angle } => json!({"call": "cosine", "angle": angle}),
            Call::CalcPoint { local } => {
                json!({"call": "calcPoint", "local": local.map(f32::to_bits)})
            }
            Call::PopMatrix => json!({"call": "popMatrix"}),
            Call::SpawnRing { .. } => json!({"call": "spawnRing"}),
            Call::MarkSetEntryDone => json!({"call": "markSetEntryDone"}),
            Call::FreeTask => json!({"call": "freeTask", "self": true}),
            Call::UnlinkAndFreeRing { .. } => json!({"call": "freeTask", "self": false}),
            Call::SaveSetState { mask } => json!({"call": "saveSetState", "mask": mask}),
            Call::FreeWork => json!({"call": "freeWork"}),
        });
    }
    out
}

fn state_json(g: &RingGroup) -> Value {
    match &g.entries {
        None => json!({"count": g.count, "entries": null, "positions": null}),
        Some(e) => json!({
            "count": g.count,
            "entries": e.iter().map(|x| json!([x.flags, x.child.is_some()])).collect::<Vec<_>>(),
            "positions": g.positions.iter().map(|p| p.map(f32::to_bits)).collect::<Vec<_>>(),
        }),
    }
}

#[test]
#[ignore = "requires the installed City Escape module and Python with Unicorn PPC"]
fn matches_installed_powerpc() {
    let python = env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let rel = gd().join("stg13D.rel");
    let o = Command::new(python)
        .arg("-I")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/ring_group_ppc.py"))
        .arg(&rel)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let doc: Value = serde_json::from_slice(&o.stdout).unwrap();
    let p = Program::read("stg13D.rel", &fs::read(&rel).unwrap()).unwrap();
    assert_eq!(
        (doc["linear"].as_u64(), doc["circle"].as_u64()),
        (Some(p.linear_init as u64), Some(p.circle_init as u64))
    );
    let (mut runs, mut rings) = (0, 0);
    for (ci, case) in doc["cases"].as_array().unwrap().iter().enumerate() {
        let bits = |k: &str| -> [f32; 3] {
            let v: Vec<u32> = serde_json::from_value(case[k].clone()).unwrap();
            [0, 1, 2].map(|i| f32::from_bits(v[i]))
        };
        let angles: Vec<u32> = serde_json::from_value(case["angles"].clone()).unwrap();
        let kind = if case["kind"] == "linear" {
            Kind::Linear
        } else {
            Kind::Circle
        };
        let mut g = RingGroup::new(
            kind,
            bits("position"),
            [angles[0], angles[1], angles[2]],
            bits("scale"),
        );
        let mut e = Scripted {
            clipped: VecDeque::new(),
            spawn: VecDeque::new(),
            alloc: case["alloc"].as_bool().unwrap(),
            state: Some(case["state"].as_u64().unwrap() as u8 as i8),
            has_set: case["hasSet"].as_bool().unwrap(),
            next: 0,
        };
        let native = case["native"].as_array().unwrap();
        let rs = case["runs"].as_array().unwrap();
        for (ri, (r, n)) in rs.iter().zip(native).enumerate() {
            if let Some(i) = r["collect"].as_u64() {
                let x = &mut g.entries.as_mut().unwrap()[i as usize];
                x.flags |= 2;
                x.child = None;
            }
            e.clipped = VecDeque::from([r["clipped"].as_bool().unwrap()]);
            e.spawn = r["spawn"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_bool().unwrap())
                .collect();
            let calls = g.step(&p, &mut e);
            rings += calls
                .iter()
                .filter(|c| matches!(c, Call::SpawnRing { .. }))
                .count();
            assert_eq!(
                e.clipped.is_empty(),
                r["clippedUsed"].as_bool().unwrap(),
                "case {ci} run {ri}"
            );
            assert_eq!(
                8 - e.spawn.len() as u64,
                r["spawnUsed"].as_u64().unwrap(),
                "case {ci} run {ri}"
            );
            let mut rust = state_json(&g);
            rust["calls"] = Value::from(calls_json(&calls));
            assert_eq!(rust, *n, "case {ci} run {ri}");
            runs += 1;
        }
        let calls = g.destroy(&mut e);
        let mut rust = state_json(&g);
        rust["calls"] = Value::from(calls_json(&calls));
        assert_eq!(rust, *native.last().unwrap(), "case {ci} destructor");
    }
    eprintln!("{runs} ring-group runs ({rings} ring spawns) and every destructor matched installed PowerPC");
}
