//! Gekko instruction -> Rust translation.
//!
//! Each game function becomes one Rust function: a `loop { match pc { .. } }`
//! over its basic blocks. Branches inside the function set `pc`; calls and
//! branches out of it go through `rt::call`, so overrides apply everywhere.

use std::collections::BTreeSet;
use std::fmt::Write;

use std::collections::BTreeMap;

/// A relocation applied to one instruction (REL code). `target` is a Rust
/// expression for the u32 address; `kind` is the R_PPC number.
#[derive(Clone, Debug)]
pub struct Reloc {
    pub kind: u8,
    pub target: String,
}

/// What the translator needs to know about the function being emitted.
#[derive(Default)]
pub struct Ctx {
    pub start: u32,
    pub end: u32,
    /// Prefix of the emitted Rust name (`f_{prefix}{start:08X}`).
    pub prefix: String,
    /// REL code: addresses are section offsets and this expression gives the
    /// section's runtime address. None for main.dol (absolute addresses).
    pub base: Option<String>,
    /// Relocations by instruction address (REL code only).
    pub relocs: BTreeMap<u32, Reloc>,
}

impl Ctx {
    fn local(&self, t: u32) -> bool {
        (self.start..self.end).contains(&t)
    }
    /// Runtime address of `t` in this function's address space.
    fn abs(&self, t: u32) -> String {
        if self.base.is_some() {
            format!("base.wrapping_add({t:#x})")
        } else {
            format!("{t:#010x}")
        }
    }
    pub fn ident(&self) -> String {
        format!("f_{}{:08X}", self.prefix, self.start)
    }
}

/// The 16-bit immediate a relocation puts in an instruction, as a u32 expression.
fn reloc_imm(r: &Reloc) -> Option<String> {
    let t = &r.target;
    Some(match r.kind {
        3 | 4 => format!("({t} & 0xFFFF)"),
        5 => format!("({t} >> 16)"),
        6 => format!("({t}.wrapping_add(0x8000) >> 16)"),
        _ => return None,
    })
}

pub struct Ins {
    pub code: String,
    /// Local branch targets (block starts).
    pub targets: Vec<u32>,
    /// Control never falls through to the next instruction.
    pub ends: bool,
    /// Instruction the translator does not know.
    pub unknown: bool,
}

fn simm(w: u32) -> i32 {
    w as u16 as i16 as i32
}

fn ea_d(ra: u32, d: i32) -> String {
    if ra == 0 {
        format!("{:#x}u32", d as u32)
    } else {
        format!("c.r[{ra}].wrapping_add({:#x})", d as u32)
    }
}

fn ea_x(ra: u32, rb: u32) -> String {
    if ra == 0 {
        format!("c.r[{rb}]")
    } else {
        format!("c.r[{ra}].wrapping_add(c.r[{rb}])")
    }
}

fn mask(mb: u32, me: u32) -> u32 {
    let a = u32::MAX >> mb;
    let b = u32::MAX << (31 - me);
    if mb <= me { a & b } else { a | b }
}

fn bc_cond(bo: u32, bi: u32) -> (String, String) {
    let pre = if bo & 4 == 0 { "c.ctr = c.ctr.wrapping_sub(1); ".to_string() } else { String::new() };
    let mut parts = Vec::new();
    if bo & 4 == 0 {
        parts.push(if bo & 2 != 0 { "c.ctr == 0".to_string() } else { "c.ctr != 0".to_string() });
    }
    if bo & 0x10 == 0 {
        parts.push(if bo & 8 != 0 { format!("c.bit({bi})") } else { format!("!c.bit({bi})") });
    }
    let cond = if parts.is_empty() { "true".to_string() } else { parts.join(" && ") };
    (pre, cond)
}

/// Branch to `t` without link: local jump, or tail call out of the function.
fn goto(ctx: &Ctx, t: u32, targets: &mut Vec<u32>) -> String {
    if ctx.local(t) {
        targets.push(t);
        format!("pc = {t:#010x}; continue;")
    } else {
        format!("rt::call(c, m, {}); return;", ctx.abs(t))
    }
}

