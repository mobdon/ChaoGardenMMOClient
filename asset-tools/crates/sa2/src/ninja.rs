//! Ninja trigonometry as SA2B's main.dol (GameCube, GSNE8P) computes it.
//!
//! The player code reads angles through `njSin`/`njCos`, which look up a
//! 65,536-entry table that the game builds at start-up (fn_801181A0, called
//! from main through fn_80118180). Entry k holds sinf/cosf of
//! `pi_f32 * (2k) / 65536` computed with MSL's single-precision `sinf` and
//! `cosf` (trigf.c, which needs its static constructor to have run). Those
//! routines are not correctly rounded, so a correctly rounded sine differs
//! from about half the entries. This module follows the compiled
//! instructions: every `fmadds` is fused (rounded to double, then to single)
//! and every single-precision result is rounded once.
//!
//! The constants are the values stored in main.dol. `tests/ninja.rs` checks
//! them and the whole table against the original instructions, which run
//! under `tests/dol_ppc.py`.

/// 2/pi as main.dol stores it (sdata2, r2 - 0x5838).
pub const TWO_OVER_PI: f32 = f32::from_bits(0x3f22_f983);
pub const HALF: f32 = 0.5;
/// MSL `__epsilon` (r2 - 0x5830).
pub const EPSILON: f32 = f32::from_bits(0x39b5_04f3);
/// `__four_over_pi_m1` after `__sinit_trigf_c` copies `tmp_float` into it.
pub const FOUR_OVER_PI_M1: [f32; 4] = [
    f32::from_bits(0x3e80_0000),
    f32::from_bits(0x3cbe_6080),
    f32::from_bits(0x3437_2200),
    f32::from_bits(0x2da4_4152),
];
/// MSL `__sincos_on_quadrant` (0x80176DA0).
pub const ON_QUADRANT: [f32; 8] = [0.0, 1.0, 1.0, 0.0, 0.0, -1.0, -1.0, 0.0];
/// MSL `__sincos_poly` (0x80176DC0).
pub const POLY: [f32; 10] = [
    f32::from_bits(0x366c_cfaa),
    f32::from_bits(0x34a5_e129),
    f32::from_bits(0xb9aa_e275),
    f32::from_bits(0xb819_6543),
    f32::from_bits(0x3c81_e0ed),
    f32::from_bits(0x3b23_35dd),
    f32::from_bits(0xbe9d_e9e6),
    f32::from_bits(0xbda5_5de7),
    f32::from_bits(0x3f80_0000),
    f32::from_bits(0x3f49_0fdb),
];
/// The table builder's pi (r2 - 0x55F4) and divisor (r2 - 0x55F0).
pub const TABLE_PI: f32 = f32::from_bits(0x4049_0fd8);
pub const TABLE_STEPS: f32 = 65536.0;

/// `fmadds d, a, c, b`: a*c+b fused, rounded to double then to single.
#[inline]
pub fn fmadds(a: f32, c: f32, b: f32) -> f32 {
    (a as f64).mul_add(c as f64, b as f64) as f32
}

/// PowerPC negates a negative-multiply-add result unless it is a NaN.
#[inline]
fn negate_unless_nan(v: f32) -> f32 {
    if v.is_nan() {
        v
    } else {
        -v
    }
}

/// `fnmadds`: the negated fused a*c+b.
#[inline]
pub fn fnmadds(a: f32, c: f32, b: f32) -> f32 {
    negate_unless_nan(fmadds(a, c, b))
}

/// `fnmsubs`: the negated fused a*c-b.
#[inline]
pub fn fnmsubs(a: f32, c: f32, b: f32) -> f32 {
    negate_unless_nan(fmadds(a, c, -b))
}

