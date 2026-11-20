//! Compare `minimal::Minimal::step` with an oracle run of main.dol's MINIMAL
//! exec (0x80026AFC) on random states (scratch oracle minimal_ppc.py: one
//! player in slot 0, nearest-player answers 0, one land entry whose
//! fn_800D05D0 hit block is fixed per case).
//!
//!     cargo run --release --example minimal_check -- ORACLE.json
use sa2_asset_tools::land_probe::{Hit, Probe};
use sa2_asset_tools::minimal::{Minimal, MinimalEvent, MinimalInputs, MinimalPlayer};

fn main() {
    let path = std::env::args().nth(1).expect("ORACLE.json");
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let u = |x: &serde_json::Value| x.as_u64().unwrap() as u32;
    let i = |x: &serde_json::Value| x.as_i64().unwrap() as i32;
    let f = |x: &serde_json::Value| f32::from_bits(u(x));
    let cases = v["cases"].as_array().unwrap();
    let (mut bad, mut frames_checked) = (0, 0);
    let hit = Hit {
        flags: 0,
        angle_x: 0,
        angle_z: 0,
        y: 0.0,
        normal: [0.0; 3],
    };
    for (n, c) in cases.iter().enumerate() {
        let w = &c["w"];
        let mut m = Minimal {
            layer: 0,
            index: 0,
            position: [0, 1, 2].map(|k| f(&c["pos"][k])),
            rotation: [0, 1, 2].map(|k| i(&c["rot"][k])),
            scale: [0, 1, 2].map(|k| f(&c["scale"][k])),
            action: u(&c["action"]) as u8,
            timer: u(&c["timer"]) as u16,
            status: u(&c["status"]) as u16,
            mode: u(&w[0]),
            slot: u(&w[1]),
            animal: u(&w[2]),
            spin: i(&w[3]),
            speed: f(&w[4]),
            hold: i(&w[5]),
            frame: f(&w[6]),
            gravity: u(&w[7]),
        };
        let player = MinimalPlayer {
            position: [0, 1, 2].map(|k| f(&c["ppos"][k])),
            character: i(&c["char"]) as i8,
            action: u(&c["pact"]) as u8,
        };
        let below = c["below"].as_f64().map(|y| y as f32);
        let above = c["above"].as_f64().map(|y| y as f32);
        let motion = c["motion"]
            .as_array()
            .map(|a| (a[0].as_f64().unwrap() as f32, a[1].as_i64().unwrap() as i32));
        for (k, out) in c["out"].as_array().unwrap().iter().enumerate() {
            let mut land = |_p: [f32; 3]| {
                vec![Probe {
                    below: below.map(|_| hit),
                    below_y: below.unwrap_or(0.0),
                    above: above.map(|_| hit),
                    above_y: above.unwrap_or(0.0),
                }]
            };
            let mut players = [None; 8];
            players[0] = Some(player);
            let nearest = |_p: [f32; 3]| 0usize;
            let mut input = MinimalInputs {
                players,
                nearest: &nearest,
                use_nearest: false,
                suppressed: false,
                gravity_mode: u(&c["grav"]),
                rotation_x_byte: 5,
                land: &mut land,
                motion,
            };
            let events = m.step(&mut input);
            frames_checked += 1;
            let gone = events.contains(&MinimalEvent::Gone);
            let action = if m.action == 3 {
                2
            } else {
                u32::from(m.action)
            };
            let got = (
                action,
                u32::from(m.timer),
                m.rotation,
                m.position.map(f32::to_bits),
                m.scale.map(f32::to_bits),
                [
                    m.mode,
                    m.slot,
                    m.animal,
                    m.spin as u32,
                    m.speed.to_bits(),
                    m.hold as u32,
                    m.frame.to_bits(),
                    m.gravity,
                ],
                events
                    .iter()
                    .any(|e| matches!(e, MinimalEvent::RequestAction { .. })),
                events
                    .iter()
                    .any(|e| matches!(e, MinimalEvent::Collected { .. })),
            );
            let calls: Vec<&str> = out["calls"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_str().unwrap())
                .collect();
            let want = (
                u(&out["action"]),
                u(&out["timer"]),
                [0, 1, 2].map(|q| i(&out["rot"][q])),
                [0, 1, 2].map(|q| u(&out["pos"][q])),
                [0, 1, 2].map(|q| u(&out["scale"][q])),
                [0, 1, 2, 3, 4, 5, 6, 7].map(|q| u(&out["w"][q])),
                u(&out["request"]) == 0x2a,
                calls.contains(&"give"),
            );
            if got != want {
                bad += 1;
                if bad <= 6 {
                    println!("case {n} frame {k}:\n got  {got:x?}\n want {want:x?}");
                }
                break;
            }
            if gone {
                break;
            }
        }
    }
    println!(
        "cases {} frames {frames_checked} mismatching cases {bad}",
        cases.len()
    );
}
