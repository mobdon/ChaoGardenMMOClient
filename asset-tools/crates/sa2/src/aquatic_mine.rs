//! Aquatic Mine (story level 7, stg07D.rel): the stage program "STG07" that
//! `_prolog` (+0x7604) hands to LoadSubprogram, its stage task, the water
//! areas, and the stage's simple objects. All offsets are section-1 (code)
//! offsets of the GameCube disc stg07D.rel; engine calls are reported at
//! their GSNE8P main.dol addresses.
//!
//! | Offset | Size | Routine |
//! | --- | --- | --- |
//! | +0x7630 | 0x43C | stage setup (the program's first entry) |
//! | +0x7A6C | 0x1FC | stage teardown (second entry) |
//! | +0x7C88 | 0x1E4 | stage task exec: random effect near player 0 |
//! | +0x7E6C | 0x13C | stage task +0x14 callback: water test, fog and effect switch |
//! | +0x58B0 | 0xFC | water level under a point (16-row table .data+0x38C34) |
//! | +0x579C | 0x114 | water box of one NJS_OBJECT (centre, half extents) |
//! | +0x12210 | 0x2C | fog block select (.bss+0x86C) |
//! | +0x5100 | 0x80 | area task spawn (.bss+4) |
//! | +0x51B4 | 0x78 | area task warm-up exec |
//! | +0x522C | 0x44 | area task destructor |
//! | +0x5270 | 0x30 | flag every child task (+0x4C) |
//! | +0x44B20 | 0xA4 | clear the 32 bytes at 0x802B99E0 |
//! | +0xE08 | 0x50 | prop constructor (MITRUCKP, MIKUSA, MILATTI, MIRAIL, MIRING, MIFENCE) |
//! | +0xE58 / +0xE68 / +0xEC0 | | prop destructor / exec / display |
//! | +0x3824 | 0x2C0 | frame constructor (MIFRAME, MILIFT, MIBAR, MIWOOD, MIWOODY) |
//! | +0x3B34 / +0x3C10 / +0x3CF8 | | frame exec / display / masked display |
//! | +0x106C | 0x5C | NOCOLSKULL loader |
//! | +0x1180 | 0x68 | CMDOOR display |
//!
//! Constants and data pointers are read from the module through the
//! relocation of the instruction that loads them; engine addresses are
//! checked against the GC module's imports when the GC REL is given.
//!
//! The Steam `resource/gd_PC/stg07D.rel` is not byte-identical: its data
//! sections are, but its code lacks two blocks of the GC build (the setup's
//! install of hook slot 0x801A2A18+0x14 at +0x78C4..+0x78E8 and the
//! teardown's removal of it at +0x7AD0..+0x7AF8), so its later code sits
//! 0x24 and then 0x4C bytes lower. Both are accepted and offsets are
//! always reported in GC terms. Where the two differ the Steam behaviour is
//! ported: setup and teardown leave slot +0x14 alone for either input.
//! Rodata and data sections are byte-identical.
use crate::{
    city_rel::{CityRel, Reference},
    manifest, Error, Result,
};
use serde::Deserialize;
use serde_json::{json, Value};

/// GameCube disc `stg07D.rel` (module 7).
pub const GC_REL_SHA256: &str = "55a59f0773d73b7a7269d08ebb3da77fa4358e2bc3c4b61dea01d3fd289c874a";
pub const GC_REL_LEN: usize = 1_307_688;
/// Installed Steam `resource/gd_PC/stg07D.rel` (module 6).
pub const STEAM_REL_SHA256: &str =
    "9356f15b8160ffd975639755d54845477399067c0b84f9c21356b8ee2dd96715";
pub const STEAM_REL_LEN: usize = 1_307_528;

/// GC code blocks the Steam build does not have: (start, end).
const STEAM_GAPS: [(u32, u32); 2] = [(0x78c4, 0x78e8), (0x7ad0, 0x7af8)];

fn invalid() -> Error {
    Error::invalid("unexpected stage 7 code")
}
fn s(section: u8, offset: u32) -> Value {
    json!(format!("s{section}+0x{offset:x}"))
}
fn module_call(offset: u32, args: Value) -> Value {
    json!({ "module": format!("s1+0x{offset:x}"), "args": args })
}
fn bits(v: f32) -> u32 {
    v.to_bits()
}
fn fl(v: u32) -> f32 {
    f32::from_bits(v)
}

pub struct AquaticMine<'a> {
    rel: CityRel<'a>,
    steam: bool,
}

