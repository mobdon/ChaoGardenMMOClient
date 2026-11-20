//! Omochao, the hint robot MSGER mode 2 brings in: its exec, its main
//! display and the helpers they call (stg42D.rel section 1, decomp split
//! `stg42D_39.c`).
//!
//! * 0x156CC, the exec ([`exec`]): eight modes (0 idle by its home point,
//!   1 flying back, 2 and 3 knocked, 4 spinning down, 5 greeting, 6 rising,
//!   7 talking), the out-of-range timer, the ride on the player (twp +4
//!   bit 0x8000) with its hint messages, the force query and the
//!   attitude from the velocity;
//! * 0x17468, the main display ([`display`]): the light flash, the shadow
//!   disc, the pose written into the stage's model nodes and the draw of
//!   the body parts by depth;
//! * 0x14DEC ([`follow`]), 0x14AC8 ([`steer`]), 0x1451C ([`limit_turn`]),
//!   0x14608 ([`place_init`]), 0x148A4 ([`place`]), 0x14344
//!   ([`hint_random`]), 0x14484 ([`hint`]), 0x6300 ([`holder`]) and
//!   0x7558 ([`force_query`]).
//!
//! Reused native ports: the trail library (`trail::drop_oldest`, 0x6EE4),
//! the particle creators 0xA67C and the trail effect 0x1DFE0
//! (`effects::create`), the depth clip 0x6F28
//! (as [`clip_depth`]; `messenger::clip` returns only its test), the REL's
//! `sqrtf` 0x74CC (`enemy_lib::engine::sqrtf_rel`), main.dol's `rand`
//! (`prison_visuals::MslRand`), `atan2f` (`msl_math`), `njSin`
//! (`ninja_trig`), `njScalor` / `njDistanceP2P` / fn_80037C84
//! (`enemy_lib::engine`) and the Ninja matrix routines (`ninja_matrix`,
//! `enemy_lib::e_ai::translate`, the table rotations of `land_nodes`).
//!
//! The movable body (task +0x3C) is `dyn_land`'s: the exec runs its
//! update 0x4BE0 (`dyn_land::update`).
//!
//! Left to the host ([`OmochaoHost`], named after their addresses): the
//! message window (fn_800E00E8,
//! fn_800E0118, fn_800E018C), the player queries (fn_80038008,
//! GetPlayerCharacter, CCL_IsHitPlayer, fn_800063F4, fn_80006574) and the
//! rendering calls, which are [`Event`]s with the current matrix.
use super::trail::{self, Trail, TrailHost};
use super::V3;
use crate::enemy_lib::e_ai::translate;
use crate::enemy_lib::engine::{
    distance, fctiwz_f64, nearest_player, scalor, sqrtf_rel, BAMS_PER_RADIAN,
};
use crate::enemy_lib::PlayerView;
use crate::land_nodes::{rotate_pair, rotate_y};
use crate::msl_math::atan2f;
use crate::ninja_matrix::{concat, Axis, MatrixStack, Mtx, IDENTITY};
use crate::ninja_trig::{fctiwz, nj_sin};
use crate::prison_visuals::MslRand;
use crate::stage_objects::dyn_land::{self, Body, Movable, MovableHost};
use crate::stage_objects::effects::{self, Creator, EffectHost, Particle, Spawn};

/// The stage REL data Omochao reads (section 5 offsets; each stage has its
/// own copy).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StageData {
    /// The hint shown for each stage number 3..=0x5F when the player asks
    /// for the stage hint (the jump table at +0x17634; 0 for none).
    pub stage_hints: [u8; 0x5d],
    /// +0x175EC: the largest heading change [`steer`] makes at once.
    pub turn_limit: i32,
    /// +0x17494: the window kind of the default text (also MSGER's mode-1
    /// kind).
    pub kind_default: i32,
    /// +0x17498: the window kind of a hint.
    pub kind_hint: i32,
    /// +0x174A0: the text shown when a hint index has no entry.
    pub hint_default: u32,
    /// +0x175F0: the text shown when the message index has no entry.
    pub message_default: u32,
    /// +0x1749C: the rise target above the position in mode 6.
    pub rise: f32,
    /// +0x174C8: the upward kick when mode 5 ends.
    pub kick: f32,
    /// +0x17378, +0x1737C, +0x17380, +0x17384: propeller steps.
    pub spin: [i32; 4],
    /// +0x17388 (0.1), +0x1738C (0.05): the scale grow and shrink steps.
    pub grow: f32,
    pub shrink: f32,
    /// +0x17390 (9.0): the shake kick, +0x17394 (0.985): its decay.
    pub shake: f32,
    pub shake_decay: f32,
    /// +0x1736C (0.003): the speed added while dashing, +0x17370 (0.1):
    /// the approach below which a dash may start, +0x17374 (0.96): the
    /// speed decay.
    pub dash: f32,
    pub stall: f32,
    pub speed_decay: f32,
    /// The particle descriptor 0xA67C uses (+0xDA40).
    pub particle: u32,
    /// The trail effect 0x1DFE0's descriptor (+0x1EEE8).
    pub trail_effect: u32,
    /// The texlist set before drawing (+0x129B4).
    pub texlist: u32,
    /// The model nodes (NJS_OBJECT) the display poses and draws.
    pub nodes: Nodes,
    /// +0x173EC: the clipped-model table of the alternative body, and
    /// +0x17334 (its +4 word is the head model).
    pub clip_models: u32,
    pub head_models: u32,
}

/// Section 5 offsets of the model nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Nodes {
    /// +0x14A1C: the root (both bodies; the others hang below it).
    pub root: u32,
    /// +0x13894: blinks (flag 8), drawn through 0x6294 (both bodies).
    pub blink: u32,
    /// +0x16E54: drawn nearer than 150; its Z angle is the propeller angle.
    pub near: u32,
    /// +0x16A74: X angle from the head tilt, Y from the roll.
    pub mid: u32,
    /// +0x16094: drawn nearer than 200 while scaled; Y angle and scale.
    pub prop: u32,
    /// +0x15B54, +0x152B4: drawn nearer than 200; swing and roll.
    pub far: [u32; 2],
    /// +0x14E34, +0x156D4: always drawn; swing.
    pub side: [u32; 2],
    /// The alternative body (section 6 +0x690 set): +0x1462C (near),
    /// +0x14324 (mid), +0x13AC0 (scaled), +0x136EC / +0x13038 (nearer
    /// than 200), +0x12CAC / +0x13360 (always).
    pub alt_near: u32,
    pub alt_mid: u32,
    pub alt_prop: u32,
    pub alt_far: [u32; 2],
    pub alt_side: [u32; 2],
}

/// stg42D's copy.
pub const STG42D: StageData = StageData {
    stage_hints: stage_hints(),
    turn_limit: 0x7000,
    kind_default: 1,
    kind_hint: 5,
    hint_default: 0x174a0,
    message_default: 0x175f0,
    rise: 5.0,
    kick: f32::from_bits(0x3e70_a3d7),
    spin: [0x1800, 0x180, 0x111, 0x280],
    grow: f32::from_bits(0x3dcc_cccd),
    shrink: f32::from_bits(0x3d4c_cccd),
    shake: 9.0,
    shake_decay: f32::from_bits(0x3f7c_28f6),
    dash: f32::from_bits(0x3b44_9ba6),
    stall: f32::from_bits(0x3dcc_cccd),
    speed_decay: f32::from_bits(0x3f75_c28f),
    particle: 0xda40,
    trail_effect: 0x1eee8,
    texlist: 0x129b4,
    nodes: Nodes {
        root: 0x14a1c,
        blink: 0x13894,
        near: 0x16e54,
        mid: 0x16a74,
        prop: 0x16094,
        far: [0x15b54, 0x152b4],
        side: [0x14e34, 0x156d4],
        alt_near: 0x1462c,
        alt_mid: 0x14324,
        alt_prop: 0x13ac0,
        alt_far: [0x136ec, 0x13038],
        alt_side: [0x12cac, 0x13360],
    },
    clip_models: 0x173ec,
    head_models: 0x17334,
};

