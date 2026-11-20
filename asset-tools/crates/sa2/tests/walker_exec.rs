//! Compare `walker_exec` with main.dol (tests/walker_exec_ppc.py).
use sa2_asset_tools::{
    ninja_matrix::MatrixStack,
    walker_exec::{end_frame, EndFrame, EngineCall, PlayerHistory},
};
use serde_json::Value;
use std::{path::PathBuf, process::Command};

fn f(x: &Value) -> f32 {
    f32::from_bits(x.as_u64().unwrap() as u32)
}

fn v3(x: &Value) -> [f32; 3] {
    [f(&x[0]), f(&x[1]), f(&x[2])]
}

fn i3(x: &Value) -> [i32; 3] {
    [0, 1, 2].map(|j| x[j].as_i64().unwrap() as i32)
}

fn u(x: &Value) -> u64 {
    x.as_u64().unwrap()
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, the local Gekko tables and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn matches_main_dol() {
    let python = std::env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/walker_exec_ppc.py"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    if o.stdout.is_empty() {
        eprintln!("skipped: {}", String::from_utf8_lossy(&o.stderr));
        return;
    }
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    let mut stack = MatrixStack::new();
    let bits = |a: [f32; 3]| a.map(f32::to_bits);
    for c in v["end"].as_array().unwrap() {
        let i = |x: &Value| x.as_i64().unwrap();
        let mut w = EndFrame {
            task_flag: u(&c["flag"]) as u16 as i16,
            task_b1: 7,
            angles: i3(&c["ang"]),
            pos: v3(&c["pos"]),
            mw_acc: [1.0, 2.0, 3.0],
            pnum: u(&c["pnum"]) as u8,
            p_b1: u(&c["b"]["b1"]) as u8,
            p_b3: u(&c["b"]["b3"]) as u8,
            p_bd: u(&c["b"]["bd"]) as u8,
            p_h10: i(&c["h"]["h10"]) as i16,
            p_h14: i(&c["h"]["h14"]) as i16,
            air_timer: i(&c["h"]["h16"]) as i16,
            p_h1a: i(&c["h"]["h1a"]) as i16,
            p_h1e: i(&c["h"]["h1e"]) as i16,
            p_w24: u(&c["w"]["w24"]) as u32,
            p_wa8: 5,
            p_wac: 6,
            p_w1b8: u(&c["w"]["w1b8"]) as u32,
            p_f38: 9.0,
            p_f4c: [9.0; 6],
            head: f(&c["head"]),
            water: f(&c["water"]),
            ccl_b2: 0xFF,
        };
        let mut hist = PlayerHistory::new(2);
        hist.index[w.pnum as usize] = u(&c["hidx"]) as u8;
        let mut r13 = 0x1234;
        let calls = end_frame(
            &mut stack,
            &mut w,
            &mut hist,
            v3(&c["ppos"]),
            i3(&c["pang"]),
            u(&c["gm"]) as u8,
            &mut r13,
        );
        assert_eq!(w.task_flag as u16 as u64, u(&c["flag_after"]), "{c}");
        assert_eq!(w.task_b1 as u64, u(&c["tb1_after"]), "{c}");
        assert_eq!(bits(w.mw_acc), bits(v3(&c["mwacc_after"])), "{c}");
        let mut pf = vec![w.p_f38.to_bits() as u64];
        pf.extend(w.p_f4c.iter().map(|x| x.to_bits() as u64));
        assert_eq!(Value::from(pf), c["pf_after"], "{c}");
        let h = [w.p_h14, w.air_timer, w.p_h1a, w.p_h1e].map(|x| x as i64);
        assert_eq!(Value::from(h.to_vec()), c["h_after"], "{c}");
        assert_eq!((w.p_wa8 as u64, w.p_wac as u64), (u(&c["wa8"]), u(&c["wac"])), "{c}");
        assert_eq!(w.p_bd as u64, u(&c["bd_after"]), "{c}");
        assert_eq!(w.ccl_b2 as u64, u(&c["info_b2"]), "{c}");
        assert_eq!(r13 as u64, u(&c["r13_7900"]), "{c}");
        let n = w.pnum as usize;
        assert_eq!(hist.index[n] as u64, u(&c["hidx_after"]), "{c}");
        let slot = (n << 8) + hist.index[n] as usize;
        assert_eq!(bits(hist.pos[slot]), bits(v3(&c["hpos"])), "{c}");
        assert_eq!(hist.ang[slot], i3(&c["hang"]), "{c}");
        let want = c["created"].as_array().unwrap();
        assert_eq!(calls.len(), want.len(), "{c}");
        for (call, wv) in calls.iter().zip(want) {
            let EngineCall::CreateElementalTask {
                list,
                flags,
                exec,
                name,
                pwp_byte0,
            } = *call;
            let got = [list as u64, flags as u64, exec as u64, name as u64];
            assert_eq!(got, [0, 1, 2, 3].map(|j| u(&wv[j])), "{c}");
            assert_eq!(pwp_byte0.map(u64::from), Some(u(&c["newpw"])), "{c}");
        }
    }
}
