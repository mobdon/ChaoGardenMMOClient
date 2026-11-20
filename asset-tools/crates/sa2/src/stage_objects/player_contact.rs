//! The player-contact routines of main.dol (GSNE8P) that the stage objects
//! call: the collision hit searches, the attack checks, the item effects on
//! the player, rings, score, lives and mech HP, and the routines that push,
//! place, turn or knock back a player.
//!
//! They are main.dol code, not stage REL code, so [`ROUTINES`] (stg42D
//! offsets) is empty; [`DOL_ROUTINES`] names each one by its GSNE8P address
//! with the SHA-256 of its bytes in main.dol (a DOL is not relocated, so the
//! bytes are the code). `tests/so_contact.rs` checks those hashes and
//! replays `tests/so_contact_ppc.py`, which runs each routine itself under
//! the main.dol harness on randomized player, task and collision memory and
//! records every byte it reads and writes.
//!
//! **Memory.** The routines work on engine memory: the player tables
//! (`playerpwp` 0x801E7728, `playermwp` 0x801E7748, `playertwp` 0x801E7768,
//! four slots each), the game-state block `lbl_801CC168`, the collision
//! search state (`CCL_SEARCH` 0x801DF680), and small-data words. The port
//! reads and writes it through [`Ram`] at the same addresses with the same
//! widths, in the same order, so a host that keeps the game's memory gets
//! the same bytes, quirks included (see each function). Constants the code
//! loads from `.sdata2`, `.rodata` and its jump tables are written out here
//! with their addresses.
//!
//! **Hooks.** Calls into engine code that is not ported here (sound, voice,
//! music, the barrier/invincibility task creators, task creation, MAlloc,
//! the 2P ring rank) go to [`ContactHost`], named after their routine.
//!
//! **Matrix work.** SetVelocityP and fn_80039EB0 run on the shared native
//! Ninja matrix port (`ninja_matrix`, checked against main.dol by
//! `tests/ninja_matrix_dol.rs`) on a stack of their own; the game's matrix
//! stack is pushed and popped back, so only the result is visible.
//!
//! **Defaults for the object hosts.** The `*_default` functions at the end
//! do what a round-1 object host method (or event) stands for, using these
//! ports; the module docs of each name which method they replace.
use super::host::V3;
use crate::ninja_matrix::{rotate_by_angles, unrotate_by_angles, MatrixStack};

/// No stage-REL routines: everything here is main.dol code.
pub const ROUTINES: &[super::Routine] = &[];

/// A ported main.dol routine: its GSNE8P address, length and the SHA-256 of
/// its bytes in main.dol.
#[derive(Debug, Clone, Copy)]
pub struct DolRoutine {
    pub name: &'static str,
    pub addr: u32,
    pub len: u32,
    pub sha256: &'static str,
}

macro_rules! dol {
    ($name:expr, $addr:expr, $len:expr, $sha:expr) => {
        DolRoutine {
            name: $name,
            addr: $addr,
            len: $len,
            sha256: $sha,
        }
    };
}

/// Every main.dol routine this module ports.
pub const DOL_ROUTINES: &[DolRoutine] = &[
    dol!(
        "fn_800059C8",
        0x800059c8,
        0x13c,
        "ce76a0e5ac9c38f41ad23c7df12a3b694cfd87880cb6b3a14d2d4c087b0dc46a"
    ),
    dol!(
        "fn_80005904",
        0x80005904,
        0x78,
        "a48545f17c842be7537989a58bcc410408376bea81fd02bdfb69264e154f0a8c"
    ),
    dol!(
        "fn_80005B04",
        0x80005b04,
        0x34,
        "d0f7c65ed035b1627eb3ab44f2cc3af89818f10ad270d818fdde489044d0b8a7"
    ),
    dol!(
        "fn_80006428",
        0x80006428,
        0x14c,
        "7dafec92bb4354219eae7e64f1b7da0ecad56569d67bea8a2b6cb3caceed842f"
    ),
    dol!(
        "fn_80006574",
        0x80006574,
        0x34,
        "92e90016ae13d07e719d601fa62a7088ad2e82016ca955cd1f0a56ea115b2f3f"
    ),
    dol!(
        "CCL_IsHitPlayerEx",
        0x800065a8,
        0x14c,
        "3b41d0bec05f281a00616c228ffe6b742b7efc80ab3416f47d5461ea30013583"
    ),
    dol!(
        "CCL_IsHitPlayer",
        0x800066f4,
        0x34,
        "92e90016ae13d07e719d601fa62a7088ad2e82016ca955cd1f0a56ea115b2f3f"
    ),
    dol!(
        "fn_800067C0",
        0x800067c0,
        0x38,
        "10d9feb75e1ef5a1149dab11a1d7db142884b84843942fa34c55e359a5d79616"
    ),
    dol!(
        "CCL_ClearSearch",
        0x80006d7c,
        0x24,
        "9e74f6fb803feb716a57ad376321e5d6503df160f42308c20f5091e158d1b5f3"
    ),
    dol!(
        "_rename_GetStageNum",
        0x8001dd78,
        0x8,
        "72ef9e4c36516feb15d61bb74d54e86d39f142e53b0a96f7db26b72ecf8e11c1"
    ),
    dol!(
        "OffControl3D",
        0x8002b2d8,
        0x10,
        "2b080e32c66a028e51fe16e313cdd719dd8ae40b9b3aebf87c9461e14249832f"
    ),
    dol!(
        "OnControl3D",
        0x8002b2e8,
        0x10,
        "18a948469742edf31d2ab68ea6c25938355f65c51416c370629a52a308161a70"
    ),
    dol!(
        "fn_8002FB2C",
        0x8002fb2c,
        0x1b4,
        "bb8ebb0ccaf829e74fd37cee7e1c980e35fd9916eaf3892b18e07158fc806465"
    ),
    dol!(
        "fn_8002FCE0",
        0x8002fce0,
        0xec,
        "d7e3c601d10ec91a6493e68536530426b35aeb34d93ed552984a3cefac3da711"
    ),
    dol!(
        "AddMechHP",
        0x80035cf0,
        0xe0,
        "0dd08a99bd230f657cdbd9fb32b6295cb8f9a7758fd482121d50c198c205e665"
    ),
    dol!(
        "fn_800368A0",
        0x800368a0,
        0xfc,
        "6ae2640754cb764dd11f5fd405a1b9ca7d1eede69c4e771b99d7e2968f6df8c9"
    ),
    dol!(
        "fn_8003699C",
        0x8003699c,
        0x230,
        "2cec0da477905ffd183665805596362bcac54c72a00bdabfd600ab6a739f3504"
    ),
    dol!(
        "fn_800373B8",
        0x800373b8,
        0x128,
        "16304a3f8168f0483a89f781b49f4fa75f68dafd456fc6cfeb1c38d00c9a3c18"
    ),
    dol!(
        "fn_800374E0",
        0x800374e0,
        0x11c,
        "45b05b584b44f68fcb0cda35865c7923cd76172407625549a75dcf1b7787403b"
    ),
    dol!(
        "fn_800375FC",
        0x800375fc,
        0x11c,
        "c81b49833e7cd8b1b269044f5f59aea39a4f68688d7058600ba4945f6114178d"
    ),
    dol!(
        "fn_80037718",
        0x80037718,
        0x84,
        "b2fc85fc5c0be0659e0b349ff31524a3a509a8b89cdb76e70a0b5410252166bf"
    ),
    dol!(
        "IsThisTaskPlayer",
        0x80037edc,
        0x88,
        "cbaf224ef58645122d9678d0b06f3181a0190e6bf9c3f2ef5b129ade2e15cc54"
    ),
    dol!(
        "fn_800399BC",
        0x800399bc,
        0x6c,
        "e9bc434f04603f4f73cb9051ad9632d232723a186b65b2721393dbfa09dd1dc8"
    ),
    dol!(
        "fn_80039EB0",
        0x80039eb0,
        0x140,
        "e849b9c9c7d07a7d22e37f40567dec29ccb4ddc5e100f7d503947dfe7a9e5abc"
    ),
    dol!(
        "SetVelocityP",
        0x8003a3dc,
        0x12c,
        "89a624ad7f2e3eca374ae48dc876b02fbdc4a61bc99fe29057c8f538e5262c96"
    ),
    dol!(
        "SetInputP",
        0x8003a508,
        0x44,
        "e7f2f4c3a44c09bd4f41a093a7238b3b0990b4892571afe1725ca3dc093eeb57"
    ),
    dol!(
        "fn_8003A54C",
        0x8003a54c,
        0x6c,
        "4599764b7ed85afdd0792a04a0007a51e1bce9419b849f6a1c7297ea39331847"
    ),
    dol!(
        "AddScore",
        0x8006378c,
        0x60,
        "d7bffb500ac18d43b65f264b431b7f46e770684219ad98c8a9aa57817836d1a3"
    ),
    dol!(
        "AddNumRing",
        0x800637ec,
        0x648,
        "e650dceab521c597e5c2e5cce4a9265cf6d3e9629ddb8ef1050d8de002266218"
    ),
    dol!(
        "fn_800641B8",
        0x800641b8,
        0xa0,
        "55865fa1a5ce2f4d52c83631fffdad04cd20b7410a99d4a7e9b6284ac406b21c"
    ),
];

