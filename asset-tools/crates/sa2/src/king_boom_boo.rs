//! King Boom Boo (story level 63, boss_bigbogyD.rel): the stage program
//! "BIGBOGY" that `_prolog` (+0xEBF0) hands to LoadSubprogram, the stage
//! task that keeps the player inside the arena, and the boss spawn. All
//! offsets are section-1 (code) offsets of the GameCube disc REL; engine calls
//! are reported at their GSNE8P main.dol addresses.
//!
//! | Offset | Size | Routine |
//! | --- | --- | --- |
//! | +0xEC1C | 0x2A0 | stage setup (the program's first entry) |
//! | +0xEEBC | 0xE8 | stage teardown (second entry) |
//! | +0xEFA4 | 0x88 | stage program exec (third entry): spawns the boss once |
//! | +0xF02C | 0xF8 | arena fence, run by the shared exec +0xF124 the program installs |
//! | +0x584 | 0x3FC | boss spawn |
//!
//! Constants and data pointers are read from the module through the
//! relocation of the instruction that loads them (the offsets are recorded at
//! each use). Every engine call is checked against the module's own import
//! relocation at its call site; for the GameCube module the relocation must
//! name the reported GSNE8P address.
//!
//! The Steam `resource/gd_PC/boss_bigbogyD.rel` (module 84) is a different
//! build: the code below +0x197A0 is the same instruction for instruction, but
//! a shared routine near +0x197A0 is 0x2F4 bytes shorter, so every later code
//! offset is 0x2F4 lower. This port reads either file and reports module code
//! offsets in the GameCube layout (`code_offset`). Constants are always read
//! from the file passed in, so a Steam file supplies Steam values; for every
//! routine here the two builds' instructions, constants and data offsets are
//! identical (the test compares both outputs).
use crate::{
    city_rel::{CityRel, Reference},
    manifest, Error, Result,
};
use serde::Deserialize;
use serde_json::{json, Value};

/// GameCube disc `boss_bigbogyD.rel` (module 85).
pub const GC_REL_SHA256: &str = "73ba80862a366ad895f49681f122899b5e0ce17e336487160f13d9673cdcc130";
pub const GC_REL_LEN: usize = 1_322_630;
/// Installed Steam `resource/gd_PC/boss_bigbogyD.rel` (module 84).
pub const STEAM_REL_SHA256: &str =
    "ab634339709db67be44c00db889a44347dfc20a3968e4663311264e69b76362f";
pub const STEAM_REL_LEN: usize = 1_321_518;
const GC_MODULE: u32 = 85;
const STEAM_MODULE: u32 = 84;
/// Steam code at or past this offset sits `STEAM_SHIFT` below the GameCube
/// copy (GameCube +0x197A0..+0x19A84, part of a routine shared by 70 RELs, has
/// no Steam counterpart); earlier code is at the same offset in both.
const STEAM_SHIFTED_FROM: u32 = 0x19790;
const STEAM_SHIFT: u32 = 0x2f4;

fn invalid() -> Error {
    Error::invalid("unexpected stage 63 code")
}
fn bad(pc: u32) -> Error {
    Error::invalid(format!("unexpected stage 63 code at +0x{pc:x}"))
}
fn s(section: u8, offset: u32) -> Value {
    json!(format!("s{section}+0x{offset:x}"))
}
fn engine(address: u32, args: Value) -> Value {
    json!({ "engine": format!("0x{address:08x}"), "args": args })
}
fn module_call(offset: u32, args: Value) -> Value {
    json!({ "module": format!("s1+0x{offset:x}"), "args": args })
}

pub struct KingBoomBoo<'a> {
    rel: CityRel<'a>,
}

