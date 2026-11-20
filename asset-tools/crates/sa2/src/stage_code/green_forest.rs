//! Green Forest (stage 3, stg03D.rel): the stage program "STG03" that
//! `_prolog` (+0x5860) hands to LoadSubprogram (.data+0x1574C), its stage
//! tasks and the per-player task the setup creates. Offsets are stg03D
//! section-1 offsets; engine calls are GSNE8P main.dol addresses.
//!
//! | Offset | Size | Routine | Port |
//! | --- | --- | --- | --- |
//! | +0x588C | 0x398 | stage setup (program entry 1) | [`setup`] |
//! | +0x5C24 | 0x11C | stage teardown (entry 2) | [`teardown`] |
//! | +0x5DFC | 0x4 | program exec (entry 3): `blr`, no effect | none |
//! | +0x5D40 | 0xBC | two-player music task exec | [`music_exec`] |
//! | +0x5E00 | 0x2E4 | environment task exec: per-camera zone bits, texture cycle | [`env_exec`] |
//! | +0x60E4 | 0x194 | environment task display (+0x14): canopy model | [`env_display`] |
//! | +0x6278 | 0x270 | environment task display (+0x20): water and floating models | [`env_display2`] |
//! | +0x4D08 | 0x1B8 | per-player task create (exec +0x4EC0) | [`player_task_create`] |
use super::{sec, Ident, Returns, Run, StageRel};
use crate::{Error, Result};
use serde::Deserialize;
use serde_json::{json, Value};

pub const IDENT: Ident = Ident {
    name: "stage 3",
    len: 1_965_800,
    gc_sha256: "5cf32dfafa7b27fbaa1e36881edc2becc83b957c388270c2d219a1bf657ca5a6",
    gc_module: 3,
    steam_sha256: "d4f8f94bad9793a5e2bd70fdedc647ac9505997e617844d4719932bff1e11e06",
    steam_module: 2,
};

pub fn read(bytes: &[u8]) -> Result<StageRel<'_>> {
    StageRel::read(bytes, &IDENT)
}