// --- memory --------------------------------------------------------------

/// `playerpwp[4]` (.bss 0x801E7728): each player's own work.
pub const PLAYER_PWP: u32 = 0x801e_7728;
/// `playermwp[4]` (.bss 0x801E7748): each player's motion work.
pub const PLAYER_MWP: u32 = 0x801e_7748;
/// `playertwp[4]` (.bss 0x801E7768): each player's task work.
pub const PLAYER_TWP: u32 = 0x801e_7768;
/// `lbl_801CC168` (.data): the game-state block (lives at +0x64, rings at
/// +0x68, score at +0x90, the 2P flag at +0x1E).
pub const GAME: u32 = 0x801c_c168;
/// The collision search state (`CCL_SEARCH`: last task, last search
/// routine, current hit index, kind), 0x801DE600 + 0x1080 (`lis -0x7FE2`, `addi -0x1A00`).
pub const CCL_SEARCH: u32 = 0x801d_f680;
/// r13 - 0x7A60 (0x803AD860): the HUD block (lives at +8, rings at +0xC,
/// score at +0x18).
pub const HUD: u32 = 0x803a_d860;
/// r13 - 0x7AA0 (0x803AD820): the stage number (a halfword).
pub const STAGE_NUM: u32 = 0x803a_d820;
/// r13 - 0x71D8 (0x803AE0E8): the 3D control flags.
pub const CONTROL_3D: u32 = 0x803a_e0e8;
/// r13 - 0x7910 (0x803AD9B0): a byte per player, the index into its
/// position log.
pub const POS_LOG_INDEX: u32 = 0x803a_d9b0;
/// r13 - 0x7908 (0x803AD9B8): the position log (12 bytes a point, 256
/// points a player).
pub const POS_LOG: u32 = 0x803a_d9b8;
/// r13 - 0x78C4 (0x803AD9FC): the word whose bit 0 alternates two voices.
pub const VOICE_TOGGLE: u32 = 0x803a_d9fc;
/// `lbl_801E5358` (.bss): each player's controller port, or -1.
pub const VIB_PORT: u32 = 0x801e_5358;
/// `lbl_80174340` (.rodata): port to motor channel (0, 1, 2, 3).
pub const VIB_CHANNEL: u32 = 0x8017_4340;
const VIB_CHANNELS: [u32; 4] = [0, 1, 2, 3];
/// `lbl_801964F0` (.data): the vibration task of each channel.
pub const VIB_TASKS: u32 = 0x8019_64f0;

/// The engine memory the routines read and write (big-endian, at GSNE8P
/// addresses).
pub trait Ram {
    fn r8(&mut self, addr: u32) -> u8;
    fn w8(&mut self, addr: u32, v: u8);

    fn r16(&mut self, a: u32) -> u16 {
        u16::from_be_bytes([self.r8(a), self.r8(a.wrapping_add(1))])
    }
    fn r32(&mut self, a: u32) -> u32 {
        u32::from_be_bytes([
            self.r8(a),
            self.r8(a.wrapping_add(1)),
            self.r8(a.wrapping_add(2)),
            self.r8(a.wrapping_add(3)),
        ])
    }
    fn w16(&mut self, a: u32, v: u16) {
        for (k, b) in v.to_be_bytes().into_iter().enumerate() {
            self.w8(a.wrapping_add(k as u32), b);
        }
    }
    fn w32(&mut self, a: u32, v: u32) {
        for (k, b) in v.to_be_bytes().into_iter().enumerate() {
            self.w8(a.wrapping_add(k as u32), b);
        }
    }
    /// `lbz` + `extsb`.
    fn rs8(&mut self, a: u32) -> i32 {
        i32::from(self.r8(a) as i8)
    }
    /// `lha`.
    fn rs16(&mut self, a: u32) -> i32 {
        i32::from(self.r16(a) as i16)
    }
    fn rf(&mut self, a: u32) -> f32 {
        f32::from_bits(self.r32(a))
    }
    fn wf(&mut self, a: u32, v: f32) {
        self.w32(a, v.to_bits());
    }
}

/// Engine routines the contact code calls and this module does not port.
pub trait ContactHost {
    /// SE_Call(id, 0, 0, 0) (0x8006B978).
    fn se_call(&mut self, id: u32);
    /// fn_80011714(id): a character voice.
    fn voice(&mut self, id: u32);
    /// Jingle_Play(name) (0x800110A4); `name` is a main.dol address
    /// ("invncibl.adx" at 0x80174388).
    fn jingle_play(&mut self, name: u32);
    /// fn_800BE308(player): the invincibility effect; 0 when it could not
    /// start.
    fn fn_800be308(&mut self, player: i32) -> u32;
    /// fn_800BE374(player): the magnetic barrier; 0 when it could not start.
    fn fn_800be374(&mut self, player: i32) -> u32;
    /// fn_800BE408(player): the barrier; 0 when it could not start.
    fn fn_800be408(&mut self, player: i32) -> u32;
    /// CreateElementalTask(flags, level, exec, name) (0x80071E0C), with
    /// main.dol addresses; the new task, or 0.
    fn create_elemental_task(&mut self, flags: u32, level: u32, exec: u32, name: u32) -> u32;
    /// MAlloc(size) (0x801197D0); 0 on failure.
    fn malloc(&mut self, size: u32) -> u32;
    /// DestroyTask(task) (0x80071AF0).
    fn destroy_task(&mut self, task: u32);
    /// fn_80063E34(player): the 2P ring rank the ring bonus uses (it reads
    /// both players' standing; not ported).
    fn fn_80063e34(&mut self, player: i32) -> i32;
}

