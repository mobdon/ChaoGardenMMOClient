//! A small runner shared by the stage-code ports (Sky Rail and later
//! stages): it reads constants and pointers out of a stage REL through the
//! relocation of the instruction that loads them, checks every engine call
//! against the REL's own import relocation, and collects the ordered
//! engine/module calls and the locations a routine writes.
//!
//! A routine's input is a set of named locations (`"twp+0x14"`, `"t+0x38"`):
//! the task is `t`, its work (`t+0x34`) is `twp`, and any other pointer is a
//! name given by the input (a string) that later keys are built from
//! (`"<name>+0x4"`). Return values of engine calls are given per engine
//! routine name, in call order (`"rand": [..]`). The output is
//! `{ "calls": [..], "state": { location: final value } }`.
use crate::{
    city_rel::{CityRel, Reference},
    manifest, Error, Result,
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};

/// One released stage module: the GameCube disc copy or the Steam
/// `resource/gd_PC` copy (same section bytes, other module id and main
/// program import addresses).
pub struct StageRel<'a> {
    pub(crate) rel: CityRel<'a>,
    /// The GameCube disc module (its imports carry GSNE8P addresses).
    pub(crate) gc: bool,
    name: &'static str,
}

/// A pinned module revision: file length, GC and Steam SHA-256 and module ids.
pub struct Pin {
    pub name: &'static str,
    pub len: usize,
    pub gc_sha256: &'static str,
    pub gc_module: u32,
    pub steam_sha256: &'static str,
    pub steam_module: u32,
}

