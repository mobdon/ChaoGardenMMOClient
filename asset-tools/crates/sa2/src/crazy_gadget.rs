//! Crazy Gadget (stage 22, stg22D.rel): the stage program `_prolog`
//! (+0xB4B4) hands to LoadSubprogram (.data+0xBECFC), the gravity-angle
//! exec, the stage's voice-cue task and its three mechanism children, and
//! the stage task's destructor. All offsets are section-1 (code) offsets of
//! stg22D.rel; engine calls are reported at their GSNE8P main.dol addresses.
//!
//! | Offset | Size | Routine |
//! | --- | --- | --- |
//! | +0xB4E0 | 0x300 | stage setup (the program's first entry) |
//! | +0xB7E0 | 0xE4 | stage teardown (second entry) |
//! | +0xBB6C | 0x138 | stage program exec (third entry): gravity angles |
//! | +0xBAC4 | 0xA8 | gravity axis classifier |
//! | +0xB8C4 | 0x1E0 | voice-cue task exec ("ObjSetMa…", .bss+0x18) |
//! | +0x93B0 | 0x74 | mechanism children spawn (three rows .data+0xB0B1C) |
//! | +0x9360 | 0x50 | one mechanism child (also stg31D) |
//! | +0x9424 | 0x4C | mechanism child init |
//! | +0x9470 | 0x14 | mechanism child destructor |
//! | +0x9484 | 0x98 | mechanism child activation |
//! | +0xCF74 | 0x5C | stage task destructor |
//! | +0xCFD0 | 0x14 | scale by .data+0xC1DCC (also stg35/36) |
//!
//! Constants and data pointers are read from the module through the
//! relocation of the instruction that loads them; every engine call is
//! checked against the relocation of its `bl`. The Steam
//! `resource/gd_PC/stg22D.rel` has the same section bytes as the GameCube
//! disc copy, so either can be passed. REL-local helpers outside this port
//! are reported as `module` calls; engine results are inputs.
use crate::{
    city_rel::{CityRel, Reference},
    manifest, Error, Result,
};
use serde::Deserialize;
use serde_json::{json, Value};

/// GameCube disc `stg22D.rel` (module 22).
pub const GC_REL_SHA256: &str = "2c27e00124ca5a8702767253cb556b06ccf1a4720b818cbb65ee43e5b8dab347";
/// Installed Steam `resource/gd_PC/stg22D.rel` (module 21, same section bytes).
pub const STEAM_REL_SHA256: &str =
    "720030a6aefeec071240e7ad7da46e1c54f471e418bd3d239b41eecf7698cf59";
pub const REL_LEN: usize = 1_991_320;
const GC_MODULE: u32 = 22;

fn invalid() -> Error {
    Error::invalid("unexpected stage 22 code")
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

pub struct CrazyGadget<'a> {
    rel: CityRel<'a>,
}

impl<'a> CrazyGadget<'a> {
    pub fn read(bytes: &'a [u8]) -> Result<Self> {
        if bytes.len() != REL_LEN {
            return Err(Error::invalid("unsupported stage 22 REL revision"));
        }
        let module = match manifest::sha256(bytes).as_str() {
            GC_REL_SHA256 => GC_MODULE,
            STEAM_REL_SHA256 => 21,
            _ => return Err(Error::invalid("unsupported stage 22 REL revision")),
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
                "unexpected stage 22 code at +0x{pc:x}"
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
                "no engine call 0x{address:08x} at stage 22 +0x{pc:x}"
            ))),
        }
    }
}

/// A word that is either a plain value or one of the module's own
/// addresses (`"s1+0xbca4"`).
pub type Word = Value;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupInput {
    /// fn_80065048's return, kept at .bss+0x10.
    pub handle: u32,
    /// CreateElementalTask returned the stage task.
    pub task: bool,
    /// The words at 0x801A2A18 +0 and +8 before the setup.
    pub slots: [u32; 2],
}

