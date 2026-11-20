//! A headless Iron Gate run (milestone E5): session init, the stage load with
//! the Steam SET data and object list, then `fn_8001E45C` frames with the
//! PAD path, the SET task and the walker exec slot in the exec pass.
//!
//! Engine routines that are not ported are logged by the runtime and answered
//! here with fixed values (handles counted up, zero otherwise), so a run is a
//! pure function of the Steam data and the pad inputs.
use crate::frame::{stage_frame, FrameHost, GameState};
use crate::input::{PadContext, PadRef, PadSystem};
use crate::objects::{object_list, steam_dol_to_gsne8p, steam_stg11};
use crate::player::{steam_physics_rows, PlayerHost, Players, Ptr, TEX_BUFFER_SIZE};
use crate::session::{session_init, Session, SessionHost, MESSAGE_LOAD, SAVE_PROBE};
use crate::set::{join_files, ObjectDef, SetRow, WithSet};
use crate::stage::{stage_load, StageHost, StageState, Stg11Layout};
use crate::task::{Natives, Registry, Runtime, TaskId};
use sa2_asset_tools::input::PadStatus;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// The walker exec (Mech Eggman, Tails' walker and the Chao Walker share it).
pub const WALKER_EXEC: u32 = 0x800B_3A8C;
/// `fn_80065048`, the SET file load.
const SET_LOAD: u32 = 0x8006_5048;

/// The stage module's place in memory (the port's choice of base).
pub const LAYOUT: Stg11Layout = Stg11Layout {
    text: 0x8120_0100,
    rodata: 0x8126_0000,
    data: 0x8127_0000,
    bss: 0x8135_0000,
};

/// One player's work blocks at their native offsets (the byte buffers
/// `sa2::native_work::Work` wraps), for the walker exec.
pub struct WalkerWork<'a> {
    pub task: TaskId,
    /// Player index (playertwp slot).
    pub player: usize,
    /// +0x34 twp, +0x38 mwp, +0x40 pwp.
    pub twp: &'a mut Vec<u8>,
    pub mwp: &'a mut Vec<u8>,
    pub pwp: &'a mut Vec<u8>,
    /// G 0x801CC168.
    pub g: &'a [u8],
}

/// The engine as the walker sees it: calls run at once on the runtime (in
/// call order, so `rand` keeps its sequence). `sa2::walker_exec::Engine` is
/// implemented over this once #456 is on main.
pub struct RuntimeEngine<'a> {
    pub rt: &'a mut Runtime,
    /// MSL `rand` state 0x803AD6A0.
    pub rand_next: &'a mut u32,
}

impl RuntimeEngine<'_> {
    /// An engine routine by GSNE8P address (logged; run when ported).
    pub fn call(&mut self, addr: u32, args: Vec<u32>) {
        self.rt.engine(addr, args);
    }
    /// MSL `rand` 0x8010DC88.
    pub fn rand(&mut self) -> i32 {
        *self.rand_next = self
            .rand_next
            .wrapping_mul(1_103_515_245)
            .wrapping_add(12_345);
        ((*self.rand_next >> 16) & 0x7FFF) as i32
    }
}

/// The walker exec slot. The walker thread's adapter (`walker_frame`,
/// with its own player globals and land context) sits behind this; a mode it
/// has not ported panics with its address, which the slot reports as
/// unported instead of ending the run.
pub trait WalkerExec {
    fn exec(&mut self, w: WalkerWork<'_>, eng: &mut RuntimeEngine<'_>);
}

/// The Steam files a run reads (never committed).
pub struct SteamData {
    /// sonic2app.exe (PhysicsArray).
    pub exe: Vec<u8>,
    /// resource/gd_PC/stg11D.rel.
    pub stg11: Vec<u8>,
    pub set_s: Option<Vec<u8>>,
    pub set_u: Option<Vec<u8>>,
}

struct Host {
    physics: Vec<[u8; 0x84]>,
    rows: Vec<SetRow>,
    defs: Vec<ObjectDef>,
    handle: u32,
    pads: PadSystem,
}

