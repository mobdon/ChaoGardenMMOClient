//! Egg Golem, Sonic's fight (story level 64, Boss_GolemD.rel): the stage
//! program "GOLEM   " that `_prolog` (+0xF950) hands to LoadSubprogram, its
//! background task, the arena fence and the arena pieces the program spawns.
//! All offsets are section-1 (code) offsets of the GameCube disc REL; engine
//! calls are reported at their GSNE8P main.dol addresses.
//!
//! | Offset | Size | Routine |
//! | --- | --- | --- |
//! | +0xF97C | 0x248 | stage setup (the program's first entry) |
//! | +0xFBC4 | 0xB0 | stage teardown (second entry) |
//! | +0xFC74 | 0x8C | stage program exec (third entry): spawns the boss and the arena once |
//! | +0xFD00 | 0x11C | arena fence, run by the shared exec +0xFE1C the program installs |
//! | +0xFE40 | 0x184 | background task display (pulsing overlay and scrolling texture) |
//! | +0xFFC4 | 0x190 | six-task ring around the arena |
//! | +0xB218 | 0x1E4 | twelve-platform ring |
//! | +0xB3FC | 0xC4 | platforms 2 and 10 start in mode 3 |
//!
//! Constants and data pointers are read from the module through the
//! relocation of the instruction that loads them (the offsets are recorded at
//! each use). Every engine call is checked against the module's own import
//! relocation at its call site; for the GameCube module the relocation must
//! name the reported GSNE8P address.
//!
//! The Steam `resource/gd_PC/Boss_GolemD.rel` (module 86) is a different
//! build: the code below +0x17EC0 is the same instruction for instruction, but
//! a shared routine near +0x17EC0 is 0x2F4 bytes shorter, so every later code
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

/// GameCube disc `Boss_GolemD.rel` (module 87).
pub const GC_REL_SHA256: &str = "277d4b97f31826996843952f455384c0104e95828f9f66eb292f5bc421801360";
pub const GC_REL_LEN: usize = 1_525_334;
/// Installed Steam `resource/gd_PC/Boss_GolemD.rel` (module 86).
pub const STEAM_REL_SHA256: &str =
    "5d28501afa5a35d7f653823c7829fb6428092e8c65ffe966d499c64713940896";
pub const STEAM_REL_LEN: usize = 1_524_190;
const GC_MODULE: u32 = 87;
const STEAM_MODULE: u32 = 86;
/// Steam code at or past this offset sits `STEAM_SHIFT` below the GameCube
/// copy (GameCube +0x17EC0..+0x181A4, part of a routine shared by 70 RELs, has
/// no Steam counterpart); earlier code is at the same offset in both.
const STEAM_SHIFTED_FROM: u32 = 0x17eb0;
const STEAM_SHIFT: u32 = 0x2f4;

fn invalid() -> Error {
    Error::invalid("unexpected stage 64 code")
}
fn bad(pc: u32) -> Error {
    Error::invalid(format!("unexpected stage 64 code at +0x{pc:x}"))
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

pub struct EggGolem<'a> {
    rel: CityRel<'a>,
}

