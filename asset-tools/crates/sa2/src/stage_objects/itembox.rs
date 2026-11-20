//! The item boxes ITEMBOX and ITEMBOXAIR (object list rows 5 and 6), the
//! item box HUD queue `itemBoxManager` ([`queue`]) and the bomb item's
//! explosion manager `ExpManExec` ([`explosion`]).
//!
//! stg42D.rel section 1, ITEMBOX (exec 0x12028):
//!
//! * the range check comes first, and only for a box that is not a child
//!   (twp +1 == 0): `CheckRangeOut(task)` (0x800655F4);
//! * the display callbacks follow the mode: modes 0 to 2 show the whole box
//!   (task +0x14 = 0x12690, +0x20 = 0x12750), modes 3 and 4 the broken
//!   shell (+0x14 = 0x1289C, +0x20 = 0);
//! * mode 0, 0x12164: the item index (task +0x3C) is `fctiwz(Scale.x)`
//!   clamped to 0..=10; a box whose SET entry (or, for a child, its
//!   parent's) carries flag 0x20 and is not broken turns a 1-up (item kind
//!   10) into the first empty entry (kind 7); a box whose entry is already
//!   broken starts as the shell (mode 4, broken collision rows), any other
//!   one gets the whole box's rows; then fn_80018D28(task) and the
//!   callbacks (dest 0x12904);
//! * mode 1, 0x12364: the box grows by 0.11 a frame to 1.0 and spins 5.0
//!   a frame (or jumps to full size when lbl_801CC168 +0x37 is set), then
//!   mode 2;
//! * mode 2, 0x123FC: waits for a player's touch (CCL_IsHitPlayer, then
//!   fn_80006574, IsThisTaskPlayer; or the work's +0x44 player when twp +4
//!   has bit 4). Unless that player's pwp +0x10 has bit 0x4000, the HUD
//!   queue gets the item ([`queue::item_callback`], 0x9FB8), the item's
//!   effect runs (the item table's function) and fn_800168C8(player) is
//!   called. Then the box breaks: mode 3, the break effect (stg42D 0xCCFC),
//!   SE 0x800D, fn_8002FB2C(player, 1, 0x30, 0) and SetBroken on its SET
//!   entry. CCL_Entry and the spin follow every frame of mode 2;
//! * mode 3, 0x12634: fn_800067F8(task), the shell's collision rows, mode 4;
//! * modes 4 and 5: CCL_Entry; mode 6: the dest 0x12904 (fn_80018B88, item
//!   index 0) and mode 0; any other mode becomes 0.
//!
//! ITEMBOXAIR (exec 0x12FD0) is the same machine with its own copies
//! (0x130A4, 0x132E4, 0x1337C, 0x13590, 0x135A0, 0x1397C and the item
//! functions 0x139B4..0x13C40, item table s5 +0x128D8, jump table s5
//! +0x12930): the item index is twp +3 (an unsigned byte), the collision is
//! a single row (CCL_Init) whose centre is the box's position plus
//! (0, 14, 0) turned by its Y and Z angles, a broken entry removes the task
//! at once (DeadOut), a broken box grows by 0.2 a frame to 5.0 and is then
//! removed, and the 1-up's flag goes to the parent or the task without
//! checking their SET entries.
//!
//! The constants are stg42D section 4 +0x710..+0x780; the collision rows,
//! the item tables and the models are section 5 data every stage carries at
//! its own offsets ([`StageData`]). What the box does to the player (score,
//! rings, mech HP, 1-up, the player item routines) and every engine routine
//! are [`ItemBoxHost`] calls named after their main.dol routine; rendering
//! is returned as [`Draw`] commands, with the geometry and timing the
//! display callbacks compute.
pub mod explosion;
pub mod queue;

use super::host::{Event, ObjectHost, V3};
use crate::ninja_matrix::{Axis, MatrixStack, IDENTITY};
use crate::ninja_trig::fctiwz;

