//! The breakable containers CONTIRON (object list row 9) and CONTWOOD
//! (row 8): a 20-unit crate that stacks, falls when the one below it goes,
//! and breaks when the player attacks it, scattering debris and flying
//! pieces and sometimes dropping an item.
//!
//! stg42D.rel section 1:
//!
//! | routine  | CONTIRON | CONTWOOD |
//! |----------|----------|----------|
//! | set-up   | 0xE888   | 0xF4A0   |
//! | delete   | 0xE970   | 0xF588   |
//! | exec     | 0xE9A8   | 0xF5C0   |
//! | display  | 0xF380 / 0xF42C | 0xFF28 (+ 0x10034 on stages 0x3C to 0x3E) |
//!
//! The display callbacks only draw (Ninja matrix and model calls) and are
//! not ported here; the set-up still reports which one it installs.
//!
//! **Set-up.** `CheckRangeOut(task)` (0x800655F4) first; when it reports the
//! task out of range nothing else happens. Otherwise it installs its
//! callbacks (CONTIRON's display is 0xF42C when the module's chunk-draw flag
//! at section 6 +0x670 is set, else 0xF380; CONTWOOD installs 0xFF28, and
//! when `_rename_GetStageNum()` (0x8001DD78) is 0x3C to 0x3E also 0x10034 at
//! task +0x20), sets the mode (twp +1) to 0, the ground height (task +0x40)
//! to -1000000 (section 4 +0x5F8 / +0x640), the fall (twp +0x24) and its
//! speed (+0x28) to 0 (+0x5FC / +0x644), and the item byte (twp +3) to the
//! low byte of Rotation.z (twp +0x10). It calls `CCL_InitShare(task, row, 1,
//! 4)` (0x8000F7D8) with the CollisionData row (section 5 +0xEF58 / +0xFF48:
//! one cube, centre (0, 10, 0), half-extents 10) and joins the stack
//! registry (0x5238).
//!
//! **Delete.** Leaves the registry (0x5488) and zeroes task +0x40.
//!
//! **Exec**, each frame:
//! * `CheckRangeOut(task)` again; out of range ends the frame.
//! * The stacked fall, as SOLIDBOX's: while there is fall left the crate
//!   moves by its speed (or the rest of the fall), then `speed = 0.99 *
//!   speed - 0.08` (+0x600/+0x604, +0x648/+0x64C;
//!   [`crate::solid_box::fall_step`]). A landing child (0x59E4 with the
//!   crate's own collision row) is created when there is none, and put at
//!   `(x, y - fall, z)`. With no fall left the child is freed (`FreeTaskC`,
//!   0x8007098C). When the fall is used up while the crate still has speed,
//!   both reset to 0 and, when the crate is within 5 of its ground height
//!   (fabs, section 1 0x84), dust is raised (0x1E8DC with 10, 10, the
//!   rotation, 8, 1.2, 0.5).
//! * Mode 0 (standing): the collision's hits (`fn_800067C0` count,
//!   `fn_80005904` record, last first) are asked whether they break the
//!   crate: `fn_800368A0` (CONTIRON) or `fn_8003699C` (CONTWOOD), the attack
//!   checks. On a break: a sound (`SE_CallV2_Timer`, 0x8006AFFC; 0x1011
//!   iron, 0x1010 wood), 14 debris particles (0x1EC18) 10 above the crate
//!   with sizes from `rand()` (0x8010DC88), the timer (twp +6) to 0, mode 1,
//!   `AddScore(20)` (0x8006378C), and, when the breaker is a player
//!   (`IsThisTaskPlayer`, 0x80037EDC) of the right kind, its velocity is
//!   zeroed (`SetVelocityP`, 0x8003A3DC). Otherwise the timer counts, the
//!   ground height is probed every 32nd frame while unknown
//!   (`GetShadowPos`, 0x80067AF0, 5 above), collision attribute 0x4000 is
//!   set while player 0's equipment (`playerpwp[0]` +0x24) has an attack
//!   upgrade bit and cleared otherwise, and `CCL_Entry` (0x8000F4EC) runs.
//! * Mode 1 (breaking): on timer 1 the stack above collapses (0x55F8 at the
//!   crate's position); after timer 2 the flying pieces start (0x782C with
//!   the chunk models, or 0x7804) and the mode becomes 2. With an upgrade
//!   the collision is only marked; otherwise `CCL_Entry` runs.
//! * Mode 2 (gone): for a SET-placed crate not yet marked broken
//!   (`_rename_CheckFlag0x20`, 0x80065404) with an item count (high nibble of
//!   twp +3), an item may drop (`fn_800260FC` 10 above, kind = low nibble
//!   mod 3): always for a count of 10 or more, else when `rand() / 32768 <
//!   (0.1 * count) * 0.1`; the crate is then marked (`_rename_SetFlag0x20`,
//!   0x80065414). It ends with `DeadOut` (0x800653B0) when SET-placed, else
//!   `FreeTask` (0x80071AE0).
//! * Any other mode does nothing more.
//!
//! The two execs are the same code apart from the order of the break
//! sound, the player kinds whose velocity is zeroed and the equipment bits.
//! Their constants sit at section 4 +0x5F8 (iron) and +0x640 (wood), one
//! copy each with the same values.
//!
//! **Shared code reused, not re-ported.** 0x5238, 0x5488, 0x55F8 and 0x59E4
//! are relocation-masked identical to stg09D.rel 0x8F04, 0x9154, 0x92C4 and
//! 0x96B0: the stack registry, its collapse and the landing child that
//! `crate::solid_box` covers (the collapse is
//! [`crate::solid_box::collapse_above`]); here they are host events. The
//! fall step is [`crate::solid_box::fall_step`].
//!
//! The REL's own effect helpers (dust 0x1E8DC, debris 0x1EC18, flying pieces
//! 0x7804/0x782C, which wrap 0x7850) and the player-side routines are host
//! events or queries named after their addresses.
use super::host::{ObjectHost, V3};
use crate::solid_box::fall_step;

