//! Runtime the recompiled code links against: Gekko register file, big-endian
//! memory and the call table. Game code calls every function through
//! [`call`], so a hand port registered with [`Runtime::set_override`] replaces
//! the recompiled function at that GSNE8P address.

use std::collections::HashMap;

pub type Func = fn(&mut Cpu, &mut Mem);

/// One floating-point register: Gekko keeps two slots (paired singles).
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Fpr {
    pub ps0: f64,
    pub ps1: f64,
}

pub struct Cpu {
    pub r: [u32; 32],
    pub f: [Fpr; 32],
    pub cr: u32,
    pub lr: u32,
    pub ctr: u32,
    pub ca: bool,
    pub so: bool,
    pub fpscr: u32,
    pub msr: u32,
    pub gqr: [u32; 8],
    pub spr: Box<[u32; 1024]>,
    pub tb: u64,
    pub rt: Runtime,
}

impl Default for Cpu {
    fn default() -> Self {
        Self {
            r: [0; 32],
            f: [Fpr::default(); 32],
            cr: 0,
            lr: 0,
            ctr: 0,
            ca: false,
            so: false,
            fpscr: 0,
            msr: 0,
            gqr: [0; 8],
            spr: Box::new([0; 1024]),
            tb: 0,
            rt: Runtime::default(),
        }
    }
}

/// Address resolution for calls. `generated` is the recompiled dispatch
/// (address -> function); overrides are checked first.
pub struct Runtime {
    pub overrides: HashMap<u32, Func>,
    pub generated: fn(u32) -> Option<Func>,
    /// Recompiled REL modules: module id -> dispatch(section, offset).
    pub rels: HashMap<u32, fn(u32, u32) -> Option<Func>>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self { overrides: HashMap::new(), generated: |_| None, rels: HashMap::new() }
    }
}

impl Runtime {
    pub fn set_override(&mut self, addr: u32, f: Func) {
        self.overrides.insert(addr, f);
    }
    pub fn resolve(&self, m: &Mem, addr: u32) -> Option<Func> {
        if let Some(f) = self.overrides.get(&addr) {
            return Some(*f);
        }
        if let Some(f) = (self.generated)(addr) {
            return Some(f);
        }
        // Code inside a loaded REL: find the module and section that hold it.
        let mut info = m.r32(OS_MODULE_HEAD);
        while info != 0 {
            let id = m.r32(info);
            let n = m.r32(info + 0xC);
            let tab = m.r32(info + 0x10);
            for sec in 0..n {
                let off = m.r32(tab + sec * 8);
                let size = m.r32(tab + sec * 8 + 4);
                let base = off & !1;
                if off & 1 != 0 && (base..base + size).contains(&addr) {
                    return self.rels.get(&id).and_then(|d| d(sec, addr - base));
                }
            }
            info = m.r32(info + 4);
        }
        None
    }
}

/// Call the function at `addr` (bl, bctrl, blrl and tail calls all end here).
pub fn call(c: &mut Cpu, m: &mut Mem, addr: u32) {
    match c.rt.resolve(m, addr) {
        Some(f) => f(c, m),
        None => panic!("sa2-recomp: no function at {addr:#010x} (lr {:#010x})", c.lr),
    }
}

/// OS module queue head (OSModuleInfo list linked by OSLink).
pub const OS_MODULE_HEAD: u32 = 0x8000_30C8;

/// Run-time load address of a REL section (after the game's own OSLink).
pub fn sec(m: &Mem, id: u32, sec: u32) -> u32 {
    let mut info = m.r32(OS_MODULE_HEAD);
    while info != 0 {
        if m.r32(info) == id {
            return m.r32(m.r32(info + 0x10) + sec * 8) & !1;
        }
        info = m.r32(info + 4);
    }
    panic!("sa2-recomp: module {id} is not linked");
}