fn slot(table: u32, n: i32) -> u32 {
    table.wrapping_add((n as u32) << 2)
}

/// `lha r, 4(twp)`, `ori`, `sth`: set bits of a halfword the way the code
/// does (read sign-extended, store the low half).
fn or16(ram: &mut impl Ram, a: u32, bits: u32) {
    let v = ram.rs16(a) as u32 | bits;
    ram.w16(a, v as u16);
}

// --- collision hits --------------------------------------------------------

/// The search routines' ids (`CCL_SEARCH +4` holds the last one used).
const SEARCH_59C8: u32 = 0x8000_59c8;
const SEARCH_6428: u32 = 0x8000_6428;
const SEARCH_65A8: u32 = 0x8000_65a8;

/// The shared start of the hit searches: a new task or a new search
/// routine restarts the walk (index and kind -1); then the index moves on
/// by one, capped at 16. Returns the index to start from.
fn search_step(ram: &mut impl Ram, task: u32, func: u32) -> u32 {
    if ram.r32(CCL_SEARCH) != task {
        ram.w32(CCL_SEARCH, task);
        ram.w32(CCL_SEARCH + 4, 0);
        ram.w32(CCL_SEARCH + 0xc, u32::MAX);
        ram.w32(CCL_SEARCH + 8, u32::MAX);
    }
    if ram.r32(CCL_SEARCH + 4) != func {
        ram.w32(CCL_SEARCH + 4, func);
        ram.w32(CCL_SEARCH + 0xc, u32::MAX);
        ram.w32(CCL_SEARCH + 8, u32::MAX);
    }
    let n = ram.r32(CCL_SEARCH + 8).wrapping_add(1);
    ram.w32(CCL_SEARCH + 8, n);
    if ram.r32(CCL_SEARCH + 8) as i32 > 0x10 {
        ram.w32(CCL_SEARCH + 8, 0x10);
    }
    ram.r32(CCL_SEARCH + 8)
}

/// The colliwk of a task (`task +0x34` twp, its `+0x2C`), or `None` where
/// the code returns 0 for a null task or twp. `Some(0)` is a null cwp.
fn cwp_of(ram: &mut impl Ram, task: u32) -> Option<u32> {
    if task == 0 {
        return None;
    }
    let twp = ram.r32(task + 0x34);
    if twp == 0 {
        return None;
    }
    Some(ram.r32(twp + 0x2c))
}

/// The records from `cur` to the end of `hit_info[16]` (cwp +0x10, 8 bytes
/// each): the count the code puts in CTR, and the first record; `None`
/// when the start is already past the end.
fn records(cwp: u32, cur: u32) -> Option<(u32, u32)> {
    let rec = cwp.wrapping_add((cur << 3).wrapping_add(0x10));
    let end = cwp.wrapping_add(0x90);
    let count = end.wrapping_add(7).wrapping_sub(rec) >> 3;
    (rec < end).then_some((rec, count))
}

/// The colli id of the task a record names (`hit_tp->twp->cwp->id`), -1
/// when a pointer on the way is null.
fn hit_colli_id(ram: &mut impl Ram, rec: u32) -> i32 {
    let t = ram.r32(rec + 4);
    if t == 0 {
        return -1;
    }
    let twp = ram.r32(t + 0x34);
    if twp == 0 {
        return -1;
    }
    let cwp = ram.r32(twp + 0x2c);
    if cwp == 0 {
        return -1;
    }
    i32::from(ram.r16(cwp))
}

fn search_by_id(ram: &mut impl Ram, task: u32, func: u32, id: i32) -> u32 {
    let mut cur = search_step(ram, task, func);
    let Some(cwp) = cwp_of(ram, task) else {
        return 0;
    };
    if cwp == 0 {
        return 0;
    }
    let Some((mut rec, count)) = records(cwp, cur) else {
        return 0;
    };
    for _ in 0..count {
        if ram.rs8(rec + 1) == -1 {
            return 0;
        }
        if hit_colli_id(ram, rec) == id {
            ram.w32(CCL_SEARCH + 8, cur);
            return rec;
        }
        rec = rec.wrapping_add(8);
        cur = cur.wrapping_add(1);
    }
    0
}

/// CCL_IsHitPlayerEx(task) (0x800065A8): the next hit record (from the
/// search index on) whose task's colli id is 0 (a player), or 0. The walk
/// stops at a record whose `hit_num` is -1.
pub fn ccl_is_hit_player_ex(ram: &mut impl Ram, task: u32) -> u32 {
    search_by_id(ram, task, SEARCH_65A8, 0)
}

/// fn_80006428(task): the same walk for colli id 1.
pub fn fn_80006428(ram: &mut impl Ram, task: u32) -> u32 {
    search_by_id(ram, task, SEARCH_6428, 1)
}

/// fn_800059C8(task): when the task's cwp flag has bit 2, the next record
/// whose flag has bit 2 and whose task is set; else 0.
pub fn fn_800059c8(ram: &mut impl Ram, task: u32) -> u32 {
    let mut cur = search_step(ram, task, SEARCH_59C8);
    let Some(cwp) = cwp_of(ram, task) else {
        return 0;
    };
    if cwp == 0 || ram.r16(cwp + 4) & 2 == 0 {
        return 0;
    }
    let Some((mut rec, count)) = records(cwp, cur) else {
        return 0;
    };
    for _ in 0..count {
        if ram.rs8(rec + 1) == -1 {
            return 0;
        }
        if ram.r16(rec + 2) & 2 != 0 && ram.r32(rec + 4) != 0 {
            ram.w32(CCL_SEARCH + 8, cur);
            return rec;
        }
        rec = rec.wrapping_add(8);
        cur = cur.wrapping_add(1);
    }
    0
}

fn hit_task_of(ram: &mut impl Ram, rec: u32) -> u32 {
    if rec == 0 {
        0
    } else {
        ram.r32(rec + 4)
    }
}

/// CCL_IsHitPlayer(task) (0x800066F4): the player task touching `task`
/// (the record's `hit_tp`), or 0.
pub fn ccl_is_hit_player(ram: &mut impl Ram, task: u32) -> u32 {
    let rec = ccl_is_hit_player_ex(ram, task);
    hit_task_of(ram, rec)
}

/// fn_80006574(task): the touching task of colli id 1, or 0.
pub fn fn_80006574(ram: &mut impl Ram, task: u32) -> u32 {
    let rec = fn_80006428(ram, task);
    hit_task_of(ram, rec)
}

/// fn_80005B04(task): the task of fn_800059C8's record, or 0.
pub fn fn_80005b04(ram: &mut impl Ram, task: u32) -> u32 {
    let rec = fn_800059c8(ram, task);
    hit_task_of(ram, rec)
}

/// CCL_ClearSearch (0x80006D7C).
pub fn ccl_clear_search(ram: &mut impl Ram) {
    ram.w32(CCL_SEARCH, 0);
    ram.w32(CCL_SEARCH + 4, 0);
    ram.w32(CCL_SEARCH + 8, u32::MAX);
    ram.w32(CCL_SEARCH + 0xc, u32::MAX);
}

