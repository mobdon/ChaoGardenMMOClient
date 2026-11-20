//! Stage 42's display (draw) callbacks, ported from stg42D.rel:
//! DODAI platform display +0x19C8 and delete +0x1A58, FIREBALL meteor
//! display +0x25BC, the debris manager display +0x91A8, BgExec display
//! +0x312C, the rival gauge display +0x20700, and the title card glyph
//! (+0x2140C) and window (+0x21A00) displays with their render-state helpers
//! (+0x20DD4, +0x20E28). The module's own colour helpers are ported too:
//! +0x615C/+0x5D24 tint a chunk model's coloured vertex list into the scratch
//! buffer 0x80FFFE60, +0x61F0/+0x5FA0 tint a GC model's colour arrays there.
//!
//! The engine renderer is not executed. Every engine call is recorded in
//! order with its arguments (pointer arguments as the data they point to at
//! the call), every write the module makes to the scratch buffer or its own
//! data is recorded, and engine results (sine, floor, colour lerp) and
//! engine-owned state are request inputs. Models are read from the REL.
use crate::city_rel::{CityRel, Reference};
use crate::{duel_platform, Error, Result};
use serde::Deserialize;
use serde_json::{json, Value};

const MODULE: u32 = 41;
const NO_RETURN: &str = "the native display loops forever on this model data";

/// A float given as a number or as `"0x…"` single-precision bits.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(try_from = "FloatIn")]
pub struct F(pub f32);
#[derive(Deserialize)]
#[serde(untagged)]
enum FloatIn {
    Bits(String),
    Num(f64),
}
impl TryFrom<FloatIn> for F {
    type Error = String;
    fn try_from(v: FloatIn) -> std::result::Result<Self, String> {
        match v {
            FloatIn::Num(n) => Ok(F(n as f32)),
            FloatIn::Bits(s) => u32::from_str_radix(s.trim_start_matches("0x"), 16)
                .map(|b| F(f32::from_bits(b)))
                .map_err(|e| e.to_string()),
        }
    }
}
fn v3f(v: [F; 3]) -> [f32; 3] {
    [v[0].0, v[1].0, v[2].0]
}

fn b(v: f32) -> u32 {
    v.to_bits()
}
fn bits3(v: [f32; 3]) -> Value {
    json!([b(v[0]), b(v[1]), b(v[2])])
}
fn s5(o: u32) -> String {
    format!("s5+0x{o:x}")
}
/// PPC fctiwz: truncate, saturate, NaN gives 0x80000000.
fn fctiwz(v: f32) -> i32 {
    if v.is_nan() {
        i32::MIN
    } else {
        v as i32
    }
}

#[derive(Default)]
pub struct Rec {
    pub out: Vec<Value>,
}
impl Rec {
    fn call(&mut self, a: u32, args: Value) {
        self.out
            .push(json!({ "engine": format!("0x{a:08x}"), "args": args }));
    }
    fn write(&mut self, at: String, value: Value) {
        self.out.push(json!({ "write": at, "value": value }));
    }
}

pub struct Display<'a> {
    rel: CityRel<'a>,
}
impl<'a> Display<'a> {
    pub fn new(rel_bytes: &'a [u8]) -> Result<Self> {
        Ok(Self {
            rel: duel_platform::module(rel_bytes)?,
        })
    }
    fn word(&self, s: u8, at: u32) -> Result<u32> {
        // A relocated word is a pointer at run time, not this data.
        if self.rel.reference(s, at).is_some() {
            return Err(Error::invalid(
                "stage 42 display data is a relocated pointer",
            ));
        }
        let at = at as usize;
        self.rel
            .data(s)?
            .get(at..at + 4)
            .map(|x| u32::from_be_bytes(x.try_into().unwrap()))
            .ok_or_else(duel_platform::invalid)
    }
    fn half(&self, s: u8, at: u32) -> Result<u16> {
        Ok((self.word(s, at & !3)? >> (16 - 8 * (at & 2))) as u16)
    }
    fn byte(&self, s: u8, at: u32) -> Result<u8> {
        Ok((self.word(s, at & !3)? >> (24 - 8 * (at & 3))) as u8)
    }
    fn f(&self, s: u8, at: u32) -> Result<f32> {
        Ok(f32::from_bits(self.word(s, at)?))
    }
    fn t(&self, pc: u32, s: u8) -> Result<u32> {
        duel_platform::target(&self.rel, pc, s)
    }
    fn k(&self, pc: u32) -> Result<f32> {
        duel_platform::float(&self.rel, pc)
    }
    /// The section 5 target of a relocated data pointer at section 5 `at`.
    fn ptr(&self, at: u32) -> Result<u32> {
        match self.rel.reference(5, at) {
            Some(Reference {
                module: MODULE,
                kind: 1,
                section: 5,
                addend,
            }) => Ok(addend),
            _ => Err(Error::invalid(
                "stage 42 model pointer outside the module data",
            )),
        }
    }
    fn center(&self, at: u32) -> Result<[f32; 3]> {
        Ok([self.f(5, at)?, self.f(5, at + 4)?, self.f(5, at + 8)?])
    }