/// The routines this file ports (stg42D.rel section 1). The set-up, delete
/// and exec routines call the module's own helpers with `bl` instructions
/// the REL does not relocate, whose displacements differ from stage to
/// stage, so each routine is listed in pieces split at those calls (a piece
/// ends before the `bl` and the next starts after it).
pub const ROUTINES: &[super::Routine] = &[
    super::Routine {
        name: "CONTIRON set-up 0xE888 [+0x0, +0xCC)",
        stg42d: 0xe888,
        len: 0xcc,
        masked_sha256: "f7c8dd25208ffc94b733b97fba2d546a1d552108213fe79f61c0f0a2839f32dd",
    },
    super::Routine {
        name: "CONTIRON set-up 0xE888 [+0xD0, +0xE8)",
        stg42d: 0xe958,
        len: 0x18,
        masked_sha256: "bea18f122a141c4684ba0f6b8a46a846af8a0cea12d136168453668e2bd438e5",
    },
    super::Routine {
        name: "CONTIRON delete 0xE970 [+0x0, +0x18)",
        stg42d: 0xe970,
        len: 0x18,
        masked_sha256: "420496a0a0e513d64a7cf147d45f56e9da49b85c26ce28ade6488ce53ef4c806",
    },
    super::Routine {
        name: "CONTIRON delete 0xE970 [+0x1C, +0x38)",
        stg42d: 0xe98c,
        len: 0x1c,
        masked_sha256: "6dad9cf5be27fc1d3328022070e72aa46f59a46c2de4314fff5a683cfb6a44a8",
    },
    super::Routine {
        name: "CONTIRON exec 0xE9A8 [+0x0, +0xCC)",
        stg42d: 0xe9a8,
        len: 0xcc,
        masked_sha256: "35a6dcdf8751e6951c16db643ab9b2440b94877aa0737e16810941a79ed02f74",
    },
    super::Routine {
        name: "CONTIRON exec 0xE9A8 [+0xD0, +0x158)",
        stg42d: 0xea78,
        len: 0x88,
        masked_sha256: "4da3ff324ffd75b0436b56759634fec70e2e49457d53c457070de39a9f0975bf",
    },
    super::Routine {
        name: "CONTIRON exec 0xE9A8 [+0x15C, +0x1AC)",
        stg42d: 0xeb04,
        len: 0x50,
        masked_sha256: "ae8bf02057958a1be3218c3e7e0b2bff45fac778fe2136a222b53b64f39f9f52",
    },
    super::Routine {
        name: "CONTIRON exec 0xE9A8 [+0x1B0, +0x380)",
        stg42d: 0xeb58,
        len: 0x1d0,
        masked_sha256: "3178cbbf733e537ffd544dc94ca91dd3ca9081d57dce1ace78ec26b61a8c7a34",
    },
    super::Routine {
        name: "CONTIRON exec 0xE9A8 [+0x384, +0x584)",
        stg42d: 0xed2c,
        len: 0x200,
        masked_sha256: "4e96da8d4ca7d635ca0e7a5819efdc509ec5270ad0f5636a2e3b28a3eda37ae1",
    },
    super::Routine {
        name: "CONTIRON exec 0xE9A8 [+0x588, +0x6E8)",
        stg42d: 0xef30,
        len: 0x160,
        masked_sha256: "b8685b18a0978fbcb551c3583ad40c3163f6b7dd1059f4bca11421ce24ed204c",
    },
    super::Routine {
        name: "CONTIRON exec 0xE9A8 [+0x6EC, +0x7F0)",
        stg42d: 0xf094,
        len: 0x104,
        masked_sha256: "1f39af90fd09cdddaa2c87f45ab181b7b45b81da091997c9303f65d129197960",
    },
    super::Routine {
        name: "CONTIRON exec 0xE9A8 [+0x7F4, +0x9D8)",
        stg42d: 0xf19c,
        len: 0x1e4,
        masked_sha256: "4b5c9c83ea073d3e48b8e4f0f528f6b20355552853e987d5240805b2af48f251",
    },
    super::Routine {
        name: "CONTWOOD set-up 0xF4A0 [+0x0, +0xCC)",
        stg42d: 0xf4a0,
        len: 0xcc,
        masked_sha256: "550c668e8c14a0cd0c3c92569b05032eed8a9291dad79cc45e4542e01c37a778",
    },
    super::Routine {
        name: "CONTWOOD set-up 0xF4A0 [+0xD0, +0xE8)",
        stg42d: 0xf570,
        len: 0x18,
        masked_sha256: "bea18f122a141c4684ba0f6b8a46a846af8a0cea12d136168453668e2bd438e5",
    },
    super::Routine {
        name: "CONTWOOD delete 0xF588 [+0x0, +0x18)",
        stg42d: 0xf588,
        len: 0x18,
        masked_sha256: "420496a0a0e513d64a7cf147d45f56e9da49b85c26ce28ade6488ce53ef4c806",
    },
    super::Routine {
        name: "CONTWOOD delete 0xF588 [+0x1C, +0x38)",
        stg42d: 0xf5a4,
        len: 0x1c,
        masked_sha256: "6dad9cf5be27fc1d3328022070e72aa46f59a46c2de4314fff5a683cfb6a44a8",
    },
    super::Routine {
        name: "CONTWOOD exec 0xF5C0 [+0x0, +0xCC)",
        stg42d: 0xf5c0,
        len: 0xcc,
        masked_sha256: "dbb6d7a4115769b34a278196bc0a20939d0e5cada5c5040b69cb04d506abc4ad",
    },
    super::Routine {
        name: "CONTWOOD exec 0xF5C0 [+0xD0, +0x158)",
        stg42d: 0xf690,
        len: 0x88,
        masked_sha256: "4da3ff324ffd75b0436b56759634fec70e2e49457d53c457070de39a9f0975bf",
    },
    super::Routine {
        name: "CONTWOOD exec 0xF5C0 [+0x15C, +0x1AC)",
        stg42d: 0xf71c,
        len: 0x50,
        masked_sha256: "ae8bf02057958a1be3218c3e7e0b2bff45fac778fe2136a222b53b64f39f9f52",
    },
    super::Routine {
        name: "CONTWOOD exec 0xF5C0 [+0x1B0, +0x364)",
        stg42d: 0xf770,
        len: 0x1b4,
        masked_sha256: "7a9f80c9e280949443849f7a6d6d0121fd51f64af5cdd59c0a11723e18870eb2",
    },
    super::Routine {
        name: "CONTWOOD exec 0xF5C0 [+0x368, +0x534)",
        stg42d: 0xf928,
        len: 0x1cc,
        masked_sha256: "36e1207dde3dac1375103a1ec62cff3dbc4e5b002e3549003aa0bcf2c3e68540",
    },
    super::Routine {
        name: "CONTWOOD exec 0xF5C0 [+0x538, +0x698)",
        stg42d: 0xfaf8,
        len: 0x160,
        masked_sha256: "b8685b18a0978fbcb551c3583ad40c3163f6b7dd1059f4bca11421ce24ed204c",
    },
    super::Routine {
        name: "CONTWOOD exec 0xF5C0 [+0x69C, +0x7A0)",
        stg42d: 0xfc5c,
        len: 0x104,
        masked_sha256: "1f39af90fd09cdddaa2c87f45ab181b7b45b81da091997c9303f65d129197960",
    },
    super::Routine {
        name: "CONTWOOD exec 0xF5C0 [+0x7A4, +0x968)",
        stg42d: 0xfd64,
        len: 0x1c4,
        masked_sha256: "8ae33de2e0db023cda863b773177af8e700763a0bbf0905b824fd67558ceb4fd",
    },
];

