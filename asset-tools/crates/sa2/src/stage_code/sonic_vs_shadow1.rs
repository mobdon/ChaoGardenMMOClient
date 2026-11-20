//! Sonic vs Shadow 1 (stage 19, stg19D.rel): the stage program "STG19"
//! `_prolog` (+0xE4C) hands to LoadSubprogram (.data+0x6B9C), its stage
//! task, and the stage's own rival director ("ControlShadow"). Offsets are
//! stg19D section-1 offsets; engine calls are GSNE8P main.dol addresses.
//!
//! | Offset | Size | Routine | Port |
//! | --- | --- | --- | --- |
//! | +0xE78 | 0x2FC | stage setup (program entry 1) | [`setup`] |
//! | +0x1174 | 0xF8 | stage teardown (entry 2) | [`teardown`] |
//! | +0x126C | 0x100 | program exec (entry 3; same code in stg20D, stg29D) | [`exec`] |
//! | +0x136C | 0x4C | stage task exec: texture-id animation | [`task_exec`] |
//! | +0x13B8 | 0xA4 | stage task display: sky models | [`task_display`] |
//! | +0xD30 | 0x11C | director task create | [`director_create`] |
//! | +0x38C | 0x954 | director exec, nine modes | [`director_exec`] |
//! | +0x0 | 0x138 | keep the rival inside the arena (radius 130) | in [`director_exec`] |
//! | +0x1C4 | 0x130 | catch-up test | in [`director_exec`] |
//! | +0x2F4 | 0x98 | facing test (stg42D +0x334, `duel_director::h334`) | in [`director_exec`] |
use super::{fctiwz, sec, Ident, Returns, Run, StageRel};
use crate::{Error, Result};
use serde::Deserialize;
use serde_json::{json, Value};

pub const IDENT: Ident = Ident {
    name: "stage 19",
    len: 407_126,
    gc_sha256: "cf88d26b14ec012d1df022a743be670d266f31d51b5fcf1e60755980ad331afd",
    gc_module: 19,
    steam_sha256: "ce44ab1815ca7d7e957a2faccd2560a9117c5987f49cbf872977a4e40aff88fa",
    steam_module: 18,
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
    /// 0x801E7788 slot 1 (player 2's task) is non-null.
    pub player2: bool,
    /// The words at 0x801A2A18 +0, +8 and +0x24 before the setup.
    pub slots: [u32; 3],
    /// fn_8001DDA8, fn_80065048 (kept at .bss+0) and CreateElementalTask.
    pub returns: Returns,
}