impl<'a> StageRel<'a> {
    pub fn read(bytes: &'a [u8], pin: &Pin) -> Result<Self> {
        let bad = || Error::invalid(format!("unsupported {} revision", pin.name));
        if bytes.len() != pin.len {
            return Err(bad());
        }
        let sha = manifest::sha256(bytes);
        let (module, gc) = if sha == pin.gc_sha256 {
            (pin.gc_module, true)
        } else if sha == pin.steam_sha256 {
            (pin.steam_module, false)
        } else {
            return Err(bad());
        };
        Ok(Self {
            rel: CityRel::read_identified(bytes, module)?,
            gc,
            name: pin.name,
        })
    }
    pub(crate) fn bad(&self, pc: u32) -> Error {
        Error::invalid(format!("unexpected {} code at +0x{pc:x}", self.name))
    }
    /// The (section, offset) the `addi`/load at `pc` (LO half relocated at
    /// `pc + 2`) points at, in this module.
    pub fn at(&self, pc: u32) -> Result<(u8, u32)> {
        match self.rel.reference(1, pc + 2) {
            Some(Reference {
                module,
                kind: 4,
                section,
                addend,
            }) if module == self.rel.module_id() => Ok((section, addend)),
            _ => Err(self.bad(pc)),
        }
    }
    /// The module address the instruction at `pc` loads, as `"s5+0x1234"`.
    pub fn ptr(&self, pc: u32) -> Result<Value> {
        let (s, o) = self.at(pc)?;
        Ok(json!(format!("s{s}+0x{o:x}")))
    }
    pub fn word(&self, section: u8, at: u32) -> Result<u32> {
        let d = self.rel.data(section)?;
        d.get(at as usize..at as usize + 4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
            .ok_or_else(|| self.bad(at))
    }
    /// The single-precision constant the instruction at `pc` addresses
    /// (.rodata or .data).
    pub fn f(&self, pc: u32) -> Result<f32> {
        let (s, o) = self.at(pc)?;
        if s != 4 && s != 5 {
            return Err(self.bad(pc));
        }
        Ok(f32::from_bits(self.word(s, o)?))
    }
    /// Checks that the main-program location the instruction at `pc` names
    /// (a `bl` at `pc` with `half == false`, or the LO half of a `lis`/`addi`
    /// at `pc + 2`) is `addr` in GSNE8P. The Steam module imports another main
    /// program, so for it only the presence of a main-program import is
    /// checked.
    pub fn dol(&self, pc: u32, addr: u32, half: bool) -> Result<()> {
        let (at, kind) = if half { (pc + 2, 4) } else { (pc, 10) };
        match self.rel.reference(1, at) {
            Some(r) if r.module == 0 && r.kind == kind && (!self.gc || r.addend == addr) => Ok(()),
            _ => Err(self.bad(pc)),
        }
    }
    /// A data word as a JSON value: a module address (`"s5+0x1234"`) when it
    /// carries an ADDR32 relocation to this module, else its value.
    pub fn data_value(&self, section: u8, at: u32) -> Result<Value> {
        Ok(match self.rel.reference(section, at) {
            Some(Reference {
                module,
                kind: 1,
                section: s,
                addend,
            }) if module == self.rel.module_id() => json!(format!("s{s}+0x{addend:x}")),
            Some(_) => return Err(self.bad(at)),
            None => json!(self.word(section, at)?),
        })
    }
    /// The target of the module-local `bl` at `pc`.
    pub fn local(&self, pc: u32) -> Result<u32> {
        let ins = self.word(1, pc)?;
        if ins & 0xfc00_0003 != 0x4800_0001 {
            return Err(self.bad(pc));
        }
        let disp = ((ins & 0x03ff_fffc) as i32) << 6 >> 6;
        pc.checked_add_signed(disp).ok_or_else(|| self.bad(pc))
    }
}

/// `fctiwz`: truncate toward zero, saturating; NaN gives 0x80000000.
pub fn fctiwz(x: f32) -> i32 {
    if x.is_nan() {
        i32::MIN
    } else {
        x as i32
    }
}

/// One routine run: inputs, writes, calls.
pub struct Run<'s, 'a> {
    pub sr: &'s StageRel<'a>,
    mem: BTreeMap<String, Value>,
    out: BTreeMap<String, Value>,
    calls: Vec<Value>,
    rets: BTreeMap<String, VecDeque<Value>>,
    zeroed: Vec<String>,
}

/// `"s5+0x100"` or `"s5+0x100+0x8"`: a module location.
pub fn rel_loc(k: &str) -> Option<(u8, u32)> {
    let rest = k.strip_prefix('s')?;
    let mut parts = rest.split("+0x");
    let s: u8 = parts.next()?.parse().ok()?;
    let mut o = 0u32;
    let mut n = 0;
    for p in parts {
        o = o.checked_add(u32::from_str_radix(p, 16).ok()?)?;
        n += 1;
    }
    (n >= 1).then_some((s, o))
}

pub fn key(base: &str, off: u32) -> String {
    format!("{base}+0x{off:x}")
}

impl<'s, 'a> Run<'s, 'a> {
    /// `input` is `{ "mem": {..}, "rets": {..} }` (both optional).
    pub fn new(sr: &'s StageRel<'a>, input: &Value) -> Result<Self> {
        let obj = |k: &str| -> Result<serde_json::Map<String, Value>> {
            match input.get(k) {
                None => Ok(Default::default()),
                Some(Value::Object(m)) => Ok(m.clone()),
                _ => Err(Error::invalid(format!("stage request: `{k}` must be an object"))),
            }
        };
        let mem = obj("mem")?.into_iter().collect();
        let mut rets = BTreeMap::new();
        for (k, v) in obj("rets")? {
            let Value::Array(a) = v else {
                return Err(Error::invalid("stage request: returns must be arrays"));
            };
            rets.insert(k, a.into_iter().collect());
        }
        Ok(Self {
            sr,
            mem,
            out: BTreeMap::new(),
            calls: Vec::new(),
            rets,
            zeroed: Vec::new(),
        })
    }
    pub fn finish(self) -> Value {
        json!({ "calls": self.calls, "state": self.out })
    }
    /// A location's value: written by this run, given by the input, inside a
    /// block the run allocated with `syCalloc` (zero), or, for a module
    /// address (`"s5+0x1234+0x8"`), the REL word there (a module address when
    /// it carries a relocation to this module).
    pub fn get(&self, k: &str) -> Result<Value> {
        if let Some(v) = self.out.get(k).or_else(|| self.mem.get(k)) {
            return Ok(v.clone());
        }
        if let Some((s, o)) = rel_loc(k) {
            return self.sr.data_value(s, o);
        }
        if self.zeroed.iter().any(|b| k.starts_with(&format!("{b}+"))) {
            return Ok(json!(0));
        }
        Err(Error::invalid(format!("stage request lacks `{k}`")))
    }
    pub fn f(&self, k: &str) -> Result<f32> {
        if !self.out.contains_key(k) && !self.mem.contains_key(k) {
            if let Some((s, o)) = rel_loc(k) {
                return Ok(f32::from_bits(self.sr.word(s, o)?));
            }
        }
        self.get(k)?
            .as_f64()
            .map(|v| v as f32)
            .ok_or_else(|| Error::invalid(format!("`{k}` must be a number")))
    }
    pub fn w(&self, k: &str) -> Result<u32> {
        Ok(self.i(k)? as u32)
    }
    pub fn b(&self, k: &str) -> Result<u8> {
        Ok(self.i(k)? as u8)
    }
    pub fn sb(&self, k: &str) -> Result<i8> {
        Ok(self.i(k)? as i8)
    }
    pub fn h(&self, k: &str) -> Result<u16> {
        Ok(self.i(k)? as u16)
    }
    pub fn sh(&self, k: &str) -> Result<i16> {
        Ok(self.i(k)? as i16)
    }
    /// The word is non-zero (a pointer name counts as non-zero).
    pub fn nz(&self, k: &str) -> Result<bool> {
        Ok(match self.get(k)? {
            Value::String(_) => true,
            v => v.as_i64().ok_or_else(|| Error::invalid(format!("`{k}` must be a word")))? != 0,
        })
    }
    /// Records that `name` is a fresh zero-filled block.
    pub fn calloc(&mut self, name: &str) {
        self.zeroed.push(name.to_string());
    }
    /// A double-precision constant the instruction at `pc` addresses.
    pub fn cd(&self, pc: u32) -> Result<f64> {
        let (s, o) = self.sr.at(pc)?;
        let hi = self.sr.word(s, o)? as u64;
        let lo = self.sr.word(s, o + 4)? as u64;
        Ok(f64::from_bits(hi << 32 | lo))
    }
    /// The unsigned int -> float conversion (magic 2^52 at `pc`).
    pub fn cvtu(&self, pc: u32, v: u32) -> Result<f32> {
        let (s, o) = self.sr.at(pc)?;
        if (self.sr.word(s, o)?, self.sr.word(s, o + 4)?) != (0x4330_0000, 0) {
            return Err(self.sr.bad(pc));
        }
        Ok(v as f64 as f32)
    }
    /// GSNE8P game state 0x801CC168 (checked at the `addi` at `pc`) + `off`.
    pub fn gs(&self, pc: u32, off: u32) -> Result<String> {
        self.d(pc, 0x801c_c168)?;
        Ok(key("0x801cc168", off))
    }
    /// A float vector at `base + off` (three words).
    pub fn v3(&self, base: &str, off: u32) -> Result<[f32; 3]> {
        Ok([
            self.f(&key(base, off))?,
            self.f(&key(base, off + 4))?,
            self.f(&key(base, off + 8))?,
        ])
    }
    pub fn set3(&mut self, base: &str, off: u32, v: [f32; 3]) {
        for (k, x) in v.into_iter().enumerate() {
            self.setf(key(base, off + 4 * k as u32), x);
        }
    }
    /// A scripted `[x, y, z]` result of engine routine `name` (njCalcPoint…).
    pub fn ret3(&mut self, name: &str) -> Result<[f32; 3]> {
        let v = self.ret(name)?;
        let a = v.as_array().filter(|a| a.len() == 3);
        let f = |i: usize| a.and_then(|a| a[i].as_f64()).map(|x| x as f32);
        match (f(0), f(1), f(2)) {
            (Some(x), Some(y), Some(z)) => Ok([x, y, z]),
            _ => Err(Error::invalid(format!("`{name}` returns [x, y, z]"))),
        }
    }
    pub fn i(&self, k: &str) -> Result<i64> {
        self.get(k)?
            .as_i64()
            .ok_or_else(|| Error::invalid(format!("`{k}` must be an integer")))
    }
    /// A pointer location: `None` when it holds 0, else the name it holds.
    pub fn p(&self, k: &str) -> Result<Option<String>> {
        match self.get(k)? {
            Value::String(s) => Ok(Some(s)),
            v if v.as_i64() == Some(0) => Ok(None),
            _ => Err(Error::invalid(format!("`{k}` must be 0 or a pointer name"))),
        }
    }
    /// The location `off` bytes into what the pointer location `k` points at.
    pub fn via(&self, k: &str, off: u32) -> Result<String> {
        let p = self
            .p(k)?
            .ok_or_else(|| Error::invalid(format!("`{k}` is null where the routine reads through it")))?;
        Ok(key(&p, off))
    }
    pub fn set(&mut self, k: impl Into<String>, v: impl Into<Value>) {
        self.out.insert(k.into(), v.into());
    }
    pub fn setf(&mut self, k: impl Into<String>, v: f32) {
        self.out.insert(k.into(), json!(v));
    }
    /// The next return value scripted for engine routine `name`.
    pub fn ret(&mut self, name: &str) -> Result<Value> {
        self.rets
            .get_mut(name)
            .and_then(VecDeque::pop_front)
            .ok_or_else(|| Error::invalid(format!("stage request lacks a return for `{name}`")))
    }
    pub fn ret_i(&mut self, name: &str) -> Result<i64> {
        self.ret(name)?
            .as_i64()
            .ok_or_else(|| Error::invalid(format!("`{name}` returns an integer")))
    }
    /// A call to GSNE8P main.dol routine `f` from the `bl` at `pc`.
    pub fn e(&mut self, pc: u32, f: dol::F, args: Value) -> Result<()> {
        let (addr, name) = f;
        self.sr.dol(pc, addr, false)?;
        self.calls
            .push(json!({ "engine": format!("0x{addr:08x}"), "name": name, "args": args }));
        Ok(())
    }
    /// An engine call whose integer return the routine uses.
    pub fn er(&mut self, pc: u32, f: dol::F, args: Value) -> Result<i64> {
        self.e(pc, f, args)?;
        self.ret_i(f.1)
    }
    /// An engine call returning a pointer (a name, or 0). A returned block
    /// from `syCalloc` reads as zero.
    pub fn ep(&mut self, pc: u32, f: dol::F, args: Value) -> Result<Option<String>> {
        self.e(pc, f, args)?;
        match self.ret(f.1)? {
            Value::String(s) => {
                if f == dol::SY_CALLOC {
                    self.calloc(&s);
                }
                Ok(Some(s))
            }
            v if v.as_i64() == Some(0) => Ok(None),
            _ => Err(Error::invalid(format!("`{}` returns 0 or a pointer name", f.1))),
        }
    }
    /// An engine call returning a float.
    pub fn ef(&mut self, pc: u32, f: dol::F, args: Value) -> Result<f32> {
        self.e(pc, f, args)?;
        self.ret(f.1)?
            .as_f64()
            .map(|v| v as f32)
            .ok_or_else(|| Error::invalid(format!("`{}` returns a number", f.1)))
    }
    /// An engine call returning a vector (njCalcPoint, njCalcVector).
    pub fn e3(&mut self, pc: u32, f: dol::F, args: Value) -> Result<[f32; 3]> {
        self.e(pc, f, args)?;
        self.ret3(f.1)
    }
    /// A call to the module routine the local `bl` at `pc` targets.
    pub fn m(&mut self, pc: u32, args: Value) -> Result<()> {
        let to = self.sr.local(pc)?;
        self.calls
            .push(json!({ "module": format!("s1+0x{to:x}"), "args": args }));
        Ok(())
    }
    /// A module call whose integer return the routine uses (named by offset).
    pub fn mr(&mut self, pc: u32, args: Value) -> Result<i64> {
        let to = self.sr.local(pc)?;
        self.m(pc, args)?;
        self.ret_i(&format!("s1+0x{to:x}"))
    }
    /// `rand()` at `pc` converted to single precision the compiler's way
    /// (xoris/0x4330 double minus the constant the `addi` at `pc + 8` names,
    /// which must be 2^52 + 2^31).
    pub fn rnd(&mut self, pc: u32) -> Result<f32> {
        let r = self.er(pc, dol::RAND, json!([]))?;
        self.cvt(pc + 8, r)
    }
    /// The signed int -> float conversion with the magic double at `pc`.
    pub fn cvt(&self, pc: u32, v: i64) -> Result<f32> {
        let (s, o) = self.sr.at(pc)?;
        let hi = self.sr.word(s, o)?;
        let lo = self.sr.word(s, o + 4)?;
        if (hi, lo) != (0x4330_0000, 0x8000_0000) {
            return Err(self.sr.bad(pc));
        }
        Ok(v as i32 as f64 as f32)
    }
    /// Constant (section 4/5 float) the instruction at `pc` loads.
    pub fn c(&self, pc: u32) -> Result<f32> {
        self.sr.f(pc)
    }
    pub fn ptr(&self, pc: u32) -> Result<Value> {
        self.sr.ptr(pc)
    }
    /// Checks a GSNE8P data address loaded by `lis`/`addi` at `pc`.
    pub fn d(&self, pc: u32, addr: u32) -> Result<()> {
        self.sr.dol(pc, addr, true)
    }
}

pub mod sky_rail;

/// GSNE8P main.dol routines the stage modules call (address, name). The
/// runner checks each against the GC module's import relocation.
pub mod dol {
    pub type F = (u32, &'static str);
    pub const ADD_SCORE: F = (0x8006378c, "AddScore");
    pub const BGM_SET_VOLUME: F = (0x80011a04, "BGM_SetVolume");
    pub const CCL_ENTRY: F = (0x8000f4ec, "CCL_Entry");
    pub const CCL_INIT: F = (0x8000f8b8, "CCL_Init");
    pub const CHECK_RANGE_OUT: F = (0x800655f4, "CheckRangeOut");
    pub const CREATE_ELEMENTAL_TASK: F = (0x80071e0c, "CreateElementalTask");
    pub const DEAD_OUT: F = (0x800653b0, "DeadOut");
    pub const DESTROY_TASK: F = (0x80071af0, "DestroyTask");
    pub const FREE_TASK: F = (0x80071ae0, "FreeTask");
    pub const IS_THIS_TASK_PLAYER: F = (0x80037edc, "IsThisTaskPlayer");
    pub const OFF_CONTROL_3D: F = (0x8002b2d8, "OffControl3D");
    pub const ON_CONTROL_3D: F = (0x8002b2e8, "OnControl3D");
    pub const SE_CALL: F = (0x8006b978, "SE_Call");
    pub const SE_CALL_V2_TIMER: F = (0x8006affc, "SE_CallV2_Timer");
    pub const SET_VELOCITY_P: F = (0x8003a3dc, "SetVelocityP");
    pub const STG_PLAY_MUSIC: F = (0x80011050, "StgPlayMusic");
    pub const CVT_FP2UNSIGNED: F = (0x8010af3c, "__cvt_fp2unsigned");
    pub const NJ_COLOR_BLENDING_MODE: F = (0x801179f8, "__njColorBlendingMode");
    pub const EITHER_PLAYER_WITHIN_SPHERE: F = (0x800398d8, "_rename_EitherPlayerWithinSphere");
    pub const ATAN2F: F = (0x80111168, "atan2f");
    pub const DS_DRAW_MODEL_CLIP: F = (0x8011f514, "ds_DrawModelClip");
    pub const FN_800068E4: F = (0x800068e4, "fn_800068E4");
    pub const FN_80011DF4: F = (0x80011df4, "fn_80011DF4");
    pub const FN_800168C8: F = (0x800168c8, "fn_800168C8");
    pub const FN_8001A2B0: F = (0x8001a2b0, "fn_8001A2B0");
    pub const FN_8001A8A8: F = (0x8001a8a8, "fn_8001A8A8");
    pub const FN_8001CD40: F = (0x8001cd40, "fn_8001CD40");
    pub const FN_800231CC: F = (0x800231cc, "fn_800231CC");
    pub const FN_800232A4: F = (0x800232a4, "fn_800232A4");
    pub const FN_800245B8: F = (0x800245b8, "fn_800245B8");
    pub const FN_800245EC: F = (0x800245ec, "fn_800245EC");
    pub const FN_80025028: F = (0x80025028, "fn_80025028");
    pub const FN_8002503C: F = (0x8002503c, "fn_8002503C");
    pub const FN_80025048: F = (0x80025048, "fn_80025048");
    pub const FN_8002506C: F = (0x8002506c, "fn_8002506C");
    pub const FN_8002FB2C: F = (0x8002fb2c, "fn_8002FB2C");
    pub const FN_800324E4: F = (0x800324e4, "fn_800324E4");
    pub const FN_80033620: F = (0x80033620, "fn_80033620");
    pub const FN_800336DC: F = (0x800336dc, "fn_800336DC");
    pub const FN_80033710: F = (0x80033710, "fn_80033710");
    pub const FN_8003699C: F = (0x8003699c, "fn_8003699C");
    pub const FN_800399BC: F = (0x800399bc, "fn_800399BC");
    pub const FN_8003BCC4: F = (0x8003bcc4, "fn_8003BCC4");
    pub const FN_8003D7D4: F = (0x8003d7d4, "fn_8003D7D4");
    pub const FN_8005523C: F = (0x8005523c, "fn_8005523C");
    pub const FN_8005D0EC: F = (0x8005d0ec, "fn_8005D0EC");
    pub const FN_80065048: F = (0x80065048, "fn_80065048");
    pub const FN_80066364: F = (0x80066364, "fn_80066364");
    pub const FN_8006638C: F = (0x8006638c, "fn_8006638C");
    pub const FN_8006D500: F = (0x8006d500, "fn_8006D500");
    pub const FN_80072118: F = (0x80072118, "fn_80072118");
    pub const FN_80072218: F = (0x80072218, "fn_80072218");
    pub const FN_80072270: F = (0x80072270, "fn_80072270");
    pub const FN_80072B2C: F = (0x80072b2c, "fn_80072B2C");
    pub const FN_800C3CD8: F = (0x800c3cd8, "fn_800C3CD8");
    pub const FN_8011E150: F = (0x8011e150, "fn_8011E150");
    pub const FN_8011E154: F = (0x8011e154, "fn_8011E154");
    pub const FN_8011E158: F = (0x8011e158, "fn_8011E158");
    pub const GJ_DRAW_MODEL: F = (0x80121bdc, "gjDrawModel");
    pub const GJ_SET_FOG: F = (0x8011c310, "gjSetFog");
    pub const LATE_DRAW_MODEL: F = (0x80031a28, "late_DrawModel");
    pub const NJ_CALC_POINT: F = (0x80114f0c, "njCalcPoint");
    pub const NJ_CALC_VECTOR: F = (0x801154ec, "njCalcVector");
    pub const NJ_DISABLE_FOG: F = (0x8011c2bc, "njDisableFog");
    pub const NJ_DISTANCE_P2P: F = (0x8011b65c, "njDistanceP2P");
    pub const NJ_ENABLE_FOG: F = (0x8011c2b0, "njEnableFog");
    pub const NJ_POP_MATRIX: F = (0x80115dc8, "njPopMatrix");
    pub const NJ_POP_MATRIX_EX: F = (0x801160e4, "njPopMatrixEx");
    pub const NJ_PUSH_MATRIX: F = (0x80115d60, "njPushMatrix");
    pub const NJ_PUSH_MATRIX_EX: F = (0x80116098, "njPushMatrixEx");
    pub const NJ_ROTATE_X: F = (0x80115c60, "njRotateX");
    pub const NJ_ROTATE_Y: F = (0x80115828, "njRotateY");
    pub const NJ_ROTATE_Z: F = (0x801158a8, "njRotateZ");
    pub const NJ_SCALE: F = (0x80115178, "njScale");
    pub const NJ_SCALOR: F = (0x801151f4, "njScalor");
    pub const NJ_SET_TEXTURE: F = (0x80116e18, "njSetTexture");
    pub const NJ_SIN: F = (0x80112398, "njSin");
    pub const NJ_TRANSLATE: F = (0x80115bd8, "njTranslate");
    pub const NJ_TRANSLATE_EX: F = (0x8011616c, "njTranslateEx");
    pub const NJ_TRANSLATE_V: F = (0x8011538c, "njTranslateV");
    pub const NJ_UNIT_MATRIX: F = (0x801153e4, "njUnitMatrix");
    pub const RAND: F = (0x8010dc88, "rand");
    pub const SY_CALLOC: F = (0x801197fc, "syCalloc");
    pub const SY_FREE: F = (0x80119828, "syFree");
}