/// The stage-hint jump table of stg42D (+0x17634, cases 3..=0x5F): the
/// hint each case passes to [`hint`].
const fn stage_hints() -> [u8; 0x5d] {
    let mut t = [0u8; 0x5d];
    let rows: [(usize, u8); 24] = [
        (3, 0x5d),
        (4, 0x69),
        (6, 0x6a),
        (8, 0x68),
        (10, 0x5c),
        (13, 0x5a),
        (14, 0x66),
        (16, 0x5b),
        (18, 0x65),
        (22, 0x63),
        (25, 0x61),
        (26, 0x67),
        (28, 0x60),
        (30, 0x64),
        (32, 0x62),
        (34, 0x6d),
        (35, 0x6d),
        (36, 0x6d),
        (37, 0x6d),
        (38, 0x6d),
        (40, 0x6c),
        (44, 0x6b),
        (94, 5),
        (95, 7),
    ];
    let mut i = 0;
    while i < rows.len() {
        t[rows[i].0 - 3] = rows[i].1;
        i += 1;
    }
    t
}

/// Omochao's task work (twp) as its code uses it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Twp {
    /// +0: the mode.
    pub mode: i8,
    /// +1: bit 0 follow the player, bit 1 free flight, bit 2 drawn.
    pub flag1: u8,
    /// +3: the dash timer.
    pub btimer: u8,
    /// +4: bit 2 pushed, bit 0x8000 riding on the player.
    pub flags: i16,
    /// +6: the angle [`place`] turns its offset by.
    pub wtimer: u16,
    /// +8, +0xC, +0x10.
    pub ang: [i32; 3],
    /// +0x14.
    pub pos: V3,
    /// +0x20 (the message index, as a float), +0x24 (no trails at 1.0 or
    /// more), +0x28.
    pub scale: V3,
}

/// The collision child (task +0xC) as the exec touches it.
#[derive(Debug, Clone, PartialEq)]
pub struct Child {
    pub id: u32,
    /// Its twp +4.
    pub flags: i16,
    /// Its twp +0x14.
    pub pos: V3,
    /// Its collision infos' attributes (twp +0x2C -> +0xC, each +4).
    pub attrs: Vec<u32>,
}

/// Omochao's 0xDC-byte work (task +0x40).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Work {
    /// The block's allocation.
    pub id: u32,
    /// +0: the point it flies toward.
    pub goal: V3,
    /// +0xC: the flight speed.
    pub speed: f32,
    /// +0x10: the player point [`place`] last aimed at.
    pub anchor: V3,
    /// +0x1C: the propeller angle.
    pub spin: i32,
    /// +0x20: the hat angle.
    pub w20: i32,
    /// +0x24: the bob angle.
    pub bob: i32,
    /// +0x28: the arm swing angle.
    pub swing: i32,
    /// +0x2C: the head tilt angle.
    pub tilt: i32,
    /// +0x30: the propeller scale.
    pub scale: f32,
    /// +0x34: a mode timer.
    pub timer: i32,
    /// +0x38: the talk timer.
    pub talk: i32,
    /// +0x3C: the message window.
    pub window: u32,
    /// +0x40: the home point.
    pub home: V3,
    /// +0x4C.
    pub w4c: i32,
    /// +0x50: the player it follows.
    pub follow: i32,
    /// +0x54: the player it rides on.
    pub player: i32,
    /// +0x58: the light flash, easing toward +0x5C.
    pub flash: f32,
    pub flash_target: f32,
    /// +0x60: the idle chatter count.
    pub chatter: i32,
    /// +0x64: frames away from home.
    pub away: i32,
    /// +0x68: the shake.
    pub shake: f32,
    /// +0x6C: frames with no message on screen (to 600).
    pub quiet: i32,
    /// +0x70: frames ridden (to 1000).
    pub ridden: i32,
    /// +0x74..+0x80: the propeller trails.
    pub trails: [Option<Trail>; 4],
}

/// An Omochao task.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Omochao {
    pub id: u32,
    /// task +0x4C: 0 from the object list, 2 from the spawner.
    pub mode: i32,
    /// task +0xC.
    pub child: Option<Child>,
    pub twp: Twp,
    /// The mwp's velocity (+0, +4, +8).
    pub vel: V3,
    /// task +0x3C: the movable body (`dyn_land`).
    pub movable: Option<Movable>,
    /// The collision work's flags (twp +0x2C -> +4), None without one.
    pub cwp_flag: Option<u16>,
    pub work: Work,
}

/// The players' task work (playertwp, 0x801E7768) as Omochao reads it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PlayerTwp {
    /// +8, +0xC, +0x10.
    pub ang: [i32; 3],
    /// +0x14.
    pub pos: V3,
}

/// The query buffer 0x7558 fills (+0 flags, +4 vector, +0x10 direction).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Force {
    pub flags: u32,
    pub vector: V3,
    pub direction: V3,
}

/// A call whose result the code does not read.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// SE_Call(id, 0, 0, 0) (0x8006B978).
    Sound(u32),
    /// fn_8006B7EC(id, 0, 0, 0, pos): a positioned sound.
    SoundAt { id: u32, pos: V3 },
    /// SE_CallV2_Timer(id, twp, 1, 0, 0x1E, pos) (0x8006AFFC).
    SoundTimer { id: u32, task: u32, pos: V3 },
    /// fn_800E2828(pos, (0, 0, 0), 6.0, 5.0): the vanish effect.
    Vanish { pos: V3, vel: V3, a: f32, b: f32 },
    /// FreeTask(task) (0x80071AE0).
    FreeTask(u32),
    /// njDisableFog (false) / njEnableFog (true).
    Fog(bool),
    /// gjSetFog (0x8011C310).
    SetFog,
    /// A light setter with its arguments (0x8011F518, 0x8011E154,
    /// 0x8011DF1C with two, 0x8011E150, 0x8011F51C, 0x8011DF18 with three).
    Light { func: u32, args: Vec<f32> },
    /// fn_8011E244(lbl_801B6408): the shadow disc, with the current matrix.
    Disc { matrix: Mtx },
    /// OnControl3D (true) / OffControl3D (false) (0x8002B2E8 / 0x8002B2D8).
    Control3D { on: bool, flags: u32 },
    /// njSetTexture(texlist) (0x80116E18), a section-5 offset.
    SetTexture(u32),
    /// A model drawn with the current matrix: `func` is gjDrawModel
    /// (0x80121BDC), ds_DrawModelClip (0x8011F514), fn_8011E158, or the
    /// module's 0x6294 (`model, -1`).
    Draw { func: u32, model: u32, matrix: Mtx },
    /// fn_80024CB8(2): the light is restored.
    LightReset(u32),
}

