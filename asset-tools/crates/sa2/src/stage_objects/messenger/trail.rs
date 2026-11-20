//! The ribbon-trail library every stage REL carries (stg42D.rel section 1,
//! decomp split `stg42D_8.c`): a ring buffer of recent points that is drawn
//! as a strip of quads, narrowing and fading from the newest point to the
//! oldest. Omochao (MSGER) uses it for the tips of its propeller.
//!
//! The stage's setup (stg42D +0x2EC8..+0x2F1C) registers three of these
//! routines in main.dol's stage callback table (GSNE8P bss 0x801E77D8, 0x48
//! bytes, by the disc REL's own relocation at +0x2ED0/+0x2ED4; the Steam
//! install's resource/gd_PC copy of stg42D.rel is linked against another
//! main.dol and names 0x801E73D8 there, so it must not be paired with the
//! GSNE8P main.dol): +4 = alloc (0x635C), +8 = step (0x6E50), +0xC = free
//! (0x6EA4). main.dol's own users call them through that table (for example
//! fn_800139EC allocates two buffers through +4 and fn_80013B04 frees them
//! through +0xC); stage code such as Omochao calls them directly.
//!
//! Routines (stg42D section 1): 0x5C30 midpoint, 0x635C alloc, 0x63D4,
//! 0x63DC and 0x6428 the three easing curves and 0x642C which picks one,
//! 0x6478 the untextured step, 0x6938 the textured step, 0x6E50 the step
//! dispatcher, 0x6EA4 free and 0x6EE4 drop-oldest.
//!
//! A step does two things, in this order:
//! 1. **Draw.** The current point is taken through the current matrix
//!    (njCalcPoint(NULL), 0x80114F0C). For each stored segment from the
//!    newest (i = count-1) down to i = 1, with `t = i / (count-1)` clamped to
//!    0..1 and `c = curve(t)`: the segment's 2D normal
//!    `(P[i-1].y - P[i].y, -P[i-1].x + P[i].x, 0)` (P indexed from the oldest)
//!    is scaled to half-width `tail + c * (head - tail)`; segments whose
//!    normal is not longer than 1.0 (njScalor, 0x801151F4) are skipped. The
//!    first drawn segment adds a pair of vertices at the current point in the
//!    head colour; each drawn segment adds a pair at its midpoint in
//!    `blend(head, tail, 1 - c)` (main.dol 0x800334B0). With three or more
//!    vertices the strip is drawn with an identity matrix (njPushMatrixEx,
//!    njUnitMatrix, draw, njPopMatrixEx).
//! 2. **Record.** Unless the game state (fn_8001CD40) is 0x11, the frames
//!    since the last step (`|frame - last|`, at most 4; frame is lbl_801CC168
//!    +0x7C) each append the current point, oldest overwritten when full;
//!    with flag 8 only frames with an odd number are kept (counted from the
//!    previous `last`). When no frame has passed and lbl_801CC168 +0x37 is set,
//!    the buffer is emptied first. `last` becomes the frame.
use crate::color_blend::blend_argb;
use crate::land_nodes::nj_scalor;
use crate::ninja_matrix::{MatrixStack, IDENTITY};
use crate::ninja_trig::{fctiwz, nj_sin};

pub type V3 = [f32; 3];

/// A trail's look (the descriptor its owner passes; Omochao's is
/// [`MESSENGER_TRAIL`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrailDesc {
    /// +0: bit 0 textured (0x6938, else 0x6478); bits 1-2 the curve
    /// (2 square, 4 sine, 0 or 6 linear); bit 3 every other frame.
    pub flags: u32,
    /// +4: half-width at the newest point.
    pub width_head: f32,
    /// +8: half-width at the oldest point.
    pub width_tail: f32,
    /// +0xC: ARGB at the newest point.
    pub color_head: u32,
    /// +0x10: ARGB at the oldest point.
    pub color_tail: u32,
    /// +0x18: the texture number the textured step sets (njSetTextureNum).
    pub texture: u32,
}

/// Omochao's trail descriptor, stg42D section 5 +0x173BC: flags 8, widths
/// 0.3 and 0.03, colours 0x3E8080FF and 0x08FF8080 (+0x18 is 0; +0x14 and
/// +0x1C are relocated pointers the step does not read).
pub const MESSENGER_TRAIL: TrailDesc = TrailDesc {
    flags: 8,
    width_head: f32::from_bits(0x3e99_999a),
    width_tail: f32::from_bits(0x3cf5_c28f),
    color_head: 0x3e80_80ff,
    color_tail: 0x08ff_8080,
    texture: 0,
};

/// A trail buffer (0x635C allocates the 0x14-byte header and the points).
#[derive(Debug, Clone, PartialEq)]
pub struct Trail {
    /// The header's allocation (handle from the host's syCalloc).
    pub id: u32,
    /// The points' allocation.
    pub points_id: u32,
    /// +0: the points (12 bytes each), +4: their number (the capacity).
    pub points: Vec<V3>,
    /// +8: the oldest point's slot.
    pub head: i32,
    /// +0xC: how many points are stored.
    pub count: i32,
    /// +0x10: the frame of the last step.
    pub last_frame: i32,
}