    /// +0x5D24: copy a chunk vertex list of type 0x23 into the scratch buffer,
    /// multiplying each vertex colour channel by `color`'s.
    fn copy_d8(&self, r: &mut Rec, src: u32, color: u32) -> Result<()> {
        let mut at = src;
        let mut out = 0u32;
        let mut put = |r: &mut Rec, v: u32| {
            r.write(format!("scratch+0x{out:x}"), json!(v));
            out += 4;
        };
        put(r, self.word(5, at)?);
        let head = self.word(5, at + 4)?;
        put(r, head);
        at += 8;
        for _ in 0..head >> 16 {
            for _ in 0..3 {
                put(r, self.word(5, at)?);
                at += 4;
            }
            let c = self.word(5, at)?;
            at += 4;
            let mut m = 0;
            for i in 0..4 {
                let p = ((c >> (8 * i)) & 0xff) * ((color >> (8 * i)) & 0xff);
                m |= ((p >> 8) & 0xff) << (8 * i);
            }
            put(r, m);
        }
        let end = self.word(5, at)?;
        put(r, end);
        if end != 0xff {
            return Err(Error::invalid(NO_RETURN));
        }
        Ok(())
    }
    /// +0x5FA0: `count` colour words into the scratch buffer, each channel
    /// multiplied by the colour rotated left by 8 bits.
    fn copy_colors(&self, r: &mut Rec, src: u32, count: u16, color: u32) -> Result<()> {
        let k = color.rotate_left(8);
        for n in 0..count as u32 {
            let c = self.word(5, src + 4 * n)?;
            let mut m = 0;
            for i in 0..4 {
                let p = ((c >> (8 * i)) & 0xff) * ((k >> (8 * i)) & 0xff);
                m |= ((p >> 8) & 0xff) << (8 * i);
            }
            r.write(format!("scratch+0x{:x}", 4 * n), json!(m));
        }
        Ok(())
    }
    /// 0x8011DE10 for a chunk model, recording its vertex list.
    fn draw_chunk(&self, r: &mut Rec, model: u32) -> Result<()> {
        r.call(0x8011de10, json!([s5(model), s5(self.ptr(model)?)]));
        Ok(())
    }
    /// +0x615C: draw a chunk model tinted by `color`.
    fn tinted(&self, r: &mut Rec, model: u32, color: u32) -> Result<()> {
        let list = self.ptr(model)?;
        if color != 0xffff_ffff && self.word(5, list)? & 0xffff == 0x23 {
            self.copy_d8(r, list, color)?;
            r.write(s5(model), json!("scratch"));
            r.call(0x8011de10, json!([s5(model), "scratch"]));
            r.write(s5(model), json!(s5(list)));
            Ok(())
        } else {
            self.draw_chunk(r, model)
        }
    }
    /// +0x61F0: draw a GC model once per tinted colour array.
    fn tinted_gc(&self, r: &mut Rec, model: u32, color: u32) -> Result<()> {
        let mut e = self.ptr(model)?;
        loop {
            let kind = self.byte(5, e)?;
            if kind == 0xff {
                return Ok(());
            }
            if kind == 3 && self.byte(5, e + 7)? == 0xa8 {
                let count = self.half(5, e + 2)?;
                let data = self.ptr(e + 8)?;
                r.write(s5(e + 8), json!("scratch"));
                self.copy_colors(r, data, count, color)?;
                r.call(0x800317c0, json!([s5(model)]));
                r.write(s5(e + 8), json!(s5(data)));
            }
            e += 0x10;
        }
    }

