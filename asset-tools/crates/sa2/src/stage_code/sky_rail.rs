//! Sky Rail (story level 6, `stg06D.rel`): the stage program "STG06", its
//! stage tasks and the stage's own objects. Offsets are section-1 (code)
//! offsets of stg06D.rel; engine calls are reported at GSNE8P main.dol
//! addresses. The Steam `resource/gd_PC/stg06D.rel` (module 5) has
//! byte-identical sections to the GameCube disc copy (module 6), so both are
//! accepted and give the same results.
//!
//! Every routine is run with [`replay`] on a [`super::Run`] input: the task
//! is `t`, its work `twp`, other pointers are names given by the input (see
//! the runner). Module routines outside this port are reported as `module`
//! calls; ported module routines this port calls are run in place (their
//! calls appear in order). See `docs/SKY-RAIL.md` for the routine table.
use super::{dol, fctiwz, key, Pin, Run, StageRel};
use crate::{Error, Result};
use serde_json::{json, Value};

pub const PIN: Pin = Pin {
    name: "stage 6 REL",
    len: 1_389_820,
    gc_sha256: "791bbcaddc8cac8c52a8cdbc688f6f644033047e2ef76956c34b306edbd33555",
    gc_module: 6,
    steam_sha256: "331fcfc6a73f98d95e8fd5a2595eaa509fb5785998f5d78a71137c0327aba615",
    steam_module: 5,
};

pub fn read(bytes: &[u8]) -> Result<StageRel<'_>> {
    StageRel::read(bytes, &PIN)
}

fn tw(o: u32) -> String {
    key("twp", o)
}
fn tk(o: u32) -> String {
    key("t", o)
}
fn bad(what: &str) -> Error {
    Error::invalid(format!("stage 6: {what}"))
}
/// The game's `fabsf`/`sqrtf` helpers (MSL, shared by every module): plain
/// float math, computed in place rather than reported.
fn sqrtf(x: f32) -> f32 {
    (x as f64).sqrt() as f32
}

type Routine = fn(&mut Run<'_, '_>) -> Result<()>;

/// Every ported routine: name, stg06D section-1 offset, size.
pub const ROUTINES: &[(&str, u32, u32, Routine)] = &[
    // stage program and stage tasks
    ("setup", 0x5130, 0x4d8, setup),
    ("teardown", 0x5608, 0x178, teardown),
    ("exec", 0x5834, 0x2c, exec),
    ("bgm_exec", 0x5780, 0xb4, bgm_exec),
    ("task_exec", 0x5860, 0xf8, task_exec),
    ("task_display", 0x5958, 0x1b0, task_display),
    ("task_display2", 0x5b08, 0x288, task_display2),
    // DRMCN
    ("drum_load", 0x0, 0xa0, drum_load),
    ("drum_dest", 0xa0, 0x44, drum_dest),
    ("drum_exec", 0xe4, 0x304, drum_exec),
    ("drum_display", 0x3e8, 0x70, drum_display),
    // FIRZIZ and its sparks
    ("firziz_load", 0x458, 0xac, firziz_load),
    ("firziz_dest", 0x504, 0x40, firziz_dest),
    ("firziz_emit", 0x544, 0x25c, firziz_emit),
    ("firziz_exec", 0x7a0, 0x394, firziz_exec),
    ("firziz_display", 0xb34, 0x100, firziz_display),
    ("spark_spawn", 0xc34, 0x314, spark_spawn),
    ("spark_exec", 0xf48, 0x170, spark_exec),
    ("spark_dest", 0x10b8, 0x2c, spark_dest),
    ("spark_display", 0x10e4, 0xdc, spark_display),
    // KAZANI (flock)
    ("flock_member_init", 0x11c0, 0x2c0, flock_member_init),
    ("flock_load", 0x1480, 0x4ac, flock_load),
    ("flock_dest", 0x192c, 0x64, flock_dest),
    ("flock_steer", 0x1990, 0x344, flock_steer),
    ("flock_exec", 0x1cd4, 0x6fc, flock_exec),
    ("flock_display", 0x23d0, 0x1cc, flock_display),
    // RAILOBJA, MOASIBA(F) and the shared rail-piece constructor
    ("railobja_load", 0x2660, 0x60, railobja_load),
    ("moasiba_load", 0x26c0, 0x60, moasiba_load),
    ("piece_load", 0x2720, 0x2e8, piece_load_a),
    ("piece_dest", 0x2a08, 0x5c, piece_dest_a),
    ("piece_exec", 0x2a64, 0xcc, piece_exec_a),
    ("piece_display", 0x2b30, 0x8c, piece_display),
    ("piece_display_lit", 0x2bbc, 0x9c, piece_display_lit),
    // MOVENT(F)
    ("movent_load", 0x2c58, 0x2c0, movent_load),
    ("movent_dest", 0x2f18, 0x64, movent_dest),
    ("movent_exec", 0x2f7c, 0xc8, movent_exec),
    ("movent_display", 0x3044, 0xb8, movent_display),
    // PNP (spring pad)
    ("pnp_load", 0x30fc, 0xf0, pnp_load),
    ("pnp_dest", 0x31ec, 0x40, pnp_dest),
    ("pnp_exec", 0x322c, 0x92c, pnp_exec),
    ("pnp_display", 0x3b58, 0x148, pnp_display),
    ("pnp_display2", 0x3ca0, 0xfc, pnp_display2),
    // scenery: RCROSS, TIM(F), KUSA, KAKASI, FENCE, WOODEAD
    ("rcross_load", 0x3da4, 0x78, rcross_load),
    ("tim_load", 0x3e1c, 0x6c, tim_load),
    ("kusa_load", 0x3e88, 0x74, kusa_load),
    ("kakasi_load", 0x3efc, 0x6c, kakasi_load),
    ("fence_load", 0x3f68, 0x6c, fence_load),
    ("woodead_load", 0x3fd4, 0x6c, woodead_load),
    ("scenery_init", 0x4040, 0x80, scenery_init_entry),
    ("scenery_dest", 0x40c0, 0x24, scenery_dest),
    ("scenery_exec", 0x40e4, 0x58, scenery_exec),
    ("scenery_display", 0x413c, 0x214, scenery_display),
    // RAILOBJCL / RAILOBJCR
    ("railobjcl_load", 0x4350, 0x10c, railobjcl_load),
    ("railobjcr_load", 0x445c, 0x10c, railobjcr_load),
    ("railobjc_dest", 0x4568, 0x24, railobjc_dest),
    ("railobjc_exec", 0x458c, 0x7c, railobjc_exec),
    ("railobjc_display", 0x4608, 0x108, railobjc_display),
    // MOTOOL(F), RAILOBJBL, RAILOBJBR and their constructor
    ("motool_load", 0x4710, 0x64, motool_load),
    ("railobjbr_load", 0x4774, 0x60, railobjbr_load),
    ("railobjbl_load", 0x47d4, 0x60, railobjbl_load),
    ("spinner_load", 0x4834, 0x2ec, piece_load_b),
    ("spinner_dest", 0x4b20, 0x5c, piece_dest_b),
    ("spinner_exec", 0x4b7c, 0x110, spinner_exec),
    ("spinner_display_x", 0x4c8c, 0xf4, spinner_display_x),
    ("spinner_display_z", 0x4d80, 0xf4, spinner_display_z),
];

/// Run one routine by name. `input` is `{ "mem": {..}, "rets": {..} }`.
pub fn replay(bytes: &[u8], routine: &str, input: &Value) -> Result<Value> {
    let sr = read(bytes)?;
    let (_, _, _, f) = ROUTINES
        .iter()
        .find(|r| r.0 == routine)
        .ok_or_else(|| bad(&format!("unknown routine {routine}")))?;
    let mut r = Run::new(&sr, input)?;
    f(&mut r)?;
    Ok(r.finish())
}

fn range_out(r: &mut Run<'_, '_>, pc: u32) -> Result<bool> {
    Ok(r.er(pc, dol::CHECK_RANGE_OUT, json!(["t"]))? != 0)
}
fn pos(r: &Run<'_, '_>) -> Result<[f32; 3]> {
    r.v3("twp", 0x14)
}
/// `twp+4 |= 0x100` while a player is within the sphere, else cleared.
fn near_flag(r: &mut Run<'_, '_>, within: bool) -> Result<()> {
    let f = r.sh(&tw(4))?;
    r.set(tw(4), if within { f | 0x100 } else { f & !0x100 });
    Ok(())
}
/// The model pointer `+4` and a child (`+0x2c`) / sibling (`+0x30`) of a
/// module NJS_OBJECT named by `obj`.
fn field(r: &Run<'_, '_>, obj: &str, off: u32) -> Result<Value> {
    r.get(&key(obj, off))
}
fn name(v: &Value) -> Result<String> {
    v.as_str()
        .map(str::to_string)
        .ok_or_else(|| bad("expected a module object"))
}
fn ptr_name(r: &Run<'_, '_>, pc: u32) -> Result<String> {
    name(&r.ptr(pc)?)
}

// ---------------------------------------------------------------- program

/// +0x5130, the program's setup.
fn setup(r: &mut Run<'_, '_>) -> Result<()> {
    let flag = r.gs(0x5144, 0x1e)?;
    if r.sb(&flag)? != 0 {
        // BgmStart task; its handle is kept only in the frame.
        let a = json!([2, 0, r.ptr(0x5164)?, r.ptr(0x516c)?]);
        r.e(0x5170, dol::CREATE_ELEMENTAL_TASK, a)?;
        r.ret(dol::CREATE_ELEMENTAL_TASK.1)?;
    }
    let p = |r: &Run<'_, '_>, pc| r.ptr(pc);
    let a = json!([128, p(r, 0x5180)?]);
    r.e(0x5184, dol::FN_800324E4, a)?;
    let a = json!([p(r, 0x518c)?, p(r, 0x5194)?, p(r, 0x519c)?, p(r, 0x51a4)?]);
    r.e(0x51a8, dol::FN_800245EC, a)?;
    for (pc, f, a, b) in [
        (0x51bc, dol::FN_80072270, 0x51b0, 0x51b8),
        (0x51d0, dol::FN_80072218, 0x51c4, 0x51cc),
        (0x51e4, dol::FN_80072270, 0x51d8, 0x51e0),
        (0x51f8, dol::FN_80072218, 0x51ec, 0x51f4),
    ] {
        let a = json!([p(r, a)?, p(r, b)?]);
        r.e(pc, f, a)?;
    }
    let h = r.er(0x5204, dol::FN_80065048, json!([0, 0x800]))?;
    let slot = ptr_name(r, 0x520c)?; // .bss+4
    r.set(slot, h);
    let a = json!([p(r, 0x5218)?, h]);
    r.e(0x5228, dol::FN_8006638C, a)?;
    r.e(0x522c, dol::FN_8001A8A8, json!([]))?;
    r.e(0x5230, dol::FN_80033710, json!([]))?;
    let a = json!([r.c(0x5238)?, r.c(0x5244)?, r.c(0x5250)?]);
    r.e(0x5258, dol::FN_800C3CD8, a)?;
    let a = json!([r.c(0x5260)?, r.c(0x526c)?]);
    r.e(0x5274, dol::FN_8011E154, a)?;
    let a = json!([r.c(0x527c)?, r.c(0x5288)?, r.c(0x5294)?]);
    r.e(0x529c, dol::FN_8011E150, a)?;
    let lights = ptr_name(r, 0x52a8)?;
    let second = key(&lights, 0x20); // addi r4,r4,32 at +0x52BC
    for (k, pc, at) in [
        (0, 0x52ac, lights.clone()),
        (1, 0x52c0, second),
        (2, 0x52d0, ptr_name(r, 0x52cc)?),
        (3, 0x52e0, ptr_name(r, 0x52dc)?),
    ] {
        r.e(pc, dol::FN_8002506C, json!([k, at]))?;
    }
    r.e(0x52ec, dol::FN_80025028, json!([0, 0]))?;
    r.e(0x52f8, dol::FN_80025028, json!([0, 1]))?;
    r.e(0x5300, dol::FN_80025048, json!([0]))?;
    r.e(0x5308, dol::FN_8002503C, json!([2]))?;
    let a = json!([p(r, 0x5310)?]);
    r.m(0x5314, a)?;
    let flag = r.gs(0x531c, 0x1e)?;
    if r.sb(&flag)? == 0 {
        r.e(0x5334, dol::BGM_SET_VOLUME, json!([0]))?;
        let a = json!([p(r, 0x533c)?]);
        r.e(0x5340, dol::STG_PLAY_MUSIC, a)?;
    }
    let a = json!([p(r, 0x5348)?]);
    r.e(0x534c, dol::FN_8005D0EC, a)?;
    let a = json!([2, 1, p(r, 0x535c)?, p(r, 0x5364)?]);
    if let Some(task) = r.ep(0x5368, dol::CREATE_ELEMENTAL_TASK, a)? {
        let (d, d2) = (p(r, 0x5380)?, p(r, 0x538c)?);
        r.set(key(&task, 0x14), d);
        r.set(key(&task, 0x20), d2);
    }
    let a = json!([p(r, 0x5398)?]);
    r.e(0x539c, dol::FN_8003BCC4, a)?;
    // +0x4E74: texture override install, shared with Metal Harbor.
    r.m(0x53a0, json!([]))?;
    // Damage and hook slots at 0x801A2A18 (+0 is tested twice).
    for (off, load, func) in [
        (0, 0x53a8, 0x53bc),
        (0, 0x53d0, 0x53e4),
        (4, 0x53f8, 0x540c),
        (8, 0x5420, 0x5434),
        (0xc, 0x5448, 0x545c),
        (0x10, 0x5470, 0x5484),
    ] {
        r.d(load, 0x801a_2a18)?;
        let k = key("0x801a2a18", off);
        if !r.nz(&k)? {
            let f = p(r, func)?;
            r.set(k, f);
        }
    }
    let a = json!([p(r, 0x5498)?, p(r, 0x54a0)?]);
    r.m(0x54a4, a)?;
    r.e(0x54a8, dol::NJ_DISABLE_FOG, json!([]))?;
    r.e(0x54ac, dol::GJ_SET_FOG, json!([]))?;
    r.m(0x54b0, json!([]))?;
    let a = json!([p(r, 0x54b8)?]);
    r.m(0x54bc, a)?;
    r.m(0x54c8, json!([0, 0]))?;
    let flag = r.gs(0x54d0, 0x1e)?;
    if r.sb(&flag)? != 0 {
        r.m(0x54ec, json!([1, 0]))?;
    }
    // Trail plugin table 0x801E77D8: fn_800E014C, three module routines, zeros.
    r.d(0x54f4, 0x800e_014c)?;
    r.d(0x54fc, 0x801e_77d8)?;
    let mut trail = vec![json!(0x800e014c_u32)];
    for pc in [0x5508, 0x551c, 0x5530] {
        trail.push(p(r, pc)?);
    }
    trail.resize(14, json!(0));
    for (k, v) in trail.into_iter().enumerate() {
        r.set(key("0x801e77d8", 4 * k as u32), v);
    }
    let a = json!([p(r, 0x55e4)?, p(r, 0x55ec)?]);
    r.e(0x55f0, dol::FN_8006D500, a)
}