/// Which container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Iron,
    Wood,
}

/// -1000000: the ground height before it is probed (section 4 +0x5F8 /
/// +0x640).
pub const NO_GROUND: f32 = -1_000_000.0;
/// Dust when landing within this of the ground; the probe's lift
/// (+0x608 / +0x650).
const NEAR: f32 = 5.0;
/// Ten above the crate for debris, pieces and items, and the pieces'
/// spread (+0x60C / +0x654).
const LIFT: f32 = 10.0;
/// Dust arguments (+0x60C, +0x60C, +0x610, +0x614, +0x618 / +0x654, +0x654,
/// +0x658, +0x65C, +0x660).
const DUST: [f32; 5] = [10.0, 10.0, 8.0, f32::from_bits(0x3f99_999a), 0.5];
/// 1 / 32768, `rand()`'s scale (+0x624 / +0x66C).
const RAND_SCALE: f32 = f32::from_bits(0x3800_0000);
/// Debris sizes `7 + 3r`, `0.5 + r`, `2.5 + 4r` (+0x61C, +0x620, +0x618,
/// +0x628, +0x62C / +0x664, +0x668, +0x660, +0x670, +0x674).
const DEBRIS: [f32; 5] = [7.0, 3.0, 0.5, 2.5, 4.0];
/// Flying piece speed `0.7 + 1.2r` (+0x630, +0x614 / +0x678, +0x65C).
const PIECE: [f32; 2] = [f32::from_bits(0x3f33_3333), f32::from_bits(0x3f99_999a)];
/// Item chance factor (+0x634 / +0x67C).
const ITEM_FACTOR: f32 = f32::from_bits(0x3dcc_cccd);
/// How many debris particles a break throws.
pub const DEBRIS_COUNT: usize = 14;
/// Collision info attribute bit set while player 0 has an attack upgrade.
pub const ATTR_UPGRADE: u32 = 0x4000;

