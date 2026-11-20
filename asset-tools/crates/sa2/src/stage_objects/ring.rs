//! RING (object list row 2), the scattered ring Tobitiri and the shared ring
//! model list they draw through (GSNE8P o_ring.c).
//!
//! stg42D.rel section 1, all relocation-masked identical in the other stage
//! RELs except where noted:
//!
//! * 0x10544, 0x105F8 (RINGLINE) and 0x109B8 (RINGCIRC): reused from
//!   [`crate::ring_group`], ported from City Escape (stg13D.rel 0x27F08,
//!   0x27FBC, 0x2837C there, the same masked bytes). The group's other
//!   routines 0x10748, 0x10828 and 0x10AA4 differ from stg13D's only in the
//!   displacement of their REL-local `bl` to the link helpers below
//!   (0x1079C, 0x10948, 0x10BC4), so they are not listed.
//! * 0x19550 / 0x1957C: the ring-group link and unlink helpers RINGLINE and
//!   RINGCIRC call on their RING children ([`link`], [`unlink`]).
//! * 0x195AC `o_ring_0`: four pickup particles (REL-local `MakeParticle`
//!   0xC56C, a hook here) around the ring; the native port is
//!   [`crate::stage_gameplay::pickup_particles`] fed with njSin / njCos
//!   (0x80112398 / 0x801123B0, [`crate::ninja_trig`]).
//! * 0x196E8 `Ring`: the RING exec ([`ring_exec`]).
//! * 0x19D70 `DamegeRingScatter`: the hit ring drop. Already ported on main as
//!   `finalhazard`'s ring drop (Boss_Last2D 0x19470, the same masked bytes);
//!   [`damege_ring_scatter`] reaches it, nothing is copied here.
//! * 0x19F48 `o_ring_5`: the gravity-axis classifier, ported on main as
//!   [`crate::damage_ring::gravity_face`] and reused ([`o_ring_5`]).
//! * 0x19FF0 `Tobitiri`: the scattered ring's exec ([`tobitiri_exec`]).
//! * 0x1AECC `RingEnd` ([`ring_end`]), 0x1AF40 `o_ring_8` (the magnet pull),
//!   0x1B1E0 `CreateRingObject`, 0x1B2E8 `o_ring_10`, 0x1B318 `RingModelDest`
//!   ([`ring_model_dest`]), 0x1B3A4 `RingModelExec` ([`ring_model_exec`]),
//!   0x1B478 `RingModelDisp` ([`ring_model_disp`]), 0x1B5EC `RingModelDisp2`
//!   ([`ring_model_disp2`]) and 0x1B768 `CreateRingTask`.
//!
//! `o_ring_0`, `Ring` and `Tobitiri` call REL-local routines with unrelocated
//! `bl`s (`MakeParticle` 0xC56C, `fabsf` 0x84, `CreateRingObject`, `o_ring_8`,
//! `o_ring_0`, `o_ring_5`), whose displacements differ from stage to stage;
//! the rest of each is identical. Their `ROUTINES` entries are the spans
//! between those `bl`s (the 4-byte `mr r3, r30` between the two adjacent
//! calls at 0x19AE0 / 0x19AE8 is left out).
//!
//! Module state (stg42D section 6): `colli_timer` +0x6A8, `RingObjects`
//! +0x6AC, `CleanupRingObjs` +0x6B0, `RingTP` +0x6B4 ([`RingWorld`]); the
//! object-fog switch `DisableObjectFog` +0x630 is an input
//! ([`RingGlobals::disable_fog`]).
//!
//! Constants (stg42D section 4): 0.0 +0x8B8, 0.02 +0x8BC, 4.0 +0x8C0, 1.8
//! +0x8C4, 1.5 +0x8C8, 2.5 +0x8CC, -1000000 +0x8D8, 5.0 +0x8DC, -1.0 +0x8E0,
//! -2.0 +0x8E4, -4.0 +0x8E8, 0.1 +0x8EC, 160000 +0x8F0, 8.0 +0x8F4, 10.0
//! +0x8F8, 65536/360 +0x8FC, 360 +0x900, 2^-15 +0x904, 1.0 +0x908, 0.9
//! +0x90C, 0.4 +0x914, 100 +0x918, 3.44 +0x91C, -0.08 +0x920, 0.995 +0x924,
//! 0.95 +0x928, 50 +0x92C, 1.3 +0x930, 0.85 +0x934, 0.5 +0x938, 3.0 +0x93C,
//! -10 +0x940, 65 +0x944, 130 +0x948, 1000000 +0x94C. Section 5: the shadow
//! scale 0.8 +0x1ACF4 and 0.4 +0x1ACF8, the ring object template +0x1B0FC
//! (evalflags 0x16, the ring model, unit scale, no sibling).
//!
//! Main.dol data read (GSNE8P): `playerpwp` 0x801E7728, `playertwp`
//! 0x801E7768, gravity 0x801E5624 and its angles 0x803AD914 / 0x803AD918,
//! the game state bytes 0x801CC168 +0x37 / +0x3D, the task at 0x803ADB04
//! ([`RingGlobals`]).
use super::host::ObjectHost;
use crate::enemy_lib::engine::{distance, scalor, unit_vector_len};
use crate::ninja_matrix::{concat, rotate, Axis, MatrixStack, Mtx, IDENTITY};
use crate::ninja_trig::{fctiwz, fmadds, nj_cos, nj_sin};

pub type V3 = [f32; 3];

const fn r(name: &'static str, stg42d: u32, len: u32, sha: &'static str) -> super::Routine {
    super::Routine {
        name,
        stg42d,
        len,
        masked_sha256: sha,
    }
}