pub fn translate(w: u32, pc: u32, ctx: &Ctx) -> Ins {
    let op = w >> 26;
    let d = w >> 21 & 31;
    let a = w >> 16 & 31;
    let b = w >> 11 & 31;
    let cr = w >> 6 & 31;
    let rc = w & 1 != 0;
    let xo10 = w >> 1 & 0x3FF;
    let xo5 = w >> 1 & 31;
    let next = pc.wrapping_add(4);
    let mut targets = Vec::new();
    let mut ends = false;
    let mut unknown = false;
    let si = simm(w);
    let ui = w & 0xFFFF;
    let rd = |v: &str| format!("c.r[{d}] = {v};");
    let ra_set = |v: &str| format!("c.r[{a}] = {v};");
    let recd = if rc { format!(" c.rec(c.r[{d}]);") } else { String::new() };
    let reca = if rc { format!(" c.rec(c.r[{a}]);") } else { String::new() };
    let (fa, fb, fc) = (format!("c.f[{a}].ps0"), format!("c.f[{b}].ps0"), format!("c.f[{cr}].ps0"));
    let fill = |v: &str| format!("{{ let v = rs({v}); c.f[{d}] = Fpr {{ ps0: v, ps1: v }}; }}");
    let set0 = |v: &str| format!("c.f[{d}].ps0 = {v};");
    let next_abs = ctx.abs(next);
    let reloc = ctx.relocs.get(&pc);
    let imm = reloc.and_then(reloc_imm);
    let ra0 = if a == 0 { "0u32".to_string() } else { format!("c.r[{a}]") };
    // D-form effective address, with the relocated displacement when there is one.
    let d_ea = |disp: i32| match &imm {
        Some(v) => format!("{ra0}.wrapping_add({v} as u16 as i16 as i32 as u32)"),
        None => ea_d(a, disp),
    };
    if let Some(r) = reloc {
        let code = match (op, r.kind, &imm) {
            (18, 10, _) => {
                ends = w & 1 == 0;
                let link = if w & 1 != 0 { format!("c.lr = {next_abs}; ") } else { String::new() };
                let ret = if ends { " return;" } else { "" };
                // Bound first: the target may read c.rt, and rt::call borrows c mutably.
                Some(format!("{{ let t = {}; {link}rt::call(c, m, t);{ret} }}", r.target))
            }
            (14, _, Some(v)) => Some(format!("c.r[{d}] = {ra0}.wrapping_add({v} as u16 as i16 as i32 as u32);")),
            (15, _, Some(v)) => Some(format!("c.r[{d}] = {ra0}.wrapping_add({v} << 16);")),
            (24, _, Some(v)) => Some(format!("c.r[{a}] = c.r[{d}] | {v};")),
            (25, _, Some(v)) => Some(format!("c.r[{a}] = c.r[{d}] | ({v} << 16);")),
            (32..=55, _, Some(_)) => None, // the D-form arms below use d_ea
            _ => {
                let code = format!("panic!(\"sa2-recomp: relocation type {} on {w:#010x} at {pc:#x}\");", r.kind);
                return Ins { code, targets, ends: true, unknown: true };
            }
        };
        if let Some(code) = code {
            return Ins { code, targets, ends, unknown: false };
        }
    }

    let code: String = match op {
        3 => String::new(), // twi: conditional traps are debug checks
        4 => {
            let xo6 = w >> 1 & 0x3F;
            match xo6 {
                6 | 38 => {
                    let (wb, i) = (w >> 10 & 1 != 0, w >> 7 & 7);
                    let up = if xo6 == 38 { format!(" c.r[{a}] = ea;") } else { String::new() };
                    format!("{{ let ea = {}; c.f[{d}] = psq_load(c, m, ea, {wb}, {i});{up} }}", ea_x(a, b))
                }
                7 | 39 => {
                    let (wb, i) = (w >> 10 & 1 != 0, w >> 7 & 7);
                    let up = if xo6 == 39 { format!(" c.r[{a}] = ea;") } else { String::new() };
                    format!("{{ let ea = {}; let f = c.f[{d}]; psq_store(c, m, ea, {wb}, {i}, f);{up} }}", ea_x(a, b))
                }
                _ => {
                    let pair = |p0: &str, p1: &str| {
                        format!(
                            "{{ let a = c.f[{a}]; let b = c.f[{b}]; let cc = c.f[{cr}]; let _ = (a, b, cc); c.f[{d}] = Fpr {{ ps0: rs({p0}), ps1: rs({p1}) }}; }}"
                        )
                    };
                    let raw = |p0: &str, p1: &str| {
                        format!("{{ let a = c.f[{a}]; let b = c.f[{b}]; let _ = (a, b); c.f[{d}] = Fpr {{ ps0: {p0}, ps1: {p1} }}; }}")
                    };
                    match xo5 {
                        10 => pair("a.ps0 + b.ps1", "cc.ps1"),
                        11 => pair("cc.ps0", "a.ps0 + b.ps1"),
                        12 => pair("a.ps0 * cc.ps0", "a.ps1 * cc.ps0"),
                        13 => pair("a.ps0 * cc.ps1", "a.ps1 * cc.ps1"),
                        14 => pair("a.ps0.mul_add(cc.ps0, b.ps0)", "a.ps1.mul_add(cc.ps0, b.ps1)"),
                        15 => pair("a.ps0.mul_add(cc.ps1, b.ps0)", "a.ps1.mul_add(cc.ps1, b.ps1)"),
                        18 => pair("a.ps0 / b.ps0", "a.ps1 / b.ps1"),
                        20 => pair("a.ps0 - b.ps0", "a.ps1 - b.ps1"),
                        21 => pair("a.ps0 + b.ps0", "a.ps1 + b.ps1"),
                        23 => pair("fsel(a.ps0, cc.ps0, b.ps0)", "fsel(a.ps1, cc.ps1, b.ps1)"),
                        24 => pair("1.0 / b.ps0", "1.0 / b.ps1"),
                        25 => pair("a.ps0 * cc.ps0", "a.ps1 * cc.ps1"),
                        26 => pair("frsqrte(b.ps0)", "frsqrte(b.ps1)"),
                        28 => pair("a.ps0.mul_add(cc.ps0, -b.ps0)", "a.ps1.mul_add(cc.ps1, -b.ps1)"),
                        29 => pair("a.ps0.mul_add(cc.ps0, b.ps0)", "a.ps1.mul_add(cc.ps1, b.ps1)"),
                        30 => pair("-a.ps0.mul_add(cc.ps0, -b.ps0)", "-a.ps1.mul_add(cc.ps1, -b.ps1)"),
                        31 => pair("-a.ps0.mul_add(cc.ps0, b.ps0)", "-a.ps1.mul_add(cc.ps1, b.ps1)"),
                        _ => match xo10 {
                            0 | 32 => format!("c.cmp_f({}, c.f[{a}].ps0, c.f[{b}].ps0);", d >> 2),
                            64 | 96 => format!("c.cmp_f({}, c.f[{a}].ps1, c.f[{b}].ps1);", d >> 2),
                            40 => raw("-b.ps0", "-b.ps1"),
                            72 => raw("b.ps0", "b.ps1"),
                            136 => raw("-b.ps0.abs()", "-b.ps1.abs()"),
                            264 => raw("b.ps0.abs()", "b.ps1.abs()"),
                            528 => raw("a.ps0", "b.ps0"),
                            560 => raw("a.ps0", "b.ps1"),
                            592 => raw("a.ps1", "b.ps0"),
                            624 => raw("a.ps1", "b.ps1"),
                            1014 => format!("m.dcbz({});", ea_x(a, b)),
                            _ => {
                                unknown = true;
                                String::new()
                            }
                        },
                    }
                }
            }
        }
        7 => rd(&format!("c.r[{a}].wrapping_mul({:#x})", si as u32)),
        8 => format!("c.r[{d}] = {{ let (v, k) = adde(!c.r[{a}], {:#x}, true); c.ca = k; v }};", si as u32),
        10 => format!("c.cmp_u({}, c.r[{a}], {ui:#x});", d >> 2),
        11 => format!("c.cmp_s({}, c.r[{a}] as i32, {si});", d >> 2),
        12 | 13 => {
            let r = if op == 13 { format!(" c.rec(c.r[{d}]);") } else { String::new() };
            format!("c.r[{d}] = {{ let (v, k) = adde(c.r[{a}], {:#x}, false); c.ca = k; v }};{r}", si as u32)
        }
        14 => rd(&ea_d(a, si)),
        15 => rd(&ea_d(a, si << 16)),
        16 => {
            let bd = (w & 0xFFFC) as u16 as i16 as i32;
            let t = if w & 2 != 0 { bd as u32 } else { pc.wrapping_add(bd as u32) };
            let (pre, cond) = bc_cond(d, a);
            if w & 1 != 0 {
                let ta = if w & 2 != 0 { format!("{t:#010x}") } else { ctx.abs(t) };
                format!("{pre}if {cond} {{ c.lr = {next_abs}; rt::call(c, m, {ta}); }}")
            } else {
                if cond == "true" {
                    ends = true;
                }
                format!("{pre}if {cond} {{ {} }}", goto(ctx, t, &mut targets))
            }
        }
        17 => format!("panic!(\"sc at {pc:#010x}\");"),
        18 => {
            let li = ((w & 0x03FF_FFFC) << 6) as i32 >> 6;
            let t = if w & 2 != 0 { li as u32 } else { pc.wrapping_add(li as u32) };
            if w & 1 != 0 {
                let ta = if w & 2 != 0 { format!("{t:#010x}") } else { ctx.abs(t) };
                format!("c.lr = {next_abs}; rt::call(c, m, {ta});")
            } else {
                ends = true;
                goto(ctx, t, &mut targets)
            }
        }
        19 => {
            let crop = |e: &str| format!("{{ let a = c.bit({a}); let b = c.bit({b}); c.set_bit({d}, {e}); }}");
            match xo10 {
                0 => format!("{{ let v = c.field({}); c.set_field({}, v); }}", a >> 2, d >> 2),
                16 => {
                    let (pre, cond) = bc_cond(d, a);
                    if w & 1 != 0 {
                        format!("{pre}if {cond} {{ let t = c.lr; c.lr = {next_abs}; rt::call(c, m, t); }}")
                    } else {
                        if cond == "true" {
                            ends = true;
                        }
                        format!("{pre}if {cond} {{ return; }}")
                    }
                }
                33 => crop("!(a | b)"),
                50 => {
                    ends = true;
                    "return;".into()
                }
                129 => crop("a & !b"),
                150 => String::new(),
                193 => crop("a ^ b"),
                225 => crop("!(a & b)"),
                257 => crop("a & b"),
                289 => crop("a == b"),
                417 => crop("a | !b"),
                449 => crop("a | b"),
                528 => {
                    let (pre, cond) = bc_cond(d | 4, a);
                    let _ = pre;
                    if w & 1 != 0 {
                        format!("if {cond} {{ let t = c.ctr; c.lr = {next_abs}; rt::call(c, m, t); }}")
                    } else {
                        if cond == "true" {
                            ends = true;
                        }
                        // Jump tables land on local blocks; anything else is a tail call.
                        let off = if ctx.base.is_some() { ".wrapping_sub(base)" } else { "" };
                        format!("if {cond} {{ pc = c.ctr{off}; continue; }}")
                    }
                }
                _ => {
                    unknown = true;
                    String::new()
                }
            }
        }
        20 => {
            let (sh, mb, me) = (b, w >> 6 & 31, w >> 1 & 31);
            let k = mask(mb, me);
            format!("c.r[{a}] = (c.r[{d}].rotate_left({sh}) & {k:#x}) | (c.r[{a}] & {:#x});{reca}", !k)
        }
        21 => {
            let (sh, mb, me) = (b, w >> 6 & 31, w >> 1 & 31);
            format!("c.r[{a}] = c.r[{d}].rotate_left({sh}) & {:#x};{reca}", mask(mb, me))
        }
        23 => {
            let (mb, me) = (w >> 6 & 31, w >> 1 & 31);
            format!("c.r[{a}] = c.r[{d}].rotate_left(c.r[{b}] & 31) & {:#x};{reca}", mask(mb, me))
        }
        24 => ra_set(&format!("c.r[{d}] | {ui:#x}")),
        25 => ra_set(&format!("c.r[{d}] | {:#x}", ui << 16)),
        26 => ra_set(&format!("c.r[{d}] ^ {ui:#x}")),
        27 => ra_set(&format!("c.r[{d}] ^ {:#x}", ui << 16)),
        28 => format!("c.r[{a}] = c.r[{d}] & {ui:#x}; c.rec(c.r[{a}]);"),
        29 => format!("c.r[{a}] = c.r[{d}] & {:#x}; c.rec(c.r[{a}]);", ui << 16),
        31 => {
            let xo9 = xo10 & 0x1FF;
            let arith = |e: &str| format!("c.r[{d}] = {e};{recd}");
            let logic = |e: &str| format!("c.r[{a}] = {e};{reca}");
            let (rs_, ra_, rb_) = (format!("c.r[{d}]"), format!("c.r[{a}]"), format!("c.r[{b}]"));
            let ld = |f: &str, upd: bool| {
                let up = if upd { format!(" c.r[{a}] = ea;") } else { String::new() };
                format!("{{ let ea = {}; c.r[{d}] = {f};{up} }}", ea_x(a, b))
            };
            let st = |f: &str, upd: bool| {
                let up = if upd { format!(" c.r[{a}] = ea;") } else { String::new() };
                format!("{{ let ea = {}; {f};{up} }}", ea_x(a, b))
            };
            match xo9 {
                8 => arith(&format!("{{ let (v, k) = adde(!{ra_}, {rb_}, true); c.ca = k; v }}")),
                10 => arith(&format!("{{ let (v, k) = adde({ra_}, {rb_}, false); c.ca = k; v }}")),
                11 => arith(&format!("(({ra_} as u64 * {rb_} as u64) >> 32) as u32")),
                40 => arith(&format!("{rb_}.wrapping_sub({ra_})")),
                75 => arith(&format!("(({ra_} as i32 as i64 * {rb_} as i32 as i64) >> 32) as u32")),
                104 => arith(&format!("{ra_}.wrapping_neg()")),
                136 => arith(&format!("{{ let k = c.ca; {{ let (v, k) = adde(!{ra_}, {rb_}, k); c.ca = k; v }} }}")),
                138 => arith(&format!("{{ let k = c.ca; {{ let (v, k) = adde({ra_}, {rb_}, k); c.ca = k; v }} }}")),
                200 => arith(&format!("{{ let k = c.ca; {{ let (v, k) = adde(!{ra_}, 0, k); c.ca = k; v }} }}")),
                202 => arith(&format!("{{ let k = c.ca; {{ let (v, k) = adde({ra_}, 0, k); c.ca = k; v }} }}")),
                232 => arith(&format!("{{ let k = c.ca; {{ let (v, k) = adde(!{ra_}, u32::MAX, k); c.ca = k; v }} }}")),
                234 => arith(&format!("{{ let k = c.ca; {{ let (v, k) = adde({ra_}, u32::MAX, k); c.ca = k; v }} }}")),
                235 => arith(&format!("{ra_}.wrapping_mul({rb_})")),
                266 => arith(&format!("{ra_}.wrapping_add({rb_})")),
                459 => arith(&format!("divwu({ra_}, {rb_})")),
                491 => arith(&format!("divw({ra_}, {rb_})")),
                _ => match xo10 {
                    0 => format!("c.cmp_s({}, {ra_} as i32, {rb_} as i32);", d >> 2),
                    32 => format!("c.cmp_u({}, {ra_}, {rb_});", d >> 2),
                    4 => {
                        if d == 31 {
                            format!("panic!(\"trap at {pc:#010x}\");")
                        } else {
                            String::new()
                        }
                    }
                    19 => rd("c.cr"),
                    20 | 23 => ld("m.r32(ea)", false),
                    55 => ld("m.r32(ea)", true),
                    87 => ld("m.r8(ea) as u32", false),
                    119 => ld("m.r8(ea) as u32", true),
                    279 => ld("m.r16(ea) as u32", false),
                    311 => ld("m.r16(ea) as u32", true),
                    343 => ld("m.r16(ea) as i16 as u32", false),
                    375 => ld("m.r16(ea) as i16 as u32", true),
                    534 => ld("m.r32(ea).swap_bytes()", false),
                    790 => ld("m.r16(ea).swap_bytes() as u32", false),
                    150 => format!("{{ let ea = {}; m.w32(ea, {rs_}); let so = c.so as u32; c.set_field(0, 2 | so); }}", ea_x(a, b)),
                    151 => st(&format!("m.w32(ea, {rs_})"), false),
                    183 => st(&format!("m.w32(ea, {rs_})"), true),
                    215 => st(&format!("m.w8(ea, {rs_} as u8)"), false),
                    247 => st(&format!("m.w8(ea, {rs_} as u8)"), true),
                    407 => st(&format!("m.w16(ea, {rs_} as u16)"), false),
                    439 => st(&format!("m.w16(ea, {rs_} as u16)"), true),
                    662 => st(&format!("m.w32(ea, {rs_}.swap_bytes())"), false),
                    918 => st(&format!("m.w16(ea, ({rs_} as u16).swap_bytes())"), false),
                    535 | 567 => {
                        let up = if xo10 == 567 { format!(" c.r[{a}] = ea;") } else { String::new() };
                        format!("{{ let ea = {}; let v = m.rf32(ea); c.f[{d}] = Fpr {{ ps0: v, ps1: v }};{up} }}", ea_x(a, b))
                    }
                    599 | 631 => {
                        let up = if xo10 == 631 { format!(" c.r[{a}] = ea;") } else { String::new() };
                        format!("{{ let ea = {}; c.f[{d}].ps0 = m.rf64(ea);{up} }}", ea_x(a, b))
                    }
                    663 => st(&format!("m.wf32(ea, c.f[{d}].ps0)"), false),
                    695 => st(&format!("m.wf32(ea, c.f[{d}].ps0)"), true),
                    727 => st(&format!("m.wf64(ea, c.f[{d}].ps0)"), false),
                    759 => st(&format!("m.wf64(ea, c.f[{d}].ps0)"), true),
                    983 => st(&format!("m.w32(ea, c.f[{d}].ps0.to_bits() as u32)"), false),
                    24 => logic(&format!("if {rb_} & 0x20 != 0 {{ 0 }} else {{ {rs_} << ({rb_} & 31) }}")),
                    536 => logic(&format!("if {rb_} & 0x20 != 0 {{ 0 }} else {{ {rs_} >> ({rb_} & 31) }}")),
                    26 => logic(&format!("{rs_}.leading_zeros()")),
                    28 => logic(&format!("{rs_} & {rb_}")),
                    60 => logic(&format!("{rs_} & !{rb_}")),
                    124 => logic(&format!("!({rs_} | {rb_})")),
                    284 => logic(&format!("!({rs_} ^ {rb_})")),
                    316 => logic(&format!("{rs_} ^ {rb_}")),
                    412 => logic(&format!("{rs_} | !{rb_}")),
                    444 => logic(&format!("{rs_} | {rb_}")),
                    476 => logic(&format!("!({rs_} & {rb_})")),
                    792 => logic(&format!("{{ let v = {rs_}; let s = {rb_}; let (r, k) = sraw(v, s); c.ca = k; r }}")),
                    824 => logic(&format!("{{ let v = {rs_}; let (r, k) = sraw(v, {b}); c.ca = k; r }}")),
                    922 => logic(&format!("{rs_} as u16 as i16 as u32")),
                    954 => logic(&format!("{rs_} as u8 as i8 as u32")),
                    83 => rd("c.msr"),
                    146 => format!("c.msr = {rs_};"),
                    144 => format!("{{ let v = {rs_}; c.mtcrf({:#x}, v); }}", w >> 12 & 0xFF),
                    339 | 371 => rd(&format!("c.mfspr({})", a | b << 5)),
                    467 => format!("{{ let v = {rs_}; c.mtspr({}, v); }}", a | b << 5),
                    512 => format!(
                        "{{ let v = (c.so as u32) << 3 | (c.ca as u32) << 1; c.set_field({}, v); c.so = false; c.ca = false; }}",
                        d >> 2
                    ),
                    210 | 242 | 595 | 659 => {
                        if xo10 >= 595 {
                            rd("0")
                        } else {
                            String::new()
                        }
                    }
                    54 | 86 | 246 | 278 | 470 | 982 | 598 | 854 | 306 | 566 | 370 => String::new(),
                    1014 => format!("m.dcbz({});", ea_x(a, b)),
                    597 => format!("{{ let ea = {}; lswi(c, m, {d}, ea, {b}); }}", ea_d(a, 0)),
                    725 => format!("{{ let ea = {}; stswi(c, m, {d}, ea, {b}); }}", ea_d(a, 0)),
                    _ => {
                        unknown = true;
                        String::new()
                    }
                },
            }
        }
        32..=47 => {
            let ea = d_ea(si);
            let upd = op & 1 == 1 && op != 47 && op != 46;
            let up = if upd { format!(" c.r[{a}] = ea;") } else { String::new() };
            let body = match op {
                32 | 33 => format!("c.r[{d}] = m.r32(ea);"),
                34 | 35 => format!("c.r[{d}] = m.r8(ea) as u32;"),
                36 | 37 => format!("m.w32(ea, c.r[{d}]);"),
                38 | 39 => format!("m.w8(ea, c.r[{d}] as u8);"),
                40 | 41 => format!("c.r[{d}] = m.r16(ea) as u32;"),
                42 | 43 => format!("c.r[{d}] = m.r16(ea) as i16 as u32;"),
                44 | 45 => format!("m.w16(ea, c.r[{d}] as u16);"),
                46 => format!("lmw(c, m, {d}, ea);"),
                _ => format!("stmw(c, m, {d}, ea);"),
            };
            format!("{{ let ea = {ea}; {body}{up} }}")
        }
        48..=55 => {
            let ea = d_ea(si);
            let up = if op & 1 == 1 { format!(" c.r[{a}] = ea;") } else { String::new() };
            let body = match op {
                48 | 49 => format!("let v = m.rf32(ea); c.f[{d}] = Fpr {{ ps0: v, ps1: v }};"),
                50 | 51 => format!("c.f[{d}].ps0 = m.rf64(ea);"),
                52 | 53 => format!("m.wf32(ea, c.f[{d}].ps0);"),
                _ => format!("m.wf64(ea, c.f[{d}].ps0);"),
            };
            format!("{{ let ea = {ea}; {body}{up} }}")
        }
        56 | 57 | 60 | 61 => {
            let off = ((w & 0xFFF) << 20) as i32 >> 20;
            let (wb, i) = (w >> 15 & 1 != 0, w >> 12 & 7);
            let up = if op & 1 == 1 { format!(" c.r[{a}] = ea;") } else { String::new() };
            let body = if op < 60 {
                format!("c.f[{d}] = psq_load(c, m, ea, {wb}, {i});")
            } else {
                format!("let f = c.f[{d}]; psq_store(c, m, ea, {wb}, {i}, f);")
            };
            format!("{{ let ea = {}; {body}{up} }}", ea_d(a, off))
        }
        59 => match xo5 {
            18 => fill(&format!("{fa} / {fb}")),
            20 => fill(&format!("{fa} - {fb}")),
            21 => fill(&format!("{fa} + {fb}")),
            24 => fill(&format!("fres({fb})")),
            25 => fill(&format!("{fa} * {fc}")),
            28 => fill(&format!("{fa}.mul_add({fc}, -{fb})")),
            29 => fill(&format!("{fa}.mul_add({fc}, {fb})")),
            30 => fill(&format!("-{fa}.mul_add({fc}, -{fb})")),
            31 => fill(&format!("-{fa}.mul_add({fc}, {fb})")),
            _ => {
                unknown = true;
                String::new()
            }
        },
        63 => match xo5 {
            18 => set0(&format!("{fa} / {fb}")),
            20 => set0(&format!("{fa} - {fb}")),
            21 => set0(&format!("{fa} + {fb}")),
            23 => set0(&format!("fsel({fa}, {fc}, {fb})")),
            25 => set0(&format!("{fa} * {fc}")),
            26 => set0(&format!("frsqrte({fb})")),
            28 => set0(&format!("{fa}.mul_add({fc}, -{fb})")),
            29 => set0(&format!("{fa}.mul_add({fc}, {fb})")),
            30 => set0(&format!("-{fa}.mul_add({fc}, -{fb})")),
            31 => set0(&format!("-{fa}.mul_add({fc}, {fb})")),
            _ => match xo10 {
                0 | 32 => format!("c.cmp_f({}, {fa}, {fb});", d >> 2),
                12 => fill(&fb),
                14 => set0(&format!("fctiw({fb}, false)")),
                15 => set0(&format!("fctiw({fb}, true)")),
                38 => format!("c.fpscr |= {:#x};", 1u32 << (31 - d)),
                70 => format!("c.fpscr &= !{:#x};", 1u32 << (31 - d)),
                40 => set0(&format!("-{fb}")),
                72 => set0(&fb),
                136 => set0(&format!("-{fb}.abs()")),
                264 => set0(&format!("{fb}.abs()")),
                64 => format!("{{ let v = c.fpscr >> (28 - 4 * {}) & 0xF; c.set_field({}, v); }}", a >> 2, d >> 2),
                134 => {
                    let f = d >> 2;
                    let sh = 28 - 4 * f;
                    format!("c.fpscr = (c.fpscr & !(0xF << {sh})) | ({} << {sh});", w >> 12 & 0xF)
                }
                583 => set0("f64::from_bits(c.fpscr as u64)"),
                711 => {
                    let fm = w >> 17 & 0xFF;
                    let mut k = 0u32;
                    for i in 0..8 {
                        if fm >> (7 - i) & 1 != 0 {
                            k |= 0xF << (28 - 4 * i);
                        }
                    }
                    format!("c.fpscr = (c.fpscr & !{k:#x}) | (c.f[{b}].ps0.to_bits() as u32 & {k:#x});")
                }
                _ => {
                    unknown = true;
                    String::new()
                }
            },
        },
        _ => {
            unknown = true;
            String::new()
        }
    };
    let code = if unknown { format!("panic!(\"sa2-recomp: unknown instruction {w:#010x} at {pc:#010x}\");") } else { code };
    if unknown {
        ends = true;
    }
    Ins { code, targets, ends, unknown }
}