/// The routines this file and its children port (stg42D.rel section 1).
///
/// Five routines call other routines of the stage REL (0x9FB8, 0xCCFC,
/// 0x9C08, 0x7684) with `bl` instructions the REL doesn't relocate, so
/// their displacement follows each REL's layout. Those routines are listed
/// in pieces that leave out just those 4-byte branches; every piece is the
/// same masked code in 64 other stage RELs.
pub const ROUTINES: &[super::Routine] = &[
    r(
        "ITEMBOX exec",
        0x12028,
        0x13c,
        "d875337a8d45e3c2ccc46a5ae9a46c08668761af044c4ef014549f48b027c02e",
    ),
    r(
        "ITEMBOX init",
        0x12164,
        0x200,
        "7a15b3e684e484e0e8b943be392e32f4ff438f4b1669966f0fb7e43cc004c930",
    ),
    r(
        "ITEMBOX grow",
        0x12364,
        0x98,
        "8afd654d6ffeeaf1a180f9411e6677dcce45c9cdf6746541f33d44bb4ae3f102",
    ),
    r(
        "ITEMBOX wait, to the queue call",
        0x123fc,
        0xd4,
        "12c553ccf34cabe4f6aa5e0ecc72e41a1e8a5d41fba09a1b88b0044ba9971fe6",
    ),
    r(
        "ITEMBOX wait, to the break effect",
        0x124d4,
        0x9c,
        "00c7f82767b22943a8197f229a885d896dd184adbdcae1323f90f69d6e5a55f6",
    ),
    r(
        "ITEMBOX wait, the rest",
        0x12574,
        0xc0,
        "b35f0149fd71761eb4c3048162fb0a74e03cf46121d719be74e72da9d23eb6fa",
    ),
    r(
        "ITEMBOX break",
        0x12634,
        0x5c,
        "a418a5f1e98378b0227a93db9c1a55de77ffa9318852f3679ba3ffdc70ca5e56",
    ),
    r(
        "ITEMBOX display",
        0x12690,
        0xc0,
        "c296c7fa9c610ce6cbb878753bb17819791fa337ef0e14f979851af42afba272",
    ),
    r(
        "ITEMBOX display icon, to the icon",
        0x12750,
        0xf8,
        "db0c09cb68500362d51c2781a46e97433983c10a39894eac10f1ac3f43572845",
    ),
    r(
        "ITEMBOX display icon, the rest",
        0x1284c,
        0x50,
        "bc688f9365e5e8bea8796246b9b7a1ed92f3ee0a20ed20cb0aaa6b54378183d3",
    ),
    r(
        "ITEMBOX display shell",
        0x1289c,
        0x68,
        "8ee484aee009db2f9b7987702660f64898f93cd13bac860fbb63a2b91709059a",
    ),
    r(
        "ITEMBOX dest",
        0x12904,
        0x38,
        "5c80053ca87f0e47b8ea6fd54a6b6e4d1a254de78c7a33cd144865e8c4cd9c2f",
    ),
    r(
        "ITEMBOX item rings 10",
        0x1293c,
        0x50,
        "f53df58b238c1167ee777db2305099718d21dd99a4d166a4d27c05cc9c894ea6",
    ),
    r(
        "ITEMBOX item 1-up",
        0x1298c,
        0x88,
        "1a85a3ad6c6c28538cf91b64167355286536de96979d4fa2d59318120613ecf5",
    ),
    r(
        "ITEMBOX item rings 5",
        0x12a14,
        0x50,
        "b266028b9298966c0ba1309138146ee3d5b623e2e9f9ae6f3f7d49d7619bf212",
    ),
    r(
        "ITEMBOX item 0x80037718",
        0x12a64,
        0x38,
        "53c1f36f61b5d441d35a95faafcd2ce947ce7893919797e4c31c0e32ce841d0d",
    ),
    r(
        "ITEMBOX item rings 20",
        0x12a9c,
        0x50,
        "4771bf447644c14c985a4fe40bd2156287f65c6461469901a7afc9dff1f325ef",
    ),
    r(
        "ITEMBOX item 0x800374E0",
        0x12aec,
        0x38,
        "53c1f36f61b5d441d35a95faafcd2ce947ce7893919797e4c31c0e32ce841d0d",
    ),
    r(
        "ITEMBOX item bomb",
        0x12b24,
        0x3c,
        "c2ac61e5285c392ded924cbe17010acafdd3475c07ad222b32d520bf91acf735",
    ),
    r(
        "ITEMBOX item mech HP",
        0x12b60,
        0x44,
        "8aeef4e5569849abe35fb5c85d4260f4d6f5e1e578e8048e8d0407db698a0c43",
    ),
    r(
        "ITEMBOX item 0x800375FC",
        0x12ba4,
        0x38,
        "53c1f36f61b5d441d35a95faafcd2ce947ce7893919797e4c31c0e32ce841d0d",
    ),
    r(
        "ITEMBOX item empty",
        0x12bdc,
        0x4,
        "f332ea5b5437103cbb6f1508679da89eec9288ad775c96c439a17fccabe3de8e",
    ),
    r(
        "ITEMBOX item 0x800373B8",
        0x12be0,
        0x38,
        "53c1f36f61b5d441d35a95faafcd2ce947ce7893919797e4c31c0e32ce841d0d",
    ),
    r(
        "ExpMan spawn",
        0x12c18,
        0x138,
        "1fb8bc56d47299463c9297169a3dcaa8250ccfc2e07089a51330bdd118a5b2a3",
    ),
    r(
        "ExpMan exec",
        0x12d50,
        0xd0,
        "a4053ff1ce6b47e448334b6d2f61c2d78dc8fb6c9a1082bff876af81061fd580",
    ),
    r(
        "ExpMan display, to the sphere",
        0x12e20,
        0x60,
        "8ec28fe17f6e5aa8dd4988bb4b4f1a31e709e2d6bcb0d23328e04c686c1c2c8a",
    ),
    r(
        "ExpMan display, the rest",
        0x12e84,
        0x2c,
        "4032659430fdf9335b77fd4f4e51abb932b408b899fb41ab1361efa8bc1290db",
    ),
    r(
        "ExpMan dest",
        0x12eb0,
        0x94,
        "49e36dbc733c39b923fb3a4f3c781232e219b44e439daa1a45d149f66f03f76a",
    ),
    r(
        "ExpMan hit test",
        0x12f44,
        0x8c,
        "f5f9cd8d2ee5822a0cda8f90c980b8e2eb7eb917852b4c32722d9a8875f89585",
    ),
    r(
        "ITEMBOXAIR exec",
        0x12fd0,
        0xd4,
        "b9e186bdcdc1b6c4d03de55dc9efd2314d42d624677f037077914a88f7858c92",
    ),
    r(
        "ITEMBOXAIR init",
        0x130a4,
        0x240,
        "685385dbec19179f477ce8d9a44471649a12eebc031a5f5a8ffabdb89297378e",
    ),
    r(
        "ITEMBOXAIR grow",
        0x132e4,
        0x98,
        "8afd654d6ffeeaf1a180f9411e6677dcce45c9cdf6746541f33d44bb4ae3f102",
    ),
    r(
        "ITEMBOXAIR wait, to the queue call",
        0x1337c,
        0xd4,
        "80eb8ef7f2fea82a51c2aa91842fcf5b1b3ef4aea24fad62190a6e3e422cb963",
    ),
    r(
        "ITEMBOXAIR wait, to the break effect",
        0x13454,
        0x6c,
        "7805bb03262240dc775f90b0262c560b486f899511a6a1455488635058459a81",
    ),
    r(
        "ITEMBOXAIR wait, the rest",
        0x134c4,
        0xcc,
        "b9fccb9ef9c39ca917aeb09630eac32f3f9acde1aca3807d96c7dc9f489e37ef",
    ),
    r(
        "ITEMBOXAIR break",
        0x13590,
        0x10,
        "ebfb7bab0da52e5dda1666b343f6066dcaeeb1304a2933b04abbd4c8ed79e8cf",
    ),
    r(
        "ITEMBOXAIR vanish",
        0x135a0,
        0xa0,
        "89707f0eecc7a5484e6a79473be6543956cb653016c96cab5d7dac859a26b56b",
    ),
    r(
        "ITEMBOXAIR display",
        0x13640,
        0xd0,
        "021e77155fdab548f59962724fdbcd6c7b43b75ab5f8b4219c581b5d08ce490a",
    ),
    r(
        "ITEMBOXAIR display icon, to the icon",
        0x13710,
        0x198,
        "2a6c0be33193cd46ec2c84595887ee71bc57c8108656aaa9b98c93bd3b324858",
    ),
    r(
        "ITEMBOXAIR display icon, the rest",
        0x138ac,
        0xd0,
        "6b3ca4eec750611847879e64382b36ae1a1aeb76a8ff4f397201a29d961b3739",
    ),
    r(
        "ITEMBOXAIR dest",
        0x1397c,
        0x38,
        "5c80053ca87f0e47b8ea6fd54a6b6e4d1a254de78c7a33cd144865e8c4cd9c2f",
    ),
    r(
        "ITEMBOXAIR item rings 10",
        0x139b4,
        0x50,
        "f53df58b238c1167ee777db2305099718d21dd99a4d166a4d27c05cc9c894ea6",
    ),
    r(
        "ITEMBOXAIR item 1-up",
        0x13a04,
        0x70,
        "19366c04b55753a9733f997ebae2651335cb188296258daa8f23cf8f15f20480",
    ),
    r(
        "ITEMBOXAIR item rings 5",
        0x13a74,
        0x50,
        "b266028b9298966c0ba1309138146ee3d5b623e2e9f9ae6f3f7d49d7619bf212",
    ),
    r(
        "ITEMBOXAIR item 0x80037718",
        0x13ac4,
        0x38,
        "53c1f36f61b5d441d35a95faafcd2ce947ce7893919797e4c31c0e32ce841d0d",
    ),
    r(
        "ITEMBOXAIR item rings 20",
        0x13afc,
        0x50,
        "4771bf447644c14c985a4fe40bd2156287f65c6461469901a7afc9dff1f325ef",
    ),
    r(
        "ITEMBOXAIR item 0x800374E0",
        0x13b4c,
        0x38,
        "53c1f36f61b5d441d35a95faafcd2ce947ce7893919797e4c31c0e32ce841d0d",
    ),
    r(
        "ITEMBOXAIR item bomb",
        0x13b84,
        0x3c,
        "4a912165a66c6693ef6a1011331dbaaef20e3861f0f96e70a16c1fa9e9dcdfd3",
    ),
    r(
        "ITEMBOXAIR item mech HP",
        0x13bc0,
        0x44,
        "8aeef4e5569849abe35fb5c85d4260f4d6f5e1e578e8048e8d0407db698a0c43",
    ),
    r(
        "ITEMBOXAIR item 0x800375FC",
        0x13c04,
        0x38,
        "53c1f36f61b5d441d35a95faafcd2ce947ce7893919797e4c31c0e32ce841d0d",
    ),
    r(
        "ITEMBOXAIR item empty",
        0x13c3c,
        0x4,
        "f332ea5b5437103cbb6f1508679da89eec9288ad775c96c439a17fccabe3de8e",
    ),
    r(
        "ITEMBOXAIR item 0x800373B8",
        0x13c40,
        0x38,
        "53c1f36f61b5d441d35a95faafcd2ce947ce7893919797e4c31c0e32ce841d0d",
    ),
    r(
        "itemBoxManager spawn",
        0x9ea0,
        0xa8,
        "b1b073f1a75665fd85816e46b0523d79ae484ed87f2b80bdc78269246cfee786",
    ),
    r(
        "itemBoxManager dest",
        0x9f48,
        0x70,
        "2e5db24660d38f0254bd25273c191ad212fbb13dd27f4d314cc95b79d48feab5",
    ),
    r(
        "itemBoxManager item callback",
        0x9fb8,
        0x114,
        "28a23bb1876f07cf00aa6736c694481af7d006f1907e7aa16ed77a2afd85b87d",
    ),
    r(
        "itemBoxManager exec",
        0xa0cc,
        0x6c,
        "698f21110008915cc5b286c532a8d28f4b2842363ffc9e480265b57fb1bd3d87",
    ),
    r(
        "itemBoxManager slide in",
        0xa138,
        0x180,
        "718beaf047f6754efff2526991137d1cd4a51ee1f95189bfa641f503f2368506",
    ),
    r(
        "itemBoxManager hold",
        0xa2b8,
        0xa4,
        "eec8dbd37a04de942f4b1616e606c2c900f844fc399a04f8c27b0b5259af1750",
    ),
    r(
        "itemBoxManager shrink",
        0xa35c,
        0xb0,
        "51381a7e1d7dd25a9bf55a3a91e37c66629d75a703c784eddd8f05b7d3747853",
    ),
    r(
        "itemBoxManager display",
        0xa40c,
        0x270,
        "32bd3ab4e15085add3816fbce4199b380f2df806b75aeba031ba08c5587c9794",
    ),
];