/// +0x5608, the program's teardown.
fn teardown(r: &mut Run<'_, '_>) -> Result<()> {
    r.m(0x5614, json!([]))?;
    r.m(0x5618, json!([]))?;
    r.m(0x561c, json!([]))?;
    r.e(0x5620, dol::NJ_DISABLE_FOG, json!([]))?;
    r.e(0x5624, dol::GJ_SET_FOG, json!([]))?;
    for (off, load, func) in [
        (0, 0x562c, 0x5638),
        (4, 0x5658, 0x5664),
        (8, 0x5684, 0x5690),
        (0xc, 0x56b0, 0x56bc),
        (0x10, 0x56dc, 0x56e8),
    ] {
        r.d(load, 0x801a_2a18)?;
        let k = key("0x801a2a18", off);
        if r.get(&k)? == r.ptr(func)? {
            r.set(k, 0);
        }
    }
    r.e(0x5704, dol::FN_800336DC, json!([]))?;
    let a = json!([r.ptr(0x570c)?, r.ptr(0x5714)?]);
    r.e(0x5718, dol::FN_80072118, a)?;
    let a = json!([r.ptr(0x5720)?, r.ptr(0x5728)?]);
    r.e(0x572c, dol::FN_80072118, a)?;
    r.e(0x5730, dol::FN_80066364, json!([]))?;
    let slot = ptr_name(r, 0x5738)?;
    if r.nz(&slot)? {
        let h = r.get(&slot)?;
        r.e(0x5754, dol::FN_8001A2B0, json!([h]))?;
        r.set(slot, 0);
    }
    r.e(0x5768, dol::FN_800245B8, json!([]))?;
    // +0x4F54: player reset and texture override restore (shared with Metal Harbor).
    r.m(0x576c, json!([]))
}

/// +0x5834, the program's exec: reads the work's mode byte and returns.
fn exec(r: &mut Run<'_, '_>) -> Result<()> {
    r.sb(&tw(0))?;
    Ok(())
}

/// +0x5780, the BgmStart task: once the game reaches mode 16 with both
/// players' character byte 54, start "s_rail.adx" (at clock 198 exactly) or
/// call fn_80011DF4 (after it); in mode 7, fn_80011DF4.
fn bgm_exec(r: &mut Run<'_, '_>) -> Result<()> {
    if r.er(0x578c, dol::FN_8001CD40, json!([]))? == 16 {
        r.d(0x579c, 0x801e_7768)?;
        let p0 = r.via("0x801e7768+0x0", 0)?;
        r.d(0x57b8, 0x801e_7768)?;
        if r.sb(&p0)? == 54 {
            let p1 = r.via("0x801e7768+0x4", 0)?;
            if r.sb(&p1)? == 54 {
                r.d(0x57d4, 0x803a_d9dc)?;
                let clock = r.i("0x803ad9dc+0x0")? as i32;
                if clock == 198 {
                    r.e(0x57e8, dol::BGM_SET_VOLUME, json!([0]))?;
                    let a = json!([r.ptr(0x57f0)?]);
                    r.e(0x57f4, dol::STG_PLAY_MUSIC, a)?;
                } else if clock > 198 {
                    r.e(0x5810, dol::FN_80011DF4, json!([]))?;
                }
            }
        }
    }
    if r.er(0x5814, dol::FN_8001CD40, json!([]))? == 7 {
        r.e(0x5820, dol::FN_80011DF4, json!([]))?;
    }
    Ok(())
}

/// +0x5860, the stage task exec: bit k of work +0 says whether camera k
/// (0x80175378[k], up to two, stopping at a null) is past x > c and below
/// y < c'; work +6 counts 0..13.
fn task_exec(r: &mut Run<'_, '_>) -> Result<()> {
    r.d(0x5880, 0x8017_5378)?;
    let (cx, cy) = (r.c(0x58ac)?, r.c(0x58c4)?);
    let mut bits = r.b(&tw(0))?;
    for i in 0..2u32 {
        let slot = key("0x80175378", 4 * i);
        let Some(cam) = r.p(&slot)? else { break };
        let v = r.v3(&cam, 0)?;
        if v[0] > cx && v[1] < cy {
            bits |= 1 << i;
        } else {
            bits &= !(1 << i);
        }
        r.set(tw(0), bits);
    }
    let n = r.h(&tw(6))?.wrapping_add(1);
    r.set(tw(6), if n < 14 { n } else { 0 });
    Ok(())
}

/// Skips drawing in split screen when the viewport (0x803ADAD0) is not the
/// one this pass draws (0x801CC168+0x3D modes 1 and 2) and in mode 3.
fn sky_hidden(r: &Run<'_, '_>, pcs: [u32; 5]) -> Result<bool> {
    let mode = r.gs(pcs[0], 0x3d)?;
    let mode = r.sb(&mode)?;
    r.d(pcs[1], 0x803a_dad0)?;
    r.d(pcs[2], 0x803a_dad0)?;
    r.gs(pcs[3], 0x3d)?;
    r.gs(pcs[4], 0x3d)?;
    let vp = r.i("0x803adad0+0x0")?;
    Ok((mode == 1 && vp == 0) || (mode == 2 && vp == 1) || mode == 3)
}
/// The sky's origin: camera x/z, and a y that follows the player at a rate.
fn sky_origin(r: &Run<'_, '_>, pcs: [u32; 6]) -> Result<[f32; 3]> {
    r.d(pcs[0], 0x8017_5378)?;
    r.d(pcs[1], 0x801e_7768)?;
    let vp = r.i("0x803adad0+0x0")? as u32;
    let cam = r.via(&key("0x80175378", 4 * vp), 0)?;
    let cam = cam.trim_end_matches("+0x0").to_string();
    let pl = r.via(&key("0x801e7768", 4 * vp), 0x18)?;
    let base = ptr_name(r, pcs[3])?; // .bss+8
    let y = r.f(&base)? + (r.f(&pl)? - r.c(pcs[2])?) * r.c(pcs[4])?;
    r.d(pcs[5], 0x8017_5378)?;
    Ok([r.f(&key(&cam, 0))?, y, r.f(&key(&cam, 8))?])
}
/// The sky model: a clipped draw of the module object's model while
/// .bss+0 is set, else fn_8011E158 on the other model.
fn sky_model(r: &mut Run<'_, '_>, flag: u32, obj: u32, off: u32, draw: u32, alt: u32, call: u32) -> Result<()> {
    let flag = ptr_name(r, flag)?;
    if r.nz(&flag)? {
        let o = ptr_name(r, obj)?;
        let m = r.get(&key(&o, off))?;
        r.e(draw, dol::DS_DRAW_MODEL_CLIP, json!([m]))
    } else {
        let a = json!([r.ptr(alt)?]);
        r.e(call, dol::FN_8011E158, a)
    }
}

/// +0x5958, the stage task display: the sky model at the camera.
fn task_display(r: &mut Run<'_, '_>) -> Result<()> {
    if sky_hidden(r, [0x597c, 0x5994, 0x59c0, 0x59a8, 0x59d4])? {
        return Ok(());
    }
    r.e(0x59ec, dol::NJ_DISABLE_FOG, json!([]))?;
    r.e(0x59f0, dol::GJ_SET_FOG, json!([]))?;
    r.e(0x59f4, dol::NJ_PUSH_MATRIX_EX, json!([]))?;
    let v = sky_origin(r, [0x5a0c, 0x5a34, 0x5a48, 0x5a58, 0x5a64, 0x5a8c])?;
    r.e(0x5aa4, dol::NJ_TRANSLATE_EX, json!([v]))?;
    let a = json!([r.ptr(0x5aac)?]);
    r.e(0x5ab0, dol::NJ_SET_TEXTURE, a)?;
    sky_model(r, 0x5ab8, 0x5acc, 0xc, 0x5ad4, 0x5ae0, 0x5ae4)?;
    r.e(0x5ae8, dol::NJ_POP_MATRIX_EX, json!([]))?;
    r.e(0x5aec, dol::NJ_ENABLE_FOG, json!([]))?;
    r.e(0x5af0, dol::GJ_SET_FOG, json!([]))
}

/// +0x5B08, the stage task's second display: two scrolling cloud layers
/// (UV offsets from the frame clock, 9 bits) and the second sky model.
fn task_display2(r: &mut Run<'_, '_>) -> Result<()> {
    if sky_hidden(r, [0x5b2c, 0x5b44, 0x5b70, 0x5b58, 0x5b84])? {
        return Ok(());
    }
    r.e(0x5b9c, dol::NJ_DISABLE_FOG, json!([]))?;
    r.e(0x5ba0, dol::GJ_SET_FOG, json!([]))?;
    r.e(0x5ba4, dol::NJ_PUSH_MATRIX_EX, json!([]))?;
    let v = sky_origin(r, [0x5bbc, 0x5be4, 0x5bf8, 0x5c08, 0x5c14, 0x5c3c])?;
    r.e(0x5c54, dol::NJ_TRANSLATE_EX, json!([v]))?;
    for (tex, a, b, rate1, clk1, rate2, clk2, bl, draw, call) in [
        (0x5c5c, 0x5c68, 0x5c70, 0x5c78, 0x5c84, 0x5c98, 0x5ca4, 0x5cb4, 0x5cbc, 0x5cc0),
        (0x5cc8, 0x5cd4, 0x5cdc, 0x5ce4, 0x5cf0, 0x5d04, 0x5d10, 0x5d20, 0x5d28, 0x5d2c),
    ] {
        let t = json!([r.ptr(tex)?]);
        r.e(tex + 4, dol::NJ_SET_TEXTURE, t)?;
        let clock = r.gs(clk1, 0x7c)?;
        r.gs(clk2, 0x7c)?;
        let clock = r.w(&clock)?;
        let s1 = r.w(&ptr_name(r, rate1)?)?; // .bss word
        let (sec, off) = r.sr.at(rate2)?; // .data word
        let s2 = r.sr.word(sec, off)?;
        let u = s1.wrapping_mul(clock) & 0x1ff;
        let w = s2.wrapping_mul(clock) & 0x1ff;
        let args = json!([r.ptr(a)?, r.ptr(b)?, u, w]);
        r.e(bl, dol::FN_80072B2C, args)?;
        let m = json!([r.ptr(draw)?]);
        r.e(call, dol::FN_8011E158, m)?;
    }
    let t = json!([r.ptr(0x5d34)?]);
    r.e(0x5d38, dol::NJ_SET_TEXTURE, t)?;
    sky_model(r, 0x5d40, 0x5d54, 0x1c, 0x5d5c, 0x5d68, 0x5d6c)?;
    r.e(0x5d70, dol::NJ_POP_MATRIX_EX, json!([]))?;
    r.e(0x5d74, dol::NJ_ENABLE_FOG, json!([]))?;
    r.e(0x5d78, dol::GJ_SET_FOG, json!([]))
}

// ---------------------------------------------------------------- DRMCN

/// +0x0, the drum loader.
fn drum_load(r: &mut Run<'_, '_>) -> Result<()> {
    if range_out(r, 0x20)? {
        return Ok(());
    }
    let (d, e, x) = (r.ptr(0x30)?, r.ptr(0x3c)?, r.ptr(0x48)?);
    r.set(tk(0x14), d);
    r.set(tk(0x10), e);
    r.set(tk(0x18), x);
    r.set(tw(1), 0);
    let a = json!(["t", r.ptr(0x60)?, 1, 4]);
    r.e(0x6c, dol::CCL_INIT, a)?;
    let k = r.w(&tw(0x10))? & 0x1f;
    r.m(0x7c, json!([k, "twp+0x20"]))?;
    r.set(tw(3), 0);
    Ok(())
}

/// +0xA0, the drum's destructor.
fn drum_dest(r: &mut Run<'_, '_>) -> Result<()> {
    if r.nz(&tw(0x2c))? {
        r.m(0xcc, json!(["twp+0x20"]))?;
    }
    Ok(())
}