/// Function ranges `(start, end, name)` from symbol starts (`addr -> (name,
/// size)`, size 0 = up to the next start) clipped to `text` (`[lo, hi)`
/// ranges). Every direct branch target that leaves its function, and every
/// `bl` target, also becomes a function start: routines with several entry
/// points (register save/restore ladders) have a symbol only for the first.
/// `calls` adds more targets (relocated calls); `skip` marks instructions whose
/// words are relocated (their branch offset is not meaningful).
/// Returns the functions and how many starts were added.
pub fn layout(
    mut starts: BTreeMap<u32, (String, u32)>,
    text: &[(u32, u32)],
    word: impl Fn(u32) -> Option<u32>,
    calls: &BTreeSet<u32>,
    skip: impl Fn(u32) -> bool,
) -> (Vec<(u32, u32, String)>, usize) {
    let sec = |a: u32| text.iter().find(|(lo, hi)| (*lo..*hi).contains(&a)).copied();
    starts.retain(|&a, _| sec(a).is_some());
    let mut added = 0;
    for &t in calls {
        if sec(t).is_some() && !starts.contains_key(&t) {
            starts.insert(t, (format!("fn_{t:08X}"), 0));
            added += 1;
        }
    }
    loop {
        let funcs = ranges(&starts, &sec);
        let mut new = BTreeSet::new();
        for (a, end, _) in &funcs {
            for pc in (*a..*end).step_by(4) {
                let w = match word(pc) {
                    Some(w) if w >> 26 == 18 && w & 2 == 0 && !skip(pc) => w,
                    _ => continue,
                };
                let li = ((w & 0x03FF_FFFC) << 6) as i32 >> 6;
                let t = pc.wrapping_add(li as u32);
                let leaves = !(*a..*end).contains(&t);
                if (w & 1 == 1 || leaves) && sec(t).is_some() && !starts.contains_key(&t) {
                    new.insert(t);
                }
            }
        }
        if new.is_empty() {
            return (funcs, added);
        }
        added += new.len();
        for t in new {
            starts.insert(t, (format!("fn_{t:08X}"), 0));
        }
    }
}