const fn r(name: &'static str, stg42d: u32, len: u32, sha: &'static str) -> super::Routine {
    super::Routine {
        name,
        stg42d,
        len,
        masked_sha256: sha,
    }
}

// --- constants (stg42D section 4) ---------------------------------------

/// s4 +0x710 (ITEMBOX) and +0x748 (ITEMBOXAIR): 0.0.
const ZERO: f32 = 0.0;
/// s4 +0x714 / +0x74C: full size, 1.0.
const FULL: f32 = 1.0;
/// s4 +0x718: ITEMBOX growth per frame (0.11).
const GROUND_GROW: f32 = f32::from_bits(0x3de1_47ae);
/// s4 +0x750: ITEMBOXAIR growth per frame (0.05).
const AIR_GROW: f32 = f32::from_bits(0x3d4c_cccd);
/// s4 +0x71C / +0x754: spin per frame (degrees), and ITEMBOXAIR's vanish
/// size.
const SPIN: f32 = 5.0;
/// s4 +0x720: height of the ITEMBOX break effect and icon.
const GROUND_ICON_Y: f32 = 7.5;
/// s4 +0x724 / +0x758: the break effect vector's y (0.1).
const BREAK_Y: f32 = f32::from_bits(0x3dcc_cccd);
/// s4 +0x728 / +0x75C: the break effect's size and ITEMBOX's icon size.
const BREAK_SIZE: f32 = 6.0;
/// s4 +0x72C / +0x774: BAMS per degree (65536 / 360).
const BAMS_PER_DEGREE: f32 = f32::from_bits(0x4336_0b61);
/// s4 +0x760 (a double): ITEMBOXAIR's growth once broken.
const AIR_VANISH_GROW: f64 = 0.2;
/// s4 +0x768: ITEMBOXAIR's fade slope.
const AIR_FADE: f32 = 0.25;
/// s4 +0x76C: ITEMBOXAIR's alpha scale (0.7).
const AIR_ALPHA: f32 = f32::from_bits(0x3f33_3333);
/// s4 +0x770: ITEMBOXAIR's icon height.
const AIR_ICON_Y: f32 = 14.5;
/// s4 +0x778: ITEMBOXAIR's icon size.
const AIR_ICON_SIZE: f32 = 9.0;
/// s5 +0x1294C: the ITEMBOXAIR collision centre in the box's frame.
const AIR_CENTER: V3 = [0.0, 14.0, 0.0];

/// The item kind of the 1-up (it can be taken once per SET entry).
pub const KIND_ONE_UP: i32 = 10;
/// The item kind of an empty box.
pub const KIND_EMPTY: i32 = 7;

/// Byte offsets read from main.dol's lbl_801CC168 (the decomp guesses
/// `TWO_PLAYER` for +0x1E).
pub mod game {
    /// +0x0B: the HUD queue takes items only when it is set.
    pub const QUEUE_ON: u32 = 0x0b;
    /// +0x1E.
    pub const SPLIT: u32 = 0x1e;
    /// +0x20.
    pub const FLAG_20: u32 = 0x20;
    /// +0x37: when set the boxes don't grow or spin.
    pub const FREEZE: u32 = 0x37;
}