/// +0xE4, the drum exec: when hit (collision flag 1 and fn_8003699C, or
/// work +0x20 set) it bursts into 17 pieces, scores 20 and spawns +0xA9CC;
/// then counts work +3 up and frees itself after 4 frames.
fn drum_exec(r: &mut Run<'_, '_>) -> Result<()> {
    if range_out(r, 0x110)? {
        return Ok(());
    }
    let st = r.b(&tw(3))?;
    if st != 0 {
        let p = pos(r)?;
        let v = [r.c(0x144)?, r.c(0x154)?, r.c(0x164)?];
        if st == 2 {
            let y = r.f(&tw(0x18))? - r.c(0x1b0)?;
            let a = json!([p, r.get(&tw(0xc))?, r.ptr(0x188)?, 6, r.c(0x194)?, r.c(0x1a0)?, y, v]);
            r.m(0x1c0, a)?;
        }
        if st > 4 {
            if r.nz(&tk(0x30))? {
                r.e(0x1e0, dol::DEAD_OUT, json!(["t"]))?;
            } else {
                r.e(0x1ec, dol::FREE_TASK, json!(["t"]))?;
            }
        }
        r.set(tw(3), st.wrapping_add(1));
    } else {
        let cw = r.via(&tw(0x2c), 0)?;
        let cw = cw.trim_end_matches("+0x0").to_string();
        let mut hit = false;
        if r.h(&key(&cw, 4))? & 1 != 0 {
            let other = r.via(&key(&cw, 0xa4), 0x9c)?;
            let a = json!(["t", r.get(&other)?]);
            hit = r.er(0x224, dol::FN_8003699C, a)? != 0;
        }
        if hit || r.nz(&tw(0x20))? {
            let mut p = pos(r)?;
            p[1] += r.c(0x25c)?;
            for _ in 0..17 {
                let f31 = r.rnd(0x274)? * r.c(0x2a0)? * r.c(0x2b0)? + r.c(0x2c0)?;
                let f30 = r.rnd(0x2cc)? * r.c(0x2f8)? * r.c(0x308)? + r.c(0x318)?;
                let f1 = r.rnd(0x324)? * r.c(0x350)? * r.c(0x360)? + r.c(0x370)?;
                r.m(0x388, json!([p, f1, f30, f31]))?;
            }
            r.set(tw(3), 1);
            r.e(0x3a4, dol::ADD_SCORE, json!([20]))?;
            r.e(0x3ac, dol::FN_800168C8, json!([0]))?;
            r.m(0x3b8, json!([0, "twp+0x14"]))?;
        }
    }
    r.e(0x3c0, dol::CCL_ENTRY, json!(["t"]))
}

/// +0x3E8, the drum display.
fn drum_display(r: &mut Run<'_, '_>) -> Result<()> {
    let a = json!([r.ptr(0x408)?]);
    r.e(0x40c, dol::NJ_SET_TEXTURE, a)?;
    r.e(0x414, dol::NJ_PUSH_MATRIX, json!([0]))?;
    r.e(0x420, dol::NJ_TRANSLATE_V, json!([0, pos(r)?]))?;
    let a = json!([0, r.get(&tw(0xc))?]);
    r.e(0x42c, dol::NJ_ROTATE_Y, a)?;
    let a = json!([r.ptr(0x434)?]);
    r.e(0x438, dol::LATE_DRAW_MODEL, a)?;
    r.e(0x440, dol::NJ_POP_MATRIX, json!([1]))
}

// ---------------------------------------------------------------- FIRZIZ

/// +0x458, the fire jet loader: a 12-byte block at t+0x38 (+0 angle,
/// +4 flame height, +8 delay).
fn firziz_load(r: &mut Run<'_, '_>) -> Result<()> {
    if range_out(r, 0x478)? {
        return Ok(());
    }
    let blk = r.ep(0x48c, dol::SY_CALLOC, json!([1, 12]))?;
    r.set(tk(0x38), blk.clone().map_or(json!(0), Value::from));
    if blk.is_none() {
        return Ok(());
    }
    let (d, e, x) = (r.ptr(0x4a4)?, r.ptr(0x4b0)?, r.ptr(0x4bc)?);
    r.set(tk(0x14), d);
    r.set(tk(0x10), e);
    r.set(tk(0x18), x);
    r.set(tw(0), 0);
    let a = json!(["t", r.ptr(0x4d4)?, 2, 4]);
    r.e(0x4e0, dol::CCL_INIT, a)?;
    r.set(tw(3), 0);
    Ok(())
}

/// +0x504: free the block.
fn firziz_dest(r: &mut Run<'_, '_>) -> Result<()> {
    let b = r.get(&tk(0x38))?;
    r.e(0x524, dol::SY_FREE, json!([b]))?;
    r.set(tk(0x38), 0);
    Ok(())
}

fn blk(r: &Run<'_, '_>) -> Result<String> {
    r.p(&tk(0x38))?
        .ok_or_else(|| bad("t+0x38 is null where the routine reads its block"))
}

/// +0x544: emit one spark (+0xC34) from the jet's nozzle with a random
/// spread, speed scaled by work +0x20.
fn firziz_emit(r: &mut Run<'_, '_>) -> Result<()> {
    r.e(0x560, dol::NJ_PUSH_MATRIX_EX, json!([]))?;
    r.e(0x568, dol::NJ_UNIT_MATRIX, json!([0]))?;
    r.e(0x574, dol::NJ_TRANSLATE_V, json!([0, pos(r)?]))?;
    let a = json!([0, r.get(&tw(0xc))?]);
    r.e(0x580, dol::NJ_ROTATE_Y, a)?;
    let a = json!([0, r.ptr(0x58c)?]);
    let p = r.e3(0x594, dol::NJ_CALC_POINT, a)?;
    let a = json!([0, r.ptr(0x5a0)?]);
    let mut v = r.e3(0x5a8, dol::NJ_CALC_VECTOR, a)?;
    r.e(0x5b0, dol::NJ_POP_MATRIX, json!([1]))?;
    let s = r.f(&tw(0x20))?;
    for (k, (rp, c3c, c40, d90, c38)) in [
        (0x5b4, 0x5e0, 0x5f0, 0x600, 0x614),
        (0x630, 0x65c, 0x66c, 0x67c, 0x690),
        (0x6ac, 0x6d8, 0x6e8, 0x6f8, 0x70c),
    ]
    .into_iter()
    .enumerate()
    {
        let j = r.c(d90)? * (r.rnd(rp)? * r.c(c3c)? - r.c(c40)?);
        v[k] = v[k] * (r.c(c38)? + s) + j;
    }
    let f1 = r.c(0x774)? + r.rnd(0x728)? * r.c(0x754)? * r.c(0x764)?;
    spark_spawn_with(r, 0x788, p, v, f1)
}

/// +0x7A0, the fire jet exec: a three-state burner (0 idle, 1 burning,
/// 2 cooling) driven by work +6/+3 timers and the block's height/angle.
fn firziz_exec(r: &mut Run<'_, '_>) -> Result<()> {
    if range_out(r, 0x7c4)? {
        return Ok(());
    }
    let b = blk(r)?;
    let pause = r.gs(0x7d4, 0x37)?;
    let restart = |r: &Run<'_, '_>| -> Result<u16> { Ok(fctiwz(r.f(&tw(0x24))?).wrapping_add(120) as u16) };
    if r.sb(&pause)? == 0 {
        if r.w(&tw(8))? & 1 != 0 || r.sh(&tw(4))? & 4 != 0 {
            let h = r.c(0x80c)?;
            r.setf(key(&b, 4), h);
            r.set(tw(0), 2);
            let t = restart(r)?;
            r.set(tw(6), t);
            r.set(key(&b, 8), 300);
        } else if r.i(&key(&b, 8))? as i32 != 0 {
            let n = (r.i(&key(&b, 8))? as i32).wrapping_sub(1);
            r.set(key(&b, 8), n);
        } else if r.f(&key(&b, 4))? > r.c(0x87c)? {
            let h = r.f(&key(&b, 4))? - r.c(0x898)?;
            r.setf(key(&b, 4), h);
            if h <= r.c(0x8b4)? {
                let h = r.c(0x8cc)?;
                r.setf(key(&b, 4), h);
                r.set(tw(0), 1);
                let t = restart(r)?;
                r.set(tw(6), t);
                r.set(tw(3), 30);
            }
        }
        // The flame collision follows the nozzle model (child of the
        // module object .data+0x4DBF4) raised by the flame height.
        let obj = ptr_name(r, 0x908)?;
        let child = name(&field(r, &obj, 0x2c)?)?;
        let c = r.v3(&child, 8)?;
        let dy = r.via(&tw(0x2c), 0xc)?;
        let dy = r.p(&dy)?.ok_or_else(|| bad("collision info has no shape"))?;
        let z = c[2] + r.f(&key(&b, 4))?;
        r.set3(&dy, 0x38, [c[0], c[1], z]);
    }
    r.e(0x954, dol::CCL_ENTRY, json!(["t"]))?;
    let pause = r.gs(0x95c, 0x37)?;
    if r.sb(&pause)? != 0 {
        return Ok(());
    }
    let ang = key(&b, 0);
    if r.sb(&tw(0))? == 1 {
        let a = r.i(&ang)? as i32;
        if a < 6371 {
            r.set(ang, (a + 145).min(6371));
        } else if r.b(&tw(3))? != 0 {
            let n = r.b(&tw(3))? - 1;
            r.set(tw(3), n);
        } else {
            let clock = r.gs(0x9e8, 0x7c)?;
            let clock = r.w(&clock)?;
            let n = (r.w(&tw(0x10))? & 0xff) + 1;
            if clock % n == 0 {
                firziz_emit(r)?;
            }
            r.e(0xa30, dol::SE_CALL_V2_TIMER, json!([3, "twp", 1, 10, 30, "twp+0x14"]))?;
            let t = r.h(&tw(6))?;
            if t != 0 {
                r.set(tw(6), t - 1);
            } else {
                r.set(tw(0), 2);
                let t = restart(r)?;
                r.set(tw(6), t);
                r.set(tw(3), 30);
            }
        }
    } else {
        let a = r.i(&ang)? as i32;
        if a > 0 {
            r.set(ang, (a - 182).max(0));
        } else if r.c(0xac4)? == r.f(&key(&b, 4))? {
            let t = r.h(&tw(6))?;
            if t != 0 {
                r.set(tw(6), t - 1);
            } else {
                let t = restart(r)?;
                r.set(tw(6), t);
                r.set(tw(0), 1);
            }
        }
    }
    Ok(())
}

/// +0xB34, the fire jet display: base, flame (child, tilted and raised by
/// the height) and nozzle (sibling, turned by the block angle).
fn firziz_display(r: &mut Run<'_, '_>) -> Result<()> {
    let b = blk(r)?;
    let a = json!([r.ptr(0xb58)?]);
    r.e(0xb5c, dol::NJ_SET_TEXTURE, a)?;
    r.e(0xb64, dol::NJ_PUSH_MATRIX, json!([0]))?;
    r.e(0xb70, dol::NJ_TRANSLATE_V, json!([0, pos(r)?]))?;
    let a = json!([0, r.get(&tw(0xc))?]);
    r.e(0xb7c, dol::NJ_ROTATE_Y, a)?;
    let obj = ptr_name(r, 0xb84)?;
    let m = field(r, &obj, 4)?;
    r.e(0xb8c, dol::LATE_DRAW_MODEL, json!([m]))?;
    let child = name(&field(r, &obj, 0x2c)?)?;
    r.e(0xb94, dol::NJ_PUSH_MATRIX_EX, json!([]))?;
    let a = json!([0, r.v3(&child, 8)?]);
    r.e(0xba0, dol::NJ_TRANSLATE_V, a)?;
    r.e(0xbac, dol::NJ_ROTATE_X, json!([0, -4551]))?;
    let a = json!([0, r.c(0xbb8)?, r.c(0xbc4)?, r.f(&key(&b, 4))?]);
    r.e(0xbd4, dol::NJ_TRANSLATE, a)?;
    let m = field(r, &child, 4)?;
    r.e(0xbdc, dol::LATE_DRAW_MODEL, json!([m]))?;
    r.e(0xbe4, dol::NJ_POP_MATRIX, json!([1]))?;
    let sib = name(&field(r, &obj, 0x30)?)?;
    let a = json!([0, r.v3(&sib, 8)?]);
    r.e(0xbf4, dol::NJ_TRANSLATE_V, a)?;
    let a = json!([0, r.get(&key(&b, 0))?]);
    r.e(0xc04, dol::NJ_ROTATE_X, a)?;
    let m = field(r, &sib, 4)?;
    r.e(0xc0c, dol::LATE_DRAW_MODEL, json!([m]))?;
    r.e(0xc14, dol::NJ_POP_MATRIX, json!([1]))
}

/// +0xC34 as a routine: the input gives `p`, `v` (vectors) and `f1`.
fn spark_spawn(r: &mut Run<'_, '_>) -> Result<()> {
    let p = r.v3("arg", 0)?;
    let v = r.v3("arg", 0xc)?;
    let f1 = r.f("arg+0x18")?;
    spark_spawn_with(r, 0, p, v, f1)
}

/// +0xC34: create a spark task (exec +0xF48) at `p` moving by `v`, with a
/// random spin, size `f1` and a random alpha. `from` is the caller's `bl`
/// (0 for a direct run); the created task is reported as `ret`.
fn spark_spawn_with(r: &mut Run<'_, '_>, from: u32, p: [f32; 3], v: [f32; 3], f1: f32) -> Result<()> {
    if from != 0 && r.sr.local(from)? != 0xc34 {
        return Err(bad("spark spawn call"));
    }
    let a = json!([2, 3, r.ptr(0xc64)?, r.ptr(0xc6c)?]);
    let Some(c) = r.ep(0xc70, dol::CREATE_ELEMENTAL_TASK, a)? else {
        r.set("ret", 0);
        return Ok(());
    };
    let cw = format!("{c}.twp");
    r.setf(key(&c, 0x38), f1);
    r.set3(&cw, 0x20, v);
    let spin = |r: &mut Run<'_, '_>, rp: u32, c3c: u32, c40: u32, k: u32, c5c: u32| -> Result<i32> {
        Ok(fctiwz((r.rnd(rp)? * r.c(c3c)? - r.c(c40)?) * r.c(k)? * r.c(c5c)?))
    };
    let a = spin(r, 0xca8, 0xcd4, 0xce4, 0xcf4, 0xd04)?;
    r.set(key(&cw, 0x10), a);
    let a = spin(r, 0xd20, 0xd4c, 0xd5c, 0xd6c, 0xd7c)?;
    r.set(key(&cw, 8), a);
    r.set3(&cw, 0x14, p);
    let a = spin(r, 0xdb4, 0xde0, 0xdf0, 0xe00, 0xe10)?;
    r.set(key(&cw, 0xc), a.wrapping_add(16384));
    let (x, e, d) = (r.ptr(0xe34)?, r.ptr(0xe40)?, r.ptr(0xe4c)?);
    r.set(key(&c, 0x18), x);
    r.set(key(&c, 0x10), e);
    r.set(key(&c, 0x20), d);
    let life = r.rnd(0xe54)? * r.c(0xe80)? * r.c(0xe90)?;
    r.setf(key(&c, 0x3c), life);
    // __cvt_fp2unsigned: a runtime conversion, saturating at 0.
    let u = (r.rnd(0xea0)? * r.c(0xecc)? * r.c(0xedc)?) as u32;
    let alpha = u.wrapping_add(160).min(255);
    r.set(key(&c, 0x40), 0xffff_ff00 | alpha);
    let a = json!([c, r.ptr(0xf10)?, 1, 4]);
    r.e(0xf1c, dol::CCL_INIT, a)?;
    r.set("ret", c);
    Ok(())
}

