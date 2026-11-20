//! The walker thread's adapter (`sa2::walker_frame_steps::walker_frame`)
//! behind the runtime's walker exec slot.
//!
//! Filled from the runtime: the work blocks, G, the stick (PadSystem's
//! angle and magnitude), the Steam physics rows and Mech Eggman's action
//! table from sonic2app.exe. The land context is Iron Gate's Steam
//! landtable (stg11D.rel) as NJS_OBJECT/NJS_MODEL arenas, reselected each
//! frame around the player (fn_8002334C, range 400 from 0x803AD390) and
//! turned into collision nodes (fn_800D0904).
//!
//! Not filled yet (each answers "nothing there"): CCL hits, lock-on
//! targets, damage, held objects, paths, voices, motion frame counts, the
//! camera. Hooks the adapter reports as unported are counted in
//! [`AdapterWalker::unported`].
use crate::game::{RuntimeEngine, WalkerExec, WalkerWork};
use sa2_asset_tools::{
    character_animations, character_motions,
    land_contact::ContactStats,
    land_nodes::{LandNodes, MeshSet, NjModel, NjObject},
    land_select::{select_frame, LandCol, LandTableHead, Selection},
    native_work::Work,
    ninja_matrix::Vec3,
    player_motion::{Action, MotionTables},
    player_pre::{OtherTask, PreCcl, PreGame},
    player_shadow::ProbeLand,
    player_stand::{StandGame, VoiceTables},
    stage::Stage,
    walker_act::{DamageWorld, LockTarget, LockWorld},
    walker_adapter::MoveEnv,
    walker_collide::PhysicsRow,
    walker_exec::{Engine, EngineCall, ExecGlobals, PlayerHistory},
    walker_frame_steps::{walker_frame, ObjectHooks, PlayerGlobals},
    walker_ground::{CclView, CollisionQuery, LandContext},
};
use std::collections::BTreeMap;

/// r13-0x7F30 (0x803AD390): the selection range.
const RANGE: f32 = 400.0;

/// Iron Gate's land as the collision reads it.
pub struct Land {
    pub objects: Vec<NjObject>,
    pub models: Vec<NjModel>,
    pub cols: Vec<LandCol>,
    pub col_objs: Vec<usize>,
    pub head: LandTableHead,
}

impl Land {
    /// Basic models and object trees of every landtable entry (model 0 is
    /// an empty model for objects without a basic attachment).
    pub fn from_stage(s: &Stage) -> Self {
        let mut models = vec![NjModel {
            points: vec![],
            sets: vec![],
            attrs: None,
            center: [0.0; 3],
            radius: 0.0,
        }];
        let mut model_at = BTreeMap::new();
        for a in &s.basic_attachments {
            model_at.insert(a.offset, models.len());
            let sets = a
                .meshsets
                .iter()
                .map(|m| {
                    let mut meshes = Vec::new();
                    for p in &m.polygons {
                        if m.polygon_type >= 2 {
                            let n = p.indices.len() as u16 & 0x7FFF;
                            meshes.push(n | if p.reversed { 0x8000 } else { 0 });
                        }
                        meshes.extend(&p.indices);
                    }
                    MeshSet {
                        type_mat: (u16::from(m.polygon_type) << 14) | (m.material_id & 0x3FFF),
                        count: m.polygons.len() as u16,
                        meshes,
                    }
                })
                .collect();
            models.push(NjModel {
                points: a.vertices.clone(),
                sets,
                attrs: (!a.material_words.is_empty())
                    .then(|| a.material_words.iter().map(|w| w[4]).collect()),
                center: a.bounds_center,
                radius: a.bounds_radius,
            });
        }
        let mut objects = Vec::new();
        let mut cols = Vec::new();
        let mut col_objs = Vec::new();
        for e in &s.entries {
            let base = objects.len();
            let at: BTreeMap<u32, usize> = e
                .nodes
                .iter()
                .enumerate()
                .map(|(i, n)| (n.offset, base + i))
                .collect();
            for n in &e.nodes {
                objects.push(NjObject {
                    evalflags: n.flags,
                    model: model_at.get(&n.attach_offset).copied().unwrap_or(0),
                    pos: n.position,
                    ang: n.rotation_bams,
                    scl: n.scale,
                    child: at.get(&n.child_offset).copied(),
                    sibling: at.get(&n.sibling_offset).copied(),
                });
            }
            cols.push(LandCol {
                center: e.bounds_center,
                radius: e.bounds_radius,
                flags: e.surface_flags,
            });
            col_objs.push(at.get(&e.root_offset).copied().unwrap_or(base));
        }
        let head = LandTableHead {
            count: cols.len() as i16,
            start: s.visual_count,
        };
        Self {
            objects,
            models,
            cols,
            col_objs,
            head,
        }
    }
}