/// What an item does (the item table's function).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Effect {
    /// AddScore(score), AddNumRing(player, rings), AddMechHP(player, hp).
    Rings { score: i32, rings: i32, hp: f32 },
    /// AddScore(200), fn_800641B8(player, 1), flag 0x20 on the SET entry.
    OneUp,
    /// AddScore(200), fn_80037718(player).
    Fn80037718,
    /// AddScore(200), fn_800374E0(player).
    Fn800374E0,
    /// AddScore(200), an explosion at the box ([`explosion::spawn`]).
    Bomb,
    /// AddScore(200), AddMechHP(player, 12.0).
    MechHp,
    /// AddScore(200), fn_800375FC(player).
    Fn800375FC,
    /// Nothing.
    Empty,
    /// AddScore(200), fn_800373B8(player).
    Fn800373B8,
}

/// One item table row (8 bytes: kind, function).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Item {
    pub kind: i32,
    pub effect: Effect,
}

const fn item(kind: i32, effect: Effect) -> Item {
    Item { kind, effect }
}

/// stg42D s5 +0x11E48 (ITEMBOX) and +0x128D8 (ITEMBOXAIR): both tables
/// list the same kinds; their functions are each box's own copies
/// (0x12A64, 0x12A14, 0x1298C, 0x1293C, 0x12A9C, 0x12AEC, 0x12B24, 0x12B60,
/// 0x12BA4, 0x12BDC, 0x12BE0 and 0x13AC4, 0x13A74, 0x13A04, 0x139B4,
/// 0x13AFC, 0x13B4C, 0x13B84, 0x13BC0, 0x13C04, 0x13C3C, 0x13C40). Their
/// mech HP constants: 0.25 (s4 +0x734 / +0x768), 0.5 (+0x730 / +0x77C),
/// 1.0 (+0x714 / +0x74C), 12.0 (+0x738 / +0x780).
pub const ITEMS_STG42D: [Item; 11] = [
    item(8, Effect::Fn80037718),
    item(
        2,
        Effect::Rings {
            score: 50,
            rings: 5,
            hp: 0.25,
        },
    ),
    item(KIND_ONE_UP, Effect::OneUp),
    item(
        0,
        Effect::Rings {
            score: 100,
            rings: 10,
            hp: 0.5,
        },
    ),
    item(
        1,
        Effect::Rings {
            score: 200,
            rings: 20,
            hp: 1.0,
        },
    ),
    item(3, Effect::Fn800374E0),
    item(4, Effect::Bomb),
    item(5, Effect::MechHp),
    item(6, Effect::Fn800375FC),
    item(KIND_EMPTY, Effect::Empty),
    item(9, Effect::Fn800373B8),
];

/// Texture lists and models the displays name (section-5 offsets).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Models {
    pub ground_tex: u32,
    pub ground_box: u32,
    pub ground_box2: u32,
    pub ground_cnk: u32,
    pub air_tex: u32,
    pub air_outer: u32,
    pub air_inner: u32,
    pub air_body: u32,
}

/// The section-5 data a stage REL carries for the boxes (offsets in that
/// REL's section 5) and its item tables.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StageData {
    /// ITEMBOX CollisionData rows (2) of the whole box.
    pub ground_rows: u32,
    /// ITEMBOX rows (2) of the broken shell.
    pub ground_broken_rows: u32,
    /// ITEMBOXAIR's row.
    pub air_rows: u32,
    pub ground_items: [Item; 11],
    pub air_items: [Item; 11],
    pub models: Models,
    /// The task names "itemBoxManager" and "ExpManExec".
    pub queue_name: u32,
    pub explosion_name: u32,
}

/// stg42D's copy (section-5 offsets from the REL's relocations).
pub const STG42D: StageData = StageData {
    ground_rows: 0x11d70,
    ground_broken_rows: 0x11dd0,
    air_rows: 0x12898,
    ground_items: ITEMS_STG42D,
    air_items: ITEMS_STG42D,
    models: Models {
        ground_tex: 0x11210,
        ground_box: 0x11d20,
        ground_box2: 0x115e0,
        ground_cnk: 0x111d4,
        air_tex: 0x11f04,
        air_outer: 0x12880,
        air_inner: 0x12558,
        air_body: 0x12280,
    },
    queue_name: 0xd9d4,
    explosion_name: 0x11ea0,
};

// --- task state ----------------------------------------------------------

/// The task callbacks the boxes install (stg42D section-1 offsets; 0 =
/// none).
pub mod callback {
    pub const GROUND_DISPLAY: u32 = 0x12690;
    pub const GROUND_ICON: u32 = 0x12750;
    pub const GROUND_SHELL: u32 = 0x1289c;
    pub const GROUND_DEST: u32 = 0x12904;
    pub const AIR_DISPLAY: u32 = 0x13640;
    pub const AIR_ICON: u32 = 0x13710;
    pub const AIR_DEST: u32 = 0x1397c;
}

/// An item box's task, as far as its code reads and writes it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxTask {
    /// twp +0 (signed byte).
    pub mode: i8,
    /// twp +1: set for a box that is a child of another SET task (task +8);
    /// its SET bookkeeping then goes to the parent.
    pub child: i8,
    /// twp +3: ITEMBOXAIR's item index.
    pub air_item: u8,
    /// twp +4 (halfword): bit 4 = the work's player takes the box.
    pub flags: i16,
    /// twp +8, +0xC, +0x10.
    pub ang: [i32; 3],
    /// twp +0x14.
    pub pos: V3,
    /// twp +0x20 (Scale.x from the SET entry): the item.
    pub scale_x: f32,
    /// twp +0x24 (Scale.y): the spin, in degrees.
    pub spin: f32,
    /// twp +0x28 (Scale.z): the size.
    pub size: f32,
    /// task +0x3C: ITEMBOX's item index.
    pub item: i32,
    /// task +0x30 != 0 (the task has a SET entry).
    pub has_set: bool,
    /// The parent's (task +8) +0x30 != 0.
    pub parent_has_set: bool,
    /// The work's (task +0x38) +0x44 (signed byte): a player.
    pub work_player: i8,
    /// task +0x14, +0x18, +0x20 ([`callback`]).
    pub display: u32,
    pub dest: u32,
    pub display_icon: u32,
}