impl PlayerHost for Host {
    fn physics_row(&mut self, row: i32) -> [u8; 0x84] {
        usize::try_from(row)
            .ok()
            .and_then(|r| self.physics.get(r))
            .copied()
            .unwrap_or([0; 0x84])
    }
    fn upgrades(&mut self) -> [u8; 0x1D] {
        [0; 0x1D]
    }
    fn engine_value(&mut self, addr: u32, _args: &[u32]) -> u32 {
        if addr == TEX_BUFFER_SIZE {
            return 0x1_0000;
        }
        self.handle += 1;
        0xC000_0000 + self.handle
    }
    fn collision(&mut self, _task: TaskId) -> u32 {
        0
    }
}

impl SessionHost for Host {
    fn message_tables(&mut self) -> [u32; 6] {
        [0; 6]
    }
    fn word_803ad420(&mut self) -> i32 {
        -1
    }
    fn value(&mut self, addr: u32, _args: &[u32]) -> u32 {
        match addr {
            MESSAGE_LOAD | SAVE_PROBE => 0,
            // A non-null file whenever the join found SET data.
            SET_LOAD if !self.rows.is_empty() => 0xC100_0000,
            _ => 0,
        }
    }
}

impl StageHost for Host {
    fn object_list(&mut self) -> Vec<(i16, i16, u32)> {
        // shortcut: stg11D's object-table entries (.data+0x5B344) are not read;
        // nothing in the run looks at 0x801E5D28 yet.
        Vec::new()
    }
    fn set_rows(&mut self) -> Vec<SetRow> {
        self.rows.clone()
    }
    fn object_defs(&mut self) -> Vec<ObjectDef> {
        self.defs.clone()
    }
}

impl Host {
    fn pad_word(&self, slot: usize, off: usize) -> u32 {
        match self.pads.pointers.get(slot) {
            Some(PadRef::Port(p)) => self
                .pads
                .records
                .get(*p)
                .and_then(|r| r.get(off..off + 4))
                .map_or(0, |b| u32::from_be_bytes([b[0], b[1], b[2], b[3]])),
            _ => 0,
        }
    }
}

impl FrameHost for Host {
    fn ring_count(&mut self, _player: u32) -> i16 {
        0
    }
    fn pad_pressed(&mut self, slot: usize) -> u32 {
        self.pad_word(slot, 0x10)
    }
    fn pad_held(&mut self, slot: usize) -> u32 {
        self.pad_word(slot, 0x8)
    }
    fn pad_copy_pressed(&mut self) -> u32 {
        self.pad_word(0, 0x10)
    }
    fn word_801e53a8(&mut self) -> u32 {
        0
    }
    fn word_801e5398(&mut self, _i: usize) -> u32 {
        0
    }
    fn player_flags(&mut self, _i: usize) -> Option<i16> {
        None
    }
    fn frame_end_value(&mut self) -> f32 {
        0.0
    }
    fn sort_queue_active(&mut self) -> bool {
        false
    }
    fn view_z(&mut self, pos: [f32; 3]) -> f32 {
        pos[2]
    }
}

struct GameNatives<'a> {
    reg: &'a mut Registry,
    walker: Option<&'a mut dyn WalkerExec>,
    g: &'a [u8],
    playertwp: [Option<TaskId>; 2],
    rand_next: &'a mut u32,
}

impl Natives for GameNatives<'_> {
    fn run(&mut self, rt: &mut Runtime, addr: u32, task: TaskId) -> bool {
        if addr != WALKER_EXEC {
            return self.reg.run(rt, addr, task);
        }
        let Some(walker) = self.walker.as_deref_mut() else {
            return false;
        };
        let Some(t) = rt.task_mut(task) else {
            return false;
        };
        let mut blocks = (t.twp.take(), t.w38.take(), t.w40.take());
        let ran = match &mut blocks {
            (Some(twp), Some(mwp), Some(pwp)) => {
                let player = self
                    .playertwp
                    .iter()
                    .position(|p| *p == Some(task))
                    .unwrap_or(0);
                let (g, rand_next) = (self.g, &mut *self.rand_next);
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut eng = RuntimeEngine { rt, rand_next };
                    let w = WalkerWork {
                        task,
                        player,
                        twp,
                        mwp,
                        pwp,
                        g,
                    };
                    walker.exec(w, &mut eng);
                }))
                .is_ok()
            }
            _ => false,
        };
        if let Some(t) = rt.task_mut(task) {
            (t.twp, t.w38, t.w40) = blocks;
        }
        ran
    }
}