/// One vertex of an untextured strip (0x10 bytes: position, ARGB).
pub type PlainVertex = (V3, u32);
/// One vertex of a textured strip (0x18 bytes: position, u, v, ARGB).
pub type TexturedVertex = (V3, f32, f32, u32);

/// A strip a step draws (the vertices it built at 0x80FFFE60).
#[derive(Debug, Clone, PartialEq)]
pub enum TrailDraw {
    /// fn_8011A688(vertices, n, 1).
    Plain(Vec<PlainVertex>),
    /// njSetTextureNum(texture) (0x80116E34), then
    /// njDrawTexture3DEx(vertices, n, 1) (0x8011E368).
    Textured {
        texture: u32,
        vertices: Vec<TexturedVertex>,
    },
}

/// What the trail routines ask of the engine.
pub trait TrailHost {
    /// syCalloc(n, size) (0x801197FC): a handle, or None for NULL.
    fn sy_calloc(&mut self, n: u32, size: u32) -> Option<u32>;
    /// syFree(p) (0x80119828).
    fn sy_free(&mut self, id: u32);
    /// fn_8001CD40: the game state (a signed halfword).
    fn game_state(&mut self) -> i32;
    /// lbl_801CC168 +0x7C: the frame counter.
    fn frame(&mut self) -> i32;
    /// lbl_801CC168 +0x37 (a signed byte): set when a trail with no
    /// elapsed frame is to be emptied.
    fn trail_reset(&mut self) -> i8;
    /// A strip is drawn.
    fn trail_draw(&mut self, d: TrailDraw);
}

/// The game state in which steps record nothing (fn_8001CD40 == 0x11).
pub const STATE_NO_RECORD: i32 = 0x11;

/// 0x635C: allocate a trail of `n` points (the descriptor argument is not
/// read). syCalloc(1, 0x14) for the header, then syCalloc(n, 0xC) for the
/// points; when the second fails the header is freed and NULL returned.
pub fn alloc(n: i32, host: &mut impl TrailHost) -> Option<Trail> {
    let id = host.sy_calloc(1, 0x14)?;
    match host.sy_calloc(n as u32, 0xc) {
        Some(points_id) => Some(Trail {
            id,
            points_id,
            // (At most 0x10000 points are kept; the game asks for 8.)
            points: vec![[0.0; 3]; n.clamp(0, 0x1_0000) as usize],
            head: 0,
            count: 0,
            last_frame: 0,
        }),
        None => {
            host.sy_free(id);
            None
        }
    }
}

/// 0x6EA4: free a trail (its points, then the header); NULL does nothing.
pub fn free(trail: Option<Trail>, host: &mut impl TrailHost) {
    if let Some(t) = trail {
        host.sy_free(t.points_id);
        host.sy_free(t.id);
    }
}

/// 0x6EE4: drop the oldest point. Returns whether there was one.
pub fn drop_oldest(trail: &mut Trail) -> bool {
    if trail.count < 1 {
        return false;
    }
    trail.head = rem(trail.head.wrapping_add(1), trail.cap());
    trail.count -= 1;
    true
}

/// 0x5C30: `0.5 * (a + b)` per component.
pub fn midpoint(a: V3, b: V3) -> V3 {
    [0, 1, 2].map(|k| 0.5 * (a[k] + b[k]))
}

/// 0x642C: the easing curve for `flags & 6` (0x63D4 `t*t` for 2, 0x63DC
/// `njSin(int(182.04445 * (90 * t)))` for 4, 0x6428 `t` for 0 and 6).
pub fn curve(flags: u32, t: f32) -> f32 {
    match flags & 6 {
        2 => t * t,
        4 => {
            // s4 +0x2B4 (90.0) and +0x260 (182.04445, BAMS per degree).
            let deg = f32::from_bits(0x42b4_0000) * t;
            nj_sin(fctiwz(f32::from_bits(0x4336_0b61) * deg))
        }
        _ => t,
    }
}

impl Trail {
    /// +4, the capacity.
    pub fn cap(&self) -> i32 {
        self.points.len() as i32
    }

    fn point(&self, slot: i32) -> V3 {
        usize::try_from(slot)
            .ok()
            .and_then(|s| self.points.get(s).copied())
            .unwrap_or([0.0; 3])
    }
}

/// `a % b` as `divw`/`mullw`/`subf` compute it (truncating; 0 for b == 0).
fn rem(a: i32, b: i32) -> i32 {
    if b == 0 {
        0
    } else {
        a.wrapping_sub(a.wrapping_div(b).wrapping_mul(b))
    }
}