/// The routines this file ports or reuses (stg42D.rel section 1).
pub const ROUTINES: &[super::Routine] = &[
    r(
        "RINGLINE init (ring_group.rs)",
        0x10544,
        0xb4,
        "9ae0df316d33603afbc05f9c17ec802027507c3a43d5e52f1b5deffd54d7c14e",
    ),
    r(
        "RINGLINE 0x105F8 (ring_group.rs)",
        0x105f8,
        0x150,
        "8108a044bedb70b1f66f35be09217c19642e55e9d165afd05790c082f3951912",
    ),
    r(
        "RINGCIRC init (ring_group.rs)",
        0x109b8,
        0xec,
        "e3d1562a81a44a77b152f27674d48fc4c41aa5c6585809888ced6ad34e000147",
    ),
    r(
        "RING link (0x19550)",
        0x19550,
        0x2c,
        "ca3da343ed625949c08286edccbbee4c3a8c213314efc6a1c7635d14ccc0ffbe",
    ),
    r(
        "RING unlink (0x1957C)",
        0x1957c,
        0x30,
        "ae5b226b7a254b42c39e22c11f38c3f113669d2908ebec26d20616c34dadeefd",
    ),
    r(
        "o_ring_0 (to the MakeParticle bl)",
        0x195ac,
        0x104,
        "de7b48813567666e2205e0cd1f8fb0c83803708b6938f38ee4eef6cbe5e912b6",
    ),
    r(
        "o_ring_0 (after the MakeParticle bl)",
        0x196b4,
        0x34,
        "9bb76a8897fc1083cbf425590df86c94fc4301fdc2797e295e84a178e838e865",
    ),
    r(
        "Ring (to the CreateRingObject bl)",
        0x196e8,
        0x2d4,
        "e960d870cdaabeb65373fd5721d28cc04eccd3ed3af6fe5550af7b22eeac994f",
    ),
    r(
        "Ring (to the o_ring_0 bl)",
        0x199c0,
        0x120,
        "ddaae8553518d32967ca2ec68984d33cb33f62760f62ad23f55309c47acb87a5",
    ),
    r(
        "Ring (o_ring_8 bl to the fabsf bl)",
        0x19aec,
        0x120,
        "bea6ab4f45730b7189bb8f432946b6db4d54da1df038d5835d88b82d6aca7e1f",
    ),
    r(
        "Ring (after the fabsf bl)",
        0x19c10,
        0x160,
        "b738c9db9ba6088d041637d5ffd492afe495c4840e904d6e1d20840c13374d2a",
    ),
    r(
        "DamegeRingScatter (finalhazard ring drop)",
        0x19d70,
        0x1d8,
        "078ac44ca0d692b5824bb48e58d4019d0f383df64b8654e4f10e9e34eae06414",
    ),
    r(
        "o_ring_5 (damage_ring::gravity_face)",
        0x19f48,
        0xa8,
        "1b3bc96dc1fe120c7ca2153de2b6ae420b9752b668bc58986d5446d93d1c4a69",
    ),
    r(
        "Tobitiri (to the first o_ring_5 bl)",
        0x19ff0,
        0x21c,
        "4599b60ac38a194baff0f3f09e3cc4ed9092fb39b1b7b264266534e7a9a5238e",
    ),
    r(
        "Tobitiri (to the CreateRingObject bl)",
        0x1a210,
        0x2f0,
        "996f5d89d58dbc273e2149a87671a6541c2592d37978d172869ff653b340d990",
    ),
    r(
        "Tobitiri (to the second o_ring_5 bl)",
        0x1a504,
        0x22c,
        "d2ef3ecd42d1e86ed43cab5fdad0722e6f3d445f065ad69de98b93f4676460e2",
    ),
    r(
        "Tobitiri (to the third o_ring_5 bl)",
        0x1a734,
        0x218,
        "41921c92016aa2ef6cde0ebc18e8cdc47ffabf0d2e85b1604de5d8d6c7e4d67e",
    ),
    r(
        "Tobitiri (to the fabsf bl)",
        0x1a950,
        0x260,
        "42f5821476b9dfda457ab626288c28ead1ad8853e4c42ce1ba18f90e2f66ae53",
    ),
    r(
        "Tobitiri (to the o_ring_0 bl)",
        0x1abb4,
        0x2e4,
        "25b22ee2e6a1a84352e47dd7a543b3010ea7b3f13a4f7bcade1ed979579295f8",
    ),
    r(
        "Tobitiri (after the o_ring_0 bl)",
        0x1ae9c,
        0x30,
        "5b669c13bd60968f59c53febe292bcd279ae28638d366391a44d40de32801fcd",
    ),
    r(
        "RingEnd",
        0x1aecc,
        0x74,
        "9ad17a4ffbba0b9bf673138c08a0557d25508f6f33345484c90e779da917de64",
    ),
    r(
        "o_ring_8",
        0x1af40,
        0x2a0,
        "4c3d791307c903217bfd21c8a1784b5e3db837ac5c9cdc43ca357f7338e02499",
    ),
    r(
        "CreateRingObject",
        0x1b1e0,
        0x108,
        "c4ff30387077599a626a307af06e7f81b9aa5b060238061ad66f89eab9b2fec2",
    ),
    r(
        "o_ring_10",
        0x1b2e8,
        0x30,
        "c69ae69e9370861bda2ccaf2e4924a5ec9ef7040697c6976eeeaf271eed1b4a4",
    ),
    r(
        "RingModelDest",
        0x1b318,
        0x8c,
        "5f6ae179d59a2e913532caf0c52ac8926c56754f5ce7a76216e7e7699f485c9d",
    ),
    r(
        "RingModelExec",
        0x1b3a4,
        0xd4,
        "6870f59bf9b88a745a54771ba40b31d8a4f5de51687d9ad88c1feae2b24d48a4",
    ),
    r(
        "RingModelDisp",
        0x1b478,
        0x174,
        "3d0e02258a5f1b5f51b4df1e7a710f5d6dae7e0a3653a4c89a1d14b5c301de85",
    ),
    r(
        "RingModelDisp2",
        0x1b5ec,
        0x17c,
        "9978b5d0950ddaa1909511aed8eb6009c6ab56c4b736afd382efd67341d0fd84",
    ),
    r(
        "CreateRingTask",
        0x1b768,
        0x94,
        "8e421a0aff5711702ef6eb1317ff75903d7d762058ee8fa555395deb2d1a8e82",
    ),
];

/// "No shadow" height (stg42D section 4 +0x8D8).
pub const NO_SHADOW: f32 = -1_000_000.0;
/// The other height RingModelDisp2 skips (section 4 +0x94C).
pub const FAR_SHADOW: f32 = 1_000_000.0;
/// BAMS per degree (section 4 +0x8FC).
const BAMS_PER_DEGREE: f32 = f32::from_bits(0x4336_0b61);
/// 2^-15: rand() to [0, 1) (section 4 +0x904).
const RAND_SCALE: f32 = f32::from_bits(0x3800_0000);
/// The ring's floor clearance (section 4 +0x91C).
const LIFT: f32 = f32::from_bits(0x405c_28f6);
/// The template's evalflags with NJD_EVAL_UNIT_POS | UNIT_ANG cleared
/// (section 5 +0x1B0FC, 0x16 & !3).
pub const OBJECT_EVALFLAGS: u32 = 0x14;
/// NJD_EVAL_HIDE.
pub const HIDE: u32 = 8;