impl Default for BoxTask {
    fn default() -> Self {
        Self {
            mode: 0,
            child: 0,
            air_item: 0,
            flags: 0,
            ang: [0; 3],
            pos: [0.0; 3],
            scale_x: 0.0,
            spin: 0.0,
            size: 0.0,
            item: 0,
            has_set: false,
            parent_has_set: false,
            work_player: -1,
            display: 0,
            dest: 0,
            display_icon: 0,
        }
    }
}

/// The task, or its parent (task +8), that a SET routine is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Who {
    Task,
    Parent,
}

/// What main.dol's 0x803AD7E8 (the explosion hit test other objects call)
/// holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitTestHook {
    /// 0.
    Empty,
    /// ExpMan's test (stg42D 0x12F44, [`explosion::hit_test`]).
    Explosions,
    /// Anything else.
    Other,
}

/// A value the box code reads from outside its task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Global {
    /// A signed byte of main.dol's lbl_801CC168 ([`game`]).
    Game(u32),
    /// lbl_801CC168 +0x7C (a word): ExpMan's display turns by it * 0x120.
    GameTurn,
    /// The stage REL's fog switch (stg42D s6 +0x630, a word).
    StageFog,
    /// main.dol 0x803ADAD0 (a word): the screen being drawn.
    Screen,
    /// main.dol 0x803ADAD4 (a word): the number of screens.
    Screens,
}

/// Calls with an effect outside the task, named after their routine.
#[derive(Debug, Clone, PartialEq)]
pub enum ItemEvent {
    /// CCL_InitShare(task, rows, count, list) (0x8000F7D8).
    CollisionInitShare { rows: u32, count: u8, list: u8 },
    /// CCL_Init(task, rows, count, list) (0x8000F8B8).
    CollisionInit { rows: u32, count: u8, list: u8 },
    /// The first collision info's centre (`cwp->info +8`) is written.
    CollisionCenter(V3),
    /// CCL_CalcRange(task) (0x80006908).
    CollisionCalcRange,
    /// fn_800067F8(task).
    Fn800067F8,
    /// fn_80018D28(task).
    Fn80018D28,
    /// fn_80018B88(task).
    Fn80018B88,
    /// _rename_SetFlag0x20 (0x80065414).
    SetFlag0x20(Who),
    /// SetBroken (0x80065484).
    SetBroken(Who),
    /// DeadOut (0x800653B0).
    DeadOut(Who),
    /// FreeTask (0x80071AE0).
    FreeTask(Who),
    /// DestroyTask (0x80071AF0) of the HUD queue's or ExpMan's own task.
    DestroyTask,
    /// AddScore (0x8006378C).
    AddScore(i32),
    /// AddNumRing(player, count) (0x800637EC).
    AddNumRing { player: i32, count: i32 },
    /// AddMechHP(player, hp) (0x80035CF0).
    AddMechHp { player: i32, hp: f32 },
    /// fn_800641B8(player, count).
    Fn800641B8 { player: i32, count: i32 },
    /// fn_800373B8(player).
    Fn800373B8(i32),
    /// fn_800374E0(player).
    Fn800374E0(i32),
    /// fn_800375FC(player).
    Fn800375FC(i32),
    /// fn_80037718(player).
    Fn80037718(i32),
    /// fn_800168C8(player).
    Fn800168C8(i32),
    /// The box's break effect: the stage REL's 0xCCFC(&pos, &vector, size).
    BreakEffect { pos: V3, vector: V3, size: f32 },
    /// SE_Call(id, 0, 0, 0) (0x8006B978).
    SeCall(i32),
    /// fn_8002FB2C(player, a, b, c).
    Fn8002FB2C { player: i32, a: i32, b: i32, c: i32 },
    /// syFree (0x80119828).
    SyFree,
    /// main.dol 0x803AD7E8 is written.
    SetHitTestHook(HitTestHook),
}

/// What the box code asks of the rest of the game.
pub trait ItemBoxHost: ObjectHost {
    /// CheckBroken (0x8006544C).
    fn check_broken(&mut self, who: Who) -> bool;
    /// _rename_CheckFlag0x20 (0x80065404).
    fn check_flag_0x20(&mut self, who: Who) -> bool;
    /// CCL_IsHitPlayer(task) (0x800066F4): the touching task, or 0.
    fn ccl_is_hit_player(&mut self) -> u32;
    /// fn_80006574(task): another touching task, or 0.
    fn fn_80006574(&mut self) -> u32;
    /// IsThisTaskPlayer(task) (0x80037EDC): its player number, or < 0.
    fn is_this_task_player(&mut self, task: u32) -> i32;
    /// `playerpwp[player]->+0x10` (halfword), main.dol 0x801E7728.
    fn player_flags(&mut self, player: i32) -> i16;
    /// A global the code reads.
    fn global(&mut self, g: Global) -> i32;
    /// CreateElementalTask(level, prio, exec, name) (0x80071E0C); exec is a
    /// stage REL section-1 offset, name a section-5 offset. False when it
    /// returned null.
    fn create_elemental_task(&mut self, level: u8, prio: u8, exec: u32, name: u32) -> bool;
    /// syCalloc(count, size) (0x801197FC); false when it returned null.
    fn sy_calloc(&mut self, count: u32, size: u32) -> bool;
    /// main.dol 0x803AD7E8.
    fn hit_test_hook(&mut self) -> HitTestHook;
    /// A call with an effect.
    fn item_event(&mut self, e: ItemEvent);
}

/// Rendering the display callbacks ask for, in order.
#[derive(Debug, Clone, PartialEq)]
pub enum Draw {
    /// njSetTexture(texlist) (0x80116E18), a section-5 offset.
    SetTexture(u32),
    /// njPushMatrixEx (0x80116098) / njPopMatrixEx (0x801160E4).
    Push,
    Pop,
    /// njTranslateEx(v) (0x8011616C).
    Translate(V3),
    /// njRotateY / njRotateZ (0x80115828, 0x801158A8) on the current matrix.
    RotateY(i32),
    RotateZ(i32),
    /// njScale(0, x, y, z) (0x80115178).
    Scale(V3),
    /// njDisableFog (0x8011C2BC) / njEnableFog (0x8011C2B0); gjSetFog
    /// (0x8011C310) follows each.
    DisableFog,
    EnableFog,
    SetFog,
    /// gjDrawModel (0x80121BDC) / njCnkCacheDrawModel (0x8011DF20).
    GjDrawModel(u32),
    CnkDrawModel(u32),
    /// The stage REL's item icon (stg42D 0x9C08): kind, offset, angle, size.
    Icon {
        kind: i32,
        offset: V3,
        angle: i32,
        size: f32,
    },
    /// fn_800156FC(a, b, c, d).
    Fn800156FC([f32; 4]),
    /// fn_8002B304, fn_8002B35C, fn_8002B348, fn_8002B2F8.
    Fn8002B304,
    Fn8002B35C,
    Fn8002B348,
    Fn8002B2F8,
    /// OffControl3D(flags) (0x8002B2D8) / OnControl3D(flags) (0x8002B2E8).
    OffControl3D(u32),
    OnControl3D(u32),
    /// fn_801218C8(a, b).
    Fn801218C8(u32, u32),
    /// ExpMan's sphere: the stage REL's 0x7684(radius, radius / max).
    Explosion {
        radius: f32,
        ratio: f32,
    },
    /// The HUD queue's icon: the stage REL's 0x9D08(kind, &pos, size).
    HudIcon {
        kind: i32,
        pos: V3,
        size: f32,
    },
    /// fn_800148FC, fn_80014CD8 (before) and fn_8001475C (after the split
    /// HUD's icons).
    Fn800148FC,
    Fn80014CD8,
    Fn8001475C,
}

