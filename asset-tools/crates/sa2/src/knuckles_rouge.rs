//! Knuckles vs Rouge (stage 33, stg33D.rel): the stage program `_prolog`
//! (+0x38E0) hands to LoadSubprogram ("STG33", .data+0x15370), its stage
//! task, the rival director's spawner, the falling-sparkle effect, the
//! stage's own LIGHTEFF object and the ITEMBOXBAL GEN object. All offsets
//! are section-1 (code) offsets of stg33D.rel; engine calls are reported at
//! their GSNE8P main.dol addresses.
//!
//! | Offset | Size | Routine |
//! | --- | --- | --- |
//! | +0x390C | 0x320 | stage setup (the program's first entry) |
//! | +0x3C2C | 0xFC | stage teardown (second entry) |
//! | +0x3D28 | 0x4 | stage program exec (third entry): `blr`, no effect |
//! | +0x3D2C | 0x154 | stage task exec (texture animation, light flicker) |
//! | +0x3E80 | 0x4 | stage task destructor: `blr`, no effect |
//! | +0x3E84 | 0x398 | stage task display (sky and scenery models) |
//! | +0x421C | 0x124 | model-list draw (shared with stg31/32/43/44) |
//! | +0x3354 | 0x128 | rival director spawn (director exec +0xCE8) |
//! | +0x3664 | 0x27C | sparkle spawn |
//! | +0x35E0 | 0x60 | sparkle exec |
//! | +0x347C | 0x164 | sparkle display |
//! | +0x4340 | 0x70 | LIGHTEFF loader (object list row 37) |
//! | +0x4590 | 0x188 | LIGHTEFF exec |
//! | +0x43B4 | 0x1DC | LIGHTEFF display |
//! | +0x1AB0C | 0x30 | ITEMBOXBAL GEN loader (row 14) |
//! | +0x1A9D8 | 0x130 | ITEMBOXBAL GEN exec |
//!
//! Constants and data pointers are read from the module through the
//! relocation of the instruction that loads them (the offsets are recorded
//! at each use); every engine call is checked against the relocation of
//! its `bl`. The Steam `resource/gd_PC/stg33D.rel` has the same section
//! bytes as the GameCube disc copy, so either can be passed; engine
//! addresses are always the GSNE8P ones. REL-local helpers outside this
//! port are reported as `module` calls; engine results (rand, sine, colour
//! lerp, projected depth, task creation) are inputs.
use crate::{
    city_rel::{CityRel, Reference},
    manifest, Error, Result,
};
use serde::Deserialize;
use serde_json::{json, Value};

/// GameCube disc `stg33D.rel` (module 33).
pub const GC_REL_SHA256: &str = "0de8a3a7d046618c1aca97c5d0b0c2801e6deb92878c419a258fcaf4fcb39585";
/// Installed Steam `resource/gd_PC/stg33D.rel` (module 32, same section bytes).
pub const STEAM_REL_SHA256: &str =
    "9f2d09613eb762677e1e64a7430d963157e1bcd68fbbe0eb7c54fc6fb36e9942";
pub const REL_LEN: usize = 526_430;
const GC_MODULE: u32 = 33;

fn invalid() -> Error {
    Error::invalid("unexpected stage 33 code")
}
fn s(section: u8, offset: u32) -> Value {
    json!(format!("s{section}+0x{offset:x}"))
}
fn module_call(offset: u32, args: Value) -> Value {
    json!({ "module": format!("s1+0x{offset:x}"), "args": args })
}
/// `fctiwz`: truncate toward zero, saturating; NaN gives 0x80000000.
fn fctiwz(x: f32) -> i32 {
    if x.is_nan() {
        i32::MIN
    } else {
        x as i32
    }
}
fn neg(bits: u32) -> u32 {
    bits ^ 0x8000_0000
}

pub struct KnucklesRouge<'a> {
    rel: CityRel<'a>,
}