/// fn_800067C0(task): the cwp's hit count (`nbHit`, sign-extended), 0 for
/// a null task or twp. Quirk: with a null cwp it returns `task` itself.
pub fn fn_800067c0(ram: &mut impl Ram, task: u32) -> u32 {
    match cwp_of(ram, task) {
        None => 0,
        Some(0) => task,
        Some(cwp) => ram.rs16(cwp + 2) as u32,
    }
}

/// fn_80005904(task, i): `&cwp->hit_info[i]` when `0 <= i <= 15` and its
/// task is set, else 0.
pub fn fn_80005904(ram: &mut impl Ram, task: u32, i: i32) -> u32 {
    if !(0..=0xf).contains(&i) {
        return 0;
    }
    let Some(cwp) = cwp_of(ram, task) else {
        return 0;
    };
    if cwp == 0 {
        return 0;
    }
    let rec = cwp.wrapping_add(((i as u32) << 3) + 0x10);
    if ram.r32(rec + 4) != 0 {
        rec
    } else {
        0
    }
}

// --- players ---------------------------------------------------------------

/// IsThisTaskPlayer(task) (0x80037EDC): the player number of a player's
/// task, or -1. A task with a parent is judged by its parent (whose twp is
/// read without a null check): colli kinds 1 to 4 (twp +2) give the pwp's
/// +0 (`task +0x40`); a parentless task of kind 5 gives its twp +1.
pub fn is_this_task_player(ram: &mut impl Ram, task: u32) -> i32 {
    if task == 0 {
        return -1;
    }
    let twp = ram.r32(task + 0x34);
    if twp == 0 {
        return -1;
    }
    let parent = ram.r32(task + 8);
    let t = if parent != 0 {
        parent
    } else {
        if ram.r8(twp + 2) == 5 {
            return ram.rs8(twp + 1);
        }
        task
    };
    let twp = ram.r32(t + 0x34);
    let id = ram.r8(twp + 2);
    if id == 0 || id > 4 {
        return -1;
    }
    let pwp = ram.r32(t + 0x40);
    ram.rs8(pwp)
}

/// .sdata2 0x803AEF94 / 0x803AEF98: SetVelocityP's dead zone.
const VELOCITY_EPS: f32 = 0.001;
/// .sdata2 0x803AEF28: 0.0.
const ZERO: f32 = 0.0;

/// SetVelocityP(n, x, y, z) (0x8003A3DC): the motion work's speed
/// (`mwp +0`) becomes `v`; `v` turned into the player's frame (unit matrix,
/// RotateY(ang.y), RotateX(-ang.x), RotateZ(-ang.z), njCalcVector) with X
/// and Z inside ±0.001 (`fcmpo`, so NaN stays) set to 0.0 becomes the
/// pwp's speed (+0x64).
pub fn set_velocity_p(ram: &mut impl Ram, n: i32, v: V3) {
    let twp = ram.r32(slot(PLAYER_TWP, n));
    let mwp = ram.r32(slot(PLAYER_MWP, n));
    let pwp = ram.r32(slot(PLAYER_PWP, n));
    ram.wf(mwp, v[0]);
    ram.wf(mwp + 4, v[1]);
    ram.wf(mwp + 8, v[2]);
    let ay = ram.r32(twp + 0xc) as i32;
    let ax = ram.r32(twp + 8) as i32;
    let az = ram.r32(twp + 0x10) as i32;
    let mut stack = MatrixStack::new();
    let mut out = unrotate_by_angles(&mut stack, [ax, ay, az], v);
    for k in [0, 2] {
        if out[k] <= VELOCITY_EPS && out[k] >= -VELOCITY_EPS {
            out[k] = ZERO;
        }
    }
    ram.wf(pwp + 0x64, out[0]);
    ram.wf(pwp + 0x68, out[1]);
    ram.wf(pwp + 0x6c, out[2]);
}

/// SetInputP(n, mode, b) (0x8003A508): with a twp, its flag gets 0x1000,
/// its smode (+1) becomes `mode`, and the pwp's +0x28 becomes `b`.
pub fn set_input_p(ram: &mut impl Ram, n: i32, mode: u32, b: u32) {
    let twp = ram.r32(slot(PLAYER_TWP, n));
    if twp == 0 {
        return;
    }
    or16(ram, twp + 4, 0x1000);
    ram.w8(twp + 1, mode as u8);
    let pwp = ram.r32(slot(PLAYER_PWP, n));
    ram.w32(pwp + 0x28, b);
}

/// fn_800399BC(n, x, y, z): the player's position (twp +0x14), and the
/// current point of its position log (`POS_LOG` + 12 * (index + 256 n)).
pub fn fn_800399bc(ram: &mut impl Ram, n: i32, p: V3) {
    let s = slot(PLAYER_TWP, n);
    let twp = ram.r32(s);
    let idx = ram.r8(POS_LOG_INDEX.wrapping_add(n as u32));
    if twp == 0 {
        return;
    }
    ram.wf(twp + 0x14, p[0]);
    let off = u32::from(idx)
        .wrapping_add((n as u32) << 8)
        .wrapping_mul(0xc);
    let t = ram.r32(s);
    ram.wf(t + 0x18, p[1]);
    let t = ram.r32(s);
    ram.wf(t + 0x1c, p[2]);
    let log = ram.r32(POS_LOG);
    ram.wf(log.wrapping_add(off), p[0]);
    let log = ram.r32(POS_LOG);
    ram.wf(log.wrapping_add(off) + 4, p[1]);
    let log = ram.r32(POS_LOG);
    ram.wf(log.wrapping_add(off) + 8, p[2]);
}

/// fn_8003A54C(n, task, at, b): the player looks at `task`: pwp +0x144 = 1,
/// +0x170 = task, +0x158 = `at` (the three words copied; 0.0 each when
/// `at` is null), +0x148 = `b`. Nothing without a pwp or a task.
pub fn fn_8003a54c(ram: &mut impl Ram, n: i32, task: u32, at: Option<[u32; 3]>, b: u32) {
    let pwp = ram.r32(slot(PLAYER_PWP, n));
    if pwp == 0 || task == 0 {
        return;
    }
    ram.w8(pwp + 0x144, 1);
    ram.w32(pwp + 0x170, task);
    match at {
        Some(w) => {
            ram.w32(pwp + 0x158, w[0]);
            ram.w32(pwp + 0x15c, w[1]);
            ram.w32(pwp + 0x160, w[2]);
        }
        None => {
            ram.wf(pwp + 0x160, ZERO);
            ram.wf(pwp + 0x15c, ZERO);
            ram.wf(pwp + 0x158, ZERO);
        }
    }
    ram.w32(pwp + 0x148, b);
}