/// 0x6E50: step a trail. With no descriptor the buffer (when there is one)
/// is emptied (count 0); otherwise flag bit 0 picks 0x6938 or 0x6478.
/// The original reads through a NULL buffer when a descriptor is given;
/// here that does nothing.
pub fn step(
    desc: Option<&TrailDesc>,
    trail: Option<&mut Trail>,
    point: V3,
    stack: &mut MatrixStack,
    host: &mut impl TrailHost,
) {
    match (desc, trail) {
        (None, Some(t)) => t.count = 0,
        (Some(d), Some(t)) => step_desc(d, t, point, stack, host),
        _ => {}
    }
}

/// 0x6478 (flags bit 0 clear) and 0x6938 (set): draw, then record.
// The negated comparisons keep the clamps' NaN behaviour.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn step_desc(
    desc: &TrailDesc,
    trail: &mut Trail,
    point: V3,
    stack: &mut MatrixStack,
    host: &mut impl TrailHost,
) {
    let textured = desc.flags & 1 != 0;
    let cap = trail.cap();
    let p0 = stack.calc_point(point);
    let mut plain: Vec<PlainVertex> = Vec::new();
    let mut tex: Vec<TexturedVertex> = Vec::new();
    let mut emitted = 0u32;
    let last = trail.count.wrapping_sub(1);
    let denom = last as f32;
    let mut i = last;
    while i >= 1 {
        let mut t = i as f32 / denom;
        if !(t >= 0.0) {
            t = 0.0;
        }
        if !(t <= 1.0) {
            t = 1.0;
        }
        let c = curve(desc.flags, t);
        let fade = 1.0 - c;
        let color = blend_argb(desc.color_head, desc.color_tail, fade);
        // The textured step's u: 0.5 + 0.5 * (1 - c) (s4 +0x220).
        let u = 0.5 + 0.5 * fade;
        let k = i.wrapping_add(trail.head);
        let a = trail.point(rem(k, cap));
        let b = trail.point(rem(k.wrapping_sub(1), cap));
        let mut d = [b[1] - a[1], -b[0] + a[0], 0.0];
        let len = nj_scalor(d);
        if len > 1.0 {
            let w = (desc.width_tail + c * (desc.width_head - desc.width_tail)) / len;
            d = d.map(|x| x * w);
            let lo = |p: V3| [p[0] - d[0], p[1] - d[1], p[2] - d[2]];
            let hi = |p: V3| [p[0] + d[0], p[1] + d[1], p[2] + d[2]];
            let m = midpoint(b, a);
            if textured {
                if emitted == 0 {
                    tex.push((lo(p0), u, 0.0, desc.color_head));
                    tex.push((hi(p0), u, 1.0, desc.color_head));
                    emitted += 2;
                }
                tex.push((lo(m), u, 0.0, color));
                tex.push((hi(m), u, 1.0, color));
            } else {
                if emitted == 0 {
                    plain.push((lo(p0), desc.color_head));
                    plain.push((hi(p0), desc.color_head));
                    emitted += 2;
                }
                plain.push((lo(m), color));
                plain.push((hi(m), color));
            }
            emitted += 2;
        }
        i -= 1;
    }
    if emitted >= 3 {
        stack.push();
        *stack.current_mut() = IDENTITY;
        host.trail_draw(if textured {
            TrailDraw::Textured {
                texture: desc.texture,
                vertices: tex,
            }
        } else {
            TrailDraw::Plain(plain)
        });
        stack.pop();
    }
    record(desc, trail, p0, host);
}

/// The record half of 0x6478 / 0x6938 (their +0x390 / +0x3E4 onwards).
fn record(desc: &TrailDesc, trail: &mut Trail, p0: V3, host: &mut impl TrailHost) {
    if host.game_state() == STATE_NO_RECORD {
        return;
    }
    let frame = host.frame();
    let cap = trail.cap();
    let mut n = frame.wrapping_sub(trail.last_frame);
    if n < 0 {
        n = n.wrapping_neg();
    }
    if n >= 5 {
        n = 4;
    }
    if n == 0 && host.trail_reset() != 0 {
        trail.count = 0;
        trail.head = 0;
    }
    let mut at = trail.last_frame;
    // `mtctr`: frames exactly 2^31 apart give a count of i32::MIN, which the
    // original runs 2^31 times. Here it stops once every slot has been
    // written, so in that case only `head` may differ from the original.
    let times = (n as u32).min(cap.max(0) as u32 * 2 + 2);
    for _ in 0..times {
        if desc.flags & 8 == 0 || at & 1 != 0 {
            let slot = rem(trail.head.wrapping_add(trail.count), cap);
            if let Some(p) = usize::try_from(slot)
                .ok()
                .and_then(|s| trail.points.get_mut(s))
            {
                *p = p0;
            }
            if trail.count < cap {
                trail.count += 1;
            } else {
                trail.head = rem(trail.head.wrapping_add(1), cap);
            }
        }
        at = at.wrapping_add(1);
    }
    trail.last_frame = frame;
}