/// Ring models and texture lists, as stg42D section-5 offsets (each stage
/// carries its own copy at its own offset).
pub mod data {
    /// `_rename_ring_tex_0` (RingModelDisp's texture list).
    pub const RING_TEXLIST: u32 = 0x19cdc;
    /// `_rename_ring_tex_1` (the shadow's texture list).
    pub const SHADOW_TEXLIST: u32 = 0x1ab94;
    /// `model_ring_l1` (nearer than 65).
    pub const MODEL_L1: u32 = 0x1ab50;
    /// `model_ring_l2` (nearer than 130).
    pub const MODEL_L2: u32 = 0x1a5f0;
    /// `model_ring` (the rest).
    pub const MODEL: u32 = 0x1a110;
    /// `_rename_ring_tex_2` (the shadow model).
    pub const SHADOW_MODEL: u32 = 0x1acd0;
    /// The shadow's X and Z scale (`sScaleX`, `sScaleZ`, +0x1ACF4 / +0x1ACF8).
    pub const SHADOW_SCALE: [f32; 2] = [0.8, 0.4];
}

/// `cwp` fields the ring touches: `flag` (+4, u16) and `colli_range` (+8).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Collision {
    pub flag: u16,
    pub colli_range: f32,
}

/// The task work (`taskwk`) fields the ring code uses.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TaskWork {
    /// +0 (signed byte).
    pub mode: i8,
    /// +1 (signed byte): Ring's probe countdown, Tobitiri's age.
    pub smode: i8,
    /// +2.
    pub id: u8,
    /// +3 (unsigned byte).
    pub btimer: u8,
    /// +6 (u16).
    pub wtimer: u16,
    /// +8, +0xC, +0x10 (BAMS).
    pub ang: [i32; 3],
    /// +0x14..+0x1C.
    pub pos: V3,
    /// +0x20..+0x28 (Ring: shadow height in y; Tobitiri: velocity).
    pub scl: V3,
    /// +0x2C, set by CCL_InitShare.
    pub cwp: Option<Collision>,
}

/// `ringfwk` (0x14 bytes, `tp->fwp`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RingFwk {
    /// +4: spin (degrees).
    pub spin: f32,
    /// +8: Tobitiri's resting floor.
    pub rest: f32,
    /// +0xC: the pause byte (0x801CC168 +0x37) when the task started.
    pub start_pause: i32,
    /// +0x10: bit 0 = no ground probe.
    pub flag: i32,
}

/// A ring group's per-ring entry (`ringmwk`: `flag`, `tp`) a RING child is
/// linked to through `tp->mwp`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RingLink {
    pub flag: u32,
    /// The linked RING task's handle, 0 when none.
    pub tp: u32,
}

/// A RING or Tobitiri task.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RingTask {
    /// Identity written into a link's `tp`.
    pub handle: u32,
    pub twp: TaskWork,
    /// Tobitiri's `awp->work.f[0]` (floor coordinate) and `f[1]` (probe
    /// countdown).
    pub awp: [f32; 2],
    /// `tp->mwp`: index into [`RingWorld::links`].
    pub mwp: Option<usize>,
    /// `tp->fwp`: the allocation's address (0 = none) and its contents.
    pub fwp: u32,
    pub fwk: RingFwk,
    /// `tp->work.ptr`: the ring object's address (0 = none).
    pub work: u32,
    /// `tp->ocp` is set (the task came from a SET entry).
    pub ocp: bool,
    /// `tp->dest` is RingEnd.
    pub dest_ring_end: bool,
}

/// A `ringwk` (0x48 bytes) in the shared model list.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RingObject {
    pub addr: u32,
    /// +0.
    pub evalflags: u32,
    /// +4 is non-null (cleared by o_ring_10).
    pub model: bool,
    /// +8..+0x10.
    pub pos: V3,
    /// +0x14..+0x1C.
    pub ang: [i32; 3],
    /// +0x30: the next object's address.
    pub sibling: u32,
    /// +0x38..+0x40.
    pub shadow_ang: [i32; 3],
    /// +0x44.
    pub shadow_posy: f32,
}

/// The module's own state (stg42D section 6 +0x6A8..+0x6B4) and the link
/// entries of the ring groups.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RingWorld {
    /// `colli_timer`.
    pub colli_timer: i32,
    /// Every allocated ring object (in no particular order).
    pub objects: Vec<RingObject>,
    /// `RingObjects`: the list head's address.
    pub head: u32,
    /// `CleanupRingObjs`.
    pub cleanup: i32,
    /// `RingTP`: the model task's handle.
    pub ring_tp: u32,
    pub links: Vec<RingLink>,
}

impl RingWorld {
    pub fn object(&self, addr: u32) -> Option<&RingObject> {
        self.objects.iter().find(|o| addr != 0 && o.addr == addr)
    }
    pub fn object_mut(&mut self, addr: u32) -> Option<&mut RingObject> {
        self.objects
            .iter_mut()
            .find(|o| addr != 0 && o.addr == addr)
    }
    /// The list from `RingObjects` along the siblings (bounded by the
    /// object count).
    pub fn list(&self) -> Vec<u32> {
        let mut out = Vec::new();
        let mut at = self.head;
        while at != 0 && out.len() <= self.objects.len() {
            out.push(at);
            at = self.object(at).map_or(0, |o| o.sibling);
        }
        out
    }
}

/// A player's `playerwk` fields (array 0x801E7728).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PlayerWork {
    /// +0x10 (signed halfword): bit 2 magnet, bit 0x4000 no ring credit.
    pub item: i16,
    /// +0x64: speed.
    pub spd: V3,
}

/// A player's `taskwk` fields (array 0x801E7768).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PlayerTask {
    /// +0x14.
    pub pos: V3,
    /// `cwp->info->center` (+0x2C, +0xC, +8).
    pub center: V3,
}