/// Physics rows as the adapter reads them (Steam PhysicsArray, big-endian).
pub fn physics_rows(rows: &[[u8; 0x84]]) -> Vec<PhysicsRow> {
    let f = |r: &[u8; 0x84], o: usize| f32::from_be_bytes([r[o], r[o + 1], r[o + 2], r[o + 3]]);
    rows.iter()
        .map(|r| PhysicsRow {
            run_accel: f(r, 0x3C),
            f58: f(r, 0x58),
            f74: f(r, 0x74),
            weight: f(r, 0x78),
        })
        .collect()
}

/// Mech Eggman's action table from sonic2app.exe.
pub fn mech_eggman_actions(exe: &[u8]) -> Result<Vec<Action>, String> {
    let t = character_animations::read(exe).map_err(|e| e.to_string())?;
    let t = t
        .into_iter()
        .find(|t| t.character == "MechEggman")
        .ok_or("no MechEggman table")?;
    Ok(t.rows
        .iter()
        .map(|r| Action {
            motion: r.motion,
            model: r.model,
            mode: r.mode,
            next: r.next,
            blend: r.transition_speed,
            speed: r.animation_speed,
        })
        .collect())
}

/// Nothing to hit, lock on to or hold: every query answers empty.
#[derive(Default)]
struct EmptyWorld {
    unported: Vec<u32>,
}

impl PreCcl for EmptyWorld {
    fn kind_hit(&mut self, _kind: u8) -> Option<OtherTask> {
        None
    }
    fn hit_63f4(&mut self) -> bool {
        false
    }
    fn hit_6270(&mut self) -> bool {
        false
    }
    fn first_hit(&mut self) -> Option<OtherTask> {
        None
    }
}

impl LockWorld for EmptyWorld {
    fn partner(&mut self, _pnum: i8) -> Option<Vec3> {
        None
    }
    fn targets(&mut self, _list: usize) -> Vec<LockTarget> {
        Vec::new()
    }
}

impl DamageWorld for EmptyWorld {
    fn hit_63f4(&mut self) -> bool {
        false
    }
    fn hit_player(&mut self) -> bool {
        false
    }
    fn hit_6270(&mut self) -> bool {
        false
    }
    fn last_record_is_self(&mut self) -> Option<bool> {
        None
    }
    fn first_kind(&mut self) -> i32 {
        0
    }
    fn kind_hit(&mut self, _kind: u8) -> Option<(Vec3, i32, i8)> {
        None
    }
    fn attacker_damage(&mut self) -> Option<f32> {
        None
    }
    fn other_voice_char(&mut self) -> Option<i8> {
        None
    }
    fn face(&mut self, _pnum: i8, _v: Vec3) {}
    fn release_hold(&mut self, _t: &mut Work, _p: &mut Work) {}
    fn p1_status_4000(&mut self) -> bool {
        false
    }
}

impl ObjectHooks for EmptyWorld {
    fn hold_flags(&mut self, _t: &Work, _p: &Work) -> u32 {
        0
    }
    fn release_hold(&mut self, _t: &mut Work, _m: &mut Work, _p: &mut Work) {}
    fn button_action(&mut self, addr: u32, _t: &mut Work, _m: &mut Work, _p: &mut Work) {
        self.unported.push(addr);
    }
    fn ride_acc(&mut self, _p: &Work) -> Option<Vec3> {
        None
    }
    fn set_velocity(&mut self, _pnum: i8, _v: Vec3) {}
    fn set_ccl_centers(&mut self, _a: Vec3, _b: Vec3) {}
    fn step_motion(&mut self, _p: &Work) -> (u32, f32, f32) {
        (0, 0.0, 0.0)
    }
    fn unported(&mut self, addr: u32, _t: &mut Work, _m: &mut Work, _p: &mut Work) -> i32 {
        self.unported.push(addr);
        0
    }
}

