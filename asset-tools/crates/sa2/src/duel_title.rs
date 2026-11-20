//! The boss title card system the stage 42 director opens (`BossTitleExec`):
//! create (+0x22070), update (+0x21DE4), delete (+0x2200C), the per-glyph
//! child task (create +0x217A8, update +0x211FC), the optional window child
//! (create +0x21C60, update +0x218AC) and the layout helpers (+0x21D48 line
//! copy, +0x20FB4 line width, +0x20E54 glyph scale built on the inline
//! square root at +0x74CC). Its displays (+0x2140C, +0x21A00) are in
//! `duel_display`. Engine results and the work words the engine
//! owns are inputs.
use crate::{duel_platform, Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// One 16-byte line entry: glyph code, width, height, delay, scale terms.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Glyph {
    pub code: u32,
    pub width: i16,
    pub height: i16,
    pub delay: u16,
    pub scale_x: i16,
    pub scale_y: i16,
    pub rest: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Descriptor {
    pub line: Option<Vec<Glyph>>,
    pub count: i16,
    pub spacing: i16,
    pub delay: i16,
    pub window: Option<Vec<Glyph>>,
    pub window_count: i16,
    pub flags: u16,
    pub life: u16,
    pub texlist: u32,
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub color: u32,
}

/// Read a descriptor from the module (section 5 offset), resolving its tables.
pub fn descriptor(rel_bytes: &[u8], at: u32) -> Result<Descriptor> {
    let rel = duel_platform::module(rel_bytes)?;
    let d = rel.data(5)?;
    let w = |o: u32| -> Result<u32> {
        let o = (at + o) as usize;
        d.get(o..o + 4)
            .map(|b| u32::from_be_bytes(b.try_into().unwrap()))
            .ok_or_else(duel_platform::invalid)
    };
    let h = |o: u32| -> Result<u16> { Ok((w(o & !3)? >> if o & 2 == 0 { 16 } else { 0 }) as u16) };
    let table = |o: u32, n: i16| -> Result<Option<Vec<Glyph>>> {
        match rel.local_pointer(5, at + o)? {
            None => Ok(None),
            Some((5, t)) => (0..n.max(0) as u32)
                .map(|i| {
                    let e = t + 16 * i;
                    let g = |k: u32| -> Result<u32> {
                        let k = (e + k) as usize;
                        d.get(k..k + 4)
                            .map(|b| u32::from_be_bytes(b.try_into().unwrap()))
                            .ok_or_else(duel_platform::invalid)
                    };
                    let (a, b, c) = (g(4)?, g(8)?, g(12)?);
                    Ok(Glyph {
                        code: g(0)?,
                        width: (a >> 16) as i16,
                        height: a as i16,
                        delay: (b >> 16) as u16,
                        scale_x: b as i16,
                        scale_y: (c >> 16) as i16,
                        rest: c as u16,
                    })
                })
                .collect::<Result<Vec<_>>>()
                .map(Some),
            _ => Err(duel_platform::invalid()),
        }
    };
    let count = h(4)? as i16;
    let window_count = h(0x10)? as i16;
    Ok(Descriptor {
        line: table(0, count)?,
        count,
        spacing: h(6)? as i16,
        delay: h(8)? as i16,
        window: table(0xc, window_count)?,
        window_count,
        flags: h(0x12)?,
        life: h(0x14)?,
        texlist: match rel.local_pointer(5, at + 0x18)? {
            Some((5, t)) => t,
            None => 0,
            _ => return Err(duel_platform::invalid()),
        },
        x: f32::from_bits(w(0x1c)?),
        y: f32::from_bits(w(0x20)?),
        scale: f32::from_bits(w(0x24)?),
        color: w(0x28)?,
    })
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TitleConstants {
    pub zero: f32,
    pub one: f32,
    pub half: f32,
    pub glyph_k: f32,
    pub window_rate: f32,
    pub alpha: f32,
    pub fade_grow: f32,
    pub sqrt_floor: f32,
    pub sqrt_half: f64,
    pub sqrt_three: f64,
}
pub fn constants(rel_bytes: &[u8]) -> Result<TitleConstants> {
    let rel = duel_platform::module(rel_bytes)?;
    let f = |pc| duel_platform::float(&rel, pc);
    let dbl = |pc| -> Result<f64> {
        let at = duel_platform::target(&rel, pc, 4)? as usize;
        let b = rel
            .data(4)?
            .get(at..at + 8)
            .ok_or_else(duel_platform::invalid)?;
        Ok(f64::from_bits(u64::from_be_bytes(b.try_into().unwrap())))
    };
    Ok(TitleConstants {
        zero: f(0x20fbe)?,
        one: f(0x21932)?,
        half: f(0x2233e)?,
        glyph_k: f(0x21826)?,
        window_rate: f(0x21946)?,
        alpha: f(0x212ae)?,
        fade_grow: f(0x21fa6)?,
        sqrt_floor: f(0x74d6)?,
        sqrt_half: dbl(0x74ee)?,
        sqrt_three: dbl(0x74fe)?,
    })
}

/// +0x74CC: frsqrte (taken as the correctly rounded 1/sqrt, as QEMU does),
/// three Newton steps in double precision, then frsp.
pub fn module_sqrt(c: &TitleConstants, x: f32) -> f32 {
    let x64 = x as f64;
    if !(x64 > c.sqrt_floor as f64) {
        return x;
    }
    let mut e = 1.0 / x64.sqrt();
    for _ in 0..3 {
        let half = c.sqrt_half * e;
        e = half * (c.sqrt_three - x64 * (e * e));
    }
    (x64 * e) as f32
}
/// +0x20E54: b / a computed as b * (1/sqrt a)^2 in single precision.
pub fn glyph_scale(c: &TitleConstants, a: i16, b: i16) -> f32 {
    let s = module_sqrt(c, a as f32);
    let r1 = c.one / s;
    let r2 = c.one / s;
    b as f32 * (r2 * r1)
}
/// +0x20FB4
pub fn line_width(c: &TitleConstants, line: &[Glyph], count: i16, gap: f32, scale: f32) -> f32 {
    let step = gap * scale;
    let mut acc = c.zero;
    for g in line.iter().take(count.max(0) as usize) {
        acc = acc + g.width as f32 * scale + step;
    }
    acc
}
/// +0x21D48: copy entries; codes 1 and 2 continue or repeat the previous code.
pub fn copy_line(src: &[Glyph], n: i16, delay: i16, prior: u32) -> Vec<Glyph> {
    let mut prev = prior;
    let mut out = Vec::new();
    for g in src.iter().take(n.max(0) as usize) {
        let mut g = *g;
        if g.code == 1 {
            g.code = prev.wrapping_add(1);
        } else if g.code == 2 {
            g.code = prev;
        }
        if delay > 0 {
            g.delay = delay as u16;
        }
        prev = g.code;
        out.push(g);
    }
    out
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TitleWork {
    pub index: i8,
    pub countdown: u8,
    pub flags: i16,
    pub life: u16,
    /// Index of the next line entry (the work's +8 pointer).
    pub entry: usize,
    pub sound: i32,
    pub x: f32,
    pub y: f32,
    pub fade: f32,
    pub fade_rate: f32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GlyphWork {
    pub index: u16,
    pub entry: usize,
    pub x: f32,
    pub y: f32,
    pub k: f32,
    pub scale: f32,
    pub sx: f32,
    pub sy: f32,
    pub color: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct WindowWork {
    /// +1: the window line count's low byte.
    pub count: i8,
    pub timer: u16,
    pub x: f32,
    pub y: f32,
    pub k: f32,
    pub scale: f32,
    pub alpha: f32,
    pub color: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Title {
    pub desc: Descriptor,
    pub line: Vec<Glyph>,
    pub window_line: Option<Vec<Glyph>>,
    pub work: TitleWork,
    pub glyphs: Vec<GlyphWork>,
    pub window: Option<WindowWork>,
}

fn call(calls: &mut Vec<Value>, a: u32, args: Value) {
    calls.push(json!({ "engine": format!("0x{a:08x}"), "args": args }));
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CreateInputs {
    /// s6+0x6D0 already holds a title.
    pub active: bool,
    pub parent: bool,
    pub task: bool,
    pub copy: bool,
    pub line_buffer: bool,
    pub window_buffer: bool,
    pub mode_query: i32,
    /// The engine-zeroed work words the module never writes.
    pub flags: i16,
    pub fade: f32,
    pub fade_rate: f32,
    /// 0x800709f8 result for the window child (a null child is not checked).
    pub window_task: bool,
    /// Register r10 when create starts. +0x21D48 reads it as the previous
    /// code for a leading code 1 or 2; the line copy then sees the window
    /// copy's last code instead.
    pub r10: u32,
}

/// +0x22070. Returns the calls and the created title (None when it fails).
pub fn create(
    c: &TitleConstants,
    d: &Descriptor,
    i: &CreateInputs,
) -> Result<(Vec<Value>, Option<Title>)> {
    let mut calls = Vec::new();
    if i.active || d.texlist == 0 {
        return Ok((calls, None));
    }
    if i.parent {
        call(
            &mut calls,
            0x800709f8,
            json!([2, "s1+0x21de4", "parent", i.task]),
        );
    } else {
        call(
            &mut calls,
            0x80071e0c,
            json!([2, 3, "s1+0x21de4", "BossTitleExec", i.task]),
        );
    }
    if !i.task {
        return Ok((calls, None));
    }
    call(&mut calls, 0x801197fc, json!([1, 0x2c, i.copy]));
    if !i.copy {
        call(&mut calls, 0x80071ae0, json!(["task"]));
        return Ok((calls, None));
    }
    call(&mut calls, 0x8001dd78, json!([i.mode_query]));
    let mut desc = d.clone();
    let mut work = TitleWork {
        index: 0,
        countdown: 0,
        flags: i.flags,
        life: if desc.flags & 1 != 0 { desc.life } else { 0 },
        entry: 0,
        sound: if i.mode_query >= 0x3c { 2 } else { 0x801e },
        x: 0.0,
        y: 0.0,
        fade: i.fade,
        fade_rate: i.fade_rate,
    };
    if desc.flags & 2 == 0 {
        desc.scale = c.one;
    }
    let mut window = None;
    let mut window_line = None;
    if let Some(src) = &d.window {
        call(
            &mut calls,
            0x801197fc,
            json!([d.window_count, 0x10, i.window_buffer]),
        );
        if !i.window_buffer {
            call(&mut calls, 0x80071af0, json!(["task"]));
            return Ok((calls, None));
        }
        let line = copy_line(src, d.window_count, 0, i.r10);
        let w = line_width(c, &line, desc.window_count, c.zero, desc.scale);
        call(
            &mut calls,
            0x800709f8,
            json!([2, "s1+0x218ac", "task", i.window_task]),
        );
        window = i.window_task.then_some(WindowWork {
            count: desc.window_count as i8,
            timer: 0x3c,
            x: desc.x - c.half * w,
            y: desc.y,
            k: c.glyph_k,
            scale: desc.scale,
            alpha: c.zero,
            color: desc.color & 0xff_ffff,
        });
        window_line = Some(line);
    }
    // With no line table the native code reads entry fields through a null
    // pointer; such a descriptor is outside this port.
    let src = d
        .line
        .as_ref()
        .ok_or_else(|| Error::invalid("title descriptor without a line"))?;
    let line;
    {
        call(
            &mut calls,
            0x801197fc,
            json!([d.count, 0x10, i.line_buffer]),
        );
        if !i.line_buffer {
            call(&mut calls, 0x80071af0, json!(["task"]));
            return Ok((calls, None));
        }
        let delay = if desc.flags & 4 != 0 { 0 } else { desc.delay };
        let prior = window_line
            .as_ref()
            .and_then(|l: &Vec<Glyph>| l.last())
            .map_or(i.r10, |g| g.code);
        line = copy_line(src, d.count, delay, prior);
        let w = line_width(c, &line, desc.count, desc.spacing as f32, desc.scale);
        work.x = desc.x - c.half * w;
        work.y = desc.y - line[0].height as f32 * desc.scale;
    }
    work.countdown = (line[0].delay as u32 + 0x14) as u8;
    calls.push(json!({ "write": "task+0x18", "value": "s1+0x2200c" }));
    Ok((
        calls,
        Some(Title {
            desc,
            line,
            window_line,
            work,
            glyphs: Vec::new(),
            window,
        }),
    ))
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Pauses {
    pub g37: bool,
    pub g09: bool,
    pub g39: bool,
}
impl Pauses {
    fn any(&self) -> bool {
        self.g37 || self.g09 || self.g39
    }
}

/// +0x21DE4 for the title task. Returns calls and whether it freed itself.
/// `beyond` is the halfword past the last entry's slot that the native code
/// reads as the next delay after the final glyph (engine heap memory).
pub fn update(c: &TitleConstants, t: &mut Title, p: &Pauses, beyond: u16) -> (Vec<Value>, bool) {
    let mut calls = Vec::new();
    if p.any() || t.work.flags & 0x200 != 0 {
        return (calls, false);
    }
    let w = &mut t.work;
    if (w.index as i32) < t.desc.count as i32 && w.countdown != 0 {
        w.countdown -= 1;
        if w.countdown == 0 {
            let e = t.line[w.entry];
            if e.code != 0 {
                call(
                    &mut calls,
                    0x800709f8,
                    json!([2, "s1+0x211fc", "task", true]),
                );
                t.glyphs.push(GlyphWork {
                    index: w.index as u16,
                    entry: w.entry,
                    x: w.x,
                    y: w.y,
                    k: c.glyph_k,
                    scale: t.desc.scale,
                    sx: glyph_scale(c, e.scale_x, e.width),
                    sy: glyph_scale(c, e.scale_y, e.height),
                    color: (t.desc.color & 0xff_ffff).wrapping_sub(0x0100_0000),
                });
                if w.sound != -1 {
                    call(&mut calls, 0x8006b978, json!([w.sound, 0, 0, 0]));
                }
            }
            let s = t.desc.scale;
            w.x += e.width as f32 * s + t.desc.spacing as f32 * s;
            w.entry += 1;
            w.index = w.index.wrapping_add(1);
            w.countdown = t.line.get(w.entry).map_or(beyond, |g| g.delay) as u8;
        }
    }
    if w.life != 0 {
        w.life -= 1;
        if w.life == 0 {
            call(&mut calls, 0x80071ae0, json!(["task"]));
            return (calls, true);
        }
    }
    if w.flags & 0x400 != 0 {
        t.desc.scale *= c.fade_grow;
        w.fade -= w.fade_rate;
        if w.fade <= c.zero {
            call(&mut calls, 0x80071ae0, json!(["task"]));
            return (calls, true);
        }
    }
    (calls, false)
}

/// +0x2200C
pub fn delete(t: &Title) -> Vec<Value> {
    let mut calls = Vec::new();
    call(&mut calls, 0x80119828, json!(["line"]));
    if t.window_line.is_some() {
        call(&mut calls, 0x80119828, json!(["windowLine"]));
    }
    calls
}

/// +0x211FC for glyph `k` (`parent_hold`: the parent work's 0x200 flag).
pub fn glyph_update(c: &TitleConstants, t: &mut Title, k: usize, p: &Pauses) {
    if p.any() || t.work.flags & 0x200 != 0 || t.work.flags & 0x400 == 0 {
        return;
    }
    let s = t.desc.scale;
    let g = &mut t.glyphs[k];
    g.scale = s;
    let a = (c.alpha * t.work.fade) as i32 as u32 & 0xff;
    g.color = (t.desc.color & 0xff_ffff).wrapping_add(a << 24);
    let w = line_width(c, &t.line, t.desc.count, t.desc.spacing as f32, s);
    let mut x = t.desc.x - c.half * w;
    let y = t.desc.y - t.line[0].height as f32 * s;
    for e in t.line.iter().take(g.index as usize) {
        x += e.width as f32 * s + t.desc.spacing as f32 * s;
    }
    g.x = x;
    g.y = y;
}

/// +0x218AC for the window child.
pub fn window_update(c: &TitleConstants, t: &mut Title, p: &Pauses) {
    if p.any() || t.work.flags & 0x200 != 0 {
        return;
    }
    let Some(win) = &mut t.window else { return };
    if win.timer == 0 {
        if win.alpha < c.one {
            win.alpha += c.window_rate;
            if win.alpha > c.one {
                win.alpha = c.one;
            }
        }
    } else {
        win.timer -= 1;
    }
    if t.work.flags & 0x400 != 0 {
        let line = t.window_line.as_deref().unwrap_or(&[]);
        let w = line_width(c, line, t.desc.window_count, c.zero, t.desc.scale);
        let win = t.window.as_mut().unwrap();
        win.x = t.desc.x - c.half * w;
        win.y = t.desc.y;
        win.scale = t.desc.scale;
        win.alpha = t.work.fade;
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase", tag = "op")]
enum Op {
    Create {
        /// Section 5 offset of a module descriptor, or an inline descriptor.
        #[serde(default)]
        descriptor: Option<u32>,
        #[serde(default)]
        inline: Option<Descriptor>,
        inputs: CreateInputs,
    },
    Update {
        pauses: Pauses,
        flags: i16,
        #[serde(default)]
        beyond: u16,
    },
    Glyph {
        index: usize,
        pauses: Pauses,
        flags: i16,
    },
    Window {
        pauses: Pauses,
        flags: i16,
    },
    Delete,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    ops: Vec<Op>,
}
pub fn replay(rel_bytes: &[u8], request: &[u8]) -> Result<Value> {
    let c = constants(rel_bytes)?;
    let req: Request = serde_json::from_slice(request)
        .map_err(|e| Error::invalid(format!("title request: {e}")))?;
    let mut title: Option<Title> = None;
    let mut out = Vec::new();
    for op in req.ops {
        let (calls, gone) = match op {
            Op::Create {
                descriptor: at,
                inline,
                inputs,
            } => {
                let d = match (at, inline) {
                    (Some(at), None) => descriptor(rel_bytes, at)?,
                    (None, Some(d)) => d,
                    _ => return Err(Error::invalid("title create needs one descriptor")),
                };
                let (calls, t) = create(&c, &d, &inputs)?;
                title = t;
                (calls, false)
            }
            Op::Update {
                pauses,
                flags,
                beyond,
            } => {
                let t = title.as_mut().ok_or_else(|| Error::invalid("no title"))?;
                t.work.flags = flags;
                update(&c, t, &pauses, beyond)
            }
            Op::Glyph {
                index,
                pauses,
                flags,
            } => {
                let t = title.as_mut().ok_or_else(|| Error::invalid("no title"))?;
                t.work.flags = flags;
                if index >= t.glyphs.len() {
                    return Err(Error::invalid("no such glyph"));
                }
                glyph_update(&c, t, index, &pauses);
                (vec![], false)
            }
            Op::Window { pauses, flags } => {
                let t = title.as_mut().ok_or_else(|| Error::invalid("no title"))?;
                t.work.flags = flags;
                window_update(&c, t, &pauses);
                (vec![], false)
            }
            Op::Delete => {
                let t = title.take().ok_or_else(|| Error::invalid("no title"))?;
                (delete(&t), true)
            }
        };
        out.push(json!({ "calls": calls, "title": title, "freed": gone }));
    }
    Ok(json!({ "constants": c, "ops": out }))
}
