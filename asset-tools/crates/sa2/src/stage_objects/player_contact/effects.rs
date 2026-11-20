//! The barrier, magnetic-barrier and invincibility effect tasks of main.dol
//! (GSNE8P): the creators the item effects call (fn_800BE408, fn_800BE374,
//! fn_800BE308), the execs and displays they install, and the execs and
//! displays of the sparkle tasks those execs spawn.
//!
//! Task creation (CreateElementalTask, CreateChildTask), FreeTask and SE_Call
//! go to [`ContactHost`]; the Ninja drawing calls of the displays go to
//! [`EffectDraw`], one method per main.dol routine. `rand` (0x8010DC88) is
//! ported here on its seed word in [`Ram`], so the random angles come out as
//! the game's.
//!
//! Not ported: fn_800BDD88, the magnetic sparkles' display. It is drawing
//! only (it fills the 4-vertex sprite buffer at 0x801C9EA0 + 0xF5C it
//! then draws with njDrawTexture3DEx); the exec here installs its address.
use super::{or16, slot, ContactHost, Ram, GAME, PLAYER_PWP, PLAYER_TWP};

/// r13 - 0x7C20 (0x803AD6A0): MSL `rand`'s seed.
pub const RAND_SEED: u32 = 0x803a_d6a0;

/// rand() (0x8010DC88): `seed = seed * 1103515245 + 12345`, returns bits
/// 16..30 of the new seed.
pub fn rand(ram: &mut impl Ram) -> u32 {
    let s = ram
        .r32(RAND_SEED)
        .wrapping_mul(0x41c6_4e6d)
        .wrapping_add(0x3039);
    ram.w32(RAND_SEED, s);
    (ram.r32(RAND_SEED) >> 16) & 0x7fff
}

// --- addresses --------------------------------------------------------------

/// fn_800BE308's task: exec, display, destructor and name ("Muteki",
/// .sdata2 0x803AFEEC).
pub const INVINCIBLE_EXEC: u32 = 0x800b_d2f0;
pub const INVINCIBLE_DISP: u32 = 0x800b_d478;
/// fn_800BD5BC: an empty destructor (a `blr`), shared with the barrier.
pub const EMPTY_DEST: u32 = 0x800b_d5bc;
pub const INVINCIBLE_NAME: u32 = 0x803a_feec;
/// The invincibility sparkles (CreateChildTask): exec and display.
pub const INVINCIBLE_SPARK_EXEC: u32 = 0x800b_d2b4;
pub const INVINCIBLE_SPARK_DISP: u32 = 0x800b_d120;

/// fn_800BE374's task: exec, display, destructor (fn_800BDD84, a `blr`) and
/// name ("TBarrier", .rodata 0x8017525C).
pub const MAGNET_EXEC: u32 = 0x800b_e0bc;
pub const MAGNET_DISP: u32 = 0x800b_e234;
pub const MAGNET_DEST: u32 = 0x800b_dd84;
pub const MAGNET_NAME: u32 = 0x8017_525c;
/// The magnetic sparkles: exec and display (fn_800BDD88, not ported).
pub const MAGNET_SPARK_EXEC: u32 = 0x800b_dfbc;
pub const MAGNET_SPARK_DISP: u32 = 0x800b_dd88;

/// fn_800BE408's task: exec, display, destructor ([`EMPTY_DEST`]) and name
/// ("NBarrier", .rodata 0x80175268).
pub const BARRIER_EXEC: u32 = 0x800b_d6e8;
pub const BARRIER_DISP: u32 = 0x800b_d858;
pub const BARRIER_NAME: u32 = 0x8017_5268;
/// The barrier sparkles: exec and display.
pub const BARRIER_SPARK_EXEC: u32 = 0x800b_d6ac;
pub const BARRIER_SPARK_DISP: u32 = 0x800b_d5c0;

/// The barrier sound (`SE_Call(0x801A, 0, 0, 0)`).
pub const SE_BARRIER: u32 = 0x801a;