/// Game state the ring code reads.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RingGlobals {
    /// Gravity direction 0x801E5624.
    pub gravity: V3,
    /// The gravity's Z and X angles (0x803AD914, 0x803AD918).
    pub gravity_angles: [i32; 2],
    /// 0x801CC168 +0x37 (signed byte): non-zero while play is paused.
    pub pause: i8,
    /// 0x801CC168 +0x3D (signed byte): non-zero hides the ring list.
    pub hide_rings: i8,
    /// `DisableObjectFog` (stg42D section 6 +0x630).
    pub disable_fog: i32,
    /// A missing player work reads as item 0 (the original dereferences
    /// `playerpwp[0]` / `[1]` unchecked when crediting a pickup).
    pub players: [Option<PlayerWork>; 2],
    /// A missing magnetized player's task reads as all zeros (the original
    /// dereferences it unchecked).
    pub player_tasks: [Option<PlayerTask>; 2],
    /// The position of the task at 0x803ADB04 (its +0x14), the ground probe
    /// range origin.
    pub origin: V3,
}

/// One `GetShadowPosXYZ` hit entry (findflag set): `onpos`, `angx`, `angz`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowHit {
    pub onpos: f32,
    pub angx: i32,
    pub angz: i32,
}

/// Tasks the ring code creates through CreateElementalTask (0x80071E0C).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskKind {
    /// (10, 2, Tobitiri, "Tobitiri").
    Tobitiri,
    /// (2, 1, RingModelExec, "RingModelExec"), with RingModelDisp,
    /// RingModelDisp2 and RingModelDest installed.
    RingModel,
}

/// A call the ring code makes that returns nothing it uses.
#[derive(Debug, Clone, PartialEq)]
pub enum RingEvent {
    /// syFree (0x80119828).
    Free(u32),
    /// CCL_Entry (0x8000F4EC), with the task position at the call.
    CollisionEntry(V3),
    /// AddScore (0x8006378C).
    AddScore(i32),
    /// AddNumRing (0x800637EC).
    AddNumRing(i32, i32),
    /// AddMechHP (0x80035CF0).
    AddMechHp(i32, f32),
    /// SE_CallRing (0x8006A98C).
    SeCallRing(i8),
    /// The REL's `MakeParticle` (stg42D 0xC56C): position, velocity, size.
    MakeParticle(V3, V3, f32),
    /// DeadOut (0x800653B0).
    DeadOut,
    /// FreeTask (0x80071AE0).
    FreeTask,
    /// DestroyTask (0x80071AF0) on a task handle (0 = the running task).
    DestroyTask(u32),
    /// A created Tobitiri task's position and Y angle are written.
    SpawnTobitiri { task: u32, pos: V3, ang_y: i32 },
    /// njSetTexture (0x80116E18) with a section-5 texture list.
    SetTexture(u32),
    /// njDisableFog (0x8011C2BC).
    DisableFog,
    /// njEnableFog (0x8011C2B0).
    EnableFog,
    /// gjSetFog (0x8011C310).
    SetFog,
    /// gjDrawModel (0x80121BDC) of a section-5 model, with the current
    /// matrix.
    DrawModel(u32, Mtx),
}

/// What the ring code asks of the engine, beyond CheckRangeOut.
pub trait RingHost: ObjectHost {
    /// syCalloc(1, size) (0x801197FC): the block's address, 0 on failure.
    fn calloc(&mut self, size: u32) -> u32;
    /// CCL_InitShare(task, ring_info, 1, list) (0x8000F7D8): whether the
    /// task got a collision work.
    fn ccl_init_share(&mut self, list: u8) -> bool;
    /// CCL_IsHitPlayer(task) (0x800066F4) is non-null.
    fn ccl_is_hit_player(&mut self) -> bool;
    /// IsThisTaskPlayer(hit task) (0x80037EDC).
    fn is_this_task_player(&mut self) -> i32;
    /// GetShadowPos(x, y, z, &twp->ang) (0x80067AF0); it may write the
    /// angles.
    fn get_shadow_pos(&mut self, pos: V3, ang: &mut [i32; 3]) -> f32;
    /// GetShadowPosXYZ (0x80067210) from `pos`: entry `face`, when found.
    fn get_shadow_pos_xyz(&mut self, pos: V3, face: usize) -> Option<ShadowHit>;
    /// CreateElementalTask (0x80071E0C): the new task's handle, 0 on failure.
    fn create_task(&mut self, kind: TaskKind) -> u32;
    /// MSL rand (0x8010DC88).
    fn rand(&mut self) -> i32;
    /// Ninja's matrix stack.
    fn matrix(&mut self) -> &mut MatrixStack;
    fn ring_event(&mut self, e: RingEvent);
}

/// 0x19550: link a RING child to its group entry while it has not started:
/// `tp->mwp = link`, the entry's `tp` is the task, and mode becomes 2.
pub fn link(task: &mut RingTask, index: usize, world: &mut RingWorld) {
    if task.twp.mode == 0 {
        task.mwp = Some(index);
        if let Some(l) = world.links.get_mut(index) {
            l.tp = task.handle;
        }
        task.twp.mode = 2;
    }
}

/// 0x1957C: unlink a RING child from its group entry (flag bit 0 cleared).
pub fn unlink(task: &mut RingTask, world: &mut RingWorld) {
    if let Some(i) = task.mwp {
        if let Some(l) = world.links.get_mut(i) {
            l.flag &= !1;
            l.tp = 0;
        }
        task.mwp = None;
    }
}

/// `o_ring_5` (0x19F48): which gravity face (0..5) the ring falls toward.
pub fn o_ring_5(g: V3) -> usize {
    crate::damage_ring::gravity_face(g)
}

/// `DamegeRingScatter` (stg42D 0x19D70): the port on main, run against the
/// Boss_Last2D.rel it was verified with (Boss_Last2D 0x19470, the same
/// relocation-masked bytes).
pub fn damege_ring_scatter(
    boss_last2d: &[u8],
    frame: crate::finalhazard::RingDropFrame,
) -> crate::Result<serde_json::Value> {
    crate::finalhazard::placement_clock(
        boss_last2d,
        crate::finalhazard::PlacementClock::RingDrop(Box::new(frame)),
    )
}

/// `o_ring_0` (0x195AC): the four pickup particles.
pub fn o_ring_0(pos: V3, host: &mut impl RingHost) {
    for p in crate::stage_gameplay::pickup_particles(pos, |a| {
        (nj_sin(i32::from(a)), nj_cos(i32::from(a)))
    }) {
        host.ring_event(RingEvent::MakeParticle(p.position, p.velocity, p.scale));
    }
}