/// Where a stage REL keeps the container's data (section-5 offsets; each
/// stage has its own copy at its own place).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    /// The CollisionData row.
    pub rows: u32,
    /// The three flying-piece groups: (+0 list, +4 model, +8 count).
    pub groups: [(u32, u32, u32); 3],
    /// Each group's chunk-model slot (the next draw entry's +0xC), passed
    /// when the chunk-draw flag is set.
    pub slots: [u32; 3],
}

/// stg42D CONTIRON: rows +0xEF58, groups at +0xEEF4, slots at +0xEF34.
pub const STG42D_IRON: Layout = Layout {
    rows: 0xef58,
    groups: [
        (0xe570, 0xe830, 16),
        (0xe860, 0xeba8, 4),
        (0xebcc, 0xeec8, 4),
    ],
    slots: [0xef34, 0xef44, 0xef54],
};
/// stg42D CONTWOOD: rows +0xFF48, groups at +0xFEE4, slots at +0xFF24.
pub const STG42D_WOOD: Layout = Layout {
    rows: 0xff48,
    groups: [
        (0xf3f0, 0xf824, 16),
        (0xf848, 0xfae4, 4),
        (0xfb08, 0xfeb8, 4),
    ],
    slots: [0xff24, 0xff34, 0xff44],
};

