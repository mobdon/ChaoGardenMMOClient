//! Replay tests/e_ai_ppc.py's scripted E AI and E GOLD runs (stg09D.rel's
//! own code under the Unicorn harness) through enemy_lib and compare the task,
//! the enemy work, the collision radius and attribute, the switch-group
//! counts, rand and every host call, frame by frame.
use sa2_asset_tools::{
    enemy_lib::{
        e_ai::{rows, EAi},
        e_gold::{rows as gold_rows, EGold},
        e_kumi::{rows as kumi_rows, EKumi},
        engine::GroupCounts,
        host::{EnemyHost, Event},
        motion::{read_table, Motion},
        EnemyTask, EnemyWork, PlayerView, Rand, V3,
    },
    ninja_matrix::MatrixStack,
};
use serde_json::{json, Value};
use std::{path::PathBuf, process::Command};

fn u(v: &Value) -> u32 {
    v.as_i64().unwrap() as u32
}
fn f(v: &Value) -> f32 {
    f32::from_bits(u(v))
}
fn v3(v: &Value) -> V3 {
    [f(&v[0]), f(&v[1]), f(&v[2])]
}
fn bits(v: V3) -> Value {
    json!(v.map(f32::to_bits))
}
fn hex(v: &Value) -> Vec<u8> {
    let s = v.as_str().unwrap();
    (0..s.len())
        .step_by(2)
        .map(|k| u8::from_str_radix(&s[k..k + 2], 16).unwrap())
        .collect()
}

struct Bytes<'a>(&'a [u8]);
impl Bytes<'_> {
    fn w(&self, o: usize) -> u32 {
        u32::from_be_bytes(self.0[o..o + 4].try_into().unwrap())
    }
    fn f(&self, o: usize) -> f32 {
        f32::from_bits(self.w(o))
    }
    fn v(&self, o: usize) -> V3 {
        [self.f(o), self.f(o + 4), self.f(o + 8)]
    }
    fn h(&self, o: usize) -> i16 {
        i16::from_be_bytes([self.0[o], self.0[o + 1]])
    }
}

fn decode_task(b: &[u8]) -> EnemyTask {
    let b = Bytes(b);
    EnemyTask {
        mode: b.0[0] as i8,
        kind: b.0[1] as i8,
        b2: b.0[2],
        shots: b.0[3],
        flags: b.h(4),
        timer: b.h(6) as u16,
        ang: [b.w(8) as i32, b.w(0xC) as i32, b.w(0x10) as i32],
        pos: b.v(0x14),
        scl: b.v(0x20),
    }
}

fn decode_work(b: &[u8], sec5: u32) -> EnemyWork {
    let b = Bytes(b);
    let m = &b.0[0x1E0..0x210];
    let mw = |o: usize| u32::from_be_bytes(m[o..o + 4].try_into().unwrap());
    let mp = |o: usize| (mw(o) != 0).then(|| mw(o) - sec5);
    EnemyWork {
        spd: b.v(0),
        accel: b.f(0xC),
        side_accel: b.f(0x14),
        wander_timer: b.w(0x18) as i32,
        circle_yaw: b.w(0x1C) as i32,
        link_40: b.w(0x40) != 0,
        b_44: b.0[0x44] as i8,
        group: b.0[0x45] as i8,
        b_46: b.0[0x46] as i8,
        burst: b.0[0x47] as i8,
        interval: b.h(0x48),
        flags: b.h(0x4C),
        h_4e: b.h(0x4E),
        f_54: b.f(0x54),
        home: b.v(0x58),
        wander_target: b.v(0x64),
        home_offset: b.v(0x70),
        last_pos: b.v(0x7C),
        f_94: b.f(0x94),
        f_98: b.f(0x98),
        f_ac: b.f(0xAC),
        scale: b.v(0xB0),
        f_c0: b.f(0xC0),
        f_c8: b.f(0xC8),
        f_cc: b.f(0xCC),
        hover: b.f(0xD0),
        f_d4: b.f(0xD4),
        f_d8: b.f(0xD8),
        f_dc: b.f(0xDC),
        f_e0: b.f(0xE0),
        f_e4: b.f(0xE4),
        i_e8: b.w(0xE8) as i32,
        f_ec: b.f(0xEC),
        f_f0: b.f(0xF0),
        probe: b.v(0xF4),
        shadow_y: b.f(0x150),
        f_1c0: b.f(0x1C0),
        bob: b.w(0x1C4) as i32,
        i_1cc: b.w(0x1CC) as i32,
        turn_rate: b.w(0x1C8) as i32,
        target_yaw: b.w(0x1D0) as i32,
        i_1d4: b.w(0x1D4) as i32,
        f_1d8: b.f(0x1D8),
        notice_range_sq: b.f(0x1DC),
        motion: Motion {
            frame: f32::from_bits(mw(0)),
            start: f32::from_bits(mw(8)),
            scale: f32::from_bits(mw(0xC)),
            mode: m[0x10] as i8,
            ended: m[0x11],
            current: m[0x12] as i8,
            request: m[0x13] as i8,
            previous: m[0x14] as i8,
            next: m[0x15] as i8,
            count: i16::from_be_bytes([m[0x16], m[0x17]]),
            blend_from: mp(0x20),
            blend_to: mp(0x24),
            blend_frame: f32::from_bits(mw(0x28)),
            blend_start: f32::from_bits(mw(0x2C)),
        },
    }
}