impl<'a> KingBoomBoo<'a> {
    pub fn read(bytes: &'a [u8]) -> Result<Self> {
        let module = match (bytes.len(), manifest::sha256(bytes).as_str()) {
            (GC_REL_LEN, GC_REL_SHA256) => GC_MODULE,
            (STEAM_REL_LEN, STEAM_REL_SHA256) => STEAM_MODULE,
            _ => return Err(Error::invalid("unsupported stage 63 REL revision")),
        };
        Ok(Self {
            rel: CityRel::read_identified(bytes, module)?,
        })
    }
    fn gc(&self) -> bool {
        self.rel.module_id() == GC_MODULE
    }
    /// A code offset of the loaded file in the GameCube layout.
    pub fn code_offset(&self, offset: u32) -> Result<u32> {
        if self.gc() || offset < STEAM_SHIFTED_FROM {
            Ok(offset)
        } else {
            offset.checked_add(STEAM_SHIFT).ok_or_else(invalid)
        }
    }
    /// The section offset the `addi` at `pc` (the LO half of a `lis`/`addi`
    /// pair, relocated at `pc + 2`) points at; code offsets in GC layout.
    fn at(&self, pc: u32, section: u8) -> Result<u32> {
        match self.rel.reference(1, pc + 2) {
            Some(Reference {
                module,
                kind: 4,
                section: s,
                addend,
            }) if s == section && module == self.rel.module_id() => {
                if section == 1 {
                    self.code_offset(addend)
                } else {
                    Ok(addend)
                }
            }
            _ => Err(bad(pc)),
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
    /// The NUL-terminated name the `addi` at `pc` points at, with its offset.
    fn text(&self, pc: u32) -> Result<Value> {
        let at = self.at(pc, 5)?;
        let d = self.rel.data(5)?.get(at as usize..).ok_or_else(invalid)?;
        let end = d
            .iter()
            .take(64)
            .position(|&b| b == 0)
            .ok_or_else(invalid)?;
        let text = std::str::from_utf8(&d[..end]).map_err(|_| bad(pc))?;
        Ok(json!({ "at": s(5, at), "text": text }))
    }
    /// The single-precision constant `add` bytes past the `.rodata` address
    /// the `addi` at `pc` loads.
    fn f(&self, pc: u32, add: u32) -> Result<f32> {
        Ok(f32::from_bits(self.word(4, self.at(pc, 4)? + add)?))
    }
    /// The value of a `lis rX,hi` at `pc` followed by `addi rX,rX,lo`.
    fn imm32(&self, pc: u32) -> Result<u32> {
        let (hi, lo) = (self.word(1, pc)?, self.word(1, pc + 4)?);
        if hi >> 26 != 15 || hi >> 16 & 31 != 0 || lo >> 26 != 14 {
            return Err(bad(pc));
        }
        Ok((hi << 16).wrapping_add(lo as u16 as i16 as i32 as u32))
    }
    /// The target of the module-local `bl` at `pc` (resolved at link time,
    /// without a relocation), in GC layout.
    fn local(&self, pc: u32) -> Result<u32> {
        let ins = self.word(1, pc)?;
        if ins & 0xfc00_0003 != 0x4800_0001 || self.rel.reference(1, pc).is_some() {
            return Err(Error::invalid(format!("no local call at +0x{pc:x}")));
        }
        let disp = ((ins & 0x03ff_fffc) as i32) << 6 >> 6;
        self.code_offset(pc.checked_add_signed(disp).ok_or_else(invalid)?)
    }
    fn local_call(&self, pc: u32, args: Value) -> Result<Value> {
        Ok(module_call(self.local(pc)?, args))
    }
    /// The main-program import the `bl` at `pc` calls: `address` on the
    /// GameCube module (checked against its relocation).
    fn eng(&self, pc: u32, address: u32, args: Value) -> Result<Value> {
        match self.rel.reference(1, pc) {
            Some(Reference {
                module: 0,
                kind: 10,
                addend,
                ..
            }) if !self.gc() || addend == address => Ok(engine(address, args)),
            _ => Err(bad(pc)),
        }
    }
    /// A main-program address loaded by the `addi` at `pc`, checked like `eng`.
    fn dol(&self, pc: u32, address: u32) -> Result<u32> {
        match self.rel.reference(1, pc + 2) {
            Some(Reference {
                module: 0,
                kind: 4,
                addend,
                ..
            }) if !self.gc() || addend == address => Ok(address),
            _ => Err(bad(pc)),
        }
    }
}

/// The hook slots at GSNE8P 0x801A2A18 the stage fills when empty.
const HOOKS: u32 = 0x801a_2a18;
/// The trail plugin table at GSNE8P 0x801E77D8.
const TRAIL: u32 = 0x801e_77d8;
/// GSNE8P `playertwp`.
const PLAYERTWP: u32 = 0x801e_7768;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupInput {
    /// fn_800651EC's return, kept at .bss+0x90.
    pub handle: u32,
    /// The words at 0x801A2A18 +0 and +4 before the setup.
    pub slots: [u32; 2],
}

/// +0xEC1C. Returns the ordered calls and the final value of every location
/// it writes.
pub fn setup(k: &KingBoomBoo<'_>, i: &SetupInput) -> Result<Value> {
    let p5 = |pc| k.ptr(pc, 5);
    let f = |pc| k.f(pc, 0).map(f32::to_bits);
    let colors = |pc: u32| -> Result<Value> {
        Ok(json!([k.imm32(pc)?, k.imm32(pc + 8)?, k.imm32(pc + 16)?]))
    };
    let (zero, one) = (f(0xec98)?, f(0xeca4)?);
    let mut c = vec![
        k.eng(0xec38, 0x8002_493c, json!([p5(0xec34)?]))?,
        k.eng(0xec54, 0x8011_7b8c, colors(0xec3c)?)?,
        k.eng(
            0xec6c,
            0x8006_51ec,
            json!([k.text(0xec5c)?, k.text(0xec64)?, 512]),
        )?,
        k.eng(0xec88, 0x8006_638c, json!([p5(0xec80)?, i.handle]))?,
        k.eng(0xec8c, 0x8001_a8a8, json!([]))?,
        k.eng(0xec90, 0x8003_3710, json!([]))?,
        k.eng(0xecb0, 0x800c_3cd8, json!([zero, one, zero]))?,
        k.eng(0xecc4, 0x8011_e154, json!([one, f(0xecbc)?]))?,
        k.eng(0xecd4, 0x8011_e150, json!([one, one, one]))?,
    ];
    k.at(0xec74, 6)?; // .bss+0x90 keeps the handle
                      // Light blocks: base (+0xECE0), base+0x20 twice, base+0x40 twice, base+0x60.
    let lights = k.at(0xece0, 5)?;
    for (pc, n, add) in [
        (0xece8, 0, 0),
        (0xecf8, 1, 0x20),
        (0xed04, 2, 0x20),
        (0xed14, 3, 0x40),
        (0xed20, 9, 0x40),
        (0xed2c, 4, 0x60),
    ] {
        c.push(k.eng(pc, 0x8002_506c, json!([n, s(5, lights + add)]))?);
    }
    c.extend([
        k.eng(0xed38, 0x8002_5028, json!([0, 0]))?,
        k.eng(0xed44, 0x8002_5028, json!([0, 1]))?,
        k.eng(0xed4c, 0x8002_5048, json!([2]))?,
        k.eng(0xed54, 0x8002_503c, json!([2]))?,
    ]);
    k.dol(0xed5c, HOOKS)?;
    k.dol(0xed7c, HOOKS)?;
    let mut slots = vec![json!(i.slots[0]), json!(i.slots[1])];
    for (n, pc) in [(0, 0xed70), (1, 0xed94)] {
        if i.slots[n] == 0 {
            slots[n] = k.ptr(pc, 1)?;
        }
    }
    c.extend([
        k.eng(0xedac, 0x8007_2270, json!([p5(0xeda0)?, p5(0xeda8)?]))?,
        k.eng(0xedc0, 0x8007_2820, json!([k.text(0xedb4)?, p5(0xedbc)?]))?,
        k.eng(0xedd4, 0x8007_2820, json!([k.text(0xedc8)?, p5(0xedd0)?]))?,
        k.eng(0xede8, 0x8007_2820, json!([k.text(0xeddc)?, p5(0xede4)?]))?,
        k.eng(0xedf0, 0x8001_1a04, json!([0]))?,
        k.eng(0xedfc, 0x8001_1050, json!([k.text(0xedf8)?]))?,
        k.local_call(0xee08, json!([k.text(0xee04)?]))?,
        k.eng(
            0xee1c,
            0x8006_d500,
            json!([k.text(0xee10)?, k.ptr(0xee18, 4)?]),
        )?,
        k.eng(0xee38, 0x8011_7b8c, colors(0xee20)?)?,
        k.eng(0xee3c, 0x8001_8e5c, json!([]))?,
    ]);
    // Trail plugin table: GSNE8P fn_800E014C, three module routines, ten zeros.
    k.dol(0xee4c, TRAIL)?;
    let mut trail = vec![json!(k.dol(0xee44, 0x800e_014c)?)];
    for pc in [0xee58, 0xee64, 0xee70] {
        trail.push(k.ptr(pc, 1)?);
    }
    trail.resize(14, json!(0));
    Ok(json!({ "calls": c, "state": {
        "handle": i.handle, "slots": slots, "trail": trail } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeardownInput {
    /// Each 0x801A2A18 slot: `"ours"` (the routine setup installed) or a word.
    pub slots: [Value; 2],
    /// .bss+0x90.
    pub handle: u32,
}

/// +0xEEBC: undo the setup's hooks and release the stage's resources.
pub fn teardown(k: &KingBoomBoo<'_>, i: &TeardownInput) -> Result<Value> {
    let p5 = |pc| k.ptr(pc, 5);
    k.dol(0xeed0, HOOKS)?;
    k.dol(0xeef4, HOOKS)?;
    // A slot is cleared only when it still holds the routine setup put there.
    k.at(0xeedc, 1)?;
    k.at(0xef04, 1)?;
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
    let mut c = vec![
        k.eng(0xef18, 0x8003_36dc, json!([]))?,
        k.eng(0xef2c, 0x8007_2118, json!([p5(0xef20)?, p5(0xef28)?]))?,
        k.eng(0xef38, 0x8011_66f0, json!([p5(0xef34)?]))?,
        k.eng(0xef44, 0x8011_66f0, json!([p5(0xef40)?]))?,
        k.eng(0xef50, 0x8011_66f0, json!([p5(0xef4c)?]))?,
        k.eng(0xef54, 0x8006_6364, json!([]))?,
    ];
    k.at(0xef5c, 6)?;
    if i.handle != 0 {
        c.push(k.eng(0xef6c, 0x8001_a2b0, json!([i.handle]))?);
    }
    c.extend([
        k.local_call(0xef78, json!([]))?,
        k.eng(0xef88, 0x8011_7b8c, json!([0, 0, 0]))?,
        k.eng(0xef8c, 0x8002_45b8, json!([]))?,
    ]);
    Ok(json!({ "calls": c, "state": { "handle": 0, "slots": slots } }))
}

/// The arena centre at .rodata+0xA50 (x, y, z), which the program passes to
/// the boss spawn and the fence uses.
pub fn arena_centre(k: &KingBoomBoo<'_>) -> Result<[f32; 3]> {
    Ok([k.f(0xefdc, 0)?, k.f(0xefdc, 4)?, k.f(0xefdc, 8)?])
}

/// +0xEFA4, the stage program's exec: spawns the boss at (centre.x - .rodata
/// +0xA78, centre.y, .rodata+0xA74) with angle 0 around the arena centre, then
/// replaces itself with the shared exec +0xF124 (which runs the fence).
pub fn exec(k: &KingBoomBoo<'_>) -> Result<Value> {
    let c = arena_centre(k)?;
    let z = k.f(0xefbc, 8)?;
    let pos = [c[0] - k.f(0xefe8, 0)?, c[1], z];
    Ok(json!({
        "calls": [k.local_call(0xf008, json!([pos, 0, k.ptr(0xefdc, 4)?]))?],
        "state": { "task+0x10": k.ptr(0xf010, 1)? }
    }))
}

/// Length and unit vector as the engine's njScalor / njUnitVector compute
/// them (single precision).
fn length(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

/// +0xF02C, the arena fence: player 0 farther than .rodata+0xA7C from the
/// centre in the XZ plane is put back on that circle (Y is untouched).
pub fn fence(k: &KingBoomBoo<'_>, player: [f32; 3]) -> Result<Value> {
    k.dol(0xf04c, PLAYERTWP)?;
    let (cx, cz) = (k.f(0xf074, 0)?, k.f(0xf074, 8)?);
    let v = [player[0] - cx, k.f(0xf088, 0)?, player[2] - cz];
    let radius = k.f(0xf0b4, 0)?;
    let mut calls = vec![k.eng(0xf0ac, 0x8011_51f4, json!([v]))?];
    let len = length(v);
    let mut pos = player;
    if len > radius {
        calls.push(k.eng(0xf0c8, 0x8011_5d24, json!([v]))?);
        pos[0] = cx + radius * (v[0] / len);
        pos[2] = cz + radius * (v[2] / len);
    }
    Ok(json!({ "calls": calls, "state": { "player": pos } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpawnInput {
    /// CreateElementalTask returned a task.
    pub task: bool,
    /// syCalloc(1, 0xE4) returned work.
    pub work: bool,
    /// The position the caller passes (r3).
    pub position: [f32; 3],
    /// The angle the caller passes (r4).
    pub angle: i32,
    /// The arena centre the caller passes (r5).
    pub centre: [f32; 3],
    /// The vector the module helper +0xBA68 writes for fn_800399BC.
    pub focus: [f32; 3],
}

/// +0x584, the boss spawn. Values returned by module helpers that are not
/// ported here are named `ret:<offset>`; `motion` is the record +0x562BC
/// returns, `work` the 0xE4-byte calloc.
pub fn spawn(k: &KingBoomBoo<'_>, i: &SpawnInput) -> Result<Value> {
    let ret = |pc: u32| -> Result<Value> { Ok(json!(format!("ret:s1+0x{:x}", k.local(pc)?))) };
    let mut c = vec![k.eng(
        0x5bc,
        0x8007_1e0c,
        json!([2, 2, k.ptr(0x5b0, 1)?, k.ptr(0x5b8, 5)?]),
    )?];
    if !i.task {
        return Ok(json!({ "calls": c, "state": {} }));
    }
    c.push(k.local_call(0x5d4, json!(["task"]))?);
    c.push(k.eng(0x5e4, 0x8011_97fc, json!([1, 0xe4]))?);
    if !i.work {
        c.push(k.eng(0x5f8, 0x8007_1ae0, json!(["task"]))?);
        return Ok(json!({ "calls": c, "state": {} }));
    }
    c.push(k.eng(0x610, 0x8002_d83c, json!(["motion+0xc", k.ptr(0x60c, 4)?]))?);
    let one = k.f(0x664, 0)?;
    // +0x698: sqrtf of the XZ distance, with the original's X used twice
    // (position.x - centre.z, not position.z).
    let dx = i.position[0] - i.centre[0];
    let dz = i.position[0] - i.centre[2];
    let reach = (dx * dx + dz * dz).sqrt();
    let gravity = k.f(0x6a8, 0)?;
    c.push(k.eng(0x6b0, 0x8002_f3ec, json!(["task", gravity]))?);
    let kind = k.byte(5, k.at(0x6cc, 5)?)? as i8;
    c.push(k.local_call(0x704, json!(["task"]))?);
    c.push(k.local_call(0x710, json!([ret(0x704)?]))?);
    c.push(k.local_call(0x71c, json!(["task", ret(0x704)?]))?);
    c.push(k.local_call(0x734, json!(["task", "twp+0x14", -20480, "stack"]))?);
    c.push(k.eng(
        0x748,
        0x8003_99bc,
        json!([0, i.focus[0], i.focus[1], i.focus[2]]),
    )?);
    c.push(k.eng(0x760, 0x8000_f8b8, json!(["task", k.ptr(0x754, 4)?, 11, 3]))?);
    c.push(k.eng(0x788, 0x8000_68bc, json!(["task", k.f(0x780, 0)?]))?);
    c.push(k.local_call(0x78c, json!(["ret:0x800068bc"]))?);
    c.push(k.local_call(0x79c, json!(["task", k.ptr(0x798, 5)?]))?);
    c.push(k.local_call(0x7a8, json!(["task", 0]))?);
    // Two descriptor blocks on the stack for +0x8B74.
    for (pc, tag, at, bss) in [(0x7e8, 0, 0x7b8, 0x7d0), (0x824, 1, 0x7fc, 0x810)] {
        c.push(k.local_call(
            pc,
            json!(["task", { "byte": tag, "data": k.ptr(at, 5)?,
                "twp": "twp", "twp+1": "twp+1", "twp+0xc": "twp+0xc", "bss": k.ptr(bss, 6)? }]),
        )?);
    }
    let table = k.ptr(0x834, 5)?;
    c.push(k.local_call(0x844, json!(["task", table, k.ptr(0x840, 5)?]))?);
    c.push(k.local_call(0x85c, json!(["task", table, k.ptr(0x858, 5)?]))?);
    let fade = k.f(0x868, 0)?;
    c.push(k.eng(0x87c, 0x8002_5054, json!([2, fade]))?);
    c.push(k.local_call(0x888, json!(["task", 0]))?);
    // A stack copy of the vector at .rodata+0x4D4.
    let v = [k.f(0x890, 0)?, k.f(0x890, 4)?, k.f(0x890, 8)?];
    c.push(k.local_call(0x8b8, json!([v, 8, "work+0x30"]))?);
    c.push(k.eng(0x8c4, 0x8002_ce98, json!(["motion+0xc", 0]))?);
    c.push(k.eng(0x8cc, 0x8002_2e24, json!([0]))?);
    c.push(k.eng(0x8fc, 0x8010_3128, json!([256, 256, 2, 0, 0]))?);
    c.push(k.eng(0x900, 0x8011_97d0, json!(["ret:0x80103128"]))?);
    c.push(k.eng(
        0x928,
        0x8010_334c,
        json!(["work+0x90", "ret:0x801197d0", 256, 256, 2, 0, 0, 0]),
    )?);
    let lod = k.f(0x93c, 0)?;
    c.push(k.eng(
        0x958,
        0x8010_3608,
        json!(["work+0x90", 1, 1, lod, lod, lod, 0, 0, 0]),
    )?);
    let scale = [one, one, one];
    Ok(json!({ "calls": c, "state": {
        "bss": [s(6, k.at(0x61c, 6)?), "task"],
        "twp": { "+0x0": 0, "+0xc": i.angle, "+0x14": i.position, "+0x20": scale },
        "mwp": { "+0x6c": i.centre, "+0xc4": reach, "+0xd8": i.centre[1] },
        "motion": { "+0x10": "work", "+0x28": kind as f32 },
        "cwp+0x4": "clear 0x40",
        "work": {
            "+0x1": 0, "+0x2": 2,
            "+0x8": ret(0x704)?, "+0xc": ret(0x71c)?, "+0x10": ret(0x7a8)?,
            "+0x18": ret(0x844)?, "+0x1c": ret(0x85c)?,
            "+0x24": ret(0x824)?, "+0x28": ret(0x7e8)?,
            "+0x2c": fade, "+0xb0": "ret:0x801197d0"
        },
        "task": { "+0x14": k.ptr(0x8d4, 1)?, "+0x18": k.ptr(0x8e0, 1)?, "+0x2c": k.ptr(0x960, 1)? }
    } }))
}

/// Run one routine by name on a JSON input.
pub fn replay(rel_bytes: &[u8], routine: &str, input: &Value) -> Result<Value> {
    let k = KingBoomBoo::read(rel_bytes)?;
    fn parse<T: serde::de::DeserializeOwned>(v: &Value) -> Result<T> {
        T::deserialize(v).map_err(|e| Error::invalid(format!("stage 63 request: {e}")))
    }
    match routine {
        "setup" => setup(&k, &parse(input)?),
        "teardown" => teardown(&k, &parse(input)?),
        "exec" => exec(&k),
        "fence" => fence(&k, parse::<FenceInput>(input)?.player),
        "spawn" => spawn(&k, &parse(input)?),
        _ => Err(Error::invalid(format!(
            "unknown stage 63 routine {routine}"
        ))),
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FenceInput {
    player: [f32; 3],
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both builds when present: `SA2_GC_REL_DIR` holds the disc RELs,
    /// `SA2_GAME_PATH` is the Steam install.
    fn builds() -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        if let Ok(dir) = std::env::var("SA2_GC_REL_DIR") {
            if let Ok(b) = std::fs::read(std::path::Path::new(&dir).join("boss_bigbogyD.rel")) {
                out.push(b);
            }
        }
        if let Ok(dir) = std::env::var("SA2_GAME_PATH") {
            let p = std::path::Path::new(&dir).join("resource/gd_PC/boss_bigbogyD.rel");
            if let Ok(b) = std::fs::read(p) {
                out.push(b);
            }
        }
        out
    }
    fn all(k: &KingBoomBoo<'_>) -> Value {
        let spawn_in = SpawnInput {
            task: true,
            work: true,
            position: [0.0, -40.0, 0.0],
            angle: 0,
            centre: [200.0, -40.0, 0.0],
            focus: [1.0, 2.0, 3.0],
        };
        json!([
            setup(
                k,
                &SetupInput {
                    handle: 7,
                    slots: [0, 5]
                }
            )
            .unwrap(),
            teardown(
                k,
                &TeardownInput {
                    slots: [json!("ours"), json!(9)],
                    handle: 7
                }
            )
            .unwrap(),
            exec(k).unwrap(),
            fence(k, [500.0, 3.0, 0.0]).unwrap(),
            fence(k, [210.0, 3.0, 10.0]).unwrap(),
            spawn(k, &spawn_in).unwrap(),
        ])
    }

    #[test]
    fn rejects_other_files() {
        assert!(KingBoomBoo::read(&[0; 64]).is_err());
    }

    #[test]
    fn stage_program() {
        let builds = builds();
        let mut outputs = Vec::new();
        for b in &builds {
            let k = KingBoomBoo::read(b).unwrap();
            let v = all(&k);
            let setup = &v[0];
            assert_eq!(setup["calls"].as_array().unwrap().len(), 29);
            assert_eq!(setup["calls"][2]["args"][0]["text"], "set_b_BigBogy_s.bin");
            assert_eq!(setup["calls"][24]["args"][0]["text"], "boss_02a.adx");
            assert_eq!(setup["state"]["slots"], json!(["s1+0x32240", 5]));
            assert_eq!(setup["state"]["trail"][1], "s1+0x13e08");
            assert_eq!(v[1]["state"]["slots"], json!([0, 9]));
            assert_eq!(v[2]["calls"][0]["args"][0], json!([0.0, -40.0, 0.0]));
            assert_eq!(v[2]["state"]["task+0x10"], "s1+0xf124");
            // Outside the 260 circle around (200, 0): pulled back to x = 460.
            assert_eq!(v[3]["state"]["player"], json!([460.0, 3.0, 0.0]));
            assert_eq!(v[4]["state"]["player"], json!([210.0, 3.0, 10.0]));
            // The spawn's reach uses position.x twice: |(-200, 0)| = 200.
            assert_eq!(v[5]["state"]["mwp"]["+0xc4"], json!(200.0));
            assert_eq!(v[5]["calls"][1]["module"], "s1+0x562bc");
            outputs.push(v);
        }
        // Disc and Steam builds agree on everything ported.
        if let [gc, steam] = outputs.as_slice() {
            assert_eq!(gc, steam);
        }
    }

    #[test]
    fn steam_layout_maps_to_disc() {
        for b in builds() {
            let k = KingBoomBoo::read(&b).unwrap();
            assert_eq!(k.code_offset(0xf124).unwrap(), 0xf124);
            if !k.gc() {
                assert_eq!(k.code_offset(0x569e8).unwrap(), 0x56cdc);
                assert_eq!(k.code_offset(0x19798).unwrap(), 0x19a8c);
            }
        }
    }
}