/// What Omochao's code asks of the engine, the player code and the module
/// code this port leaves out.
pub trait OmochaoHost: MovableHost + TrailHost + EffectHost + PlaceHost {
    /// lbl_801CC168 +0x11 (a signed byte): the text language.
    fn language(&mut self) -> i8;
    /// lbl_801CC168 +0x42 (a signed byte).
    fn byte_42(&mut self) -> i8;
    /// fn_800E018C: the message window on screen, 0 for none.
    fn fn_800e018c(&mut self) -> u32;
    /// fn_800E00E8(kind, text, time, language, &work +0x3C, 0): returns
    /// what it stores at work +0x3C.
    fn fn_800e00e8(&mut self, kind: i32, text: u32, time: u32, language: i8) -> u32;
    /// fn_800E0118(kind, text, time, 0, &work +0x3C): likewise.
    fn fn_800e0118(&mut self, kind: i32, text: u32, time: u32) -> u32;
    /// Section 5 +0x1747C: the message bank's entry count.
    fn message_count(&mut self) -> i32;
    /// Entry `i` of the message bank (the table at section 6 +0x698).
    fn message_entry(&mut self, i: i32) -> u32;
    /// Section 5 +0x17480: the hint bank's entry count.
    fn hint_count(&mut self) -> i32;
    /// Entry `i` of the hint bank (the table at section 6 +0x69C).
    fn hint_entry(&mut self, i: i32) -> u32;
    /// The word at section 5 +0x1748C + 4n: what player n asked for.
    fn hint_request(&mut self, n: i32) -> i32;
    fn set_hint_request(&mut self, n: i32, v: i32);
    /// _rename_GetStageNum (0x8001DD78).
    fn stage_number(&mut self) -> i32;
    /// `playertwp[n]` (0x801E7768), None when NULL.
    fn player(&mut self, n: i32) -> Option<PlayerTwp>;
    /// `playertp[n]` (0x801E7788).
    fn player_task(&mut self, n: i32) -> u32;
    /// The word at +0x94 of the 0x801E7728 entry n, None when the entry is
    /// NULL (n is 0 or 1).
    fn player_holds(&mut self, n: i32) -> Option<u32>;
    /// CCL_IsHitPlayer(task) (0x800066F4): the player task touching, or 0.
    fn ccl_is_hit_player(&mut self, task: u32) -> u32;
    /// fn_800063F4(task).
    fn fn_800063f4(&mut self, task: u32) -> u32;
    /// fn_80006574(task).
    fn fn_80006574(&mut self, task: u32) -> u32;
    /// The force objects in the list at section 6 +0x628, in order.
    fn force_nodes(&mut self) -> Vec<u32>;
    /// A force object's callback (its +0x38) with the query position.
    fn force_callback(&mut self, node: u32, pos: V3, f: &mut Force);
    /// Section 6 +0x630: the stage's fog switch.
    fn fog_switch(&mut self) -> i32;
    /// Section 6 +0x690: the alternative body is drawn.
    fn alt_body(&mut self) -> i32;
    /// Section 6 +0x694: the alternative body's model, 0 for none.
    fn alt_body_model(&mut self) -> u32;
    /// lbl_801E5870 +0x40..+0x5C: the light the flash starts from.
    fn light(&mut self) -> [f32; 8];
    /// A word of the stage REL's section 5 (the model nodes).
    fn model_word(&mut self, off: u32) -> u32;
    fn set_model_word(&mut self, off: u32, v: u32);
    /// The particle 0x14DEC made, as it leaves it (0xA67C's result with
    /// the colour, +0x14, written), or the one the exec's trail effect
    /// (0x1DFE0) made.
    fn particle_done(&mut self, p: Particle);
    /// A call with no result.
    fn om_event(&mut self, e: Event);
}

// --- small helpers ----------------------------------------------------------

/// 2^-15 (s4 +0x78C).
const RAND_SCALE: f32 = f32::from_bits(0x3800_0000);
/// BAMS per degree (s4 +0x830).
const BAMS: f32 = f32::from_bits(0x4336_0b61);

/// `2^-15 * rand()` (the int-to-float through the 0x43300000 magic double).
fn unit(r: &mut MslRand) -> f32 {
    RAND_SCALE * r.rand() as i32 as f32
}

/// `(s16)fctiwz((s16)x * k)`.
fn scale16(x: i32, k: f32) -> i32 {
    fctiwz((x as i16) as f32 * k) as i16 as i32
}

/// `fctiwz(BAMS * (k * njSin(a)))`.
fn bams_sin(k: f32, a: i32) -> i32 {
    fctiwz(BAMS * (k * nj_sin(a)))
}

/// `fctiwz(65536 / 2pi * atan2f(y, x))` (the double at s4 +0x7C0).
fn bams_atan2(y: f32, x: f32) -> i32 {
    fctiwz_f64(BAMS_PER_RADIAN * atan2f(y, x) as f64)
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// njScale (0x80115178): PSMTXScale, then PSMTXConcat(m, s, m).
fn nj_scale(stack: &mut MatrixStack, v: V3) {
    let s: Mtx = [
        [v[0], 0.0, 0.0, 0.0],
        [0.0, v[1], 0.0, 0.0],
        [0.0, 0.0, v[2], 0.0],
    ];
    *stack.current_mut() = concat(stack.current(), &s);
}

/// fn_8011610C: m = m * S(v), column by column (the same paired-single
/// code `land_nodes` ports privately).
fn scale_ex(stack: &mut MatrixStack, v: V3) {
    for row in stack.current_mut().iter_mut() {
        row[0] *= v[0];
        row[1] *= v[1];
        row[2] *= v[2];
    }
}

/// njRotateEx(ang, 0) (0x80115F68): Z, then Y, then X from Ninja's
/// sine/cosine table, each skipped when its angle is 0 mod 0x10000.
fn rotate_ex(stack: &mut MatrixStack, ang: [i32; 3]) {
    let m = stack.current_mut();
    if ang[2] & 0xffff != 0 {
        rotate_pair(m, ang[2], 0, 1);
    }
    if ang[1] & 0xffff != 0 {
        rotate_y(m, ang[1]);
    }
    if ang[0] & 0xffff != 0 {
        rotate_pair(m, ang[0], 1, 2);
    }
}

/// 0x6F28: the point through the current matrix is within the clip depth
/// (`z <= r`; false for NaN); also returns the depth (`-z`).
pub fn clip_depth(stack: &MatrixStack, pos: V3, r: f32) -> (bool, f32) {
    let z = stack.calc_point(pos)[2];
    (z <= r, -z)
}

// --- helpers ------------------------------------------------------------------

/// 0x14484(i, &work +0x3C): open hint `i` (fn_800E00E8 with the hint
/// kind, 120 frames) or, past the bank, the default text (fn_800E0118).
pub fn hint(i: i32, o: &mut Omochao, data: &StageData, host: &mut impl OmochaoHost) {
    o.work.window = if i < host.hint_count() {
        let text = host.hint_entry(i);
        let language = host.language();
        host.fn_800e00e8(data.kind_hint, text, 0x78, language)
    } else {
        host.fn_800e0118(data.kind_default, data.hint_default, 0x78)
    };
}

/// 0x14344(base, &work +0x3C): hint `base + rand % 4` (from
/// `fctiwz(100 * 2^-15 * rand())`, s4 +0x788), unless another message is
/// on screen.
pub fn hint_random(
    base: i32,
    o: &mut Omochao,
    data: &StageData,
    rand: &mut MslRand,
    host: &mut impl OmochaoHost,
) {
    let x = fctiwz(100.0 * unit(rand));
    let i = base.wrapping_add(x.wrapping_rem(4));
    if host.fn_800e018c() != 0 && o.work.window == 0 {
        return;
    }
    hint(i, o, data, host);
}

/// 0x1451C(task, &saved): the angles move toward their new values from
/// `saved` by a third (X, at most 0x5B0), a third (Y, 0x444) and an eighth
/// (Z, 0x16C) of the difference.
pub fn limit_turn(twp: &mut Twp, saved: [i32; 3]) {
    let step = |cur: i32, from: i32, div: i32, lim: i32| {
        let d = (cur.wrapping_sub(from) as i16 as i32) / div;
        from.wrapping_add((d as i16 as i32).clamp(-lim, lim))
    };
    twp.ang[0] = step(twp.ang[0], saved[0], 3, 0x5b0);
    twp.ang[1] = step(twp.ang[1], saved[1], 3, 0x444);
    twp.ang[2] = step(twp.ang[2], saved[2], 8, 0x16c);
}

/// 0x14608(n, &a, &b): where Omochao waits next to player `n`. With both
/// bones (fn_80038008 nodes 0x12 and 3): `b` is a fifteenth of the bone
/// offset (its y times 0.9; s4 +0x798, +0x79C) and `a` is bone 3 plus 21
/// times `b` (+0x7A0). Otherwise `b` is zero and `a` the player's position
/// (or zero without a player).
pub fn place_init(n: i32, host: &mut impl PlaceHost) -> (V3, V3) {
    let bones = host
        .fn_80038008(n, 0x12)
        .and_then(|p| host.fn_80038008(n, 3).map(|q| (p, q)));
    if let Some((p18, p3)) = bones {
        let k = f32::from_bits(0x3d88_8889);
        let d = sub(p3, p18);
        let mut b = [d[0] * k, d[1] * k, d[2] * k];
        b[1] *= f32::from_bits(0x3f66_6666);
        let a = add([b[0] * 21.0, b[1] * 21.0, b[2] * 21.0], p3);
        return (a, b);
    }
    match host.player_position(n) {
        Some(p) => (p, [0.0; 3]),
        None => ([0.0; 3], [0.0; 3]),
    }
}

/// What Omochao's placement (0x14608, 0x148A4) asks of the player code;
/// MSGER's set-up uses it too.
pub trait PlaceHost {
    /// fn_80038008(n, node, &out, 0): a player bone's position, None when
    /// it returns 0.
    fn fn_80038008(&mut self, n: i32, node: i32) -> Option<V3>;
    /// `playertwp[n]` (0x801E7768) +0x14, None when NULL.
    fn player_position(&mut self, n: i32) -> Option<V3>;
    /// _rename_GetPlayerCharacter(n) (0x80038108).
    fn player_character(&mut self, n: i32) -> i32;
}

/// The fields 0x148A4 writes: work +0 (`goal`), work +0x10 (`anchor`) and
/// twp +6 (`wtimer`); it reads the player index at work +0x50.
pub struct Placement<'a> {
    pub goal: &'a mut V3,
    pub anchor: &'a mut V3,
    pub wtimer: &'a mut u16,
    pub follow: i32,
}