/// fn_80039EB0(n, spd, ang, timer): knock the player back. Unless its flag
/// has 0x1000 while its smode is 0x24: pwp +0x14 = `timer` (a halfword),
/// flag |= 0x1000, smode = 3, pwp speed (+0x64) = `spd`, ang = (ang.x,
/// 0x4000 - ang.y, ang.z) with mwp +0x1C = the new ang.y, and the motion
/// speed (`mwp +0`) = `spd` turned by the new angles (unit matrix,
/// RotateZ(ang.z), RotateX(ang.x), RotateY(-ang.y)).
pub fn fn_80039eb0(ram: &mut impl Ram, n: i32, spd: V3, ang: [i32; 3], timer: i32) {
    let twp = ram.r32(slot(PLAYER_TWP, n));
    let mwp = ram.r32(slot(PLAYER_MWP, n));
    let flag = ram.rs16(twp + 4);
    let pwp = ram.r32(slot(PLAYER_PWP, n));
    if flag & 0x1000 != 0 && ram.r8(twp + 1) == 0x24 {
        return;
    }
    ram.w16(pwp + 0x14, timer as u16);
    or16(ram, twp + 4, 0x1000);
    ram.w8(twp + 1, 3);
    ram.wf(pwp + 0x64, spd[0]);
    ram.wf(pwp + 0x68, spd[1]);
    ram.wf(pwp + 0x6c, spd[2]);
    ram.w32(twp + 8, ang[0] as u32);
    ram.w32(twp + 0xc, 0x4000u32.wrapping_sub(ang[1] as u32));
    let y = ram.r32(twp + 0xc);
    ram.w32(mwp + 0x1c, y);
    ram.w32(twp + 0x10, ang[2] as u32);
    let az = ram.r32(twp + 0x10) as i32;
    let ax = ram.r32(twp + 8) as i32;
    let ay = ram.r32(twp + 0xc) as i32;
    let mut stack = MatrixStack::new();
    let out = rotate_by_angles(&mut stack, [ax, ay, az], spd);
    ram.wf(mwp, out[0]);
    ram.wf(mwp + 4, out[1]);
    ram.wf(mwp + 8, out[2]);
}

/// OffControl3D(flags) (0x8002B2D8).
pub fn off_control_3d(ram: &mut impl Ram, flags: u32) {
    let v = ram.r32(CONTROL_3D) & !flags;
    ram.w32(CONTROL_3D, v);
}

/// OnControl3D(flags) (0x8002B2E8).
pub fn on_control_3d(ram: &mut impl Ram, flags: u32) {
    let v = ram.r32(CONTROL_3D) | flags;
    ram.w32(CONTROL_3D, v);
}

/// _rename_GetStageNum (0x8001DD78).
pub fn get_stage_num(ram: &mut impl Ram) -> i32 {
    ram.rs16(STAGE_NUM)
}

// --- attack checks ---------------------------------------------------------

/// .sdata2 0x803AEF2C: the speed a spin needs.
const ATTACK_SPEED: f32 = 3.0;

/// fn_8003699C(task, hit): whether the task `hit` attacks `task` (the
/// CONTWOOD test). A hit of colli kind 6 always does; otherwise it must be
/// a player (IsThisTaskPlayer) with a cwp, and then, by its character
/// (pwp +1) and action (twp +0): Sonic or Shadow (0, 1) in actions 0x3D to
/// 0x44 unless `task`'s cwp flag has bit 2; Sonic in action 0x45; the
/// walkers (4, 5) when they touch it (`pwp +0xA4`, or fn_80005B04, or the
/// player search and `pwp +0xA0`) in their attack motions (pwp +0x176 to
/// +0x17A: 0x73, 0xA3, or 0x5E / 0x63 at speed +0x184 >= 3.0), or, when
/// only the player search finds them, in actions 0x47 to 0x4C. It clears
/// the collision search first.
pub fn fn_8003699c(ram: &mut impl Ram, task: u32, hit: u32) -> bool {
    if hit == 0 {
        return false;
    }
    let htwp = ram.r32(hit + 0x34);
    if htwp == 0 {
        return false;
    }
    if ram.r8(htwp + 2) == 6 {
        return true;
    }
    if ram.r32(htwp + 0x2c) == 0 {
        return false;
    }
    let n = is_this_task_player(ram, hit);
    if n == -1 {
        return false;
    }
    let ttwp = ram.r32(task + 0x34);
    let cwp = ram.r32(ttwp + 0x2c);
    let pwp = ram.r32(slot(PLAYER_PWP, n));
    let ptwp = ram.r32(slot(PLAYER_TWP, n));
    ccl_clear_search(ram);
    let touching = ccl_is_hit_player(ram, task);
    let found = if ram.r16(cwp + 4) & 2 != 0 {
        ram.r32(pwp + 0xa4) == task || fn_80005b04(ram, task) == hit
    } else {
        false
    };
    let k = ram.r8(pwp + 1);
    let ks = i32::from(k as i8);
    if ks == 0 || ks == 1 {
        let act = ram.rs8(ptwp);
        if (0x3d..=0x44).contains(&act) && ram.r16(cwp + 4) & 2 == 0 {
            return true;
        }
    }
    if ks == 0 && ram.r8(ptwp) == 0x45 {
        return true;
    }
    let walker = k.wrapping_sub(4) <= 1;
    if walker && (found && touching == hit || ram.r32(pwp + 0xa0) == task) {
        let a = ram.rs16(pwp + 0x178);
        if a == 0x5e && ram.rf(pwp + 0x184) >= ATTACK_SPEED {
            return true;
        }
        if a == 0x63 && ram.rf(pwp + 0x184) >= ATTACK_SPEED {
            return true;
        }
        let b = ram.rs16(pwp + 0x176);
        if b == 0x73 || b == 0xa3 || a == 0x73 || a == 0xa3 {
            return true;
        }
        let c = ram.rs16(pwp + 0x17a);
        if c == 0xa3 || c == 0x73 {
            return true;
        }
    }
    if walker && !found && touching == hit {
        let act = ram.rs8(ptwp);
        if (0x47..=0x4c).contains(&act) {
            return true;
        }
    }
    false
}

/// fn_800368A0(task, hit): fn_8003699C, and the player's equipment
/// (pwp +0x24) has the upgrade that breaks iron: Sonic 0x8, Knuckles
/// (6) 0x80, Tails (4) 0x1000, Shadow 0x40000, Rouge (7) 0x200000,
/// Eggman (5) 0x8000000.
pub fn fn_800368a0(ram: &mut impl Ram, task: u32, hit: u32) -> bool {
    if !fn_8003699c(ram, task, hit) {
        return false;
    }
    let n = is_this_task_player(ram, hit);
    if n == -1 {
        return false;
    }
    let pwp = ram.r32(slot(PLAYER_PWP, n));
    if ram.rs8(pwp + 1) == 0 && ram.r32(pwp + 0x24) & 0x8 != 0 {
        return true;
    }
    let k = ram.r8(pwp + 1);
    let need = match k as i8 {
        6 => 0x80,
        4 => 0x1000,
        1 => 0x4_0000,
        7 => 0x20_0000,
        5 => 0x800_0000,
        _ => return false,
    };
    ram.r32(pwp + 0x24) & need != 0
}

// --- score, rings, lives, HP ----------------------------------------------

/// AddScore's cap (`0x5F5E0FF`).
pub const SCORE_MAX: u32 = 99_999_999;