/// +0xF48, the spark exec: move, spin, grow and fade; destroyed when its
/// life reaches the limit or its collision is hit.
fn spark_exec(r: &mut Run<'_, '_>) -> Result<()> {
    let pause = r.gs(0xf68, 0x37)?;
    if r.sb(&pause)? == 0 {
        let p = pos(r)?;
        let v = r.v3("twp", 0x20)?;
        r.set3("twp", 0x14, [p[0] + v[0], p[1] + v[1], p[2] + v[2]]);
        let spin = r.w(&tw(0x10))?.wrapping_add(r.w(&tw(8))?);
        r.set(tw(0x10), spin);
        let s = r.f(&tk(0x38))? + r.c(0xfc4)?;
        r.setf(tk(0x38), s);
        let v = [
            v[0] * r.c(0xfdc)?,
            r.c(0xff0)? * v[1] + r.c(0x1004)?,
            v[2] * r.c(0x101c)?,
        ];
        r.set3("twp", 0x20, v);
        let l = r.f(&tk(0x3c))? + r.c(0x1034)?;
        r.setf(tk(0x3c), l);
    }
    if r.f(&tk(0x3c))? >= r.c(0x104c)? {
        return r.e(0x1064, dol::DESTROY_TASK, json!(["t"]));
    }
    if r.nz(&tw(0x2c))? {
        let cw = r.via(&tw(0x2c), 4)?;
        if r.h(&cw)? & 1 != 0 {
            r.e(0x1090, dol::DESTROY_TASK, json!(["t"]))?;
        } else {
            r.e(0x109c, dol::CCL_ENTRY, json!(["t"]))?;
        }
    }
    Ok(())
}

/// +0x10B8: clear the spark's size, life and colour.
fn spark_dest(r: &mut Run<'_, '_>) -> Result<()> {
    r.set(tk(0x3c), 0);
    r.set(tk(0x38), 0);
    r.set(tk(0x40), 0);
    Ok(())
}

/// +0x10E4, the spark display: an additive sprite (fn_80033620) without fog.
fn spark_display(r: &mut Run<'_, '_>) -> Result<()> {
    let a = json!([r.ptr(0x1104)?]);
    r.e(0x1108, dol::NJ_SET_TEXTURE, a)?;
    r.e(0x1114, dol::NJ_COLOR_BLENDING_MODE, json!([0, 8]))?;
    r.e(0x1120, dol::NJ_COLOR_BLENDING_MODE, json!([1, 10]))?;
    r.e(0x1138, dol::NJ_DISABLE_FOG, json!([]))?;
    r.e(0x113c, dol::GJ_SET_FOG, json!([]))?;
    let s = r.f(&tk(0x38))?;
    let a = json!([
        "twp+0x14",
        fctiwz(r.f(&tk(0x3c))?),
        r.get(&tw(0xc))?,
        r.get(&tw(0x10))?,
        s,
        s,
        r.get(&tk(0x40))?,
        0,
        r.c(0x1170)?,
        r.c(0x117c)?
    ]);
    r.e(0x1184, dol::FN_80033620, a)?;
    r.e(0x1188, dol::NJ_ENABLE_FOG, json!([]))?;
    r.e(0x118c, dol::GJ_SET_FOG, json!([]))?;
    r.e(0x1198, dol::NJ_COLOR_BLENDING_MODE, json!([0, 8]))?;
    r.e(0x11a4, dol::NJ_COLOR_BLENDING_MODE, json!([1, 6]))
}

// ---------------------------------------------------------------- KAZANI

/// A flock member: 64 bytes at `arr + 64 * i`. Fields: +0 position,
/// +0xC target, +0x18 velocity, +0x24 speed, +0x28/+0x2C/+0x30 angles,
/// +0x34 wing phase, +0x38 retarget timer, +0x3C boost timer.
#[derive(Clone)]
struct Member {
    arr: String,
    base: u32,
}
impl Member {
    fn k(&self, off: u32) -> String {
        key(&self.arr, self.base + off)
    }
    fn v3(&self, r: &Run<'_, '_>, off: u32) -> Result<[f32; 3]> {
        Ok([r.f(&self.k(off))?, r.f(&self.k(off + 4))?, r.f(&self.k(off + 8))?])
    }
    fn set3(&self, r: &mut Run<'_, '_>, off: u32, v: [f32; 3]) {
        for (i, x) in v.into_iter().enumerate() {
            r.setf(self.k(off + 4 * i as u32), x);
        }
    }
}
fn member_arg(r: &Run<'_, '_>) -> Result<Member> {
    let arr = r.p("member")?.ok_or_else(|| bad("member array"))?;
    let i = r.i("index")?;
    let i = u32::try_from(i).map_err(|_| bad("member index"))?;
    Ok(Member { arr, base: 64 * i })
}

/// +0x11C0 on the input's `member`/`index`.
fn flock_member_init(r: &mut Run<'_, '_>) -> Result<()> {
    let m = member_arg(r)?;
    member_init(r, &m)
}

/// +0x11C0: a new target (+0xC) around the flock's scale, a speed boost and
/// a retarget delay.
fn member_init(r: &mut Run<'_, '_>, m: &Member) -> Result<()> {
    let z = (r.rnd(0x11e4)? * r.c(0x1210)? - r.c(0x1220)?) * r.c(0x1230)?;
    let y = (r.rnd(0x1240)? * r.c(0x126c)? - r.c(0x127c)?) * r.c(0x128c)?;
    let x = r.c(0x12a0)?;
    r.e(0x12ac, dol::NJ_PUSH_MATRIX_EX, json!([]))?;
    r.e(0x12b4, dol::NJ_UNIT_MATRIX, json!([0]))?;
    let s = r.v3("twp", 0x20)?;
    r.e(0x12c8, dol::NJ_SCALE, json!([0, s[0], s[1], s[2]]))?;
    let a = fctiwz(r.rnd(0x12cc)? * r.c(0x12f8)? * r.c(0x1308)?);
    let ang = fctiwz(r.cvt(0x1324, a as i64)? * r.c(0x1348)? * r.c(0x1358)?);
    r.e(0x1374, dol::NJ_ROTATE_Y, json!([0, ang]))?;
    let v = r.e3(0x1384, dol::NJ_CALC_VECTOR, json!([0, [x, y, z]]))?;
    r.e(0x138c, dol::NJ_POP_MATRIX, json!([1]))?;
    m.set3(r, 0xc, v);
    let boost = r.rnd(0x13a8)? * r.c(0x13d4)? * r.c(0x13e4)? + r.c(0x13f4)?;
    let sp = r.f(&m.k(0x24))? + boost;
    r.setf(m.k(0x24), sp);
    let t = fctiwz(r.rnd(0x140c)? * r.c(0x1438)? * r.c(0x1448)?);
    r.set(m.k(0x38), t.wrapping_add(120));
    Ok(())
}

/// +0x1480, the flock loader: work +8's low byte + 1 members.
fn flock_load(r: &mut Run<'_, '_>) -> Result<()> {
    if range_out(r, 0x14a8)? {
        return Ok(());
    }
    let Some(b) = r.ep(0x14bc, dol::SY_CALLOC, json!([1, 8]))? else {
        r.set(tk(0x38), 0);
        return Ok(());
    };
    r.set(tk(0x38), b.clone());
    let n = (r.w(&tw(8))? & 0xff) + 1;
    let Some(arr) = r.ep(0x14e0, dol::SY_CALLOC, json!([n, 64]))? else {
        r.set(key(&b, 0), 0);
        r.e(0x1500, dol::SY_FREE, json!([b]))?;
        r.set(tk(0x38), 0);
        return Ok(());
    };
    r.set(key(&b, 0), arr.clone());
    r.set(key(&b, 4), n);
    let s = r.v3("twp", 0x20)?;
    for i in 0..n {
        let m = Member { arr: arr.clone(), base: 64 * i };
        if r.sr.local(0x153c)? != 0x11c0 {
            return Err(bad("flock init call"));
        }
        member_init(r, &m)?;
        let mut p = [0.0; 3];
        for (k, (rp, a, b2, c)) in [(0x1540, 0x156c, 0x157c, 0x158c), (0x15a4, 0x15d0, 0x15e0, 0x15f0), (0x1608, 0x1634, 0x1644, 0x1654)]
            .into_iter()
            .enumerate()
        {
            p[k] = s[k] * ((r.rnd(rp)? * r.c(a)? - r.c(b2)?) * r.c(c)?);
        }
        m.set3(r, 0, p);
        let mut v = [0.0; 3];
        for (k, (rp, a, b2, c, d)) in [
            (0x166c, 0x1698, 0x16a8, 0x16b8, 0x16c8),
            (0x16d8, 0x1704, 0x1714, 0x1724, 0x1734),
            (0x1744, 0x1770, 0x1780, 0x1790, 0x17a0),
        ]
        .into_iter()
        .enumerate()
        {
            v[k] = (r.rnd(rp)? * r.c(a)? - r.c(b2)?) * r.c(c)? * r.c(d)?;
        }
        m.set3(r, 0x18, v);
        let a = fctiwz(r.rnd(0x17b0)? * r.c(0x17dc)?);
        let f2 = r.c(0x182c)? + r.cvt(0x17f8, a as i64)? * r.c(0x181c)?;
        let old = r.i(&m.k(0x3c))?;
        let t = fctiwz(r.cvt(0x1840, old)? + f2);
        r.set(m.k(0x3c), t);
        let sp = r.f(&m.k(0x24))? + r.rnd(0x1874)? * r.c(0x18a0)? * r.c(0x18b0)?;
        r.setf(m.k(0x24), sp);
    }
    let (d, e, x) = (r.ptr(0x18e4)?, r.ptr(0x18f0)?, r.ptr(0x18fc)?);
    r.set(tk(0x14), d);
    r.set(tk(0x10), e);
    r.set(tk(0x18), x);
    r.set(tw(6), 0);
    Ok(())
}

/// +0x192C: free the members and the block.
fn flock_dest(r: &mut Run<'_, '_>) -> Result<()> {
    let b = blk(r)?;
    if r.nz(&key(&b, 0))? {
        let a = r.get(&key(&b, 0))?;
        r.e(0x1960, dol::SY_FREE, json!([a]))?;
    }
    r.set(tk(0x40), 0);
    r.e(0x1970, dol::SY_FREE, json!([b]))?;
    r.set(tk(0x38), 0);
    Ok(())
}

/// +0x1990 on the input's `member`/`index`.
fn flock_steer(r: &mut Run<'_, '_>) -> Result<()> {
    let m = member_arg(r)?;
    steer(r, &m)
}

/// +0x1990: turn toward the target at most 8192 per step (banking +0x30 by
/// half the turn), or let the bank decay, then accelerate along the
/// heading (climbs damped by one rate, dives by another).
fn steer(r: &mut Run<'_, '_>, m: &Member) -> Result<()> {
    let v = m.v3(r, 0x18)?;
    let p = m.v3(r, 0)?;
    let t = m.v3(r, 0xc)?;
    let mut d = [t[0] - p[0], t[1] - p[1], t[2] - p[2]];
    if v[0] * v[0] + v[2] * v[2] > r.c(0x19d0)? {
        let a1 = r.ef(0x1a3c, dol::ATAN2F, json!([d[0], d[2]]))?;
        let r19 = fctiwz((r.cd(0x1a44)? * a1 as f64) as f32);
        let a2 = r.ef(0x1a64, dol::ATAN2F, json!([v[0], v[2]]))?;
        let r18 = fctiwz((r.cd(0x1a6c)? * a2 as f64) as f32);
        let mut turn = r19.wrapping_sub(r18) as i16;
        if !(-8192..=8192).contains(&turn) {
            turn = turn.clamp(-8192, 8192);
            r.e(0x1ac0, dol::NJ_PUSH_MATRIX_EX, json!([]))?;
            r.e(0x1ac8, dol::NJ_UNIT_MATRIX, json!([0]))?;
            let a = r18.wrapping_add(turn as i32).wrapping_sub(r19);
            r.e(0x1adc, dol::NJ_ROTATE_Y, json!([0, a]))?;
            d = r.e3(0x1aec, dol::NJ_CALC_VECTOR, json!([0, d]))?;
            r.e(0x1af4, dol::NJ_POP_MATRIX, json!([1]))?;
        }
        r.set(m.k(0x30), -(turn as i32 / 2));
    } else {
        let bank = r.w(&m.k(0x30))? as u16 as i16;
        let b = fctiwz(r.cvt(0x1b74, bank as i64)? * r.c(0x1b98)?);
        r.set(m.k(0x30), b as i16 as i32);
    }
    let len = r.ef(0x1bbc, dol::NJ_SCALOR, json!([d]))?;
    if len > r.c(0x1bc8)? {
        let s = r.f(&m.k(0x24))? / len;
        for x in &mut d {
            *x *= s;
        }
        d[1] *= if d[1] > r.c(0x1c20)? { r.c(0x1c38)? } else { r.c(0x1c54)? };
        m.set3(r, 0x18, [v[0] + d[0], v[1] + d[1], v[2] + d[2]]);
    }
    Ok(())
}