/// The textures and models the displays draw (.data): the invincibility's
/// texture list (r13 - 0x7D88, 0x803AD538) and model (0x801CABAC), the
/// sparkles' texture lists (0x801CADBC, 8 bytes a frame; 0x801CAD3C, 4
/// bytes a frame) and models (0x801CAC84, 0x801CA194).
pub const INVINCIBLE_TEXLIST: u32 = 0x803a_d538;
pub const INVINCIBLE_MODEL: u32 = 0x801c_abac;
pub const INVINCIBLE_SPARK_TEX: u32 = 0x801c_adbc;
pub const INVINCIBLE_SPARK_MODEL: u32 = 0x801c_ac84;
pub const BARRIER_SPARK_TEX: u32 = 0x801c_ad3c;
pub const BARRIER_SPARK_MODEL: u32 = 0x801c_a194;

// --- .sdata2 constants -------------------------------------------------------

/// 0x803AFEB0: 1/32768 (a `rand` value to 0..1).
const RAND_SCALE: f32 = 1.0 / 32768.0;
/// 0x803AFEAC: 65536.0 (0..1 to an angle).
const ANGLE: f32 = 65536.0;
/// 0x803AFEC0: the chance bar of a barrier sparkle (0.8).
const SPARK_CHANCE: f32 = 0.8;
/// 0x803AFEC4: 1.0, a magnetic sparkle's start size.
const ONE: f32 = 1.0;
/// 0x803AFECC: 0.0.
const ZERO: f32 = 0.0;
/// 0x803AFEC8: 0.5.
const HALF: f32 = 0.5;
/// 0x803AFED4: a magnetic sparkle's shrink per frame (0.025).
const SHRINK: f32 = 0.025;
/// 0x803AFED8 / 0x803AFEE8: a magnetic sparkle's frame and texture ranges.
const SPARK_FRAMES: f32 = 16.0;
const SPARK_TEXTURES: f32 = 3.0;
/// 0x803AFEA0 / 0x803AFEA4: the effect size against the player's scale,
/// for mechs (13.0) and the others (6.0).
const SIZE_MECH: f32 = 13.0;
const SIZE: f32 = 6.0;
/// 0x803AFEA8: 0.2.
const SIZE_SCALE: f32 = 0.2;

/// The invincibility's length: it ends after its counter passes 0x4EC
/// (1260 frames).
pub const INVINCIBLE_FRAMES: u32 = 0x4ec;

/// `fctiwz`: toward zero, saturating, NaN to 0x80000000.
fn fctiwz(x: f32) -> i32 {
    if x.is_nan() {
        i32::MIN
    } else {
        x as i32
    }
}

/// `scale * (RAND_SCALE * (float)rand())`, `fctiwz`.
fn rand_times(ram: &mut impl Ram, scale: f32) -> i32 {
    let r = rand(ram) as f32;
    fctiwz(scale * (RAND_SCALE * r))
}

// --- creators ---------------------------------------------------------------

/// The shared tail of the creators: the task's twp +1 gets the player.
fn set_player(ram: &mut impl Ram, task: u32, player: i32) {
    let twp = ram.r32(task + 0x34);
    ram.w8(twp + 1, player as u8);
}

/// fn_800BE308(player) (0x800BE308): the invincibility task
/// (CreateElementalTask(2, 3, fn_800BD2F0, "Muteki")) for the player, with
/// display fn_800BD478 and the empty destructor. Returns the task, or 0.
pub fn fn_800be308(ram: &mut impl Ram, host: &mut impl ContactHost, player: i32) -> u32 {
    let task = host.create_elemental_task(2, 3, INVINCIBLE_EXEC, INVINCIBLE_NAME);
    if task != 0 {
        set_player(ram, task, player);
        ram.w32(task + 0x18, EMPTY_DEST);
        ram.w32(task + 0x1c, INVINCIBLE_DISP);
    }
    task
}

/// fn_800BE374(player) (0x800BE374): the magnetic barrier task
/// (CreateElementalTask(2, 3, fn_800BE0BC, "TBarrier")), the barrier sound,
/// display fn_800BE234, destructor fn_800BDD84. Returns the task, or 0.
pub fn fn_800be374(ram: &mut impl Ram, host: &mut impl ContactHost, player: i32) -> u32 {
    let task = host.create_elemental_task(2, 3, MAGNET_EXEC, MAGNET_NAME);
    if task != 0 {
        set_player(ram, task, player);
        host.se_call(SE_BARRIER);
        ram.w32(task + 0x1c, MAGNET_DISP);
        ram.w32(task + 0x18, MAGNET_DEST);
    }
    task
}