impl<'a> AquaticMine<'a> {
    pub fn read(bytes: &'a [u8]) -> Result<Self> {
        let (module, steam) = match (bytes.len(), manifest::sha256(bytes).as_str()) {
            (GC_REL_LEN, GC_REL_SHA256) => (7, false),
            (STEAM_REL_LEN, STEAM_REL_SHA256) => (6, true),
            _ => return Err(Error::invalid("unsupported stage 7 REL revision")),
        };
        Ok(Self {
            rel: CityRel::read_identified(bytes, module)?,
            steam,
        })
    }
    /// True for the Steam build (no hook slot +0x14).
    pub fn is_steam(&self) -> bool {
        self.steam
    }
    /// This module's code offset of a GC code offset.
    pub fn code(&self, pc: u32) -> Result<u32> {
        if !self.steam {
            return Ok(pc);
        }
        let mut shift = 0;
        for (lo, hi) in STEAM_GAPS {
            if pc >= hi {
                shift += hi - lo;
            } else if pc >= lo {
                return Err(Error::invalid(format!("+0x{pc:x} is GC-only stage 7 code")));
            }
        }
        Ok(pc - shift)
    }
    /// The GC code offset of one of this module's code offsets.
    pub fn gc_code(&self, pc: u32) -> u32 {
        if !self.steam {
            return pc;
        }
        let mut gc = pc;
        for (lo, hi) in STEAM_GAPS {
            if gc >= lo {
                gc += hi - lo;
            }
        }
        gc
    }
    fn reference(&self, pc: u32) -> Result<Option<Reference>> {
        Ok(self.rel.reference(1, self.code(pc)?))
    }
    /// The section offset the `addi` at `pc` (relocated at `pc + 2`) points at.
    fn at(&self, pc: u32, section: u8) -> Result<u32> {
        match self.reference(pc + 2)? {
            Some(Reference {
                module,
                kind: 4,
                section: sec,
                addend,
            }) if sec == section && module == self.rel.module_id() => Ok(if section == 1 {
                self.gc_code(addend)
            } else {
                addend
            }),
            _ => Err(Error::invalid(format!(
                "unexpected stage 7 code at +0x{pc:x}"
            ))),
        }
    }
    fn ptr(&self, pc: u32, section: u8) -> Result<Value> {
        Ok(s(section, self.at(pc, section)?))
    }
    fn word(&self, section: u8, at: u32) -> Result<u32> {
        let at = if section == 1 { self.code(at)? } else { at };
        let d = self.rel.data(section)?;
        d.get(at as usize..at as usize + 4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
            .ok_or_else(invalid)
    }
    /// The single-precision constant the `addi` at `pc` addresses (.rodata).
    fn f(&self, pc: u32) -> Result<f32> {
        Ok(fl(self.word(4, self.at(pc, 4)?)?))
    }
    /// The single-precision data word the `addi` at `pc` addresses, plus `k`.
    fn fdata(&self, pc: u32, k: u32) -> Result<f32> {
        Ok(fl(self.word(5, self.at(pc, 5)? + k)?))
    }
    /// The GC target of the module-local `bl` at `pc`.
    fn local(&self, pc: u32) -> Result<u32> {
        let ins = self.word(1, pc)?;
        if ins & 0xfc00_0003 != 0x4800_0001 || self.reference(pc)?.is_some() {
            return Err(Error::invalid(format!("no local call at +0x{pc:x}")));
        }
        let disp = ((ins & 0x03ff_fffc) as i32) << 6 >> 6;
        let here = self.code(pc)?;
        Ok(self.gc_code(here.checked_add_signed(disp).ok_or_else(invalid)?))
    }
    fn local_call(&self, pc: u32, args: Value) -> Result<Value> {
        Ok(module_call(self.local(pc)?, args))
    }
    /// The main-program call at `pc`: a `bl` with an import relocation to
    /// main.dol; on the GC module the import must name `address`.
    fn eng(&self, pc: u32, address: u32, args: Value) -> Result<Value> {
        match self.reference(pc)? {
            Some(Reference {
                module: 0,
                kind: 10,
                addend,
                ..
            }) if self.steam || addend == address => {
                Ok(json!({ "engine": format!("0x{address:08x}"), "args": args }))
            }
            _ => Err(Error::invalid(format!(
                "no call to 0x{address:08x} at +0x{pc:x}"
            ))),
        }
    }
    /// Check the `addi` at `pc` loads main-program address `address`.
    fn dol(&self, pc: u32, address: u32) -> Result<()> {
        match self.reference(pc + 2)? {
            Some(Reference {
                module: 0,
                kind: 4,
                addend,
                ..
            }) if self.steam || addend == address => Ok(()),
            _ => Err(Error::invalid(format!(
                "no load of 0x{address:08x} at +0x{pc:x}"
            ))),
        }
    }
    /// A data word as a module address when it carries an ADDR32
    /// relocation to this module, else its value.
    fn data_value(&self, section: u8, at: u32) -> Result<Value> {
        Ok(match self.rel.local_pointer(section, at) {
            Ok(Some((1, off))) => s(1, self.gc_code(off)),
            Ok(Some((sec, off))) => s(sec, off),
            Ok(None) => json!(0),
            Err(_) if self.rel.reference(section, at).is_none() => json!(self.word(section, at)?),
            Err(e) => return Err(e),
        })
    }
    /// The 16-bit immediate of the `li rD,imm` at `pc` (`addi rD,0,imm`).
    fn li(&self, pc: u32, d: u32) -> Result<i16> {
        let ins = self.word(1, pc)?;
        if ins >> 16 != 0x3800 | d << 5 {
            return Err(Error::invalid(format!("no li r{d} at +0x{pc:x}")));
        }
        Ok(ins as u16 as i16)
    }
}

// ---- Stage program ---------------------------------------------------------

/// Hook slots of 0x801A2A18 the program installs: (word index, setup `addi`
/// of the routine, teardown `addi` of the same routine).
const HOOKS: [(usize, u32, u32); 7] = [
    (0, 0x77e0, 0x7b5c),
    (1, 0x7804, 0x7b84),
    (2, 0x7828, 0x7bac),
    (3, 0x784c, 0x7bd4),
    (4, 0x7870, 0x7bfc),
    (7, 0x7894, 0x7b38),
    (9, 0x78b8, 0x7b0c),
];

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupInput {
    /// fn_80065048's return, kept at .bss+0xC.
    pub handle: u32,
    /// CreateElementalTask returned the stage task.
    pub task: bool,
    /// The ten words of 0x801A2A18 before the setup.
    pub slots: [u32; 10],
    /// .bss+0x10 when the setup reads it (after +0x5100).
    pub effect: u32,
    /// fn_8008C868's return, kept at .bss+8.
    pub buffer: u32,
    /// Game state 0x801CC168 +0x1E (non-zero: a second player).
    pub second: i8,
}

/// +0x7630. Returns the ordered calls and every location it writes
/// (`task` is the stage task's +0x14 callback; `trail` is 0x801E77D8).
pub fn setup(am: &AquaticMine<'_>, i: &SetupInput) -> Result<Value> {
    let f = |pc| am.f(pc).map(bits);
    let p5 = |pc| am.ptr(pc, 5);
    let lights = am.at(0x771c, 5)?;
    let mut c = vec![
        am.eng(0x7650, 0x800324e4, json!([128, p5(0x764c)?]))?,
        am.eng(
            0x7674,
            0x800245ec,
            json!([p5(0x7658)?, p5(0x7660)?, p5(0x7668)?, p5(0x7670)?]),
        )?,
        am.eng(0x7690, 0x80072270, json!([p5(0x767c)?, p5(0x7688)?]))?,
        am.eng(0x769c, 0x80072218, json!([p5(0x767c)?, p5(0x7688)?]))?,
        am.eng(0x76a8, 0x80065048, json!([0, 0x800]))?,
        am.eng(0x76c4, 0x8006638c, json!([p5(0x76bc)?, i.handle]))?,
        am.eng(0x76c8, 0x8001a8a8, json!([]))?,
        am.eng(0x76cc, 0x80033710, json!([]))?,
        am.eng(
            0x76ec,
            0x800c3cd8,
            json!([f(0x76d4)?, f(0x76e0)?, f(0x76d4)?]),
        )?,
        am.eng(0x7700, 0x8011e154, json!([f(0x76e0)?, f(0x76f8)?]))?,
        am.eng(
            0x7710,
            0x8011e150,
            json!([f(0x76e0)?, f(0x76e0)?, f(0x76e0)?]),
        )?,
        // The second light block is the first plus 0x20 (addi at +0x772C).
        am.eng(0x7724, 0x8002506c, json!([0, s(5, lights)]))?,
        am.eng(0x7730, 0x8002506c, json!([1, s(5, lights + 0x20)]))?,
        am.eng(0x773c, 0x8002506c, json!([2, s(5, lights)]))?,
        am.eng(0x7748, 0x8002506c, json!([3, s(5, lights)]))?,
        am.eng(0x7754, 0x80025028, json!([0, 0]))?,
        am.eng(0x7760, 0x80025028, json!([0, 1]))?,
        am.eng(0x7768, 0x80025048, json!([0]))?,
        am.eng(0x7770, 0x8002503c, json!([2]))?,
        am.local_call(0x777c, json!([p5(0x7778)?]))?,
        am.eng(0x7784, 0x80011a04, json!([0]))?,
        am.eng(0x7790, 0x80011050, json!([p5(0x778c)?]))?,
        am.eng(
            0x77ac,
            0x80071e0c,
            json!([2, 1, am.ptr(0x77a0, 1)?, p5(0x77a8)?]),
        )?,
    ];
    let task = if i.task {
        Some(am.ptr(0x77bc, 1)?)
    } else {
        None
    };
    c.push(am.local_call(0x77c4, json!([]))?);
    am.dol(0x77cc, 0x801a2a18)?;
    let mut slots: Vec<Value> = i.slots.iter().map(|v| json!(v)).collect();
    // Steam behaviour: slot +0x14 (+0x27E04) is not installed; the GC disc
    // build installs it at +0x78C4..+0x78E8 (left out, see the module doc).
    for &(k, pc, _) in &HOOKS {
        if i.slots[k] == 0 {
            slots[k] = am.ptr(pc, 1)?;
        }
    }
    c.push(am.local_call(0x78c0, json!([]))?);
    c.extend([
        am.local_call(0x78f8, json!([p5(0x78ec)?, am.ptr(0x78f4, 6)?]))?,
        am.local_call(0x790c, json!([p5(0x7900)?, am.ptr(0x7908, 6)?]))?,
        am.local_call(0x7910, json!([]))?,
        am.local_call(0x791c, json!([0, 0]))?,
        am.local_call(0x7920, json!([]))?,
    ]);
    am.at(0x7928, 6)?; // .bss+0x10, the `effect` word
    if i.effect == 0 {
        c.push(am.local_call(0x7940, json!([0, am.li(0x793c, 4)?]))?);
    }
    c.extend([
        am.local_call(0x794c, json!([p5(0x7948)?]))?,
        am.local_call(0x7958, json!([p5(0x7954)?]))?,
        am.eng(0x7964, 0x8008c8f0, json!([p5(0x7960)?]))?,
        am.eng(0x7970, 0x8008c868, json!([p5(0x796c)?]))?,
    ]);
    am.at(0x7978, 6)?; // .bss+8 takes the buffer
    c.extend([
        am.local_call(0x798c, json!([p5(0x7984)?, am.li(0x7988, 4)?]))?,
        am.local_call(0x7998, json!([0, 0]))?,
        am.local_call(0x79a4, json!([0, 0]))?,
    ]);
    am.dol(0x79ac, 0x801cc168)?;
    if i.second != 0 {
        c.push(am.local_call(0x79c8, json!([1, 0]))?);
        c.push(am.local_call(0x79d4, json!([1, 0]))?);
    }
    am.dol(0x79dc, 0x800e014c)?;
    am.dol(0x79e4, 0x801e77d8)?;
    let mut trail = vec![json!(0x800e014c_u32)];
    for pc in [0x79f0, 0x79fc, 0x7a08] {
        trail.push(am.ptr(pc, 1)?);
    }
    trail.resize(14, json!(0));
    c.push(am.local_call(0x7a3c, json!([]))?);
    c.push(am.eng(0x7a50, 0x8006d500, json!([p5(0x7a44)?, am.ptr(0x7a4c, 4)?]))?);
    Ok(json!({ "calls": c, "state": {
        "handle": i.handle, "buffer": i.buffer, "slots": slots, "trail": trail, "task": task } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeardownInput {
    /// .bss+8, .bss+0x10 and .bss+0xC.
    pub buffer: u32,
    pub effect: u32,
    pub handle: u32,
    /// Each 0x801A2A18 word: `"ours"` (the routine setup installs there) or a value.
    pub slots: [Value; 10],
}

/// +0x7A6C: release the stage's resources and remove its hooks.
pub fn teardown(am: &AquaticMine<'_>, i: &TeardownInput) -> Result<Value> {
    let mut slots = Vec::new();
    for v in &i.slots {
        if v != "ours" && !v.is_u64() {
            return Err(Error::invalid("teardown slot must be \"ours\" or a word"));
        }
        slots.push(v.clone());
    }
    let clear = |slots: &mut Vec<Value>, k: usize, pc: u32| -> Result<()> {
        am.ptr(pc, 1)?;
        if slots[k] == "ours" {
            slots[k] = json!(0);
        }
        Ok(())
    };
    am.at(0x7a80, 6)?;
    let mut c = vec![
        am.eng(0x7a88, 0x80119828, json!([i.buffer]))?,
        am.local_call(0x7a94, json!([]))?,
        am.local_call(0x7a98, json!([]))?,
    ];
    if i.effect != 0 {
        c.push(am.eng(0x7ab0, 0x80071af0, json!([i.effect]))?);
    }
    c.extend([
        am.local_call(0x7ab4, json!([]))?,
        am.local_call(0x7ab8, json!([]))?,
        am.eng(0x7abc, 0x8002455c, json!([]))?,
        am.local_call(0x7ac0, json!([]))?,
        am.local_call(0x7ac4, json!([]))?,
        am.eng(0x7ac8, 0x8011c2bc, json!([]))?,
        am.eng(0x7acc, 0x8011c310, json!([]))?,
    ]);
    // Steam behaviour: slot +0x14 is left alone (GC disc: +0x7AD0..+0x7AF8).
    for &(k, _, pc) in &HOOKS {
        clear(&mut slots, k, pc)?;
    }
    c.push(am.local_call(0x7b20, json!([]))?);
    c.extend([
        am.eng(0x7c10, 0x800336dc, json!([]))?,
        am.eng(
            0x7c24,
            0x80072118,
            json!([am.ptr(0x7c18, 5)?, am.ptr(0x7c20, 5)?]),
        )?,
        am.eng(0x7c28, 0x80066364, json!([]))?,
    ]);
    if i.handle != 0 {
        c.push(am.eng(0x7c40, 0x8001a2b0, json!([i.handle]))?);
    }
    c.push(am.eng(0x7c4c, 0x800245b8, json!([]))?);
    c.push(am.local_call(0x7c50, json!([]))?);
    Ok(json!({ "calls": c, "state": {
        "buffer": 0, "handle": 0, "effect": i.effect, "slots": slots } }))
}

// ---- Stage task ------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskExecInput {
    /// playertwp[0] is set.
    pub player: bool,
    /// Game state 0x801CC168 +0x7C.
    pub clock: u32,
    /// Stage task work +1.
    pub under: i8,
    /// Player 0's position (twp +0x14, bit patterns).
    pub pos: [u32; 3],
    /// What `rand` returns, in call order (only as many as are called).
    pub rands: Vec<i32>,
}

/// `(double)(int)r` rounded to single: the `xoris`/`lfd`/`fsubs` idiom
/// against the double at .rodata+0x290 (2^52 + 2^31, checked).
fn rand_f(am: &AquaticMine<'_>, pc: u32, r: i32) -> Result<f32> {
    let at = am.at(pc, 4)?;
    let d = (u64::from(am.word(4, at)?) << 32) | u64::from(am.word(4, at + 4)?);
    if d != 0x4330_0000_8000_0000 {
        return Err(invalid());
    }
    Ok(r as f32)
}

/// +0x7C88, the stage task exec: every third frame, with probability
/// `rand * 1/32768 < 0.3` and work +1 clear, effect 6 (fn_8006B7EC) at a
/// random offset (±150, ±100, ±150) around player 0.
pub fn task_exec(am: &AquaticMine<'_>, i: &TaskExecInput) -> Result<Value> {
    am.dol(0x7ca4, 0x801e7768)?;
    am.dol(0x7cb8, 0x801cc168)?;
    let mut c = Vec::new();
    let done = |c: Vec<Value>| Ok(json!({ "calls": c, "state": {} }));
    if !i.player || i.clock % 3 != 0 {
        return done(c);
    }
    let mut rands = i.rands.iter().copied();
    let mut next = |c: &mut Vec<Value>, pc: u32| -> Result<i32> {
        c.push(am.eng(pc, 0x8010dc88, json!([]))?);
        rands
            .next()
            .ok_or_else(|| Error::invalid("stage 7 task exec needs another rand value"))
    };
    let scale = am.f(0x7d0c)?;
    let r = next(&mut c, 0x7ce0)?;
    if !(scale * rand_f(am, 0x7ce8, r)? < am.f(0x7d1c)?) || i.under != 0 {
        return done(c);
    }
    let half = am.f(0x7d70)?;
    let mut p = [0u32; 3];
    for (k, (pc, cpc, mpc)) in [
        (0x7d40, 0x7d48, 0x7d80),
        (0x7d9c, 0x7da4, 0x7dd4),
        (0x7df0, 0x7df8, 0x7d80),
    ]
    .into_iter()
    .enumerate()
    {
        let r = next(&mut c, pc)?;
        let off = am.f(mpc)? * (scale * rand_f(am, cpc, r)? - half);
        p[k] = bits(fl(i.pos[k]) + off);
    }
    c.push(am.eng(0x7e50, 0x8006b7ec, json!([6, 0, 0, 0, p]))?);
    done(c)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WaterInput {
    /// .bss+0, the current area.
    pub area: i32,
    /// The active camera's position (camera +0, +4, +8; bit patterns).
    pub cam: [u32; 3],
    /// playertwp[0] is set, and playerpwp[0] +0x1CC.
    pub player: bool,
    pub status: u32,
    /// .bss+0x10 holds a task.
    pub effect: bool,
}

/// +0x7E6C, the stage task's +0x14 callback: below the water level (or,
/// with no level, while player 0 has status bit 1) select fog block
/// .bss+0x224, clear the effect task's work +0 and set work +1; otherwise
/// fog block .bss+0x14, effect work +0 = 1 and work +1 = 0.
pub fn water(am: &AquaticMine<'_>, i: &WaterInput) -> Result<Value> {
    am.dol(0x7e84, 0x803adad0)?;
    am.dol(0x7e94, 0x80175378)?;
    let level = water_level(am, i.area, i.cam)?;
    let none = am.f(0x7ea8)?;
    let under =
        (level == none && i.player && i.status & 2 != 0) || (level != none && fl(i.cam[1]) < level);
    let (pc, fpc) = if under {
        (0x7f30, 0x7f2c)
    } else {
        (0x7f68, 0x7f64)
    };
    let c = vec![
        module_call(0x58b0, json!(["camera"])),
        am.local_call(pc, json!([am.ptr(fpc, 6)?]))?,
    ];
    let fog = fog_select(am, am.at(fpc, 6)?)?;
    Ok(json!({ "calls": c, "state": {
        "level": bits(level),
        "under": u8::from(under),
        "effect": if i.effect { json!(u8::from(!under)) } else { Value::Null },
        "fog": fog } }))
}

/// One water box: the area it belongs to, the byte at row +8, the
/// NJS_OBJECT it is measured from, its centre and its X/Z half extents.
#[derive(Debug, Clone, PartialEq)]
pub struct WaterBox {
    pub area: i32,
    pub kind: u8,
    pub object: u32,
    pub centre: [f32; 3],
    pub half: [f32; 2],
}

/// The 16 rows of .data+0x38C34 (address from +0x58E4), each measured by
/// +0x579C: the bounding box of the object's model points, offset by the
/// object's position (its rotation and scale are not applied).
pub fn water_boxes(am: &AquaticMine<'_>) -> Result<Vec<WaterBox>> {
    let table = am.at(0x58e8, 5)?;
    let half = am.f(0x5844)?;
    (0..16)
        .map(|k| {
            let row = table + 12 * k;
            let object = match am.rel.local_pointer(5, row + 4)? {
                Some((5, off)) => off,
                _ => return Err(invalid()),
            };
            let (centre, half) = water_box(am, object, half)?;
            Ok(WaterBox {
                area: am.word(5, row)? as i32,
                kind: (am.word(5, row + 8)? >> 24) as u8,
                object,
                centre,
                half,
            })
        })
        .collect()
}

/// +0x579C on the NJS_OBJECT at .data+`object`.
fn water_box(am: &AquaticMine<'_>, object: u32, half: f32) -> Result<([f32; 3], [f32; 2])> {
    let model = match am.rel.local_pointer(5, object + 4)? {
        Some((5, off)) => off,
        _ => return Err(invalid()),
    };
    let points = match am.rel.local_pointer(5, model)? {
        Some((5, off)) => off,
        _ => return Err(invalid()),
    };
    let count = am.word(5, model + 8)? as i32;
    let p = |k: u32, c: u32| am.word(5, points + 12 * k + 4 * c).map(fl);
    // f4/f6 max/min x, f7/f8 max/min y, f3/f5 max/min z, seeded by point 0.
    let (mut max, mut min) = (
        [p(0, 0)?, p(0, 1)?, p(0, 2)?],
        [p(0, 0)?, p(0, 1)?, p(0, 2)?],
    );
    for k in 0..count.max(0) as u32 {
        for a in 0..3usize {
            let v = p(k, a as u32)?;
            if v > max[a] {
                max[a] = v;
            }
            if v < min[a] {
                min[a] = v;
            }
        }
    }
    let pos = |c: u32| am.word(5, object + 8 + 4 * c).map(fl);
    let centre = [
        pos(0)? + half * (max[0] + min[0]),
        pos(1)? + half * (max[1] + min[1]),
        pos(2)? + half * (max[2] + min[2]),
    ];
    Ok((centre, [half * (max[0] - min[0]), half * (max[2] - min[2])]))
}

/// +0x58B0: the centre height of the first box of `area` whose X/Z extent
/// strictly contains `pos`, else -100000 (.rodata+0x1D8).
pub fn water_level(am: &AquaticMine<'_>, area: i32, pos: [u32; 3]) -> Result<f32> {
    am.at(0x58f4, 6)?; // .bss+0, the area
    let (x, z) = (fl(pos[0]), fl(pos[2]));
    for b in water_boxes(am)? {
        if b.area == area
            && b.centre[0] + b.half[0] > x
            && b.centre[0] - b.half[0] < x
            && b.centre[2] + b.half[1] > z
            && b.centre[2] - b.half[1] < z
        {
            return Ok(b.centre[1]);
        }
    }
    am.f(0x58d8)
}

/// +0x12210: .bss+0x86C = the fog block, .bss+0x870 and +0x878 = 0.
fn fog_select(am: &AquaticMine<'_>, block: u32) -> Result<Value> {
    for pc in [0x12214, 0x12224, 0x12230] {
        am.at(pc, 6)?;
    }
    Ok(json!({ "block": s(6, block), "0x870": 0, "0x878": 0 }))
}

// ---- Area task -------------------------------------------------------------

/// +0x5100: create the area task (exec +0x51B4) unless .bss+4 holds one.
/// Returns the task (`ret`), and on creation clears its work +3 and the
/// area (.bss+0).
pub fn area_spawn(am: &AquaticMine<'_>, existing: bool, created: bool) -> Result<Value> {
    am.at(0x5114, 6)?;
    if existing {
        return Ok(json!({ "calls": [], "state": { "ret": 0 } }));
    }
    let c = vec![am.eng(
        0x513c,
        0x80071e0c,
        json!([2, 1, am.ptr(0x5130, 1)?, am.ptr(0x5138, 5)?]),
    )?];
    if !created {
        return Ok(json!({ "calls": c, "state": { "ret": 0 } }));
    }
    am.at(0x515c, 6)?;
    Ok(json!({ "calls": c, "state": { "ret": "task", "task": "task", "area": 0, "work3": 0 } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WarmupInput {
    /// Work +6.
    pub timer: u16,
    pub player: bool,
    /// .bss+0x8A0, the water-level hook (0: none).
    pub hook: u32,
}

/// +0x51B4: count frames; once player 0 exists after the third, switch to
/// the area exec +0x52A0, display +0x5640 and destructor +0x522C, and
/// install +0x58B0 as the water-level hook .bss+0x8A0 if none is set.
pub fn area_warmup(am: &AquaticMine<'_>, i: &WarmupInput) -> Result<Value> {
    am.dol(0x51c8, 0x801e7768)?;
    let timer = i.timer.wrapping_add(1);
    let mut task = Value::Null;
    let mut hook = json!(i.hook);
    if i.player && timer > 3 {
        task = json!([am.ptr(0x51f4, 1)?, am.ptr(0x51e8, 1)?, am.ptr(0x5200, 1)?]);
        am.at(0x520c, 6)?;
        if i.hook == 0 {
            hook = am.ptr(0x5220, 1)?;
        }
    }
    Ok(json!({ "calls": [], "state": { "timer": timer, "task": task, "hook": hook } }))
}

/// +0x522C: forget the area task (.bss+4) and the water-level hook when
/// they are this task and +0x58B0.
pub fn area_dest(am: &AquaticMine<'_>, own: bool, hook_ours: bool) -> Result<Value> {
    am.at(0x5230, 6)?;
    am.at(0x524c, 6)?;
    am.ptr(0x5258, 1)?;
    Ok(json!({ "calls": [], "state": {
        "area_task": if own { json!(0) } else { Value::Null },
        "hook": if hook_ours { json!(0) } else { Value::Null } } }))
}

/// +0x5270: every child of the task gets +0x4C = 1 (the frames free
/// themselves on it, see `frame_exec`).
pub fn flag_children(children: usize) -> Value {
    json!({ "calls": [], "state": { "children": vec![1; children] } })
}

/// +0x44B20: clear the 32 bytes at 0x802B99E0.
pub fn clear_flags(am: &AquaticMine<'_>) -> Result<Value> {
    am.dol(0x44b24, 0x802b99e0)?;
    Ok(json!({ "calls": [], "state": { "0x802b99e0": vec![0u8; 32] } }))
}

// ---- Props: MITRUCKP, MIKUSA, MILATTI, MIRAIL, MIRING, MIFENCE ---------------

/// A prop loader: its offset, the `addi`s of the model (+0x38) and texture
/// list (+0x3C), whether it sets work +1 = 3, the collision `addi` (0: none)
/// and the `bl` to +0xE08.
struct Prop {
    name: &'static str,
    loader: u32,
    model: u32,
    tex: u32,
    flag: Option<u32>,
    colli: u32,
    call: u32,
}
const PROPS: [Prop; 6] = [
    Prop {
        name: "MITRUCKP",
        loader: 0xb94,
        model: 0xbbc,
        tex: 0xbc8,
        flag: None,
        colli: 0xbd8,
        call: 0xbe0,
    },
    Prop {
        name: "MIKUSA",
        loader: 0xbf8,
        model: 0xc28,
        tex: 0xc34,
        flag: Some(0xc3c),
        colli: 0,
        call: 0xc50,
    },
    Prop {
        name: "MILATTI",
        loader: 0xc6c,
        model: 0xc94,
        tex: 0xca0,
        flag: None,
        colli: 0xcb0,
        call: 0xcb8,
    },
    Prop {
        name: "MIRAIL",
        loader: 0xcd0,
        model: 0xd00,
        tex: 0xd0c,
        flag: Some(0xd14),
        colli: 0,
        call: 0xd28,
    },
    Prop {
        name: "MIRING",
        loader: 0xd44,
        model: 0xd6c,
        tex: 0xd78,
        flag: None,
        colli: 0,
        call: 0xd8c,
    },
    Prop {
        name: "MIFENCE",
        loader: 0xda4,
        model: 0xdcc,
        tex: 0xdd8,
        flag: None,
        colli: 0xde8,
        call: 0xdf0,
    },
];
pub const PROP_NAMES: [&str; 6] = [
    "MITRUCKP", "MIKUSA", "MILATTI", "MIRAIL", "MIRING", "MIFENCE",
];

fn prop(name: &str) -> Result<&'static Prop> {
    PROPS
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| Error::invalid(format!("not a stage 7 prop: {name}")))
}

/// A prop loader and +0xE08: unless out of range, set the model, texture
/// list and callbacks (exec +0xE68, display +0xEC0, destructor +0xE58) and
/// set up collision when the loader passes it.
pub fn prop_load(am: &AquaticMine<'_>, name: &str, range_out: bool) -> Result<Value> {
    let p = prop(name)?;
    let range = match p.loader {
        0xbf8 => 0xc18,
        0xcd0 => 0xcf0,
        l => l + 0x18,
    };
    let mut c = vec![am.eng(range, 0x800655f4, json!(["task"]))?];
    if range_out {
        return Ok(json!({ "calls": c, "state": { "task": Value::Null } }));
    }
    if am.local(p.call)? != 0xe08 {
        return Err(invalid());
    }
    let count = am.li(p.call - 4, 5)?;
    let work1 = match p.flag {
        Some(pc) => json!(am.li(pc, 0)?),
        None => Value::Null,
    };
    if p.colli != 0 {
        c.push(am.eng(
            0xe44,
            0x8000f8b8,
            json!(["task", am.ptr(p.colli, 5)?, count, am.li(0xe40, 6)?]),
        )?);
    } else {
        am.li(p.call - 8, 4)?;
    }
    Ok(json!({ "calls": c, "state": {
        "task": [am.ptr(0xe24, 1)?, am.ptr(0xe18, 1)?, am.ptr(0xe30, 1)?],
        "model": am.ptr(p.model, 5)?, "tex": am.ptr(p.tex, 5)?, "work1": work1 } }))
}

/// +0xE68: collision entry while in range and set up.
pub fn prop_exec(am: &AquaticMine<'_>, range_out: bool, colli: bool) -> Result<Value> {
    let mut c = vec![am.eng(0xe88, 0x800655f4, json!(["task"]))?];
    if !range_out && colli {
        c.push(am.eng(0xea4, 0x8000f4ec, json!(["task"]))?);
    }
    Ok(json!({ "calls": c, "state": {} }))
}

/// +0xE58: clear the model and texture list words.
pub fn prop_dest() -> Value {
    json!({ "calls": [], "state": { "model": 0, "tex": 0 } })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropDisplayInput {
    /// Work +0 (scale pulse kind) and +1 (non-zero: rotate Z, Y, X).
    pub mode: i8,
    pub flag: i8,
    /// Work +6, +8/+0xC/+0x10 and +0x14 (bit patterns).
    pub timer: u16,
    pub rot: [u32; 3],
    pub pos: [u32; 3],
    /// Game state 0x801CC168 +0x7C.
    pub clock: u32,
    /// njSin's return (bit pattern), used when the prop pulses.
    pub sin: u32,
    /// Task +0x3C and +0x38.
    pub tex: Value,
    pub model: Value,
}

/// +0xEC0: draw the prop; mode 1 and 2 pulse its scale with
/// `njSin(rate * (clock + work +6))` (rates and amplitudes .data+0x20510..).
pub fn prop_display(am: &AquaticMine<'_>, i: &PropDisplayInput) -> Result<Value> {
    let mut c = vec![
        am.eng(0xee0, 0x80116e18, json!([i.tex]))?,
        am.eng(0xee8, 0x80115d60, json!([0]))?,
        am.eng(0xef4, 0x8011538c, json!([0, i.pos]))?,
    ];
    if i.flag != 0 {
        c.extend([
            am.eng(0xf10, 0x801158a8, json!([0, i.rot[2]]))?,
            am.eng(0xf1c, 0x80115828, json!([0, i.rot[1]]))?,
            am.eng(0xf28, 0x80115c60, json!([0, i.rot[0]]))?,
        ]);
    } else {
        c.push(am.eng(0xf38, 0x80115828, json!([0, i.rot[1]]))?);
    }
    let pulse = |rate_pc: u32, sin_pc: u32| -> Result<(Value, f32, f32)> {
        am.dol(0xf6c, 0x801cc168)?;
        let rate = am.word(5, am.at(rate_pc, 5)?)?;
        let angle = rate.wrapping_mul(i.clock.wrapping_add(u32::from(i.timer)));
        Ok((
            am.eng(sin_pc, 0x80112398, json!([angle]))?,
            am.f(0xf88)?,
            fl(i.sin),
        ))
    };
    if i.mode == 1 {
        let (call, one, s) = pulse(0xf60, 0xf80)?;
        let y = one + am.fdata(0xf94, 0)? * s;
        let x = one + am.fdata(0xfa8, 0)? * s;
        c.push(call);
        c.push(am.eng(0xfc0, 0x80115178, json!([0, bits(x), bits(y), bits(x)]))?);
    } else if i.mode == 2 {
        let (call, one, s) = pulse(0xfcc, 0xfec)?;
        let y = one + am.fdata(0x1000, 0)? * s;
        let x = one + am.fdata(0x1014, 0)? * s;
        let z = one + am.fdata(0x1028, 0)? * s;
        c.push(call);
        c.push(am.eng(0x1040, 0x80115178, json!([0, bits(x), bits(y), bits(z)]))?);
    }
    c.push(am.eng(0x1048, 0x80031a28, json!([i.model]))?);
    c.push(am.eng(0x1050, 0x80115dc8, json!([1]))?);
    Ok(json!({ "calls": c, "state": {} }))
}

// ---- Frames: MIFRAME, MILIFT, MIBAR, MIWOOD, MIWOODY ---------------------------

/// The five loaders that pass module data to the constructor +0x3824.
pub const FRAME_LOADERS: [(&str, u32); 5] = [
    ("MIFRAME", 0x36bc),
    ("MILIFT", 0x3704),
    ("MIBAR", 0x374c),
    ("MIWOOD", 0x3794),
    ("MIWOODY", 0x37dc),
];

/// The constructor arguments a frame loader passes in r4..r10: texture
/// list, model, NJS_OBJECT template (data offsets), the display choice,
/// the land flags, the collision info (0: none) and its count.
struct FrameArgs {
    tex: u32,
    model: u32,
    template: u32,
    masked: i16,
    land: i16,
    colli: u32,
    count: i16,
}

fn frame_args(am: &AquaticMine<'_>, frame: usize) -> Result<FrameArgs> {
    let (_, l) = *FRAME_LOADERS
        .get(frame)
        .ok_or_else(|| Error::invalid("frame index"))?;
    if am.local(l + 0x34)? != 0x3824 {
        return Err(invalid());
    }
    let colli = match am.word(1, l + 0x2c)? >> 16 {
        0x3920 => {
            am.li(l + 0x2c, 9)?;
            0
        }
        _ => am.at(l + 0x2c, 5)?,
    };
    Ok(FrameArgs {
        tex: am.at(l + 0x10, 5)?,
        model: am.at(l + 0x18, 5)?,
        template: am.at(l + 0x20, 5)?,
        masked: am.li(l + 0x24, 7)?,
        land: am.li(l + 0x28, 8)?,
        colli,
        count: am.li(l + 0x30, 10)?,
    })
}

/// The template's model (its +4) and that model's centre (+0x18) and radius (+0x24).
fn frame_model(am: &AquaticMine<'_>, template: u32) -> Result<(u32, [u32; 3], f32)> {
    let model = match am.rel.local_pointer(5, template + 4)? {
        Some((5, off)) => off,
        _ => return Err(invalid()),
    };
    let w = |k| am.word(5, model + k);
    Ok((model, [w(0x18)?, w(0x1c)?, w(0x20)?], fl(w(0x24)?)))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FrameLoadInput {
    pub frame: usize,
    /// Work +1 (non-zero skips the range check).
    pub work1: i8,
    pub range_out: bool,
    /// syCalloc returned memory.
    pub alloc: bool,
    /// Work +8/+0xC/+0x10 and +0x14 (bit patterns).
    pub rot: [u32; 3],
    pub pos: [u32; 3],
}

/// A frame loader and +0x3824: allocate the 76-byte dynamic land object
/// (`dyn`, task +0x38), copy the NJS_OBJECT template into it with the
/// task's angles, transform its position (njCalcPoint), register it
/// (fn_800232A4) with a size class from the model radius, set up collision
/// and transform the model centre into `dyn+0x38`.
pub fn frame_load(am: &AquaticMine<'_>, i: &FrameLoadInput) -> Result<Value> {
    let a = frame_args(am, i.frame)?;
    let mut c = Vec::new();
    let stop =
        |c: Vec<Value>| json!({ "calls": c, "state": { "task": Value::Null, "dyn": Value::Null } });
    if i.work1 == 0 {
        c.push(am.eng(0x3870, 0x800655f4, json!(["task"]))?);
        if i.range_out {
            return Ok(stop(c));
        }
    }
    c.push(am.eng(0x3884, 0x801197fc, json!([1, 76]))?);
    if !i.alloc {
        return Ok(stop(c));
    }
    let display = if a.masked != 0 {
        am.ptr(0x38c0, 1)?
    } else {
        am.ptr(0x38d0, 1)?
    };
    let mut d: Vec<Value> = (0..14)
        .map(|k| am.data_value(5, a.template + 4 * k))
        .collect::<Result<_>>()?;
    let first = d[0].as_u64().ok_or_else(invalid)? as u32;
    d[0] = json!(first & !3);
    d[5] = json!(i.rot[0]);
    d[6] = json!(i.rot[1]);
    d[7] = json!(i.rot[2]);
    d.resize(19, json!(0));
    d[17] = s(5, a.tex);
    d[18] = s(5, a.model);
    let (_, centre, radius) = frame_model(am, a.template)?;
    let r = am.f(0x39e4)? + radius;
    let size = if r > am.f(0x39fc)? {
        0
    } else if r > am.f(0x3a10)? {
        0x2000_0000
    } else if r > am.f(0x3a2c)? {
        0x4000_0000
    } else {
        0x6000_0000
    };
    let rotate = |c: &mut Vec<Value>, at: [u32; 3]| -> Result<()> {
        c.extend([
            am.eng(at[0], 0x801158a8, json!([0, i.rot[2]]))?,
            am.eng(at[1], 0x80115828, json!([0, i.rot[1]]))?,
            am.eng(at[2], 0x80115c60, json!([0, i.rot[0]]))?,
        ]);
        Ok(())
    };
    c.extend([
        am.eng(0x3988, 0x80116098, json!([]))?,
        am.eng(0x3990, 0x801153e4, json!([0]))?,
        am.eng(0x399c, 0x8011538c, json!([0, i.pos]))?,
    ]);
    rotate(&mut c, [0x39a8, 0x39b4, 0x39c0])?;
    c.extend([
        am.eng(0x39d0, 0x80114f0c, json!([0, "dyn+8", "dyn+8"]))?,
        am.eng(0x39d8, 0x80115dc8, json!([1]))?,
        am.eng(
            0x3a50,
            0x800232a4,
            json!([(a.land as i32 as u32 | 1) | size, "task", "dyn"]),
        )?,
    ]);
    if a.colli != 0 {
        c.push(am.eng(
            0x3a6c,
            0x8000f8b8,
            json!(["task", s(5, a.colli), a.count, 4]),
        )?);
    }
    c.extend([
        am.eng(0x3a70, 0x80116098, json!([]))?,
        am.eng(0x3a78, 0x801153e4, json!([0]))?,
        am.eng(0x3a84, 0x8011538c, json!([0, "dyn+8"]))?,
    ]);
    rotate(&mut c, [0x3a90, 0x3a9c, 0x3aa8])?;
    c.extend([
        am.eng(0x3ac0, 0x80114f0c, json!([0, centre, "dyn+0x38"]))?,
        am.eng(0x3ac8, 0x80115dc8, json!([1]))?,
    ]);
    Ok(json!({ "calls": c, "state": {
        "task": [am.ptr(0x38dc, 1)?, display, am.ptr(0x38e8, 1)?, "dyn"], "dyn": d } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FrameExecInput {
    pub frame: usize,
    pub work1: i8,
    pub range_out: bool,
    /// `dyn`+4 (the registered model) is set.
    pub obj: bool,
    /// _rename_EitherPlayerWithinSphere's answer.
    pub within: bool,
    /// Work +4.
    pub flags: i16,
    /// Task +0x4C (set by the area task's +0x5270).
    pub dead: bool,
    /// Work +0x2C (collision) is set.
    pub colli: bool,
}

/// +0x3B34: flag 0x100 of work +4 while a player is within the model's
/// radius + 15 of `dyn+0x38`; free the task once flagged dead, else feed
/// collision.
pub fn frame_exec(am: &AquaticMine<'_>, i: &FrameExecInput) -> Result<Value> {
    let mut c = Vec::new();
    let mut flags = i.flags;
    if i.work1 == 0 {
        c.push(am.eng(0x3b64, 0x800655f4, json!(["task"]))?);
        if i.range_out {
            return Ok(json!({ "calls": c, "state": { "flags": flags } }));
        }
    }
    if i.obj {
        let a = frame_args(am, i.frame)?;
        let (_, _, radius) = frame_model(am, a.template)?;
        let r = am.f(0x3b88)? + radius;
        c.push(am.eng(0x3b98, 0x800398d8, json!(["dyn+0x38", bits(r)]))?);
        if i.within {
            flags |= 0x100;
        } else {
            flags &= !0x100;
        }
    }
    if i.dead {
        flags &= !0x100;
        c.push(am.eng(0x3bdc, 0x80071ae0, json!(["task"]))?);
    } else if i.colli {
        c.push(am.eng(0x3bf4, 0x8000f4ec, json!(["task"]))?);
    }
    Ok(json!({ "calls": c, "state": { "flags": flags } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameDisplayInput {
    /// The masked display +0x3CF8 (else +0x3C10).
    pub masked: bool,
    /// Game state 0x801CC168 +0x3D and the screen being drawn (0x803ADAD0).
    pub split: i8,
    pub screen: i32,
    pub rot: [u32; 3],
    pub pos: [u32; 3],
    /// `dyn`+0x44 and +0x48.
    pub tex: Value,
    pub model: Value,
}

/// +0x3C10 / +0x3CF8: draw the frame, except on the screen the split mode
/// hides it on (mode 1 screen 0, mode 2 screen 1, mode 3 both); the masked
/// display wraps the draw in OnControl3D/OffControl3D(0x2400).
pub fn frame_display(am: &AquaticMine<'_>, i: &FrameDisplayInput) -> Result<Value> {
    let o = if i.masked { 0xe8 } else { 0 };
    am.dol(0x3c30 + o, 0x801cc168)?;
    am.dol(0x3c48 + o, 0x803adad0)?;
    let hidden = (i.split == 1 && i.screen == 0) || (i.split == 2 && i.screen == 1) || i.split == 3;
    let mut c = Vec::new();
    if hidden {
        return Ok(json!({ "calls": c, "state": {} }));
    }
    let k = if i.masked { 8 } else { 0 }; // OnControl3D shifts the masked body
    c.push(am.eng(0x3c90 + o, 0x80116e18, json!([i.tex]))?);
    if i.masked {
        let mask = am.li(0x3d7c, 3)?;
        c.push(am.eng(0x3d80, 0x8002b2e8, json!([mask]))?);
    }
    c.extend([
        am.eng(0x3c98 + o + k, 0x80115d60, json!([0]))?,
        am.eng(0x3ca4 + o + k, 0x8011538c, json!([0, i.pos]))?,
        am.eng(0x3cb0 + o + k, 0x801158a8, json!([0, i.rot[2]]))?,
        am.eng(0x3cbc + o + k, 0x80115828, json!([0, i.rot[1]]))?,
        am.eng(0x3cc8 + o + k, 0x80115c60, json!([0, i.rot[0]]))?,
        am.eng(0x3cd4 + o + k, 0x80031a28, json!([i.model]))?,
        am.eng(0x3cdc + o + k, 0x80115dc8, json!([1]))?,
    ]);
    if i.masked {
        c.push(am.eng(0x3dd4, 0x8002b2d8, json!([am.li(0x3dd0, 3)?]))?);
    }
    Ok(json!({ "calls": c, "state": {} }))
}

// ---- NOCOLSKULL, CMDOOR display, SIGN -----------------------------------------

/// +0x106C (NOCOLSKULL): in range, the shared model object +0xB8E4 with
/// the model, its texture list and land flags 4.
pub fn nocolskull(am: &AquaticMine<'_>, range_out: bool) -> Result<Value> {
    let mut c = vec![am.eng(0x1084, 0x800655f4, json!(["task"]))?];
    if !range_out {
        c.push(am.local_call(
            0x10b0,
            json!([
                "task",
                am.ptr(0x1098, 5)?,
                am.ptr(0x10a0, 5)?,
                0,
                0,
                am.li(0x10ac, 8)?
            ]),
        )?);
    }
    Ok(json!({ "calls": c, "state": {} }))
}

/// +0x11E8 (SIGN): the shared model object +0xB8E4 with the model, the
/// second word of the texture record .data+0x220B4, and land flags 4.
pub fn sign(am: &AquaticMine<'_>) -> Result<Value> {
    let tex = am.data_value(5, am.at(0x1200, 5)? + 4)?;
    let c = vec![am.local_call(
        0x1214,
        json!(["task", am.ptr(0x11f8, 5)?, tex, 0, 0, am.li(0x1210, 8)?]),
    )?];
    Ok(json!({ "calls": c, "state": {} }))
}

/// +0x1180, CMDOOR's display: the door model (second word of
/// .data+0x21B84) at the task's place and Y angle.
pub fn cmdoor_display(am: &AquaticMine<'_>, rot_y: u32, pos: [u32; 3]) -> Result<Value> {
    let model = am.data_value(5, am.at(0x119c, 5)? + 4)?;
    let c = vec![
        am.eng(0x11a8, 0x80116e18, json!([am.ptr(0x11a4, 5)?]))?,
        am.eng(0x11ac, 0x80116098, json!([]))?,
        am.eng(0x11b4, 0x8011616c, json!([pos]))?,
        am.eng(0x11c0, 0x80115828, json!([0, rot_y]))?,
        am.eng(0x11c8, 0x80031a28, json!([model]))?,
        am.eng(0x11cc, 0x801160e4, json!([]))?,
    ];
    Ok(json!({ "calls": c, "state": {} }))
}

// ---- Replay ------------------------------------------------------------------

/// Run one routine by name on a JSON input.
pub fn replay(rel_bytes: &[u8], routine: &str, input: &Value) -> Result<Value> {
    let am = AquaticMine::read(rel_bytes)?;
    fn parse<T: serde::de::DeserializeOwned>(v: &Value) -> Result<T> {
        T::deserialize(v).map_err(|e| Error::invalid(format!("stage 7 request: {e}")))
    }
    let field = |k: &str| input.get(k).cloned().unwrap_or(Value::Null);
    let flag = |k: &str| parse::<bool>(&field(k));
    match routine {
        "setup" => setup(&am, &parse(input)?),
        "teardown" => teardown(&am, &parse(input)?),
        "task_exec" => task_exec(&am, &parse(input)?),
        "water" => water(&am, &parse(input)?),
        "water_level" => Ok(json!(bits(water_level(
            &am,
            parse(&field("area"))?,
            parse(&field("pos"))?
        )?))),
        "area_spawn" => area_spawn(&am, flag("existing")?, flag("created")?),
        "area_warmup" => area_warmup(&am, &parse(input)?),
        "area_dest" => area_dest(&am, flag("own")?, flag("hook_ours")?),
        "flag_children" => Ok(flag_children(parse(&field("children"))?)),
        "clear_flags" => clear_flags(&am),
        "prop_load" => prop_load(&am, &parse::<String>(&field("name"))?, flag("range_out")?),
        "prop_exec" => prop_exec(&am, flag("range_out")?, flag("colli")?),
        "prop_dest" => Ok(prop_dest()),
        "prop_display" => prop_display(&am, &parse(input)?),
        "frame_load" => frame_load(&am, &parse(input)?),
        "frame_exec" => frame_exec(&am, &parse(input)?),
        "frame_display" => frame_display(&am, &parse(input)?),
        "nocolskull" => nocolskull(&am, flag("range_out")?),
        "sign" => sign(&am),
        "cmdoor_display" => cmdoor_display(&am, parse(&field("rot_y"))?, parse(&field("pos"))?),
        _ => Err(Error::invalid(format!("unknown stage 7 routine {routine}"))),
    }
}