/// +0x1CD4, the flock exec (at most 21 members kept in work +8).
fn flock_exec(r: &mut Run<'_, '_>) -> Result<()> {
    if range_out(r, 0x1d08)? {
        return Ok(());
    }
    let n = (r.w(&tw(8))? & 0xff) as i32;
    r.set(tw(8), n.min(20));
    let pause = r.gs(0x1d38, 0x37)?;
    if r.sb(&pause)? != 0 {
        return Ok(());
    }
    let b = blk(r)?;
    let arr = r.p(&key(&b, 0))?.ok_or_else(|| bad("flock has no members"))?;
    let count = r.i(&key(&b, 4))? as i32;
    for i in 0..count.max(0) as u32 {
        let m = Member { arr: arr.clone(), base: 64 * i };
        let save = [r.i(&m.k(0x28))? as i32, r.i(&m.k(0x2c))? as i32, r.i(&m.k(0x30))? as i32];
        let mut f31 = r.f(&m.k(0x24))? * r.c(0x1d7c)?;
        if f31 < r.c(0x1d8c)? && r.f(&m.k(0x34))? > r.c(0x1da4)? {
            f31 = r.c(0x1db8)?;
        }
        let mut ph = r.f(&m.k(0x34))? + f31;
        while ph > r.c(0x1df0)? {
            ph -= r.c(0x1dd8)?;
        }
        r.setf(m.k(0x34), ph);
        let p = m.v3(r, 0)?;
        let v = m.v3(r, 0x18)?;
        let t = m.v3(r, 0xc)?;
        let q = [v[0] + p[0], v[1] + p[1], v[2] + p[2]];
        let d1 = r.ef(0x1e5c, dol::NJ_DISTANCE_P2P, json!([p, t]))?;
        let d2 = r.ef(0x1e6c, dol::NJ_DISTANCE_P2P, json!([q, t]))?;
        let closing = -d2 + d1;
        if r.i(&m.k(0x3c))? as i32 != 0 {
            let sp = r.f(&m.k(0x24))? + r.c(0x1e8c)?;
            r.setf(m.k(0x24), sp);
            let n = r.i(&m.k(0x3c))? as i32 - 1;
            r.set(m.k(0x3c), n);
        }
        let mut boosted = false;
        if closing < r.c(0x1eac)? && r.rnd(0x1ebc)? * r.c(0x1ee8)? < r.c(0x1ef8)? && r.i(&m.k(0x3c))? as i32 == 0 {
            let t = fctiwz(r.rnd(0x1f14)? * r.c(0x1f40)? * r.c(0x1f50)?);
            r.set(m.k(0x3c), t.wrapping_add(6));
            boosted = true;
        }
        if !boosted {
            let v = m.v3(r, 0x18)?;
            let k = [r.c(0x1f88)?, r.c(0x1fa8)?, r.c(0x1fc0)?];
            let y = v[1] * k[1] * r.c(0x1fd8)?;
            m.set3(r, 0x18, [v[0] * k[0], y, v[2] * k[2]]);
        }
        let y = r.f(&m.k(0x1c))? + r.c(0x1ff0)?;
        r.setf(m.k(0x1c), y);
        if r.sr.local(0x2008)? != 0x1990 {
            return Err(bad("flock steer call"));
        }
        steer(r, &m)?;
        let sp = r.f(&m.k(0x24))? * r.c(0x2014)?;
        r.setf(m.k(0x24), sp);
        let p = m.v3(r, 0)?;
        let mut v = m.v3(r, 0x18)?;
        m.set3(r, 0, [p[0] + v[0], p[1] + v[1], p[2] + v[2]]);
        let len = r.ef(0x2078, dol::NJ_SCALOR, json!([v]))?;
        if len > r.c(0x2084)? {
            let s = r.c(0x2098)? / len;
            for x in &mut v {
                *x *= s;
            }
            m.set3(r, 0x18, v);
        }
        let t = m.v3(r, 0xc)?;
        let p = m.v3(r, 0)?;
        let d3 = r.ef(0x20e4, dol::NJ_DISTANCE_P2P, json!([t, p]))?;
        let mut reinit = d3 < r.c(0x20ec)?;
        if !reinit {
            let old = r.i(&m.k(0x38))? as i32;
            r.set(m.k(0x38), old.wrapping_sub(1));
            reinit = old <= 0;
        }
        if reinit {
            if r.sr.local(0x2118)? != 0x11c0 {
                return Err(bad("flock init call"));
            }
            member_init(r, &m)?;
            let t = fctiwz(r.rnd(0x211c)? * r.c(0x2148)? * r.c(0x2158)?);
            r.set(m.k(0x3c), t as i8 as i32);
        }
        let v = m.v3(r, 0x18)?;
        let len = r.ef(0x217c, dol::NJ_SCALOR, json!([v]))?;
        if len > r.c(0x2184)? {
            let h = sqrtf(v[0] * v[0] + v[2] * v[2]);
            let a = r.ef(0x21c0, dol::ATAN2F, json!([v[1], h]))?;
            let pitch = fctiwz((r.cd(0x21c8)? * a as f64) as f32);
            let a = r.ef(0x21ec, dol::ATAN2F, json!([v[0], v[2]]))?;
            r.set(m.k(0x2c), fctiwz((r.cd(0x21f4)? * a as f64) as f32));
            r.set(m.k(0x28), (pitch as i16).clamp(-5461, 5461) as i32);
        }
        // Ease each angle toward its new value: pitch and yaw clamped and
        // scaled, bank only clamped.
        for (k, lim, cvt, rate) in [(0x28, 1456i16, Some((0x2278, 0x229c)), 0), (0x2c, 1092, Some((0x2300, 0x2328)), 1), (0x30, 1456, None, 2)] {
            let cur = r.i(&m.k(k))? as i32;
            let mut dlt = (cur.wrapping_sub(save[rate]) as i16).clamp(-lim, lim);
            if let Some((cv, c)) = cvt {
                dlt = fctiwz(r.cvt(cv, dlt as i64)? * r.c(c)?) as i16;
            }
            r.set(m.k(k), save[rate].wrapping_add(dlt as i32));
        }
    }
    Ok(())
}

/// +0x23D0, the flock display: each visible member (+0xA770) with wing
/// flap angles from the clock.
fn flock_display(r: &mut Run<'_, '_>) -> Result<()> {
    r.e(0x23ec, dol::NJ_PUSH_MATRIX_EX, json!([]))?;
    r.e(0x23f4, dol::NJ_TRANSLATE_EX, json!([pos(r)?]))?;
    let a = json!([0, r.get(&tw(0xc))?]);
    r.e(0x2400, dol::NJ_ROTATE_Y, a)?;
    let a = json!([r.ptr(0x2408)?]);
    r.e(0x240c, dol::NJ_SET_TEXTURE, a)?;
    r.e(0x2410, dol::NJ_PUSH_MATRIX_EX, json!([]))?;
    let b = blk(r)?;
    let count = r.i(&key(&b, 4))? as i32;
    if count > 0 {
        let arr = r.p(&key(&b, 0))?.ok_or_else(|| bad("flock has no members"))?;
        let obj = ptr_name(r, 0x2458)?;
        let ob = r.v3(&obj, 8)?;
        for i in 0..count as u32 {
            let m = Member { arr: arr.clone(), base: 64 * i };
            let p = m.v3(r, 0)?;
            let a = json!([p, r.c(0x242c)?]);
            if r.mr(0x2434, a)? == 0 {
                continue;
            }
            let clock = r.gs(0x2444, 0x7c)?;
            let ph = r.w(&clock)?.wrapping_add(i.wrapping_mul(55));
            let s = r.ef(0x2460, dol::NJ_SIN, json!([(ph << 8) as i32]))?;
            let r26 = fctiwz(r.c(0x2468)? * s * r.c(0x2478)?);
            let s = r.ef(0x2494, dol::NJ_SIN, json!([ph.wrapping_mul(224) as i32]))?;
            let r27 = fctiwz(r.c(0x249c)? * s * r.c(0x24ac)?);
            r.e(0x24c8, dol::NJ_TRANSLATE_EX, json!([p]))?;
            let s = r.ef(0x24d4, dol::NJ_SIN, json!([(ph << 8).wrapping_add(16384) as i32]))?;
            let y = ob[1] + r.c(0x24dc)? * s;
            r.e(0x24fc, dol::NJ_TRANSLATE, json!([0, ob[0], y, ob[2]]))?;
            let a = json!([0, r.get(&m.k(0x2c))?]);
            r.e(0x2508, dol::NJ_ROTATE_Y, a)?;
            let x = (r.i(&m.k(0x28))? as i32).wrapping_neg().wrapping_add(r26).wrapping_sub(512);
            r.e(0x2520, dol::NJ_ROTATE_X, json!([0, x]))?;
            let z = (r.i(&m.k(0x30))? as i32).wrapping_add(r27);
            r.e(0x2530, dol::NJ_ROTATE_Z, json!([0, z]))?;
            r.e(0x2550, dol::NJ_TRANSLATE, json!([0, -ob[0], -ob[1], -ob[2]]))?;
            let md = field(r, &obj, 4)?;
            r.e(0x2558, dol::LATE_DRAW_MODEL, json!([md]))?;
            r.e(0x255c, dol::NJ_POP_MATRIX_EX, json!([]))?;
            r.e(0x2560, dol::NJ_PUSH_MATRIX_EX, json!([]))?;
        }
    }
    r.e(0x2580, dol::NJ_POP_MATRIX, json!([2]))
}

// ------------------------------------------------- rail-piece constructors

/// The two copies of the rail-piece constructor: +0x2720 (callbacks
/// +0x2A64/+0x2B30/+0x2BBC/+0x2A08) and +0x4834 (+0x4B7C/+0x4D80/+0x4C8C/
/// +0x4B20, and it stops when there is no template). `pc` maps an offset of
/// the first copy to the same instruction of the other.
#[derive(Clone, Copy)]
struct Piece {
    second: bool,
}
impl Piece {
    fn pc(self, a: u32) -> u32 {
        match (self.second, a >= 0x2800) {
            (false, _) => a,
            (true, false) => a + 0x2114,
            (true, true) => a + 0x211c,
        }
    }
}

/// Constructor arguments: texture, model (or object), template, lit flag,
/// collision flags, collision info and its count.
struct PieceArgs {
    tex: Value,
    model: Value,
    template: Value,
    lit: bool,
    flags: u32,
    ccl: Value,
    ccl_n: i64,
}
fn piece_args_input(r: &Run<'_, '_>) -> Result<PieceArgs> {
    Ok(PieceArgs {
        tex: r.get("args+0x0")?,
        model: r.get("args+0x4")?,
        template: r.get("args+0x8")?,
        lit: r.nz("args+0xc")?,
        flags: r.w("args+0x10")?,
        ccl: r.get("args+0x14")?,
        ccl_n: r.i("args+0x18")?,
    })
}
fn piece_load_a(r: &mut Run<'_, '_>) -> Result<()> {
    let a = piece_args_input(r)?;
    piece_load(r, Piece { second: false }, a)
}
fn piece_load_b(r: &mut Run<'_, '_>) -> Result<()> {
    let a = piece_args_input(r)?;
    piece_load(r, Piece { second: true }, a)
}
/// A loader that only passes module data to a constructor: decode r4..r10
/// from its `lis`/`addi` and `li` instructions.
fn thin(r: &mut Run<'_, '_>, call: u32, second: bool, regs: [(u32, Option<u32>); 7]) -> Result<()> {
    let want = if second { 0x4834 } else { 0x2720 };
    if r.sr.local(call)? != want {
        return Err(bad("rail piece constructor call"));
    }
    let mut v = Vec::new();
    for (imm, pc) in regs {
        v.push(match pc {
            Some(pc) => r.ptr(pc)?,
            None => json!(imm),
        });
    }
    let a = PieceArgs {
        tex: v[0].clone(),
        model: v[1].clone(),
        template: v[2].clone(),
        lit: v[3] != json!(0),
        flags: v[4].as_u64().unwrap_or(0) as u32,
        ccl: v[5].clone(),
        ccl_n: v[6].as_i64().unwrap_or(0),
    };
    piece_load(r, Piece { second }, a)
}
/// +0x2660 (RAILOBJA).
fn railobja_load(r: &mut Run<'_, '_>) -> Result<()> {
    thin(r, 0x26a8, false, [(0, Some(0x2684)), (0, Some(0x268c)), (0, Some(0x2694)), (1, None), (0, None), (0, None), (0, None)])
}
/// +0x26C0 (MOASIBA, MOASIBAF).
fn moasiba_load(r: &mut Run<'_, '_>) -> Result<()> {
    thin(r, 0x2708, false, [(0, Some(0x26e4)), (0, Some(0x26ec)), (0, Some(0x26f4)), (1, None), (0, None), (0, None), (0, None)])
}
/// +0x4710 (MOTOOL, MOTOOLF).
fn motool_load(r: &mut Run<'_, '_>) -> Result<()> {
    thin(r, 0x475c, true, [(0, Some(0x4734)), (0, Some(0x473c)), (0, Some(0x4744)), (1, None), (0, None), (0, Some(0x4754)), (1, None)])
}
/// +0x4774 (RAILOBJBR): no template, so only the callbacks are set.
fn railobjbr_load(r: &mut Run<'_, '_>) -> Result<()> {
    thin(r, 0x47bc, true, [(0, Some(0x4798)), (0, Some(0x47a0)), (0, None), (1, None), (0, None), (0, Some(0x47b4)), (1, None)])
}
/// +0x47D4 (RAILOBJBL).
fn railobjbl_load(r: &mut Run<'_, '_>) -> Result<()> {
    thin(r, 0x481c, true, [(0, Some(0x47f8)), (0, Some(0x4800)), (0, None), (1, None), (0, None), (0, Some(0x4814)), (1, None)])
}