/// fn_800BE408(player) (0x800BE408): the barrier task
/// (CreateElementalTask(2, 3, fn_800BD6E8, "NBarrier")), the barrier sound,
/// display fn_800BD858 and the empty destructor. Returns the task, or 0.
pub fn fn_800be408(ram: &mut impl Ram, host: &mut impl ContactHost, player: i32) -> u32 {
    let task = host.create_elemental_task(2, 3, BARRIER_EXEC, BARRIER_NAME);
    if task != 0 {
        set_player(ram, task, player);
        host.se_call(SE_BARRIER);
        ram.w32(task + 0x1c, BARRIER_DISP);
        ram.w32(task + 0x18, EMPTY_DEST);
    }
    task
}

// --- execs --------------------------------------------------------------------

/// The task's twp and the player tables' twp and pwp of its player (twp +1).
fn owner(ram: &mut impl Ram, task: u32) -> (u32, u32, u32) {
    let twp = ram.r32(task + 0x34);
    let n = ram.rs8(twp + 1);
    let ptwp = ram.r32(slot(PLAYER_TWP, n));
    let pwp = ram.r32(slot(PLAYER_PWP, n));
    (twp, ptwp, pwp)
}

/// fn_800BD2F0(task) (0x800BD2F0), the invincibility's exec. It ends
/// (pwp +0x10 loses 0x8000, FreeTask) when the player's twp is gone, its
/// frame counter (twp +6, counted on every frame the player is there) was
/// past 0x4EC, or GAME +0x42 is 1. Otherwise the flag keeps 0x8000 and one
/// sparkle (two on odd frames) is spawned: CreateChildTask(2, fn_800BD2B4)
/// with random X and Y angles (twp +8, +0xC; `rand` scaled to 0..0xFFFE),
/// the player number and display fn_800BD120.
pub fn fn_800bd2f0(ram: &mut impl Ram, host: &mut impl ContactHost, task: u32) {
    let (twp, ptwp, pwp) = owner(ram, task);
    let mut end = ptwp == 0;
    if !end {
        let t = ram.r16(twp + 6);
        ram.w16(twp + 6, t.wrapping_add(1));
        end = u32::from(t) > INVINCIBLE_FRAMES || ram.r8(GAME + 0x42) == 1;
    }
    if end {
        let f = ram.rs16(pwp + 0x10) as u32 & 0xffff_7fff;
        ram.w16(pwp + 0x10, f as u16);
        host.free_task(task);
        return;
    }
    or16(ram, pwp + 0x10, 0x8000);
    let count = (u32::from(ram.r16(twp + 6)) & 1) + 1;
    for _ in 0..count {
        let child = host.create_child_task(2, INVINCIBLE_SPARK_EXEC, task);
        if child == 0 {
            continue;
        }
        let ctwp = ram.r32(child + 0x34);
        let x = rand_times(ram, ANGLE);
        ram.w32(ctwp + 8, x as u32);
        let y = rand_times(ram, ANGLE);
        ram.w32(ctwp + 0xc, y as u32);
        let n = ram.r8(twp + 1);
        ram.w8(ctwp + 1, n);
        ram.w32(child + 0x1c, INVINCIBLE_SPARK_DISP);
    }
}

