//! The PAD path against main.dol's own routines under Unicorn
//! (tests/input_ppc.py): recorded pad statuses go through `read` and `tick`,
//! and the events and every PAD table are compared bit for bit after each op.
//! Skips without SA2_MAIN_DOL.
mod common;
use common::{event_json, oracle};
use sa2_asset_tools::input::PadStatus;
use sa2_runtime::input::RECORD;
use sa2_runtime::{PadContext, PadSystem};
use serde_json::{json, Value};

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
        .collect()
}

fn pad(v: &Value) -> PadStatus {
    let n = |i: usize| v[i].as_i64().expect("int");
    PadStatus {
        button: n(0) as u16,
        stick: [n(1) as i8, n(2) as i8],
        substick: [n(3) as i8, n(4) as i8],
        triggers: [n(5) as u8, n(6) as u8],
        analog_a: n(7) as u8,
        analog_b: n(8) as u8,
        err: n(9) as i8,
    }
}

fn pack(p: &PadStatus) -> Vec<u8> {
    let mut b = p.button.to_be_bytes().to_vec();
    b.extend([
        p.stick[0] as u8,
        p.stick[1] as u8,
        p.substick[0] as u8,
        p.substick[1] as u8,
    ]);
    b.extend([
        p.triggers[0],
        p.triggers[1],
        p.analog_a,
        p.analog_b,
        p.err as u8,
        0,
    ]);
    b
}

fn state(s: &PadSystem) -> Value {
    let pads = |a: &[PadStatus; 4]| hex(&a.iter().flat_map(pack).collect::<Vec<_>>());
    let flat = |r: &[[u8; RECORD]]| hex(&r.concat());
    let mut sticks = vec![];
    for (a, m) in s.raw_sticks.iter().chain(s.sticks.iter()) {
        sticks.extend(a.to_be_bytes());
        sticks.extend(m.to_bits().to_be_bytes());
    }
    json!({
        "prev": pads(&s.prev), "cur": pads(&s.cur), "records": flat(&s.records),
        "status": s.status, "pointers": s.pointers.map(|p| p.addr()),
        "connected": s.connected, "port_index": s.port_index, "copies": flat(&s.copies),
        "sticks": hex(&sticks), "pushed": hex(&s.pushed),
    })
}

#[test]
fn pad_path_matches_main_dol() {
    let Some(out) = oracle("input_ppc.py") else {
        return;
    };
    let mut compared = 0;
    for sc in out["scenarios"].as_array().expect("scenarios") {
        let name = sc["name"].as_str().expect("name");
        let init = &sc["init"];
        let mut s = PadSystem::default();
        let rec = unhex(init["records"].as_str().expect("records"));
        for (p, r) in s.records.iter_mut().enumerate() {
            r.copy_from_slice(&rec[p * RECORD..(p + 1) * RECORD]);
        }
        s.idle
            .copy_from_slice(&unhex(init["idle"].as_str().expect("idle")));
        for (i, c) in init["port_config"]
            .as_array()
            .expect("cfg")
            .iter()
            .enumerate()
        {
            s.port_config[i] = c.as_u64().expect("u32") as u32;
        }
        let mut ctx = PadContext {
            g: vec![0; 0xF0],
            ..PadContext::default()
        };
        let ops = sc["ops"].as_array().expect("ops");
        let steps = sc["steps"].as_array().expect("steps");
        for (k, (op, step)) in ops.iter().zip(steps).enumerate() {
            let mut events = vec![];
            match op["op"].as_str().expect("op") {
                "state" => {
                    if let Some(g) = op["g"].as_object() {
                        for (o, v) in g {
                            ctx.g[o.parse::<usize>().expect("off")] = v.as_u64().expect("b") as u8;
                        }
                    }
                    if let Some(v) = op["camera_active"].as_i64() {
                        ctx.camera_active = v != 0;
                    }
                    if let Some(v) = op["viewports"].as_i64() {
                        ctx.viewports = v as i32;
                    }
                    if let Some(a) = op["camera_angles"].as_array() {
                        for (i, v) in a.iter().enumerate() {
                            ctx.camera_angles[i] = v.as_u64().expect("u32") as u32;
                        }
                    }
                    if let Some(a) = op["port_config"].as_array() {
                        for (i, v) in a.iter().enumerate() {
                            s.port_config[i] = v.as_u64().expect("u32") as u32;
                        }
                    }
                }
                "read" => {
                    let pads = op["pads"].as_array().expect("pads");
                    s.read(std::array::from_fn(|i| pad(&pads[i])), &mut events);
                }
                "tick" => s.tick(&ctx, &mut events),
                other => panic!("unknown op {other}"),
            }
            let ev: Vec<Value> = events.iter().map(event_json).collect();
            assert_eq!(
                Value::Array(ev),
                step["events"],
                "{name} op {k} {op}: events"
            );
            let got = state(&s);
            for key in [
                "prev",
                "cur",
                "records",
                "status",
                "pointers",
                "connected",
                "port_index",
                "copies",
                "sticks",
                "pushed",
            ] {
                assert_eq!(got[key], step["state"][key], "{name} op {k} {op}: {key}");
            }
            compared += 1;
        }
    }
    eprintln!("compared {compared} ops against main.dol");
    assert!(compared > 50);
}