/// +0x2720 / +0x4834: allocate the 76-byte dynamic collision block, copy the
/// 14-word template object into it at the task's place, register it with a
/// size class from the model radius and set up collision.
fn piece_load(r: &mut Run<'_, '_>, pc: Piece, a: PieceArgs) -> Result<()> {
    if r.sb(&tw(1))? == 0 && range_out(r, pc.pc(0x2770))? {
        return Ok(());
    }
    let Some(d) = r.ep(pc.pc(0x2784), dol::SY_CALLOC, json!([1, 76]))? else {
        r.set(tk(0x38), 0);
        return Ok(());
    };
    r.set(tk(0x38), d.clone());
    r.set(key(&d, 72), a.model.clone());
    r.set(key(&d, 68), a.tex);
    if a.model != json!(0) {
        let disp = r.ptr(pc.pc(if a.lit { 0x27d0 } else { 0x27e0 }))?;
        r.set(tk(0x14), disp);
    }
    let (e, x) = (r.ptr(pc.pc(0x27ec))?, r.ptr(pc.pc(0x27f8))?);
    r.set(tk(0x10), e);
    r.set(tk(0x18), x);
    let tpl = match &a.template {
        Value::String(s) => s.clone(),
        _ if pc.second => return Ok(()),
        _ => return Err(bad("rail piece without a template")),
    };
    for k in 0..14 {
        let v = r.get(&key(&tpl, 4 * k))?;
        r.set(key(&d, 4 * k), v);
    }
    let w0 = r.w(&key(&d, 0))?;
    r.set(key(&d, 0), w0 & !3);
    for (to, from) in [(24, 12), (28, 16), (20, 8)] {
        let v = r.get(&tw(from))?;
        r.set(key(&d, to), v);
    }
    let rot = |r: &Run<'_, '_>| -> Result<[Value; 3]> { Ok([r.get(&tw(16))?, r.get(&tw(12))?, r.get(&tw(8))?]) };
    r.e(pc.pc(0x289c), dol::NJ_PUSH_MATRIX_EX, json!([]))?;
    r.e(pc.pc(0x28a4), dol::NJ_UNIT_MATRIX, json!([0]))?;
    r.e(pc.pc(0x28b0), dol::NJ_TRANSLATE_V, json!([0, pos(r)?]))?;
    let [z, y, x] = rot(r)?;
    r.e(pc.pc(0x28bc), dol::NJ_ROTATE_Z, json!([0, z]))?;
    r.e(pc.pc(0x28c8), dol::NJ_ROTATE_Y, json!([0, y]))?;
    r.e(pc.pc(0x28d4), dol::NJ_ROTATE_X, json!([0, x]))?;
    let v = r.v3(&d, 8)?;
    let p = r.e3(pc.pc(0x28e4), dol::NJ_CALC_POINT, json!([0, v]))?;
    r.set3(&d, 8, p);
    r.e(pc.pc(0x28ec), dol::NJ_POP_MATRIX, json!([1]))?;
    let model = name(&r.get(&key(&d, 4))?)?;
    let rad = r.c(pc.pc(0x28fc))? + r.f(&key(&model, 36))?;
    let mut flags = a.flags | 1;
    if rad > r.c(pc.pc(0x2914))? {
    } else if rad > r.c(pc.pc(0x292c))? {
        flags |= 0x2000_0000;
    } else if rad > r.c(pc.pc(0x2948))? {
        flags |= 0x4000_0000;
    } else {
        flags |= 0x6000_0000;
    }
    r.e(pc.pc(0x2970), dol::FN_800232A4, json!([flags, "t", d]))?;
    if a.ccl != json!(0) {
        r.e(pc.pc(0x298c), dol::CCL_INIT, json!(["t", a.ccl, a.ccl_n, 4]))?;
    }
    r.e(pc.pc(0x2990), dol::NJ_PUSH_MATRIX_EX, json!([]))?;
    r.e(pc.pc(0x2998), dol::NJ_UNIT_MATRIX, json!([0]))?;
    r.e(pc.pc(0x29a4), dol::NJ_TRANSLATE_V, json!([0, p]))?;
    let [z, y, x] = rot(r)?;
    r.e(pc.pc(0x29b0), dol::NJ_ROTATE_Z, json!([0, z]))?;
    r.e(pc.pc(0x29bc), dol::NJ_ROTATE_Y, json!([0, y]))?;
    r.e(pc.pc(0x29c8), dol::NJ_ROTATE_X, json!([0, x]))?;
    let c = r.v3(&model, 24)?;
    let q = r.e3(pc.pc(0x29e0), dol::NJ_CALC_POINT, json!([0, c]))?;
    r.set3(&d, 56, q);
    r.e(pc.pc(0x29e8), dol::NJ_POP_MATRIX, json!([1]))
}

/// +0x2A08 / +0x4B20: unregister and free the collision block.
fn piece_dest(r: &mut Run<'_, '_>, base: u32) -> Result<()> {
    let d = blk(r)?;
    if r.nz(&key(&d, 4))? {
        r.e(base + 0x34, dol::FN_800231CC, json!(["t", d]))?;
    }
    r.e(base + 0x3c, dol::SY_FREE, json!([d]))?;
    r.set(tk(0x38), 0);
    Ok(())
}
fn piece_dest_a(r: &mut Run<'_, '_>) -> Result<()> {
    piece_dest(r, 0x2a08)
}
fn piece_dest_b(r: &mut Run<'_, '_>) -> Result<()> {
    piece_dest(r, 0x4b20)
}

/// The shared middle of +0x2A64 and +0x4B7C; false when range-out stopped it.
fn piece_tick(r: &mut Run<'_, '_>, range: u32, cpc: u32, within: u32, ccl: u32) -> Result<bool> {
    if r.sb(&tw(1))? == 0 && range_out(r, range)? {
        return Ok(false);
    }
    let d = blk(r)?;
    if r.nz(&key(&d, 4))? {
        let model = name(&r.get(&key(&d, 4))?)?;
        let rad = r.c(cpc)? + r.f(&key(&model, 36))?;
        let c = r.v3(&d, 56)?;
        let hit = r.er(within, dol::EITHER_PLAYER_WITHIN_SPHERE, json!([c, rad]))? != 0;
        near_flag(r, hit)?;
    }
    if r.nz(&tw(0x2c))? {
        r.e(ccl, dol::CCL_ENTRY, json!(["t"]))?;
    }
    Ok(true)
}
/// +0x2A64: flag nearby players, feed collision.
fn piece_exec_a(r: &mut Run<'_, '_>) -> Result<()> {
    piece_tick(r, 0x2a98, 0x2ac4, 0x2ad8, 0x2b10)?;
    Ok(())
}
/// +0x4B7C: the same, then spin work +6 by work +0x20 times a rate.
fn spinner_exec(r: &mut Run<'_, '_>) -> Result<()> {
    if !piece_tick(r, 0x4bb0, 0x4bdc, 0x4bf0, 0x4c28)? {
        return Ok(());
    }
    let pause = r.gs(0x4c30, 0x37)?;
    if r.sb(&pause)? == 0 {
        let d = fctiwz(r.c(0x4c4c)? * r.f(&tw(0x20))?);
        let a = r.h(&tw(6))?.wrapping_add(d as u16);
        r.set(tw(6), a);
    }
    Ok(())
}

fn draw_placed(r: &mut Run<'_, '_>, push: u32, x: u32, model: Value, draw: u32) -> Result<()> {
    r.e(push, dol::NJ_PUSH_MATRIX, json!([0]))?;
    r.e(push + 0xc, dol::NJ_TRANSLATE_V, json!([0, pos(r)?]))?;
    let a = json!([0, r.get(&tw(16))?]);
    r.e(push + 0x18, dol::NJ_ROTATE_Z, a)?;
    let a = json!([0, r.get(&tw(12))?]);
    r.e(push + 0x24, dol::NJ_ROTATE_Y, a)?;
    let a = json!([0, r.get(&tw(8))?]);
    r.e(x, dol::NJ_ROTATE_X, a)?;
    r.e(draw, dol::LATE_DRAW_MODEL, json!([model]))
}
/// +0x2B30: draw the piece model.
fn piece_display(r: &mut Run<'_, '_>) -> Result<()> {
    let d = blk(r)?;
    let t = r.get(&key(&d, 68))?;
    r.e(0x2b54, dol::NJ_SET_TEXTURE, json!([t]))?;
    let m = r.get(&key(&d, 72))?;
    draw_placed(r, 0x2b5c, 0x2b8c, m, 0x2b98)?;
    r.e(0x2ba0, dol::NJ_POP_MATRIX, json!([1]))
}
/// +0x2BBC: the same with control flag 0x2400 (lit) set around it.
fn piece_display_lit(r: &mut Run<'_, '_>) -> Result<()> {
    let d = blk(r)?;
    let t = r.get(&key(&d, 68))?;
    r.e(0x2be0, dol::NJ_SET_TEXTURE, json!([t]))?;
    r.e(0x2be8, dol::ON_CONTROL_3D, json!([0x2400]))?;
    let m = r.get(&key(&d, 72))?;
    draw_placed(r, 0x2bf0, 0x2c20, m, 0x2c2c)?;
    r.e(0x2c34, dol::NJ_POP_MATRIX, json!([1]))?;
    r.e(0x2c3c, dol::OFF_CONTROL_3D, json!([0x2400]))
}
/// +0x4C8C / +0x4D80: the object, then every child turned by work +6 about
/// X (or Z) and by its own Y angle.
fn spinner_display(r: &mut Run<'_, '_>, base: u32, axis: dol::F) -> Result<()> {
    let at = |o: u32| base + o - 0x4c8c;
    let d = blk(r)?;
    let obj = name(&r.get(&key(&d, 72))?)?;
    let t = r.get(&key(&d, 68))?;
    r.e(at(0x4cbc), dol::NJ_SET_TEXTURE, json!([t]))?;
    r.e(at(0x4cc4), dol::ON_CONTROL_3D, json!([0x2400]))?;
    let m = field(r, &obj, 4)?;
    draw_placed(r, at(0x4ccc), at(0x4cfc), m, at(0x4d04))?;
    r.e(at(0x4d08), dol::NJ_PUSH_MATRIX_EX, json!([]))?;
    let mut c = field(r, &obj, 0x2c)?;
    let mut n = 0;
    while let Value::String(node) = c {
        n += 1;
        if n > 256 {
            return Err(bad("object chain too long"));
        }
        let a = json!([r.v3(&node, 8)?]);
        r.e(at(0x4d1c), dol::NJ_TRANSLATE_EX, a)?;
        let a = json!([0, r.h(&tw(6))?]);
        r.e(at(0x4d28), axis, a)?;
        let a = json!([0, r.get(&key(&node, 24))?]);
        r.e(at(0x4d34), dol::NJ_ROTATE_Y, a)?;
        let m = field(r, &node, 4)?;
        r.e(at(0x4d3c), dol::LATE_DRAW_MODEL, json!([m]))?;
        r.e(at(0x4d40), dol::NJ_POP_MATRIX_EX, json!([]))?;
        r.e(at(0x4d44), dol::NJ_PUSH_MATRIX_EX, json!([]))?;
        c = field(r, &node, 0x30)?;
    }
    r.e(at(0x4d58), dol::NJ_POP_MATRIX, json!([2]))?;
    r.e(at(0x4d60), dol::OFF_CONTROL_3D, json!([0x2400]))
}
fn spinner_display_x(r: &mut Run<'_, '_>) -> Result<()> {
    spinner_display(r, 0x4c8c, dol::NJ_ROTATE_X)
}
fn spinner_display_z(r: &mut Run<'_, '_>) -> Result<()> {
    spinner_display(r, 0x4d80, dol::NJ_ROTATE_Z)
}

// ---------------------------------------------------------------- MOVENT

/// +0x2C58: a rotating block with a random phase and spin rate.
fn movent_load(r: &mut Run<'_, '_>) -> Result<()> {
    if range_out(r, 0x2c84)? {
        return Ok(());
    }
    let Some(d) = r.ep(0x2c98, dol::SY_CALLOC, json!([1, 56]))? else {
        r.set(tk(0x38), 0);
        return Ok(());
    };
    r.set(tk(0x38), d.clone());
    let (a, e, x) = (r.ptr(0x2cb0)?, r.ptr(0x2cbc)?, r.ptr(0x2cc8)?);
    r.set(tk(0x14), a);
    r.set(tk(0x10), e);
    r.set(tk(0x18), x);
    r.set(tw(1), 0);
    let ph = fctiwz(r.rnd(0x2cd8)? * r.c(0x2d04)? * r.c(0x2d14)? * r.c(0x2d24)?);
    r.set(tw(6), ph as u16);
    let sp = fctiwz(r.rnd(0x2d44)? * r.c(0x2d70)? * r.c(0x2d80)? * r.c(0x2d90)?);
    let (s, o) = r.sr.at(0x2dac)?;
    let base = r.sr.word(s, o)? as i32;
    r.set(tk(0x40), base.wrapping_add(sp));
    let tpl = ptr_name(r, 0x2dc4)?;
    for k in 0..14 {
        let v = r.get(&key(&tpl, 4 * k))?;
        r.set(key(&d, 4 * k), v);
    }
    let w0 = r.w(&key(&d, 0))?;
    r.set(key(&d, 0), w0 & !3);
    let (y, z) = (r.get(&tw(12))?, r.get(&tw(16))?);
    r.set(key(&d, 24), y);
    r.set(key(&d, 28), z);
    r.set(key(&d, 20), 0);
    let p = pos(r)?;
    r.set3(&d, 8, p);
    let model = name(&r.get(&key(&d, 4))?)?;
    let rad = r.c(0x2e7c)? + r.f(&key(&model, 36))?;
    let mut flags = 129u32;
    if rad > r.c(0x2e94)? {
    } else if rad > r.c(0x2eac)? {
        flags |= 0x2000_0000;
    } else if rad > r.c(0x2ec8)? {
        flags |= 0x4000_0000;
    } else {
        flags |= 0x6000_0000;
    }
    r.e(0x2ef0, dol::FN_800232A4, json!([flags, "t", d]))
}
/// +0x2F18.
fn movent_dest(r: &mut Run<'_, '_>) -> Result<()> {
    let d = blk(r)?;
    if r.nz(&key(&d, 4))? {
        r.e(0x2f4c, dol::FN_800231CC, json!(["t", d]))?;
    }
    r.set(tk(0x40), 0);
    r.e(0x2f5c, dol::SY_FREE, json!([d]))?;
    r.set(tk(0x38), 0);
    Ok(())
}
/// +0x2F7C: spin work +6 by t+0x40, flag nearby players.
fn movent_exec(r: &mut Run<'_, '_>) -> Result<()> {
    if range_out(r, 0x2f9c)? {
        return Ok(());
    }
    let pause = r.gs(0x2fac, 0x37)?;
    if r.sb(&pause)? == 0 {
        let a = r.h(&tw(6))?.wrapping_add(r.w(&tk(0x40))? as u16);
        r.set(tw(6), a);
    }
    let d = blk(r)?;
    if r.nz(&key(&d, 4))? {
        let model = name(&r.get(&key(&d, 4))?)?;
        let rad = r.c(0x2fec)? + r.f(&key(&model, 36))?;
        let c = r.v3(&d, 8)?;
        let hit = r.er(0x3004, dol::EITHER_PLAYER_WITHIN_SPHERE, json!([c, rad]))? != 0;
        near_flag(r, hit)?;
    }
    Ok(())
}
/// +0x3044: base, then the child turned about Z by work +6.
fn movent_display(r: &mut Run<'_, '_>) -> Result<()> {
    let a = json!([r.ptr(0x3068)?]);
    r.e(0x306c, dol::NJ_SET_TEXTURE, a)?;
    let obj = ptr_name(r, 0x3074)?;
    let m = field(r, &obj, 4)?;
    r.e(0x307c, dol::NJ_PUSH_MATRIX, json!([0]))?;
    r.e(0x3088, dol::NJ_TRANSLATE_V, json!([0, pos(r)?]))?;
    let a = json!([0, r.get(&tw(16))?]);
    r.e(0x3094, dol::NJ_ROTATE_Z, a)?;
    let a = json!([0, r.get(&tw(12))?]);
    r.e(0x30a0, dol::NJ_ROTATE_Y, a)?;
    let a = json!([0, r.get(&tw(8))?]);
    r.e(0x30ac, dol::NJ_ROTATE_X, a)?;
    r.e(0x30b4, dol::LATE_DRAW_MODEL, json!([m]))?;
    let c = name(&field(r, &obj, 0x2c)?)?;
    let a = json!([0, r.v3(&c, 8)?]);
    r.e(0x30c4, dol::NJ_TRANSLATE_V, a)?;
    let a = json!([0, r.h(&tw(6))?]);
    r.e(0x30d0, dol::NJ_ROTATE_Z, a)?;
    let m = field(r, &c, 4)?;
    r.e(0x30d8, dol::LATE_DRAW_MODEL, json!([m]))?;
    r.e(0x30e0, dol::NJ_POP_MATRIX, json!([1]))
}