/// AddScore(n) (0x8006378C): doubled while player 0's pwp +0x10 has
/// 0x8000 (invincible); the score (GAME +0x90) grows, capped at 99999999
/// as an unsigned compare, and the HUD's +0x18 follows.
pub fn add_score(ram: &mut impl Ram, n: i32) {
    let pwp = ram.r32(PLAYER_PWP);
    let mut n = n as u32;
    if pwp != 0 && ram.rs16(pwp + 0x10) & 0x8000 != 0 {
        n <<= 1;
    }
    let mut s = ram.r32(GAME + 0x90).wrapping_add(n);
    if s > SCORE_MAX {
        s = SCORE_MAX;
    }
    ram.w32(GAME + 0x90, s);
    let hud = ram.r32(HUD);
    ram.w32(hud + 0x18, s);
}

/// The lives update AddNumRing and fn_800641B8 share: lives (GAME +0x64 +
/// 2p) += add, capped at 99 (and an overflow past 0x7FFF with a positive
/// add); the HUD's +8 + 2p follows.
fn add_lives(ram: &mut impl Ram, p: u32, add: u32) {
    let a = GAME + 0x64 + 2 * p;
    let v = (ram.rs16(a) as u32).wrapping_add(add);
    ram.w16(a, v as u16);
    let v = ram.rs16(a);
    if v > 0x63 || v < 0 && (add as i16) > 0 {
        ram.w16(a, 0x63);
    }
    let hud = ram.r32(HUD);
    let v = ram.r16(a);
    ram.w16(hud.wrapping_add(2 * p) + 8, v);
}

/// fn_800641B8(player, count): extra lives (SE 0x1008 when `count` > 0);
/// the player number is masked to 0 or 1.
pub fn fn_800641b8(ram: &mut impl Ram, host: &mut impl ContactHost, player: i32, count: i32) {
    let p = player as u32 & 1;
    if count as i16 > 0 {
        host.se_call(0x1008);
    }
    add_lives(ram, p, count as u32);
}

/// AddMechHP(n, hp) (0x80035CF0): for a mech (pwp +1 is 6 or 7) the HP at
/// pwp +0x48 grows by `hp` (`fadds`), clamped to 0.0 and to 10.0 (12.0
/// with the armour bit 0x800000 at +0x24). Reaching the top from below
/// plays a voice for characters 13 (0xA88) and 14 (0xA93).
pub fn add_mech_hp(ram: &mut impl Ram, host: &mut impl ContactHost, n: i32, hp: f32) {
    let pwp = ram.r32(slot(PLAYER_PWP, n));
    if pwp == 0 {
        return;
    }
    let k = ram.rs8(pwp + 1);
    if k != 6 && k != 7 {
        return;
    }
    let old = ram.rf(pwp + 0x48);
    ram.wf(pwp + 0x48, old + hp);
    // .sdata2 0x803AEF20 (10.0) and 0x803AEF24 (2.0).
    let mut max = 10.0f32;
    if ram.r32(pwp + 0x24) & 0x0080_0000 != 0 {
        max += 2.0;
    }
    let v = ram.rf(pwp + 0x48);
    if v <= ZERO {
        ram.wf(pwp + 0x48, ZERO);
        return;
    }
    if v >= max {
        ram.wf(pwp + 0x48, max);
        if old == max {
            return;
        }
        match ram.rs8(pwp + 3) {
            0xe => host.voice(0xa93),
            0xd => host.voice(0xa88),
            _ => {}
        }
    }
}

/// The ring bonus thresholds (r6, r7, r8) AddNumRing picks by character
/// (jump table 0x801B59E0) and the 2P rank.
fn ring_steps(chr: i32, kind: i32, rank: i32) -> (i32, i32, i32) {
    const A: (i32, i32, i32) = (10, 20, 30);
    const B: (i32, i32, i32) = (20, 40, 60);
    const C: (i32, i32, i32) = (5, 10, 15);
    const D: (i32, i32, i32) = (15, 30, 45);
    match chr as u32 {
        8 => A,
        13 => {
            if rank >= 5 {
                C
            } else if rank >= 2 {
                A
            } else {
                D
            }
        }
        4 | 5 | 15 | 16 => {
            if rank >= 3 && rank != 5 {
                A
            } else {
                B
            }
        }
        6 | 7 | 14 => {
            if rank >= 5 {
                C
            } else if rank >= 2 || chr == 7 && kind == 1 {
                A
            } else {
                B
            }
        }
        _ => {
            if rank >= 2 {
                A
            } else {
                B
            }
        }
    }
}

/// The bonus box shift: rank 1 or 2 doubles it (to 4 at most).
fn ring_shift(bits: u32) -> u32 {
    let b = bits << 1;
    if b & 0xffff_fff8 != 0 {
        4
    } else {
        b
    }
}

/// AddNumRing(player, count) (0x800637EC): rings (GAME +0x68 + 2p, p =
/// player & 1) += count, clamped to 0..=999, the HUD's +0xC + 2p follows.
/// Outside 2P (GAME +0x1E == 0) each new hundred is an extra life (SE
/// 0x1008). In 2P (with the player's twp, and not on stages 0x46/0x47) the
/// rings may pass one of three thresholds (by character and the 2P rank,
/// fn_80063E34); the bonus box (bit 1, 2 or 4, adjusted by the player's
/// pwp +1 and rank) is marked at GAME +0x2D + 3p, a character voice plays
/// (jump table 0x801B59BC), and GAME +0x33 + p becomes 0x3C.
pub fn add_num_ring(ram: &mut impl Ram, host: &mut impl ContactHost, player: i32, count: i32) {
    let p = player as u32 & 1;
    let ring = GAME + 0x68 + 2 * p;
    let old = ram.rs16(ring);
    ram.w16(ring, (old as u32).wrapping_add(count as u32) as u16);
    let v = ram.rs16(ring);
    if v > 0x3e7 {
        ram.w16(ring, 0x3e7);
    } else if v < 0 {
        ram.w16(ring, 0);
    }
    let new = ram.rs16(ring);
    let (h_old, h_new) = (old / 100, new / 100);
    if h_old < h_new && ram.rs8(GAME + 0x1e) == 0 {
        let d = h_new - h_old;
        if d as i16 > 0 {
            host.se_call(0x1008);
        }
        add_lives(ram, p, i32::from(d as i16) as u32);
    }
    let hud = ram.r32(HUD);
    let v = ram.r16(ring);
    ram.w16(hud.wrapping_add(2 * p) + 0xc, v);
    if ram.rs8(GAME + 0x1e) == 0 {
        return;
    }
    if ram.r32(slot(PLAYER_TWP, p as i32)) == 0 {
        return;
    }
    if get_stage_num(ram) == 0x46 || get_stage_num(ram) == 0x47 {
        return;
    }
    let pslot = slot(PLAYER_PWP, p as i32);
    let pwp = ram.r32(pslot);
    let chr = ram.rs8(pwp + 3);
    let kind = ram.rs8(pwp + 2);
    let rank = host.fn_80063e34(p as i32);
    let (s1, s2, s3) = ring_steps(chr, kind, rank);
    let m = i32::from((old - old / s3 * s3) as i16);
    let reach = i32::from((ram.rs16(ring) - old + m) as i16);
    let mut bits = 0u32;
    if m < s1 && reach >= s1 {
        bits = 1;
    }
    if m < s2 && reach >= s2 {
        bits = 2;
    }
    if m < s3 && reach >= s3 {
        bits = 4;
    }
    if bits != 0 {
        let pwp = ram.r32(pslot);
        let mode = ram.rs8(pwp + 1);
        bits = match mode {
            0 | 1 => {
                if chr == 0 && kind == 1 {
                    2
                } else if chr == 1 && kind == 1 || rank == 3 || rank == 6 {
                    4
                } else if rank == 5 {
                    2
                } else if (rank - 1) as u32 <= 1 {
                    ring_shift(bits)
                } else {
                    bits
                }
            }
            4 | 5 => {
                if chr == 4 && kind == 1 {
                    2
                } else if chr == 5 && kind == 1 || rank == 4 || rank >= 7 {
                    4
                } else if rank < 2 {
                    bits
                } else {
                    ring_shift(bits)
                }
            }
            6 | 7 => {
                if chr == 6 && kind == 1 {
                    0
                } else if chr == 7 && kind == 1 || rank == 3 || rank == 6 {
                    4
                } else if (rank - 1) as u32 <= 1 {
                    ring_shift(bits)
                } else {
                    bits
                }
            }
            _ => bits,
        };
    }
    let mut voice = -1i32;
    let boxes = GAME + 0x2d + 3 * p;
    for (bit, at) in [(1, 0), (2, 1), (4, 2)] {
        if bits & bit != 0 {
            for k in 0..3 {
                ram.w8(boxes + k, u8::from(k == at));
            }
            voice = 0;
        }
    }
    if voice != 0 || ram.r32(pslot) == 0 {
        return;
    }
    voice = match chr as u32 {
        0 => 0x5ef,
        1 => 0x5ee,
        4 => 0x4dc,
        5 => 0x4dd,
        6 => 0x661,
        7 => {
            let other = slot(PLAYER_PWP, (!p & 1) as i32);
            let o = ram.r32(other);
            if ram.r8(o + 3) == 6 {
                0x660
            } else {
                0x6c4
            }
        }
        8 => {
            if ram.rs8(GAME + 0x12) != 0 {
                0xa75
            } else {
                let w = ram.r32(VOICE_TOGGLE);
                let id = if w & 1 != 0 { 0xa6f } else { 0xa7c };
                let w = ram.r32(VOICE_TOGGLE);
                ram.w32(VOICE_TOGGLE, w ^ 1);
                id
            }
        }
        _ => 0,
    };
    if voice > 0 {
        host.voice(voice as u32);
    }
    ram.w8(GAME + 0x33 + p, 0x3c);
}