/// The container's task state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContainerTask {
    /// twp +1 (signed byte).
    pub smode: i8,
    /// twp +3: low nibble the item kind, high nibble the item count.
    pub btimer: u8,
    /// twp +6.
    pub wtimer: u16,
    /// twp +8..+0x10 (Rotation; x bit 0 gives the pieces no floor).
    pub ang: [i32; 3],
    /// twp +0x14.
    pub pos: V3,
    /// twp +0x24: the fall left.
    pub fall: f32,
    /// twp +0x28.
    pub speed: f32,
    /// task +0x40: the probed ground height.
    pub ground: f32,
    /// The collision info's attribute word (`cwp->info +4`).
    pub attr: u32,
    /// The landing child (task +0xC) and its position.
    pub child: Option<V3>,
    /// task +0x30 is set: placed from the SET file.
    pub set_placed: bool,
}

/// The callbacks the set-up installs (stg42D section-1 offsets).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Callbacks {
    pub exec: u32,
    pub disp: u32,
    pub dest: u32,
    /// CONTWOOD's task +0x20 on stages 0x3C to 0x3E (left as it was
    /// otherwise).
    pub sort_disp: Option<u32>,
}

/// A call the container makes, in order.
#[derive(Debug, Clone, PartialEq)]
pub enum ContainerEvent {
    /// CCL_InitShare(task, rows, count, list) (0x8000F7D8).
    InitShare { rows: u32, count: u32, list: u32 },
    /// 0x5238 (stg09D 0x8F04): join the stack registry.
    StackAdd,
    /// 0x5488 (stg09D 0x9154): leave it.
    StackRemove,
    /// FreeTaskC(task) (0x8007098C): the landing child goes.
    FreeChild,
    /// 0x1E8DC(&pos, s0, s1, &ang, s2, s3, s4): landing dust.
    Dust {
        pos: V3,
        ang: [i32; 3],
        sizes: [f32; 5],
    },
    /// SE_CallV2_Timer(id, twp, 1, 0x7F, 0x50, &pos) (0x8006AFFC).
    Sound { id: u32, pos: V3 },
    /// 0x1EC18(&at, s0, s1, s2): one debris particle.
    Debris { at: V3, sizes: [f32; 3] },
    /// AddScore(n) (0x8006378C).
    Score(u32),
    /// SetVelocityP(player, 0, 0, 0) (0x8003A3DC).
    StopPlayer(i32),
    /// CCL_Entry(task) (0x8000F4EC).
    CollisionEntry,
    /// 0x55F8(&pos) (stg09D 0x92C4): the stack above collapses.
    Collapse(V3),
    /// One flying piece: 0x782C(&at, 1, &spread, ang.y, model, *slot, list,
    /// speed, floor) when `chunk` is set, else 0x7804(&at, 1, &spread,
    /// ang.y, model, list, speed, floor). `slot` is the section-5 offset of
    /// the chunk-model pointer passed (its value is runtime state).
    Piece {
        chunk: bool,
        at: V3,
        spread: V3,
        ang_y: i32,
        model: u32,
        slot: Option<u32>,
        list: u32,
        speed: f32,
        floor: f32,
    },
    /// fn_800260FC(x, y, z, kind): an item drops.
    Item { at: V3, kind: i32 },
    /// _rename_SetFlag0x20(task) (0x80065414).
    MarkBroken,
    /// DeadOut(task) (0x800653B0).
    DeadOut,
    /// FreeTask(task) (0x80071AE0).
    FreeTask,
}