impl Engine for RuntimeEngine<'_> {
    fn call(&mut self, c: EngineCall) {
        let i = |v: i32| v as u32;
        let f = |v: f32| v.to_bits();
        let (addr, args) = match c {
            EngineCall::CreateElementalTask {
                list,
                flags,
                exec,
                name,
                pwp_byte0,
            } => {
                self.rt.engine(
                    0x8007_1E0C,
                    vec![u32::from(list), u32::from(flags), exec, name],
                );
                if let Ok(t) =
                    self.rt
                        .create_elemental_task(u16::from(list), u32::from(flags), exec, name)
                {
                    if let (Some(b), Some(w)) =
                        (pwp_byte0, self.rt.task_mut(t).and_then(|x| x.w40.as_mut()))
                    {
                        if let Some(x) = w.first_mut() {
                            *x = b;
                        }
                    }
                }
                return;
            }
            EngineCall::PlayVoice { id } => (0x8001_1714, vec![i(id.into())]),
            EngineCall::Deferred { addr, arg } => (addr, vec![i(arg)]),
            EngineCall::DeferredAt { addr, arg, points } => (
                addr,
                [
                    vec![i(arg)],
                    points[0].map(f).to_vec(),
                    points[1].map(f).to_vec(),
                ]
                .concat(),
            ),
            EngineCall::StopSound { id } => (0x8006_9C58, vec![i(id)]),
            EngineCall::KillPlayer { pnum } => (0x8003_BB7C, vec![i(pnum.into())]),
            EngineCall::BattleVoice { id } => (0x8001_17EC, vec![i(id), 3]),
            EngineCall::Indirect { ptr, arg } => (ptr, vec![i(arg)]),
            EngineCall::GameStateBytes { at, len, value } => (
                0x801C_C168,
                vec![u32::from(at), u32::from(len), u32::from(value)],
            ),
            EngineCall::CclEntry => (0x8000_F4EC, vec![]),
            EngineCall::Rumble { addr, args } => (addr, args.map(i).to_vec()),
            EngineCall::Sound {
                addr,
                id,
                args,
                pos,
            } => (
                addr,
                [
                    vec![i(id)],
                    args.map(i).to_vec(),
                    pos.map_or(vec![], |p| p.map(f).to_vec()),
                ]
                .concat(),
            ),
        };
        self.rt.engine(addr, args);
    }
    fn rand(&mut self) -> i32 {
        RuntimeEngine::rand(self)
    }
}

/// The adapter with Iron Gate's land and the walker's persistent globals.
pub struct AdapterWalker {
    pub land: Land,
    pub rows: Vec<PhysicsRow>,
    pub actions: Vec<Action>,
    /// Motion frame counts by motion-table slot (ewalkmtn.prs).
    pub frames: BTreeMap<i16, u32>,
    nodes: LandNodes,
    selection: Selection,
    stats: ContactStats,
    hist: PlayerHistory,
    query: CollisionQuery,
    /// Addresses the adapter reached without a port, in order.
    pub unported: Vec<u32>,
}

impl AdapterWalker {
    pub fn new(land: Land, rows: Vec<PhysicsRow>, actions: Vec<Action>, motions: &[u8]) -> Self {
        let frames = character_motions::Bank::read(motions)
            .map(|b| b.rows.iter().map(|r| (r.slot as i16, r.frames)).collect())
            .unwrap_or_default();
        Self {
            land,
            rows,
            actions,
            frames,
            nodes: LandNodes::new(),
            selection: Selection::default(),
            stats: ContactStats::default(),
            hist: PlayerHistory::new(2),
            query: CollisionQuery {
                radius: 1.0,
                center: [0.0; 3],
                motion: [0.0; 3],
                up: [0.0, 1.0, 0.0],
                count: 0,
                best: 0,
                ang_x: 0,
                ang_z: 0,
                normal: [0.0; 3],
                facing: [0.0; 3],
                foot: [0.0; 3],
                head: [0.0; 3],
                along: [0.0; 3],
                across: [0.0; 3],
            },
            unported: Vec::new(),
        }
    }
}

fn gs8(g: &[u8], o: usize) -> i8 {
    g.get(o).copied().unwrap_or(0) as i8
}