/// `fctiwz(65536/360 * degrees)`.
fn bams(degrees: f32) -> i32 {
    fctiwz(BAMS_PER_DEGREE * degrees)
}

/// rand() to a random BAMS angle, as Ring and Tobitiri compute it.
fn random_angle(r: i32) -> i32 {
    bams(360.0 * (RAND_SCALE * r as f32))
}

/// SqMag (0x800D24A4).
fn sq_mag(x: f32, y: f32, z: f32) -> f32 {
    fmadds(z, z, fmadds(x, x, y * y))
}

/// CalcAdvanceAsPossible (0x8002BA50): move `from` toward `to` by `step`
/// along njUnitVector of the difference (0x80115D24); unchanged when its
/// length estimate is 0 (r2-0x77CC).
fn calc_advance(from: V3, to: V3, step: f32) -> V3 {
    let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let (len, u) = unit_vector_len(d);
    if len == 0.0 {
        return from;
    }
    let s = [u[0] * step, u[1] * step, u[2] * step];
    [s[0] + from[0], s[1] + from[1], s[2] + from[2]]
}

/// njTranslate / njTranslateEx on the current matrix (0x80115BD8,
/// 0x8011616C): m = m * T(v).
fn translate(m: &mut Mtx, v: V3) {
    crate::enemy_lib::e_ai::translate(m, v);
}

/// njScale (0x80115178): m = m * S(v) (PSMTXScale, then PSMTXConcat).
fn scale(m: &mut Mtx, v: V3) {
    let s = [
        [v[0], 0.0, 0.0, 0.0],
        [0.0, v[1], 0.0, 0.0],
        [0.0, 0.0, v[2], 0.0],
    ];
    *m = concat(m, &s);
}

/// Whether a pickup by player `p` credits it (item bit 0x4000 clear).
fn credited(p: i32, g: &RingGlobals) -> bool {
    let no_credit = |i: usize| g.players[i].map_or(0, |w| w.item) & 0x4000 != 0;
    (p == 0 && !no_credit(0)) || (p == 1 && !no_credit(1))
}

/// The floor Tobitiri assumes when nothing is found: 100 beyond the ring
/// on the gravity axis.
fn fallback(face: usize, pos: V3) -> f32 {
    match face {
        1 => 100.0 + pos[0],
        0 => pos[0] - 100.0,
        3 => 100.0 + pos[1],
        5 => 100.0 + pos[2],
        4 => pos[2] - 100.0,
        _ => pos[1] - 100.0,
    }
}

/// Timers run while play runs, or always for a ring started while paused.
fn running(task: &RingTask, g: &RingGlobals) -> bool {
    task.fwk.start_pause != 0 || g.pause == 0
}

/// The fwk, when allocated (the original writes it unchecked).
fn fwk(task: &mut RingTask) -> Option<&mut RingFwk> {
    (task.fwp != 0).then_some(&mut task.fwk)
}

/// `Ring` (0x196E8): one frame of a RING task.
pub fn ring_exec(
    task: &mut RingTask,
    world: &mut RingWorld,
    g: &RingGlobals,
    host: &mut impl RingHost,
) {
    if host.range_out() {
        if task.twp.mode != 0 {
            if let Some(i) = task.mwp {
                if let Some(l) = world.links.get_mut(i) {
                    l.flag |= 1;
                    l.tp = 0;
                }
            }
            task.mwp = None;
        }
        return;
    }
    let twp = task.twp;
    if let Some(o) = world.object_mut(task.work) {
        o.pos = twp.pos;
        o.ang[1] = twp.ang[1];
        o.shadow_ang = twp.ang;
        o.shadow_posy = if twp.btimer < 50 && twp.scl[1] != NO_SHADOW {
            twp.scl[1]
        } else {
            NO_SHADOW
        };
    }
    match twp.mode {
        0 | 2 => ring_start(task, world, host),
        1 => ring_wait(task, g, host),
        3 => {
            if let Some(i) = task.mwp {
                if let Some(l) = world.links.get_mut(i) {
                    l.flag |= 2;
                    l.tp = 0;
                }
                task.mwp = None;
            }
            host.ring_event(if task.ocp {
                RingEvent::DeadOut
            } else {
                RingEvent::FreeTask
            });
        }
        _ => {}
    }
}

fn ring_start(task: &mut RingTask, world: &mut RingWorld, host: &mut impl RingHost) {
    if task.twp.mode != 2 {
        task.mwp = None;
    }
    if let Some(i) = task.mwp {
        if let Some(l) = world.links.get_mut(i) {
            l.flag &= !1;
            l.tp = task.handle;
        }
    }
    task.fwp = host.calloc(0x14);
    task.fwk = RingFwk::default();
    let twp = &mut task.twp;
    twp.cwp = host.ccl_init_share(6).then_some(Collision::default());
    if let Some(c) = &mut twp.cwp {
        c.flag |= 0x40;
        c.colli_range = 5.0;
    }
    if twp.scl[0] == -1.0 {
        twp.smode = world.colli_timer as i8;
        world.colli_timer = world.colli_timer.wrapping_add(1);
        if world.colli_timer > 8 {
            world.colli_timer = 0;
        }
    } else {
        twp.smode = -1;
    }
    if twp.ang[0] & 0xffff == 1 {
        if task.fwp != 0 {
            task.fwk.flag |= 1;
        }
        task.twp.scl[1] = NO_SHADOW;
    } else {
        let mut ang = twp.ang;
        twp.scl[1] = host.get_shadow_pos(twp.pos, &mut ang);
        twp.ang = ang;
    }
    task.twp.mode = 1;
    if let Some(f) = fwk(task) {
        f.spin = 0.0;
    }
    let twp = &mut task.twp;
    twp.scl[0] = -2.0;
    twp.scl[2] = -4.0;
    twp.id = 17;
    task.dest_ring_end = true;
    task.work = create_ring_object(world, host);
    let twp = task.twp;
    if let Some(o) = world.object_mut(task.work) {
        o.pos = twp.pos;
        o.ang[1] = twp.ang[1];
        o.shadow_ang = twp.ang;
        o.shadow_posy = NO_SHADOW;
    }
}

