//! Stage-specific GameCube REL code ported straight from the GSNE8P disc
//! modules (one submodule per stage). Every routine returns the ordered
//! engine (`0x80…` GSNE8P main.dol address) and module (`s1+0x…` section-1
//! offset) calls it makes, plus the final value of every location it writes.
//!
//! Engine results are inputs: [`Returns`] holds, per called address, the
//! values that address returns, consumed in call order. Constants and
//! pointers are read from the module through the relocation of the
//! instruction that loads them, never typed in. When the module is the GC
//! disc copy, every engine address a routine reports is also checked
//! against the import relocation of the `bl` (or `lis`/`addi`) that names
//! it; the Steam `resource/gd_PC` copies have the same section bytes but
//! link another main program, so their imports are not compared.
use crate::{
    city_rel::{CityRel, Reference},
    manifest, Error, Result,
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};

pub mod green_forest;
pub mod sonic_vs_shadow1;

/// The two released copies of one stage module.
pub struct Ident {
    pub name: &'static str,
    pub len: usize,
    pub gc_sha256: &'static str,
    pub gc_module: u32,
    pub steam_sha256: &'static str,
    pub steam_module: u32,
}

pub struct StageRel<'a> {
    rel: CityRel<'a>,
    gc: bool,
    name: &'static str,
}

fn bad(name: &str, pc: u32) -> Error {
    Error::invalid(format!("unexpected {name} code at +0x{pc:x}"))
}