/// The scripted game around the enemy, from one frame's inputs.
struct Host<'a> {
    rel5: &'a [u8],
    g7c: i32,
    i: Value,
    events: Vec<Value>,
    tail: Vec<Value>,
    radius: Option<f32>,
    attr0: Option<u32>,
    attr1: Option<u32>,
    g3c: u8,
}

impl Host<'_> {
    fn b(&self, k: &str) -> bool {
        self.i[k].as_i64().unwrap() != 0
    }
}

impl EnemyHost for Host<'_> {
    fn game_37(&self) -> bool {
        self.b("game37")
    }
    fn game_39_42(&self) -> (u8, u8) {
        (u(&self.i["g39"]) as u8, u(&self.i["g42"]) as u8)
    }
    fn game_7c(&self) -> i32 {
        self.g7c
    }
    fn game_3c(&self) -> u8 {
        self.g3c
    }
    fn shadow_pos(&mut self, pos: V3) -> f32 {
        self.events
            .push(json!(["shadow", bits(pos)[0], bits(pos)[1], bits(pos)[2]]));
        f(&self.i["shadow_pos"])
    }
    fn spring_touch(&mut self) -> Option<i32> {
        self.b("spring")
            .then(|| self.i["hit_index"].as_i64().unwrap() as i32)
    }
    fn kumi_display_buffer(&mut self, _model: u32) -> bool {
        // One E KUMI per run creates the shared task, with its buffer when
        // +0x3C is 1 (syCalloc succeeds in the harness).
        self.g3c == 1
    }
    fn players(&self) -> [Option<PlayerView>; 2] {
        std::array::from_fn(|k| {
            let p = &self.i["players"][k];
            (!p.is_null()).then(|| {
                let pos = v3(p);
                PlayerView {
                    pos,
                    center: [pos[0], pos[1] + 10.0, pos[2]],
                }
            })
        })
    }
    fn player_mode(&self, index: usize) -> i8 {
        self.i["modes"][index].as_i64().unwrap() as i8
    }
    fn range_out(&mut self) -> bool {
        self.b("range_out")
    }
    fn set_entry_dead(&self) -> bool {
        self.b("set_dead")
    }
    fn hit_player(&self) -> bool {
        self.b("hit_player")
    }
    fn bumped(&self) -> bool {
        self.b("cwp_bit") && self.b("a4")
    }
    fn hit_by_player(&mut self) -> bool {
        self.b("hit_by_player")
    }
    fn damage_hook(&mut self, _pos: V3) -> bool {
        false
    }
    fn collision_hit(&self) -> bool {
        self.b("cwp_bit")
    }
    fn hit_by_player_index(&mut self) -> Option<i32> {
        let i = self.i["hit_index"].as_i64().unwrap() as i32;
        (i as u32 <= 1).then_some(i)
    }
    fn score_doubled(&mut self) -> bool {
        self.b("score_doubled")
    }
    fn ground(&mut self, _task: &mut EnemyTask, work: &mut EnemyWork) {
        work.probe[1] = f(&self.i["probe_y"]);
        work.shadow_y = f(&self.i["shadow_y"]);
        if self.b("landed") {
            work.flags |= 0x80;
        }
        self.events.push(json!(["ground"]));
    }
    fn collision_radius(&self) -> f32 {
        self.radius.unwrap()
    }
    fn set_collision_radius(&mut self, r: f32) {
        self.radius = Some(r);
    }
    fn collision_entry(&mut self) {
        self.events.push(json!(["ccl_entry"]));
    }
    fn event(&mut self, e: Event) {
        let v = match e {
            Event::Sound { id, volume, pos } => json!(["sound", id, volume as u32, bits(pos)]),
            Event::TaskSound { id, volume } => json!(["task_sound", id, volume as u32]),
            Event::Child {
                creator,
                args,
                ints,
                floats,
            } => {
                let mut v = vec![json!("child"), json!(creator)];
                v.extend(args.into_iter().map(bits));
                v.extend(ints.into_iter().map(|x| json!(x as u32)));
                v.extend(floats.into_iter().map(|x| json!(x.to_bits())));
                Value::Array(v)
            }
            Event::Carried { player, model } => json!(["carried", player as u32, model]),
            Event::KnockBack {
                player,
                speed,
                angles,
            } => json!(["knock", player as u32, bits(speed), angles]),
            Event::PlayerHurt { player, a, b } => json!(["hurt", player as u32, a, b]),
            Event::CollisionRange { range } => json!(["range", range.to_bits()]),
            Event::Spring { player, speed } => json!(["spring", player as u32, speed.to_bits()]),
            Event::Burrow { pos } => json!(["burrow", bits(pos)]),
            Event::Debris => json!(["debris"]),
            Event::Score { points } => json!(["score", points]),
            Event::Drop { animal, .. } => {
                if let Some((timer, kind)) = animal {
                    self.tail.push(json!(["animal", timer, kind]));
                    json!(["sound", 0x8015, 0x7f, "*"])
                } else {
                    return;
                }
            }
            Event::ClearCollision => json!(["clear"]),
            Event::CollisionOff { entry: 1 } => {
                *self.attr1.as_mut().unwrap() |= 0x10;
                return;
            }
            Event::CollisionOn { entry: 1 } => {
                *self.attr1.as_mut().unwrap() &= !0x10;
                return;
            }
            Event::CollisionOff { entry: 0 } => {
                *self.attr0.as_mut().unwrap() |= 0x10;
                return;
            }
            Event::CollisionOff { entry: 2.. } | Event::CollisionOn { entry: 2.. } => {
                unreachable!()
            }
            Event::CollisionInit { rows, count } => {
                let r = &self.rel5[rows as usize..];
                self.attr0 = Some(u32::from_be_bytes(r[4..8].try_into().unwrap()));
                self.attr1 =
                    (count > 1).then(|| u32::from_be_bytes(r[0x34..0x38].try_into().unwrap()));
                self.radius = Some(f32::from_be_bytes(r[0xC..0x10].try_into().unwrap()));
                json!(["ccl_init", rows])
            }
            Event::WorkSound {
                id,
                a,
                volume,
                b,
                pos,
            } => json!(["work_sound", id, a, volume as u32, b, bits(pos)]),
            Event::Hidden => {
                self.tail.push(json!(["hidden"]));
                return;
            }
            Event::CollisionOn { entry: 0 } => {
                *self.attr0.as_mut().unwrap() &= !0x10;
                return;
            }
            Event::Free => json!(["free"]),
            Event::SetEntryDead => json!(["set_dead"]),
        };
        self.events.push(v);
    }
}