fn bits(v: f32) -> u32 {
    v.to_bits()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SetupInput {
    /// Game byte 0x801CC168 +0x1E (non-zero in two-player play).
    pub g1e: i8,
    /// The words at 0x801A2A18 +0 and +0x24.
    pub slots: [u32; 2],
    /// .bss+0x30 and +0x34, the per-player task slots.
    pub players: [u32; 2],
    /// playertwp[0] and [1] are non-null.
    pub present: [bool; 2],
    /// fn_80065048 (kept at .bss+0x2C) and CreateElementalTask.
    pub returns: Returns,
}

/// +0x588C. `state.task` lists the environment task's display callbacks
/// (+0x14 and +0x20).
pub fn setup(r: &StageRel<'_>, i: &SetupInput) -> Result<Value> {
    let mut run = Run::new(r, i.returns.clone());
    let p5 = |pc| r.ptr(pc, 5);
    r.dol(0x58a8, 0x801cc168)?;
    let two = i.g1e != 0;
    if two {
        let t = json!([2, 0, r.ptr(0x58cc, 1)?, p5(0x58d4)?]);
        run.query(0x58d8, 0x80071e0c, t)?;
    }
    run.call(0x58e8, 0x800324e4, json!([255, p5(0x58e4)?]))?;
    run.call(
        0x590c,
        0x800245ec,
        json!([p5(0x58f0)?, p5(0x58f8)?, p5(0x5900)?, p5(0x5908)?]),
    )?;
    for (a, b, load, reg) in [
        (0x5914, 0x5920, 0x5928, 0x5934),
        (0x593c, 0x5948, 0x5950, 0x595c),
    ] {
        let args = json!([p5(a)?, p5(b)?]);
        run.call(load, 0x80072270, args.clone())?;
        run.call(reg, 0x80072218, args)?;
    }
    let handle = run.query(0x5968, 0x80065048, json!([0, 0x800]))? as u32;
    r.at(0x5970, 6)?; // .bss+0x2C keeps the handle
    run.call(0x5984, 0x8006638c, json!([p5(0x597c)?, handle]))?;
    run.call(0x5988, 0x8001a8a8, json!([]))?;
    run.call(0x598c, 0x80033710, json!([]))?;
    run.module(0x5998, json!([p5(0x5994)?]))?;
    if !two {
        run.call(0x59b0, 0x80011a04, json!([0]))?;
        run.call(0x59bc, 0x80011050, json!([p5(0x59b8)?]))?;
    }
    run.call(
        0x59d8,
        0x801125c0,
        json!([bits(r.f(0x59c4)?), bits(r.f(0x59d0)?)]),
    )?;
    run.call(0x59e4, 0x8005d0ec, json!([p5(0x59e0)?]))?;
    let env = json!([2, 1, r.ptr(0x59f4, 1)?, p5(0x59fc)?]);
    let task = if run.query(0x5a00, 0x80071e0c, env)? as u32 != 0 {
        Some([r.ptr(0x5a10, 1)?, r.ptr(0x5a1c, 1)?])
    } else {
        None
    };
    run.call(0x5a2c, 0x8003bcc4, json!([p5(0x5a28)?]))?;
    run.module(0x5a30, json!([]))?;
    let mut slots: Vec<Value> = i.slots.iter().map(|v| json!(v)).collect();
    for (k, (lis, own)) in [(0x5a38, 0x5a4c), (0x5a58, 0x5a70)].into_iter().enumerate() {
        r.dol(lis, 0x801a2a18)?;
        if i.slots[k] == 0 {
            slots[k] = r.ptr(own, 1)?;
        }
    }
    run.module(0x5a78, json!([]))?;
    run.module(0x5a84, json!([p5(0x5a80)?]))?;
    // Trail plugin table 0x801E77D8: fn_800E014C, three module routines, zeros.
    r.dol(0x5a94, 0x801e77d8)?;
    let mut trail = vec![json!(r.dol(0x5a8c, 0x800e014c)?)];
    for pc in [0x5aa0, 0x5aac, 0x5ab8] {
        trail.push(r.ptr(pc, 1)?);
    }
    trail.resize(14, json!(0));
    run.module(0x5af4, json!([0, 0]))?;
    if two {
        run.module(0x5b10, json!([1, 0]))?;
    }
    let (s0, s1) = (r.ptr(0x5b18, 6)?, r.ptr(0x5b8c, 6)?);
    r.at(0x5b2c, 6)?;
    if i.players == [0, 0] {
        // 1000 frames of trail in two-player play, 1800 alone.
        let (n, c0, c1) = if two {
            (1000, 0x5b68, 0x5b90)
        } else {
            (1800, 0x5bb4, 0x5bdc)
        };
        if i.present[0] {
            run.module(c0, json!([0, n, s0]))?;
        }
        if i.present[1] {
            run.module(c1, json!([1, n, s1]))?;
        }
    }
    run.call(0x5bf0, 0x8006d500, json!([p5(0x5be4)?, r.ptr(0x5bec, 4)?]))?;
    run.module(0x5c04, json!([p5(0x5bf8)?, r.ptr(0x5c00, 6)?]))?;
    Ok(json!({ "calls": run.calls, "state": {
        "handle": handle, "task": task, "slots": slots, "trail": trail } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TeardownInput {
    /// .bss+0x30 and +0x34.
    pub players: [u32; 2],
    /// 0x801A2A18 +0 and +0x24: `"ours"` or a word.
    pub slots: [Value; 2],
    /// .bss+0x2C.
    pub handle: u32,
}

/// +0x5C24.
pub fn teardown(r: &StageRel<'_>, i: &TeardownInput) -> Result<Value> {
    let mut run = Run::new(r, Returns::new());
    let p5 = |pc| r.ptr(pc, 5);
    run.module(0x5c34, json!([]))?;
    run.call(0x5c38, 0x8011c2bc, json!([]))?;
    run.call(0x5c3c, 0x8011c310, json!([]))?;
    for (k, (at, pc)) in [(0x5c44, 0x5c54), (0x5c64, 0x5c74)].into_iter().enumerate() {
        r.at(at, 6)?;
        if i.players[k] != 0 {
            run.call(pc, 0x80071af0, json!([i.players[k]]))?;
        }
    }
    run.module(0x5c80, json!([]))?;
    let mut slots = Vec::new();
    for (v, (lis, own)) in i.slots.iter().zip([(0x5c88, 0x5c94), (0x5cac, 0x5cbc)]) {
        r.dol(lis, 0x801a2a18)?;
        r.ptr(own, 1)?;
        slots.push(match v {
            Value::String(s) if s == "ours" => json!(0),
            v if v.is_u64() => v.clone(),
            _ => return Err(Error::invalid("teardown slot must be \"ours\" or a word")),
        });
    }
    run.module(0x5cd0, json!([]))?;
    run.call(0x5cd4, 0x800336dc, json!([]))?;
    run.call(0x5ce8, 0x80072118, json!([p5(0x5cdc)?, p5(0x5ce4)?]))?;
    run.call(0x5cfc, 0x80072118, json!([p5(0x5cf0)?, p5(0x5cf8)?]))?;
    run.call(0x5d00, 0x80066364, json!([]))?;
    r.at(0x5d08, 6)?;
    if i.handle != 0 {
        run.call(0x5d18, 0x8001a2b0, json!([i.handle]))?;
    }
    run.call(0x5d24, 0x800245b8, json!([]))?;
    run.module(0x5d28, json!([]))?;
    Ok(json!({ "calls": run.calls, "state": {
        "handle": 0, "players": [0, 0], "slots": slots } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MusicInput {
    /// playertwp[0] and [1] action bytes.
    pub actions: [i8; 2],
    /// Words at 0x803AD9E0 and 0x803AD9E4.
    pub mode: i32,
    pub track: i32,
    /// fn_8001CD40 (twice).
    pub returns: Returns,
}

/// +0x5D40, the two-player music task: once both players stand at the
/// start (action 54) the stage music restarts or fades (fn_80011DF4).
pub fn music_exec(r: &StageRel<'_>, i: &MusicInput) -> Result<Value> {
    let mut run = Run::new(r, i.returns.clone());
    if run.query(0x5d4c, 0x8001cd40, json!([]))? as i32 == 16 && i.actions == [54, 54] {
        r.dol(0x5d5c, 0x801e7768)?;
        r.dol(0x5d8c, 0x803ad9e0)?;
        if i.mode == 1 {
            r.dol(0x5da0, 0x803ad9e4)?;
            if i.track == 118 {
                run.call(0x5db4, 0x80011a04, json!([0]))?;
                run.call(0x5dc0, 0x80011050, json!([r.ptr(0x5dbc, 5)?]))?;
            } else if i.track < 118 {
                run.call(0x5dcc, 0x80011df4, json!([]))?;
            }
        } else if i.mode < 1 {
            run.call(0x5dd8, 0x80011df4, json!([]))?;
        }
    }
    if run.query(0x5ddc, 0x8001cd40, json!([]))? as i32 == 7 {
        run.call(0x5de8, 0x80011df4, json!([]))?;
    }
    Ok(json!({ "calls": run.calls, "state": {} }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct EnvInput {
    /// lbl_80175378[0] and [1] camera positions (absent camera: null).
    pub cameras: [Option<[f32; 3]>; 2],
    /// Game words 0x801CC168 +0xA8 and +0xAC (per-camera flags).
    pub cam_flags: [u32; 2],
    /// Game byte +0x37 (non-zero pauses the texture cycle).
    pub g37: i8,
    /// Work +0 (zone bits) and +6 (texture counter).
    pub bits: u8,
    pub counter: u16,
    /// fn_8011B51C (distance from the camera to the shore line) and
    /// njDistanceP2P, per camera.
    pub returns: Returns,
}

/// Chunk models whose texture id (halfword at +10 of the block their
/// model's +4 points at) the environment task animates: the model at
/// .data+0x14CF8 and every sibling of the tree under .data+0x14EA8 +0x2C.
fn animated(r: &StageRel<'_>) -> Result<Vec<(u8, u32)>> {
    let target = |s: u8, at: u32| -> Result<(u8, u32)> {
        let (s, p) = r.pointer(s, at)?;
        Ok((s, p + 10))
    };
    let mut out = vec![target(5, r.at(0x6068, 5)? + 4)?];
    let (mut s, mut node) = r.pointer(5, r.at(0x608c, 5)? + 44)?;
    for _ in 0..256 {
        let (ms, model) = r.pointer(s, node + 4)?;
        out.push(target(ms, model + 4)?);
        match r.pointer(s, node + 48) {
            Ok((ns, n)) => (s, node) = (ns, n),
            Err(_) if r.word(s, node + 48)? == 0 => return Ok(out),
            Err(e) => return Err(e),
        }
    }
    Err(Error::invalid("stage 3 model list too long"))
}

/// +0x5E00, the environment task exec. Per camera, bit 1<<i of the work
/// marks the camera past the canopy line, bit 4<<i near the water (within
/// 50 of the shore line, or of the pool point below it); the texture
/// counter cycles 0..13 into every animated model.
pub fn env_exec(r: &StageRel<'_>, i: &EnvInput) -> Result<Value> {
    let mut run = Run::new(r, i.returns.clone());
    r.dol(0x5e20, 0x80175378)?;
    r.dol(0x5e3c, 0x801cc168)?;
    let vec3 = |pc| -> Result<[f32; 3]> {
        let a = r.at(pc, 4)?;
        Ok([r.float(4, a)?, r.float(4, a + 4)?, r.float(4, a + 8)?])
    };
    let (from, to, pool) = (vec3(0x5e54)?, vec3(0x5e5c)?, vec3(0x5e64)?);
    let (near, deep, edge) = (r.f(0x5e2c)?, r.f(0x5e34)?, r.f(0x5e6c)?);
    let (cx, cz) = (r.f(0x5e44)?, r.f(0x5e4c)?);
    let mut b = i.bits;
    for (k, cam) in i.cameras.iter().enumerate() {
        let Some(p) = *cam else { continue };
        let flags = i.cam_flags[k];
        if (p[0] > cx && p[2] > cz) || (flags & 0x400 != 0 && p[2] > cz) {
            b |= 1 << k;
        } else {
            b &= !(1 << k);
        }
        let line = [from, [from[0] - to[0], from[1] - to[1], from[2] - to[2]]];
        let d = run.query(
            0x5fa8,
            0x8011b51c,
            json!([p.map(bits), line.map(|v| v.map(bits))]),
        )? as f32;
        let wet = d < near || {
            let e = run.query(0x5fc0, 0x8011b65c, json!([pool.map(bits), p.map(bits)]))? as f32;
            e < near && p[1] < deep && p[0] < edge
        };
        if wet {
            b |= 4 << k;
        } else {
            b &= !(4 << k);
        }
    }
    r.dol(0x6030, 0x801cc168)?;
    let mut counter = i.counter;
    if i.g37 == 0 {
        counter = counter.wrapping_add(1);
        if counter >= 14 {
            counter = 0;
        }
    }
    let mut textures = Vec::new();
    for (s, at) in animated(r)? {
        let old = r.half(s, at)?;
        textures.push(json!([sec(s, at), (old & 0xc000) | counter]));
    }
    Ok(json!({ "calls": run.calls, "state": {
        "bits": b, "counter": counter, "textures": textures } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DisplayInput {
    /// Game byte 0x801CC168 +0x3D (split-screen mode).
    pub g3d: i8,
    /// lbl_803ADAD0, the camera being drawn, and its position.
    pub camera: u32,
    pub pos: [f32; 3],
    /// Work +0 zone bits.
    pub bits: u8,
    /// .bss+0x28 and the model word at .data+0x15748 (for +0x60E4).
    #[serde(default)]
    pub clip: u32,
    #[serde(default)]
    pub clip_model: u32,
    /// njCalcPoint's Z per floating model (for +0x6278).
    #[serde(default)]
    pub returns: Returns,
}

/// The split-screen cases where neither display draws.
fn hidden(i: &DisplayInput) -> bool {
    (i.g3d == 1 && i.camera == 0) || (i.g3d == 2 && i.camera == 1) || i.g3d == 3
}

/// +0x60E4: the canopy, lowered by a fixed amount past the canopy line.
pub fn env_display(r: &StageRel<'_>, i: &DisplayInput) -> Result<Value> {
    let mut run = Run::new(r, Returns::new());
    r.dol(0x6104, 0x801cc168)?;
    if hidden(i) {
        return Ok(json!({ "calls": [], "state": {} }));
    }
    run.call(0x615c, 0x8011c2bc, json!([]))?;
    run.call(0x6160, 0x8011c310, json!([]))?;
    run.call(0x616c, 0x80116e18, json!([r.ptr(0x6168, 5)?]))?;
    run.call(0x6170, 0x80116098, json!([]))?;
    r.dol(0x6188, 0x80175378)?;
    let zero = r.f(0x61a0)?;
    let v = [i.pos[0], zero, i.pos[2]].map(bits);
    run.call(0x61bc, 0x8011538c, json!([0, v]))?;
    let (pc, k) = if i.bits as u32 & (1 << (i.camera & 31)) != 0 {
        (0x61f8, r.f(0x61ec)?)
    } else {
        (0x6218, r.f(0x620c)?)
    };
    run.call(pc, 0x80115bd8, json!([0, bits(zero), bits(k), bits(zero)]))?;
    r.at(0x6220, 6)?;
    if i.clip != 0 {
        r.at(0x6234, 5)?;
        run.call(0x623c, 0x8011f514, json!([i.clip_model]))?;
    } else {
        run.call(0x624c, 0x8011df20, json!([r.ptr(0x6248, 5)?]))?;
    }
    run.call(0x6250, 0x801160e4, json!([]))?;
    run.call(0x6254, 0x8011c2b0, json!([]))?;
    run.call(0x6258, 0x8011c310, json!([]))?;
    Ok(json!({ "calls": run.calls, "state": {} }))
}

/// +0x6278: the water surface while the camera is above it near the water,
/// then each floating model of the tree under .data+0x14EA8 whose view
/// depth lies between -1000 and 300.
pub fn env_display2(r: &StageRel<'_>, i: &DisplayInput) -> Result<Value> {
    let mut run = Run::new(r, i.returns.clone());
    r.dol(0x6298, 0x801cc168)?;
    if hidden(i) {
        return Ok(json!({ "calls": [], "state": {} }));
    }
    run.call(0x62f0, 0x8011c2bc, json!([]))?;
    run.call(0x62f4, 0x8011c310, json!([]))?;
    run.call(0x6300, 0x80116e18, json!([r.ptr(0x62fc, 5)?]))?;
    run.call(0x6304, 0x80116098, json!([]))?;
    r.dol(0x631c, 0x80175378)?;
    let cam = i.camera & 31;
    let mut v = [i.pos[0], r.f(0x6334)?, i.pos[2]];
    v[1] += if i.bits as u32 & (1 << cam) != 0 {
        r.f(0x636c)?
    } else {
        r.f(0x6388)?
    };
    run.call(0x63a0, 0x8011538c, json!([0, v.map(bits)]))?;
    if i.bits as u32 & (4 << cam) != 0 {
        let water = r.at(0x63ec, 5)?;
        if i.pos[1] > v[1] + r.float(5, water + 12)? {
            run.call(0x6408, 0x80116e18, json!([r.ptr(0x6404, 5)?]))?;
            run.call(0x6410, 0x8011df20, json!([sec(5, water)]))?;
        }
    }
    run.call(0x6414, 0x801160e4, json!([]))?;
    run.call(0x6418, 0x80116098, json!([]))?;
    let root = r.at(0x6420, 5)?;
    run.call(0x6428, 0x8011616c, json!([sec(5, root + 8)]))?;
    run.call(0x642c, 0x80116098, json!([]))?;
    run.call(0x643c, 0x80116e18, json!([r.ptr(0x6438, 5)?]))?;
    let (far, near) = (r.f(0x6448)?, r.f(0x6450)?);
    let mut node = r.pointer(5, root + 44)?;
    for _ in 0..256 {
        let (s, n) = node;
        let z = run.query(0x6468, 0x80114f0c, json!([0, sec(s, n + 8)]))? as f32;
        if -z < far && -z > near {
            let model = r.pointer(s, n + 4)?;
            run.call(0x6490, 0x8011616c, json!([sec(s, n + 8)]))?;
            run.call(0x649c, 0x80115f68, json!([sec(s, n + 20), 0]))?;
            run.call(0x64a4, 0x8011df20, json!([sec(model.0, model.1)]))?;
            run.call(0x64a8, 0x801160e4, json!([]))?;
            run.call(0x64ac, 0x80116098, json!([]))?;
        }
        if r.word(s, n + 48)? == 0 {
            run.call(0x64c0, 0x80115dc8, json!([2]))?;
            run.call(0x64c4, 0x8011c2b0, json!([]))?;
            run.call(0x64c8, 0x8011c310, json!([]))?;
            return Ok(json!({ "calls": run.calls, "state": {} }));
        }
        node = r.pointer(s, n + 48)?;
    }
    Err(Error::invalid("stage 3 model list too long"))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PlayerTaskInput {
    pub player: u32,
    /// Trail length in frames (1000 or 1800 from the setup).
    pub frames: i32,
    /// Where to store the task (.bss+0x30/+0x34), as a label, or null.
    pub slot: Option<Value>,
    /// playertwp[player] is non-null.
    pub present: bool,
    /// CreateElementalTask and syCalloc (twice).
    pub returns: Returns,
}

/// +0x4D08: a task (exec +0x4EC0, display +0x520C, destructor +0x5430)
/// holding `frames / 10 * 10` twelve-byte entries, each initialised by
/// +0x4A80 with the float at .data+0x106A0.
pub fn player_task_create(r: &StageRel<'_>, i: &PlayerTaskInput) -> Result<Value> {
    let mut run = Run::new(r, i.returns.clone());
    r.dol(0x4d30, 0x801e7768)?;
    let none = |run: Run| Ok(json!({ "calls": run.calls, "state": { "returned": 0 } }));
    if !i.present {
        return none(run);
    }
    let exec = r.ptr(0x4d58, 1)?;
    let task = run.query(0x4d68, 0x80071e0c, json!([2, 1, exec, r.ptr(0x4d64, 5)?]))? as u32;
    if task == 0 {
        return none(run);
    }
    run.module(0x4d84, json!(["work+0x14", i.player]))?;
    let block = run.query(0x4d98, 0x801197fc, json!([1, 12]))? as u32;
    if block == 0 {
        run.call(0x4db0, 0x80071af0, json!([task]))?;
        return none(run);
    }
    // mulhw by 0x66666667 and srawi 2 at +0x4DBC: a signed division by ten.
    let tens = i.frames / 10;
    let count = tens.wrapping_mul(10);
    let entries = run.query(0x4de0, 0x801197fc, json!([count, 12]))? as u32;
    if entries == 0 {
        run.call(0x4dfc, 0x80119828, json!([0]))?;
        run.call(0x4e04, 0x80119828, json!([block]))?;
        run.call(0x4e14, 0x80071af0, json!([task]))?;
        return none(run);
    }
    let start = r.float(5, r.at(0x4e54, 5)?)?;
    for k in 0..count.max(0) as u32 {
        run.module(
            0x4e74,
            json!([entries.wrapping_add(12 * k), bits(start), 0]),
        )?;
    }
    Ok(json!({ "calls": run.calls, "state": {
        "returned": task, "work3": i.player as u8,
        "task": { "0x10": exec, "0x18": r.ptr(0x4e34, 1)?, "0x1c": r.ptr(0x4e24, 1)?,
            "0x38": block },
        "block": [entries, tens, i.slot.is_some()],
        "slot": i.slot.as_ref().map(|s| json!([s, task])) } }))
}

/// Run one routine by name with a JSON input.
pub fn replay(bytes: &[u8], routine: &str, input: &Value) -> Result<Value> {
    let r = read(bytes)?;
    fn parse<T: serde::de::DeserializeOwned>(v: &Value) -> Result<T> {
        serde_json::from_value(v.clone()).map_err(|e| Error::invalid(format!("input: {e}")))
    }
    match routine {
        "setup" => setup(&r, &parse(input)?),
        "teardown" => teardown(&r, &parse(input)?),
        "music_exec" => music_exec(&r, &parse(input)?),
        "env_exec" => env_exec(&r, &parse(input)?),
        "env_display" => env_display(&r, &parse(input)?),
        "env_display2" => env_display2(&r, &parse(input)?),
        "player_task_create" => player_task_create(&r, &parse(input)?),
        _ => Err(Error::invalid(format!("unknown stage 3 routine {routine}"))),
    }
}