impl Cpu {
    #[inline]
    pub fn bit(&self, n: u32) -> bool {
        self.cr >> (31 - n) & 1 != 0
    }
    #[inline]
    pub fn set_bit(&mut self, n: u32, v: bool) {
        let m = 1u32 << (31 - n);
        if v {
            self.cr |= m
        } else {
            self.cr &= !m
        }
    }
    #[inline]
    pub fn set_field(&mut self, f: u32, v: u32) {
        let sh = 28 - 4 * f;
        self.cr = (self.cr & !(0xF << sh)) | ((v & 0xF) << sh);
    }
    #[inline]
    pub fn field(&self, f: u32) -> u32 {
        self.cr >> (28 - 4 * f) & 0xF
    }
    #[inline]
    pub fn cmp_s(&mut self, f: u32, a: i32, b: i32) {
        let v = if a < b { 8 } else if a > b { 4 } else { 2 };
        self.set_field(f, v | self.so as u32);
    }
    #[inline]
    pub fn cmp_u(&mut self, f: u32, a: u32, b: u32) {
        let v = if a < b { 8 } else if a > b { 4 } else { 2 };
        self.set_field(f, v | self.so as u32);
    }
    #[inline]
    pub fn cmp_f(&mut self, f: u32, a: f64, b: f64) {
        let v = if a.is_nan() || b.is_nan() {
            1
        } else if a < b {
            8
        } else if a > b {
            4
        } else {
            2
        };
        self.set_field(f, v);
    }
    /// Record form (Rc=1) of an integer op: CR0 from the result.
    #[inline]
    pub fn rec(&mut self, v: u32) {
        self.cmp_s(0, v as i32, 0);
    }
    pub fn mfspr(&mut self, n: u32) -> u32 {
        match n {
            1 => (self.so as u32) << 31 | (self.ca as u32) << 29,
            8 => self.lr,
            9 => self.ctr,
            268 | 284 => self.tb as u32,
            269 | 285 => (self.tb >> 32) as u32,
            912..=919 => self.gqr[(n - 912) as usize],
            _ => self.spr[n as usize & 1023],
        }
    }
    pub fn mtspr(&mut self, n: u32, v: u32) {
        match n {
            1 => {
                self.so = v >> 31 & 1 != 0;
                self.ca = v >> 29 & 1 != 0
            }
            8 => self.lr = v,
            9 => self.ctr = v,
            284 => self.tb = (self.tb & !0xFFFF_FFFF) | v as u64,
            285 => self.tb = (self.tb & 0xFFFF_FFFF) | (v as u64) << 32,
            912..=919 => self.gqr[(n - 912) as usize] = v,
            _ => self.spr[n as usize & 1023] = v,
        }
    }
    pub fn mtcrf(&mut self, mask: u32, v: u32) {
        for f in 0..8 {
            if mask >> (7 - f) & 1 != 0 {
                self.set_field(f, v >> (28 - 4 * f));
            }
        }
    }
}

// ---- integer helpers ----

#[inline]
pub fn adde(a: u32, b: u32, cin: bool) -> (u32, bool) {
    let s = a as u64 + b as u64 + cin as u64;
    (s as u32, s > 0xFFFF_FFFF)
}
#[inline]
pub fn sraw(v: u32, sh: u32) -> (u32, bool) {
    let s = v as i32;
    if sh & 0x20 != 0 {
        return (if s < 0 { u32::MAX } else { 0 }, s < 0);
    }
    let sh = sh & 31;
    (( s >> sh) as u32, s < 0 && sh != 0 && (v << (32 - sh)) != 0)
}
#[inline]
pub fn divw(a: u32, b: u32) -> u32 {
    let (a, b) = (a as i32, b as i32);
    if b == 0 || (a == i32::MIN && b == -1) {
        if a < 0 { u32::MAX } else { 0 }
    } else {
        (a / b) as u32
    }
}
#[inline]
pub fn divwu(a: u32, b: u32) -> u32 {
    if b == 0 { 0 } else { a / b }
}

// ---- float helpers ----

#[inline]
pub fn rs(v: f64) -> f64 {
    v as f32 as f64
}
#[inline]
pub fn fctiw(v: f64, trunc: bool) -> f64 {
    let t = if trunc { v.trunc() } else { v.round_ties_even() };
    let i = if v.is_nan() {
        i32::MIN
    } else if t >= 2147483647.0 {
        i32::MAX
    } else if t <= -2147483648.0 {
        i32::MIN
    } else {
        t as i32
    };
    f64::from_bits(0xFFF8_0000_0000_0000 | i as u32 as u64)
}
/// Reciprocal estimates. Exact values, matching the crate's fallback when the
/// local Gekko tables are absent.
#[inline]
pub fn fres(v: f64) -> f64 {
    rs(1.0 / v)
}
#[inline]
pub fn frsqrte(v: f64) -> f64 {
    1.0 / v.sqrt()
}
#[inline]
pub fn fsel(a: f64, b: f64, c: f64) -> f64 {
    if a >= 0.0 { b } else { c }
}