/// The state the box code keeps outside its tasks: the HUD queues (stg42D
/// s5 +0xD9CC, one task per player) and ExpMan (s6 +0x688).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct World {
    pub queues: [Option<queue::Queue>; 2],
    pub explosions: Option<explosion::Explosions>,
}

// --- shared pieces -------------------------------------------------------

fn freeze(host: &mut impl ItemBoxHost) -> bool {
    host.global(Global::Game(game::FREEZE)) as i8 != 0
}

/// The SET entry has flag 0x20 and is not broken (the 1-up was taken).
fn taken(t: &BoxTask, host: &mut impl ItemBoxHost) -> bool {
    if t.has_set && !host.check_broken(Who::Task) && host.check_flag_0x20(Who::Task) {
        return true;
    }
    t.child != 0
        && t.parent_has_set
        && !host.check_broken(Who::Parent)
        && host.check_flag_0x20(Who::Parent)
}

fn broken(t: &BoxTask, host: &mut impl ItemBoxHost) -> bool {
    (t.has_set && host.check_broken(Who::Task))
        || (t.child != 0 && t.parent_has_set && host.check_broken(Who::Parent))
}

/// A taken 1-up becomes the first empty entry (if the table has one).
fn retarget(items: &[Item; 11], item: i32) -> i32 {
    if items.get(item as usize).map(|i| i.kind) != Some(KIND_ONE_UP) {
        return item;
    }
    items
        .iter()
        .position(|i| i.kind == KIND_EMPTY)
        .map_or(item, |k| k as i32)
}

/// The player that touched the box, if any (the start of 0x123FC/0x1337C).
fn toucher(t: &BoxTask, host: &mut impl ItemBoxHost) -> Option<i32> {
    let player = if t.flags & 4 != 0 && t.work_player != -1 {
        i32::from(t.work_player)
    } else {
        let mut hit = host.ccl_is_hit_player();
        if hit == 0 {
            hit = host.fn_80006574();
            if hit == 0 {
                return None;
            }
        }
        let p = host.is_this_task_player(hit);
        if p >= 0 {
            p
        } else {
            i32::from(t.work_player)
        }
    };
    (player >= 0).then_some(player)
}

/// The player takes the item (unless its pwp +0x10 has bit 0x4000).
fn give(
    t: &BoxTask,
    item: Item,
    air: bool,
    player: i32,
    world: &mut World,
    host: &mut impl ItemBoxHost,
) {
    if host.player_flags(player) & 0x4000 != 0 {
        return;
    }
    queue::item_callback(world, player, item.kind, host);
    effect(t, item.effect, air, player, world, host);
    host.item_event(ItemEvent::Fn800168C8(player));
}

/// The item functions (0x1293C.. for ITEMBOX, 0x139B4.. for ITEMBOXAIR).
fn effect(
    t: &BoxTask,
    e: Effect,
    air: bool,
    player: i32,
    world: &mut World,
    host: &mut impl ItemBoxHost,
) {
    use ItemEvent as E;
    match e {
        Effect::Empty => {}
        Effect::Rings { score, rings, hp } => {
            host.item_event(E::AddScore(score));
            host.item_event(E::AddNumRing {
                player,
                count: rings,
            });
            host.item_event(E::AddMechHp { player, hp });
        }
        Effect::OneUp => {
            host.item_event(E::AddScore(200));
            host.item_event(E::Fn800641B8 { player, count: 1 });
            let who = if air {
                // 0x13A04 doesn't look at the SET entries.
                Some(if t.child != 0 { Who::Parent } else { Who::Task })
            } else if t.child != 0 && t.parent_has_set {
                Some(Who::Parent)
            } else if t.has_set {
                Some(Who::Task)
            } else {
                None
            };
            if let Some(w) = who {
                host.item_event(E::SetFlag0x20(w));
            }
        }
        other => {
            host.item_event(E::AddScore(200));
            match other {
                Effect::Fn80037718 => host.item_event(E::Fn80037718(player)),
                Effect::Fn800374E0 => host.item_event(E::Fn800374E0(player)),
                Effect::Bomb => explosion::spawn(world, t.pos, host),
                Effect::MechHp => host.item_event(E::AddMechHp { player, hp: 12.0 }),
                Effect::Fn800375FC => host.item_event(E::Fn800375FC(player)),
                _ => host.item_event(E::Fn800373B8(player)),
            }
        }
    }
}

/// The SE the boxes play when they break (0x10000 - 0x7FF3).
pub const SE_BREAK: i32 = 0x800d;

fn break_feedback(player: i32, host: &mut impl ItemBoxHost) {
    host.item_event(ItemEvent::SeCall(SE_BREAK));
    host.item_event(ItemEvent::Fn8002FB2C {
        player,
        a: 1,
        b: 0x30,
        c: 0,
    });
}

// --- ITEMBOX -------------------------------------------------------------

/// ITEMBOX's exec (0x12028), one frame.
pub fn ground_exec(
    t: &mut BoxTask,
    world: &mut World,
    stage: &StageData,
    host: &mut impl ItemBoxHost,
) {
    if t.child == 0 && host.range_out() {
        return;
    }
    let m = t.mode as u8;
    if t.mode == 0 || m.wrapping_sub(1) <= 1 {
        t.display = callback::GROUND_DISPLAY;
        t.display_icon = callback::GROUND_ICON;
    } else if m.wrapping_sub(3) <= 1 {
        t.display = callback::GROUND_SHELL;
        t.display_icon = 0;
    }
    match t.mode {
        0 => ground_init(t, stage, host),
        1 => grow(t, GROUND_GROW, host),
        2 => ground_wait(t, world, stage, host),
        3 => {
            host.item_event(ItemEvent::Fn800067F8);
            host.item_event(ItemEvent::CollisionInitShare {
                rows: stage.ground_broken_rows,
                count: 2,
                list: 4,
            });
            t.mode = 4;
        }
        4 | 5 => host.event(Event::CollisionEntry),
        6 => {
            ground_dest(t, host);
            t.mode = 0;
        }
        _ => t.mode = 0,
    }
}

