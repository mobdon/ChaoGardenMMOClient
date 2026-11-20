//! Play a walker stage (Iron Gate by default, or Prison Lane) on the Rust
//! stage session.
//!
//!     sa2-play GAME_DIR PC_SA2_CONFIG [--stage iron-gate|prison-lane]          # window
//!     sa2-play GAME_DIR PC_SA2_CONFIG [--stage S] --headless OUT.json [MAX_FRAMES [CHECKPOINT]]
//!
//! GAME_DIR is the Steam "Sonic Adventure 2" folder; PC_SA2_CONFIG is SA Tools
//! 786's GameConfig/PC_SA2 directory. Nothing is written except OUT.json.
use sa2_asset_tools::{
    fan_stage::Campaign,
    play_autopilot::Autopilot,
    play_session::{Outcome, Session, Sources, StageRules, IRON_GATE, PRISON_LANE},
};
use std::{fs, path::PathBuf, process::ExitCode};

/// (option name, story sequence, action index, rules)
const STAGES: [(&str, &str, usize, &StageRules); 2] = [
    ("iron-gate", "Dark Story Sequence", 1, &IRON_GATE),
    ("prison-lane", "Hero Story Sequence", 9, &PRISON_LANE),
];

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("sa2-play: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!(
            "usage: sa2-play GAME_DIR PC_SA2_CONFIG [--stage iron-gate|prison-lane] [--headless OUT.json [MAX_FRAMES [CHECKPOINT]]]"
        );
        return Ok(ExitCode::from(2));
    }
    let mut stage_name = "iron-gate".to_string();
    if let Some(i) = args.iter().position(|a| a == "--stage") {
        stage_name = args.get(i + 1).ok_or("--stage needs a name")?.clone();
        args.drain(i..i + 2);
    }
    let &(_, sequence, action, rules) = STAGES
        .iter()
        .find(|s| s.0 == stage_name)
        .ok_or_else(|| format!("unknown stage {stage_name}"))?;
    let game = PathBuf::from(&args[1]);
    let cfg = PathBuf::from(&args[2]);
    let gd = game.join("resource").join("gd_PC");
    let exe = fs::read(game.join("sonic2app.exe"))?;
    let read = |n: &str| fs::read_to_string(cfg.join(n));
    let campaign = Campaign::new(
        &exe,
        &read("ADVERTISE.ini")?,
        &read("sa2lvl.ini")?,
        &read("ObjectLists.ini")?,
    )?;
    let load = |n: &str| -> sa2_asset_tools::Result<Option<Vec<u8>>> {
        let f = gd.join(n);
        Ok(if f.is_file() {
            Some(fs::read(f)?)
        } else {
            None
        })
    };
    let stage = campaign.mount(sequence, action, load)?;
    let module = fs::read(gd.join(rules.movers.module_file))?;
    let session = Session::walker_stage(
        &stage,
        Sources {
            exe: &exe,
            stage_module: &module,
        },
        rules,
    )?;
    match args.get(3).map(String::as_str) {
        Some("--headless") => {
            let out = args.get(4).ok_or("--headless needs an output path")?;
            let max: u32 = args
                .get(5)
                .map(|v| v.parse())
                .transpose()?
                .unwrap_or(60 * 60 * 10);
            let mut session = session;
            if let Some(k) = args.get(6) {
                session.restart_from_checkpoint(k.parse()?)?;
            }
            headless(session, PathBuf::from(out), max)
        }
        None => window(session, rules.name),
        Some(other) => Err(format!("unknown option {other}").into()),
    }
}

