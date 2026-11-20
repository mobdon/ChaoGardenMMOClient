//! fn_8002334C (GSNE8P main.dol): pick the land-table entries near the
//! players for this frame's collision.
//!
//! This is the static land-table half. The routine first walks the dynamic
//! land objects (r13-0x79E0, count r13-0x79E4) that movers register, which
//! is not ported yet, so callers pass the count those produced. Then, when
//! r13-0x7F28 == 1, it walks the current LandTable (r13-0x79F0). Each 0x20-byte
//! COL entry has center +0, radius +0xC, object +0x10 and flags +0x1C.
//! Entries with flags & 0x2003 are tested against each player position:
//! horizontal-only when flags & 0x2002, otherwise in 3D. The square root is
//! the routine's inline Gekko `frsqrte` and three double-precision Newton
//! steps. An entry is taken when `distance - range < radius`. `range` is
//! r13-0x7F30 for the first position and 400.0 (r2-0x7AC4) after a miss.
//! Selection stops at 0x80 static entries or 0x400 in total.
use crate::{msl_math::gekko_frsqrte, ninja_matrix::Vec3};

/// r2-0x7AC4: the range used after the first position misses.
pub const RETRY_RANGE: f32 = 400.0;
pub const STATIC_LIMIT: u32 = 0x80;
pub const TOTAL_LIMIT: i16 = 0x400;

/// The fields of a LandTable COL entry the selection reads.
#[derive(Debug, Clone, Copy)]
pub struct LandCol {
    pub center: Vec3,
    pub radius: f32,
    pub flags: u32,
}

/// The LandTable header fields: entry count (+0) and first entry (+2; a
/// negative value means all entries).
#[derive(Debug, Clone, Copy)]
pub struct LandTableHead {
    pub count: i16,
    pub start: i16,
}

fn inline_sqrt(v: f32) -> f32 {
    let v = v as f64;
    let mut e = gekko_frsqrte(v);
    for _ in 0..2 {
        let sq = e * e;
        e *= 0.5;
        e *= (-v).mul_add(sq, 3.0);
    }
    let sq = e * e;
    let r = (0.5 * e) * (-v).mul_add(sq, 3.0);
    (v * r) as f32
}

/// The selection state the routine keeps in r13 globals.
#[derive(Debug, Clone, Copy, Default)]
pub struct Counters {
    /// r13-0x7A04: static entries taken this frame.
    pub statics: u32,
    /// r13-0x79A4: all entries in the output list.
    pub total: i16,
}

/// Indices (into `cols`) of the static entries taken, in output order.
pub fn select_static(
    head: LandTableHead,
    cols: &[LandCol],
    positions: &[Vec3],
    range: f32,
    counters: &mut Counters,
) -> Vec<usize> {
    let mut out = vec![];
    let (first, mut left) = if head.start >= 0 {
        (head.start as usize, head.count as i32 - head.start as i32)
    } else {
        (0, head.count as i32)
    };
    let mut i = first;
    while left > 0 {
        let e = &cols[i];
        if e.flags & 0x2003 != 0 {
            let mut reach = range;
            for p in positions {
                let dy = e.center[1] - p[1];
                let dz = e.center[2] - p[2];
                let dx = e.center[0] - p[0];
                let mut y2 = dy * dy;
                let z2 = dz * dz;
                let x2 = dx * dx;
                if e.flags & 0x2002 != 0 {
                    y2 = 0.0;
                }
                let mut d = z2 + (x2 + y2);
                if d > 0.0 {
                    d = inline_sqrt(d);
                }
                if d - reach < e.radius {
                    out.push(i);
                    counters.statics += 1;
                    counters.total += 1;
                    if counters.statics >= STATIC_LIMIT || counters.total >= TOTAL_LIMIT {
                        return out;
                    }
                    break;
                }
                reach = RETRY_RANGE;
            }
        }
        i += 1;
        left -= 1;
    }
    out
}
