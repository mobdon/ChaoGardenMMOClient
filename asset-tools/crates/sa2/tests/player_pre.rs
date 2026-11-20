//! Compare `player_pre::pre_switch` with main.dol's fn_80056450
//! (tests/player_pre_ppc.py).
use sa2_asset_tools::{
    ninja_matrix::MatrixStack,
    player_pre::{pre_switch, OtherTask, PreCcl, PreGame, PreSteps, PreWork},
    player_shadow::ShadowBlock,
};
use serde_json::{json, Value};
use std::{path::PathBuf, process::Command};

fn i(v: &Value) -> i64 {
    v.as_i64().unwrap()
}

fn f(v: &Value) -> f32 {
    f32::from_bits(v.as_u64().unwrap() as u32)
}

fn v3(v: &Value) -> [f32; 3] {
    [0, 1, 2].map(|k| f(&v[k]))
}

fn i3(v: &Value) -> [i32; 3] {
    [0, 1, 2].map(|k| i(&v[k]) as i32)
}

const OT: usize = 0x8111_0000;

struct Ccl<'a> {
    st: &'a Value,
    other: OtherTask,
    log: Vec<Value>,
}

impl PreCcl for Ccl<'_> {
    fn kind_hit(&mut self, kind: u8) -> Option<OtherTask> {
        self.log.push(json!(["kind", kind]));
        self.st["kinds"][kind.to_string()]
            .as_bool()
            .unwrap_or(false)
            .then_some(self.other)
    }
    fn hit_63f4(&mut self) -> bool {
        self.log.push(json!(["h63f4"]));
        self.st["h63f4"].as_bool().unwrap()
    }
    fn hit_6270(&mut self) -> bool {
        self.log.push(json!(["h6270"]));
        self.st["h6270"].as_bool().unwrap()
    }
    fn first_hit(&mut self) -> Option<OtherTask> {
        self.log.push(json!(["first"]));
        Some(self.other)
    }
}

struct Steps(Vec<Value>);

impl PreSteps for Steps {
    fn settle(&mut self, _: &mut PreWork) {
        self.0.push(json!(["settle", 0x174]));
    }
    fn stand(&mut self, w: &mut PreWork) {
        self.0.push(json!(["stand", w.mode, w.p_h18]));
    }
    fn motion(&mut self, _: &mut PreWork) {
        self.0.push(json!(["motion", 0x174]));
    }
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, the local Gekko tables and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn pre_switch_matches_main_dol() {
    let python = std::env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/player_pre_ppc.py"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    if o.stdout.is_empty() {
        eprintln!("skipped: {}", String::from_utf8_lossy(&o.stderr));
        return;
    }
    let c: Value = serde_json::from_slice(&o.stdout).unwrap();
    let mut stack = MatrixStack::new();
    let mut snapped = 0;
    for (ci, case) in c["cases"].as_array().unwrap().iter().enumerate() {
        let (w0, g0, o0) = (&case["w"], &case["g"], &case["o"]);
        let h = |k: usize| i(&w0["here"][k]);
        let mut w = PreWork {
            mode: i(&w0["mode"]) as i8,
            task_b1: i(&w0["b1"]) as u8,
            flag: i(&w0["flag"]) as u16 as i16,
            task_h6: 7,
            angles: i3(&w0["ang"]),
            pos: v3(&w0["pos"]),
            info_attr: i(&w0["info"]) as u32,
            mw_spd: v3(&w0["mwspd"]),
            mw_ang: i3(&w0["mwang"]),
            pnum: i(&w0["pnum"]) as i8,
            character: i(&w0["c"]) as i8,
            p_bc: 9,
            p_w28: i(&w0["w28"]) as i32,
            attr: 0x77,
            p_w9c: 5,
            p_wa0: 6,
            ride: Some(0x1234_5678),
            p_h10: i(&w0["h10"]) as i16,
            p_h18: 9,
            action: 0x22,
            spd: [1.0, 2.0, 3.0],
            f138: f(&w0["f138"]),
            here: ShadowBlock {
                angle_x: h(0) as i32,
                angle_z: h(1) as i32,
                ceiling_flags: h(2) as u32,
                floor_flags: h(3) as u32,
                ceiling_y: f32::from_bits(h(4) as u32),
                floor_y: f32::from_bits(h(5) as u32),
                last_ceiling_flags: h(6) as u32,
                last_floor_flags: h(7) as u32,
            },
            pwp_h180_4: false,
        };
        let g = PreGame {
            b1e: i(&g0["b1e"]) as i8,
            b39: i(&g0["b39"]) as i8,
            b42: i(&g0["b42"]) as u8,
            b49: i(&g0["b49"]) as i8,
            b3f: i(&g0["b3f"]) as i8,
            gravity_mode: i(&g0["gmode"]) as i32,
            level: (i(&g0["level"][0]) as i32, i(&g0["level"][1]) as i32),
            gravity: v3(&g0["grav"]),
        };
        let mut ccl = Ccl {
            st: &case["st"],
            other: OtherTask {
                id: OT,
                pos: v3(&o0["pos"]),
                yaw: i(&o0["yaw"]) as i32,
                angle_z: i(&o0["z"]) as i32,
                info_attr: i(&o0["info"]) as u32,
            },
            log: vec![],
        };
        let mut steps = Steps(vec![]);
        pre_switch(&mut stack, &mut w, &g, &mut ccl, &mut steps);
        let mut log = ccl.log;
        log.extend(steps.0);
        let out = &case["out"];
        let bits = |v: [f32; 3]| v.map(f32::to_bits);
        let tw = json!([w.mode, w.task_b1, w.flag, w.task_h6]);
        assert_eq!(tw, out["tw"], "case {ci}: {case}");
        assert_eq!(w.angles, i3(&out["ang"]), "case {ci}: {case}");
        assert_eq!(json!(bits(w.pos)), out["pos"], "case {ci}: {case}");
        assert_eq!(json!(bits(w.mw_spd)), out["mwspd"], "case {ci}");
        assert_eq!(w.mw_ang, i3(&out["mwang"]), "case {ci}: {case}");
        let got = json!([
            w.p_bc,
            w.p_h10,
            w.p_h18,
            w.attr,
            w.p_w9c,
            w.p_wa0,
            w.ride.unwrap_or(0),
            w.action
        ]);
        let want = json!([
            out["bc"],
            out["h10"],
            out["h18"],
            out["attr"],
            out["w9c"],
            out["wa0"],
            out["wa4"],
            out["action"]
        ]);
        assert_eq!(got, want, "case {ci}: {case}");
        assert_eq!(json!(bits(w.spd)), out["spd"], "case {ci}");
        let hb = &w.here;
        let here = json!([
            hb.angle_x,
            hb.angle_z,
            hb.ceiling_flags,
            hb.floor_flags,
            hb.ceiling_y.to_bits(),
            hb.floor_y.to_bits(),
            hb.last_ceiling_flags,
            hb.last_floor_flags
        ]);
        assert_eq!(here, out["here"], "case {ci}: {case}");
        assert_eq!(w.pwp_h180_4, i(&out["h180"]) == 4, "case {ci}");
        assert_eq!(Value::from(log), case["log"], "case {ci}: {case}");
        snapped += (w.flag & 1 != 0 && i(&w0["flag"]) & 1 == 0) as u32;
    }
    assert!(snapped > 100, "{snapped}");
}
