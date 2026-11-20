//! Compare `stage_objects` with stg42D.rel's own object code run under the
//! main.dol harness (tests/stage_objects_ppc.py), case by case and frame by
//! frame: the events the REL code calls out to and the task state it leaves.
use sa2_asset_tools::stage_objects::{
    colli_prim::{self, Primitive, PrimitiveTask},
    host::{Event, ObjectHost},
};
use serde_json::{json, Value};
use std::{path::PathBuf, process::Command};

pub fn oracle(object: &str) -> Option<Value> {
    let python = std::env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/stage_objects_ppc.py"))
        .arg(object)
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
fn bits(v: [f32; 3]) -> Value {
    json!(v.map(f32::to_bits))
}

/// Records events in the oracle's JSON shape.
#[derive(Default)]
struct Recorder {
    out: bool,
    events: Vec<Value>,
    pending_sizes: Option<[f32; 3]>,
}

impl ObjectHost for Recorder {
    fn range_out(&mut self) -> bool {
        self.events.push(json!(["range_out"]));
        self.out
    }
    fn range_out_with_r(&mut self, r: f32) -> bool {
        self.events.push(json!(["range_out_r", r.to_bits()]));
        self.out
    }
    fn event(&mut self, e: Event) {
        match e {
            Event::CollisionInit { rows, count, list } => {
                self.events.push(json!(["init", rows, count, list]))
            }
            Event::CollisionSizes(s) => self.pending_sizes = Some(s),
            Event::CollisionCalcRange => {
                let s = self
                    .pending_sizes
                    .take()
                    .expect("sizes before CCL_CalcRange");
                self.events.push(json!(["calc", bits(s)]))
            }
            Event::CollisionEntry => self.events.push(json!(["entry"])),
        }
    }
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_GC_REL_DIR and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn collision_primitives_match_stg42d() {
    let Some(c) = oracle("colli_prim") else {
        return;
    };
    // The CollisionData rows of stg42D (section 5).
    let row = |k: Primitive| match k {
        Primitive::Sphere => 0xdb80,
        Primitive::Cylinder => 0xdbb0,
        Primitive::Cube => 0xdbe0,
        Primitive::Wall => 0xdc10,
        Primitive::Circle => 0xdc40,
    };
    let cases = c["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 300);
    for case in cases {
        let kind = match case["kind"].as_str().unwrap() {
            "sphere" => Primitive::Sphere,
            "cylinder" => Primitive::Cylinder,
            "cube" => Primitive::Cube,
            "wall" => Primitive::Wall,
            _ => Primitive::Circle,
        };
        let s = &case["scale"];
        let mut task = PrimitiveTask {
            mode: case["mode"].as_i64().unwrap() as i8,
            scale: [f(&s[0]), f(&s[1]), f(&s[2])],
        };
        for fr in case["frames"].as_array().unwrap() {
            let mut host = Recorder {
                out: fr["out"].as_bool().unwrap(),
                ..Default::default()
            };
            colli_prim::exec(kind, &mut task, row(kind), &mut host);
            assert_eq!(json!(host.events), fr["events"], "{case}");
            assert_eq!(i64::from(task.mode), fr["mode"].as_i64().unwrap(), "{case}");
        }
    }
}