/// +0xB4E0. Returns the ordered calls and the final value of every location
/// it writes (`task` is the stage task's callbacks at +0x14, +0x20, +0x18).
pub fn setup(k: &CrazyGadget<'_>, i: &SetupInput) -> Result<Value> {
    let p5 = |pc| k.ptr(pc, 5);
    let f = |pc| k.f(pc);
    let (a, b) = (p5(0xb52c)?, p5(0xb538)?);
    let (a2, b2) = (p5(0xb554)?, p5(0xb560)?);
    let mut c = vec![
        k.e(0xb500, 0x800324e4, json!([256, p5(0xb4fc)?]))?,
        k.e(
            0xb524,
            0x800245ec,
            json!([p5(0xb508)?, p5(0xb510)?, p5(0xb518)?, p5(0xb520)?]),
        )?,
        k.e(0xb540, 0x80072270, json!([a, b]))?,
        k.e(0xb54c, 0x80072218, json!([a, b]))?,
        k.e(0xb568, 0x80072270, json!([a2, b2]))?,
        k.e(0xb574, 0x80072218, json!([a2, b2]))?,
        k.e(0xb580, 0x80065048, json!([0, 0x800]))?,
    ];
    k.at(0xb588, 6)?; // .bss+0x10 holds the handle
    let (zero, one) = (f(0xb5ac)?, f(0xb5b8)?);
    let lights = k.at(0xb5f4, 5)?;
    c.extend([
        k.e(0xb59c, 0x8006638c, json!([p5(0xb594)?, i.handle]))?,
        k.e(0xb5a0, 0x8001a8a8, json!([]))?,
        k.e(0xb5a4, 0x80033710, json!([]))?,
        k.e(0xb5c4, 0x800c3cd8, json!([zero, one, zero]))?,
        k.e(0xb5d8, 0x8011e154, json!([one, f(0xb5d0)?]))?,
        k.e(0xb5e8, 0x8011e150, json!([one, one, one]))?,
        // Light 1 is the block plus 0x20 (addi at +0xB604); the others reuse it.
        k.e(0xb5fc, 0x8002506c, json!([0, s(5, lights)]))?,
        k.e(0xb608, 0x8002506c, json!([1, s(5, lights + 0x20)]))?,
        k.e(0xb614, 0x8002506c, json!([2, s(5, lights)]))?,
        k.e(0xb620, 0x8002506c, json!([3, s(5, lights)]))?,
        k.e(0xb62c, 0x80025028, json!([0, 0]))?,
        k.e(0xb638, 0x80025028, json!([0, 1]))?,
        k.e(0xb640, 0x80025048, json!([0]))?,
        k.e(0xb648, 0x8002503c, json!([2]))?,
        k.local_call(0xb654, json!([p5(0xb650)?]))?,
        k.e(0xb65c, 0x80011a04, json!([0]))?,
        k.e(0xb668, 0x80011050, json!([p5(0xb664)?]))?,
        k.e(0xb674, 0x8005d0ec, json!([p5(0xb670)?]))?,
        k.e(
            0xb690,
            0x80071e0c,
            json!([2, 1, k.ptr(0xb684, 1)?, p5(0xb68c)?]),
        )?,
    ]);
    let task = if i.task {
        Some([k.ptr(0xb6a0, 1)?, k.ptr(0xb6ac, 1)?, k.ptr(0xb6b8, 1)?])
    } else {
        None
    };
    c.extend([
        k.e(0xb6c8, 0x8003bcc4, json!([p5(0xb6c4)?]))?,
        k.local_call(0xb6cc, json!([]))?,
    ]);
    let mut slots: Vec<Word> = i.slots.iter().map(|v| json!(v)).collect();
    for (n, pc) in [(0, 0xb6e8), (1, 0xb70c)] {
        if i.slots[n] == 0 {
            slots[n] = k.ptr(pc, 1)?;
        }
    }
    k.at(0xb720, 6)?;
    c.extend([
        k.local_call(0xb724, json!([p5(0xb718)?, "s6+0x1c"]))?,
        k.e(0xb728, 0x8011c2b0, json!([]))?,
        k.e(0xb72c, 0x8011c310, json!([]))?,
        k.local_call(0xb730, json!([]))?,
        k.local_call(0xb73c, json!([p5(0xb738)?]))?,
        k.local_call(0xb748, json!([p5(0xb744)?]))?,
    ]);
    // Trail plugin table 0x801E77D8: fn_800E014C, three module routines, zeros.
    let mut trail = vec![json!(0x800e014c_u32)];
    for pc in [0xb764, 0xb770, 0xb77c] {
        trail.push(k.ptr(pc, 1)?);
    }
    trail.resize(14, json!(0));
    c.extend([
        k.local_call(0xb7b0, json!([]))?,
        k.e(0xb7c4, 0x8006d500, json!([p5(0xb7b8)?, k.ptr(0xb7c0, 4)?]))?,
    ]);
    Ok(json!({ "calls": c, "state": {
        "handle": i.handle, "slots": slots, "trail": trail, "task": task } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeardownInput {
    /// Each 0x801A2A18 slot (+0, +8): `"ours"` (the routine setup
    /// installed) or a value.
    pub slots: [Word; 2],
    /// .bss+0x10.
    pub handle: u32,
}

/// +0xB7E0: undo the setup's hooks and release the stage's resources.
pub fn teardown(k: &CrazyGadget<'_>, i: &TeardownInput) -> Result<Value> {
    let p5 = |pc| k.ptr(pc, 5);
    let mut c = Vec::new();
    for pc in [0xb7f0, 0xb7f4, 0xb7f8, 0xb7fc, 0xb800] {
        c.push(k.local_call(pc, json!([]))?);
    }
    c.extend([
        k.e(0xb804, 0x8011c2bc, json!([]))?,
        k.e(0xb808, 0x8011c310, json!([]))?,
    ]);
    for pc in [0xb81c, 0xb844] {
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
        k.e(0xb858, 0x800336dc, json!([]))?,
        k.e(0xb86c, 0x80072118, json!([p5(0xb860)?, p5(0xb868)?]))?,
        k.e(0xb880, 0x80072118, json!([p5(0xb874)?, p5(0xb87c)?]))?,
        k.e(0xb884, 0x80066364, json!([]))?,
    ]);
    k.at(0xb88c, 6)?;
    if i.handle != 0 {
        c.push(k.e(0xb89c, 0x8001a2b0, json!([i.handle]))?);
    }
    c.extend([
        k.e(0xb8a8, 0x800245b8, json!([]))?,
        k.local_call(0xb8ac, json!([]))?,
    ]);
    Ok(json!({ "calls": c, "state": { "handle": 0, "slots": slots } }))
}

/// +0xBAC4 on the gravity vector `v` (0x801E5624): 0 unless a component
/// passes ±.rodata+0x500/+0x504; the last test that passes wins (+X 0, -X 1,
/// +Y 2, -Y 3, +Z 4, -Z 5).
pub fn axis(k: &CrazyGadget<'_>, v: [u32; 3]) -> Result<i32> {
    let hi = f32::from_bits(k.f(0xbad0)?);
    let lo = f32::from_bits(k.f(0xbae8)?);
    for (pc, want) in [(0xbb04, hi), (0xbb1c, lo), (0xbb38, hi), (0xbb50, lo)] {
        if f32::from_bits(k.f(pc)?).to_bits() != want.to_bits() {
            return Err(invalid());
        }
    }
    let mut r = 0;
    for (n, x) in v.iter().map(|b| f32::from_bits(*b)).enumerate() {
        if x > hi {
            r = 2 * n as i32;
        }
        if x < lo {
            r = 2 * n as i32 + 1;
        }
    }
    Ok(r)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecInput {
    /// The gravity vector at 0x801E5624 (bits).
    pub gravity: [u32; 3],
    /// asinf's and atan2f's results (bits; atan2f is called only when the
    /// vector's X and Y are not both zero).
    pub asin: u32,
    pub atan2: Option<u32>,
    /// .bss+0x18, the voice-cue task, is set.
    pub cue: bool,
    /// CreateElementalTask's result for it.
    pub created: bool,
}

/// +0xBB6C, the program exec: gravity pitch to 0x803AD918 and yaw to
/// 0x803AD914 (as angles: radians × .rodata+0x508), the axis to
/// fn_80005508, and the voice-cue task (destructor +0xBAA4) if missing.
pub fn exec(k: &CrazyGadget<'_>, i: &ExecInput) -> Result<Value> {
    let g = i.gravity.map(f32::from_bits);
    let scale = k.d(0xbb94)?;
    let mut c = vec![k.e(0xbb8c, 0x80111058, json!([neg(i.gravity[2])]))?];
    let pitch = fctiwz_d(scale * f64::from(f32::from_bits(i.asin)));
    let zero = f32::from_bits(k.f(0xbbbc)?);
    let yaw = if zero == g[0] && zero == g[1] {
        0
    } else {
        c.push(k.e(
            0xbbf0,
            0x80111168,
            json!([neg(i.gravity[0]), neg(i.gravity[1])]),
        )?);
        let a = i.atan2.ok_or_else(|| Error::invalid("missing atan2"))?;
        if k.d(0xbbf8)?.to_bits() != scale.to_bits() {
            return Err(invalid());
        }
        fctiwz_d(scale * f64::from(f32::from_bits(a))).wrapping_neg()
    };
    c.push(k.local_call(0xbc3c, json!([0x801e5624_u32]))?);
    c.push(k.e(0xbc40, 0x80005508, json!([axis(k, i.gravity)?]))?);
    k.at(0xbc48, 6)?;
    let mut cue = json!("existing");
    if !i.cue {
        c.push(k.e(
            0xbc70,
            0x80071e0c,
            json!([2, 2, k.ptr(0xbc64, 1)?, k.ptr(0xbc6c, 5)?]),
        )?);
        cue = if i.created {
            json!({ "task": "cue", "dest": k.ptr(0xbc88, 1)? })
        } else {
            json!(0)
        };
    }
    Ok(json!({ "calls": c, "state": { "pitch": pitch, "yaw": yaw, "cue": cue } }))
}

/// `fctiwz` on a double.
fn fctiwz_d(x: f64) -> i32 {
    if x.is_nan() {
        i32::MIN
    } else {
        x as i32
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CueInput {
    /// Task +0xC (the mechanism children exist).
    pub children: bool,
    /// What +0x93B0's three CreateChildTask calls returned.
    pub created: Option<[bool; 3]>,
    /// 0x801E50B4 +0x20 (voice setting) and game state 0x801CC168 +0x42.
    pub voice: u8,
    pub multiplayer: i8,
    /// fn_80062AEC's minutes, seconds and frames.
    pub clock: Option<[i8; 3]>,
    /// Work +3 (cue bits).
    pub flags: u8,
    /// _rename_EitherPlayerWithinSphere's answers, in call order.
    pub within: Vec<bool>,
}

/// +0xB8C4, the voice-cue task exec: on its first frame the three
/// mechanism children (+0x93B0); then, with voice setting 1, timed voice
/// lines 1735–1737 and the two place-triggered lines 1738/1739 (once each,
/// work +3 bits 0 and 1).
pub fn cue_exec(k: &CrazyGadget<'_>, i: &CueInput) -> Result<Value> {
    let mut c = Vec::new();
    let mut spawned = Value::Null;
    if !i.children {
        c.push(k.local_call(0xb8e4, json!(["task"]))?);
        let made = i.created.ok_or_else(|| Error::invalid("missing created"))?;
        let s = spawn(k, made)?;
        c.extend(s["calls"].as_array().ok_or_else(invalid)?.iter().cloned());
        spawned = s["state"].clone();
    }
    let mut flags = i.flags;
    let mut within = i.within.iter().copied();
    if i.voice == 1 {
        let [m, sec, fr] = i.clock.ok_or_else(|| Error::invalid("missing clock"))?;
        c.push(k.e(0xb908, 0x80062aec, json!(["min", "sec", "frame"]))?);
        let voice = |pc, n: u32| k.e(pc, 0x800117ec, json!([n, 0]));
        if m == 0 && sec == 0 && fr == 5 {
            c.push(voice(0xb944, 1735)?);
        }
        if i.multiplayer == 0 {
            if m == 2 && sec == 0 && fr == 0 {
                c.push(voice(0xb998, 1736)?);
            }
            if flags & 2 == 0 && m >= 4 && m % 2 == 0 && sec == 0 && fr == 0 {
                c.push(voice(0xb9fc, 1737)?);
            }
        }
        let radius = k.f(0xba1c)?;
        if k.f(0xba64)? != radius {
            return Err(invalid());
        }
        for (bit, at, call, line) in [(1, 0xba14, 0xba24, 0xba38), (2, 0xba5c, 0xba6c, 0xba80)] {
            if flags & bit == 0 {
                c.push(k.e(call, 0x800398d8, json!([k.ptr(at, 5)?, radius]))?);
                if within
                    .next()
                    .ok_or_else(|| Error::invalid("missing within"))?
                {
                    c.push(voice(line, 1737 + bit as u32)?);
                    flags |= bit;
                }
            }
        }
    }
    Ok(json!({ "calls": c, "state": { "flags": flags, "children": spawned } }))
}

/// +0x93B0 with +0x9360: one child (exec +0x9424) per 32-byte row of
/// .data+0xB0B1C, its +0x4C set to the row.
pub fn spawn(k: &CrazyGadget<'_>, made: [bool; 3]) -> Result<Value> {
    let rows = k.at(0x93d8, 5)?;
    let mut c = Vec::new();
    let mut children = Vec::new();
    for (n, ok) in made.iter().enumerate() {
        c.push(k.e(0x9384, 0x800709f8, json!([2, k.ptr(0x9380, 1)?, "task"]))?);
        children.push(if *ok {
            s(5, rows + 32 * n as u32)
        } else {
            Value::Null
        });
    }
    Ok(json!({ "calls": c, "state": children }))
}

/// +0x9424, a mechanism child's first exec: install display +0xA058, exec
/// +0x951C and destructor +0x9470; task +0x38 = 0, task +0x40 and work
/// +0x28 = .rodata+0x370.
pub fn child_init(k: &CrazyGadget<'_>) -> Result<Value> {
    let z = k.f(0x9458)?;
    Ok(json!({ "calls": [], "state": {
        "display": k.ptr(0x942c, 1)?, "exec": k.ptr(0x9438, 1)?,
        "dest": k.ptr(0x9444, 1)?, "t38": 0, "t40": z, "w28": z } }))
}

/// +0x9470: clear task +0x38, +0x3C, +0x40.
pub fn child_dest() -> Value {
    json!({ "calls": [], "state": { "t38": 0, "t3c": 0, "t40": 0 } })
}

/// +0x9484(task, player): when the child is idle (work +0 and +6 zero),
/// activate it for `player`, lock that player's input (SetInputP 10, 11)
/// and return 1; otherwise 0.
pub fn child_activate(k: &CrazyGadget<'_>, mode: i8, timer: u16, player: u32) -> Result<Value> {
    if mode != 0 || timer != 0 {
        return Ok(json!({ "calls": [], "state": {}, "result": 0 }));
    }
    let z = k.f(0x94d4)?;
    let p = player & 0xff;
    Ok(json!({
        "calls": [k.e(0x94ec, 0x8003a508, json!([p, 10, 11]))?],
        "state": { "player": p, "mode": 1, "t38": 0, "t40": z, "w28": z },
        "result": 1 }))
}

/// +0xCF74, the stage task destructor: release .bss+0x14 if it is this
/// task, and free task +0x4C.
pub fn task_dest(k: &CrazyGadget<'_>, ours: bool, buffer: bool) -> Result<Value> {
    k.at(0xcf8c, 6)?;
    let mut c = Vec::new();
    if buffer {
        c.push(k.e(0xcfb0, 0x80119828, json!(["task+0x4c"]))?);
    }
    Ok(json!({ "calls": c, "state": {
        "slot": if ours { json!(0) } else { json!("unchanged") },
        "buffer": 0 } }))
}

/// +0xCFD0: x × the float at .data+0xC1DCC (single precision).
pub fn scale(k: &CrazyGadget<'_>, x: u32) -> Result<u32> {
    Ok((f32::from_bits(x) * f32::from_bits(k.dw(0xcfd4)?)).to_bits())
}

/// Run one routine by name on a JSON input.
pub fn replay(rel_bytes: &[u8], routine: &str, input: &Value) -> Result<Value> {
    let k = CrazyGadget::read(rel_bytes)?;
    fn parse<T: serde::de::DeserializeOwned>(v: &Value) -> Result<T> {
        T::deserialize(v).map_err(|e| Error::invalid(format!("stage 22 request: {e}")))
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Vector {
        v: [u32; 3],
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Activate {
        mode: i8,
        timer: u16,
        player: u32,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Dest {
        ours: bool,
        buffer: bool,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Scale {
        x: u32,
    }
    match routine {
        "setup" => setup(&k, &parse(input)?),
        "teardown" => teardown(&k, &parse(input)?),
        "exec" => exec(&k, &parse(input)?),
        "axis" => Ok(json!({ "result": axis(&k, parse::<Vector>(input)?.v)? })),
        "cue_exec" => cue_exec(&k, &parse(input)?),
        "spawn" => spawn(&k, parse(input)?),
        "child_init" => child_init(&k),
        "child_dest" => Ok(child_dest()),
        "child_activate" => {
            let a: Activate = parse(input)?;
            child_activate(&k, a.mode, a.timer, a.player)
        }
        "task_dest" => {
            let d: Dest = parse(input)?;
            task_dest(&k, d.ours, d.buffer)
        }
        "scale" => Ok(json!({ "result": scale(&k, parse::<Scale>(input)?.x)? })),
        _ => Err(Error::invalid(format!(
            "unknown stage 22 routine {routine}"
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
            v.push(std::fs::read(PathBuf::from(d).join("stg22D.rel")).unwrap());
        }
        if let Some(g) = std::env::var_os("SA2_GAME_PATH") {
            v.push(std::fs::read(PathBuf::from(g).join("resource/gd_PC/stg22D.rel")).unwrap());
        }
        v
    }

    #[test]
    fn rejects_other_modules() {
        assert!(CrazyGadget::read(&[0; 16]).is_err());
        assert!(CrazyGadget::read(&vec![0; REL_LEN]).is_err());
    }

    #[test]
    fn gc_and_steam_sections_match() {
        let r = rels();
        if r.len() < 2 {
            return;
        }
        let (a, b) = (
            CrazyGadget::read(&r[0]).unwrap(),
            CrazyGadget::read(&r[1]).unwrap(),
        );
        for sec in 1..=5 {
            assert_eq!(a.rel.data(sec).unwrap(), b.rel.data(sec).unwrap());
        }
    }

    #[test]
    fn routines_read_the_module() {
        for rel in rels() {
            let r = |name, v: Value| replay(&rel, name, &v).unwrap();
            let s = r(
                "setup",
                json!({ "handle": 7, "task": true, "slots": [0, 5] }),
            );
            let calls = s["calls"].as_array().unwrap();
            assert_eq!(calls.len(), 37);
            assert_eq!(
                s["state"]["task"],
                json!(["s1+0xcfe4", "s1+0xd598", "s1+0xcf74"])
            );
            assert_eq!(s["state"]["slots"], json!(["s1+0x37a44", 5]));
            assert_eq!(s["state"]["trail"][1], "s1+0x12418");
            let t = r("teardown", json!({ "slots": ["ours", 9], "handle": 7 }));
            assert_eq!(t["state"]["slots"], json!([0, 9]));
            assert_eq!(t["calls"].as_array().unwrap().len(), 15);

            // Gravity straight down (-Y): pitch from asin(0), no atan2.
            let down = [0u32, (-1f32).to_bits(), 0];
            let e = r(
                "exec",
                json!({ "gravity": [0, 0, 0], "asin": 0, "atan2": null,
                    "cue": false, "created": true }),
            );
            assert_eq!(e["state"]["yaw"], 0);
            assert_eq!(e["state"]["cue"]["dest"], "s1+0xbaa4");
            assert_eq!(r("axis", json!({ "v": down }))["result"], 3);
            let e = r(
                "exec",
                json!({ "gravity": down, "asin": 0, "atan2": 1f32.to_bits(),
                    "cue": true, "created": false }),
            );
            // 1 rad x 65536/(2 pi) = 10430.37 -> -10430.
            assert_eq!(e["state"]["yaw"], -10430);
            assert_eq!(e["calls"][3]["args"][0], 3);

            let q = r(
                "cue_exec",
                json!({ "children": false, "created": [true, true, false],
                    "voice": 1, "multiplayer": 0, "clock": [4, 0, 0], "flags": 0,
                    "within": [true, false] }),
            );
            assert_eq!(q["state"]["flags"], 1);
            assert_eq!(q["state"]["children"][1], "s5+0xb0b3c");
            let lines: Vec<_> = q["calls"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|c| c["engine"] == "0x800117ec")
                .map(|c| c["args"][0].clone())
                .collect();
            assert_eq!(lines, vec![json!(1737), json!(1738)]);

            let a = r(
                "child_activate",
                json!({ "mode": 0, "timer": 0, "player": 0x101 }),
            );
            assert_eq!(a["result"], 1);
            assert_eq!(a["state"]["player"], 1);
            assert_eq!(r("child_init", json!(null))["state"]["exec"], "s1+0x951c");
        }
    }
}