fn headless(
    mut s: Session,
    out: PathBuf,
    max: u32,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut pilot = Autopilot::new(&s);
    let mut events = vec![];
    let mut track = vec![];
    // Track sampling interval in frames (default once a second).
    let every: u32 = std::env::var("SA2_PLAY_TRACK_EVERY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60)
        .max(1);
    while s.frame < max && s.outcome == Outcome::Playing && !pilot.gave_up {
        let c = pilot.controls(&s);
        for e in s.step(c) {
            events.push(serde_json::json!({"frame": s.frame, "event": e}));
        }
        if s.frame.is_multiple_of(every) {
            track.push(serde_json::json!({"frame": s.frame, "position": s.player.position, "grounded": s.player.grounded, "onMover": s.player.on_mover, "stick": c.stick, "jump": c.jump}));
        }
    }
    let cleared = s.outcome == Outcome::Cleared;
    let report = serde_json::json!({
        "level": s.level,
        "character": s.character,
        "outcome": s.outcome,
        "frames": s.frame,
        "seconds": s.elapsed_seconds(),
        "rings": s.rings,
        "score": s.score,
        "deaths": s.deaths,
        "slotWords": s.slot_words,
        "finalPosition": s.player.position,
        "jumpHeight": s.jump_height(),
        "autopilotGaveUp": pilot.gave_up,
        "plans": pilot.plans,
        "goals": s.goals,
        "checkpoints": s.checkpoints,
        "springs": s.springs,
        "movers": s.movers,
        "deathZones": s.death_zone_count(),
        "unimplementedPlacements": s.unimplemented,
        "moverProgram": s.movers_program,
        "events": events,
        "track": track,
    });
    fs::write(&out, serde_json::to_vec_pretty(&report)?)?;
    let rings = events
        .iter()
        .filter(|e| e["event"]["kind"] == "ring")
        .count();
    println!(
        "{} after {} frames ({:.1} s); rings {rings}; deaths {}; plans {}",
        if cleared { "CLEARED" } else { "NOT CLEARED" },
        s.frame,
        s.elapsed_seconds(),
        s.deaths,
        pilot.plans.len()
    );
    Ok(if cleared {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

#[cfg(not(feature = "native-window"))]
fn window(_: Session, _: &str) -> Result<ExitCode, Box<dyn std::error::Error>> {
    Err("this build has no window; rebuild with the default native-window feature or use --headless".into())
}

#[cfg(feature = "native-window")]
fn window(s: Session, name: &str) -> Result<ExitCode, Box<dyn std::error::Error>> {
    view::run(s, name)
}

#[cfg(feature = "native-window")]
mod view {
    use minifb::{Key, KeyRepeat, Window, WindowOptions};
    use sa2_asset_tools::{
        play_session::{Controls, Event, MoverState, Outcome, Session},
        play_world::{add, cross, dot, len, scale, sub, V3},
    };
    use std::process::ExitCode;

    const W: usize = 800;
    const H: usize = 500;

    struct Cam {
        eye: V3,
        right: V3,
        up: V3,
        fwd: V3,
        focal: f32,
    }

    impl Cam {
        fn look(eye: V3, at: V3) -> Self {
            let mut f = sub(at, eye);
            f = scale(f, 1.0 / len(f).max(1e-4));
            let mut r = cross(f, [0.0, 1.0, 0.0]);
            r = scale(r, 1.0 / len(r).max(1e-4));
            let u = cross(r, f);
            Self {
                eye,
                right: r,
                up: u,
                fwd: f,
                focal: H as f32 * 0.9,
            }
        }
        fn view(&self, p: V3) -> V3 {
            let d = sub(p, self.eye);
            [dot(d, self.right), dot(d, self.up), dot(d, self.fwd)]
        }
        fn project(&self, v: V3) -> (f32, f32, f32) {
            (
                W as f32 * 0.5 + v[0] * self.focal / v[2],
                H as f32 * 0.5 - v[1] * self.focal / v[2],
                v[2],
            )
        }
    }

    struct Frame {
        color: Vec<u32>,
        depth: Vec<f32>,
    }

    impl Frame {
        fn clear(&mut self, sky: u32) {
            for (i, c) in self.color.iter_mut().enumerate() {
                let y = i / W;
                *c = if y < H / 2 { sky } else { 0x0010_1418 };
            }
            self.depth.iter_mut().for_each(|d| *d = f32::MAX);
        }

        fn tri(&mut self, cam: &Cam, t: [V3; 3], rgb: [f32; 3]) {
            const NEAR: f32 = 2.0;
            let v = t.map(|p| cam.view(p));
            // Clip against the near plane.
            let mut poly: Vec<V3> = vec![];
            for i in 0..3 {
                let a = v[i];
                let b = v[(i + 1) % 3];
                if a[2] >= NEAR {
                    poly.push(a);
                }
                if (a[2] >= NEAR) != (b[2] >= NEAR) {
                    let k = (NEAR - a[2]) / (b[2] - a[2]);
                    poly.push(add(a, scale(sub(b, a), k)));
                }
            }
            if poly.len() < 3 {
                return;
            }
            let n = {
                let c = cross(sub(t[1], t[0]), sub(t[2], t[0]));
                scale(c, 1.0 / len(c).max(1e-6))
            };
            let light = (dot(n, [0.35, 0.85, 0.4]).abs() * 0.75 + 0.25).min(1.0);
            let fog = (1.0 - poly[0][2] / 3500.0).clamp(0.25, 1.0);
            let col = rgb.map(|c| (c * light * fog * 255.0) as u32);
            let color = (col[0] << 16) | (col[1] << 8) | col[2];
            let pts: Vec<(f32, f32, f32)> = poly.iter().map(|&p| cam.project(p)).collect();
            for i in 1..pts.len() - 1 {
                self.raster(pts[0], pts[i], pts[i + 1], color);
            }
        }

        fn raster(
            &mut self,
            a: (f32, f32, f32),
            b: (f32, f32, f32),
            c: (f32, f32, f32),
            color: u32,
        ) {
            let minx = a.0.min(b.0).min(c.0).max(0.0) as i32;
            let maxx = a.0.max(b.0).max(c.0).min(W as f32 - 1.0) as i32;
            let miny = a.1.min(b.1).min(c.1).max(0.0) as i32;
            let maxy = a.1.max(b.1).max(c.1).min(H as f32 - 1.0) as i32;
            let area = (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
            if area.abs() < 1e-6 || minx > maxx || miny > maxy {
                return;
            }
            let (ia, ib, ic) = (1.0 / a.2, 1.0 / b.2, 1.0 / c.2);
            for y in miny..=maxy {
                for x in minx..=maxx {
                    let px = x as f32 + 0.5;
                    let py = y as f32 + 0.5;
                    let w0 = ((b.0 - px) * (c.1 - py) - (b.1 - py) * (c.0 - px)) / area;
                    let w1 = ((c.0 - px) * (a.1 - py) - (c.1 - py) * (a.0 - px)) / area;
                    let w2 = 1.0 - w0 - w1;
                    if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                        continue;
                    }
                    let z = 1.0 / (w0 * ia + w1 * ib + w2 * ic);
                    let i = y as usize * W + x as usize;
                    if z < self.depth[i] {
                        self.depth[i] = z;
                        self.color[i] = color;
                    }
                }
            }
        }

        fn marker(&mut self, cam: &Cam, p: V3, size: f32, color: u32) {
            let v = cam.view(p);
            if v[2] < 2.0 {
                return;
            }
            let (x, y, z) = cam.project(v);
            let r = (size * cam.focal / z).clamp(1.0, 60.0) as i32;
            for dy in -r..=r {
                for dx in -r..=r {
                    let (px, py) = (x as i32 + dx, y as i32 + dy);
                    if px < 0 || py < 0 || px >= W as i32 || py >= H as i32 {
                        continue;
                    }
                    let i = py as usize * W + px as usize;
                    if z - size < self.depth[i] {
                        self.color[i] = color;
                    }
                }
            }
        }
    }

    pub fn run(mut s: Session, name: &str) -> Result<ExitCode, Box<dyn std::error::Error>> {
        let mut window = Window::new(
            &format!("{name} (Rust stage session)"),
            W,
            H,
            WindowOptions::default(),
        )?;
        window.set_target_fps(60);
        let mut frame = Frame {
            color: vec![0; W * H],
            depth: vec![f32::MAX; W * H],
        };
        let mut cam_yaw = s.start_yaw;
        let mut message = String::from("Reach the goal ring");
        let floor = [0.55, 0.58, 0.62];
        let wall = [0.40, 0.42, 0.50];
        while window.is_open() && !window.is_key_down(Key::Escape) {
            let k = |a: Key, b: Key| window.is_key_down(a) || window.is_key_down(b);
            let mut stick = [0.0f32; 2];
            if k(Key::Left, Key::A) {
                stick[0] -= 1.0;
            }
            if k(Key::Right, Key::D) {
                stick[0] += 1.0;
            }
            if k(Key::Up, Key::W) {
                stick[1] += 1.0;
            }
            if k(Key::Down, Key::S) {
                stick[1] -= 1.0;
            }
            if window.is_key_down(Key::Q) {
                cam_yaw += 0.04;
            }
            if window.is_key_down(Key::E) {
                cam_yaw -= 0.04;
            }
            if window.is_key_pressed(Key::C, KeyRepeat::No) {
                cam_yaw = s.player.yaw;
            }
            let jump = window.is_key_down(Key::Space) || window.is_key_down(Key::J);
            for e in s.step(Controls {
                stick,
                camera_yaw: cam_yaw,
                jump,
            }) {
                message = match e {
                    Event::Checkpoint { .. } => "Checkpoint".into(),
                    Event::Death { .. } => "Fell - back to the last checkpoint".into(),
                    Event::LiftStarted { .. } => "Lift moving".into(),
                    Event::ElevatorStarted { .. } => "Elevator moving".into(),
                    Event::GoalContact { .. } => "Goal ring!".into(),
                    Event::Cleared { .. } => "MISSION CLEAR - Esc to quit".into(),
                    _ => message,
                };
            }
            // Chase camera behind and above the walker.
            let p = s.player.position;
            let back = [cam_yaw.sin() * 70.0, 38.0, cam_yaw.cos() * 70.0];
            let cam = Cam::look(add(p, back), add(p, [0.0, 18.0, 0.0]));
            frame.clear(0x0020_2838);
            for t in &s.world.tris {
                // Cull far triangles cheaply.
                if len(sub(t.v[0], p)) > 3500.0 {
                    continue;
                }
                let rgb = if t.n[1].abs() > 0.6 { floor } else { wall };
                frame.tri(&cam, t.v, rgb);
            }
            for m in &s.movers {
                let o = sub(m.position(), m.origin);
                let rgb = match m.state {
                    MoverState::Lift(_) => [0.9, 0.55, 0.15],
                    MoverState::Elevator(_) => [0.85, 0.75, 0.2],
                    MoverState::Bridge(_) => [0.6, 0.45, 0.3],
                };
                for t in &m.solid.tris {
                    frame.tri(&cam, t.v.map(|v| add(v, o)), rgb);
                }
            }
            for t in s.death_zone_triangles() {
                if len(sub(t[0], p)) < 1500.0 {
                    frame.tri(&cam, *t, [0.35, 0.05, 0.05]);
                }
            }
            for r in s.rings_placed.iter().filter(|r| !r.taken) {
                frame.marker(&cam, add(r.position, [0.0, 0.0, 0.0]), 4.0, 0x00ff_d020);
            }
            for c in &s.checkpoints {
                frame.marker(
                    &cam,
                    add(c.position, [0.0, 15.0, 0.0]),
                    5.0,
                    if c.taken { 0x0040_ff60 } else { 0x0020_a040 },
                );
            }
            for g in &s.goals {
                frame.marker(&cam, g.position, 18.0, 0x0060_e0ff);
            }
            frame.marker(&cam, s.player_center(), s.physics.rad, 0x00e0_3030);
            let t = s.elapsed_seconds();
            window.set_title(&format!(
                "{name} | {:02}:{:05.2} | rings {} | score {} | {}{}",
                (t / 60.0) as u32,
                t % 60.0,
                s.rings,
                s.score,
                message,
                if s.outcome == Outcome::Cleared {
                    ""
                } else {
                    "  [WASD/arrows move, Space jump, Q/E camera, C recenter]"
                }
            ));
            window.update_with_buffer(&frame.color, W, H)?;
        }
        Ok(ExitCode::SUCCESS)
    }
}