// --- item effects ------------------------------------------------------------

/// The voice each character (pwp +3) says on invincibility (jump table
/// 0x80196F84), -1 for none.
const VOICE_INVINCIBLE: [i32; 15] = [
    0x613, 0x610, 0x683, 0x682, 0x508, 0x509, 0x683, 0x682, 0xa75, -1, -1, -1, -1, 0xa85, 0xa90,
];
/// The voice on a barrier or magnetic barrier (jump tables 0x80196FC0 and
/// 0x80196FFC, identical).
const VOICE_BARRIER: [i32; 15] = [
    0x611, 0x60e, 0x681, 0x680, 0x4fe, 0x4ff, 0x681, 0x680, 0xa75, -1, -1, -1, -1, 0xa85, 0xa90,
];

/// "invncibl.adx" (.rodata 0x80174388).
pub const JINGLE_INVINCIBLE: u32 = 0x8017_4388;

/// The player check of the barrier and invincibility items: a number above
/// 7 whose twp slot (read past the table) is empty becomes 0; others stay.
fn item_player(ram: &mut impl Ram, n: i32) -> i32 {
    // The code tests `n < 1` first, then `n <= 7`.
    if n > 7 && ram.r32(slot(PLAYER_TWP, n)) == 0 {
        0
    } else {
        n
    }
}

fn item_voice(ram: &mut impl Ram, host: &mut impl ContactHost, pwp: u32, table: &[i32; 15]) {
    let c = ram.rs8(pwp + 3) as u32;
    if let Some(&v) = table.get(c as usize) {
        if v != -1 {
            host.voice(v as u32);
        }
    }
}

/// fn_800373B8(player): invincibility. Unless the pwp's +0x10 already has
/// 0x8000: fn_800BE308 starts the effect, the flag gets 0x8000, the
/// invincibility jingle plays and the character's voice.
pub fn fn_800373b8(ram: &mut impl Ram, host: &mut impl ContactHost, player: i32) {
    let n = item_player(ram, player);
    let pwp = ram.r32(slot(PLAYER_PWP, n));
    if pwp == 0 || ram.rs16(pwp + 0x10) & 0x8000 != 0 {
        return;
    }
    if host.fn_800be308(n) == 0 {
        return;
    }
    or16(ram, pwp + 0x10, 0x8000);
    host.jingle_play(JINGLE_INVINCIBLE);
    item_voice(ram, host, pwp, &VOICE_INVINCIBLE);
}

/// The barriers: bit 1 started by fn_800BE408, bit 2 (magnetic) by
/// fn_800BE374.
fn barrier(ram: &mut impl Ram, host: &mut impl ContactHost, player: i32, bit: u32) {
    let n = item_player(ram, player);
    let pwp = ram.r32(slot(PLAYER_PWP, n));
    if pwp == 0 || ram.rs16(pwp + 0x10) as u32 & bit != 0 {
        return;
    }
    let started = if bit == 1 {
        host.fn_800be408(n)
    } else {
        host.fn_800be374(n)
    };
    if started == 0 {
        return;
    }
    let f = ram.rs16(pwp + 0x10) as u32 & 0xffff_fffc;
    ram.w16(pwp + 0x10, f as u16);
    or16(ram, pwp + 0x10, bit);
    item_voice(ram, host, pwp, &VOICE_BARRIER);
}

/// fn_800374E0(player): the barrier. Unless the pwp's +0x10 has bit 1:
/// fn_800BE408 starts it, the flag's low two bits become 1, and the
/// character's voice plays.
pub fn fn_800374e0(ram: &mut impl Ram, host: &mut impl ContactHost, player: i32) {
    barrier(ram, host, player, 1);
}

/// fn_800375FC(player): the magnetic barrier (bit 2, fn_800BE374).
pub fn fn_800375fc(ram: &mut impl Ram, host: &mut impl ContactHost, player: i32) {
    barrier(ram, host, player, 2);
}

/// fn_80037718's task: exec fn_8003779C, name "PlayerSpeedUp"
/// (.rodata 0x80174398).
pub const SPEED_UP_EXEC: u32 = 0x8003_779c;
pub const SPEED_UP_NAME: u32 = 0x8017_4398;

/// fn_80037718(player): the speed shoes. A number above 7, or 1 to 7 with
/// no twp, becomes 0 (one below 1 stays); CreateElementalTask(8, 2,
/// fn_8003779C, "PlayerSpeedUp") and the new task's work (+0x40) +0 gets
/// the player.
pub fn fn_80037718(ram: &mut impl Ram, host: &mut impl ContactHost, player: i32) {
    let n = if player < 1 {
        player
    } else if player > 7 || ram.r32(slot(PLAYER_TWP, player)) == 0 {
        0
    } else {
        player
    };
    let task = host.create_elemental_task(8, 2, SPEED_UP_EXEC, SPEED_UP_NAME);
    if task != 0 {
        let w = ram.r32(task + 0x40);
        ram.w8(w, n as u8);
    }
}