fn ring_wait(task: &mut RingTask, g: &RingGlobals, host: &mut impl RingHost) {
    let flag = task.twp.cwp.map_or(0, |c| c.flag);
    if flag & 1 != 0 && host.ccl_is_hit_player() {
        task.twp.mode = 3;
        let p = host.is_this_task_player();
        if credited(p, g) {
            host.ring_event(RingEvent::AddScore(10));
            host.ring_event(RingEvent::AddNumRing(p, 1));
            host.ring_event(RingEvent::AddMechHp(p, 0.1));
            host.ring_event(RingEvent::SeCallRing(p as i8));
        }
        o_ring_0(task.twp.pos, host);
    }
    if !o_ring_8(task, g, host) {
        host.ring_event(RingEvent::CollisionEntry(task.twp.pos));
        if running(task, g) {
            task.twp.ang[1] = task.twp.ang[1].wrapping_add(0x16c);
        }
    } else {
        task.twp.scl[1] = NO_SHADOW;
    }
    let probe = task.fwk.flag & 1 == 0;
    let twp = &mut task.twp;
    if twp.scl[1] == NO_SHADOW || !probe || twp.smode < 0 {
        return;
    }
    twp.smode = twp.smode.wrapping_sub(1);
    if twp.smode != -1 {
        return;
    }
    twp.smode = 8;
    let o = g.origin;
    let d = sq_mag(o[0] - twp.pos[0], o[1] - twp.pos[1], o[2] - twp.pos[2]);
    if d < 160_000.0 {
        let mut ang = twp.ang;
        let y = host.get_shadow_pos(twp.pos, &mut ang);
        twp.ang = ang;
        let dy = y - twp.scl[1];
        if dy.abs() < 8.0 {
            twp.scl[1] = y;
            twp.pos[1] += dy;
        } else {
            twp.mode = 3;
            let t = host.create_task(TaskKind::Tobitiri);
            if t != 0 {
                let mut pos = twp.pos;
                pos[1] -= 10.0;
                let ang_y = random_angle(host.rand());
                host.ring_event(RingEvent::SpawnTobitiri {
                    task: t,
                    pos,
                    ang_y,
                });
            }
        }
    }
}

/// `o_ring_8` (0x1AF40): the magnet pull. True when a magnetized player
/// (item bit 2) is the nearer one; then the ring moves toward it (within 50,
/// or once it has started) and takes part in collision.
fn o_ring_8(task: &mut RingTask, g: &RingGlobals, host: &mut impl RingHost) -> bool {
    let mut mask = 0;
    for (i, p) in g.players.iter().enumerate() {
        if p.is_some_and(|p| p.item & 2 != 0) {
            mask |= 1 << i;
        }
    }
    let pos = task.twp.pos;
    let at = |i: usize| g.player_tasks[i].map_or([0.0; 3], |t| t.pos);
    let slot = match mask {
        0 => return false,
        3 => {
            let a = distance(pos, at(0));
            let b = distance(pos, at(1));
            if a < b {
                0
            } else if a > b {
                1
            } else {
                return false;
            }
        }
        1 => 0,
        _ => 1,
    };
    let mut d = distance(pos, at(slot));
    if d < 50.0 || task.twp.wtimer >= 1 {
        let center = g.player_tasks[slot].map_or([0.0; 3], |t| t.center);
        if d > 50.0 {
            d = 50.0;
        }
        // Above 5, else below 0.85 (a NaN stays).
        let mut v = ((1.3 * d) / 50.0).clamp(0.85, 5.0);
        let spd = g.players[slot].map_or([0.0; 3], |p| p.spd);
        v *= 1.0 + 0.5 * scalor(spd);
        task.twp.pos = calc_advance(task.twp.pos, center, v);
        task.twp.wtimer = task.twp.wtimer.wrapping_add(1);
    }
    if let Some(f) = fwk(task) {
        f.spin += 3.0;
    }
    host.ring_event(RingEvent::CollisionEntry(task.twp.pos));
    true
}

/// `Tobitiri` (0x19FF0): one frame of a scattered ring.
pub fn tobitiri_exec(
    task: &mut RingTask,
    world: &mut RingWorld,
    g: &RingGlobals,
    host: &mut impl RingHost,
) {
    if host.range_out() {
        return;
    }
    let twp = task.twp;
    let fwk = task.fwk;
    let floor = task.awp[0];
    if let Some(o) = world.object_mut(task.work) {
        o.pos = twp.pos;
        o.ang[1] = bams(fwk.spin);
        o.shadow_ang[0] = twp.ang[0];
        o.shadow_ang[1] = bams(fwk.spin);
        o.shadow_ang[2] = twp.ang[2];
        if twp.mode != 1 {
            o.shadow_posy = 0.4 + fwk.rest;
            if twp.wtimer & 2 != 0 {
                o.evalflags &= !HIDE;
            } else {
                o.evalflags |= HIDE;
            }
        } else {
            o.shadow_posy = 0.4 + floor;
            if fwk.start_pause == 0 && g.pause != 0 {
                if twp.smode > 46 {
                    o.evalflags &= !HIDE;
                } else {
                    o.evalflags |= HIDE;
                }
            }
        }
    }
    match twp.mode {
        0 => tobitiri_start(task, world, g, host),
        1 => tobitiri_fly(task, g, host),
        2 => tobitiri_rest(task, g, host),
        4 => {
            o_ring_0(task.twp.pos, host);
            host.ring_event(RingEvent::DestroyTask(0));
        }
        3 => host.ring_event(RingEvent::DestroyTask(0)),
        _ => {}
    }
}