fn ranges(starts: &BTreeMap<u32, (String, u32)>, sec: &impl Fn(u32) -> Option<(u32, u32)>) -> Vec<(u32, u32, String)> {
    let addrs: Vec<u32> = starts.keys().copied().collect();
    let mut out = Vec::new();
    for (i, (&a, (name, size))) in starts.iter().enumerate() {
        let Some((_, sec_end)) = sec(a) else { continue };
        let next = addrs.get(i + 1).copied().unwrap_or(sec_end).min(sec_end);
        let end = if *size > 0 { a.saturating_add(*size).min(sec_end) } else { next };
        out.push((a, end, name.clone()));
    }
    out
}

/// Emit one function. `words` are its instructions starting at `ctx.start`;
/// `extra` holds more block starts (jump table entries found in data).
pub fn function(name: &str, ctx: &Ctx, words: &[u32], extra: &BTreeSet<u32>) -> (String, usize) {
    let ins: Vec<Ins> = words
        .iter()
        .enumerate()
        .map(|(i, &w)| translate(w, ctx.start + 4 * i as u32, ctx))
        .collect();
    let mut starts: BTreeSet<u32> = extra.iter().copied().filter(|&a| ctx.local(a) && a % 4 == 0).collect();
    starts.insert(ctx.start);
    for (i, x) in ins.iter().enumerate() {
        starts.extend(x.targets.iter().copied());
        // Any branch splits the block after it, so both paths are blocks.
        if !x.targets.is_empty() || x.ends {
            starts.insert(ctx.start + 4 * (i as u32 + 1));
        }
    }
    let unknown = ins.iter().filter(|x| x.unknown).count();
    let mut s = String::new();
    let _ = writeln!(s, "/// {name}");
    let _ = writeln!(s, "pub fn {}(c: &mut Cpu, m: &mut Mem) {{", ctx.ident());
    if let Some(b) = &ctx.base {
        let _ = writeln!(s, "    let base: u32 = {b};");
    }
    let _ = writeln!(s, "    let mut pc: u32 = {:#010x};", ctx.start);
    let _ = writeln!(s, "    loop {{\n        match pc {{");
    let mut open = false;
    for (i, x) in ins.iter().enumerate() {
        let a = ctx.start + 4 * i as u32;
        if starts.contains(&a) {
            if open {
                let _ = writeln!(s, "                pc = {a:#010x};\n            }}");
            }
            let _ = writeln!(s, "            {a:#010x} => {{");
            open = true;
        }
        if !open {
            continue; // unreachable after an unconditional branch
        }
        let _ = writeln!(s, "                {}", x.code);
        if x.ends {
            let _ = writeln!(s, "            }}");
            open = false;
        }
    }
    if open {
        let _ = writeln!(s, "                pc = {:#010x};\n            }}", ctx.end);
    }
    let back = if ctx.base.is_some() { "base.wrapping_add(pc)" } else { "pc" };
    let _ = writeln!(s, "            _ => {{ rt::call(c, m, {back}); return; }}");
    let _ = writeln!(s, "        }}\n    }}\n}}");
    (s, unknown)
}