impl<'a> StageRel<'a> {
    /// Accepts the GC disc REL or the Steam copy, by SHA-256.
    pub fn read(bytes: &'a [u8], id: &Ident) -> Result<Self> {
        let unsupported = || Error::invalid(format!("unsupported {} REL revision", id.name));
        if bytes.len() != id.len {
            return Err(unsupported());
        }
        let sha = manifest::sha256(bytes);
        let (module, gc) = if sha == id.gc_sha256 {
            (id.gc_module, true)
        } else if sha == id.steam_sha256 {
            (id.steam_module, false)
        } else {
            return Err(unsupported());
        };
        Ok(Self {
            rel: CityRel::read_identified(bytes, module)?,
            gc,
            name: id.name,
        })
    }
    pub fn is_gc(&self) -> bool {
        self.gc
    }
    /// The section offset the `addi`/`lfs` at `pc` (the LO half of a
    /// `lis`/`addi` pair, relocated at `pc + 2`) points at.
    pub fn at(&self, pc: u32, section: u8) -> Result<u32> {
        match self.rel.reference(1, pc + 2) {
            Some(Reference {
                module,
                kind: 4,
                section: s,
                addend,
            }) if s == section && module == self.rel.module_id() => Ok(addend),
            _ => Err(bad(self.name, pc)),
        }
    }
    pub fn ptr(&self, pc: u32, section: u8) -> Result<Value> {
        Ok(sec(section, self.at(pc, section)?))
    }
    pub fn word(&self, section: u8, at: u32) -> Result<u32> {
        let d = self.rel.data(section)?;
        d.get(at as usize..at as usize + 4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
            .ok_or_else(|| bad(self.name, at))
    }
    /// The halfword at `section + at`.
    pub fn half(&self, section: u8, at: u32) -> Result<u16> {
        let d = self.rel.data(section)?;
        d.get(at as usize..at as usize + 2)
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
            .ok_or_else(|| bad(self.name, at))
    }
    /// The single-precision float at `section + at`.
    pub fn float(&self, section: u8, at: u32) -> Result<f32> {
        Ok(f32::from_bits(self.word(section, at)?))
    }
    /// The .rodata float the `addi` at `pc` addresses.
    pub fn f(&self, pc: u32) -> Result<f32> {
        self.float(4, self.at(pc, 4)?)
    }
    /// The .rodata double the `addi` at `pc` addresses.
    pub fn fd(&self, pc: u32) -> Result<f64> {
        let at = self.at(pc, 4)?;
        let hi = u64::from(self.word(4, at)?);
        Ok(f64::from_bits(hi << 32 | u64::from(self.word(4, at + 4)?)))
    }
    /// The module's own pointer stored in the data word at `section + at`.
    pub fn pointer(&self, section: u8, at: u32) -> Result<(u8, u32)> {
        self.rel
            .local_pointer(section, at)?
            .ok_or_else(|| bad(self.name, at))
    }
    /// The target of the module-local `bl` at `pc` (resolved at link time).
    pub fn local(&self, pc: u32) -> Result<u32> {
        let ins = self.word(1, pc)?;
        if ins & 0xfc00_0003 != 0x4800_0001 || self.rel.reference(1, pc).is_some() {
            return Err(bad(self.name, pc));
        }
        let disp = ((ins & 0x03ff_fffc) as i32) << 6 >> 6;
        pc.checked_add_signed(disp)
            .ok_or_else(|| bad(self.name, pc))
    }
    /// `addr`, after checking (GC copy only) that the `bl` at `pc`, or the
    /// `addi` relocated at `pc + 2`, imports exactly that GSNE8P address.
    pub fn dol(&self, pc: u32, addr: u32) -> Result<u32> {
        if !self.gc {
            return Ok(addr);
        }
        let hit = |at: u32, kind: u8| {
            matches!(self.rel.reference(1, at), Some(Reference { module: 0, kind: k, addend, .. })
                if k == kind && addend == addr)
        };
        if hit(pc, 10) || hit(pc + 2, 4) {
            Ok(addr)
        } else {
            Err(Error::invalid(format!(
                "{}: +0x{pc:x} does not import 0x{addr:08x}",
                self.name
            )))
        }
    }
}

/// `"s<section>+0x<offset>"`, one of the module's own addresses.
pub fn sec(section: u8, offset: u32) -> Value {
    json!(format!("s{section}+0x{offset:x}"))
}

/// Return values by called address (`"0x8011b65c"`, or `"s1+0x138"` for a
/// module helper whose result the routine uses), consumed in call order.
pub type Returns = BTreeMap<String, VecDeque<Value>>;

/// One routine's run: the calls it made, in order.
pub struct Run<'r, 'a> {
    pub rel: &'r StageRel<'a>,
    pub calls: Vec<Value>,
    returns: Returns,
}

impl<'r, 'a> Run<'r, 'a> {
    pub fn new(rel: &'r StageRel<'a>, returns: Returns) -> Self {
        Self {
            rel,
            calls: Vec::new(),
            returns,
        }
    }
    fn take(&mut self, key: &str) -> Result<f64> {
        self.returns
            .get_mut(key)
            .and_then(VecDeque::pop_front)
            .and_then(|v| v.as_f64())
            .ok_or_else(|| Error::invalid(format!("missing return for {key}")))
    }
    /// The engine routine `addr` called by the `bl` at `pc`.
    pub fn call(&mut self, pc: u32, addr: u32, args: Value) -> Result<()> {
        let a = self.rel.dol(pc, addr)?;
        self.calls
            .push(json!({ "engine": format!("0x{a:08x}"), "args": args }));
        Ok(())
    }
    /// An engine call whose result the routine uses.
    pub fn query(&mut self, pc: u32, addr: u32, args: Value) -> Result<f64> {
        let a = self.rel.dol(pc, addr)?;
        let v = self.take(&format!("0x{a:08x}"))?;
        self.calls
            .push(json!({ "engine": format!("0x{a:08x}"), "args": args, "returned": v }));
        Ok(v)
    }
    /// The module routine the local `bl` at `pc` calls (not executed here).
    pub fn module(&mut self, pc: u32, args: Value) -> Result<()> {
        let t = self.rel.local(pc)?;
        self.calls
            .push(json!({ "module": format!("s1+0x{t:x}"), "args": args }));
        Ok(())
    }
    /// A module helper whose result the routine uses.
    pub fn module_query(&mut self, pc: u32, args: Value) -> Result<f64> {
        let key = format!("s1+0x{:x}", self.rel.local(pc)?);
        let v = self.take(&key)?;
        self.calls
            .push(json!({ "module": key, "args": args, "returned": v }));
        Ok(v)
    }
}

/// `fctiwz`: truncate toward zero, saturating; NaN gives 0x80000000.
pub fn fctiwz(x: f64) -> i32 {
    if x.is_nan() {
        i32::MIN
    } else {
        x as i32
    }
}