// --- vibration ---------------------------------------------------------------

/// fn_8002FCE0's task: exec fn_8002FDCC, name "VibTask" (.sdata2
/// 0x803AEE80), destructor fn_8002FE70.
pub const VIB_EXEC: u32 = 0x8002_fdcc;
pub const VIB_NAME: u32 = 0x803a_ee80;
pub const VIB_DEST: u32 = 0x8002_fe70;

/// fn_8002FB2C(player, a, b, c): a controller vibration request (the round-1
/// objects called it a player reaction; it is the rumble). Nothing while
/// GAME +0 is set, GAME +0x4A is 0 or GAME +0x14 is 1, nor for a player
/// other than 0 outside 2P except on stage 0x5A, nor without a controller
/// port. Then `a` is clamped to ±7 (±1 become ±2), `b` to 15..=59, `c` to
/// 0..=255; `c` 0 means mode 1 with `c = |a| - 1`, else mode 2; and
/// fn_8002FCE0 writes the request.
pub fn fn_8002fb2c(
    ram: &mut impl Ram,
    host: &mut impl ContactHost,
    player: i32,
    a: i32,
    b: i32,
    c: i32,
) {
    if ram.rs8(GAME) != 0 || ram.rs8(GAME + 0x4a) == 0 || ram.r8(GAME + 0x14) == 1 {
        return;
    }
    if ram.rs8(GAME + 0x1e) == 0 && player != 0 && get_stage_num(ram) != 0x5a {
        return;
    }
    let port = ram.r32(slot(VIB_PORT, player));
    let chan = if port == u32::MAX {
        u32::MAX
    } else {
        match VIB_CHANNELS.get(port as usize) {
            Some(&c) => c,
            // Past the four-entry table the code reads on into .rodata.
            None => ram.r32(VIB_CHANNEL.wrapping_add(port << 2)),
        }
    };
    if chan == u32::MAX {
        return;
    }
    let mut a = a.clamp(-7, 7);
    if a == 1 {
        a = 2;
    } else if a == -1 {
        a = -2;
    }
    let b = b.clamp(15, 0x3b);
    let mut c = c.clamp(0, 0xff);
    let mode = if c == 0 {
        // (float)|a| - 1.0 + (float)c, fctiwz: exact for |a| <= 7.
        c = a.abs() - 1;
        1
    } else {
        2
    };
    fn_8002fce0(ram, host, chan, mode, a, b, 0, c);
}

/// fn_8002FCE0(chan, mode, a, b, d, c): the channel's vibration task
/// (`VIB_TASKS[chan]`, created with an MAlloc'd 12-byte work when there is
/// none; DestroyTask when the MAlloc fails) gets work = (chan, mode, a, b
/// as bytes, d, c + 1), and its twp +4 is zeroed.
#[allow(clippy::too_many_arguments)]
pub fn fn_8002fce0(
    ram: &mut impl Ram,
    host: &mut impl ContactHost,
    chan: u32,
    mode: u32,
    a: i32,
    b: i32,
    d: u32,
    c: i32,
) {
    let s = VIB_TASKS.wrapping_add(chan << 2);
    let mut task = ram.r32(s);
    let w = if task == 0 {
        task = host.create_elemental_task(8, 0, VIB_EXEC, VIB_NAME);
        if task == 0 {
            return;
        }
        let w = host.malloc(0xc);
        if w == 0 {
            host.destroy_task(task);
            return;
        }
        ram.w32(s, task);
        ram.w32(task + 0x18, VIB_DEST);
        ram.w32(task + 0x3c, w);
        w
    } else {
        ram.r32(task + 0x3c)
    };
    ram.w8(w, chan as u8);
    ram.w8(w + 1, mode as u8);
    ram.w8(w + 2, a as u8);
    ram.w8(w + 3, b as u8);
    ram.w32(w + 4, d);
    ram.w32(w + 8, (c as u32).wrapping_add(1));
    let twp = ram.r32(task + 0x40);
    ram.w32(twp + 4, 0);
}

// --- defaults for the round-1 object hosts ---------------------------------

/// `containers::ContainerHost::hit_task(i)`: fn_80005904(task, i) +4 (the
/// container reads +4 of the record, also of a null one).
pub fn container_hit_task_default(ram: &mut impl Ram, task: u32, i: u32) -> u32 {
    let rec = fn_80005904(ram, task, i as i32);
    ram.r32(rec.wrapping_add(4))
}

/// `containers::ContainerHost::breaks(kind, hit)`: fn_800368A0 for
/// CONTIRON, fn_8003699C for CONTWOOD.
pub fn container_breaks_default(
    ram: &mut impl Ram,
    task: u32,
    kind: super::containers::Kind,
    hit: u32,
) -> bool {
    match kind {
        super::containers::Kind::Iron => fn_800368a0(ram, task, hit),
        super::containers::Kind::Wood => fn_8003699c(ram, task, hit),
    }
}

/// `containers::ContainerHost::hit_task_id(hit)`: the hit task's twp +2.
pub fn hit_task_id_default(ram: &mut impl Ram, hit: u32) -> u8 {
    let twp = ram.r32(hit + 0x34);
    ram.r8(twp + 2)
}

/// `containers::ContainerHost::player_mode(n)`: playertwp\[n\] +0.
pub fn player_mode_default(ram: &mut impl Ram, n: i32) -> Option<i8> {
    let twp = ram.r32(slot(PLAYER_TWP, n));
    (twp != 0).then(|| ram.r8(twp) as i8)
}

/// `containers::ContainerHost::player_kind(n)`: playerpwp\[n\] +1.
pub fn player_kind_default(ram: &mut impl Ram, n: i32) -> Option<i8> {
    let pwp = ram.r32(slot(PLAYER_PWP, n));
    (pwp != 0).then(|| ram.r8(pwp + 1) as i8)
}

/// `containers::ContainerHost::equipment()`: playerpwp\[0\] +0x24.
pub fn equipment_default(ram: &mut impl Ram) -> u32 {
    let pwp = ram.r32(PLAYER_PWP);
    ram.r32(pwp + 0x24)
}

/// `itembox::ItemBoxHost::player_flags(player)`: playerpwp\[player\] +0x10.
pub fn player_flags_default(ram: &mut impl Ram, player: i32) -> i16 {
    let pwp = ram.r32(slot(PLAYER_PWP, player));
    ram.r16(pwp + 0x10) as i16
}

/// `ring::RingHost::ccl_is_hit_player()`: CCL_IsHitPlayer(task) is
/// non-null. The host keeps the returned task (`last_hit`) for
/// `is_this_task_player()`, which is [`is_this_task_player`] of it; the
/// search is not run a second time (that would move its index on).
pub fn ring_hit_default(ram: &mut impl Ram, task: u32, last_hit: &mut u32) -> bool {
    *last_hit = ccl_is_hit_player(ram, task);
    *last_hit != 0
}

/// `hintbox::BoxHost::look_at(player)`: fn_8003A54C(player, task,
/// &(z, z, z), 1) with the stage's zero (stg42D section 4 +0x6EC) as `z`.
pub fn look_at_default(ram: &mut impl Ram, player: u32, task: u32, z: f32) {
    let w = z.to_bits();
    fn_8003a54c(ram, player as i32, task, Some([w, w, w]), 1);
}