// ---------------------------------------------------------------- PNP

/// +0x30FC: a 20-byte block (+0 turn, +4 spin speed, +8 rest height,
/// +0xC bounce speed, +0x10 bounce step); the pad starts raised.
fn pnp_load(r: &mut Run<'_, '_>) -> Result<()> {
    if range_out(r, 0x311c)? {
        return Ok(());
    }
    let Some(d) = r.ep(0x3130, dol::SY_CALLOC, json!([1, 20]))? else {
        r.set(tk(0x38), 0);
        return Ok(());
    };
    r.set(tk(0x38), d.clone());
    r.set(tw(1), 0);
    let a = json!(["t", r.ptr(0x3154)?, 5, 4]);
    r.e(0x3160, dol::CCL_INIT, a)?;
    let cw = r.via(&tw(0x2c), 4)?;
    let f = r.h(&cw)? | 0x40;
    r.set(cw, f);
    let (a, b, c) = (r.ptr(0x3178)?, r.ptr(0x3184)?, r.ptr(0x3190)?);
    r.set(tk(0x14), a);
    r.set(tk(0x20), b);
    r.set(tk(0x10), c);
    let y = r.f(&tw(0x18))?;
    r.setf(key(&d, 8), y);
    r.setf(tw(0x18), y + r.c(0x31ac)?);
    r.set(tw(2), 9);
    let x = r.ptr(0x31cc)?;
    r.set(tk(0x18), x);
    Ok(())
}
/// +0x31EC.
fn pnp_dest(r: &mut Run<'_, '_>) -> Result<()> {
    let d = r.get(&tk(0x38))?;
    r.e(0x320c, dol::SY_FREE, json!([d]))?;
    r.set(tk(0x38), 0);
    Ok(())
}

/// +0x322C, the spring pad exec: a player touching it is launched along
/// their horizontal speed (capped), the pad bounces and spins, and its
/// three collision pieces turn with it.
fn pnp_exec(r: &mut Run<'_, '_>) -> Result<()> {
    let d = blk(r)?;
    let pause = r.gs(0x325c, 0x37)?;
    if r.sb(&pause)? == 0 {
        r.set(tk(0x4c), 0);
    }
    let cw = r.via(&tw(0x2c), 0)?;
    let cw = cw.trim_end_matches("+0x0").to_string();
    if r.h(&key(&cw, 4))? & 1 != 0 {
        let other = r.via(&key(&cw, 0xa4), 0x9c)?;
        let a = json!([r.get(&other)?]);
        let mut pl = r.er(0x3298, dol::IS_THIS_TASK_PLAYER, a)?;
        if pl >= 0 {
            let mut ok = true;
            if r.sh(&key(&cw, 0xa0))? != 0 {
                let mut p = pos(r)?;
                p[1] -= r.c(0x32c8)?;
                pl = r.er(0x32f8, dol::EITHER_PLAYER_WITHIN_SPHERE, json!([p, r.c(0x32f0)?]))? - 1;
                ok = pl >= 0;
            }
            let g = r.gs(if ok { 0x3320 } else { 0x3660 }, 0x37)?;
            let g = r.sb(&g)? as i32;
            r.set(tk(0x4c), g);
            if ok {
                pnp_launch(r, &d, pl as u32)?;
            } else if r.h(&key(&cw, 4))? & 2 != 0 {
                let s = r.f(&key(&d, 4))? + r.c(0x3690)?;
                r.setf(key(&d, 4), s);
            }
        }
    }
    let active = |r: &Run<'_, '_>, pc: u32| -> Result<bool> {
        let g = r.gs(pc, 0x37)?;
        Ok(r.i(&tk(0x4c))? as i32 != 0 || r.sb(&g)? == 0)
    };
    if active(r, 0x36b0)? && r.b(&tw(3))? != 0 {
        let n = r.b(&tw(3))? - 1;
        r.set(tw(3), n);
        let v = r.f(&key(&d, 12))? + r.f(&key(&d, 16))?;
        r.setf(key(&d, 12), v);
        let f = r.h(&key(&cw, 4))?;
        r.set(key(&cw, 4), if n < 18 { f & !0x40 } else { f | 0x40 });
        if n == 10 {
            r.e(0x3740, dol::SE_CALL, json!([1, 0, 0, 0]))?;
        }
    }
    if active(r, 0x3754)? {
        pnp_bounce(r, &d, &cw)?;
    }
    r.e(0x3b24, dol::FN_800068E4, json!([cw]))?;
    r.e(0x3b2c, dol::CCL_ENTRY, json!(["t"]))
}

/// +0x3308..+0x3658: launch player `pl`.
fn pnp_launch(r: &mut Run<'_, '_>, d: &str, pl: u32) -> Result<()> {
    r.e(0x3318, dol::SE_CALL, json!([0, 0, 0, 0]))?;
    r.d(0x3338, 0x801e_7768)?;
    r.d(0x3350, 0x801e_7788)?;
    let ptwp = r.p(&key("0x801e7768", 4 * pl))?.ok_or_else(|| bad("no player work"))?;
    let ptp = r.p(&key("0x801e7788", 4 * pl))?.ok_or_else(|| bad("no player task"))?;
    let mw = r.p(&key(&ptp, 0x38))?.ok_or_else(|| bad("no player motion work"))?;
    let vel = r.v3(&mw, 0)?;
    let mut h = vel;
    h[1] = r.c(0x3394)?;
    let mut f30 = r.ef(0x33a4, dol::NJ_SCALOR, json!([vel]))?;
    let f28 = f30;
    let f31 = r.ef(0x33b4, dol::NJ_SCALOR, json!([h]))?;
    if f28 > r.c(0x33c0)? {
        let s = if f31 > r.c(0x33d4)? {
            Some(r.c(0x33e8)? / f31)
        } else if f31 > r.c(0x3414)? {
            Some(r.c(0x3428)? / f31)
        } else {
            None
        };
        if let Some(s) = s {
            h[0] *= s;
            h[2] *= s;
        }
    }
    f30 *= r.c(0x3450)?;
    r.setf(key(d, 4), f30);
    if vel[1] < r.c(0x346c)? {
        if f30 < r.c(0x3480)? {
            f30 = r.c(0x3494)?;
        }
        let pp = r.v3(&ptwp, 0x14)?;
        r.e(0x34bc, dol::FN_800399BC, json!([pl, pp[0], r.c(0x34a8)? + pp[1], pp[2]]))?;
        let y = if vel[1] < r.c(0x34d4)? { r.c(0x34e8)? + f30 } else { r.c(0x34fc)? };
        r.e(0x350c, dol::SET_VELOCITY_P, json!([pl, -h[0], y, -h[2]]))?;
        if f28 > r.c(0x3514)? {
            let p = pos(r)?;
            let a = r.ef(0x353c, dol::ATAN2F, json!([pp[0] - p[0], pp[2] - p[2]]))?;
            let a = fctiwz((r.cd(0x3544)? * a as f64) as f32);
            r.d(0x3570, 0x801e_7768)?;
            r.set(key(&ptwp, 0xc), a.wrapping_neg().wrapping_add(0x10000 - 16384));
        }
        r.d(0x358c, 0x801e_7728)?;
        let pwp = r.p(&key("0x801e7728", 4 * pl))?.ok_or_else(|| bad("no player data"))?;
        r.set(key(&pwp, 20), 13);
    }
    let b = r.c(0x35a8)? * f30.abs();
    r.setf(key(d, 12), b);
    r.set(tw(3), 23);
    let s = r.c(0x35dc)? * (f30 * r.c(0x35c8)?).abs();
    r.setf(key(d, 16), s);
    let mut q = pos(r)?;
    q[1] += r.c(0x3610)?;
    let a = json!([q, r.c(0x3628)?, r.c(0x3634)?, 7]);
    r.m(0x3640, a)?;
    r.e(0x3654, dol::FN_8002FB2C, json!([pl, 4, 15, 0]))
}

/// +0x3768..+0x3B1C: bounce the pad on its rest height, decay the spin and
/// place the three collision pieces around it.
fn pnp_bounce(r: &mut Run<'_, '_>, d: &str, cw: &str) -> Result<()> {
    let y0 = r.f(&tw(0x18))?;
    let s = r.f(&key(d, 4))? * r.c(0x3778)?;
    r.setf(key(d, 4), s);
    let mut v = r.f(&key(d, 12))?;
    if r.b(&tw(3))? == 0 {
        v *= r.c(0x37a0)?;
    }
    v -= r.c(0x37bc)?;
    r.setf(key(d, 12), v);
    let rest = r.f(&key(d, 8))?;
    let mut y = y0 + v;
    r.setf(tw(0x18), y);
    if y < r.c(0x37e8)? + rest {
        v += r.c(0x3810)?;
        if v.abs() > r.c(0x3830)? {
            let mut q = pos(r)?;
            q[1] -= r.c(0x3860)?;
            let a = json!([q, r.c(0x3878)?, r.c(0x3884)?, 8]);
            r.m(0x3890, a)?;
            let a = json!([q, r.c(0x389c)?, r.c(0x38a8)?, 6]);
            r.m(0x38b4, a)?;
            r.e(0x38c8, dol::SE_CALL, json!([2, 0, 0, 0]))?;
        }
        if v > r.c(0x38d8)? {
            v = r.c(0x38ec)?;
        }
        if v < r.c(0x3908)? {
            v = r.c(0x391c)?;
        }
        v *= r.c(0x3938)?;
        r.setf(key(d, 12), v);
        y = r.c(0x394c)? + rest;
        r.setf(tw(0x18), y);
    }
    let top = rest + (r.c(0x3974)? + r.f(&tw(0x20))?);
    if y > top {
        y = rest + (r.c(0x399c)? + r.f(&tw(0x20))?);
        r.setf(tw(0x18), y);
    }
    let dy = y - y0;
    let s = r.c(0x39c4)? * r.f(&key(d, 4))? + r.c(0x39d8)? * dy;
    r.setf(key(d, 4), s);
    let step = r.cd(0x39fc)? * (r.cd(0x3a08)? * (s as f64 / r.cd(0x3a1c)?));
    let turn = (r.i(&key(d, 0))? as i32).wrapping_add(fctiwz(step as f32));
    r.set(key(d, 0), turn);
    let dy = r.p(&key(cw, 0xc))?.ok_or_else(|| bad("collision info has no shapes"))?;
    let mut a = turn.wrapping_neg().wrapping_sub(5461);
    for i in 0..3u32 {
        let v = [r.c(0x3a64)?, r.c(0x3a74)?, r.c(0x3a84)?];
        r.e(0x3a90, dol::NJ_PUSH_MATRIX_EX, json!([]))?;
        r.e(0x3a98, dol::NJ_UNIT_MATRIX, json!([0]))?;
        r.e(0x3aa4, dol::NJ_ROTATE_Y, json!([0, a]))?;
        let p = r.e3(0x3ab4, dol::NJ_CALC_POINT, json!([0, v]))?;
        r.e(0x3abc, dol::NJ_POP_MATRIX, json!([1]))?;
        let at = (i + 2) * 48;
        r.set(key(&dy, at + 40), a);
        r.setf(key(&dy, at + 8), p[0]);
        r.setf(key(&dy, at + 16), p[2]);
        a = a.wrapping_add(21845);
    }
    Ok(())
}