/// +0xE78. `state.task` is the stage task's display callback (+0x14).
pub fn setup(r: &StageRel<'_>, i: &SetupInput) -> Result<Value> {
    let mut run = Run::new(r, i.returns.clone());
    let p5 = |pc| r.ptr(pc, 5);
    if run.query(0xe8c, 0x8001dda8, json!([0]))? as i32 == 0 {
        run.call(0xe9c, 0x80086e3c, json!([1]))?;
    } else if run.query(0xea8, 0x8001dda8, json!([0]))? as i32 == 1 {
        run.call(0xeb8, 0x800874c0, json!([1]))?;
    }
    r.dol(0xec0, 0x801e7788)?;
    let mut player2_byte2 = None;
    if i.player2 {
        run.call(0xed4, 0x8001d74c, json!([1]))?;
        r.dol(0xee0, 0x801e7768)?;
        player2_byte2 = Some(3); // li r0,3 / stb r0,2(playertwp[1]) at +0xED8
        run.call(0xef0, 0x80022e24, json!([1]))?;
        run.module(0xef4, json!([]))?;
    }
    run.call(0xf04, 0x800324e4, json!([128, p5(0xf00)?]))?;
    run.call(0xf18, 0x80072270, json!([p5(0xf0c)?, p5(0xf14)?]))?;
    run.call(0xf2c, 0x80072820, json!([p5(0xf20)?, p5(0xf28)?]))?;
    let handle = run.query(0xf38, 0x80065048, json!([0, 0x800]))? as u32;
    r.at(0xf40, 6)?; // .bss+0 keeps the handle
    run.call(0xf54, 0x8006638c, json!([p5(0xf4c)?, handle]))?;
    run.call(0xf58, 0x8001a8a8, json!([]))?;
    run.call(0xf5c, 0x80033710, json!([]))?;
    let task = run.query(
        0xf78,
        0x80071e0c,
        json!([2, 1, r.ptr(0xf6c, 1)?, p5(0xf74)?]),
    )? as u32;
    let display = if task != 0 {
        Some(r.ptr(0xf88, 1)?)
    } else {
        None
    };
    let (a, b, c) = (r.f(0xf94)?, r.f(0xfa0)?, r.f(0xfb8)?);
    run.call(0xfac, 0x800c3cd8, json!([bits(a), bits(b), bits(a)]))?;
    run.call(0xfc0, 0x8011e154, json!([bits(b), bits(c)]))?;
    run.call(0xfd0, 0x8011e150, json!([bits(b), bits(b), bits(b)]))?;
    let lights = r.at(0xfdc, 5)?;
    // addi r4,r31,32 at +0xFEC: the second light is the block plus 0x20.
    for (k, pc, off) in [
        (0, 0xfe4, 0),
        (1, 0xff0, 0x20),
        (2, 0xffc, 0),
        (3, 0x1008, 0),
    ] {
        run.call(pc, 0x8002506c, json!([k, sec(5, lights + off)]))?;
    }
    run.call(0x1014, 0x80025028, json!([0, 0]))?;
    run.call(0x1020, 0x80025028, json!([0, 1]))?;
    run.call(0x1028, 0x80025048, json!([0]))?;
    run.call(0x1030, 0x8002503c, json!([2]))?;
    run.module(0x1044, json!([p5(0x1038)?, r.ptr(0x1040, 6)?]))?;
    run.call(0x1048, 0x8011c2b0, json!([]))?;
    run.call(0x104c, 0x8011c310, json!([]))?;
    run.call(0x1058, 0x8003bcc4, json!([p5(0x1054)?]))?;
    let mut slots: Vec<Value> = i.slots.iter().map(|v| json!(v)).collect();
    for (k, (lis, pc)) in [(0x1060, 0x1074), (0x1080, 0x1098), (0x10a4, 0x10bc)]
        .into_iter()
        .enumerate()
    {
        r.dol(lis, 0x801a2a18)?;
        if i.slots[k] == 0 {
            slots[k] = r.ptr(pc, 1)?;
        }
    }
    run.module(0x10c4, json!([]))?;
    // Trail plugin table 0x801E77D8: fn_800E014C, three module routines, zeros.
    r.dol(0x10d4, 0x801e77d8)?;
    let mut trail = vec![json!(r.dol(0x10cc, 0x800e014c)?)];
    for pc in [0x10e0, 0x10ec, 0x10f8] {
        trail.push(r.ptr(pc, 1)?);
    }
    trail.resize(14, json!(0));
    run.call(0x1130, 0x80011a04, json!([0]))?;
    run.call(0x113c, 0x80011050, json!([p5(0x1138)?]))?;
    run.module(0x1148, json!([p5(0x1144)?]))?;
    run.call(0x115c, 0x8006d500, json!([p5(0x1150)?, r.ptr(0x1158, 4)?]))?;
    Ok(json!({ "calls": run.calls, "state": {
        "handle": handle, "player2Byte2": player2_byte2, "task": display,
        "slots": slots, "trail": trail } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TeardownInput {
    /// Each 0x801A2A18 slot: `"ours"` (the routine setup installed) or a word.
    pub slots: [Value; 3],
    /// .bss+0.
    pub handle: u32,
}

/// +0x1174: remove the setup's hooks and release the stage's resources.
pub fn teardown(r: &StageRel<'_>, i: &TeardownInput) -> Result<Value> {
    let mut run = Run::new(r, Returns::new());
    let p5 = |pc| r.ptr(pc, 5);
    run.module(0x1184, json!([]))?;
    let mut slots = Vec::new();
    for (v, (lis, own)) in
        i.slots
            .iter()
            .zip([(0x118c, 0x1198), (0x11b0, 0x11c0), (0x11d8, 0x11e8)])
    {
        r.dol(lis, 0x801a2a18)?;
        r.ptr(own, 1)?;
        slots.push(match v {
            Value::String(s) if s == "ours" => json!(0),
            v if v.is_u64() => v.clone(),
            _ => return Err(Error::invalid("teardown slot must be \"ours\" or a word")),
        });
    }
    run.module(0x11fc, json!([]))?;
    run.module(0x1200, json!([]))?;
    run.call(0x1204, 0x8011c2bc, json!([]))?;
    run.call(0x1208, 0x8011c310, json!([]))?;
    run.call(0x120c, 0x800336dc, json!([]))?;
    run.call(0x1218, 0x801166f0, json!([p5(0x1214)?]))?;
    run.call(0x122c, 0x80072118, json!([p5(0x1220)?, p5(0x1228)?]))?;
    run.call(0x1230, 0x80066364, json!([]))?;
    r.at(0x1238, 6)?;
    if i.handle != 0 {
        run.call(0x1248, 0x8001a2b0, json!([i.handle]))?;
    }
    run.call(0x1254, 0x800245b8, json!([]))?;
    Ok(json!({ "calls": run.calls, "state": { "handle": 0, "slots": slots } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ExecInput {
    /// Work (task +0x34) mode byte.
    pub mode: i8,
    /// Work +0x14..+0x1C.
    pub pos: [f32; 3],
    /// Player 2's position (playertwp[1] +0x14), if player 2 exists.
    pub player2: Option<[f32; 3]>,
}

/// +0x126C, the program exec. Mode 0 registers a camera-target request
/// (fn_800C8544) aimed at the work's position and moves to mode 1; mode 1
/// follows player 2, ten units above.
pub fn exec(r: &StageRel<'_>, i: &ExecInput) -> Result<Value> {
    let mut run = Run::new(r, Returns::new());
    let (mut mode, mut pos) = (i.mode, i.pos);
    if mode == 0 {
        let wide = r.f(0x12b0)?;
        // The request block on the stack (sp+12); bytes +0x12/+0x13 are not set.
        let block = json!({ "target": "work+0x14", "0x4": bits(wide), "0x8": bits(wide),
            "0xc": 10, "0x10": 1, "0x11": 0, "0x14": bits(r.f(0x12c8)?),
            "0x18": bits(r.f(0x12f0)?), "0x1c": 0 });
        run.call(0x1308, 0x800c8544, json!([0, block]))?;
        mode = 1;
    } else if mode == 1 {
        r.dol(0x1314, 0x801e7768)?;
        if let Some(p) = i.player2 {
            pos = [p[0], p[1] + r.f(0x1344)?, p[2]];
        }
    }
    Ok(json!({ "calls": run.calls, "state": { "mode": mode, "pos": pos } }))
}

/// +0x136C, the stage task exec: work +6 counts 0..13 and is written into
/// the low bits of the halfword at `state.at`, keeping its top two bits.
pub fn task_exec(r: &StageRel<'_>, counter: u16, texture: u16) -> Result<Value> {
    let counter = match counter.wrapping_add(1) {
        n if n >= 14 => 0, // cmplwi 14 at +0x1380
        n => n,
    };
    // lwz r3,4(.data+0x6A1C): a pointer the module relocates; +10 is the id.
    let (s, off) = r.pointer(5, r.at(0x1394, 5)? + 4)?;
    let texture = (texture & 0xc000) | counter;
    Ok(json!({ "calls": [], "state": {
        "counter": counter, "at": sec(s, off + 10), "texture": texture } }))
}

/// +0x13B8, the stage task display: two sky models drawn at the camera's
/// X/Z and a fixed height.
pub fn task_display(r: &StageRel<'_>, camera: [f32; 3]) -> Result<Value> {
    let mut run = Run::new(r, Returns::new());
    run.call(0x13c4, 0x8011c2b0, json!([]))?;
    run.call(0x13c8, 0x8011c310, json!([]))?;
    run.call(0x13d4, 0x80116e18, json!([r.ptr(0x13d0, 5)?]))?;
    run.call(0x13d8, 0x80116098, json!([]))?;
    // The camera is lbl_80175378[lbl_803ADAD0]; its X and Z are read.
    r.dol(0x13e0, 0x803adad0)?;
    r.dol(0x13f0, 0x80175378)?;
    let v = [camera[0], r.f(0x1408)?, camera[2]].map(bits);
    run.call(0x1420, 0x8011616c, json!([v]))?;
    run.call(0x142c, 0x8011df20, json!([r.ptr(0x1428, 5)?]))?;
    run.call(0x1438, 0x80116e18, json!([r.ptr(0x1434, 5)?]))?;
    run.call(0x1444, 0x8011df20, json!([r.ptr(0x1440, 5)?]))?;
    run.call(0x1448, 0x801160e4, json!([]))?;
    Ok(json!({ "calls": run.calls, "state": {} }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CreateInput {
    /// playerpwp[1] +3, the rival's character.
    pub character: i8,
    /// CreateElementalTask and syCalloc.
    pub returns: Returns,
}

/// +0xD30: create the director task (exec +0x38C) with a 20-byte work,
/// fill the rival's gauge and open the power gauge (+0x22984, the shared
/// `duel_gauge` creator) and the character's name card texture.
pub fn director_create(r: &StageRel<'_>, i: &CreateInput) -> Result<Value> {
    let mut run = Run::new(r, i.returns.clone());
    let task = run.query(
        0xd60,
        0x80071e0c,
        json!([0, 0, r.ptr(0xd54, 1)?, r.ptr(0xd5c, 5)?]),
    )? as u32;
    let mut state = json!({ "task": task });
    if task == 0 {
        return Ok(json!({ "calls": run.calls, "state": state }));
    }
    let work = run.query(0xd78, 0x801197fc, json!([1, 20]))? as u32;
    if work == 0 {
        run.call(0xd8c, 0x80071ae0, json!([task]))?;
        return Ok(json!({ "calls": run.calls, "state": state }));
    }
    r.dol(0xda8, 0x801e7728)?;
    state["work"] = json!(work);
    state["gauge"] = json!(r.f(0xd9c)?);
    run.module(0xdc4, json!([task, "playerpwp[1]+0x48"]))?;
    state["mode"] = json!(0);
    for (ch, tex, list, disp, call) in [
        (0, 0xde8, 0xdf0, 0xdfc, 0xdf4),
        (1, 0xe14, 0xe1c, 0xe28, 0xe20),
    ] {
        if i.character == ch {
            run.call(call, 0x80072820, json!([r.ptr(tex, 5)?, r.ptr(list, 5)?]))?;
            state["display"] = r.ptr(disp, 1)?;
        }
    }
    Ok(json!({ "calls": run.calls, "state": state }))
}

/// Player object (playertwp[i]) fields the director reads.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Body {
    pub action: i8,
    /// +4 halfword.
    pub flags4: i16,
    /// +0xC rotation Y.
    pub yaw: i32,
    /// +0x14.
    pub pos: [f32; 3],
}
/// Player data (playerpwp[i]) fields the director reads or writes.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Data {
    pub byte1: i8,
    pub character: i8,
    /// +0x10 halfword (bit 0x4000 ends the duel).
    pub flags10: i16,
    /// +0x1C halfword.
    pub field1c: i16,
    /// +0x48.
    pub gauge: f32,
    /// +0x68.
    pub field68: f32,
    /// +0x1B4.
    pub field1b4: f32,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DirectorInput {
    /// Game bytes at 0x801CC168 +0x37, +0x39, +0x42 and the word +0x7C.
    pub g37: i8,
    pub g39: i8,
    pub g42: i8,
    pub g7c: u32,
    pub bodies: [Body; 2],
    pub data: [Data; 2],
    /// Work: mode, +1 flag, +2 timer, +4 limit, +8 point.
    pub mode: i8,
    pub flag: i8,
    pub timer: i16,
    pub limit: i16,
    pub point: [f32; 3],
    /// Director task +0xC (child).
    pub child: u32,
    /// playertp[1].
    pub player2_task: u32,
    /// Engine and module results; `"task+0xc"` is the child word read again
    /// after DestroyTask removed it.
    pub returns: Returns,
}

struct Dir<'r, 'a> {
    run: Run<'r, 'a>,
    i: DirectorInput,
    k: f64,
    camera: Option<(i32, f32)>,
    flags: bool,
    flag898: bool,
}

impl Dir<'_, '_> {
    fn r(&self) -> &StageRel<'_> {
        self.run.rel
    }
    /// atan2 of player 1 relative to `to`, as the 0x4000-based BAMS angle.
    fn angle(&mut self, pc: u32, from: [f32; 2], to: [f32; 3]) -> Result<i32> {
        let (dx, dz) = (from[0] - to[0], from[1] - to[2]);
        let a = self
            .run
            .query(pc, 0x80111168, json!([bits(dx), bits(dz)]))? as f32;
        Ok(0x4000i32.wrapping_sub(fctiwz(self.k * f64::from(a))))
    }
    fn facing(&mut self, atan: u32, diff: u32) -> Result<bool> {
        let [p1, p2] = self.i.bodies;
        let ang = self.angle(atan, [p1.pos[0], p1.pos[2]], p2.pos)?;
        Ok((self.run.query(diff, 0x8002b8e0, json!([ang, p2.yaw]))? as i32) < 0x5000)
    }
    /// +0x1C4: the rival is ahead of player 1 and close behind it.
    fn catch_up(&mut self) -> Result<bool> {
        let a = self.i.bodies[0].action;
        if a == 13 && self.facing(0x208, 0x230)? {
            return Ok(true);
        }
        if !(a == 6 || a == 3 || a == 4) || !self.facing(0x27c, 0x2a4)? {
            return Ok(false);
        }
        let [p1, p2] = self.i.bodies;
        let d = self.run.query(
            0x2b8,
            0x8011b65c,
            json!([p1.pos.map(bits), p2.pos.map(bits)]),
        )?;
        Ok((d as f32) < self.r().f(0x2c0)?)
    }
    /// +0x2F4 (stg42D +0x334).
    fn facing_attack(&mut self) -> Result<bool> {
        Ok((61..=68).contains(&self.i.bodies[0].action) && self.facing(0x338, 0x360)?)
    }
    fn caught(&mut self) -> Result<bool> {
        Ok(self.catch_up()? || self.facing_attack()?)
    }
    fn enter_mode3(&mut self) {
        self.i.timer = 0;
        self.flags = true;
        self.i.mode = 3;
    }
    fn rand_unit(&mut self, pc: u32, scale_pc: u32) -> Result<f32> {
        let r = self.run.query(pc, 0x8010dc88, json!([]))? as i32;
        Ok(self.r().f(scale_pc)? * r as f32)
    }
    fn aim(&mut self, pc: u32, from: [f32; 2]) -> Result<()> {
        let p2 = self.i.bodies[1].pos;
        let a = self.angle(pc, from, p2)?;
        self.camera = Some((a, self.r().f(pc + 0x34)?));
        Ok(())
    }
    /// +0x0: push the rival back inside the arena circle.
    fn arena(&mut self) -> Result<()> {
        let r = self.run.rel;
        let c = r.at(0x2c, 5)?;
        let centre = [r.float(5, c)?, r.float(5, c + 4)?, r.float(5, c + 8)?];
        let radius = r.float(5, r.at(0x3c, 5)?)?;
        let p = self.i.bodies[1].pos;
        let d = self
            .run
            .query(0x34, 0x8011b65c, json!([p.map(bits), centre.map(bits)]))?
            as f32;
        if d >= radius {
            let y = if p[1] < centre[1] { centre[1] } else { p[1] };
            let sq = p[2] * p[2] + (p[0] * p[0] + y * y);
            let len = self.run.module_query(0x84, json!([bits(sq)]))? as f32;
            let one = r.f(0x8c)?;
            let s = (radius - one) * (one / len);
            let x = centre[0] + s * (p[0] - centre[0]);
            let z = centre[2] + s * (p[2] - centre[2]);
            self.run
                .call(0xdc, 0x800399bc, json!([1, bits(x), bits(y), bits(z)]))?;
        } else if p[1] < centre[1] {
            self.run.call(
                0x108,
                0x800399bc,
                json!([1, bits(p[0]), bits(centre[1]), bits(p[2])]),
            )?;
            self.i.bodies[1].pos[1] = centre[1];
        }
        Ok(())
    }
    fn update(&mut self) -> Result<()> {
        let r = self.run.rel;
        r.dol(0x3b0, 0x801cc168)?;
        if self.i.g39 == 1 || self.i.g37 & 2 != 0 || self.i.g42 == 1 {
            return Ok(());
        }
        r.dol(0x3f0, 0x801e7768)?;
        r.dol(0x400, 0x801e7728)?;
        let [d1, d2] = self.i.data;
        if d1.flags10 & 0x4000 != 0 || d2.flags10 & 0x4000 != 0 {
            self.i.mode = 9;
        }
        let jump = r.at(0x44c, 5)?;
        // The jump table's nine entries (mode 8 reuses mode 2's code).
        let mut targets = Vec::new();
        for k in 0..9 {
            targets.push(r.pointer(5, jump + 4 * k)?.1);
        }
        let target = match self.i.mode {
            m @ 0..=8 => Some(targets[m as usize]),
            _ => None,
        };
        let [p1, p2] = self.i.bodies;
        match target {
            Some(0x460) => {
                if self.run.query(0x460, 0x8001cd40, json!([]))? as i32 == 16 {
                    self.run.call(0x470, 0x80022e24, json!([3]))?;
                    self.run.call(0x474, 0x8001dd50, json!([]))?;
                    self.i.mode = 1;
                    self.i.limit = 360;
                    if d2.character == 0 {
                        self.run.module(0x4a4, json!([0, r.ptr(0x4a0, 5)?]))?;
                    } else if d2.character == 1 {
                        self.run.module(0x4c0, json!([0, r.ptr(0x4bc, 5)?]))?;
                    }
                    let base = r.at(0x50c, 5)?;
                    self.i.point = p2.pos;
                    self.i.point[1] = r.f(0x500)? + r.float(5, base + 4)?;
                    let block = json!({ "0x0": bits(r.f(0x4c8)?),
                        "0x4": p2.yaw.wrapping_add(0x10000).wrapping_sub(0x1000),
                        "0x8": "work+0x8", "0xc": bits(r.f(0x528)?), "0x10": bits(r.f(0x538)?),
                        "0x14": 0x4000, "point": self.i.point.map(bits) });
                    self.run.call(0x554, 0x800c7aac, json!([0, block]))?;
                }
            }
            Some(0x55c) => {
                if self.i.timer > self.i.limit {
                    if self.i.flag == 0 {
                        let base = r.at(0x584, 5)?;
                        let mut pos = [
                            r.float(5, base)?,
                            r.float(5, base + 4)?,
                            r.float(5, base + 8)?,
                        ];
                        pos[1] += r.f(0x5a8)?;
                        self.i.flag = 1;
                        let kind = match self.run.query(0x5c0, 0x8001dda0, json!([]))? as i32 {
                            0 => None,
                            1 => Some(0),
                            2 => Some(1),
                            _ => Some(2),
                        };
                        if let Some(mut k) = kind {
                            if d1.byte1 == 1 {
                                k += 3;
                            }
                            self.run
                                .module(0x630, json!([pos.map(bits), 0, 0, k, 2, 0]))?;
                        }
                    }
                    self.run.call(0x638, 0x80022e64, json!([0]))?;
                    self.run.call(0x63c, 0x8001dd64, json!([]))?;
                    self.i.timer = 0;
                    self.i.mode = 5;
                    let u = self.rand_unit(0x650, 0x67c)?;
                    self.i.limit = fctiwz(f64::from(r.f(0x69c)? + r.f(0x68c)? * u)) as i16;
                    if self.run.query(0x6bc, 0x800c3bdc, json!([0]))? as i32 == 13 {
                        self.run.call(0x6cc, 0x800c77a8, json!([0]))?;
                    }
                } else if self.i.timer == 120 {
                    let voice = match d2.character {
                        0 => 2236,
                        1 => 2212,
                        _ => -1,
                    };
                    if voice != -1 {
                        self.run.call(0x70c, 0x80011714, json!([voice]))?;
                    }
                }
            }
            Some(0x714) => {
                if d1.field1c & 3 == 0 {
                    r.dol(0x764, 0x801e53f8)?;
                    self.aim(0x73c, [p1.pos[0], p1.pos[2]])?;
                }
                if self.caught()? {
                    self.enter_mode3();
                } else if self.i.timer > 60 {
                    let u = self.rand_unit(0x7e8, 0x814)?;
                    self.i.timer = 0;
                    self.flags = true;
                    if u < r.f(0x824)? {
                        self.i.mode = 3;
                    } else {
                        self.i.mode = 5;
                        let u = self.rand_unit(0x890, 0x814)?;
                        self.i.limit = fctiwz(f64::from(r.f(0x8d0)? + r.f(0x8c0)? * u)) as i16;
                    }
                }
            }
            Some(t @ (0x8f0 | 0x974)) => {
                let next = if t == 0x8f0 { 4 } else { 6 };
                if self.i.timer >= 2 && p2.flags4 & 3 != 0 {
                    self.i.mode = 8;
                } else {
                    if t == 0x8f0 {
                        self.flag898 = true;
                    }
                    let (zero, limit) = if t == 0x8f0 {
                        (r.f(0x920)?, r.f(0x944)?)
                    } else {
                        (r.f(0x994)?, r.f(0x9b8)?)
                    };
                    if d2.field68 == zero && p2.pos[1] - d1.field1b4 > limit {
                        self.i.mode = next;
                        self.i.timer = 0;
                        if t == 0x974 {
                            self.flags = true;
                        }
                    }
                }
            }
            Some(0xa04) => {
                if self.caught()? {
                    self.enter_mode3();
                } else if self.i.timer > self.i.limit {
                    self.i.mode = 2;
                    self.i.timer = 0;
                } else {
                    // A point 35 units from player 1 at the game clock's angle.
                    r.dol(0xa8c, 0x801cc168)?;
                    let ang = (self.i.g7c << 8) as i32;
                    let reach = r.f(0xaa4)?;
                    let x =
                        p1.pos[0] + reach * self.run.query(0xa9c, 0x80112398, json!([ang]))? as f32;
                    let z =
                        p1.pos[2] + reach * self.run.query(0xac0, 0x801123b0, json!([ang]))? as f32;
                    r.dol(0xb0c, 0x801e53f8)?;
                    let a = self.angle(0xae4, [x, z], p2.pos)?;
                    self.camera = Some((a, r.f(0xb18)?));
                }
            }
            Some(0xb28) => {
                if self.i.timer >= 2 && p2.flags4 & 3 != 0 {
                    self.i.mode = 7;
                    self.i.timer = 0;
                }
            }
            Some(0xb58) => {
                if self.caught()? {
                    self.enter_mode3();
                } else if self.i.timer > 60 {
                    self.i.mode = 2;
                    self.i.timer = 0;
                }
            }
            Some(_) => return Err(Error::invalid("unexpected stage 19 director jump table")),
            None => {}
        }
        self.arena()?;
        r.dol(0xbe4, 0x801cc168)?;
        let reset = self.i.g7c == 0
            || (self.run.query(0xbf4, 0x8001cd40, json!([]))? as i32 == 7
                && self.i.mode != 0
                && self.i.mode != 1);
        if reset {
            self.i.data[1].gauge = r.f(0xc1c)?;
            if self.i.child != 0 {
                self.run.call(0xc34, 0x80071af0, json!([self.i.child]))?;
                self.i.child = self.run.take("task+0xc")? as u32;
            }
            self.i.flag = 0;
            self.i.timer = 0;
            self.i.mode = 0;
        } else {
            self.i.timer = self.i.timer.wrapping_add(1);
        }
        let zero = r.f(0xc60)?;
        if self.i.data[1].gauge > zero && self.i.child == 0 {
            self.run
                .module(0xc88, json!(["task", "playerpwp[1]+0x48"]))?;
        }
        if self.i.data[1].gauge <= r.f(0xc94)? {
            r.dol(0xcb0, 0x801e7788)?;
            self.run
                .call(0xcc0, 0x8003a54c, json!([0, self.i.player2_task, 0, 3]))?;
        }
        Ok(())
    }
}

/// +0x38C, the director exec (the task the setup creates through +0xD30).
/// Modes: 0 wait for the intro (fn_8001CD40 == 16), 1 intro timer, voice
/// line at 120 and the opening text, 2/8 chase with the camera on the
/// rival, 3/4 and 6/7 the rival's jump sequences, 5 roam around player 1,
/// 9 (bit 0x4000 of either player's +0x10) stops the modes. Every mode then
/// keeps the rival inside the arena (+0x0), resets on a restart and reopens
/// the gauge while it has power; an empty gauge ends player 2.
pub fn director_exec(r: &StageRel<'_>, i: &DirectorInput) -> Result<Value> {
    let k = r.fd(0x210)?;
    let mut d = Dir {
        run: Run::new(r, i.returns.clone()),
        i: i.clone(),
        k,
        camera: None,
        flags: false,
        flag898: false,
    };
    d.update()?;
    let i = &d.i;
    Ok(json!({ "calls": d.run.calls, "state": {
        "mode": i.mode, "flag": i.flag, "timer": i.timer, "limit": i.limit,
        "point": i.point, "child": i.child, "gauge2": i.data[1].gauge,
        "player2Y": i.bodies[1].pos[1],
        // 0x801E53F8 +8 (angle) and +0xC.
        "camera": d.camera.map(|(a, f)| json!([a, f])),
        // Bytes 0x803AD888+1 and 0x803AD898+1 set to 1.
        "flag888": d.flags, "flag898": d.flags || d.flag898 } }))
}

/// Run one routine by name with a JSON input (the CLI/test entry point).
pub fn replay(bytes: &[u8], routine: &str, input: &Value) -> Result<Value> {
    let r = read(bytes)?;
    fn parse<T: serde::de::DeserializeOwned>(v: &Value) -> Result<T> {
        serde_json::from_value(v.clone()).map_err(|e| Error::invalid(format!("input: {e}")))
    }
    let u16_of = |k: &str| {
        input[k]
            .as_u64()
            .and_then(|v| u16::try_from(v).ok())
            .ok_or_else(|| Error::invalid(format!("missing {k}")))
    };
    match routine {
        "setup" => setup(&r, &parse(input)?),
        "teardown" => teardown(&r, &parse(input)?),
        "exec" => exec(&r, &parse(input)?),
        "task_exec" => task_exec(&r, u16_of("counter")?, u16_of("texture")?),
        "task_display" => task_display(&r, parse(&input["camera"])?),
        "director_create" => director_create(&r, &parse(input)?),
        "director_exec" => director_exec(&r, &parse(input)?),
        _ => Err(Error::invalid(format!(
            "unknown stage 19 routine {routine}"
        ))),
    }
}