// ---- memory ----

pub const MEM1: u32 = 0x0180_0000;
pub const LC: u32 = 0x4000;

/// Gekko physical memory: 24 MB MEM1 (seen at 0x80000000 and 0xC0000000)
/// and the 16 KB locked cache at 0xE0000000. Hardware registers go to `io`.
pub struct Mem {
    pub ram: Vec<u8>,
    pub lc: Vec<u8>,
    pub io_read: fn(u32, u32) -> u32,
    pub io_write: fn(u32, u32, u32),
}

impl Default for Mem {
    fn default() -> Self {
        Self {
            ram: vec![0; MEM1 as usize],
            lc: vec![0; LC as usize],
            io_read: |a, _| panic!("sa2-recomp: unhandled hardware read {a:#010x}"),
            io_write: |a, _, _| panic!("sa2-recomp: unhandled hardware write {a:#010x}"),
        }
    }
}

enum Loc {
    Ram(usize),
    Lc(usize),
    Io,
}

impl Mem {
    #[inline]
    fn loc(&self, a: u32) -> Loc {
        match a >> 28 {
            0x8 | 0xC if a & 0x0FFF_FFFF < MEM1 => Loc::Ram((a & 0x0FFF_FFFF) as usize),
            0xE if a & 0x0FFF_FFFF < LC => Loc::Lc((a & 0x0FFF_FFFF) as usize),
            0x0 if a < MEM1 => Loc::Ram(a as usize),
            _ => Loc::Io,
        }
    }
    #[inline]
    fn rd<const N: usize>(&self, a: u32) -> Option<[u8; N]> {
        match self.loc(a) {
            Loc::Ram(i) => self.ram.get(i..i + N).map(|s| s.try_into().unwrap()),
            Loc::Lc(i) => self.lc.get(i..i + N).map(|s| s.try_into().unwrap()),
            Loc::Io => None,
        }
    }
    #[inline]
    fn wr(&mut self, a: u32, b: &[u8]) -> bool {
        let n = b.len();
        let s = match self.loc(a) {
            Loc::Ram(i) => self.ram.get_mut(i..i + n),
            Loc::Lc(i) => self.lc.get_mut(i..i + n),
            Loc::Io => None,
        };
        match s {
            Some(s) => {
                s.copy_from_slice(b);
                true
            }
            None => false,
        }
    }
    #[inline]
    pub fn r8(&self, a: u32) -> u8 {
        self.rd::<1>(a).map(|b| b[0]).unwrap_or_else(|| (self.io_read)(a, 1) as u8)
    }
    #[inline]
    pub fn r16(&self, a: u32) -> u16 {
        self.rd::<2>(a).map(u16::from_be_bytes).unwrap_or_else(|| (self.io_read)(a, 2) as u16)
    }
    #[inline]
    pub fn r32(&self, a: u32) -> u32 {
        self.rd::<4>(a).map(u32::from_be_bytes).unwrap_or_else(|| (self.io_read)(a, 4))
    }
    #[inline]
    pub fn r64(&self, a: u32) -> u64 {
        (self.r32(a) as u64) << 32 | self.r32(a.wrapping_add(4)) as u64
    }
    #[inline]
    pub fn w8(&mut self, a: u32, v: u8) {
        if !self.wr(a, &[v]) {
            (self.io_write)(a, 1, v as u32)
        }
    }
    #[inline]
    pub fn w16(&mut self, a: u32, v: u16) {
        if !self.wr(a, &v.to_be_bytes()) {
            (self.io_write)(a, 2, v as u32)
        }
    }
    #[inline]
    pub fn w32(&mut self, a: u32, v: u32) {
        if !self.wr(a, &v.to_be_bytes()) {
            (self.io_write)(a, 4, v)
        }
    }
    #[inline]
    pub fn w64(&mut self, a: u32, v: u64) {
        self.w32(a, (v >> 32) as u32);
        self.w32(a.wrapping_add(4), v as u32);
    }
    #[inline]
    pub fn rf32(&self, a: u32) -> f64 {
        f32::from_bits(self.r32(a)) as f64
    }
    #[inline]
    pub fn rf64(&self, a: u32) -> f64 {
        f64::from_bits(self.r64(a))
    }
    #[inline]
    pub fn wf32(&mut self, a: u32, v: f64) {
        self.w32(a, (v as f32).to_bits())
    }
    #[inline]
    pub fn wf64(&mut self, a: u32, v: f64) {
        self.w64(a, v.to_bits())
    }
    pub fn dcbz(&mut self, a: u32) {
        let a = a & !31;
        for i in 0..8 {
            self.w32(a + i * 4, 0);
        }
    }
    pub fn load(&mut self, addr: u32, bytes: &[u8]) {
        assert!(self.wr(addr, bytes) || bytes.is_empty(), "load outside RAM at {addr:#x}");
    }
}