/// What the containers ask of the game.
pub trait ContainerHost: ObjectHost {
    /// _rename_GetStageNum() (0x8001DD78).
    fn stage_num(&mut self) -> i32;
    /// The module's chunk-draw flag (section 6 +0x670 iron, +0x678 wood).
    fn chunk_draw(&mut self, kind: Kind) -> bool;
    /// 0x59E4(task, 0, 0, rows, 1, 0): create the landing child; true when
    /// the task now has one.
    fn create_landing_child(&mut self, rows: u32) -> bool;
    /// fn_800067C0(task): how many collision hits there are.
    fn hit_count(&mut self) -> u32;
    /// fn_80005904(task, i) +4: the task behind hit `i`.
    fn hit_task(&mut self, i: u32) -> u32;
    /// fn_800368A0(task, hit) (CONTIRON) or fn_8003699C(task, hit)
    /// (CONTWOOD): does this hit break the crate.
    fn breaks(&mut self, kind: Kind, hit: u32) -> bool;
    /// The hit task's twp +2 (1 for a player).
    fn hit_task_id(&mut self, hit: u32) -> u8;
    /// IsThisTaskPlayer(task) (0x80037EDC): the player number or -1.
    fn is_this_task_player(&mut self, hit: u32) -> i32;
    /// playertwp\[n\] +0 (signed), `None` when there is no task.
    fn player_mode(&mut self, n: i32) -> Option<i8>;
    /// playerpwp\[n\] +1 (signed), `None` when there is no work.
    fn player_kind(&mut self, n: i32) -> Option<i8>;
    /// playerpwp\[0\] +0x24 (equipment).
    fn equipment(&mut self) -> u32;
    /// rand() (0x8010DC88).
    fn rand(&mut self) -> i32;
    /// GetShadowPos(x, y, z, &ang) (0x80067AF0).
    fn shadow_pos(&mut self, at: V3, ang: [i32; 3]) -> f32;
    /// _rename_CheckFlag0x20(task) (0x80065404).
    fn broken_flag(&mut self) -> bool;
    /// A call the container makes.
    fn container_event(&mut self, e: ContainerEvent);
}

/// The set-up (0xE888 / 0xF4A0). `None` when out of range (nothing set).
pub fn setup(
    kind: Kind,
    layout: &Layout,
    t: &mut ContainerTask,
    host: &mut impl ContainerHost,
) -> Option<Callbacks> {
    if host.range_out() {
        return None;
    }
    let cb = match kind {
        Kind::Iron => Callbacks {
            exec: 0xe9a8,
            disp: if host.chunk_draw(kind) {
                0xf42c
            } else {
                0xf380
            },
            dest: 0xe970,
            sort_disp: None,
        },
        Kind::Wood => {
            let stage = host.stage_num();
            Callbacks {
                exec: 0xf5c0,
                disp: 0xff28,
                dest: 0xf588,
                sort_disp: (0x3c..0x3f).contains(&stage).then_some(0x10034),
            }
        }
    };
    t.smode = 0;
    t.ground = NO_GROUND;
    t.btimer = t.ang[2] as u8;
    t.fall = 0.0;
    t.speed = 0.0;
    host.container_event(ContainerEvent::InitShare {
        rows: layout.rows,
        count: 1,
        list: 4,
    });
    host.container_event(ContainerEvent::StackAdd);
    Some(cb)
}