/// The barrier execs: end (FreeTask) when the player's twp is gone or its
/// pwp +0x10 lost `bit`; otherwise, when `rand` / 32768 is above 0.8, a
/// sparkle with random X and Y angles and the player number.
fn barrier_exec(
    ram: &mut impl Ram,
    host: &mut impl ContactHost,
    task: u32,
    bit: u32,
    magnet: bool,
) {
    let twp = ram.r32(task + 0x34);
    let n = ram.rs8(twp + 1);
    let ptwp = ram.r32(slot(PLAYER_TWP, n));
    if ptwp == 0 || {
        let pwp = ram.r32(slot(PLAYER_PWP, n));
        ram.rs16(pwp + 0x10) as u32 & bit == 0
    } {
        host.free_task(task);
        return;
    }
    let r = rand(ram) as f32;
    // `ble`: rand() is 0..0x7FFF, so the product is never NaN.
    if RAND_SCALE * r <= SPARK_CHANCE {
        return;
    }
    let exec = if magnet {
        MAGNET_SPARK_EXEC
    } else {
        BARRIER_SPARK_EXEC
    };
    let child = host.create_child_task(2, exec, task);
    if child == 0 {
        return;
    }
    let ctwp = ram.r32(child + 0x34);
    if magnet {
        let n = ram.r8(twp + 1);
        ram.w8(ctwp + 1, n);
    }
    let x = rand_times(ram, ANGLE);
    ram.w32(ctwp + 8, x as u32);
    let y = rand_times(ram, ANGLE);
    ram.w32(ctwp + 0xc, y as u32);
    if magnet {
        ram.wf(ctwp + 0x28, ONE);
        ram.w32(child + 0x1c, MAGNET_SPARK_DISP);
    } else {
        let n = ram.r8(twp + 1);
        ram.w8(ctwp + 1, n);
        ram.w32(child + 0x1c, BARRIER_SPARK_DISP);
    }
}

/// fn_800BD6E8(task) (0x800BD6E8), the barrier's exec (pwp +0x10 bit 1;
/// sparkles fn_800BD6AC, display fn_800BD5C0).
pub fn fn_800bd6e8(ram: &mut impl Ram, host: &mut impl ContactHost, task: u32) {
    barrier_exec(ram, host, task, 1, false);
}

/// fn_800BE0BC(task) (0x800BE0BC), the magnetic barrier's exec (bit 2;
/// sparkles fn_800BDFBC with size 1.0 at twp +0x28, display fn_800BDD88).
pub fn fn_800be0bc(ram: &mut impl Ram, host: &mut impl ContactHost, task: u32) {
    barrier_exec(ram, host, task, 2, true);
}

/// The invincibility and barrier sparkles' exec: twp +3 counts frames up
/// to `last`, then FreeTask.
fn spark_exec(ram: &mut impl Ram, host: &mut impl ContactHost, task: u32, last: u8) {
    let twp = ram.r32(task + 0x34);
    let v = ram.r8(twp + 3);
    if v >= last {
        host.free_task(task);
    } else {
        ram.w8(twp + 3, v.wrapping_add(1));
    }
}

/// fn_800BD2B4(task) (0x800BD2B4): an invincibility sparkle, 16 frames.
pub fn fn_800bd2b4(ram: &mut impl Ram, host: &mut impl ContactHost, task: u32) {
    spark_exec(ram, host, task, 0x10);
}

/// fn_800BD6AC(task) (0x800BD6AC): a barrier sparkle, 30 frames.
pub fn fn_800bd6ac(ram: &mut impl Ram, host: &mut impl ContactHost, task: u32) {
    spark_exec(ram, host, task, 0x1e);
}

/// fn_800BDFBC(task) (0x800BDFBC): a magnetic sparkle. Its frame count
/// (twp +3) goes up; for its first 11 frames twp +6 gets a random frame
/// (`rand` scaled to 0..15) and twp +4 a random texture (0..2); after that
/// its size (twp +0x28), capped to 0.5, shrinks by 0.025 a frame, and once
/// it is at most 0.0 the task is freed.
pub fn fn_800bdfbc(ram: &mut impl Ram, host: &mut impl ContactHost, task: u32) {
    let twp = ram.r32(task + 0x34);
    let v = ram.r8(twp + 3);
    ram.w8(twp + 3, v.wrapping_add(1));
    if v > 0xa {
        let s = ram.rf(twp + 0x28);
        if s <= ZERO {
            host.free_task(task);
            return;
        }
        if s > HALF {
            ram.wf(twp + 0x28, HALF);
        }
        let s = ram.rf(twp + 0x28);
        ram.wf(twp + 0x28, s - SHRINK);
    } else {
        let f = rand_times(ram, SPARK_FRAMES);
        ram.w16(twp + 6, f as u16);
        let t = rand_times(ram, SPARK_TEXTURES);
        ram.w16(twp + 4, t as u16);
    }
}