/// `fctiwz`: truncate toward zero. NaN and every out-of-range value give
/// 0x80000000, as the harness's QEMU core and x86's cvttss2si do (Gekko
/// itself saturates positive overflow to 0x7FFFFFFF).
#[inline]
pub fn fctiwz(x: f32) -> i32 {
    if x.is_nan() || x >= 2147483648.0 || x < -2147483648.0 {
        i32::MIN
    } else {
        x as i32
    }
}

/// Shared argument reduction: (y, quadrant).
fn reduce(x: f32) -> (f32, usize) {
    let z = TWO_OVER_PI * x;
    let n = if x.to_bits() & 0x8000_0000 != 0 {
        fctiwz(z - HALF)
    } else {
        fctiwz(HALF + z)
    };
    let twice = (n as u32).wrapping_shl(1) as i32;
    let mut y = x - (twice as f64) as f32;
    for c in FOUR_OVER_PI_M1 {
        y = fmadds(c, x, y);
    }
    (y, (n & 3) as usize)
}

/// MSL `sinf` (0x801115A8) as compiled.
pub fn sinf(x: f32) -> f32 {
    let (y, q) = reduce(x);
    let i = q << 1;
    if y.abs() < EPSILON || y.is_nan() {
        return fmadds(POLY[9], y * ON_QUADRANT[i + 1], ON_QUADRANT[i]);
    }
    let s = y * y;
    if q & 1 != 0 {
        let a = fmadds(POLY[0], s, POLY[2]);
        let a = fmadds(s, a, POLY[4]);
        let a = fmadds(s, a, POLY[6]);
        fmadds(s, a, POLY[8]) * ON_QUADRANT[i]
    } else {
        let a = fmadds(POLY[1], s, POLY[3]);
        let a = fmadds(s, a, POLY[5]);
        let a = fmadds(s, a, POLY[7]);
        (y * fmadds(s, a, POLY[9])) * ON_QUADRANT[i + 1]
    }
}

/// MSL `cosf` (0x80111414) as compiled.
pub fn cosf(x: f32) -> f32 {
    let (y, q) = reduce(x);
    let i = q << 1;
    if y.abs() < EPSILON || y.is_nan() {
        return fnmsubs(y, ON_QUADRANT[i], ON_QUADRANT[i + 1]);
    }
    let s = y * y;
    if q & 1 != 0 {
        let a = fmadds(POLY[1], s, POLY[3]);
        let a = fmadds(s, a, POLY[5]);
        let a = fmadds(s, a, POLY[7]);
        (y * fnmadds(s, a, POLY[9])) * ON_QUADRANT[i]
    } else {
        let a = fmadds(POLY[0], s, POLY[2]);
        let a = fmadds(s, a, POLY[4]);
        let a = fmadds(s, a, POLY[6]);
        fmadds(s, a, POLY[8]) * ON_QUADRANT[i + 1]
    }
}

/// The table `njSin`/`njCos` read (njSinCosTbl at 0x802D7758).
pub struct SinCos {
    table: Vec<[f32; 2]>,
}

impl Default for SinCos {
    fn default() -> Self {
        Self::new()
    }
}

impl SinCos {
    /// Build it as fn_801181A0 does.
    pub fn new() -> Self {
        let table = (0..65536u32)
            .map(|k| {
                let step = ((2 * k) as f64) as f32;
                let angle = (TABLE_PI * step) / TABLE_STEPS;
                [sinf(angle), cosf(angle)]
            })
            .collect();
        Self { table }
    }

    /// `njSin`: the entry for the angle's low 16 bits.
    pub fn sin(&self, angle: i32) -> f32 {
        self.table[angle as u16 as usize][0]
    }

    /// `njCos`.
    pub fn cos(&self, angle: i32) -> f32 {
        self.table[angle as u16 as usize][1]
    }

    /// The table as main.dol stores it (big-endian sin, cos pairs).
    pub fn to_be_bytes(&self) -> Vec<u8> {
        self.table
            .iter()
            .flat_map(|[s, c]| s.to_be_bytes().into_iter().chain(c.to_be_bytes()))
            .collect()
    }
}