/// 0x148A4 on the fields it touches (see [`place`]).
pub fn place_at(
    p: Placement,
    a: V3,
    stack: &mut MatrixStack,
    rand: &mut MslRand,
    host: &mut impl PlaceHost,
) {
    let c = host.player_character(p.follow);
    let big = c == 7 || host.player_character(p.follow) == 6;
    let (x0, y0) = if big { (25.0, 30.0) } else { (12.0, 13.0) };
    let x = x0 + 5.0 * unit(rand);
    let y = y0 + unit(rand);
    stack.push();
    *stack.current_mut() = IDENTITY;
    translate(stack.current_mut(), a);
    stack.rotate(Axis::Y, *p.wtimer as i32);
    *p.goal = stack.calc_point([x, y, 0.0]);
    stack.pop();
    *p.wtimer = p.wtimer.wrapping_add(0x8800);
    *p.anchor = a;
}

/// 0x148A4(task, a, b): the goal (work +0) becomes `a` plus an offset
/// turned by twp +6, which then advances by 0x8800; `a` is kept at work
/// +0x10. The offset is (12 + 5r, 13 + r, 0), or (25 + 5r, 30 + r, 0) for
/// characters 6 and 7 (s4 +0x7A8..+0x7B8; r = 2^-15 * rand()).
pub fn place(
    o: &mut Omochao,
    a: V3,
    stack: &mut MatrixStack,
    rand: &mut MslRand,
    host: &mut impl OmochaoHost,
) {
    let p = Placement {
        goal: &mut o.work.goal,
        anchor: &mut o.work.anchor,
        wtimer: &mut o.twp.wtimer,
        follow: o.work.follow,
    };
    place_at(p, a, stack, rand, host);
}

/// 0x14AC8: steer the velocity toward the goal. While moving (x² + z² of
/// the velocity above 0.0001, s4 +0x7BC) the heading to the goal is
/// limited to the stage's turn limit from the velocity's and the roll
/// leans by a fifth of the difference (within 0x1555); otherwise the roll
/// decays (0.97). Then `speed` times the unit direction is added.
pub fn steer(o: &mut Omochao, data: &StageData, stack: &mut MatrixStack) {
    let tw = &mut o.twp;
    let v = o.vel;
    let mut d;
    if v[0] * v[0] + v[2] * v[2] > f32::from_bits(0x38d1_b717) {
        d = sub(o.work.goal, tw.pos);
        let a = bams_atan2(d[0], d[2]);
        let b = bams_atan2(v[0], v[2]);
        let mut t = a.wrapping_sub(b) as i16 as i32;
        let lim = data.turn_limit;
        if t > lim || t < lim.wrapping_neg() {
            if t > lim {
                t = lim as i16 as i32;
            }
            if (t as i16 as i32) < lim.wrapping_neg() {
                t = lim.wrapping_neg() as i16 as i32;
            }
            stack.push();
            *stack.current_mut() = IDENTITY;
            let r = b.wrapping_add(t as i16 as i32).wrapping_sub(a);
            stack.rotate(Axis::Y, r);
            d = stack.calc_vector(d);
            stack.pop();
        }
        let z = (t as i16 as i32).wrapping_neg() / 5;
        tw.ang[2] = (z as i16 as i32).clamp(-0x1555, 0x1555);
    } else {
        d = sub(o.work.goal, tw.pos);
        tw.ang[2] = scale16(tw.ang[2], f32::from_bits(0x3f78_51ec));
    }
    let len = scalor(d);
    if len > f32::from_bits(0x38d1_b717) {
        let s = o.work.speed / len;
        let d = [d[0] * s, d[1] * s, d[2] * s];
        o.vel = add(o.vel, d);
    }
}

/// 0x6300: the player (0 or 1) whose +0x94 holds this task, or -1.
pub fn holder(task: u32, host: &mut impl OmochaoHost) -> i32 {
    for n in 0..2 {
        if host.player_holds(n) == Some(task) {
            return n;
        }
    }
    -1
}

/// 0x7558(pos, &out): ask every force object (section 6 +0x628 list)
/// about `pos`; with flag 1 the vector is made a unit (njScalor above
/// 0.0001, s4 +0x2E4; 1.0 at +0x2E8).
pub fn force_query(pos: V3, host: &mut impl OmochaoHost) -> Force {
    let mut f = Force::default();
    for node in host.force_nodes() {
        host.force_callback(node, pos, &mut f);
    }
    if f.flags & 1 != 0 {
        let len = scalor(f.vector);
        if len > f32::from_bits(0x38d1_b717) {
            let s = 1.0 / len;
            f.vector = [f.vector[0] * s, f.vector[1] * s, f.vector[2] * s];
        }
    }
    f
}

/// 0x14DEC: follow the player (twp +1 bit 1). See the module notes on
/// [`exec`]; the sparks are particles of 0xA67C, coloured white or pale.
pub fn follow(
    o: &mut Omochao,
    data: &StageData,
    stack: &mut MatrixStack,
    rand: &mut MslRand,
    host: &mut impl OmochaoHost,
) {
    let (a, b) = place_init(o.work.follow, host);
    if o.twp.flag1 & 2 != 0 {
        if let Some(c) = o.movable.as_mut().map(|b| &mut b.flags) {
            *c |= 0x8000;
        }
        let ahead = add(o.vel, o.twp.pos);
        let near = distance(o.twp.pos, o.work.goal);
        let gain = -distance(ahead, o.work.goal) + near;
        if o.twp.btimer != 0 {
            o.work.speed += data.dash;
            o.twp.btimer = o.twp.btimer.wrapping_sub(1);
        }
        if gain < data.stall && unit(rand) < f32::from_bits(0x3dcc_cccd) && o.twp.btimer == 0 {
            o.twp.btimer = fctiwz(8.0 * unit(rand)).wrapping_add(6) as u8;
        }
        steer(o, data, stack);
        o.work.speed *= data.speed_decay;
        if o.twp.flag1 & 1 != 0 {
            if distance(o.work.anchor, a) > 28.0 {
                place(o, a, stack, rand, host);
            }
            let d = distance(o.work.goal, o.twp.pos);
            if d < 8.0 || (host.frame() as u32).is_multiple_of(200) {
                place(o, a, stack, rand, host);
            } else {
                let k = f32::from_bits(0x3f7d_70a4);
                o.vel = [o.vel[0] * k, o.vel[1] * k, o.vel[2] * k];
                let mut f1 = d;
                if d < 18.0 {
                    f1 = scalor(o.vel);
                    if f1 > f32::from_bits(0x3f4c_cccd) {
                        let k = f32::from_bits(0x3f73_3333);
                        o.vel = [o.vel[0] * k, o.vel[1] * k, o.vel[2] * k];
                        o.twp.btimer = 0;
                    }
                }
                let k = f32::from_bits(0x3f5e_b852);
                let t = [b[0] * k, b[1] * k, b[2] * k];
                if o.twp.btimer == 0 && f1 > 35.0 {
                    o.twp.btimer = 1;
                }
                o.twp.pos = add(o.twp.pos, t);
                if f1 > 110.0 {
                    let u = sub(o.twp.pos, o.work.goal);
                    let s = 110.0 / scalor(u);
                    let u = [u[0] * s, u[1] * s, u[2] * s];
                    o.twp.pos = add(u, o.work.goal);
                }
            }
        }
    } else {
        if let Some(c) = o.movable.as_mut().map(|b| &mut b.flags) {
            *c &= !0x8000;
        }
        o.twp.flag1 &= !1;
    }
    sparks(o, data, rand, host);
}