/// The delete callback (0xE970 / 0xF588).
pub fn dest(t: &mut ContainerTask, host: &mut impl ContainerHost) {
    host.container_event(ContainerEvent::StackRemove);
    t.ground = 0.0;
}

fn lifted(p: V3) -> V3 {
    [p[0], p[1] + LIFT, p[2]]
}

fn rand_unit(host: &mut impl ContainerHost) -> f32 {
    RAND_SCALE * host.rand() as f32
}

/// Whether player 0's equipment marks the collision (0xEE88 / 0xFA70).
fn upgraded(kind: Kind, eq: u32) -> bool {
    match kind {
        Kind::Iron => eq & 0x0800_1000 != 0 || (eq & 0x10 != 0 && eq & 0x0004_0008 != 0),
        Kind::Wood => eq & 0x0200_0410 != 0,
    }
}

/// Zero the breaker's velocity, for some player kinds only.
fn stop_breaker(kind: Kind, hit: u32, host: &mut impl ContainerHost) {
    if host.hit_task_id(hit) != 1 {
        return;
    }
    let n = host.is_this_task_player(hit);
    if n == -1 {
        return;
    }
    let stop = match kind {
        // 0xED50: both pointers checked; kind 0 needs mode 0x45, kinds 4
        // and 5 need mode 0x4E.
        Kind::Iron => {
            let (Some(mode), Some(k)) = (host.player_mode(n), host.player_kind(n)) else {
                return;
            };
            !(k == 0 && mode != 0x45 || (k == 4 || k == 5) && mode != 0x4e)
        }
        // 0xF960: read without pointer checks (a player's task and work
        // exist); kinds 0, 4 and 5 and modes 0x45 and 0x4E are left alone.
        Kind::Wood => {
            let (Some(k), Some(mode)) = (host.player_kind(n), host.player_mode(n)) else {
                return;
            };
            k != 0 && mode != 0x45 && !(4..=5).contains(&(k as u8)) && mode != 0x4e
        }
    };
    if stop {
        host.container_event(ContainerEvent::StopPlayer(n));
    }
}

/// One exec frame (0xE9A8 / 0xF5C0).
pub fn exec(kind: Kind, layout: &Layout, t: &mut ContainerTask, host: &mut impl ContainerHost) {
    if host.range_out() {
        return;
    }
    if t.fall > 0.0 {
        fall_step(&mut t.pos[1], &mut t.fall, &mut t.speed);
        if t.child.is_none() && host.create_landing_child(layout.rows) {
            t.child = Some([0.0; 3]);
        }
        if let Some(c) = &mut t.child {
            *c = [t.pos[0], t.pos[1] - t.fall, t.pos[2]];
        }
    } else if t.child.is_some() {
        host.container_event(ContainerEvent::FreeChild);
        t.child = None;
    }
    if t.fall <= 0.0 && t.speed != 0.0 {
        t.fall = 0.0;
        t.speed = 0.0;
        if (t.pos[1] - t.ground).abs() < NEAR {
            host.container_event(ContainerEvent::Dust {
                pos: t.pos,
                ang: t.ang,
                sizes: DUST,
            });
        }
    }
    match t.smode {
        0 => standing(kind, t, host),
        1 => breaking(kind, layout, t, host),
        2 => gone(t, host),
        _ => {}
    }
}