/// +0x3B58, the pad display: base below the work position, the spinning
/// top (child) and the spring (sibling) stretched by the bounce timer.
fn pnp_display(r: &mut Run<'_, '_>) -> Result<()> {
    let d = blk(r)?;
    let a = json!([r.ptr(0x3b80)?]);
    r.e(0x3b84, dol::NJ_SET_TEXTURE, a)?;
    let obj = ptr_name(r, 0x3b8c)?;
    r.e(0x3b90, dol::NJ_PUSH_MATRIX_EX, json!([]))?;
    let p = pos(r)?;
    let a = json!([0, p[0], p[1] - r.c(0x3ba4)?, p[2]]);
    r.e(0x3bb4, dol::NJ_TRANSLATE, a)?;
    let a = json!([0, r.get(&tw(0xc))?]);
    r.e(0x3bc0, dol::NJ_ROTATE_Y, a)?;
    let m = field(r, &obj, 4)?;
    r.e(0x3bc8, dol::LATE_DRAW_MODEL, json!([m]))?;
    let c = name(&field(r, &obj, 0x2c)?)?;
    r.e(0x3bd0, dol::NJ_PUSH_MATRIX_EX, json!([]))?;
    let a = json!([0, r.v3(&c, 8)?]);
    r.e(0x3bdc, dol::NJ_TRANSLATE_V, a)?;
    let t = (r.i(&key(&d, 0))? as i32).wrapping_neg();
    r.e(0x3bf0, dol::NJ_ROTATE_Y, json!([0, t]))?;
    let m = field(r, &c, 4)?;
    r.e(0x3bf8, dol::LATE_DRAW_MODEL, json!([m]))?;
    r.e(0x3c00, dol::NJ_POP_MATRIX, json!([1]))?;
    let s = name(&field(r, &obj, 0x30)?)?;
    let n = r.b(&tw(3))?;
    let lift = if n != 0 { r.c(0x3c18)? * r.cvtu(0x3c28, n.into())? } else { r.c(0x3c50)? };
    let sp = r.v3(&s, 8)?;
    r.e(0x3c6c, dol::NJ_TRANSLATE, json!([0, sp[0], sp[1] + lift, sp[2]]))?;
    let m = field(r, &s, 4)?;
    r.e(0x3c74, dol::LATE_DRAW_MODEL, json!([m]))?;
    r.e(0x3c7c, dol::NJ_POP_MATRIX, json!([1]))
}

/// +0x3CA0, the pad's second display: two fogless models at the rest
/// height and at the top of the travel.
fn pnp_display2(r: &mut Run<'_, '_>) -> Result<()> {
    let d = blk(r)?;
    r.e(0x3cbc, dol::NJ_DISABLE_FOG, json!([]))?;
    r.e(0x3cc0, dol::GJ_SET_FOG, json!([]))?;
    let p = pos(r)?;
    let rest = r.f(&key(&d, 8))?;
    for (push, tex, tr, y, rot, obj, draw, pop) in [
        (0x3cc4, 0x3ccc, 0x3cf8, None, 0x3d04, 0x3d0c, 0x3d14, 0x3d18),
        (0x3d1c, 0x3d24, 0x3d58, Some(0x3d40), 0x3d64, 0x3d6c, 0x3d74, 0x3d78),
    ] {
        r.e(push, dol::NJ_PUSH_MATRIX_EX, json!([]))?;
        let a = json!([r.ptr(tex)?]);
        r.e(tex + 4, dol::NJ_SET_TEXTURE, a)?;
        let h = match y {
            None => r.c(0x3ce0)? + rest,
            Some(c) => rest + (r.c(c)? + r.f(&tw(0x20))?),
        };
        r.e(tr, dol::NJ_TRANSLATE, json!([0, p[0], h, p[2]]))?;
        let a = json!([0, r.get(&tw(0xc))?]);
        r.e(rot, dol::NJ_ROTATE_Y, a)?;
        let o = ptr_name(r, obj)?;
        let m = field(r, &o, 4)?;
        r.e(draw, dol::GJ_DRAW_MODEL, json!([m]))?;
        r.e(pop, dol::NJ_POP_MATRIX_EX, json!([]))?;
    }
    r.e(0x3d7c, dol::NJ_ENABLE_FOG, json!([]))?;
    r.e(0x3d80, dol::GJ_SET_FOG, json!([]))
}

// ---------------------------------------------------------------- scenery

/// The scenery loaders: range check, optional `twp+1 = 3` (rotate on all
/// three axes), model and texture into t+0x38/+0x3C, then +0x4040.
fn scenery_load(r: &mut Run<'_, '_>, range: u32, three: bool, model: u32, tex: u32, ccl: Option<u32>, n: i64, call: u32) -> Result<()> {
    if range_out(r, range)? {
        return Ok(());
    }
    if three {
        r.set(tw(1), 3);
    }
    let (m, t) = (r.ptr(model)?, r.ptr(tex)?);
    r.set(tk(0x38), m);
    r.set(tk(0x3c), t);
    if r.sr.local(call)? != 0x4040 {
        return Err(bad("scenery init call"));
    }
    let c = match ccl {
        Some(pc) => r.ptr(pc)?,
        None => json!(0),
    };
    scenery_init(r, c, n)
}
fn rcross_load(r: &mut Run<'_, '_>) -> Result<()> {
    scenery_load(r, 0x3dc4, true, 0x3ddc, 0x3de8, Some(0x3df8), 2, 0x3e00)
}
fn tim_load(r: &mut Run<'_, '_>) -> Result<()> {
    scenery_load(r, 0x3e3c, false, 0x3e4c, 0x3e58, Some(0x3e68), 2, 0x3e70)
}
fn kusa_load(r: &mut Run<'_, '_>) -> Result<()> {
    scenery_load(r, 0x3ea8, true, 0x3ec0, 0x3ecc, None, 0, 0x3ee0)
}
fn kakasi_load(r: &mut Run<'_, '_>) -> Result<()> {
    scenery_load(r, 0x3f1c, false, 0x3f2c, 0x3f38, Some(0x3f48), 1, 0x3f50)
}
fn fence_load(r: &mut Run<'_, '_>) -> Result<()> {
    scenery_load(r, 0x3f88, false, 0x3f98, 0x3fa4, Some(0x3fb4), 1, 0x3fbc)
}
fn woodead_load(r: &mut Run<'_, '_>) -> Result<()> {
    scenery_load(r, 0x3ff4, false, 0x4004, 0x4010, Some(0x4020), 1, 0x4028)
}
/// +0x4040 on the input's `ccl` and `n`.
fn scenery_init_entry(r: &mut Run<'_, '_>) -> Result<()> {
    let c = r.get("ccl")?;
    let n = r.i("n")?;
    scenery_init(r, c, n)
}
/// +0x4040: callbacks, and collision when given.
fn scenery_init(r: &mut Run<'_, '_>, ccl: Value, n: i64) -> Result<()> {
    let (d, e, x) = (r.ptr(0x406c)?, r.ptr(0x4078)?, r.ptr(0x4084)?);
    r.set(tk(0x14), d);
    r.set(tk(0x10), e);
    r.set(tk(0x18), x);
    if ccl != json!(0) {
        r.e(0x40a4, dol::CCL_INIT, json!(["t", ccl, n, 4]))?;
    }
    Ok(())
}
/// +0x40C0.
fn scenery_dest(r: &mut Run<'_, '_>) -> Result<()> {
    r.set(tk(0x3c), 0);
    r.set(tk(0x38), 0);
    Ok(())
}
/// +0x40E4.
fn scenery_exec(r: &mut Run<'_, '_>) -> Result<()> {
    if !range_out(r, 0x4104)? && r.nz(&tw(0x2c))? {
        r.e(0x4120, dol::CCL_ENTRY, json!(["t"]))?;
    }
    Ok(())
}
/// +0x413C: place, rotate (three axes when work +1 is set), and for modes
/// 1 and 2 sway the scale with the clock.
fn scenery_display(r: &mut Run<'_, '_>) -> Result<()> {
    let t = r.get(&tk(0x3c))?;
    r.e(0x4178, dol::NJ_SET_TEXTURE, json!([t]))?;
    r.e(0x417c, dol::NJ_PUSH_MATRIX_EX, json!([]))?;
    r.e(0x4184, dol::NJ_TRANSLATE_EX, json!([pos(r)?]))?;
    if r.sb(&tw(1))? != 0 {
        let a = json!([0, r.get(&tw(16))?]);
        r.e(0x41a0, dol::NJ_ROTATE_Z, a)?;
        let a = json!([0, r.get(&tw(8))?]);
        r.e(0x41ac, dol::NJ_ROTATE_X, a)?;
        let a = json!([0, r.get(&tw(12))?]);
        r.e(0x41b8, dol::NJ_ROTATE_Y, a)?;
    } else {
        let a = json!([0, r.get(&tw(12))?]);
        r.e(0x41c8, dol::NJ_ROTATE_Y, a)?;
    }
    let sway = |r: &mut Run<'_, '_>, rate: u32, clk: u32, sin: u32| -> Result<f32> {
        let (s, o) = r.sr.at(rate)?;
        let k = r.sr.word(s, o)?;
        let c = r.gs(clk, 0x7c)?;
        let ph = r.w(&c)?.wrapping_add(r.h(&tw(6))? as u32);
        r.ef(sin, dol::NJ_SIN, json!([k.wrapping_mul(ph) as i32]))
    };
    match r.sb(&tw(0))? {
        1 => {
            let s = sway(r, 0x41f0, 0x41fc, 0x4210)?;
            let y = r.c(0x421c)? + r.c(0x4228)? * s;
            let xz = r.c(0x423c)? + r.c(0x4248)? * s;
            r.e(0x4268, dol::NJ_SCALE, json!([0, xz, y, xz]))?;
        }
        2 => {
            let s = sway(r, 0x4274, 0x4280, 0x4294)?;
            let y = r.c(0x42a0)? + r.c(0x42ac)? * s;
            let x = r.c(0x42c0)? + r.c(0x42cc)? * s;
            let z = r.c(0x42e0)? + r.c(0x42ec)? * s;
            r.e(0x430c, dol::NJ_SCALE, json!([0, x, y, z]))?;
        }
        _ => {}
    }
    let m = r.get(&tk(0x38))?;
    r.e(0x4314, dol::LATE_DRAW_MODEL, json!([m]))?;
    r.e(0x4318, dol::NJ_POP_MATRIX_EX, json!([]))
}

// ---------------------------------------------------- RAILOBJCL / CR

fn railobjc_load(r: &mut Run<'_, '_>, d: u32) -> Result<()> {
    if range_out(r, 0x4370 + d)? {
        return Ok(());
    }
    let (a, e, x) = (r.ptr(0x4380 + d)?, r.ptr(0x438c + d)?, r.ptr(0x4398 + d)?);
    r.set(tk(0x14), a);
    r.set(tk(0x10), e);
    r.set(tk(0x18), x);
    r.set(tw(1), 0);
    let (t, o) = (r.ptr(0x43ac + d)?, r.ptr(0x43b8 + d)?);
    r.set(tk(0x3c), t);
    r.set(tk(0x38), o);
    let ph = fctiwz(r.rnd(0x43c0 + d)? * r.c(0x43ec + d)? * r.c(0x43fc + d)? * r.c(0x440c + d)?);
    r.set(tw(6), ph as u16);
    let a = json!(["t", r.ptr(0x4434 + d)?, 1, 4]);
    r.e(0x4440 + d, dol::CCL_INIT, a)
}
fn railobjcl_load(r: &mut Run<'_, '_>) -> Result<()> {
    railobjc_load(r, 0)
}
fn railobjcr_load(r: &mut Run<'_, '_>) -> Result<()> {
    railobjc_load(r, 0x10c)
}
/// +0x4568.
fn railobjc_dest(r: &mut Run<'_, '_>) -> Result<()> {
    r.set(tk(0x38), 0);
    r.set(tk(0x3c), 0);
    Ok(())
}
/// +0x458C: advance the swing phase by the module's rate.
fn railobjc_exec(r: &mut Run<'_, '_>) -> Result<()> {
    if range_out(r, 0x45ac)? {
        return Ok(());
    }
    let pause = r.gs(0x45bc, 0x37)?;
    if r.sb(&pause)? == 0 {
        let (s, o) = r.sr.at(0x45d8)?;
        let k = r.sr.word(s, o)?;
        let a = (r.h(&tw(6))? as u32).wrapping_add(k) as u16;
        r.set(tw(6), a);
    }
    r.e(0x45ec, dol::CCL_ENTRY, json!(["t"]))
}
/// +0x4608: the post, then its child swung about X by sin(phase).
fn railobjc_display(r: &mut Run<'_, '_>) -> Result<()> {
    let obj = name(&r.get(&tk(0x38))?)?;
    let t = r.get(&tk(0x3c))?;
    r.e(0x4630, dol::NJ_SET_TEXTURE, json!([t]))?;
    r.e(0x4638, dol::NJ_PUSH_MATRIX, json!([0]))?;
    r.e(0x4644, dol::NJ_TRANSLATE_V, json!([0, pos(r)?]))?;
    let a = json!([0, r.get(&tw(16))?]);
    r.e(0x4650, dol::NJ_ROTATE_Z, a)?;
    let a = json!([0, r.get(&tw(8))?]);
    r.e(0x465c, dol::NJ_ROTATE_X, a)?;
    let a = json!([0, r.get(&tw(12))?]);
    r.e(0x4668, dol::NJ_ROTATE_Y, a)?;
    let oy = r.get(&key(&obj, 24))?;
    if oy != json!(0) {
        r.e(0x4680, dol::NJ_ROTATE_Y, json!([0, oy]))?;
    }
    let m = field(r, &obj, 4)?;
    r.e(0x4688, dol::LATE_DRAW_MODEL, json!([m]))?;
    let c = name(&field(r, &obj, 0x2c)?)?;
    let a = json!([0, r.v3(&c, 8)?]);
    r.e(0x4698, dol::NJ_TRANSLATE_V, a)?;
    let a = json!([0, r.get(&key(&c, 24))?]);
    r.e(0x46a4, dol::NJ_ROTATE_Y, a)?;
    let ph = r.h(&tw(6))?;
    let s = r.ef(0x46ac, dol::NJ_SIN, json!([ph]))?;
    let a = fctiwz(r.c(0x46b4)? * s * r.c(0x46c4)?);
    r.e(0x46e0, dol::NJ_ROTATE_X, json!([0, a]))?;
    let m = field(r, &c, 4)?;
    r.e(0x46e8, dol::LATE_DRAW_MODEL, json!([m]))?;
    r.e(0x46f0, dol::NJ_POP_MATRIX, json!([1]))
}