/// The end of 0x14DEC: three times in ten, while flying fast (njScalor of
/// the velocity above 0.4) with trails, a spark (0xA67C) is made near the
/// position.
// The negated comparisons keep the NaN behaviour (NaN makes no spark).
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn sparks(o: &mut Omochao, data: &StageData, rand: &mut MslRand, host: &mut impl OmochaoHost) {
    let half = 0.5f32;
    let fifth = f32::from_bits(0x3e4c_cccd);
    if !(unit(rand) < f32::from_bits(0x3e99_999a)) {
        return;
    }
    if !(scalor(o.vel) > f32::from_bits(0x3ecc_cccd)) {
        return;
    }
    if o.twp.scale[1] >= 1.0 {
        return;
    }
    let mut p = o.twp.pos;
    let r = unit(rand) - half;
    p[1] += 4.5 + 6.0 * r;
    let r = unit(rand) - half;
    let vx = fifth * o.vel[0] + fifth * r;
    let r = unit(rand) - half;
    let vy = 0.1 * o.vel[1] + (f32::from_bits(0x3e05_1eb8) + fifth * r);
    let r = unit(rand) - half;
    let vz = fifth * o.vel[2] + fifth * r;
    let size = fifth + fifth * unit(rand);
    let spawn = Spawn {
        pos: p,
        vel: [vx, vy, vz],
        size,
        work: 0.0,
    };
    let Some(mut part) = effects::create(Creator::A67c, data.particle, &spawn, rand, host) else {
        return;
    };
    let pick = |rand: &mut MslRand| if unit(rand) > half { 0xffu32 } else { 0xe0 };
    let c1 = pick(rand);
    let c2 = pick(rand);
    let c3 = pick(rand);
    part.color = 0xff00_0000 | c1 << 16 | c2 << 8 | c3;
    host.particle_done(part);
}

// --- exec -----------------------------------------------------------------------

/// twp +4 bit 0x8000: riding on the player.
fn riding(t: &Twp) -> bool {
    t.flags as u16 & 0x8000 != 0
}

/// Pushed (modes 1 and 7): twp +4 bit 2 or fn_80006574 on the task or on
/// its collision child.
fn pushed(o: &Omochao, host: &mut impl OmochaoHost) -> bool {
    if o.twp.flags & 4 != 0 || host.fn_80006574(o.id) != 0 {
        return true;
    }
    match &o.child {
        Some(c) => c.flags & 4 != 0 || host.fn_80006574(c.id) != 0,
        None => false,
    }
}

/// Raised home: 300 above it when spawned (s4 +0x83C), else 15 (+0x820).
fn home_goal(o: &mut Omochao) {
    o.work.goal = o.work.home;
    o.work.goal[1] += if o.mode != 0 { 300.0 } else { 15.0 };
}

fn clear_push(o: &mut Omochao) {
    o.twp.flags &= !4;
    if let Some(c) = o.child.as_mut() {
        c.flags &= !4;
    }
}

fn fly_spin(o: &mut Omochao, data: &StageData) {
    o.work.spin = o.work.spin.wrapping_add(data.spin[3]);
    o.work.w20 = o.work.w20.wrapping_add(data.spin[0]);
}

fn fly_swing(o: &mut Omochao) {
    o.work.bob = o.work.bob.wrapping_add(0x300);
    o.work.swing = o.work.swing.wrapping_add(0x600);
}

fn buzz(o: &Omochao, host: &mut impl OmochaoHost) {
    host.om_event(Event::SoundTimer {
        id: 0x8017,
        task: o.id,
        pos: o.twp.pos,
    });
}

fn vanish(o: &Omochao, host: &mut impl OmochaoHost) {
    host.om_event(Event::Vanish {
        pos: o.twp.pos,
        vel: [0.0; 3],
        a: 6.0,
        b: 5.0,
    });
}

/// Mode 0: idle by the home point. Knocked (fn_800063F4 on the child) it
/// goes to mode 3; with no timer it flies home when more than 20 away (y
/// counted a fifth), else it keeps above home, greets a touching player
/// (mode 5) or turns toward the nearest one (at most 0x27D, a quarter at a
/// time). Riding, the head and arms swing with the frame and the shake.
fn mode_idle(o: &mut Omochao, data: &StageData, rand: &mut MslRand, host: &mut impl OmochaoHost) {
    let fifth = f32::from_bits(0x3e4c_cccd);
    let mut clamped = false;
    o.twp.flag1 &= !1;
    o.twp.flag1 &= !2;
    if let Some(c) = &o.child {
        if host.fn_800063f4(c.id) != 0 {
            o.twp.mode = 3;
            return;
        }
    }
    if o.work.timer == 0 {
        let mut v = sub(o.twp.pos, o.work.home);
        v[1] *= fifth;
        if scalor(v) > 20.0 {
            o.twp.mode = 1;
            o.twp.flag1 &= !1;
            o.twp.flag1 |= 2;
            o.work.goal = o.work.home;
            o.work.goal[1] += 15.0;
        } else {
            let hy = o.work.home[1];
            if hy > o.twp.pos[1] {
                o.twp.pos[1] = hy;
                o.vel[1] = 0.0;
            } else if 0.5 + hy < o.twp.pos[1] && o.vel[1].abs() < f32::from_bits(0x3a83_126f) {
                o.twp.pos[1] -= f32::from_bits(0x3c23_d70a);
            }
            let hit = host.ccl_is_hit_player(o.id);
            let who = match hit {
                0 => 0,
                _ if host.player_task(1) == hit => 2,
                _ => 1,
            };
            if who != 0 && host.fn_800e018c() == 0 && o.work.window == 0 {
                o.twp.mode = 5;
                o.work.follow = who - 1;
                host.om_event(Event::Sound(0x800b));
            } else {
                let view = |p: Option<PlayerTwp>| {
                    p.map(|p| PlayerView {
                        pos: p.pos,
                        center: [0.0; 3],
                    })
                };
                let players = [view(host.player(0)), view(host.player(1))];
                let n = nearest_player(&players, o.twp.pos);
                // (The original reads through a NULL player; here no turn.)
                if let Some(p) = host.player(n as i32) {
                    let d = sub(p.pos, o.twp.pos);
                    let a = bams_atan2(d[0], d[2]);
                    let mut t = a.wrapping_sub(o.twp.ang[1]);
                    if t as i16 > 0x27d {
                        t = 0x27d;
                        clamped = true;
                    }
                    if (t as i16) < -0x27d {
                        t = -0x27d;
                        clamped = true;
                    }
                    o.twp.ang[1] = o.twp.ang[1].wrapping_add(scale16(t, 0.25));
                }
            }
        }
        o.work.scale -= data.shrink;
        if o.work.scale < 0.0 {
            o.work.scale = 0.0;
        }
    }
    o.work.spin = o.work.spin.wrapping_add(data.spin[2]);
    o.work.shake *= data.shake_decay;
    if riding(&o.twp) {
        let k = f32::from_bits(0x3e99_999a);
        o.twp.ang[0] = scale16(o.twp.ang[0], k);
        o.twp.ang[2] = scale16(o.twp.ang[2], k);
        o.work.w20 = o.work.w20.wrapping_add(data.spin[1]);
        let frame = host.frame();
        o.work.bob = bams_sin(15.0, frame.wrapping_shl(11)).wrapping_sub(0x4000);
        o.work.swing = o
            .work
            .swing
            .wrapping_add(fctiwz(BAMS * o.work.shake))
            .wrapping_add(0x800);
        if unit(rand) < f32::from_bits(0x3ba3_d70a) {
            let r = unit(rand) - fifth;
            o.work.shake += data.shake * r;
        }
    } else {
        let k = f32::from_bits(0x3f78_51ec);
        o.twp.ang[0] = scale16(o.twp.ang[0], k);
        o.twp.ang[2] = scale16(o.twp.ang[2], k);
        let k = f32::from_bits(0x3f4c_cccd);
        o.work.bob = scale16(o.work.bob, k);
        if clamped {
            o.work.swing = o.work.swing.wrapping_add(0x380);
        } else {
            o.work.swing = scale16(o.work.swing, k);
        }
    }
    if scalor(o.vel) < fifth && o.twp.scale[1] < 1.0 {
        for k in [0, 2, 1, 3] {
            // (The original does not check for a missing trail.)
            if let Some(t) = o.work.trails[k].as_mut() {
                trail::drop_oldest(t);
            }
        }
    }
}