// --- displays -----------------------------------------------------------------

/// The Ninja drawing calls the effect displays make (main.dol routines, not
/// ported: they set render state and draw).
pub trait EffectDraw {
    /// njDisableFog (0x8011C2BC).
    fn nj_disable_fog(&mut self);
    /// njEnableFog (0x8011C2B0).
    fn nj_enable_fog(&mut self);
    /// gjSetFog (0x8011C310).
    fn gj_set_fog(&mut self);
    /// njPushMatrixEx (0x80116098).
    fn nj_push_matrix_ex(&mut self);
    /// njPopMatrixEx (0x801160E4).
    fn nj_pop_matrix_ex(&mut self);
    /// njPopMatrix(n) (0x80115DC8).
    fn nj_pop_matrix(&mut self, n: u32);
    /// njSetTexture(texlist) (0x80116E18).
    fn nj_set_texture(&mut self, texlist: u32);
    /// njTranslateEx(&v) (0x8011616C); `v` is the address of the point.
    fn nj_translate_ex(&mut self, v: u32);
    /// njRotateY(0, ang) (0x80115828).
    fn nj_rotate_y(&mut self, m: u32, ang: u32);
    /// njRotateX(0, ang) (0x80115C60).
    fn nj_rotate_x(&mut self, m: u32, ang: u32);
    /// njScale(0, x, y, z) (0x80115178).
    fn nj_scale(&mut self, m: u32, x: f32, y: f32, z: f32);
    /// njCnkCacheDrawModel(model) (0x8011DF20).
    fn nj_cnk_cache_draw_model(&mut self, model: u32);
    /// fn_8011E158(model): the other chunk model draw.
    fn fn_8011e158(&mut self, model: u32);
}

/// The shared start of the displays: the task's size (twp +0x20) is the
/// player's (its twp +0x20, copied); for a mech (pwp +1 is 6 or 7) the
/// effect size (+0x24) is 13 times it and the position (+0x14) is the
/// pwp's +0x218 point, for the others 6 times and +0x230. With `rescale`
/// the size becomes 0.2 times the effect size. Returns the task's twp.
fn pose(ram: &mut impl Ram, task: u32, rescale: bool) -> u32 {
    let (twp, ptwp, pwp) = owner(ram, task);
    let s = ram.r32(ptwp + 0x20);
    ram.w32(twp + 0x20, s);
    let k = ram.rs8(pwp + 1);
    let (size, at) = if k == 6 || k == 7 {
        (SIZE_MECH, 0x218)
    } else {
        (SIZE, 0x230)
    };
    let s = ram.rf(twp + 0x20);
    ram.wf(twp + 0x24, size * s);
    for o in [0, 4, 8] {
        let w = ram.r32(pwp + at + o);
        ram.w32(twp + 0x14 + o, w);
    }
    if rescale {
        let e = ram.rf(twp + 0x24);
        ram.wf(twp + 0x20, SIZE_SCALE * e);
    }
    twp
}

/// fn_800BD858(task) (0x800BD858), the barrier's display, and fn_800BE234
/// (0x800BE234), the magnetic barrier's (the same code): they only place
/// the task on its player; the sparkles draw.
pub fn fn_800bd858(ram: &mut impl Ram, task: u32) {
    pose(ram, task, true);
}

/// fn_800BE234(task) (0x800BE234): see [`fn_800bd858`].
pub fn fn_800be234(ram: &mut impl Ram, task: u32) {
    pose(ram, task, true);
}

/// The model draw the displays share: translate to twp +0x14, turn by the
/// Y (+0xC) and X (+8) angles, scale by twp +0x20.
fn place(ram: &mut impl Ram, draw: &mut impl EffectDraw, twp: u32) {
    draw.nj_translate_ex(twp + 0x14);
    let y = ram.r32(twp + 0xc);
    draw.nj_rotate_y(0, y);
    let x = ram.r32(twp + 8);
    draw.nj_rotate_x(0, x);
    let s = ram.rf(twp + 0x20);
    draw.nj_scale(0, s, s, s);
}