    /// +0x19C8 for the DODAI collision object (+8 position, +0x14 angles,
    /// model copy +0x24 radius).
    pub fn platform(&self, r: &mut Rec, o: &PlatformObject) -> Result<()> {
        let p = v3f(o.position);
        r.call(0x8002b080, json!([0x2400]));
        r.call(0x80116ad0, json!([s5(self.t(0x19ea, 5)?)]));
        r.call(0x80115d50, json!([]));
        r.call(0x80115e24, json!([bits3(p)]));
        r.call(0x80115c20, json!([o.angles, 0]));
        r.call(0x800665b8, json!([0xff, bits3(p), b(o.radius.0)]));
        r.call(0x8006633c, json!([bits3(p), o.angles]));
        r.call(0x8012181c, json!([s5(self.t(0x1a2e, 5)?)]));
        r.call(0x80066224, json!([]));
        r.call(0x80115d9c, json!([]));
        r.call(0x8002b070, json!([0x2400]));
        Ok(())
    }
    /// +0x1A58: free the collision object (task +0x40) and its model copy.
    pub fn platform_delete(&self, r: &mut Rec, d: &PlatformDelete) {
        if !d.object {
            return;
        }
        if d.model {
            r.call(0x80022f84, json!(["task", "object"]));
            r.call(0x801194e0, json!(["model"]));
            r.write("object+4".into(), json!(0));
        }
        r.call(0x801194e0, json!(["object"]));
        r.write("task+0x40".into(), json!(0));
    }
    /// +0x25BC for a meteor's work (+6 timer, +8 angles, +0x14 position).
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    pub fn meteor(&self, r: &mut Rec, m: &Meteor) -> Result<()> {
        r.call(0x80116ad0, json!([s5(self.t(0x25d6, 5)?)]));
        r.call(0x80115d50, json!([]));
        r.call(0x80115e24, json!([bits3(v3f(m.position))]));
        r.call(0x80115c20, json!([m.angles, 0]));
        let model = self.ptr(self.t(0x264e, 5)? + 4)?;
        if m.timer != 0 {
            let sin = m.sin.0;
            r.call(0x80112050, json!([m.timer as u32 * 0x500, b(sin)]));
            let c = self.k(0x260e)?;
            // fcmpo + bge: keep the sine unless it is below the floor.
            let f1 = if sin < c { c } else { sin };
            r.call(
                0x80033248,
                json!([0xffff_ffffu32, 0xffff_2020u32, b(f1), m.color]),
            );
            self.tinted(r, model, m.color)?;
        } else {
            self.draw_chunk(r, model)?;
        }
        r.call(0x80115d9c, json!([]));
        Ok(())
    }
    /// +0x91A8 over the debris pieces in list order.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    pub fn debris(&self, r: &mut Rec, d: &Debris) -> Result<()> {
        let fade = self.k(0x91de)?;
        let restore = self.k(0x91d6)?;
        let alpha = self.k(0x91e6)?;
        r.call(0x80115d50, json!([]));
        for p in &d.pieces {
            let object = || {
                p.object
                    .ok_or_else(|| Error::invalid("piece without an object"))
            };
            let model = || {
                p.model
                    .ok_or_else(|| Error::invalid("piece without a model"))
            };
            let gc = p.flags & 0x8000 != 0;
            let life = p.life.0;
            // fcmpo + bge: NaN life is not fading.
            let low = life < fade;
            r.call(0x80116ad0, json!([p.texlist]));
            r.call(0x80115044, json!([0, bits3(v3f(p.position))]));
            r.call(0x80115c20, json!([p.angles, 0]));
            if low {
                r.call(0x8002b09c, json!([]));
                r.call(0x8002b0f4, json!([]));
                r.call(0x8002b070, json!([0x220]));
                r.call(0x8002b080, json!([0x810]));
                r.call(0x80121580, json!([0xff, 0xa00]));
                r.call(0x800156fc, json!([b(life), b(fade), b(fade), b(fade)]));
            }
            let c = if gc {
                self.center(model()? + 0x14)?
            } else {
                self.center(object()? + 8)?
            };
            r.call(0x80115890, json!([0, bits3([-c[0], -c[1], -c[2]])]));
            if p.flags & 0x80000 != 0 {
                r.call(0x80024a70, json!([3]));
            }
            if p.flags & 0x2000 != 0 && low {
                let a = fctiwz(alpha * life).clamp(0, 0xff) as u32;
                let color = (a << 24).wrapping_add(0x00ff_ffff);
                if gc {
                    self.tinted_gc(r, model()?, color)?;
                } else {
                    self.tinted(r, object()?, color)?;
                }
            } else {
                if gc {
                    r.call(0x80121894, json!([s5(model()?)]));
                } else if p.flags & 0x10000 != 0 {
                    r.call(0x8011dbd8, json!([s5(object()?)]));
                } else {
                    self.draw_chunk(r, object()?)?;
                }
                r.call(0x801176b0, json!([0, 8]));
                r.call(0x801176b0, json!([1, 6]));
            }
            if p.flags & 0x80000 != 0 {
                r.call(0x80024a70, json!([d.game_byte as i32 as u32]));
            }
            if low {
                r.call(
                    0x800156fc,
                    json!([b(restore), b(restore), b(restore), b(restore)]),
                );
                r.call(0x8002b0e0, json!([]));
                r.call(0x8002b090, json!([]));
            }
            r.call(0x80115d9c, json!([]));
            r.call(0x80115d50, json!([]));
        }
        r.call(0x80115d9c, json!([]));
        Ok(())
    }
    /// One spinning background object (+8 position, +4 model) for BgExec.
    fn bg_spinner(
        &self,
        r: &mut Rec,
        obj: u32,
        rates: [u32; 2],
        frame: u32,
        offset: u32,
        scale: f32,
    ) -> Result<()> {
        let model = self.ptr(obj + 4)?;
        let c = self.center(model + 8)?;
        r.call(0x80115e24, json!([bits3(self.center(obj + 8)?)]));
        r.call(0x80115890, json!([0, bits3(c)]));
        r.call(
            0x80115918,
            json!([0, rates[0].wrapping_mul(frame).wrapping_add(offset)]),
        );
        r.call(0x801154e0, json!([0, rates[1].wrapping_mul(frame)]));
        r.call(0x80114e30, json!([0, bits3([scale; 3])]));
        r.call(0x80115890, json!([0, bits3([-c[0], -c[1], -c[2]])]));
        self.draw_chunk(r, model)?;
        r.call(0x80115d9c, json!([]));
        Ok(())
    }
    /// +0x312C, BgExec's display.
    pub fn background(&self, r: &mut Rec, g: &Background) -> Result<()> {
        let (mode, screen) = (g.game_byte, g.screen);
        if (mode == 1 && screen == 0) || (mode == 2 && screen == 1) || mode == 3 {
            return Ok(());
        }
        let w5 = |pc| -> Result<u32> { self.word(5, self.t(pc, 5)?) };
        let f5 = |pc| -> Result<f32> { Ok(f32::from_bits(w5(pc)?)) };
        let step = f5(0x31b6)?;
        let (k1d0, k1d4, k1d8) = (self.k(0x31f2)?, self.k(0x321a)?, self.k(0x323a)?);
        let cam = v3f(g.camera);
        r.call(0x80115d50, json!([]));
        r.call(0x80116ad0, json!([s5(self.t(0x31aa, 5)?)]));
        r.call(0x80110ef8, json!([b(cam[0] / step), b(g.floor.0)]));
        let mut v = [step * g.floor.0 - k1d0 * step, g.bss8.0, k1d4];
        let ring = f5(0x3232)?;
        let tile = s5(self.t(0x3242, 5)?);
        for _ in 0..6 {
            r.call(0x80115e24, json!([bits3(v)]));
            r.call(0x80114e30, json!([0, bits3([ring, k1d8, ring])]));
            r.call(0x8011de34, json!([tile]));
            r.call(0x80115d9c, json!([]));
            r.call(0x80115d50, json!([]));
            v[0] += step;
        }
        r.call(0x8011bf74, json!([]));
        r.call(0x8011bfc8, json!([]));
        r.call(0x80115e24, json!([bits3(cam)]));
        for (tex, y, sc, obj) in [
            (0x32ca, f5(0x32e6)?, 0x32fa, 0x331e),
            (0x3332, g.bss4.0, 0x335a, 0x3376),
        ] {
            let s = f5(sc)?;
            r.call(0x80115d50, json!([]));
            r.call(0x80116ad0, json!([s5(self.t(tex, 5)?)]));
            r.call(0x80115890, json!([0, bits3([k1d4, y, k1d4])]));
            r.call(0x80114e30, json!([0, bits3([s, k1d8, s])]));
            r.call(0x8011de34, json!([s5(self.t(obj, 5)?)]));
            r.call(0x80115d9c, json!([]));
        }
        let scale = f5(0x3406)?;
        r.call(0x80115d50, json!([]));
        r.call(0x80116ad0, json!([s5(self.t(0x338a, 5)?)]));
        self.bg_spinner(
            r,
            self.t(0x3396, 5)?,
            [w5(0x33c2)?, w5(0x33ee)?],
            g.frame,
            0x3000,
            scale,
        )?;
        r.call(0x80115d50, json!([]));
        r.call(0x80116ad0, json!([s5(self.t(0x3456, 5)?)]));
        // The last spinner is not popped before the engine call.
        let obj = self.t(0x3462, 5)?;
        let model = self.ptr(obj + 4)?;
        let c = self.center(model + 8)?;
        r.call(0x80115e24, json!([bits3(self.center(obj + 8)?)]));
        r.call(0x80115890, json!([0, bits3(c)]));
        r.call(
            0x80115918,
            json!([0, w5(0x348e)?.wrapping_mul(g.frame).wrapping_add(0x3000)]),
        );
        r.call(0x801154e0, json!([0, w5(0x34ae)?.wrapping_mul(g.frame)]));
        r.call(0x80114e30, json!([0, bits3([scale; 3])]));
        r.call(0x80115890, json!([0, bits3([-c[0], -c[1], -c[2]])]));
        self.draw_chunk(r, model)?;
        r.call(0x80115a80, json!([2]));
        r.call(0x8011bf68, json!([]));
        r.call(0x8011bfc8, json!([]));
        Ok(())
    }

    /// +0x20700, the rival gauge's display.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    pub fn gauge(&self, r: &mut Rec, g: &GaugeDisplay) -> Result<()> {
        if g.menu != 0 {
            return Ok(());
        }
        let words = |pc: u32, n: u32| -> Result<Vec<u32>> {
            let at = self.t(pc, 4)?;
            (0..n).map(|i| self.word(4, at + 4 * i)).collect()
        };
        let anim0 = words(0x20726, 5)?;
        let tmpl = words(0x20756, 8)?;
        let base = words(0x2079e, 3)?;
        let tex = s5(self.t(0x207e6, 5)?);
        let (a80, a84, a88) = (self.k(0x2098e)?, self.k(0x2087a)?, self.k(0x208ba)?);
        let (ad0, ad4, ad8) = (self.k(0x20966)?, self.k(0x20996)?, self.k(0x20986)?);
        let (adc, ae0, ae4) = (self.k(0x20c02)?, self.k(0x20c6e)?, self.k(0x20cd6)?);
        // tanim halfwords (+0 sx, +2, +4 cx, +6, ..., +0x10 texture id).
        let mut h = [0u16; 10];
        for (i, w) in anim0.iter().enumerate() {
            h[2 * i] = (w >> 16) as u16;
            h[2 * i + 1] = *w as u16;
        }
        let f = |w: u32| f32::from_bits(w);
        let template = [f(base[0]), f(base[1]), f(base[2])];
        let rest = [tmpl[3], tmpl[4], tmpl[5]];
        let mut s = template;
        let draw = |r: &mut Rec, s: &[f32; 3], h: &[u16; 10], z: f32, attr: u32| {
            let anim: Vec<u32> = (0..5)
                .map(|i| ((h[2 * i] as u32) << 16) | h[2 * i + 1] as u32)
                .collect();
            r.call(
                0x8011e59c,
                json!([
                    [
                        b(s[0]),
                        b(s[1]),
                        b(s[2]),
                        rest[0],
                        rest[1],
                        rest[2],
                        tex,
                        "tanim"
                    ],
                    anim,
                    0,
                    b(z),
                    attr
                ]),
            );
        };
        r.call(0x8011bf74, json!([]));
        r.call(0x8011bfc8, json!([]));
        r.call(0x80116ad0, json!([tex]));
        r.call(0x801176b0, json!([0, 8]));
        r.call(0x801176b0, json!([1, 6]));
        let w = &g.work;
        if w.mode != 3 {
            let n = fctiwz(a84 + a84 * w.start.0) as i8 as i32;
            let mut i: i32 = 0;
            while (i as i8 as i32) < n {
                let ii = i as i8 as i32;
                h[8] = match ii {
                    1 => 4,
                    0 => 5,
                    _ if ii == n - 2 => 2,
                    _ if ii == n - 1 => 1,
                    _ => 3,
                };
                draw(r, &s, &h, s[2], 0x20);
                s[0] -= a88;
                i += 1;
            }
        } else {
            let mut i = 0;
            let mut f31 = ad0 + w.shown.0;
            while f31 >= a80 {
                let mut f30 = a80;
                if i == 0 {
                    h[8] = 5;
                } else if f31 <= a88 {
                    h[8] = 1;
                    f31 = ad4;
                } else {
                    h[8] = 3;
                }
                if f31 <= ad0 && f31 > a88 {
                    if i == 1 {
                        let d = f31 - a88;
                        s[0] = template[0] - d;
                        h[0] = fctiwz(d) as u16;
                        h[2] = fctiwz(ad8 * d) as u16;
                        f30 = d;
                    } else {
                        s[0] += ad0 - f31;
                    }
                    f31 = a88;
                }
                draw(r, &s, &h, s[2], 0x20);
                if a80 == f30 {
                    s[0] -= a88;
                } else {
                    s[0] -= f30;
                }
                f31 -= a88;
                i += 1;
            }
        }
        // The sprite template's position was replaced by the base position.
        s = template;
        s[1] += a88;
        h[8] = 10;
        h[0] = 0x10;
        for _ in 0..5 {
            draw(r, &s, &h, s[2], 0x20);
            s[0] -= a88;
            h[8] = (h[8] as i16).wrapping_sub(1) as u16;
        }
        if w.mode != 0 && w.mode != 3 {
            s = template;
            s[0] -= adc;
            h[8] = 0;
            h[2] = 0;
            h[1] = 0xa;
            h[3] = 5;
            let shown = w.shown.0;
            let (f2, f4) = if shown <= ad0 && w.mode >= 2 {
                (ae0, a80)
            } else {
                (a80, ae0)
            };
            r.call(0x800156fc, json!([b(ae0), b(f2), b(a80), b(f4)]));
            let mut f30 = shown;
            while f30 >= a80 {
                if shown <= a88 {
                    s[0] -= shown;
                    h[0] = fctiwz(shown) as u16;
                    h[2] = 0;
                    f30 = ad4;
                } else {
                    s[0] -= a88;
                    if f30 <= a88 {
                        s[0] += a88 - f30;
                        f30 = ad4;
                    }
                }
                draw(r, &s, &h, ae4 + s[2], 0x22);
                f30 -= a88;
            }
            r.call(0x800156fc, json!([b(a80), b(a80), b(a80), b(a80)]));
        }
        r.call(0x8011bf68, json!([]));
        r.call(0x8011bfc8, json!([]));
        Ok(())
    }

    /// +0x20DD4
    fn title_begin(&self, r: &mut Rec, texlist: u32) {
        r.call(0x8011bf74, json!([]));
        r.call(0x8011bfc8, json!([]));
        r.call(0x80117938, json!([0]));
        r.call(0x801176b0, json!([0, 8]));
        r.call(0x801176b0, json!([1, 6]));
        r.call(0x80116ad0, json!([texlist]));
    }
    /// +0x20E28
    fn title_end(&self, r: &mut Rec) {
        r.call(0x80117938, json!([0x2000]));
        r.call(0x8011bf68, json!([]));
        r.call(0x8011bfc8, json!([]));
    }
    fn quad(&self, pc: u32) -> Result<[u32; 24]> {
        let at = self.t(pc, 4)?;
        let mut q = [0; 24];
        for (i, w) in q.iter_mut().enumerate() {
            *w = self.word(4, at + 4 * i as u32)?;
        }
        Ok(q)
    }
    /// +0x2140C for one glyph child.
    pub fn title_glyph(&self, r: &mut Rec, t: &TitleGlyph) -> Result<()> {
        if t.state.menu != 0 || t.state.enabled == 0 {
            return Ok(());
        }
        if t.state.parent.is_some_and(|p| p.hidden) {
            return Ok(());
        }
        if t.state.parent.is_some_and(|p| p.first) {
            self.title_begin(r, t.state.texlist);
        }
        let g = &t.glyph;
        let mut q = self.quad(0x21442)?;
        let (w, h) = (g.width as f32, g.height as f32);
        for v in q.chunks_mut(6) {
            v[0] = b(g.x.0 + g.scale.0 * (f32::from_bits(v[0]) * w));
            v[1] = b(g.y.0 + g.scale.0 * (f32::from_bits(v[1]) * h));
            v[3] = b(f32::from_bits(v[3]) * g.sx.0);
            v[4] = b(f32::from_bits(v[4]) * g.sy.0);
            v[5] = g.color;
        }
        r.call(0x8011c1c0, json!([q.to_vec(), 4, g.code, 1]));
        if t.state.parent.is_some_and(|p| p.last) {
            self.title_end(r);
        }
        Ok(())
    }
    /// +0x21A00 for the window child.
    pub fn title_window(&self, r: &mut Rec, t: &TitleWindow) -> Result<()> {
        if t.state.menu != 0 || t.state.enabled == 0 {
            return Ok(());
        }
        if t.state.parent.is_some_and(|p| p.hidden) {
            return Ok(());
        }
        let w = &t.window;
        let scale = w.scale.0;
        let a = (fctiwz(self.k(0x21aae)? * w.alpha.0) as u32 & 0xff) << 24;
        let color = w.color.wrapping_add(a);
        let mut q = self.quad(0x21a2e)?;
        for v in q.chunks_mut(6) {
            v[0] = b(w.x.0 + f32::from_bits(v[0]) * scale);
            v[1] = b(w.y.0 + f32::from_bits(v[1]) * scale);
            v[5] = color;
        }
        if t.state.parent.is_some_and(|p| p.first) {
            self.title_begin(r, t.state.texlist);
        }
        let step = self.k(0x21baa)? * scale;
        let mut k = 0u32;
        while k < w.count as i32 as u32 {
            let code = *w
                .line
                .get(k as usize)
                .ok_or_else(|| Error::invalid("window line shorter than its count"))?;
            r.call(0x8011c1c0, json!([q.to_vec(), 4, code, 1]));
            for v in q.chunks_mut(6) {
                v[0] = b(f32::from_bits(v[0]) + step);
            }
            k += 1;
        }
        if t.state.parent.is_some_and(|p| p.last) {
            self.title_end(r);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PlatformObject {
    pub position: [F; 3],
    pub angles: [i32; 3],
    /// Model copy +0x24.
    pub radius: F,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PlatformDelete {
    /// Task +0x40 and object +4 are non-null.
    pub object: bool,
    pub model: bool,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Meteor {
    pub timer: u16,
    pub angles: [i32; 3],
    pub position: [F; 3],
    /// 0x80112050 result for timer x 0x500.
    #[serde(default = "zero")]
    pub sin: F,
    /// 0x80033248 result.
    #[serde(default)]
    pub color: u32,
}
fn zero() -> F {
    F(0.0)
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Piece {
    pub position: [F; 3],
    pub angles: [i32; 3],
    pub texlist: u32,
    /// Section 5 offsets of the chunk model (+0x34) and GC model (+0x38).
    #[serde(default)]
    pub object: Option<u32>,
    #[serde(default)]
    pub model: Option<u32>,
    pub flags: u32,
    pub life: F,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Debris {
    /// The list from section 6 +0x63C, following +0x4C.
    pub pieces: Vec<Piece>,
    /// Signed byte 0x803AD526.
    pub game_byte: i8,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Background {
    /// Signed game byte +0x3D and the screen index 0x803AD6D0.
    pub game_byte: i8,
    pub screen: i32,
    /// The camera position the 0x80174F98 table gives for the screen.
    pub camera: [F; 3],
    /// 0x80110EF8 result.
    pub floor: F,
    /// Section 6 +4 and +8.
    pub bss4: F,
    pub bss8: F,
    /// Game word +0x7C.
    pub frame: u32,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GaugeWork {
    pub mode: i8,
    /// +0x14
    pub shown: F,
    /// +0x28
    pub start: F,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GaugeDisplay {
    /// Signed game byte +0x39.
    pub menu: i8,
    pub work: GaugeWork,
}
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Parent {
    /// Parent work +4 bit 0x200.
    pub hidden: bool,
    /// Parent +0xC is this task.
    pub first: bool,
    /// That child's +4 is this task.
    pub last: bool,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TitleState {
    /// Signed game bytes +0x39 and +0xB.
    pub menu: i8,
    pub enabled: i8,
    #[serde(default)]
    pub parent: Option<Parent>,
    /// Section 6 +0x6D4.
    #[serde(default)]
    pub texlist: u32,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Glyph {
    /// Work +0x14, +0x18, +0x20, +0x24, +0x28 and +0xC.
    pub x: F,
    pub y: F,
    pub scale: F,
    pub sx: F,
    pub sy: F,
    pub color: u32,
    /// The line entry at work +8: texture, width, height.
    pub code: u32,
    pub width: i16,
    pub height: i16,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TitleGlyph {
    #[serde(flatten)]
    pub state: TitleState,
    pub glyph: Glyph,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Window {
    /// Work +1, +0x14, +0x18, +0x20, +0x28 and +0xC.
    pub count: i8,
    pub x: F,
    pub y: F,
    pub scale: F,
    pub alpha: F,
    pub color: u32,
    /// Texture word of each 16-byte entry at work +8.
    pub line: Vec<u32>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TitleWindow {
    #[serde(flatten)]
    pub state: TitleState,
    pub window: Window,
}

#[derive(Deserialize)]
#[serde(tag = "callback", rename_all = "camelCase")]
enum Request {
    Platform(PlatformObject),
    PlatformDelete(PlatformDelete),
    Meteor(Meteor),
    Debris(Debris),
    Background(Background),
    Gauge(GaugeDisplay),
    TitleGlyph(TitleGlyph),
    TitleWindow(TitleWindow),
}

pub fn replay(rel_bytes: &[u8], request: &[u8]) -> Result<Value> {
    let d = Display::new(rel_bytes)?;
    let req: Request = serde_json::from_slice(request)
        .map_err(|e| Error::invalid(format!("display request: {e}")))?;
    let mut r = Rec::default();
    match &req {
        Request::Platform(o) => d.platform(&mut r, o)?,
        Request::PlatformDelete(o) => d.platform_delete(&mut r, o),
        Request::Meteor(m) => d.meteor(&mut r, m)?,
        Request::Debris(x) => d.debris(&mut r, x)?,
        Request::Background(g) => d.background(&mut r, g)?,
        Request::Gauge(g) => d.gauge(&mut r, g)?,
        Request::TitleGlyph(t) => d.title_glyph(&mut r, t)?,
        Request::TitleWindow(t) => d.title_window(&mut r, t)?,
    }
    Ok(json!({ "calls": r.out }))
}