/// How the mode switch ended.
enum Flow {
    Go,
    Freed,
}

/// The modes (the jump table at section 5 +0x177A8; other values do
/// nothing).
fn modes(
    o: &mut Omochao,
    data: &StageData,
    rand: &mut MslRand,
    host: &mut impl OmochaoHost,
) -> Flow {
    match o.twp.mode {
        0 => mode_idle(o, data, rand, host),
        5 => {
            o.work.spin = o.work.spin.wrapping_add(data.spin[2]);
            let k = f32::from_bits(0x3f78_51ec);
            o.twp.ang[0] = scale16(o.twp.ang[0], k);
            o.twp.ang[2] = scale16(o.twp.ang[2], k);
            o.work.scale += data.grow;
            if o.work.scale > 1.0 {
                o.work.scale = 1.0;
                if host.fn_800e018c() == 0 {
                    o.twp.mode = 6;
                    o.vel[1] += data.kick;
                }
            }
        }
        6 => {
            fly_spin(o, data);
            buzz(o, host);
            if o.twp.flag1 & 2 == 0 {
                o.twp.flag1 |= 2;
                o.work.goal = o.twp.pos;
                o.work.goal[1] += data.rise;
            }
            if (o.work.goal[1] - o.twp.pos[1]).abs() < 3.0 && host.fn_800e018c() == 0 {
                o.twp.mode = 7;
                o.twp.flag1 |= 3;
                o.work.talk = 300;
                clear_push(o);
                let index = |o: &Omochao| fctiwz(o.twp.scale[0].abs());
                let ok = host.message_count() >= 0 && index(o) < host.message_count();
                let i = if ok { index(o) } else { -1 };
                let text = if i >= 0 {
                    host.message_entry(i)
                } else {
                    data.message_default
                };
                let language = host.language();
                o.work.window = host.fn_800e00e8(data.kind_default, text, 600, language);
            }
        }
        7 => {
            fly_spin(o, data);
            fly_swing(o);
            buzz(o, host);
            if o.work.talk > 0 {
                o.work.talk -= 1;
            }
            let p = pushed(o, host);
            if (o.work.talk > 0 || o.work.window != 0) && !p {
                return Flow::Go;
            }
            if p {
                o.twp.mode = 2;
            } else {
                o.twp.mode = 1;
                o.twp.flag1 &= !1;
                home_goal(o);
            }
        }
        1 => {
            fly_spin(o, data);
            fly_swing(o);
            buzz(o, host);
            let dx = o.work.goal[0] - o.twp.pos[0];
            let dz = o.work.goal[2] - o.twp.pos[2];
            if dx * dx + dz * dz < 15.0 && o.work.home[1] < o.twp.pos[1] {
                if o.mode != 0 {
                    host.om_event(Event::FreeTask(o.id));
                    return Flow::Freed;
                }
                o.vel[0] = 0.0;
                o.vel[2] = 0.0;
                o.twp.mode = 0;
                o.work.timer = 0x6c;
            }
            if pushed(o, host) {
                o.twp.mode = 2;
            }
            o.work.scale += data.grow;
            if o.work.scale > 1.0 {
                o.work.scale = 1.0;
            }
        }
        2 | 3 => {
            o.work.timer = if o.twp.mode == 3 { 0xc8 } else { 0x12c };
            o.twp.mode = 4;
            vanish(o, host);
            host.om_event(Event::Sound(0x800c));
            o.work.flash_target = 1.0;
            o.vel = [0.0, f32::from_bits(0x3f4c_cccd), 0.0];
            hint_random(0, o, data, rand, host);
        }
        4 => {
            o.twp.ang[0] = 0xa38e;
            o.twp.ang[2] = 0x4000;
            o.twp.flag1 &= !1;
            o.twp.flag1 &= !2;
            let k = f32::from_bits(0x3f19_999a);
            let to = |a: i32| scale16(0xc000i32.wrapping_sub(a), k).wrapping_add(a);
            o.work.bob = to(o.work.bob);
            o.work.swing = to(o.work.swing);
            if o.work.timer == 0 {
                o.twp.mode = 1;
                o.twp.flag1 &= !1;
                o.twp.flag1 |= 2;
                home_goal(o);
                hint_random(4, o, data, rand, host);
            } else if unit(rand) < f32::from_bits(0x3da3_d70a) {
                // Three draws whose values are not used.
                rand.rand();
                rand.rand();
                rand.rand();
                let spawn = Spawn {
                    pos: o.twp.pos,
                    vel: [0.0, 0.1, 0.0],
                    size: f32::from_bits(0x4053_3333),
                    work: 3.0,
                };
                let t = Creator::X1dfe0;
                if let Some(p) = effects::create(t, data.trail_effect, &spawn, rand, host) {
                    host.particle_done(p);
                }
            }
        }
        _ => {}
    }
    Flow::Go
}

/// After the modes: on the player (twp +4 bit 0x8000) Omochao faces along
/// the holder's heading plus the head tilt (0.18), says hint 8 when first
/// ridden, answers the player's request (section 5 +0x1748C: 1 hints
/// 0x4E.., 3 hints 0x52.., 2 the stage hint) or now and then chatters, and
/// idles with the timer at 0x78. Off it, the hint 0x14 or 0x18 comes when
/// the timer passes 0x6E in mode 0.
fn ride_and_hints(
    o: &mut Omochao,
    data: &StageData,
    rand: &mut MslRand,
    host: &mut impl OmochaoHost,
) {
    if riding(&o.twp) {
        let k = holder(o.id, host);
        if k >= 0 {
            let pa = host.player(k).map_or(0, |p| p.ang[1]);
            o.twp.ang[1] = scale16(o.work.tilt, f32::from_bits(0x3e38_51ec))
                .wrapping_sub(pa)
                .wrapping_add(0x4000);
            o.work.player = k;
        }
        if o.work.ridden == 0 {
            hint_random(8, o, data, rand, host);
            o.work.shake += data.shake;
        } else {
            match host.hint_request(o.work.player) {
                1 => hint_random(0x4e, o, data, rand, host),
                3 => hint_random(0x52, o, data, rand, host),
                2 => {
                    let i = host.stage_number().wrapping_sub(3);
                    if (i as u32) <= 0x5c {
                        let h = data.stage_hints[i as usize];
                        if h != 0 {
                            hint(h as i32, o, data, host);
                        }
                    }
                }
                _ => {
                    if o.work.window == 0
                        && host.fn_800e018c() == 0
                        && o.work.quiet > 150
                        && unit(rand) < f32::from_bits(0x3cf5_c28f)
                    {
                        if o.work.chatter <= 1 {
                            hint_random(0xc, o, data, rand, host);
                            o.work.chatter += 1;
                        } else if o.work.chatter >= 3 {
                            hint_random(0x6e, o, data, rand, host);
                            o.work.chatter = 0;
                        } else {
                            hint_random(0x10, o, data, rand, host);
                            o.work.chatter += 1;
                        }
                        o.work.shake += data.shake;
                    }
                }
            }
        }
        host.set_hint_request(o.work.player, 0);
        o.work.timer = 0x78;
        o.twp.mode = 0;
        let frame = host.frame();
        o.work.tilt = bams_sin(46.0, frame.wrapping_mul(0x380));
        if o.work.ridden < 1000 {
            o.work.ridden += 1;
        }
    } else {
        o.work.ridden = 0;
        o.work.chatter = 0;
        o.work.tilt = scale16(o.work.tilt, f32::from_bits(0x3f4c_cccd));
        if o.work.timer == 0x6e && o.twp.mode == 0 && host.byte_42() == 0 {
            let base = if scalor(o.vel) < 1.0 { 0x14 } else { 0x18 };
            hint_random(base, o, data, rand, host);
        }
    }
}