/// 0x12164.
fn ground_init(t: &mut BoxTask, stage: &StageData, host: &mut impl ItemBoxHost) {
    t.mode = 1;
    t.size = ZERO;
    t.spin = ZERO;
    t.item = fctiwz(t.scale_x);
    if t.item >= 11 {
        t.item = 10;
    } else if t.item < 0 {
        t.item = 0;
    }
    if taken(t, host) {
        t.item = retarget(&stage.ground_items, t.item);
    }
    if broken(t, host) {
        host.item_event(ItemEvent::CollisionInitShare {
            rows: stage.ground_broken_rows,
            count: 2,
            list: 4,
        });
        t.mode = 4;
    } else {
        host.item_event(ItemEvent::CollisionInitShare {
            rows: stage.ground_rows,
            count: 2,
            list: 2,
        });
    }
    host.item_event(ItemEvent::Fn80018D28);
    t.dest = callback::GROUND_DEST;
    t.display = callback::GROUND_DISPLAY;
    t.display_icon = callback::GROUND_ICON;
}

/// 0x12364 (ITEMBOX) and 0x132E4 (ITEMBOXAIR): grow to full size.
fn grow(t: &mut BoxTask, step: f32, host: &mut impl ItemBoxHost) {
    if freeze(host) {
        t.size = FULL;
        t.mode = 2;
        return;
    }
    t.size += step;
    if t.size >= FULL {
        t.mode = 2;
        t.size = FULL;
    }
    t.spin += SPIN;
}

/// 0x123FC.
fn ground_wait(t: &mut BoxTask, world: &mut World, stage: &StageData, host: &mut impl ItemBoxHost) {
    if let Some(player) = toucher(t, host) {
        if let Some(&item) = stage.ground_items.get(t.item as usize) {
            give(t, item, false, player, world, host);
        }
        t.mode = 3;
        let mut pos = t.pos;
        pos[1] += GROUND_ICON_Y;
        host.item_event(ItemEvent::BreakEffect {
            pos,
            vector: [ZERO, BREAK_Y, ZERO],
            size: BREAK_SIZE,
        });
        break_feedback(player, host);
        if t.has_set {
            host.item_event(ItemEvent::SetBroken(Who::Task));
        } else if t.child != 0 && t.parent_has_set {
            host.item_event(ItemEvent::SetBroken(Who::Parent));
        }
    }
    host.event(Event::CollisionEntry);
    if !freeze(host) {
        t.spin += SPIN;
    }
}

/// ITEMBOX's dest (0x12904), also its mode 6.
pub fn ground_dest(t: &mut BoxTask, host: &mut impl ItemBoxHost) {
    host.item_event(ItemEvent::Fn80018B88);
    t.item = 0;
}

fn place(t: &BoxTask, d: &mut Vec<Draw>) {
    d.push(Draw::Push);
    d.push(Draw::Translate(t.pos));
    d.push(Draw::RotateY(t.ang[1]));
    d.push(Draw::RotateZ(t.ang[2]));
}

fn fog(on: bool, host: &mut impl ItemBoxHost, d: &mut Vec<Draw>) {
    if host.global(Global::StageFog) != 0 {
        d.push(if on {
            Draw::EnableFog
        } else {
            Draw::DisableFog
        });
        d.push(Draw::SetFog);
    }
}

/// ITEMBOX's display (0x12690).
pub fn ground_display(t: &BoxTask, stage: &StageData, host: &mut impl ItemBoxHost) -> Vec<Draw> {
    let m = &stage.models;
    let mut d = vec![Draw::SetTexture(m.ground_tex)];
    place(t, &mut d);
    d.push(Draw::Scale([t.size; 3]));
    fog(false, host, &mut d);
    d.push(Draw::GjDrawModel(m.ground_box));
    d.push(Draw::GjDrawModel(m.ground_box2));
    fog(true, host, &mut d);
    d.push(Draw::Pop);
    d
}

/// ITEMBOX's icon display (0x12750): the item's icon turns with the spin.
pub fn ground_display_icon(
    t: &BoxTask,
    stage: &StageData,
    host: &mut impl ItemBoxHost,
) -> Vec<Draw> {
    let m = &stage.models;
    let mut d = Vec::new();
    place(t, &mut d);
    d.push(Draw::Scale([t.size; 3]));
    fog(false, host, &mut d);
    if let Some(item) = stage.ground_items.get(t.item as usize) {
        if item.kind != KIND_EMPTY {
            d.push(Draw::Icon {
                kind: item.kind,
                offset: [ZERO, GROUND_ICON_Y, ZERO],
                angle: fctiwz(BAMS_PER_DEGREE * t.spin),
                size: BREAK_SIZE,
            });
        }
    }
    d.push(Draw::SetTexture(m.ground_tex));
    d.push(Draw::CnkDrawModel(m.ground_cnk));
    fog(true, host, &mut d);
    d.push(Draw::Pop);
    d
}

/// ITEMBOX's broken shell display (0x1289C).
pub fn ground_display_shell(t: &BoxTask, stage: &StageData) -> Vec<Draw> {
    let mut d = vec![Draw::SetTexture(stage.models.ground_tex)];
    place(t, &mut d);
    d.push(Draw::GjDrawModel(stage.models.ground_box));
    d.push(Draw::Pop);
    d
}

// --- ITEMBOXAIR ----------------------------------------------------------

/// ITEMBOXAIR's exec (0x12FD0), one frame.
pub fn air_exec(
    t: &mut BoxTask,
    world: &mut World,
    stage: &StageData,
    host: &mut impl ItemBoxHost,
) {
    if t.child == 0 && host.range_out() {
        return;
    }
    match t.mode {
        0 => air_init(t, stage, host),
        1 => grow(t, AIR_GROW, host),
        2 => air_wait(t, world, stage, host),
        3 => t.mode = 4,
        4 => air_vanish(t, host),
        5 => host.event(Event::CollisionEntry),
        6 => {
            air_dest(t, host);
            t.mode = 0;
        }
        _ => t.mode = 0,
    }
}