enum Enemy {
    Ai(EAi),
    Gold(EGold),
    Kumi(EKumi),
}

impl Enemy {
    fn task(&self) -> &EnemyTask {
        match self {
            Enemy::Ai(e) => &e.task,
            Enemy::Gold(e) => &e.task,
            Enemy::Kumi(e) => &e.task,
        }
    }
    fn task_mut(&mut self) -> &mut EnemyTask {
        match self {
            Enemy::Ai(e) => &mut e.task,
            Enemy::Gold(e) => &mut e.task,
            Enemy::Kumi(e) => &mut e.task,
        }
    }
    fn work(&self) -> &EnemyWork {
        match self {
            Enemy::Ai(e) => &e.work,
            Enemy::Gold(e) => &e.work,
            Enemy::Kumi(e) => &e.work,
        }
    }
    fn work_mut(&mut self) -> &mut EnemyWork {
        match self {
            Enemy::Ai(e) => &mut e.work,
            Enemy::Gold(e) => &mut e.work,
            Enemy::Kumi(e) => &mut e.work,
        }
    }
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_STAGE_REL (GC stg09D.rel) and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn e_ai_matches_stg09d() {
    run("ai");
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_STAGE_REL (GC stg09D.rel) and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn e_gold_matches_stg09d() {
    run("gold");
}

#[test]
#[ignore = "requires SA2_MAIN_DOL, SA2_STAGE_REL (GC stg09D.rel) and a Python with Unicorn PPC (SA2_PPC_PYTHON)"]
fn e_kumi_matches_stg09d() {
    run("kumi");
}

fn run(enemy: &str) {
    let python = std::env::var("SA2_PPC_PYTHON").unwrap_or_else(|_| "python".into());
    let o = Command::new(python)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/e_ai_ppc.py"))
        .env("SA2_ENEMY", enemy)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let c: Value = serde_json::from_slice(&o.stdout).unwrap();
    let rel = std::fs::read(std::env::var("SA2_STAGE_REL").unwrap()).unwrap();
    let table = if enemy == "gold" {
        read_table(&rel, 9, 5, gold_rows::MOTION, gold_rows::MOTION_ROWS).unwrap()
    } else if enemy == "kumi" {
        read_table(&rel, 9, 5, kumi_rows::MOTION, kumi_rows::MOTION_ROWS).unwrap()
    } else {
        read_table(&rel, 9, 5, rows::MOTION, rows::MOTION_ROWS).unwrap()
    };
    let sec5 = u(&c["sec5"]);
    let rel5 = {
        // Section 5 of the GC REL, as the CCL rows are read from it.
        let n = u32::from_be_bytes(rel[0xC..0x10].try_into().unwrap());
        let info = u32::from_be_bytes(rel[0x10..0x14].try_into().unwrap()) as usize;
        assert!(n > 5);
        let off = u32::from_be_bytes(rel[info + 40..info + 44].try_into().unwrap()) as usize & !1;
        let size = u32::from_be_bytes(rel[info + 44..info + 48].try_into().unwrap()) as usize;
        rel[off..off + size].to_vec()
    };
    let mut stack = MatrixStack::new();
    let mut frames_checked = 0;
    for (ri, run) in c["runs"].as_array().unwrap().iter().enumerate() {
        let init = &run["init"];
        let task = EnemyTask {
            ang: std::array::from_fn(|k| u(&init["ang"][k]) as i32),
            pos: v3(&init["pos"]),
            scl: v3(&init["scl"]),
            ..Default::default()
        };
        let mut e = if enemy == "gold" {
            Enemy::Gold(EGold::new(task, table.clone()))
        } else if enemy == "kumi" {
            Enemy::Kumi(EKumi::new(task, table.clone()))
        } else {
            Enemy::Ai(EAi::new(task, table.clone()))
        };
        let g7c = u(&init["g7c"]) as i32;
        let g3c = u(&init["g3c"]) as u8;
        let mut attr1 = None;
        let mut rand = Rand(u(&init["rand"]));
        let mut counts: GroupCounts = [0; 256];
        let mut radius = None;
        let mut attr0 = None;
        for (fi, fr) in run["frames"].as_array().unwrap().iter().enumerate() {
            let i = fr["in"].clone();
            if i["flag4"].as_i64().unwrap() != 0 {
                e.task_mut().flags |= 4;
            }
            if fi > 0 {
                e.work_mut().link_40 = i["link40"].as_i64().unwrap() != 0;
                if i["carry"].as_i64().unwrap() != 0 {
                    e.work_mut().flags |= 0x400;
                }
            }
            let mut host = Host {
                rel5: &rel5,
                g7c,
                i,
                events: vec![],
                tail: vec![],
                radius,
                attr0,
                attr1,
                g3c,
            };
            match &mut e {
                Enemy::Ai(e) => e.exec(&mut host, &mut stack, &mut rand, &mut counts),
                Enemy::Gold(e) => e.exec(&mut host, &mut stack, &mut rand, &mut counts),
                Enemy::Kumi(e) => e.exec(&mut host, &mut stack, &mut rand, &mut counts),
            }
            let mut got = host.events.clone();
            got.extend(host.tail.clone());
            radius = host.radius;
            attr0 = host.attr0;
            attr1 = host.attr1;
            let at = format!("run {ri} frame {fi}");
            assert_eq!(*e.task(), decode_task(&hex(&fr["twp"])), "task, {at}");
            if !fr["work"].is_null() {
                assert_eq!(
                    *e.work(),
                    decode_work(&hex(&fr["work"]), sec5),
                    "work, {at}"
                );
            }
            let want: Vec<Value> = fr["events"].as_array().unwrap().clone();
            assert_eq!(got.len(), want.len(), "events, {at}: {got:?} vs {want:?}");
            for (g, w) in got.iter().zip(&want) {
                if g[3] == json!("*") {
                    assert_eq!(
                        g.as_array().unwrap()[..3],
                        w.as_array().unwrap()[..3],
                        "event, {at}"
                    );
                } else {
                    assert_eq!(g, w, "event, {at}");
                }
            }
            if !fr["radius"].is_null() {
                assert_eq!(
                    radius.map(f32::to_bits),
                    Some(u(&fr["radius"])),
                    "radius, {at}"
                );
                assert_eq!(attr0, Some(u(&fr["attr0"])), "attr, {at}");
                if let Some(a) = attr1 {
                    assert_eq!(a, u(&fr["attr1"]), "attr 1, {at}");
                }
            }
            assert_eq!(rand.0, u(&fr["rand"]), "rand, {at}");
            assert_eq!(counts.to_vec(), hex(&fr["counts"]), "group counts, {at}");
            frames_checked += 1;
        }
    }
    assert!(frames_checked > 1000);
}