/// The unpaused part of the exec; false when the task freed itself.
fn update(
    o: &mut Omochao,
    data: &StageData,
    stack: &mut MatrixStack,
    rand: &mut MslRand,
    host: &mut impl OmochaoHost,
) -> bool {
    if host.fn_800e018c() == 0 {
        if o.work.quiet < 600 {
            o.work.quiet += 1;
        }
    } else {
        o.work.quiet = 0;
    }
    let d = distance(o.work.home, o.twp.pos);
    if o.twp.flag1 & 4 == 0 {
        if !riding(&o.twp) && d > 400.0 {
            o.work.away = o.work.away.wrapping_add(3);
        } else {
            o.work.away = 0;
        }
    } else if !riding(&o.twp) && d > 100.0 {
        o.work.away = o.work.away.wrapping_add(2);
    } else if o.twp.mode == 1 {
        o.work.away = o.work.away.wrapping_add(1);
    } else {
        o.work.away = 0;
    }
    if o.work.away > 3000 {
        o.twp.ang[1] = o.twp.ang[1].wrapping_add(0x444);
        o.twp.pos[1] += 5.0;
        if o.work.away > 6000 {
            host.om_event(Event::SoundAt {
                id: 0x800c,
                pos: o.twp.pos,
            });
            vanish(o, host);
            host.om_event(Event::FreeTask(o.id));
            return false;
        }
    }
    o.twp.flag1 &= !4;
    if o.twp.mode != 0 {
        let dx = o.work.home[0] - o.twp.pos[0];
        let dz = o.work.home[2] - o.twp.pos[2];
        let hy = o.work.home[1];
        if dx * dx + dz * dz < 400.0 && hy > o.twp.pos[1] && hy < 10.0 + o.twp.pos[1] {
            o.twp.pos[1] = hy;
        }
    }
    if let Flow::Freed = modes(o, data, rand, host) {
        return false;
    }
    ride_and_hints(o, data, rand, host);
    if o.work.timer != 0 {
        clear_push(o);
        o.twp.flag1 &= !3;
        o.work.timer -= 1;
    }
    o.work.flash += 0.5 * (o.work.flash_target - o.work.flash);
    o.work.flash_target *= f32::from_bits(0x3f7d_70a4);
    follow(o, data, stack, rand, host);
    true
}

/// 0x156CC: Omochao's exec. From the object list (task +0x4C 0) it ends
/// out of range (CheckRangeOut). Unless the game is paused (lbl_801CC168
/// +0x37) it counts frames with no message on screen and away from home
/// (400 undrawn, 100 drawn, s4 +0x814, +0x788; past 3000 it spins and
/// rises, past 6000 it vanishes and frees itself), runs its mode, rides on
/// the player with its hints, eases the flash and follows (0x14DEC); then
/// the force objects (0x7558) pull its velocity (flag 2) or remove it
/// (flag 8, unless talking). Then the dynamic-land update (0x4BE0) and, in
/// free flight, the attitude from the velocity (pitch within 0x2000),
/// limited by 0x1451C; the collision child follows and is solid (its
/// infos' attribute 0x10) unless Omochao moves on its own (x^2 + z^2 of
/// the velocity at least 0.8, s4 +0x7E0).
pub fn exec(
    o: &mut Omochao,
    data: &StageData,
    stack: &mut MatrixStack,
    rand: &mut MslRand,
    host: &mut impl OmochaoHost,
) {
    if o.mode == 0 && host.range_out() {
        return;
    }
    let saved = o.twp.ang;
    if host.trail_reset() == 0 {
        if !update(o, data, stack, rand, host) {
            return;
        }
        let f = force_query(o.twp.pos, host);
        if f.flags & 2 != 0 {
            let k = f32::from_bits(0x3e4c_cccd);
            for i in 0..3 {
                o.vel[i] += k * (f.direction[i] - o.vel[i]);
            }
        }
        if f.flags & 8 != 0 && o.twp.mode != 7 {
            host.om_event(Event::FreeTask(o.id));
            return;
        }
    }
    // 0x4BE0. (The original reads through a missing body.)
    if let Some(st) = o.movable.as_mut() {
        let b = Body {
            flag: o.twp.flags,
            ang: &mut o.twp.ang,
            pos: &mut o.twp.pos,
            speed: &mut o.vel,
            cwp_flag: o.cwp_flag,
        };
        dyn_land::update(b, st, host);
    }
    if host.trail_reset() == 0 && o.twp.flag1 & 2 != 0 {
        if scalor(o.vel) > f32::from_bits(0x3c23_d70a) {
            let v = o.vel;
            let h = sqrtf_rel(v[0] * v[0] + v[2] * v[2]);
            let pitch = bams_atan2(v[1], h).wrapping_neg();
            o.twp.ang[1] = bams_atan2(v[0], v[2]);
            o.twp.ang[0] = (pitch as i16 as i32).clamp(-0x2000, 0x2000);
        }
        limit_turn(&mut o.twp, saved);
    }
    // NaN counts as moving (the compare branches to the clearing side).
    let v = o.vel;
    let still = riding(&o.twp) || v[0] * v[0] + v[2] * v[2] < 0.8;
    if let Some(c) = o.child.as_mut() {
        c.pos = o.twp.pos;
        for a in c.attrs.iter_mut() {
            if still {
                *a |= 0x10;
            } else {
                *a &= !0x10;
            }
        }
        if !still {
            c.flags &= !4;
        }
    }
}

// --- display --------------------------------------------------------------------

/// gjDrawModel (0x80121BDC), ds_DrawModelClip (0x8011F514), fn_8011E158
/// and the module's 0x6294 (a section-1 offset), as [`Event::Draw`] names
/// them.
pub const GJ_DRAW_MODEL: u32 = 0x8012_1bdc;
pub const DRAW_MODEL_CLIP: u32 = 0x8011_f514;
pub const FN_8011E158: u32 = 0x8011_e158;
pub const DRAW_6294: u32 = 0x6294;

fn node_vec(n: u32, at: u32, host: &mut impl OmochaoHost) -> V3 {
    [0, 1, 2].map(|k| f32::from_bits(host.model_word(n + at + 4 * k)))
}

/// njTranslateEx(&node +8), njRotateEx(&node +0x14, 0).
fn node_frame(n: u32, stack: &mut MatrixStack, host: &mut impl OmochaoHost) {
    let p = node_vec(n, 8, host);
    translate(stack.current_mut(), p);
    let a = [0, 1, 2].map(|k| host.model_word(n + 0x14 + 4 * k) as i32);
    rotate_ex(stack, a);
}

fn draw(func: u32, model: u32, stack: &MatrixStack, host: &mut impl OmochaoHost) {
    host.om_event(Event::Draw {
        func,
        model,
        matrix: *stack.current(),
    });
}

/// node frame, then draw the node's own model (+4) with `func`.
fn draw_node(n: u32, func: u32, stack: &mut MatrixStack, host: &mut impl OmochaoHost) {
    node_frame(n, stack, host);
    let m = host.model_word(n + 4);
    draw(func, m, stack, host);
}

/// node frame, then draw a word of the clipped-model table.
fn draw_clip(n: u32, table: u32, stack: &mut MatrixStack, host: &mut impl OmochaoHost) {
    node_frame(n, stack, host);
    let m = host.model_word(table);
    draw(DRAW_MODEL_CLIP, m, stack, host);
}

fn set_flags(n: u32, clear: u32, set: u32, host: &mut impl OmochaoHost) {
    let v = host.model_word(n);
    host.set_model_word(n, (v & !clear) | set);
}