/// A running Iron Gate.
pub struct Game {
    pub rt: Runtime,
    pub reg: Registry,
    pub session: Session,
    pub stage: StageState,
    pub frame: GameState,
    pub walker: Option<Box<dyn WalkerExec>>,
    host: Host,
    pub frames: u32,
}

fn task_of(p: Option<Ptr>) -> Option<TaskId> {
    match p? {
        Ptr::Task(t) | Ptr::Twp(t) | Ptr::Mwp(t) => Some(t),
        _ => None,
    }
}

impl Game {
    /// Load Iron Gate as the one-player story does: Mech Eggman (character 7),
    /// stage 11, G+0x1E = 0.
    pub fn iron_gate(data: &SteamData) -> Result<Self, String> {
        let physics = steam_physics_rows(&data.exe).map_err(|e| e.to_string())?;
        let rel = steam_stg11(&data.stg11)?;
        let defs = object_list(&rel, LAYOUT, steam_dol_to_gsne8p)?;
        let rows = join_files(data.set_s.as_deref(), data.set_u.as_deref()).unwrap_or_default();
        let mut host = Host {
            physics,
            rows,
            defs,
            handle: 0,
            pads: PadSystem::default(),
        };
        let mut rt = Runtime::new();
        let mut reg = Registry::new();
        let mut session = Session::new();
        session.players = Players {
            g: vec![0; 0xF0],
            stage: 11,
            chars: [7, 0],
            ..Players::default()
        };
        session_init(&mut rt, &mut session, &mut host);
        let mut stage = StageState::new(LAYOUT);
        stage_load(&mut rt, &mut reg, &mut session, &mut stage, &mut host);
        let mut frame = GameState {
            stage: 11,
            mode: 0x10,
            viewports: session.camera.viewports,
            ..GameState::default()
        };
        frame.g.copy_from_slice(&session.players.g[..0xF0]);
        // Input on for player 0 (G+0x3E == 1, G+0x3F player 0).
        frame.g[0x3E] = 1;
        frame.g[0x3F] = 1;
        for (i, t) in frame.player_tasks.iter_mut().enumerate() {
            *t = task_of(session.players.tp[i]);
        }
        for i in 0..2 {
            stage.set.table.playertwp[i] = task_of(session.players.twp[i]);
        }
        Ok(Self {
            rt,
            reg,
            session,
            stage,
            frame,
            walker: None,
            host,
            frames: 0,
        })
    }

    /// One frame with these raw pads. Returns the pause request.
    pub fn step(&mut self, pads: [PadStatus; 4]) -> u32 {
        self.rt.events.clear();
        let mut ev = Vec::new();
        self.host.pads.read(pads, &mut ev);
        let ctx = PadContext {
            g: self.frame.g.to_vec(),
            camera_active: true,
            viewports: self.frame.viewports,
            camera_angles: [0; 8],
        };
        self.host.pads.tick(&ctx, &mut ev);
        self.rt.events.extend(ev);
        let g = self.frame.g;
        let mut natives = GameNatives {
            reg: &mut self.reg,
            walker: self.walker.as_deref_mut().map(|w| w as &mut dyn WalkerExec),
            g: &g,
            playertwp: self.stage.set.table.playertwp,
            rand_next: &mut self.session.rand_next,
        };
        let mut with_set = WithSet {
            inner: &mut natives,
            set: &mut self.stage.set,
        };
        self.frame.tick_index = 0;
        let pause = stage_frame(&mut self.rt, &mut with_set, &mut self.frame, &mut self.host);
        self.frames += 1;
        pause
    }

    /// A digest of everything the run keeps (tasks, events, globals).
    pub fn digest(&self) -> u64 {
        let mut h = DefaultHasher::new();
        format!("{:?}{:?}{:?}", self.rt, self.stage.set, self.frame).hash(&mut h);
        h.finish()
    }
}
