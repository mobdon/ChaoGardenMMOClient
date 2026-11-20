//! Session init (milestone E4 part 2; ENGINE-MAP.md 4.1): `fn_80021828`, run
//! by the top-level sequence before a stage. It resets the stage mode and the
//! pause state, picks one or two viewports (G+0x20/+0x21), clears the task
//! lists, creates the camera task and loads the players.
//!
//! The engine routines it calls are logged as [`Event::Engine`] and not run.
//! `fn_800DEA3C` (HUD/message handles) and `fn_800DB604` return values come
//! from a [`SessionHost`]; while `fn_800DEA3C` is not ported it returns 0 and
//! the writes into the returned object are skipped, as main.dol skips them for
//! a null result.
use crate::camera::{camera_init, Camera};
use crate::player::{load_players, PlayerHost, Players};
use crate::task::Runtime;

pub const MESSAGE_LOAD: u32 = 0x800D_EA3C;
pub const SAVE_PROBE: u32 = 0x800D_B604;
/// The 0x8000-word buffer at 0x817BFE60 the init clears.
pub const BUFFER: u32 = 0x817B_FE60;
const FIRST: [u32; 6] = [
    0x8006_4428,
    0x8006_445C,
    0x8006_4470,
    0x8006_4414,
    0x800D_DA3C,
    0x8002_7C20,
];
const AFTER_PLAYERS: [u32; 4] = [0x8006_6488, 0x8002_4B2C, 0x8001_8E5C, 0x8001_2CC0];
const LAST: [u32; 3] = [0x8006_7D58, 0x8006_CA48, 0x8001_9CCC];

/// What `fn_80021828` needs from engine parts that are not ported.
pub trait SessionHost: PlayerHost {
    /// `[0x803ADA5C]`, `[[0x803ADA5C]+0xC]`, `[0x803ADA54]` and its +0x8, +0xC,
    /// +0x10 words (message table bases and offsets).
    fn message_tables(&mut self) -> [u32; 6];
    /// The word 0x803AD420.
    fn word_803ad420(&mut self) -> i32;
    /// What an unported routine returns.
    fn value(&mut self, addr: u32, args: &[u32]) -> u32;
}

/// The globals the session init writes, plus the player and camera state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Session {
    pub players: Players,
    pub camera: Camera,
    /// 0x803AD81A, 0x803AD81C, 0x803AD818.
    pub mode: i16,
    pub exit_request: i16,
    pub b818: u8,
    /// 0x803AD830, 0x803AD832, 0x803AD834.
    pub h830: [i16; 3],
    /// 0x803AD837, 0x803AD839.
    pub pause_cursor: u8,
    pub pause_flag: i8,
    /// 0x803AD83C, 0x803AD840, 0x803AD844, 0x803AD848.
    pub messages: [u32; 4],
    /// MSL `rand` state `next` 0x803AD6A0 (`srand` 0x8010DC80 stores it).
    pub rand_next: u32,
    /// 0x817BFE60 cleared (0x8000 words).
    pub buffer_cleared: bool,
}

impl Session {
    /// A session at boot: `rand_next` is 1, as MSL starts it.
    pub fn new() -> Self {
        Self {
            rand_next: 1,
            ..Self::default()
        }
    }
    fn g(&self, off: usize) -> u8 {
        self.players.g.get(off).copied().unwrap_or(0)
    }
    fn set_g(&mut self, off: usize, v: u8) {
        if let Some(b) = self.players.g.get_mut(off) {
            *b = v;
        }
    }
}

fn value(rt: &mut Runtime, host: &mut dyn SessionHost, addr: u32, args: Vec<u32>) -> u32 {
    let v = host.value(addr, &args);
    rt.engine(addr, args);
    v
}

/// `fn_80021828`. Returns 1.
pub fn session_init(rt: &mut Runtime, s: &mut Session, host: &mut dyn SessionHost) -> u32 {
    s.exit_request = 0;
    s.mode = 3;
    s.b818 = 0;
    if s.g(0x4D) as i8 == 0 {
        s.mode = 0;
        for a in FIRST {
            rt.engine(a, vec![]);
        }
    }
    s.h830 = [-1; 3];
    rt.engine(0x8002_309C, vec![]);
    s.pause_cursor = 0;
    s.pause_flag = 0;
    let stage = s.players.stage;
    if s.g(0x1E) as i8 != 0 {
        s.set_g(0x23, 0);
        let mech = |c: i32| (c.wrapping_sub(6) as u32) <= 1;
        let g1f = s.g(0x1F) as i8;
        if mech(s.players.chars[0]) || mech(s.players.chars[1]) || g1f == 1 || g1f == 4 {
            s.set_g(0x20, 1);
        } else if stage != 0x47 {
            s.set_g(0x20, 0);
        }
    } else {
        let reset_23 = if stage > 0x3C && stage != 0x46 {
            true
        } else {
            stage == 0x2A
                || (stage.wrapping_sub(0x13) as u16) <= 1
                || stage == 0x1D
                || stage == 0x21
        };
        if reset_23 {
            s.set_g(0x23, 0);
        }
        s.set_g(0x20, 0);
    }
    rt.clear_btp();
    rt.engine(0x8007_1FA8, vec![]);
    let t = host.message_tables();
    let g11 = s.g(0x11) as i8 as i32 as u32;
    s.messages[0] = value(rt, host, MESSAGE_LOAD, vec![t[0].wrapping_add(t[1]), g11]);
    for (i, k) in [3usize, 4, 5].into_iter().enumerate() {
        s.messages[1 + i] = value(rt, host, MESSAGE_LOAD, vec![t[2].wrapping_add(t[k]), g11]);
    }
    rt.engine(0x8007_2820, vec![0x8017_4190, 0x803A_D368]);
    if s.g(0) as i8 != 0 {
        s.rand_next = 0xDEAD_0CAB;
    }
    if stage == 0x5A {
        s.set_g(0x20, 1);
        s.set_g(0x21, 1);
        camera_init(rt, &mut s.camera, stage, 4);
        rt.engine(0x8001_A8A8, vec![]);
        s.set_g(0x22, 1);
    } else if s.g(0x20) as i8 != 0 {
        let n = if s.g(0x21) as i8 != 0 { 4 } else { 2 };
        camera_init(rt, &mut s.camera, stage, n);
    } else {
        camera_init(rt, &mut s.camera, stage, 1);
    }
    s.buffer_cleared = true;
    if !(0x46..0x48).contains(&stage) {
        rt.engine(0x8005_03C8, vec![]);
        load_players(rt, &mut s.players, host);
    } else {
        rt.engine(0x800C_3C8C, vec![]);
    }
    for a in AFTER_PLAYERS {
        let args = if a == 0x8001_2CC0 { vec![0] } else { vec![] };
        rt.engine(a, args);
    }
    for a in LAST {
        rt.engine(a, vec![]);
    }
    s.set_g(0x47, 0);
    s.set_g(0x14, 0);
    s.set_g(0x15, 0);
    rt.engine(0x800D_CBC8, vec![]);
    if host.word_803ad420() >= 0 && value(rt, host, SAVE_PROBE, vec![]) as i32 == -1 {
        s.set_g(0x15, 1);
    }
    1
}