/// The pose 0x17468 writes into the model nodes (flags +0, angles +0x14,
/// scale +0x20).
fn pose(o: &Omochao, data: &StageData, host: &mut impl OmochaoHost) {
    let n = &data.nodes;
    let w = &o.work;
    let mode = o.twp.mode;
    let frame = host.frame() as u32;
    let blink = mode == 1 || mode == 4 || ((mode as u8).wrapping_sub(5) <= 1 && frame % 20 < 10);
    if blink {
        set_flags(n.blink, 0, 8, host);
    } else {
        set_flags(n.blink, 8, 0, host);
    }
    set_flags(n.prop, 6, 0, host);
    host.set_model_word(n.prop + 0x18, w.w20 as u32);
    for k in 0..3 {
        host.set_model_word(n.prop + 0x20 + 4 * k, w.scale.to_bits());
    }
    set_flags(n.near, 2, 0, host);
    host.set_model_word(n.near + 0x1c, w.spin as u32);
    let swing = w.swing;
    let pitch = scale16(o.twp.ang[0], f32::from_bits(0x3ecc_cccd));
    let roll = scale16(o.twp.ang[2], f32::from_bits(0x3f19_999a));
    let roll_u = roll & 0xffff;
    let turn = scale16(roll_u, 3.0);
    let tilt = scale16(w.tilt, f32::from_bits(0x3e4c_cccd));
    let np = pitch.wrapping_neg();
    let [fa, fb] = n.far;
    set_flags(fb, 2, 0, host);
    let x = np
        .wrapping_add(bams_sin(30.0, swing.wrapping_sub(0x400)))
        .wrapping_neg();
    host.set_model_word(fb + 0x14, x as u32);
    let z = if roll_u < 0x7000 {
        roll.wrapping_neg()
    } else {
        0
    };
    host.set_model_word(fb + 0x1c, z as u32);
    set_flags(fa, 2, 0, host);
    let x = np.wrapping_add(bams_sin(30.0, swing.wrapping_add(0x400)));
    host.set_model_word(fa + 0x14, x as u32);
    let z = if roll_u > 0x7000 {
        roll.wrapping_neg()
    } else {
        0
    };
    host.set_model_word(fa + 0x1c, z as u32);
    let [sa, sb] = n.side;
    set_flags(sa, 2, 0, host);
    let x = np.wrapping_add(bams_sin(20.0, swing.wrapping_sub(0x800)));
    host.set_model_word(sa + 0x14, x as u32);
    set_flags(sb, 2, 0, host);
    let x = np
        .wrapping_add(bams_sin(20.0, swing.wrapping_add(0x600)))
        .wrapping_neg();
    host.set_model_word(sb + 0x14, x as u32);
    set_flags(n.mid, 2, 0, host);
    host.set_model_word(n.mid + 0x14, tilt as u32);
    host.set_model_word(n.mid + 0x18, turn.wrapping_add(w.tilt) as u32);
}

/// The blinking node: unless hidden (flag 8) its position, then the
/// alternative body's own model (section 6 +0x694, clipped) or 0x6294
/// with the node's model.
fn draw_blink(n: u32, alt: bool, stack: &mut MatrixStack, host: &mut impl OmochaoHost) {
    if host.model_word(n) & 8 != 0 {
        return;
    }
    let p = node_vec(n, 8, host);
    translate(stack.current_mut(), p);
    let m = if alt { host.alt_body_model() } else { 0 };
    if m != 0 {
        draw(DRAW_MODEL_CLIP, m, stack, host);
    } else {
        let model = host.model_word(n + 4);
        draw(DRAW_6294, model, stack, host);
    }
}

/// 0x17468: Omochao's main display. Within the clip depth (100.0, s4
/// +0x788) it marks twp +1 bit 2; with the stage's fog switch the fog is
/// off meanwhile. A flash (work +0x58 above 0.01) brightens the light
/// (lbl_801E5870 +0x40: its sixth and seventh values times 1 - 0.9 flash,
/// the fourth toward 1 by 0.6 flash). The shadow disc is drawn under it
/// (lifted 0.1, scaled 3, 1, 3); the pose is written into the model nodes
/// (blinking in modes 1 and 4 and half the time in 5 and 6); then the body
/// is drawn in Omochao's frame (bobbing by 2.3 njSin(work +0x24), +0x858),
/// its parts by depth (300, 150, 200; s4 +0x83C, +0x85C, +0x860).
pub fn display(
    o: &mut Omochao,
    data: &StageData,
    stack: &mut MatrixStack,
    host: &mut impl OmochaoHost,
) {
    let (inside, depth) = clip_depth(stack, o.twp.pos, 100.0);
    if !inside {
        return;
    }
    o.twp.flag1 |= 4;
    let bob = f32::from_bits(0x4013_3333) * nj_sin(o.work.bob);
    let fog = host.fog_switch() != 0;
    if fog {
        host.om_event(Event::Fog(false));
        host.om_event(Event::SetFog);
    }
    let lit = o.work.flash > 0.01;
    if lit {
        let mut l = host.light();
        let f = o.work.flash;
        let k = f32::from_bits(0x3f66_6666);
        l[6] *= 1.0 - k * f;
        l[7] *= 1.0 - k * f;
        l[4] += f32::from_bits(0x3f19_999a) * ((1.0 - l[4]) * f);
        for func in [0x8011_f518, 0x8011_e154, 0x8011_df1c] {
            host.om_event(Event::Light {
                func,
                args: vec![l[3], l[4]],
            });
        }
        for func in [0x8011_e150, 0x8011_f51c, 0x8011_df18] {
            host.om_event(Event::Light {
                func,
                args: vec![l[5], l[6], l[7]],
            });
        }
    }
    let t = o.twp;
    stack.push();
    translate(
        stack.current_mut(),
        [t.pos[0], 0.1 + (t.pos[1] + bob), t.pos[2]],
    );
    nj_scale(stack, [3.0, 1.0, 3.0]);
    host.om_event(Event::Disc {
        matrix: *stack.current(),
    });
    stack.pop();
    pose(o, data, host);
    host.om_event(Event::Control3D {
        on: true,
        flags: 0x2400,
    });
    host.om_event(Event::SetTexture(data.texlist));
    stack.push();
    if riding(&t) {
        translate(stack.current_mut(), t.pos);
        // (The original reads through a NULL player; here no turn.)
        let p = host.player(o.work.player).unwrap_or_default();
        stack.rotate(Axis::Z, p.ang[2]);
        stack.rotate(Axis::X, p.ang[0]);
        translate(stack.current_mut(), [0.0, bob, 0.0]);
    } else {
        translate(stack.current_mut(), [t.pos[0], t.pos[1] + bob, t.pos[2]]);
    }
    stack.rotate(Axis::Y, t.ang[1]);
    stack.rotate(Axis::X, t.ang[0]);
    stack.rotate(Axis::Z, t.ang[2]);
    let n = data.nodes;
    let scaled = o.work.scale > 0.01;
    let alt = host.alt_body() != 0;
    let table = |k: u32| data.clip_models + k;
    node_frame(n.root, stack, host);
    if depth < 300.0 {
        let m = host.model_word(if alt {
            table(0xc)
        } else {
            data.head_models + 4
        });
        draw(
            if alt { DRAW_MODEL_CLIP } else { GJ_DRAW_MODEL },
            m,
            stack,
            host,
        );
    }
    stack.push();
    if depth < 150.0 {
        if alt {
            draw_clip(n.alt_near, table(0x3c), stack, host);
        } else {
            draw_node(n.near, GJ_DRAW_MODEL, stack, host);
        }
        stack.pop();
        stack.push();
    }
    if alt {
        draw_node(n.alt_mid, FN_8011E158, stack, host);
    } else {
        draw_node(n.mid, GJ_DRAW_MODEL, stack, host);
    }
    stack.push();
    if depth < 200.0 && scaled {
        let p = if alt { n.alt_prop } else { n.prop };
        node_frame(p, stack, host);
        let s = node_vec(p, 0x20, host);
        scale_ex(stack, s);
        let m = host.model_word(if alt { table(0x1c) } else { p + 4 });
        draw(
            if alt { DRAW_MODEL_CLIP } else { GJ_DRAW_MODEL },
            m,
            stack,
            host,
        );
        stack.pop();
        stack.push();
    }
    draw_blink(n.blink, alt, stack, host);
    stack.pop();
    stack.pop();
    stack.push();
    let mut parts: Vec<(u32, u32)> = Vec::new();
    if alt {
        if depth < 200.0 {
            parts.extend([(n.alt_far[0], 0x6c), (n.alt_far[1], 0x5c)]);
        }
        parts.extend([(n.alt_side[0], 0x8c), (n.alt_side[1], 0x7c)]);
    } else {
        if depth < 200.0 {
            parts.extend(n.far.map(|p| (p, 0)));
        }
        parts.extend(n.side.map(|p| (p, 0)));
    }
    for (p, k) in parts {
        if alt {
            draw_clip(p, table(k), stack, host);
        } else {
            draw_node(p, GJ_DRAW_MODEL, stack, host);
        }
        stack.pop();
        stack.push();
    }
    // njPopMatrix(2).
    stack.pop();
    stack.pop();
    host.om_event(Event::Control3D {
        on: false,
        flags: 0x2400,
    });
    if lit {
        host.om_event(Event::LightReset(2));
    }
    if fog {
        host.om_event(Event::Fog(true));
        host.om_event(Event::SetFog);
    }
}