impl<'a> KnucklesRouge<'a> {
    pub fn read(bytes: &'a [u8]) -> Result<Self> {
        if bytes.len() != REL_LEN {
            return Err(Error::invalid("unsupported stage 33 REL revision"));
        }
        let module = match manifest::sha256(bytes).as_str() {
            GC_REL_SHA256 => GC_MODULE,
            STEAM_REL_SHA256 => 32,
            _ => return Err(Error::invalid("unsupported stage 33 REL revision")),
        };
        Ok(Self {
            rel: CityRel::read_identified(bytes, module)?,
        })
    }
    /// The section offset the `addi` at `pc` (relocated at `pc + 2`) points at.
    fn at(&self, pc: u32, section: u8) -> Result<u32> {
        match self.rel.reference(1, pc + 2) {
            Some(Reference {
                module,
                kind: 4,
                section: s,
                addend,
            }) if s == section && module == self.rel.module_id() => Ok(addend),
            _ => Err(Error::invalid(format!(
                "unexpected stage 33 code at +0x{pc:x}"
            ))),
        }
    }
    fn ptr(&self, pc: u32, section: u8) -> Result<Value> {
        Ok(s(section, self.at(pc, section)?))
    }
    fn word(&self, section: u8, at: u32) -> Result<u32> {
        let d = self.rel.data(section)?;
        d.get(at as usize..at as usize + 4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
            .ok_or_else(invalid)
    }
    fn byte(&self, section: u8, at: u32) -> Result<u8> {
        self.rel
            .data(section)?
            .get(at as usize)
            .copied()
            .ok_or_else(invalid)
    }
    /// The single-precision constant (.rodata) the `addi` at `pc` addresses, as bits.
    fn f(&self, pc: u32) -> Result<u32> {
        self.word(4, self.at(pc, 4)?)
    }
    /// The double (.rodata) the `addi` at `pc` addresses.
    fn d(&self, pc: u32) -> Result<f64> {
        let at = self.at(pc, 4)?;
        Ok(f64::from_bits(
            (u64::from(self.word(4, at)?) << 32) | u64::from(self.word(4, at + 4)?),
        ))
    }
    /// The data word (.data) the `addi` at `pc` addresses.
    fn dw(&self, pc: u32) -> Result<u32> {
        self.word(5, self.at(pc, 5)?)
    }
    /// The section-5 target of the pointer stored at section-5 `at`.
    fn deref(&self, at: u32) -> Result<u32> {
        match self.rel.local_pointer(5, at)? {
            Some((5, t)) => Ok(t),
            _ => Err(invalid()),
        }
    }
    /// The int-to-float conversion the module compiles to: the double
    /// `0x43300000_(r ^ 0x80000000)` minus the magic at `pc`, rounded by fsubs.
    fn int_f(&self, pc: u32, r: i32) -> Result<f32> {
        let x = f64::from_bits(0x4330_0000_0000_0000 | u64::from(r as u32 ^ 0x8000_0000));
        Ok((x - self.d(pc)?) as f32)
    }
    /// The target of the module-local `bl` at `pc`.
    fn local(&self, pc: u32) -> Result<u32> {
        let ins = self.word(1, pc)?;
        if ins & 0xfc00_0003 != 0x4800_0001 || self.rel.reference(1, pc).is_some() {
            return Err(Error::invalid(format!("no local call at +0x{pc:x}")));
        }
        let disp = ((ins & 0x03ff_fffc) as i32) << 6 >> 6;
        pc.checked_add_signed(disp).ok_or_else(invalid)
    }
    fn local_call(&self, pc: u32, args: Value) -> Result<Value> {
        Ok(module_call(self.local(pc)?, args))
    }
    /// The engine call at `pc`: the `bl` must be relocated against the main
    /// program, at `address` in the GC module (the Steam module links
    /// another main program, so only the kind is checked there).
    fn e(&self, pc: u32, address: u32, args: Value) -> Result<Value> {
        match self.rel.reference(1, pc) {
            Some(Reference {
                module: 0,
                kind: 10,
                addend,
                ..
            }) if addend == address || self.rel.module_id() != GC_MODULE => {
                Ok(json!({ "engine": format!("0x{address:08x}"), "args": args }))
            }
            _ => Err(Error::invalid(format!(
                "no engine call 0x{address:08x} at stage 33 +0x{pc:x}"
            ))),
        }
    }
}

/// A word that is either a plain value or one of the module's own
/// addresses (`"s1+0x3d2c"`).
pub type Word = Value;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupInput {
    /// fn_8001DDA8(0)'s returns in call order (a second call is made only
    /// when the first is not 5).
    pub modes: Vec<i32>,
    /// playertp[1] is set.
    pub player1: bool,
    /// fn_800651EC's return, kept at .bss+8.
    pub handle: u32,
    /// CreateElementalTask returned the stage task.
    pub task: bool,
    /// The words at 0x801A2A18 +0, +8 and +0x20 before the setup.
    pub slots: [u32; 3],
}

/// +0x390C. Returns the ordered calls and the final value of every location
/// it writes (`task` is the stage task's callbacks at +0x14 and +0x18).
pub fn setup(k: &KnucklesRouge<'_>, i: &SetupInput) -> Result<Value> {
    let p5 = |pc| k.ptr(pc, 5);
    let f = |pc| k.f(pc);
    let mut modes = i.modes.iter().copied();
    let mut next = || modes.next().ok_or_else(|| Error::invalid("missing mode"));
    let mut c = vec![k.e(0x3924, 0x8001dda8, json!([0]))?];
    if next()? == 5 {
        c.push(k.e(0x3934, 0x800a2360, json!([1]))?);
    } else {
        c.push(k.e(0x3940, 0x8001dda8, json!([0]))?);
        if next()? == 4 {
            c.push(k.e(0x3950, 0x800a1f00, json!([1]))?);
        }
    }
    let mut player1_action = None;
    if i.player1 {
        c.push(k.e(0x396c, 0x8001d74c, json!([1]))?);
        player1_action = Some(3); // stb 3 to playertwp[1]+2 at +0x3980
        c.push(k.e(0x3988, 0x80022e24, json!([1]))?);
        c.push(k.local_call(0x398c, json!([]))?);
    }
    let (a, b) = (p5(0x39a4)?, p5(0x39b0)?);
    let (a2, b2) = (p5(0x39cc)?, p5(0x39d8)?);
    c.extend([
        k.e(0x399c, 0x800324e4, json!([128, p5(0x3998)?]))?,
        k.e(0x39b8, 0x80072270, json!([a, b]))?,
        k.e(0x39c4, 0x80072218, json!([a, b]))?,
        k.e(0x39e0, 0x80072270, json!([a2, b2]))?,
        k.e(0x39ec, 0x80072218, json!([a2, b2]))?,
        k.e(0x3a04, 0x800651ec, json!([p5(0x39f4)?, p5(0x39fc)?, 512]))?,
    ]);
    k.at(0x3a0c, 6)?; // .bss+8 holds the handle
    let (zero, one) = (f(0x3a30)?, f(0x3a3c)?);
    let lights = k.at(0x3a78, 5)?;
    c.extend([
        k.e(0x3a20, 0x8006638c, json!([p5(0x3a18)?, i.handle]))?,
        k.e(0x3a24, 0x8001a8a8, json!([]))?,
        k.e(0x3a28, 0x80033710, json!([]))?,
        k.e(0x3a48, 0x800c3cd8, json!([zero, one, zero]))?,
        k.e(0x3a5c, 0x8011e154, json!([one, f(0x3a54)?]))?,
        k.e(0x3a6c, 0x8011e150, json!([one, one, one]))?,
        // Light 1 is the block plus 0x20 (addi at +0x3A88); the others reuse it.
        k.e(0x3a80, 0x8002506c, json!([0, s(5, lights)]))?,
        k.e(0x3a8c, 0x8002506c, json!([1, s(5, lights + 0x20)]))?,
        k.e(0x3a98, 0x8002506c, json!([2, s(5, lights)]))?,
        k.e(0x3aa4, 0x8002506c, json!([3, s(5, lights)]))?,
        k.e(0x3ab0, 0x80025028, json!([0, 0]))?,
        k.e(0x3abc, 0x80025028, json!([0, 1]))?,
        k.e(0x3ac4, 0x80025048, json!([0]))?,
        k.e(0x3acc, 0x8002503c, json!([2]))?,
        k.e(
            0x3ae8,
            0x80071e0c,
            json!([2, 1, k.ptr(0x3adc, 1)?, p5(0x3ae4)?]),
        )?,
    ]);
    let task = if i.task {
        Some([k.ptr(0x3af8, 1)?, k.ptr(0x3b04, 1)?])
    } else {
        None
    };
    let mut slots: Vec<Word> = i.slots.iter().map(|v| json!(v)).collect();
    for (n, pc) in [(0, 0x3b24), (1, 0x3b48), (2, 0x3b6c)] {
        if i.slots[n] == 0 {
            slots[n] = k.ptr(pc, 1)?;
        }
    }
    k.at(0x3b80, 6)?;
    c.extend([
        k.local_call(0x3b84, json!([p5(0x3b78)?, "s6+0x14"]))?,
        k.e(0x3b88, 0x8011c2bc, json!([]))?,
        k.e(0x3b8c, 0x8011c310, json!([]))?,
        k.local_call(0x3b98, json!([p5(0x3b94)?]))?,
    ]);
    // Trail plugin table 0x801E77D8: fn_800E014C, three module routines, zeros.
    let mut trail = vec![json!(0x800e014c_u32)];
    for pc in [0x3bb4, 0x3bc0, 0x3bcc] {
        trail.push(k.ptr(pc, 1)?);
    }
    trail.resize(14, json!(0));
    c.extend([
        k.e(0x3c04, 0x80011a04, json!([0]))?,
        k.e(0x3c10, 0x80011050, json!([p5(0x3c0c)?]))?,
    ]);
    Ok(json!({ "calls": c, "state": {
        "handle": i.handle, "player1_action": player1_action, "slots": slots,
        "trail": trail, "task": task } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeardownInput {
    /// Each 0x801A2A18 slot (+0, +8, +0x20): `"ours"` (the routine setup
    /// installed) or a value.
    pub slots: [Word; 3],
    /// .bss+8.
    pub handle: u32,
}

/// +0x3C2C: undo the setup's hooks and release the stage's resources.
pub fn teardown(k: &KnucklesRouge<'_>, i: &TeardownInput) -> Result<Value> {
    let p5 = |pc| k.ptr(pc, 5);
    let mut c = vec![
        k.local_call(0x3c3c, json!([]))?,
        k.local_call(0x3c40, json!([]))?,
        k.e(0x3c44, 0x8011c2bc, json!([]))?,
        k.e(0x3c48, 0x8011c310, json!([]))?,
    ];
    // Checked in the order +0x20, +0, +8; only our own routine is cleared.
    for pc in [0x3c60, 0x3c84, 0x3cac] {
        k.at(pc, 1)?;
    }
    let mut slots = Vec::new();
    for v in &i.slots {
        slots.push(if v == "ours" {
            json!(0)
        } else if v.is_u64() {
            v.clone()
        } else {
            return Err(Error::invalid("teardown slot must be \"ours\" or a word"));
        });
    }
    c.extend([
        k.e(0x3cc0, 0x800336dc, json!([]))?,
        k.e(0x3cd4, 0x80072118, json!([p5(0x3cc8)?, p5(0x3cd0)?]))?,
        k.e(0x3ce8, 0x80072118, json!([p5(0x3cdc)?, p5(0x3ce4)?]))?,
        k.e(0x3cec, 0x80066364, json!([]))?,
    ]);
    k.at(0x3cf4, 6)?;
    if i.handle != 0 {
        c.push(k.e(0x3d04, 0x8001a2b0, json!([i.handle]))?);
    }
    c.push(k.e(0x3d10, 0x800245b8, json!([]))?);
    Ok(json!({ "calls": c, "state": { "handle": 0, "slots": slots } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskExecInput {
    /// Game state 0x801CC168 +0x7C (frame clock).
    pub clock: u32,
    /// njSin's result (bits).
    pub sin: u32,
    /// fn_800334B0's result (the blended colour).
    pub colour: u32,
    /// Stage task work +6.
    pub timer: u16,
    /// The halfword the task rewrites (see `task_exec`), before the exec.
    pub frame: u16,
}

/// +0x3D2C, the stage task exec: scroll or recolour the two animated
/// textures of the table at .data+0x15528, then step the 13-frame counter
/// into the low bits of a texture-number halfword.
pub fn task_exec(k: &KnucklesRouge<'_>, i: &TaskExecInput) -> Result<Value> {
    let scroll = i.clock.wrapping_mul(k.dw(0x3d54)?) & 0xffff;
    let table = k.at(0x3d68, 5)?;
    let wave = k.f(0x3d88)?;
    let w = f32::from_bits(wave);
    let level = w + w * f32::from_bits(i.sin); // fmuls then fadds
    let mut c = vec![
        k.e(
            0x3d80,
            0x80112398,
            json!([i.clock.wrapping_mul(k.dw(0x3d74)?) as i32]),
        )?,
        k.e(
            0x3db0,
            0x800334b0,
            json!([k.dw(0x3d9c)?, k.dw(0x3da8)?, level.to_bits()]),
        )?,
    ];
    for n in 0..2 {
        let e = table + 20 * n;
        let model = k.deref(k.deref(e + 4)? + 4)?;
        if k.word(5, e)? == 0 {
            let speed = |o| -> Result<u32> { Ok(k.word(5, e + o)?.wrapping_mul(scroll) & 0x1ff) };
            c.push(k.e(
                0x3df8,
                0x80072b2c,
                json!([s(5, model), s(5, k.deref(e + 8)?), speed(16)?, speed(12)?]),
            )?);
        } else {
            c.push(k.local_call(0x3e10, json!([s(5, k.deref(model)?), i.colour]))?);
        }
    }
    let mut timer = i.timer.wrapping_add(1);
    if timer >= 13 {
        timer = 0;
    }
    // .data+0x152A8 +4 points at the texture-number list; its +0xA halfword.
    let at = k.deref(k.at(0x3e48, 5)? + 4)? + 10;
    let frame = (i.frame & 0xc000) | timer;
    Ok(json!({ "calls": c, "state": {
        "timer": timer, "frame": frame, "frame_at": s(5, at) } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayInput {
    /// Game state 0x801CC168 +0x3D (split-screen mode byte).
    pub mode: i8,
    /// 0x803ADAD0, the screen being drawn.
    pub screen: i32,
    /// Game state +0x7C.
    pub clock: u32,
    /// .bss+0x10 (sky height, bits) and .bss+0xC (third model's X spin rate).
    pub sky_y: u32,
    pub spin: u32,
    /// Game state +0xA8 / +0xAC, the draw mask of screen 0 / other screens.
    pub masks: [u32; 2],
    /// njCalcPoint's depth (+8, bits) for each model-list entry visited.
    pub depths: Vec<u32>,
}

/// +0x3E84, the stage task display. Skipped for split-screen modes 1 (screen
/// 0), 2 (screen 1) and 3; otherwise draws the sky, three spinning scenery
/// models, the model list .data+0x8AB4 (+0x421C) and one more model.
pub fn display(k: &KnucklesRouge<'_>, i: &DisplayInput) -> Result<Value> {
    if (i.mode == 1 && i.screen == 0) || (i.mode == 2 && i.screen == 1) || i.mode == 3 {
        return Ok(json!({ "calls": [], "state": {} }));
    }
    let p5 = |pc| k.ptr(pc, 5);
    let zero = k.f(0x3f04)?;
    k.at(0x3f3c, 6)?;
    let sky_scale = k.dw(0x3f50)?;
    let mut c = vec![
        k.e(0x3ef8, 0x80116098, json!([]))?,
        k.e(0x3f1c, 0x80115bd8, json!([0, zero, k.dw(0x3f10)?, zero]))?,
        k.e(0x3f28, 0x80116e18, json!([p5(0x3f24)?]))?,
        k.e(0x3f2c, 0x80116098, json!([]))?,
        k.e(0x3f48, 0x80115bd8, json!([0, zero, i.sky_y, zero]))?,
        k.e(
            0x3f6c,
            0x80115178,
            json!([0, sky_scale, k.f(0x3f60)?, sky_scale]),
        )?,
        k.e(0x3f78, 0x8011e17c, json!([p5(0x3f74)?]))?,
        k.e(0x3f7c, 0x801160e4, json!([]))?,
    ];
    // One spinning model: (push, object, texlist, set texture, translate ex,
    // translate, rot-X rate, rot X, rot-Y rate, rot Y, scale, scale, translate,
    // draw, pop) call/load sites.
    struct Spin {
        push: u32,
        obj: u32,
        tex: u32,
        settex: u32,
        tr_ex: u32,
        tr: u32,
        rx: Option<u32>,
        rot_x: u32,
        ry: u32,
        rot_y: u32,
        sc: u32,
        scale: u32,
        tr2: u32,
        draw: u32,
        pop: u32,
    }
    let spins = [
        Spin {
            push: 0x3f80,
            obj: 0x3f88,
            tex: 0x3f90,
            settex: 0x3f94,
            tr_ex: 0x3f9c,
            tr: 0x3fb4,
            rx: Some(0x3fc0),
            rot_x: 0x3fe0,
            ry: 0x3fec,
            rot_y: 0x3ffc,
            sc: 0x4004,
            scale: 0x4018,
            tr2: 0x403c,
            draw: 0x4044,
            pop: 0x4048,
        },
        Spin {
            push: 0x404c,
            obj: 0x4054,
            tex: 0x405c,
            settex: 0x4060,
            tr_ex: 0x4068,
            tr: 0x4080,
            rx: Some(0x408c),
            rot_x: 0x40a0,
            ry: 0x40ac,
            rot_y: 0x40bc,
            sc: 0x4004, // r29 still holds .data+0x15580
            scale: 0x40d0,
            tr2: 0x40f4,
            draw: 0x40fc,
            pop: 0x4100,
        },
        Spin {
            push: 0x4104,
            obj: 0x410c,
            tex: 0x4114,
            settex: 0x4118,
            tr_ex: 0x4120,
            tr: 0x4148,
            rx: None, // .bss+0xC, without the 0x3000 bias
            rot_x: 0x4164,
            ry: 0x4170,
            rot_y: 0x4180,
            sc: 0x4188,
            scale: 0x419c,
            tr2: 0x41c0,
            draw: 0x41c8,
            pop: 0x41cc,
        },
    ];
    for (n, sp) in spins.iter().enumerate() {
        let obj = k.at(sp.obj, 5)?;
        let model = k.deref(obj + 4)?;
        let m = [
            k.word(5, model + 8)?,
            k.word(5, model + 12)?,
            k.word(5, model + 16)?,
        ];
        let rot_x = match sp.rx {
            Some(pc) => k.dw(pc)?.wrapping_mul(i.clock).wrapping_add(0x3000),
            None => {
                k.at(0x4154, 6)?;
                i.spin.wrapping_mul(i.clock)
            }
        };
        let mut y = m[1];
        if n == 2 {
            y = (f32::from_bits(y) + f32::from_bits(k.dw(0x4138)?)).to_bits();
        }
        let sc = k.dw(sp.sc)?;
        c.extend([
            k.e(sp.push, 0x80116098, json!([]))?,
            k.e(sp.settex, 0x80116e18, json!([p5(sp.tex)?]))?,
            k.e(sp.tr_ex, 0x8011616c, json!([s(5, obj + 8)]))?,
            k.e(sp.tr, 0x80115bd8, json!([0, m[0], y, m[2]]))?,
            k.e(sp.rot_x, 0x80115c60, json!([0, rot_x as i32]))?,
            k.e(
                sp.rot_y,
                0x80115828,
                json!([0, k.dw(sp.ry)?.wrapping_mul(i.clock) as i32]),
            )?,
            k.e(sp.scale, 0x80115178, json!([0, sc, sc, sc]))?,
            k.e(
                sp.tr2,
                0x80115bd8,
                json!([0, neg(m[0]), neg(m[1]), neg(m[2])]),
            )?,
            k.e(sp.draw, 0x8011e158, json!([s(5, model)]))?,
            k.e(sp.pop, 0x801160e4, json!([]))?,
        ]);
    }
    c.extend([
        k.e(0x41d0, 0x80116098, json!([]))?,
        k.e(0x41d8, 0x80115dc8, json!([2]))?,
        k.local_call(0x41e4, json!([p5(0x41e0)?]))?,
    ]);
    let list = model_list(k, k.at(0x41e0, 5)?, i.screen, i.masks, &i.depths)?;
    c.extend(
        list["calls"]
            .as_array()
            .ok_or_else(invalid)?
            .iter()
            .cloned(),
    );
    c.extend([
        k.e(0x41f0, 0x80116e18, json!([p5(0x41ec)?]))?,
        k.e(0x41fc, 0x8011e17c, json!([p5(0x41f8)?]))?,
    ]);
    Ok(json!({ "calls": c, "state": {} }))
}

/// +0x421C on the descriptor at section-5 `desc` (count +0, entries +0x10,
/// texlist +0x1C, draw distance +0xC; each 32-byte entry: point +0, radius
/// +0xC, object +0x10, screen mask +0x18, flags +0x1C). An entry is drawn
/// when its mask is 0 or shares a bit with the screen's mask, and its depth
/// (negated njCalcPoint +8) is above minus its radius and, unless flag
/// 0x800000 is set, below the draw distance. `depths` are njCalcPoint's
/// results for the entries that reach it, in order.
pub fn model_list(
    k: &KnucklesRouge<'_>,
    desc: u32,
    screen: i32,
    masks: [u32; 2],
    depths: &[u32],
) -> Result<Value> {
    let mask = if screen == 0 { masks[0] } else { masks[1] };
    let count = (k.word(5, desc)? >> 16) as i16;
    if count < 0 {
        return Err(Error::invalid("negative stage 33 model list count")); // loops ~2^32 times
    }
    let list = k.deref(desc + 0x10)?;
    let far = f32::from_bits(k.word(5, desc + 0xc)?);
    let mut depths = depths.iter().copied();
    let mut c = vec![
        k.e(0x426c, 0x80116098, json!([]))?,
        k.e(0x4274, 0x80116e18, json!([s(5, k.deref(desc + 0x1c)?)]))?,
    ];
    for n in 0..count as u32 {
        let e = list + 32 * n;
        let m = k.word(5, e + 0x18)?;
        if m != 0 && m & mask == 0 {
            continue;
        }
        c.push(k.e(0x42a8, 0x80114f0c, json!([0, s(5, e), "out"]))?);
        let z = -f32::from_bits(
            depths
                .next()
                .ok_or_else(|| Error::invalid("missing depth"))?,
        );
        if k.word(5, e + 0x1c)? & 0x0080_0000 == 0
            && z.partial_cmp(&far) != Some(std::cmp::Ordering::Less)
        {
            continue;
        }
        if z.partial_cmp(&-f32::from_bits(k.word(5, e + 0xc)?)) != Some(std::cmp::Ordering::Greater)
        {
            continue;
        }
        let obj = k.deref(e + 0x10)?;
        c.extend([
            k.e(0x42f4, 0x8011616c, json!([s(5, obj + 8)]))?,
            k.e(0x4300, 0x80115f68, json!([s(5, obj + 0x14), 0]))?,
            k.e(0x4308, 0x8011e158, json!([s(5, k.deref(obj + 4)?)]))?,
            k.e(0x430c, 0x801160e4, json!([]))?,
            k.e(0x4310, 0x80116098, json!([]))?,
        ]);
    }
    c.push(k.e(0x4324, 0x801160e4, json!([]))?);
    Ok(json!({ "calls": c, "state": {} }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectorSpawnInput {
    /// CreateElementalTask and syCalloc(1, 28) succeeded.
    pub task: bool,
    pub work: bool,
    /// playerpwp[1] +3 (the rival's character byte).
    pub character: i8,
}

/// +0x3354 (setup calls it when player 1 exists): the rival director task
/// (exec +0xCE8, display +0x40) with a zeroed 28-byte work, player 1's
/// +0x48 speed set and handed to +0x2CB08, and the rival's motion table
/// loaded for characters 4 and 5 (with its destructor).
pub fn director_spawn(k: &KnucklesRouge<'_>, i: &DirectorSpawnInput) -> Result<Value> {
    let mut c = vec![k.e(
        0x3384,
        0x80071e0c,
        json!([0, 0, k.ptr(0x3378, 1)?, k.ptr(0x3380, 5)?]),
    )?];
    if !i.task {
        return Ok(json!({ "calls": c, "state": {} }));
    }
    c.push(k.e(0x339c, 0x801197fc, json!([1, 28]))?);
    if !i.work {
        c.push(k.e(0x33b0, 0x80071ae0, json!(["task"]))?);
        return Ok(json!({ "calls": c, "state": {} }));
    }
    let speed = k.f(0x33c0)?;
    c.push(k.local_call(0x33e8, json!(["task", "pwp1+0x48"]))?);
    let dest = match i.character {
        4 => {
            c.push(k.e(
                0x3424,
                0x80072820,
                json!([k.ptr(0x3418, 5)?, k.ptr(0x3420, 5)?]),
            )?);
            Some(k.ptr(0x342c, 1)?)
        }
        5 => {
            c.push(k.e(
                0x3450,
                0x80072820,
                json!([k.ptr(0x3444, 5)?, k.ptr(0x344c, 5)?]),
            )?);
            Some(k.ptr(0x3458, 1)?)
        }
        _ => None,
    };
    Ok(json!({ "calls": c, "state": {
        "pwp1_speed": speed, "work": "dyn", "mode": 0,
        "display": k.ptr(0x33f8, 1)?, "dest": dest } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SparkleSpawnInput {
    /// .bss+0 (the sparkle parent task) is set.
    pub parent: bool,
    /// CreateElementalTask's result when there is no parent.
    pub created: bool,
    /// CreateChildTask returned a child.
    pub child: bool,
    /// rand's five results.
    pub rand: [i32; 5],
    pub clock: u32,
}

/// +0x3664: make the sparkle parent (exec +0x3640, destructor +0x3644) at
/// .bss+0 if needed, add one sparkle child at a random position, size and
/// angle, and play sound 15 every 64 frames.
pub fn sparkle_spawn(k: &KnucklesRouge<'_>, i: &SparkleSpawnInput) -> Result<Value> {
    k.at(0x367c, 6)?;
    let mut c = Vec::new();
    let mut parent = json!("existing");
    if !i.parent {
        c.push(k.e(
            0x36a4,
            0x80071e0c,
            json!([0, 4, k.ptr(0x3698, 1)?, k.ptr(0x36a0, 5)?]),
        )?);
        if !i.created {
            return Ok(json!({ "calls": c, "state": { "parent": 0 } }));
        }
        parent = json!({ "task": "parent", "dest": k.ptr(0x36b8, 1)? });
    }
    c.push(k.e(0x36e4, 0x800709f8, json!([2, k.ptr(0x36d4, 1)?, "parent"]))?);
    let mut child = Value::Null;
    if i.child {
        let unit =
            |pc: u32, r| -> Result<f32> { Ok(f32::from_bits(k.f(pc)?) * k.int_f(0x3700, r)?) };
        let fk = |pc| -> Result<f32> { Ok(f32::from_bits(k.f(pc)?)) };
        for pc in [0x36f8, 0x3754, 0x37b4, 0x37f4, 0x3844] {
            c.push(k.e(pc, 0x8010dc88, json!([]))?);
        }
        for pc in [0x375c, 0x37bc, 0x37fc, 0x384c] {
            k.d(pc)?; // the same magic at each conversion
        }
        let x = fk(0x3734)? * unit(0x3724, i.rand[0])? - fk(0x3744)?;
        let y = fk(0x3784)? * unit(0x3724, i.rand[1])? - fk(0x3794)? - fk(0x37a4)?;
        let z = fk(0x3734)? * unit(0x3724, i.rand[2])? - fk(0x3744)?;
        let size = fk(0x3834)? + fk(0x3824)? * unit(0x3724, i.rand[3])?;
        let angle = fctiwz(fk(0x3874)? * unit(0x3724, i.rand[4])?);
        child = json!({ "pos": [x.to_bits(), y.to_bits(), z.to_bits()],
            "size": size.to_bits(), "angle": angle, "display": k.ptr(0x3894, 1)? });
    }
    if i.clock & 0x3f == 0 {
        c.push(k.e(0x38c4, 0x8002fb2c, json!([0, 4, 15, 0]))?);
    }
    Ok(json!({ "calls": c, "state": { "parent": parent, "child": child } }))
}

/// +0x35E0, a sparkle's exec: count up to 60 frames, then free itself;
/// until then it rises by the constant at .rodata+0x10.
pub fn sparkle_exec(k: &KnucklesRouge<'_>, timer: u16, y: u32) -> Result<Value> {
    let t = timer.wrapping_add(1);
    if t >= 60 {
        let c = vec![k.e(0x3610, 0x80071ae0, json!(["task"]))?];
        return Ok(json!({ "calls": c, "state": { "timer": 59, "y": y } }));
    }
    let y = (f32::from_bits(y) + f32::from_bits(k.f(0x3620)?)).to_bits();
    Ok(json!({ "calls": [], "state": { "timer": t, "y": y } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SparkleDisplayInput {
    /// This task is the parent's first child / the last one (the first
    /// child's +4 link points at it).
    pub first: bool,
    pub last: bool,
    pub timer: u16,
    pub pos: [u32; 3],
    pub size: u32,
    pub angle: i32,
    pub screen: i32,
}

/// +0x347C, a sparkle's display: the first child opens the batch (fog off,
/// additive blending, the sparkle texlist), each draws its billboard quad
/// (.data+0x3C08) facing screen's camera, the last closes the batch.
pub fn sparkle_display(k: &KnucklesRouge<'_>, i: &SparkleDisplayInput) -> Result<Value> {
    let mut c = Vec::new();
    if i.first {
        c.extend([
            k.e(0x34b0, 0x80116098, json!([]))?,
            k.e(0x34b4, 0x8011c2bc, json!([]))?,
            k.e(0x34b8, 0x8011c310, json!([]))?,
            k.e(0x34c4, 0x801179f8, json!([0, 8]))?,
            k.e(0x34d0, 0x801179f8, json!([1, 10]))?,
            k.e(0x34dc, 0x80116e18, json!([k.ptr(0x34d8, 5)?]))?,
        ]);
    } else {
        c.extend([
            k.e(0x34e4, 0x801160e4, json!([]))?,
            k.e(0x34e8, 0x80116098, json!([]))?,
        ]);
    }
    c.extend([
        k.e(0x3504, 0x80116e34, json!([i.timer / 3]))?,
        k.e(0x350c, 0x8011616c, json!([i.pos]))?,
        k.e(0x3520, 0x80115178, json!([0, i.size, i.size, i.size]))?,
        // lbl_80175378[screen] + 0xC: the screen's camera angles.
        k.e(0x354c, 0x80115f68, json!([{ "camera": i.screen }, 0]))?,
        k.e(0x3558, 0x801158a8, json!([0, i.angle]))?,
        k.e(0x356c, 0x8011e368, json!([k.ptr(0x3560, 5)?, 4, 1]))?,
    ]);
    if i.last {
        let z = k.f(0x3588)?;
        c.extend([
            k.e(0x359c, 0x800156fc, json!([z, z, z, z]))?,
            k.e(0x35a0, 0x8011c2b0, json!([]))?,
            k.e(0x35a4, 0x8011c310, json!([]))?,
            k.e(0x35a8, 0x801160e4, json!([]))?,
            k.e(0x35b4, 0x801179f8, json!([0, 8]))?,
            k.e(0x35c0, 0x801179f8, json!([1, 6]))?,
        ]);
    }
    Ok(json!({ "calls": c, "state": {} }))
}

/// +0x4340, the LIGHTEFF loader: unless out of range, install the exec
/// (+0x4590), display (+0x43B4) and destructor (+0x43B0, `blr`) and clear
/// work +1 (the texture number).
pub fn light_load(k: &KnucklesRouge<'_>, range_out: bool) -> Result<Value> {
    let c = vec![k.e(0x4360, 0x800655f4, json!(["task"]))?];
    if range_out {
        return Ok(json!({ "calls": c, "state": {} }));
    }
    Ok(json!({ "calls": c, "state": {
        "exec": k.ptr(0x4370, 1)?, "display": k.ptr(0x437c, 1)?,
        "dest": k.ptr(0x4388, 1)?, "texture": 0 } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LightExecInput {
    pub range_out: bool,
    pub clock: u32,
    /// rand's three results.
    pub rand: [i32; 3],
}

/// +0x4590, the LIGHTEFF exec: on odd frames pick a random texture (0..2)
/// and random horizontal/vertical flips (work +1, +0, +3).
pub fn light_exec(k: &KnucklesRouge<'_>, i: &LightExecInput) -> Result<Value> {
    let mut c = vec![k.e(0x45ac, 0x800655f4, json!(["task"]))?];
    if i.range_out || i.clock & 1 == 0 {
        return Ok(json!({ "calls": c, "state": {} }));
    }
    for pc in [0x45d0, 0x4648, 0x469c] {
        c.push(k.e(pc, 0x8010dc88, json!([]))?);
    }
    k.d(0x4650)?;
    k.d(0x46a4)?;
    let unit = |pc, r| -> Result<f32> { Ok(f32::from_bits(k.f(pc)?) * k.int_f(0x45d8, r)?) };
    let v = fctiwz(f32::from_bits(k.f(0x460c)?) * unit(0x45fc, i.rand[0])?);
    let texture = (v.wrapping_rem(3)) as i8;
    let half =
        |pc_k, pc_h, r| -> Result<u8> { Ok(u8::from(unit(pc_k, r)? < f32::from_bits(k.f(pc_h)?))) };
    let flip_u = half(0x45fc, 0x4678, i.rand[1])?;
    let flip_v = half(0x46c8, 0x46d8, i.rand[2])?;
    Ok(json!({ "calls": c, "state": {
        "texture": texture, "flip_u": flip_u, "flip_v": flip_v } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LightDisplayInput {
    /// Work +0, +1, +3.
    pub flip_u: i8,
    pub texture: i8,
    pub flip_v: u8,
    /// Work +0x14 (position), +8 (angles), +0x20/+0x24 (half width/height).
    pub pos: [u32; 3],
    pub rot: [i32; 3],
    pub half: [u32; 2],
}

/// +0x43B4, the LIGHTEFF display: one additive, unfogged textured quad.
pub fn light_display(k: &KnucklesRouge<'_>, i: &LightDisplayInput) -> Result<Value> {
    let one = k.f(0x43dc)?;
    let zero = k.f(0x43e8)?;
    for (pc, v) in [
        (0x43f8, one),
        (0x4404, zero),
        (0x441c, one),
        (0x4428, zero),
        (0x4438, one),
        (0x4444, zero),
        (0x4468, zero),
    ] {
        if k.f(pc)? != v {
            return Err(invalid());
        }
    }
    let (u0, u1) = if i.flip_u != 0 {
        (one, zero)
    } else {
        (zero, one)
    };
    let (v0, v1) = if i.flip_v != 0 {
        (one, zero)
    } else {
        (zero, one)
    };
    let [w, h] = i.half;
    let vert = |x, y, u, v| json!([x, y, zero, u, v, -1]);
    let quad = json!([
        vert(neg(w), neg(h), u0, v0),
        vert(neg(w), h, u0, v1),
        vert(w, neg(h), u1, v0),
        vert(w, h, u1, v1),
    ]);
    let c = vec![
        k.e(0x4500, 0x80116e18, json!([k.ptr(0x44fc, 5)?]))?,
        k.e(0x450c, 0x80116e34, json!([i.texture]))?,
        k.e(0x4518, 0x801179f8, json!([0, 8]))?,
        k.e(0x4524, 0x801179f8, json!([1, 10]))?,
        k.e(0x4528, 0x8011c2bc, json!([]))?,
        k.e(0x452c, 0x8011c310, json!([]))?,
        k.e(0x4530, 0x80116098, json!([]))?,
        k.e(0x4538, 0x8011616c, json!([i.pos]))?,
        k.e(0x4544, 0x80115f68, json!([i.rot, 0]))?,
        k.e(0x4554, 0x8011e368, json!([quad, 4, 1]))?,
        k.e(0x4558, 0x801160e4, json!([]))?,
        k.e(0x455c, 0x8011c2b0, json!([]))?,
        k.e(0x4560, 0x8011c310, json!([]))?,
        k.e(0x456c, 0x801179f8, json!([0, 8]))?,
        k.e(0x4578, 0x801179f8, json!([1, 6]))?,
    ];
    Ok(json!({ "calls": c, "state": {} }))
}

/// +0x1AB0C, the ITEMBOXBAL GEN loader: work +6 += work +0x10 (the SET
/// phase), exec +0x1A9D8, +0x14 the `blr` at +0x1AB08.
pub fn balloon_load(k: &KnucklesRouge<'_>, timer: u16, phase: u32) -> Result<Value> {
    Ok(json!({ "calls": [], "state": {
        "timer": timer.wrapping_add(phase as u16),
        "exec": k.ptr(0x1ab24, 1)?, "dest": k.ptr(0x1ab30, 1)? } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BalloonExecInput {
    pub range_out: bool,
    /// Task +0xC (a child balloon is alive).
    pub child_alive: bool,
    /// Work +6 and +8 (the respawn delay).
    pub timer: u16,
    pub delay: i32,
    /// CreateChildTask returned the balloon.
    pub child: bool,
    pub rand: i32,
}

/// +0x1A9D8, the ITEMBOXBAL GEN exec: while no balloon is alive, count
/// frames; past the delay spawn an ITEMBOXBALLOON child (+0x19D08) with the
/// parent's item (+0xC) and a random content from .data+0x21F50.
pub fn balloon_exec(k: &KnucklesRouge<'_>, i: &BalloonExecInput) -> Result<Value> {
    let mut c = vec![k.e(0x1a9f8, 0x800655f4, json!(["task"]))?];
    if i.range_out {
        return Ok(json!({ "calls": c, "state": { "timer": i.timer } }));
    }
    if i.child_alive {
        return Ok(json!({ "calls": c, "state": { "timer": 0 } }));
    }
    if i32::from(i.timer) <= i.delay {
        return Ok(json!({ "calls": c, "state": { "timer": i.timer.wrapping_add(1) } }));
    }
    c.push(k.e(0x1aa38, 0x800709f8, json!([2, k.ptr(0x1aa30, 1)?, "task"]))?);
    let mut child = Value::Null;
    if i.child {
        c.push(k.e(0x1aa50, 0x8010dc88, json!([]))?);
        let unit = f32::from_bits(k.f(0x1aa7c)?) * k.int_f(0x1aa58, i.rand)?;
        let n = fctiwz(f32::from_bits(k.f(0x1aa8c)?) * unit);
        let table = k.at(0x1aaa8, 5)?;
        let at = table.checked_add_signed(n).ok_or_else(invalid)?;
        let b = k.byte(5, at)? as i8;
        k.d(0x1aabc)?;
        child = json!({ "item": "work+0xc", "content": (b as f32).to_bits() });
    }
    Ok(json!({ "calls": c, "state": { "timer": 0, "child": child } }))
}

/// Run one routine by name on a JSON input.
pub fn replay(rel_bytes: &[u8], routine: &str, input: &Value) -> Result<Value> {
    let k = KnucklesRouge::read(rel_bytes)?;
    fn parse<T: serde::de::DeserializeOwned>(v: &Value) -> Result<T> {
        T::deserialize(v).map_err(|e| Error::invalid(format!("stage 33 request: {e}")))
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Timed {
        timer: u16,
        y: u32,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct RangeOut {
        range_out: bool,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Phase {
        timer: u16,
        phase: u32,
    }
    match routine {
        "setup" => setup(&k, &parse(input)?),
        "teardown" => teardown(&k, &parse(input)?),
        "exec" | "task_dest" => Ok(json!({ "calls": [], "state": {} })),
        "task_exec" => task_exec(&k, &parse(input)?),
        "display" => display(&k, &parse(input)?),
        "director_spawn" => director_spawn(&k, &parse(input)?),
        "sparkle_spawn" => sparkle_spawn(&k, &parse(input)?),
        "sparkle_exec" => {
            let t: Timed = parse(input)?;
            sparkle_exec(&k, t.timer, t.y)
        }
        "sparkle_display" => sparkle_display(&k, &parse(input)?),
        "light_load" => light_load(&k, parse::<RangeOut>(input)?.range_out),
        "light_exec" => light_exec(&k, &parse(input)?),
        "light_display" => light_display(&k, &parse(input)?),
        "balloon_load" => {
            let p: Phase = parse(input)?;
            balloon_load(&k, p.timer, p.phase)
        }
        "balloon_exec" => balloon_exec(&k, &parse(input)?),
        _ => Err(Error::invalid(format!(
            "unknown stage 33 routine {routine}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// The GC REL (SA2_GC_REL_DIR) and, with SA2_GAME_PATH, the Steam one.
    fn rels() -> Vec<Vec<u8>> {
        let mut v = Vec::new();
        if let Some(d) = std::env::var_os("SA2_GC_REL_DIR") {
            v.push(std::fs::read(PathBuf::from(d).join("stg33D.rel")).unwrap());
        }
        if let Some(g) = std::env::var_os("SA2_GAME_PATH") {
            v.push(std::fs::read(PathBuf::from(g).join("resource/gd_PC/stg33D.rel")).unwrap());
        }
        v
    }

    #[test]
    fn rejects_other_modules() {
        assert!(KnucklesRouge::read(&[0; 16]).is_err());
        assert!(KnucklesRouge::read(&vec![0; REL_LEN]).is_err());
    }

    #[test]
    fn gc_and_steam_sections_match() {
        let r = rels();
        if r.len() < 2 {
            return;
        }
        let (a, b) = (
            KnucklesRouge::read(&r[0]).unwrap(),
            KnucklesRouge::read(&r[1]).unwrap(),
        );
        for sec in 1..=5 {
            assert_eq!(a.rel.data(sec).unwrap(), b.rel.data(sec).unwrap());
        }
    }

    #[test]
    fn routines_read_the_module() {
        for rel in rels() {
            let k = KnucklesRouge::read(&rel).unwrap();
            let r = |name, v: Value| replay(&rel, name, &v).unwrap();
            let s = r(
                "setup",
                json!({ "modes": [3, 4], "player1": true, "handle": 7,
                    "task": true, "slots": [0, 5, 0] }),
            );
            let calls = s["calls"].as_array().unwrap();
            assert_eq!(calls.len(), 33);
            assert_eq!(calls[2]["engine"], "0x800a1f00");
            assert_eq!(s["state"]["task"], json!(["s1+0x3e84", "s1+0x3e80"]));
            assert_eq!(s["state"]["slots"], json!(["s1+0x20c5c", 5, "s1+0xa8c8"]));
            assert_eq!(s["state"]["trail"][1], "s1+0x741c");
            // StgPlayMusic("BOSS_03.ADX")
            assert_eq!(calls[32]["args"][0], "s5+0x153c8");
            let t = r(
                "teardown",
                json!({ "slots": ["ours", 9, "ours"], "handle": 7 }),
            );
            assert_eq!(t["state"]["slots"], json!([0, 9, 0]));
            assert_eq!(t["calls"].as_array().unwrap().len(), 10);

            // Both table rows recolour (row kind 1) with the lerped colour.
            let e = r(
                "task_exec",
                json!({ "clock": 3, "sin": 0, "colour": 0xff10_1010_u32,
                    "timer": 12, "frame": 0xc005 }),
            );
            assert_eq!(e["calls"][0]["args"][0], 3 * 0x200);
            assert_eq!(e["calls"][1]["args"][2], 0.5f32.to_bits());
            assert_eq!(e["calls"][2]["module"], "s1+0x6c10");
            assert_eq!(e["state"]["timer"], 0);
            assert_eq!(e["state"]["frame"], 0xc000);

            let d = r(
                "display",
                json!({ "mode": 0, "screen": 0, "clock": 1, "sky_y": 0, "spin": 2,
                    "masks": [1, 2], "depths": vec![0u32; 8] }),
            );
            // Eight model-list entries checked against depth 0.
            assert!(d["calls"].as_array().unwrap().len() > 40);
            let skip = r(
                "display",
                json!({ "mode": 3, "screen": 0, "clock": 1, "sky_y": 0, "spin": 2,
                    "masks": [1, 2], "depths": [] }),
            );
            assert_eq!(skip["calls"], json!([]));

            let sp = r(
                "director_spawn",
                json!({ "task": true, "work": true, "character": 5 }),
            );
            assert_eq!(sp["state"]["dest"], "s1+0x332c");
            assert_eq!(sp["state"]["pwp1_speed"], 3.0f32.to_bits());

            let sk = r(
                "sparkle_spawn",
                json!({ "parent": false, "created": true, "child": true,
                    "rand": [0, 0, 0, 0, 0], "clock": 64 }),
            );
            // rand() = 0: x = -130, size 0.4, angle 0.
            assert_eq!(sk["state"]["child"]["pos"][0], (-130f32).to_bits());
            assert_eq!(sk["state"]["child"]["size"], 0.4f32.to_bits());
            assert_eq!(sk["calls"].as_array().unwrap().len(), 8);
            let ex = r("sparkle_exec", json!({ "timer": 59, "y": 0 }));
            assert_eq!(ex["state"]["timer"], 59);
            let ex = r("sparkle_exec", json!({ "timer": 1, "y": 0 }));
            assert_eq!(ex["state"]["y"], 3f32.to_bits());

            let l = r(
                "light_exec",
                json!({ "range_out": false, "clock": 1, "rand": [32767, 0, 32767] }),
            );
            // 100 * 32767/32768 = 99.99... -> 99 % 3 = 0; 0 < 0.5; 0.99 >= 0.5.
            assert_eq!(
                l["state"],
                json!({ "texture": 0, "flip_u": 1, "flip_v": 0 })
            );
            let ld = r(
                "light_display",
                json!({ "flip_u": 1, "texture": 2, "flip_v": 0, "pos": [0, 0, 0],
                    "rot": [0, 0, 0], "half": [0x4000_0000, 0x3f80_0000] }),
            );
            assert_eq!(ld["calls"][9]["args"][0][0][3], 1f32.to_bits());

            let b = r(
                "balloon_exec",
                json!({ "range_out": false, "child_alive": false, "timer": 5,
                    "delay": 4, "child": true, "rand": 32767 }),
            );
            // 12 * 32767/32768 -> index 11 -> .data+0x21F5B = 1.
            assert_eq!(b["state"]["child"]["content"], 1f32.to_bits());
            let _ = &k;
        }
    }
}