fn tobitiri_start(
    task: &mut RingTask,
    world: &mut RingWorld,
    g: &RingGlobals,
    host: &mut impl RingHost,
) {
    task.fwp = host.calloc(0x14);
    if task.fwp == 0 {
        return;
    }
    task.fwk = RingFwk {
        start_pause: i32::from(g.pause),
        ..RingFwk::default()
    };
    task.awp[0] = fallback(o_ring_5(g.gravity), task.twp.pos);
    task.awp[1] = world.colli_timer as f32;
    world.colli_timer = world.colli_timer.wrapping_add(1);
    if world.colli_timer > 8 {
        world.colli_timer = 0;
    }
    let twp = &mut task.twp;
    twp.cwp = host.ccl_init_share(7).then_some(Collision::default());
    if let Some(c) = &mut twp.cwp {
        c.colli_range = 5.0;
    }
    twp.mode = 1;
    for k in 0..3 {
        twp.pos[k] -= 10.0 * g.gravity[k];
    }
    let v = [0.4 * nj_sin(twp.ang[1]), 1.8, 0.4 * nj_cos(twp.ang[1])];
    let m = host.matrix();
    m.push();
    *m.current_mut() = IDENTITY;
    m.rotate(Axis::Z, g.gravity_angles[0]);
    m.rotate(Axis::X, g.gravity_angles[1]);
    twp.scl = m.calc_vector(v);
    m.pop();
    task.fwk.spin = random_angle(host.rand()) as f32;
    if let Some(c) = &mut task.twp.cwp {
        c.flag &= !0x40;
    }
    task.dest_ring_end = true;
    task.work = create_ring_object(world, host);
    let pos = task.twp.pos;
    let spin = bams(task.fwk.spin);
    if let Some(o) = world.object_mut(task.work) {
        o.pos = pos;
        o.ang[1] = spin;
        o.shadow_ang = [0, spin, 0];
        o.shadow_posy = LIFT + pos[1];
    }
}

/// The pickup shared by modes 1 and 2 (after the mode and velocity writes).
fn tobitiri_credit(g: &RingGlobals, host: &mut impl RingHost) {
    let p = host.is_this_task_player();
    if credited(p, g) {
        host.ring_event(RingEvent::AddNumRing(p, 1));
        host.ring_event(RingEvent::SeCallRing(p as i8));
        host.ring_event(RingEvent::AddMechHp(p, 0.1));
    }
}

fn hit(task: &mut RingTask, host: &mut impl RingHost) -> bool {
    let flag = task.twp.cwp.map_or(0, |c| c.flag);
    if flag & 1 != 0 && host.ccl_is_hit_player() {
        if let Some(c) = &mut task.twp.cwp {
            c.flag &= !1;
        }
        return true;
    }
    false
}

fn tobitiri_fly(task: &mut RingTask, g: &RingGlobals, host: &mut impl RingHost) {
    if running(task, g) {
        task.fwk.spin += 5.0;
    }
    if hit(task, host) {
        task.fwk.spin = 0.0;
        let twp = &mut task.twp;
        twp.scl[0] = -2.0;
        twp.scl[2] = -4.0;
        twp.pos[1] += LIFT;
        twp.mode = 4;
        tobitiri_credit(g, host);
        return;
    }
    let gv = g.gravity;
    task.awp[1] -= 1.0;
    if task.awp[1] < 0.0 {
        let face = o_ring_5(gv);
        let p = task.twp.pos;
        let probe = [
            p[0] - LIFT * gv[0],
            p[1] - LIFT * gv[1],
            p[2] - LIFT * gv[2],
        ];
        if let Some(h) = host.get_shadow_pos_xyz(probe, face) {
            task.awp[0] = h.onpos;
            task.twp.ang[0] = h.angx;
            task.twp.ang[2] = h.angz;
        } else {
            task.awp[0] = fallback(face, p);
        }
        task.awp[1] = 8.0;
    }
    if running(task, g) {
        let twp = &mut task.twp;
        twp.wtimer = twp.wtimer.wrapping_add(1);
        if twp.wtimer > 240 {
            twp.mode = 2;
            task.fwk.rest = task.awp[0];
            twp.scl[0] = 0.0;
            twp.scl[2] = 0.0;
            twp.wtimer = 0;
            return;
        }
        let f = task.awp[0];
        let origin = match o_ring_5(gv) {
            0 | 1 => [f, twp.pos[1], twp.pos[2]],
            4 | 5 => [twp.pos[0], twp.pos[1], f],
            _ => [twp.pos[0], f, twp.pos[2]],
        };
        for k in [1, 0, 2] {
            let v = (twp.scl[k] - -0.08 * gv[k]).clamp(-4.0, 4.0);
            twp.scl[k] = v;
            twp.pos[k] += v;
        }
        let damp = f32::from_bits(0x3f7e_b852);
        for s in &mut twp.scl {
            *s *= damp;
        }
        let m = host.matrix();
        m.push();
        *m.current_mut() = IDENTITY;
        rotate(m.current_mut(), Axis::X, twp.ang[0].wrapping_neg());
        rotate(m.current_mut(), Axis::Z, twp.ang[2].wrapping_neg());
        translate(m.current_mut(), [-origin[0], -origin[1], -origin[2]]);
        let mut p = m.calc_point(twp.pos);
        let mut v = m.calc_vector(twp.scl);
        if p[1] < LIFT {
            p[1] = LIFT;
            v[1] = 0.4 * v[1].abs();
            let slow = f32::from_bits(0x3f73_3333);
            v[0] *= slow;
            v[2] *= slow;
            *m.current_mut() = IDENTITY;
            translate(m.current_mut(), origin);
            rotate(m.current_mut(), Axis::Z, twp.ang[2]);
            rotate(m.current_mut(), Axis::X, twp.ang[0]);
            twp.pos = m.calc_point(p);
            twp.scl = m.calc_vector(v);
        }
        m.pop();
    }
    if running(task, g) {
        task.twp.smode = task.twp.smode.wrapping_add(1);
    }
    if task.twp.smode > 46 {
        task.twp.smode = 47;
        let mut at = task.twp.pos;
        at[1] += LIFT;
        host.ring_event(RingEvent::CollisionEntry(at));
    }
}

fn tobitiri_rest(task: &mut RingTask, g: &RingGlobals, host: &mut impl RingHost) {
    if running(task, g) {
        task.fwk.spin += 5.0;
    }
    if hit(task, host) {
        task.twp.mode = 4;
        task.fwk.spin = 0.0;
        task.twp.scl[0] = -2.0;
        task.twp.scl[2] = -4.0;
        tobitiri_credit(g, host);
        return;
    }
    let twp = &task.twp;
    let at = [
        twp.pos[0] + twp.scl[0],
        twp.pos[1] + LIFT,
        twp.pos[2] + twp.scl[2],
    ];
    host.ring_event(RingEvent::CollisionEntry(at));
    if running(task, g) {
        let twp = &mut task.twp;
        twp.wtimer = twp.wtimer.wrapping_add(1);
        if twp.wtimer >= 180 {
            twp.mode = 3;
        }
    }
}