impl<'a> EggGolem<'a> {
    pub fn read(bytes: &'a [u8]) -> Result<Self> {
        let module = match (bytes.len(), manifest::sha256(bytes).as_str()) {
            (GC_REL_LEN, GC_REL_SHA256) => GC_MODULE,
            (STEAM_REL_LEN, STEAM_REL_SHA256) => STEAM_MODULE,
            _ => return Err(Error::invalid("unsupported stage 64 REL revision")),
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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
/// GSNE8P game state 0x801CC168 (+0x7C is the frame clock).
const GAME_STATE: u32 = 0x801c_c168;

impl EggGolem<'_> {
    /// A single-precision value `add` bytes past the `.data` address the
    /// `addi` at `pc` loads.
    fn fd(&self, pc: u32, add: u32) -> Result<f32> {
        Ok(f32::from_bits(self.word(5, self.at(pc, 5)? + add)?))
    }
    /// The word at the `.data` address the `addi` at `pc` loads.
    fn wd(&self, pc: u32) -> Result<u32> {
        self.word(5, self.at(pc, 5)?)
    }
    /// The signed immediate of the `addi`/`li` at `pc`.
    fn simm(&self, pc: u32) -> Result<i32> {
        let ins = self.word(1, pc)?;
        if ins >> 26 != 14 {
            return Err(bad(pc));
        }
        Ok(ins as u16 as i16 as i32)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupInput {
    /// fn_800651EC's return, kept at .bss+0x748.
    pub handle: u32,
    /// The word at 0x801A2A18 before the setup.
    pub slot: u32,
    /// CreateElementalTask returned the background task.
    pub task: bool,
}

/// +0xF97C. Returns the ordered calls and the final value of every location
/// it writes.
pub fn setup(g: &EggGolem<'_>, i: &SetupInput) -> Result<Value> {
    let p5 = |pc| g.ptr(pc, 5);
    let mut c = vec![
        g.eng(0xf998, 0x8002_493c, json!([p5(0xf994)?]))?,
        g.eng(0xf9b4, 0x8007_2270, json!([p5(0xf9a0)?, p5(0xf9ac)?]))?,
        g.eng(0xf9c0, 0x8007_2218, json!([p5(0xf9a0)?, p5(0xf9ac)?]))?,
        g.eng(0xf9dc, 0x8007_2270, json!([p5(0xf9c8)?, p5(0xf9d4)?]))?,
        g.eng(0xf9e8, 0x8007_2218, json!([p5(0xf9c8)?, p5(0xf9d4)?]))?,
        g.eng(
            0xfa00,
            0x8006_51ec,
            json!([g.text(0xf9f0)?, g.text(0xf9f8)?, 512]),
        )?,
        g.eng(0xfa1c, 0x8006_638c, json!([p5(0xfa14)?, i.handle]))?,
        g.eng(0xfa20, 0x8003_3710, json!([]))?,
    ];
    g.at(0xfa08, 6)?; // .bss+0x748 keeps the handle
                      // Light blocks: base (+0xFA2C), base+0x20 twice, base again.
    let lights = g.at(0xfa2c, 5)?;
    for (pc, n, add) in [
        (0xfa34, 0, 0),
        (0xfa44, 1, 0x20),
        (0xfa50, 2, 0x20),
        (0xfa5c, 3, 0),
    ] {
        c.push(g.eng(pc, 0x8002_506c, json!([n, s(5, lights + add)]))?);
    }
    c.extend([
        g.eng(0xfa68, 0x8002_5028, json!([0, 0]))?,
        g.eng(0xfa74, 0x8002_5028, json!([0, 1]))?,
        g.eng(0xfa7c, 0x8002_5048, json!([2]))?,
        g.eng(0xfa84, 0x8002_503c, json!([2]))?,
        g.eng(0xfa90, 0x8003_bcc4, json!([p5(0xfa8c)?]))?,
        g.local_call(0xfa94, json!([]))?,
    ]);
    g.dol(0xfa9c, HOOKS)?;
    let slot = if i.slot == 0 {
        g.ptr(0xfab0, 1)?
    } else {
        json!(i.slot)
    };
    c.extend([
        g.eng(0xfac8, 0x8007_2820, json!([g.text(0xfabc)?, p5(0xfac4)?]))?,
        g.local_call(0xfad4, json!([0, 0]))?,
        g.eng(0xfadc, 0x8001_1a04, json!([0]))?,
        g.eng(0xfae8, 0x8001_1050, json!([g.text(0xfae4)?]))?,
        g.local_call(0xfaf4, json!([g.text(0xfaf0)?]))?,
        g.eng(
            0xfb10,
            0x8007_1e0c,
            json!([2, 1, g.ptr(0xfb04, 1)?, g.text(0xfb0c)?]),
        )?,
    ]);
    let task = if i.task {
        json!({ "+0x14": g.ptr(0xfb20, 1)? })
    } else {
        Value::Null
    };
    c.push(g.eng(0xfb28, 0x8001_8e5c, json!([]))?);
    // Trail plugin table: GSNE8P fn_800E014C, three module routines, ten zeros.
    g.dol(0xfb38, TRAIL)?;
    let mut trail = vec![json!(g.dol(0xfb30, 0x800e_014c)?)];
    for pc in [0xfb44, 0xfb50, 0xfb5c] {
        trail.push(g.ptr(pc, 1)?);
    }
    trail.resize(14, json!(0));
    c.extend([
        g.local_call(0xfb90, json!([]))?,
        g.eng(
            0xfba4,
            0x8006_d500,
            json!([g.text(0xfb98)?, g.ptr(0xfba0, 4)?]),
        )?,
        g.eng(0xfba8, 0x8001_a8a8, json!([]))?,
    ]);
    Ok(json!({ "calls": c, "state": {
        "handle": i.handle, "slot": slot, "trail": trail, "task": task } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeardownInput {
    /// The 0x801A2A18 slot: `"ours"` (the routine setup installed) or a word.
    pub slot: Value,
    /// .bss+0x748.
    pub handle: u32,
}

/// +0xFBC4: undo the setup's hook and release the stage's resources.
pub fn teardown(g: &EggGolem<'_>, i: &TeardownInput) -> Result<Value> {
    let p5 = |pc| g.ptr(pc, 5);
    g.dol(0xfbd8, HOOKS)?;
    g.at(0xfbe4, 1)?; // cleared only when it still holds setup's routine
    let slot = if i.slot == "ours" {
        json!(0)
    } else if i.slot.is_u64() {
        i.slot.clone()
    } else {
        return Err(Error::invalid("teardown slot must be \"ours\" or a word"));
    };
    let mut c = vec![
        g.eng(0xfbf8, 0x8003_36dc, json!([]))?,
        g.eng(0xfc0c, 0x8007_2118, json!([p5(0xfc00)?, p5(0xfc08)?]))?,
        g.eng(0xfc20, 0x8007_2118, json!([p5(0xfc14)?, p5(0xfc1c)?]))?,
        g.eng(0xfc2c, 0x8011_66f0, json!([p5(0xfc28)?]))?,
        g.eng(0xfc30, 0x8006_6364, json!([]))?,
    ];
    g.at(0xfc38, 6)?;
    if i.handle != 0 {
        c.push(g.eng(0xfc48, 0x8001_a2b0, json!([i.handle]))?);
    }
    c.extend([
        g.eng(0xfc54, 0x8002_45b8, json!([]))?,
        g.local_call(0xfc58, json!([]))?,
        g.local_call(0xfc5c, json!([]))?,
    ]);
    Ok(json!({ "calls": c, "state": { "handle": 0, "slot": slot } }))
}

/// +0xFC74, the stage program's exec: spawns the boss at .rodata+0xBB0, the
/// six-task ring, the twelve platforms and their two mode-3 platforms, sets
/// the fence radius (.bss+0x74C) to .rodata+0xBBC and replaces itself with the
/// shared exec +0xFE1C (which runs the fence).
pub fn exec(g: &EggGolem<'_>) -> Result<Value> {
    let pos = [g.f(0xfc8c, 0)?, g.f(0xfc8c, 4)?, g.f(0xfc8c, 8)?];
    let calls = vec![
        g.local_call(0xfcb8, json!([pos, 0, g.ptr(0xfcb4, 5)?]))?,
        g.local_call(0xfcbc, json!([]))?,
        g.local_call(0xfcc0, json!([]))?,
        g.local_call(0xfcc4, json!([]))?,
    ];
    Ok(json!({ "calls": calls, "state": {
        "radius": [g.ptr(0xfcd8, 6)?, g.f(0xfccc, 0)?],
        "task+0x10": g.ptr(0xfce4, 1)?
    } }))
}

/// +0xFD00, the arena fence around the origin. `radius` is .bss+0x74C: at
/// .rodata+0xBBC it widens to .rodata+0xBC8 while the player is below
/// .rodata+0xBC4; otherwise it returns to .rodata+0xBBC once the player is at
/// or above that height and inside the narrow circle. Player 0 farther than
/// the radius in XZ is put back on the circle.
pub fn fence(g: &EggGolem<'_>, player: [f32; 3], radius: f32) -> Result<Value> {
    g.dol(0xfd14, PLAYERTWP)?;
    g.at(0xfd54, 6)?;
    let v = [player[0], g.f(0xfd28, 0)?, player[2]];
    let (narrow, low, wide) = (g.f(0xfd48, 0)?, g.f(0xfd70, 0)?, g.f(0xfd84, 0)?);
    let mut calls = vec![g.eng(0xfd40, 0x8011_51f4, json!([v]))?];
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    let mut r = radius;
    if r == narrow {
        if player[1] < low {
            r = wide;
        }
    } else if player[1] >= low && len < narrow {
        r = narrow;
    }
    let mut pos = player;
    if len > r {
        pos[0] = r * (v[0] / len);
        pos[2] = r * (v[2] / len);
    }
    calls.truncate(1);
    Ok(json!({ "calls": calls, "state": { "player": pos, "radius": r } }))
}

/// `fctiwz`: truncate toward zero, saturating; NaN gives 0x80000000.
fn fctiwz(x: f32) -> i32 {
    if x.is_nan() {
        i32::MIN
    } else {
        x as i32
    }
}

/// +0xFE40, the background task display. Over each 60-frame cycle of the
/// clock (game state +0x7C) the overlay's alpha rises for 5 frames
/// (255 * n * 0.25) and falls over the other 55 (255 * (59 - n) / 54); the
/// second texture scrolls by -clock / 2.
pub fn display(g: &EggGolem<'_>, clock: u32) -> Result<Value> {
    g.dol(0xfe58, GAME_STATE)?;
    let n = clock % 60;
    let alpha = if n >= 5 {
        fctiwz(g.f(0xfee8, 0)? * ((59 - n) as f32 / g.f(0xff1c, 0)?))
    } else {
        fctiwz(g.f(0xfe88, 0)? * (n as f32 * g.f(0xfeb8, 0)?))
    };
    let color = (alpha as u32 & 0xff) << 24 | 0x00ff_ffff;
    let model = match g.rel.local_pointer(5, g.at(0xff70, 5)? + 4)? {
        Some((sec, off)) => s(sec, off),
        None => json!(0),
    };
    let ctl = g.simm(0xff94)?;
    let calls = vec![
        g.eng(0xff4c, 0x8011_6e18, json!([g.ptr(0xff48, 5)?]))?,
        g.local_call(0xff5c, json!([g.ptr(0xff54, 5)?, color]))?,
        g.eng(0xff68, 0x8011_6e18, json!([g.ptr(0xff64, 5)?]))?,
        g.eng(
            0xff90,
            0x8007_3028,
            json!([model, g.ptr(0xff80, 5)?, clock.wrapping_neg() >> 1]),
        )?,
        g.eng(0xff98, 0x8002_b2e8, json!([ctl]))?,
        g.eng(0xffa0, 0x8011_e158, json!([model]))?,
        g.eng(0xffa8, 0x8002_b2d8, json!([g.simm(0xffa4)?]))?,
    ];
    Ok(json!({ "calls": calls, "state": {} }))
}

/// The module helper +0xD330 (shared by 20 RELs): (r sin a, r cos a) for a
/// 16-bit angle. Computed here in single precision; the engine's njSin/njCos
/// rounding is not reproduced bit for bit.
fn polar(angle: i32, r: f32) -> (f32, f32) {
    let a = angle as f32 * (std::f32::consts::TAU / 65536.0);
    (r * a.sin(), r * a.cos())
}

/// +0xFFC4: six tasks (exec +0xDAF0) round the arena, kept at .bss+0x750.
/// `created[i]` says whether CreateElementalTask returned task i.
pub fn ring(g: &EggGolem<'_>, created: [bool; 6]) -> Result<Value> {
    let mut calls = Vec::new();
    let mut tasks = Vec::new();
    let (base, step) = (g.f(0xfff4, 0)?, g.f(0xfffc, 0)?);
    let scale = g.fd(0x10018, 0)? / g.f(0x10020, 0)? - g.f(0x10028, 0)?;
    for (n, made) in created.iter().enumerate() {
        calls.push(g.eng(
            0x10074,
            0x8007_1e0c,
            json!([2, 2, g.ptr(0x10040, 1)?, g.text(0x10048)?]),
        )?);
        if !made {
            tasks.push(Value::Null);
            continue;
        }
        let angle = fctiwz(base + step * n as f32);
        let r = g.fd(0x10038, 0)?;
        calls.push(g.local_call(0x10110, json!([angle, r, "twp+0x14", "twp+0x1c"]))?);
        let (x, z) = polar(angle, r);
        tasks.push(json!({
            "+0x8": g.wd(0xffec)?,
            "+0xc": angle.wrapping_add(g.wd(0x10050)? as i32),
            "+0x10": g.wd(0x10008)?,
            "+0x14": [x, g.fd(0x10058, 0)?, z],
            "+0x20": g.f(0x10010, 0)?,
            "+0x24": scale,
            "+0x28": g.fd(0x10030, 0)?,
        }));
    }
    Ok(json!({ "calls": calls, "state": { "list": g.ptr(0xffe0, 6)?, "twp": tasks } }))
}

/// +0xB218: twelve platform tasks (exec +0xB7A4) on a circle of radius
/// .rodata+0xA10, kept at .bss+0x710. The angle steps by 5462 after every
/// third platform starting with the first and by 5461 otherwise. The original
/// does not check CreateElementalTask's result.
pub fn platforms(g: &EggGolem<'_>) -> Result<Value> {
    let mut calls = Vec::new();
    let mut tasks = Vec::new();
    let (r, height) = (g.f(0xb264, 0)?, g.fd(0xb26c, 0)?);
    let mut angle = 0i32;
    for n in 0..12 {
        calls.push(g.eng(
            0xb2a0,
            0x8007_1e0c,
            json!([6, 2, g.ptr(0xb244, 1)?, g.text(0xb24c)?]),
        )?);
        calls.push(g.eng(0xb2c4, 0x8011_97d0, json!([g.simm(0xb2c0)?]))?);
        calls.push(g.local_call(0xb2e0, json!([angle, r, "stack", "stack"]))?);
        let (x, z) = polar(angle, r);
        tasks.push(json!({
            "task": { "+0x14": g.ptr(0xb254, 1)?, "+0x18": g.ptr(0xb25c, 1)?,
                "+0x38": "ret:0x801197d0", "+0x4c": n },
            "twp": { "+0x0": 0, "+0x1": -1, "+0x8": 0, "+0xc": angle, "+0x10": 0,
                "+0x14": [x, height, z], "+0x20": height, "+0x24": g.f(0xb274, 0)?,
                "+0x28": 0 },
            "mwp": { "+0x38": g.f(0xb27c, 0)?, "+0x3c": 1 },
        }));
        calls.push(g.local_call(0xb3d0, json!([format!("platform {n}")]))?);
        angle += if n % 3 == 0 {
            g.simm(0xb3c0)?
        } else {
            g.simm(0xb3c8)?
        };
    }
    Ok(json!({ "calls": calls, "state": { "list": g.ptr(0xb238, 6)?, "platforms": tasks } }))
}

/// +0xB3FC: platforms 2 and 10 of the list at .bss+0x710 are put in mode 3
/// (twp+0x28) with mwp+0x3C cleared and mwp+0x38 = .rodata+0xA14, then the
/// module helper +0xBBE8 (shared by 15 RELs) is called with 4.
pub fn platform_pair(g: &EggGolem<'_>) -> Result<Value> {
    g.at(0xb420, 6)?;
    let mut calls = Vec::new();
    let mut state = Vec::new();
    for (add, pc) in [(0xb424, 0xb464), (0xb468, 0xb49c)] {
        let n = g.simm(add)? / 4;
        calls.push(g.local_call(pc, json!([format!("platform {n}"), g.simm(pc - 4)?]))?);
        state.push(json!({ "platform": n, "twp+0x28": g.simm(0xb44c)?,
            "mwp+0x38": g.f(0xb438, 0)?, "mwp+0x3c": 0 }));
    }
    Ok(json!({ "calls": calls, "state": state }))
}

/// Run one routine by name on a JSON input.
pub fn replay(rel_bytes: &[u8], routine: &str, input: &Value) -> Result<Value> {
    let g = EggGolem::read(rel_bytes)?;
    fn parse<T: serde::de::DeserializeOwned>(v: &Value) -> Result<T> {
        T::deserialize(v).map_err(|e| Error::invalid(format!("stage 64 request: {e}")))
    }
    match routine {
        "setup" => setup(&g, &parse(input)?),
        "teardown" => teardown(&g, &parse(input)?),
        "exec" => exec(&g),
        "fence" => {
            let i: FenceInput = parse(input)?;
            fence(&g, i.player, i.radius)
        }
        "display" => display(&g, parse::<DisplayInput>(input)?.clock),
        "ring" => ring(&g, parse::<RingInput>(input)?.created),
        "platforms" => platforms(&g),
        "platform_pair" => platform_pair(&g),
        _ => Err(Error::invalid(format!(
            "unknown stage 64 routine {routine}"
        ))),
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FenceInput {
    player: [f32; 3],
    radius: f32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DisplayInput {
    clock: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RingInput {
    created: [bool; 6],
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both builds when present: `SA2_GC_REL_DIR` holds the disc RELs,
    /// `SA2_GAME_PATH` is the Steam install.
    fn builds() -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        if let Ok(dir) = std::env::var("SA2_GC_REL_DIR") {
            if let Ok(b) = std::fs::read(std::path::Path::new(&dir).join("Boss_GolemD.rel")) {
                out.push(b);
            }
        }
        if let Ok(dir) = std::env::var("SA2_GAME_PATH") {
            let p = std::path::Path::new(&dir).join("resource/gd_PC/Boss_GolemD.rel");
            if let Ok(b) = std::fs::read(p) {
                out.push(b);
            }
        }
        out
    }

    #[test]
    fn rejects_other_files() {
        assert!(EggGolem::read(&[0; 64]).is_err());
    }

    #[test]
    fn stage_program() {
        let mut outputs = Vec::new();
        for b in builds() {
            let g = EggGolem::read(&b).unwrap();
            let su = setup(
                &g,
                &SetupInput {
                    handle: 3,
                    slot: 0,
                    task: true,
                },
            )
            .unwrap();
            assert_eq!(su["calls"].as_array().unwrap().len(), 28);
            assert_eq!(su["calls"][5]["args"][0]["text"], "set_b_Golem_s.bin");
            assert_eq!(su["calls"][21]["args"][0]["text"], "boss_02b.adx");
            assert_eq!(su["calls"][23]["args"][3]["text"], "BgExec");
            assert_eq!(su["state"]["slot"], "s1+0x312f8");
            assert_eq!(su["state"]["task"]["+0x14"], "s1+0xfe40");
            let td = teardown(
                &g,
                &TeardownInput {
                    slot: json!("ours"),
                    handle: 3,
                },
            )
            .unwrap();
            assert_eq!(td["state"]["slot"], 0);
            let ex = exec(&g).unwrap();
            assert_eq!(ex["calls"][0]["module"], "s1+0x590");
            assert_eq!(ex["state"]["task+0x10"], "s1+0xfe1c");
            let narrow = ex["state"]["radius"][1].as_f64().unwrap() as f32;
            // Above the low line and inside: unchanged.
            let a = fence(&g, [100.0, 150.0, 0.0], narrow).unwrap();
            assert_eq!(a["state"]["player"], json!([100.0, 150.0, 0.0]));
            // Below 120 the radius widens to 300 and the player is pulled to it.
            let b = fence(&g, [400.0, 0.0, 0.0], narrow).unwrap();
            assert_eq!(
                b["state"],
                json!({ "player": [300.0, 0.0, 0.0], "radius": 300.0 })
            );
            // Back above 120 inside the narrow circle: narrow again.
            let c = fence(&g, [100.0, 130.0, 0.0], 300.0).unwrap();
            assert_eq!(c["state"]["radius"].as_f64().unwrap() as f32, narrow);
            // Alpha 255 * 4 * 0.25 = 255 at n = 4; 255 * 54 / 54 at n = 5.
            for clock in [4, 5, 64] {
                let d = display(&g, clock).unwrap();
                assert_eq!(d["calls"][1]["args"][1], 0xffff_ffff_u32);
            }
            let d = display(&g, 59).unwrap();
            assert_eq!(d["calls"][1]["args"][1], 0x00ff_ffff_u32);
            assert_eq!(d["calls"][4]["args"][0], 0x2400);
            let r = ring(&g, [true, false, true, true, true, true]).unwrap();
            assert_eq!(r["state"]["twp"][0]["+0x24"], json!(9.0));
            assert_eq!(r["state"]["twp"][1], Value::Null);
            let p = platforms(&g).unwrap();
            assert_eq!(p["state"]["platforms"][11]["twp"]["+0xc"], 60075);
            let pp = platform_pair(&g).unwrap();
            assert_eq!(pp["state"][1]["platform"], 10);
            outputs.push(json!([su, td, ex, a, b, c, d, r, p, pp]));
        }
        if let [gc, steam] = outputs.as_slice() {
            assert_eq!(gc, steam);
        }
    }
}