/// fn_800BD478(task) (0x800BD478), the invincibility's display: placed on
/// its player (without the 0.2 size), drawn with fog off.
pub fn fn_800bd478(ram: &mut impl Ram, draw: &mut impl EffectDraw, task: u32) {
    let twp = pose(ram, task, false);
    draw.nj_disable_fog();
    draw.gj_set_fog();
    draw.nj_push_matrix_ex();
    draw.nj_set_texture(INVINCIBLE_TEXLIST);
    place(ram, draw, twp);
    draw.nj_cnk_cache_draw_model(INVINCIBLE_MODEL);
    draw.nj_pop_matrix_ex();
    draw.nj_enable_fog();
    draw.gj_set_fog();
}

/// Whether `task` is its parent's first child (parent +0xC).
fn first_child(ram: &mut impl Ram, task: u32) -> bool {
    let parent = ram.r32(task + 8);
    ram.r32(parent + 0xc) == task
}

/// Whether `task` is its parent's last child (the first child's +4).
fn last_child(ram: &mut impl Ram, task: u32) -> bool {
    let parent = ram.r32(task + 8);
    let first = ram.r32(parent + 0xc);
    ram.r32(first + 4) == task
}

/// fn_800BD120(task) (0x800BD120), an invincibility sparkle's display:
/// placed on the player (with the 0.2 size); the first sparkle turns fog
/// off and the last back on; the texture is the frame's (twp +3, 0 to 7).
pub fn fn_800bd120(ram: &mut impl Ram, draw: &mut impl EffectDraw, task: u32) {
    let twp = pose(ram, task, true);
    if first_child(ram, task) {
        draw.nj_disable_fog();
        draw.gj_set_fog();
    }
    draw.nj_push_matrix_ex();
    let f = u32::from(ram.r8(twp + 3));
    draw.nj_set_texture(INVINCIBLE_SPARK_TEX + ((f << 3) & 0x38));
    place(ram, draw, twp);
    draw.fn_8011e158(INVINCIBLE_SPARK_MODEL);
    draw.nj_pop_matrix_ex();
    if last_child(ram, task) {
        draw.nj_enable_fog();
        draw.gj_set_fog();
    }
}

/// fn_800BD5C0(task) (0x800BD5C0), a barrier sparkle's display. The first
/// sparkle turns fog off, pushes a matrix and moves it to the barrier task
/// (its twp +0x14); each sparkle then draws in its own pushed matrix (the
/// others pop the previous one first) turned by its angles and scaled by
/// the barrier's size (twp +0x20), with its frame's texture (twp +3, 0 to
/// 30); the last turns fog back on and pops two.
pub fn fn_800bd5c0(ram: &mut impl Ram, draw: &mut impl EffectDraw, task: u32) {
    let twp = ram.r32(task + 0x34);
    if first_child(ram, task) {
        draw.nj_disable_fog();
        draw.gj_set_fog();
        draw.nj_push_matrix_ex();
        let parent = ram.r32(task + 8);
        let ptwp = ram.r32(parent + 0x34);
        draw.nj_translate_ex(ptwp + 0x14);
        draw.nj_push_matrix_ex();
    } else {
        draw.nj_pop_matrix_ex();
        draw.nj_push_matrix_ex();
    }
    let f = u32::from(ram.r8(twp + 3));
    draw.nj_set_texture(BARRIER_SPARK_TEX + ((f << 2) & 0x78));
    let y = ram.r32(twp + 0xc);
    draw.nj_rotate_y(0, y);
    let x = ram.r32(twp + 8);
    draw.nj_rotate_x(0, x);
    let parent = ram.r32(task + 8);
    let ptwp = ram.r32(parent + 0x34);
    let s = ram.rf(ptwp + 0x20);
    draw.nj_scale(0, s, s, s);
    draw.fn_8011e158(BARRIER_SPARK_MODEL);
    if last_child(ram, task) {
        draw.nj_enable_fog();
        draw.gj_set_fog();
        draw.nj_pop_matrix(2);
    }
}