fn standing(kind: Kind, t: &mut ContainerTask, host: &mut impl ContainerHost) {
    let mut i = host.hit_count();
    let mut breaker = None;
    while i != 0 {
        i -= 1;
        let hit = host.hit_task(i);
        if host.breaks(kind, hit) {
            breaker = Some(hit);
            break;
        }
    }
    let Some(hit) = breaker else {
        t.wtimer = t.wtimer.wrapping_add(1);
        if t.ground == NO_GROUND && t.wtimer & 0x1f == 0 {
            let [x, y, z] = t.pos;
            t.ground = host.shadow_pos([x, NEAR + y, z], [0, t.ang[1], 0]);
        }
        mark(kind, t, host);
        host.container_event(ContainerEvent::CollisionEntry);
        return;
    };
    let sound = ContainerEvent::Sound {
        id: match kind {
            Kind::Iron => 0x1011,
            Kind::Wood => 0x1010,
        },
        pos: t.pos,
    };
    if kind == Kind::Iron {
        host.container_event(sound.clone());
    }
    let at = lifted(t.pos);
    for _ in 0..DEBRIS_COUNT {
        let c = DEBRIS[3] + DEBRIS[4] * rand_unit(host);
        let b = DEBRIS[2] + rand_unit(host);
        let a = DEBRIS[0] + DEBRIS[1] * rand_unit(host);
        host.container_event(ContainerEvent::Debris {
            at,
            sizes: [a, b, c],
        });
    }
    t.wtimer = 0;
    t.smode = 1;
    match kind {
        Kind::Iron => {
            host.container_event(ContainerEvent::Score(20));
            stop_breaker(kind, hit, host);
        }
        Kind::Wood => {
            host.container_event(sound);
            stop_breaker(kind, hit, host);
            host.container_event(ContainerEvent::Score(20));
        }
    }
}

/// The collision attribute: marked with an upgrade, cleared without.
fn mark(kind: Kind, t: &mut ContainerTask, host: &mut impl ContainerHost) -> bool {
    let up = upgraded(kind, host.equipment());
    if up {
        t.attr |= ATTR_UPGRADE;
    } else {
        t.attr &= !ATTR_UPGRADE;
    }
    up
}

fn breaking(kind: Kind, layout: &Layout, t: &mut ContainerTask, host: &mut impl ContainerHost) {
    if t.wtimer == 1 {
        host.container_event(ContainerEvent::Collapse(t.pos));
    }
    let before = t.wtimer;
    t.wtimer = before.wrapping_add(1);
    if before > 2 {
        let at = lifted(t.pos);
        let chunk = host.chunk_draw(kind);
        for (g, &(list, model, count)) in layout.groups.iter().enumerate() {
            for _ in 0..count {
                let floor = if t.ang[0] & 1 != 0 {
                    NO_GROUND
                } else if t.ground == NO_GROUND {
                    t.pos[1]
                } else {
                    t.ground
                };
                let speed = PIECE[0] + PIECE[1] * rand_unit(host);
                host.container_event(ContainerEvent::Piece {
                    chunk,
                    at,
                    spread: [LIFT; 3],
                    ang_y: t.ang[1],
                    model,
                    slot: chunk.then_some(layout.slots[g]),
                    list,
                    speed,
                    floor,
                });
            }
        }
        t.smode = 2;
    }
    if !mark(kind, t, host) {
        host.container_event(ContainerEvent::CollisionEntry);
    }
}

fn gone(t: &mut ContainerTask, host: &mut impl ContainerHost) {
    let count = i32::from(t.btimer >> 4);
    if count != 0 && t.set_placed && !host.broken_flag() {
        let chance = ITEM_FACTOR * count as f32 * ITEM_FACTOR;
        if rand_unit(host) < chance || count >= 10 {
            host.container_event(ContainerEvent::Item {
                at: lifted(t.pos),
                kind: i32::from(t.btimer & 0xf) % 3,
            });
        }
        host.container_event(ContainerEvent::MarkBroken);
    }
    host.container_event(if t.set_placed {
        ContainerEvent::DeadOut
    } else {
        ContainerEvent::FreeTask
    });
}