/// `RingEnd` (0x1AECC), the RING and Tobitiri dest: retire the ring object
/// (`o_ring_10` 0x1B2E8: hidden, model cleared, list cleanup requested),
/// unlink from the group entry and free the fwk.
pub fn ring_end(task: &mut RingTask, world: &mut RingWorld, host: &mut impl RingHost) {
    if task.work != 0 {
        if let Some(o) = world.object_mut(task.work) {
            o.evalflags |= HIDE;
            o.model = false;
        }
        world.cleanup = 1;
    }
    if let Some(i) = task.mwp {
        if let Some(l) = world.links.get_mut(i) {
            l.flag &= !1;
            l.tp = 0;
        }
    }
    host.ring_event(RingEvent::Free(task.fwp));
    task.fwp = 0;
    task.mwp = None;
}

/// `CreateRingObject` (0x1B1E0): a ring object from the template, put at
/// the head of the list, starting the model task first if there is none
/// (`CreateRingTask` 0x1B768).
fn create_ring_object(world: &mut RingWorld, host: &mut impl RingHost) -> u32 {
    let addr = host.calloc(0x48);
    if addr == 0 {
        return 0;
    }
    if world.ring_tp == 0 {
        let t = host.create_task(TaskKind::RingModel);
        if t != 0 {
            world.ring_tp = t;
        }
    }
    world.objects.retain(|o| o.addr != addr);
    world.objects.push(RingObject {
        addr,
        evalflags: OBJECT_EVALFLAGS,
        model: true,
        shadow_posy: NO_SHADOW,
        sibling: world.head,
        ..RingObject::default()
    });
    world.head = addr;
    addr
}

/// `RingModelDest` (0x1B318): free the whole list.
pub fn ring_model_dest(world: &mut RingWorld, task: u32, host: &mut impl RingHost) {
    let mut guard = world.objects.len() + 1;
    while world.head != 0 && guard > 0 {
        guard -= 1;
        let head = world.head;
        let next = world.object(head).map_or(0, |o| o.sibling);
        host.ring_event(RingEvent::Free(head));
        world.objects.retain(|o| o.addr != head);
        world.head = next;
    }
    world.cleanup = 0;
    if world.ring_tp == task {
        world.ring_tp = 0;
    }
}

/// `RingModelExec` (0x1B3A4): free retired objects when asked, and end the
/// model task once the list is empty.
pub fn ring_model_exec(world: &mut RingWorld, task: u32, host: &mut impl RingHost) {
    if world.cleanup != 0 {
        let mut cur = world.head;
        let mut prev = 0;
        let mut guard = world.objects.len() + 1;
        while cur != 0 && guard > 0 {
            guard -= 1;
            let (next, model) = world
                .object(cur)
                .map_or((0, true), |o| (o.sibling, o.model));
            if !model {
                host.ring_event(RingEvent::Free(cur));
                world.objects.retain(|o| o.addr != cur);
                if let Some(p) = world.object_mut(prev) {
                    p.sibling = next;
                }
                if world.head == cur {
                    world.head = next;
                }
            } else {
                prev = cur;
            }
            cur = next;
        }
        world.cleanup = 0;
    }
    if world.head == 0 {
        host.ring_event(RingEvent::DestroyTask(task));
    }
}

fn fog_off(g: &RingGlobals, host: &mut impl RingHost) {
    if g.disable_fog != 0 {
        host.ring_event(RingEvent::DisableFog);
        host.ring_event(RingEvent::SetFog);
    }
}

fn fog_on(g: &RingGlobals, host: &mut impl RingHost) {
    if g.disable_fog != 0 {
        host.ring_event(RingEvent::EnableFog);
        host.ring_event(RingEvent::SetFog);
    }
}

/// `RingModelDisp` (0x1B478): draw every shown ring in front of the camera
/// plane (view depth above -10), the nearer models within 65 and 130.
pub fn ring_model_disp(world: &RingWorld, g: &RingGlobals, host: &mut impl RingHost) {
    if g.hide_rings != 0 {
        return;
    }
    fog_off(g, host);
    host.ring_event(RingEvent::SetTexture(data::RING_TEXLIST));
    host.matrix().push();
    for addr in world.list() {
        let Some(o) = world.object(addr) else {
            continue;
        };
        if o.evalflags & HIDE != 0 {
            continue;
        }
        let v = -host.matrix().calc_point(o.pos)[2];
        if v > -10.0 {
            let m = host.matrix();
            translate(m.current_mut(), o.pos);
            rotate(m.current_mut(), Axis::Y, o.ang[1]);
            let model = if v < 65.0 {
                data::MODEL_L1
            } else if v < 130.0 {
                data::MODEL_L2
            } else {
                data::MODEL
            };
            let cur = *m.current();
            host.ring_event(RingEvent::DrawModel(model, cur));
            let m = host.matrix();
            m.pop();
            m.push();
        }
    }
    host.matrix().pop();
    fog_on(g, host);
}

/// `RingModelDisp2` (0x1B5EC): draw each ring's shadow at its shadow height.
pub fn ring_model_disp2(world: &RingWorld, g: &RingGlobals, host: &mut impl RingHost) {
    if g.hide_rings != 0 {
        return;
    }
    fog_off(g, host);
    host.ring_event(RingEvent::SetTexture(data::SHADOW_TEXLIST));
    host.matrix().push();
    for addr in world.list() {
        let Some(o) = world.object(addr) else {
            continue;
        };
        if o.shadow_posy == NO_SHADOW || o.shadow_posy == FAR_SHADOW {
            continue;
        }
        let m = host.matrix();
        translate(m.current_mut(), [o.pos[0], o.shadow_posy, o.pos[2]]);
        rotate(m.current_mut(), Axis::Z, o.shadow_ang[2]);
        rotate(m.current_mut(), Axis::X, o.shadow_ang[0]);
        rotate(m.current_mut(), Axis::Y, o.shadow_ang[1]);
        translate(m.current_mut(), [0.0, 0.5, 0.0]);
        scale(
            m.current_mut(),
            [data::SHADOW_SCALE[0], 1.0, data::SHADOW_SCALE[1]],
        );
        let cur = *m.current();
        host.ring_event(RingEvent::DrawModel(data::SHADOW_MODEL, cur));
        let m = host.matrix();
        m.pop();
        m.push();
    }
    host.matrix().pop();
    fog_on(g, host);
}