/// 0x130A4.
fn air_init(t: &mut BoxTask, stage: &StageData, host: &mut impl ItemBoxHost) {
    if t.has_set && host.check_broken(Who::Task) {
        host.item_event(ItemEvent::DeadOut(Who::Task));
        return;
    }
    if t.child != 0 && t.parent_has_set && host.check_broken(Who::Parent) {
        host.item_event(ItemEvent::DeadOut(Who::Parent));
        return;
    }
    t.mode = 1;
    t.size = ZERO;
    t.spin = ZERO;
    t.air_item = fctiwz(t.scale_x) as u8;
    if t.air_item >= 11 {
        t.air_item = 10;
    }
    if taken(t, host) {
        t.air_item = retarget(&stage.air_items, i32::from(t.air_item)) as u8;
    }
    host.item_event(ItemEvent::CollisionInit {
        rows: stage.air_rows,
        count: 1,
        list: 2,
    });
    host.item_event(ItemEvent::CollisionCenter(air_center(t)));
    host.item_event(ItemEvent::CollisionCalcRange);
    host.item_event(ItemEvent::Fn80018D28);
    t.dest = callback::AIR_DEST;
    t.display = callback::AIR_DISPLAY;
    t.display_icon = callback::AIR_ICON;
}

/// The collision centre 0x130A4 computes: njPushMatrixEx, njUnitMatrix,
/// njTranslateEx(pos), njRotateY, njRotateZ, njCalcPoint((0, 14, 0)).
pub fn air_center(t: &BoxTask) -> V3 {
    let mut stack = MatrixStack::new();
    stack.push();
    *stack.current_mut() = IDENTITY;
    crate::enemy_lib::e_ai::translate(stack.current_mut(), t.pos);
    stack.rotate(Axis::Y, t.ang[1]);
    stack.rotate(Axis::Z, t.ang[2]);
    stack.calc_point(AIR_CENTER)
}

/// 0x1337C.
fn air_wait(t: &mut BoxTask, world: &mut World, stage: &StageData, host: &mut impl ItemBoxHost) {
    if let Some(player) = toucher(t, host) {
        if let Some(&item) = stage.air_items.get(usize::from(t.air_item)) {
            give(t, item, true, player, world, host);
        }
        t.mode = 3;
        host.item_event(ItemEvent::BreakEffect {
            pos: t.pos,
            vector: [ZERO, BREAK_Y, ZERO],
            size: BREAK_SIZE,
        });
        break_feedback(player, host);
        if t.has_set {
            host.item_event(ItemEvent::SetBroken(Who::Task));
        }
        if t.child != 0 && t.parent_has_set {
            host.item_event(ItemEvent::SetBroken(Who::Parent));
        }
    }
    if t.mode != 3 {
        host.event(Event::CollisionEntry);
    }
    if !freeze(host) {
        t.spin += SPIN;
    }
}

/// 0x135A0: the broken air box grows (in double precision) past 5.0, then
/// its task (or its parent) goes.
fn air_vanish(t: &mut BoxTask, host: &mut impl ItemBoxHost) {
    t.size = (f64::from(t.size) + AIR_VANISH_GROW) as f32;
    if t.size > SPIN {
        t.size = SPIN;
        let (who, set) = if t.child != 0 {
            (Who::Parent, t.parent_has_set)
        } else {
            (Who::Task, t.has_set)
        };
        host.item_event(if set {
            ItemEvent::DeadOut(who)
        } else {
            ItemEvent::FreeTask(who)
        });
    }
}

/// ITEMBOXAIR's dest (0x1397C), also its mode 6. It clears task +0x3C
/// (ITEMBOX's item word), not twp +3.
pub fn air_dest(t: &mut BoxTask, host: &mut impl ItemBoxHost) {
    host.item_event(ItemEvent::Fn80018B88);
    t.item = 0;
}

/// ITEMBOXAIR's display (0x13640): nothing once it is bigger than 1.0.
pub fn air_display(t: &BoxTask, stage: &StageData) -> Vec<Draw> {
    let m = &stage.models;
    let mut d = Vec::new();
    if t.size > FULL {
        return d;
    }
    if t.mode == 4 {
        d.extend([Draw::DisableFog, Draw::SetFog]);
    }
    d.push(Draw::SetTexture(m.air_tex));
    place(t, &mut d);
    d.push(Draw::Scale([t.size; 3]));
    d.push(Draw::CnkDrawModel(m.air_outer));
    d.push(Draw::CnkDrawModel(m.air_inner));
    d.push(Draw::Pop);
    if t.mode == 4 {
        d.extend([Draw::EnableFog, Draw::SetFog]);
    }
    d
}

/// The alpha 0x13710 fades a growing broken air box with:
/// `clamp(1 - (size - 1) * 0.25, 0, 1) * 0.7` (NaN gives 0).
pub fn air_alpha(size: f32) -> f32 {
    let mut a = FULL - (size - FULL) * AIR_FADE;
    if a > FULL {
        a = FULL;
    }
    if a.is_nan() || a < ZERO {
        a = ZERO;
    }
    a * AIR_ALPHA
}

/// ITEMBOXAIR's icon display (0x13710).
pub fn air_display_icon(t: &BoxTask, stage: &StageData) -> Vec<Draw> {
    let m = &stage.models;
    let mut d = Vec::new();
    if t.mode == 4 {
        d.extend([Draw::DisableFog, Draw::SetFog]);
    }
    let big = t.size > FULL;
    if big {
        d.extend([
            Draw::Fn8002B304,
            Draw::Fn8002B35C,
            Draw::OffControl3D(0x220),
            Draw::OnControl3D(0x810),
            Draw::Fn801218C8(0xff, 0x800),
            Draw::Fn800156FC([air_alpha(t.size), FULL, FULL, FULL]),
        ]);
    }
    place(t, &mut d);
    d.push(Draw::Scale([t.size; 3]));
    if let Some(item) = stage.air_items.get(usize::from(t.air_item)) {
        if item.kind != KIND_EMPTY {
            d.push(Draw::Icon {
                kind: item.kind,
                offset: [ZERO, AIR_ICON_Y, ZERO],
                angle: fctiwz(BAMS_PER_DEGREE * t.spin),
                size: AIR_ICON_SIZE,
            });
        }
    }
    d.push(Draw::SetTexture(m.air_tex));
    if big {
        d.push(Draw::CnkDrawModel(m.air_outer));
    }
    d.push(Draw::CnkDrawModel(m.air_body));
    if big {
        d.push(Draw::CnkDrawModel(m.air_inner));
    }
    d.push(Draw::Pop);
    if big {
        d.extend([
            Draw::Fn800156FC([ZERO; 4]),
            Draw::Fn8002B348,
            Draw::Fn8002B2F8,
        ]);
    }
    if t.mode == 4 {
        d.extend([Draw::EnableFog, Draw::SetFog]);
    }
    d
}