impl WalkerExec for AdapterWalker {
    fn exec(&mut self, w: WalkerWork<'_>, eng: &mut RuntimeEngine<'_>) {
        let pos = {
            let f = |o: usize| {
                w.twp
                    .get(o..o + 4)
                    .map_or(0.0, |b| f32::from_be_bytes([b[0], b[1], b[2], b[3]]))
            };
            [f(0x14), f(0x18), f(0x1C)]
        };
        let trace = std::env::var_os("WALKER_TRACE").is_some();
        if trace {
            eprintln!("  walker pos {pos:?} select");
        }
        let l = &self.land;
        // fn_8002334C then fn_800D0904: this frame's land around the player.
        self.selection = select_frame(
            &[],
            &l.objects,
            &l.models,
            Some(l.head),
            &l.cols,
            &l.col_objs,
            &[pos],
            RANGE,
        );
        if trace {
            eprintln!("  selected {} entries; nodes", self.selection.list.len());
        }
        self.nodes
            .update(&self.selection.list, &l.objects, &l.models);
        if trace {
            eprintln!("  nodes done");
        }

        let stage = 11;
        let gravity = [0.0, -1.0, 0.0];
        let (angle, magnitude) = w.stick;
        let mut mv = MoveEnv {
            gravity,
            stage,
            raw_stick: w.raw_stick,
            ..MoveEnv::default()
        };
        mv.analog.angle = angle as i32;
        mv.analog.magnitude = magnitude;
        mv.analog.stage = stage as i16;
        mv.analog.stick_y = w.raw_stick[1];
        mv.analog.gravity_y = gravity[1];
        mv.analog.frame = u32::from_be_bytes([
            w.g.get(0x7C).copied().unwrap_or(0),
            w.g.get(0x7D).copied().unwrap_or(0),
            w.g.get(0x7E).copied().unwrap_or(0),
            w.g.get(0x7F).copied().unwrap_or(0),
        ]);
        let mut gs = Work::new(0x100);
        gs.0[..w.g.len().min(0x100)].copy_from_slice(&w.g[..w.g.len().min(0x100)]);
        let mut exec = ExecGlobals::default();
        exec.act.gs = gs;
        exec.act.stage = stage;
        exec.act.gs_b49 = gs8(w.g, 0x49);
        exec.act.gravity = gravity;
        exec.act.query_20 = self.query.up[1];
        let pre = PreGame {
            b1e: gs8(w.g, 0x1E),
            b39: gs8(w.g, 0x39),
            b42: w.g.get(0x42).copied().unwrap_or(0),
            b49: gs8(w.g, 0x49),
            b3f: gs8(w.g, 0x3F),
            gravity,
            ..PreGame::default()
        };
        let stand = StandGame {
            b1e: gs8(w.g, 0x1E),
            b39: gs8(w.g, 0x39),
            frame: mv.analog.frame,
            stage,
        };
        let mut world = EmptyWorld::default();
        let mut world2 = EmptyWorld::default();
        let mut world3 = EmptyWorld::default();
        let mut world4 = EmptyWorld::default();
        let zero = |_: i16| 0u32;
        let frames = |id: i16| self.frames.get(&id).copied().unwrap_or(0);
        let no_ride = |_: usize, _: i8| None;
        let no_coll = |_: usize| false;
        let no_flag = |_: usize| None;
        let no_off = |_: usize| None;
        let (objects, models, cols, col_objs) =
            (&l.objects[..], &l.models[..], &l.cols[..], &l.col_objs[..]);
        let mut sel_probe = self.selection.clone();
        let mut reselect = |c: Vec3, r: f32| {
            sel_probe.reselect(objects, models, cols, col_objs, c, r);
            sel_probe.list.clone()
        };
        let probe_nodes = self.nodes.clone();
        let mut g = PlayerGlobals {
            mv,
            exec,
            hist: &mut self.hist,
            pre,
            stand,
            voices: VoiceTables {
                any_stage: &[],
                by_stage: &[],
            },
            motion: MotionTables {
                actions: &self.actions,
                frames: &frames,
                model_key: &zero,
            },
            motion_scales: (1.0, 1.0),
            motion_blend: false,
            query: self.query.clone(),
            ccl: CclView {
                h2: 0,
                flags: 0,
                push: [0.0; 3],
                self_b3: 0,
                self_radius: 0.0,
                hit: None,
                pushed_info_w4: 0,
                interaction: 0,
                carrier_ang_y: None,
            },
            ccl_info: None,
            rows: &self.rows,
            stuck: false,
            ride_record: &no_ride,
            task_has_collision: &no_coll,
            r13_7900: 0,
            game_mode_42: w.g.get(0x42).copied().unwrap_or(0),
            pre_ccl: &mut world,
            lock: &mut world2,
            damage: &mut world3,
            objects: &mut world4,
            probe_land: ProbeLand {
                nodes: &probe_nodes,
                objects,
                models,
                task_flag: &no_flag,
                task_offset: &no_off,
                stage,
            },
            reselect: &mut reselect,
            blend_events: Vec::new(),
        };
        let mut land = LandContext {
            selection: &mut self.selection,
            nodes: &mut self.nodes,
            objects,
            models,
            cols,
            col_objs,
            stats: &mut self.stats,
            hits: Vec::new(),
        };
        // Copies: a panic below (an unported mode) leaves the task's blocks
        // as they were before the frame.
        let mut t = Work(w.twp.clone());
        let mut m = Work(w.mwp.clone());
        let mut p = Work(w.pwp.clone());
        if trace {
            eprintln!(
                "  walker_frame: twp {} pwp action {:02x?}",
                hex(&t.0),
                p.0.get(0x174..0x180)
            );
        }
        walker_frame(&mut t, &mut m, &mut p, &mut g, &mut land, eng);
        if trace {
            eprintln!("  walker_frame done");
        }
        let query = g.query.clone();
        drop(g);
        drop(land);
        self.query = query;
        *w.twp = t.0;
        *w.mwp = m.0;
        *w.pwp = p.0;
        self.unported.extend(world4.unported);
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