pub fn lmw(c: &mut Cpu, m: &Mem, rd: usize, ea: u32) {
    for (i, r) in (rd..32).enumerate() {
        c.r[r] = m.r32(ea + 4 * i as u32);
    }
}
pub fn stmw(c: &Cpu, m: &mut Mem, rs: usize, ea: u32) {
    for (i, r) in (rs..32).enumerate() {
        m.w32(ea + 4 * i as u32, c.r[r]);
    }
}
pub fn lswi(c: &mut Cpu, m: &Mem, rd: usize, ea: u32, n: u32) {
    let n = if n == 0 { 32 } else { n };
    let mut r = rd;
    for i in 0..n {
        if i % 4 == 0 {
            if i > 0 {
                r = (r + 1) % 32;
            }
            c.r[r] = 0;
        }
        c.r[r] |= (m.r8(ea + i) as u32) << (24 - 8 * (i % 4));
    }
}
pub fn stswi(c: &Cpu, m: &mut Mem, rs: usize, ea: u32, n: u32) {
    let n = if n == 0 { 32 } else { n };
    let mut r = rs;
    for i in 0..n {
        if i > 0 && i % 4 == 0 {
            r = (r + 1) % 32;
        }
        m.w8(ea + i, (c.r[r] >> (24 - 8 * (i % 4))) as u8);
    }
}

// ---- paired-single quantized load/store (GQR types 0 float, 4 u8, 5 u16, 6 s8, 7 s16) ----

fn qscale(s: u32) -> f64 {
    let s = ((s & 0x3F) as i32) << 26 >> 26; // 6-bit signed
    2f64.powi(s)
}

pub fn psq_load(c: &Cpu, m: &Mem, ea: u32, w: bool, i: usize) -> Fpr {
    let g = c.gqr[i];
    let ty = g >> 16 & 7;
    let sc = qscale(g >> 24);
    let n = if w { 1 } else { 2 };
    let mut v = [0f64, 1.0];
    for (k, slot) in v.iter_mut().enumerate().take(n) {
        let k = k as u32;
        *slot = match ty {
            4 => m.r8(ea + k) as f64 / sc,
            5 => m.r16(ea + 2 * k) as f64 / sc,
            6 => m.r8(ea + k) as i8 as f64 / sc,
            7 => m.r16(ea + 2 * k) as i16 as f64 / sc,
            _ => m.rf32(ea + 4 * k),
        };
    }
    Fpr { ps0: v[0], ps1: v[1] }
}

pub fn psq_store(c: &Cpu, m: &mut Mem, ea: u32, w: bool, i: usize, f: Fpr) {
    let g = c.gqr[i];
    let ty = g & 7;
    let sc = qscale(g >> 8);
    let n = if w { 1 } else { 2 };
    for (k, v) in [f.ps0, f.ps1].into_iter().enumerate().take(n) {
        let k = k as u32;
        let q = v * sc;
        match ty {
            4 => m.w8(ea + k, q.clamp(0.0, 255.0) as u8),
            5 => m.w16(ea + 2 * k, q.clamp(0.0, 65535.0) as u16),
            6 => m.w8(ea + k, q.clamp(-128.0, 127.0) as i8 as u8),
            7 => m.w16(ea + 2 * k, q.clamp(-32768.0, 32767.0) as i16 as u16),
            _ => m.wf32(ea + 4 * k, v),
        }
    }
}
