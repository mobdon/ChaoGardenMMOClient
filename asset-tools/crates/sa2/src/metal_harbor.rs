//! Metal Harbor (story level 10, stg10D.rel): the stage program "STG10" that
//! `_prolog` (+0xA640) hands to LoadSubprogram, and the stage task it
//! creates. All offsets are section-1 (code) offsets of stg10D.rel; engine
//! calls are reported at their GSNE8P main.dol addresses.
//!
//! | Offset | Size | Routine |
//! | --- | --- | --- |
//! | +0xA66C | 0x3F0 | stage setup (the program's first entry) |
//! | +0xAA5C | 0x120 | stage teardown (second entry) |
//! | +0xACD4 | 0x54 | stage program exec (third entry) |
//! | +0xAB7C | 0x158 | speed probe called by the exec: computes a value it never stores; no effect |
//! | +0xAD28 | 0x16C | stage task exec |
//! | +0xAE94 | 0x14 | stage task destructor: reads its work pointer only; no effect |
//! | +0xAEA8 | 0x184 | stage task display |
//! | +0xB02C | 0x120 | stage task second display |
//! | +0x13DC | 0x120 | drift and expiry of the five-entry list at .bss+0 |
//! | +0xA3B0 | 0xE0 | texture override install (table .data+0x154404 into 0x801E5D28) |
//! | +0xA490 | 0x1B0 | player reset and texture override restore |
//! | +0x106A8 | 0x13C | follower task spawn |
//!
//! Constants and data pointers are read from the module through the
//! relocation of the instruction that loads them (the offsets are recorded
//! at each use). The Steam `resource/gd_PC/stg10D.rel` has the same section
//! bytes as the GameCube disc copy, so either can be passed; engine
//! addresses are always the GSNE8P ones (the Steam module links another
//! main program). REL-local helpers that are not part of this port are
//! reported as `module` calls. Display routines are reported as their
//! engine calls (the renderer is separate).
use crate::{
    city_rel::{CityRel, Reference},
    manifest, Error, Result,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// GameCube disc `stg10D.rel` (module 10).
pub const GC_REL_SHA256: &str = "5de85d7f52d6f01416bcd7023c2b3608769f4478daaccdb4e4506be1402b19ac";
/// Installed Steam `resource/gd_PC/stg10D.rel` (module 9, same section bytes).
pub const STEAM_REL_SHA256: &str =
    "46898f02a5bd62f395a38557787f2c3918e92437c61d2b0a0772c88f187c6c53";
pub const REL_LEN: usize = 2_448_724;

fn invalid() -> Error {
    Error::invalid("unexpected stage 10 code")
}
fn s(section: u8, offset: u32) -> Value {
    json!(format!("s{section}+0x{offset:x}"))
}
fn engine(address: u32, args: Value) -> Value {
    json!({ "engine": format!("0x{address:08x}"), "args": args })
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

pub struct MetalHarbor<'a> {
    rel: CityRel<'a>,
}

impl<'a> MetalHarbor<'a> {
    pub fn read(bytes: &'a [u8]) -> Result<Self> {
        if bytes.len() != REL_LEN {
            return Err(Error::invalid("unsupported stage 10 REL revision"));
        }
        let module = match manifest::sha256(bytes).as_str() {
            GC_REL_SHA256 => 10,
            STEAM_REL_SHA256 => 9,
            _ => return Err(Error::invalid("unsupported stage 10 REL revision")),
        };
        Ok(Self {
            rel: CityRel::read_identified(bytes, module)?,
        })
    }
    /// The section offset the `addi` at `pc` (the LO half of a `lis`/`addi`
    /// pair, relocated at `pc + 2`) points at.
    fn at(&self, pc: u32, section: u8) -> Result<u32> {
        match self.rel.reference(1, pc + 2) {
            Some(Reference {
                module,
                kind: 4,
                section: s,
                addend,
            }) if s == section && module == self.rel.module_id() => Ok(addend),
            _ => Err(Error::invalid(format!(
                "unexpected stage 10 code at +0x{pc:x}"
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
    fn half(&self, section: u8, at: u32) -> Result<i16> {
        let d = self.rel.data(section)?;
        d.get(at as usize..at as usize + 2)
            .map(|b| i16::from_be_bytes([b[0], b[1]]))
            .ok_or_else(invalid)
    }
    /// The single-precision constant the `addi` at `pc` addresses (.rodata).
    fn f(&self, pc: u32) -> Result<f32> {
        Ok(f32::from_bits(self.word(4, self.at(pc, 4)?)?))
    }
    /// The target of the module-local `bl` at `pc` (resolved at link time,
    /// without a relocation).
    fn local(&self, pc: u32) -> Result<u32> {
        let ins = self.word(1, pc)?;
        if ins & 0xfc00_0003 != 0x4800_0001 {
            return Err(Error::invalid(format!("no local call at +0x{pc:x}")));
        }
        let disp = ((ins & 0x03ff_fffc) as i32) << 6 >> 6;
        pc.checked_add_signed(disp).ok_or_else(invalid)
    }
    fn local_call(&self, pc: u32, args: Value) -> Result<Value> {
        Ok(module_call(self.local(pc)?, args))
    }
}

/// A word that is either a plain value or, after the routine, one of the
/// module's own addresses (`"s1+0x37ef8"`).
pub type Word = Value;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupInput {
    /// fn_80065048's return, kept at .bss+0xD8.
    pub handle: u32,
    /// CreateElementalTask returned a task.
    pub task: bool,
    /// The words at 0x801A2A18 +0, +8 and +0x24 before the setup.
    pub slots: [u32; 3],
    /// The 14 words of the trail plugin table 0x801E77D8.
    pub trail: [u32; 14],
}

/// +0xA66C. Returns the ordered calls and the final value of every location
/// it writes (`task` is the created stage task's callbacks at +0x14, +0x18
/// and +0x20).
pub fn setup(mh: &MetalHarbor<'_>, i: &SetupInput) -> Result<Value> {
    let f = |pc| mh.f(pc).map(f32::to_bits);
    let p5 = |pc| mh.ptr(pc, 5);
    let mut c = vec![
        engine(0x800324e4, json!([256, p5(0xa684)?])),
        engine(0x80072270, json!([p5(0xa690)?, p5(0xa698)?])),
        engine(0x80072218, json!([p5(0xa6a4)?, p5(0xa6ac)?])),
        engine(0x80072270, json!([p5(0xa6b8)?, p5(0xa6c0)?])),
        engine(0x80072218, json!([p5(0xa6cc)?, p5(0xa6d4)?])),
        engine(0x80065048, json!([0, 0x800])),
        engine(0x8006638c, json!([p5(0xa6f8)?, i.handle])),
        engine(0x8001a8a8, json!([])),
        engine(0x80033710, json!([])),
        engine(0x800c3cd8, json!([f(0xa718)?, f(0xa724)?, f(0xa730)?])),
        engine(0x8011e154, json!([f(0xa740)?, f(0xa74c)?])),
        engine(0x8011e150, json!([f(0xa75c)?, f(0xa768)?, f(0xa774)?])),
    ];
    let lights = mh.at(0xa788, 5)?;
    for (k, pc) in [(0, 0xa788), (1, 0xa798), (2, 0xa7ac), (3, 0xa7bc)] {
        // The second light block is the first plus 0x20 (addi at +0xA79C).
        let off = if k == 1 { lights + 0x20 } else { mh.at(pc, 5)? };
        c.push(engine(0x8002506c, json!([k, s(5, off)])));
    }
    c.extend([
        engine(0x80025028, json!([0, 0])),
        engine(0x80025028, json!([0, 1])),
        engine(0x80025048, json!([0])),
        engine(0x8002503c, json!([2])),
        mh.local_call(0xa7f4, json!([p5(0xa7f0)?]))?,
        engine(0x80011a04, json!([0])),
        engine(0x80011050, json!([p5(0xa804)?])),
        engine(0x8005d0ec, json!([p5(0xa810)?])),
        engine(0x80071e0c, json!([2, 1, mh.ptr(0xa824, 1)?, p5(0xa82c)?])),
    ]);
    let task = if i.task {
        c.push(mh.local_call(0xa878, json!([0, 4, 3, "task"]))?);
        Some([mh.ptr(0xa848, 1)?, mh.ptr(0xa860, 1)?, mh.ptr(0xa854, 1)?])
    } else {
        None
    };
    c.push(engine(0x8003bcc4, json!([p5(0xa880)?])));
    c.push(mh.local_call(0xa888, json!([]))?);
    let mut slots: Vec<Word> = i.slots.iter().map(|v| json!(v)).collect();
    for (k, pc) in [(0, 0xa8a4), (1, 0xa8cc), (2, 0xa8f4)] {
        if i.slots[k] == 0 {
            slots[k] = mh.ptr(pc, 1)?;
        }
    }
    c.extend([
        mh.local_call(0xa904, json!([]))?,
        mh.local_call(0xa918, json!([p5(0xa90c)?, mh.ptr(0xa914, 6)?]))?,
        mh.local_call(0xa92c, json!([p5(0xa920)?, mh.ptr(0xa928, 6)?]))?,
        engine(0x8011c2bc, json!([])),
        engine(0x8011c310, json!([])),
        mh.local_call(0xa940, json!([p5(0xa93c)?]))?,
    ]);
    // Trail plugin table: GSNE8P fn_800E014C, three module routines, then zeros.
    let mut trail = vec![json!(0x800e014c_u32)];
    for pc in [0xa95c, 0xa970, 0xa984] {
        trail.push(mh.ptr(pc, 1)?);
    }
    trail.resize(14, json!(0));
    c.push(engine(0x8006d500, json!([p5(0xaa38)?, mh.ptr(0xaa40, 4)?])));
    Ok(json!({ "calls": c, "state": {
        "handle": i.handle, "slots": slots, "trail": trail, "task": task } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeardownInput {
    /// Each 0x801A2A18 slot: `"ours"` (the routine setup installed) or a value.
    pub slots: [Word; 3],
    pub handle: u32,
}

/// +0xAA5C: undo the setup's hooks and release the stage's resources.
pub fn teardown(mh: &MetalHarbor<'_>, i: &TeardownInput) -> Result<Value> {
    let p5 = |pc| mh.ptr(pc, 5);
    let mut c = vec![
        mh.local_call(0xaa68, json!([]))?,
        mh.local_call(0xaa6c, json!([]))?,
        engine(0x8011c2bc, json!([])),
        engine(0x8011c310, json!([])),
    ];
    let slots: Vec<Word> = i
        .slots
        .iter()
        .map(|v| {
            if v == "ours" {
                json!(0)
            } else if v.is_u64() {
                v.clone()
            } else {
                Value::Null
            }
        })
        .collect();
    if slots.iter().any(Value::is_null) {
        return Err(Error::invalid("teardown slot must be \"ours\" or a word"));
    }
    c.extend([
        mh.local_call(0xaafc, json!([]))?,
        engine(0x800336dc, json!([])),
        engine(0x80072118, json!([p5(0xab08)?, p5(0xab10)?])),
        engine(0x80072118, json!([p5(0xab1c)?, p5(0xab24)?])),
        engine(0x80066364, json!([])),
    ]);
    if i.handle != 0 {
        c.push(engine(0x8001a2b0, json!([i.handle])));
    }
    c.push(engine(0x800245b8, json!([])));
    c.push(mh.local_call(0xab68, json!([]))?);
    Ok(json!({ "calls": c, "state": { "handle": 0, "slots": slots } }))
}

/// +0xACD4, the stage program's exec: the module helper +0x158E0, then the
/// speed probe +0xAB7C (which has no effect) while the work's mode byte is 0.
pub fn exec(mh: &MetalHarbor<'_>, mode: i8) -> Result<Value> {
    let mut c = vec![mh.local_call(0xacf0, json!([]))?];
    if mode == 0 {
        c.push(mh.local_call(0xad0c, json!(["task"]))?);
    }
    Ok(json!({ "calls": c, "state": {} }))
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    /// Mode byte of the task kept at .bss+0xB8.
    pub mode: i8,
    /// Its work +6.
    pub timer: u16,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskExecInput {
    /// The task at .bss+0xB8, if any.
    pub frame: Option<Frame>,
    /// Stage task work +1.
    pub flags: i8,
    /// Game state 0x801CC168 +0x7C.
    pub clock: u32,
    /// .data+0x117C before the exec (bit pattern).
    pub fog: u32,
}

/// +0xAD28, the stage task exec. Bit 4 of the flags asks the display to
/// latch the camera; bit 1 switches the display to the second sky; .data+0x117C
/// follows the frame task's mode.
pub fn task_exec(mh: &MetalHarbor<'_>, i: &TaskExecInput) -> Result<Value> {
    let zero = mh.f(0xad4c)?.to_bits();
    let mut c = vec![mh.local_call(0xad60, json!([zero, mh.f(0xad58)?.to_bits()]))?];
    // Every path rewrites .data+0x117C, so its old value (`fog`) is never read.
    let mut flags = i.flags;
    let mut fog;
    match i.frame {
        Some(fr) if fr.mode != 0 => {
            fog = mh.f(0xada0)?.to_bits();
            if flags as u8 & 6 == 0 {
                flags = (flags as u8 | 4) as i8;
            }
            if (fr.mode == 3 || fr.mode == 5) && fr.timer > 240 {
                fog = mh.f(0xae0c)?.to_bits();
                // rlwinm 0,31,28 at +0xAE24 clears both display bits.
                flags = ((flags as u8 & !6) | 1) as i8;
            }
        }
        _ => {
            flags = 0;
            fog = mh.f(0xae48)?.to_bits();
        }
    }
    mh.at(0xadac, 5)?; // .data+0x117C, the location `fog` names
    if i.clock < 10 {
        c.push(engine(0x80012218, json!([mh.ptr(0xae74, 5)?])));
    }
    Ok(json!({ "calls": c, "state": { "flags": flags, "fog": fog } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayInput {
    /// The active camera position (bit patterns).
    pub cam: [u32; 3],
    pub flags: i8,
    /// .bss+0xDC, the latched camera position.
    pub saved: [u32; 3],
}

/// +0xAEA8, the stage task display: latch the camera when asked, then draw
/// the sky model around it below a camera height, or the second sky.
pub fn display(mh: &MetalHarbor<'_>, i: &DisplayInput) -> Result<Value> {
    let v = [i.cam[0], mh.f(0xaef0)?.to_bits(), i.cam[2]];
    let (mut flags, mut saved) = (i.flags, i.saved);
    mh.at(0xaf3c, 6)?;
    if flags as u8 & 4 != 0 {
        saved = v;
        flags = ((flags as u8 & !4) | 2) as i8;
    }
    let mut c = Vec::new();
    if flags as u8 & 1 == 0 {
        c.extend([engine(0x8011c2bc, json!([])), engine(0x8011c310, json!([]))]);
        if f32::from_bits(i.cam[1]) < mh.f(0xafb4)? {
            c.extend([
                engine(0x80116098, json!([])),
                engine(0x8011538c, json!([0, v])),
                engine(0x80116e18, json!([mh.ptr(0xafd8, 5)?])),
                engine(0x8011df44, json!([mh.ptr(0xafe4, 5)?])),
                engine(0x80115dc8, json!([1])),
            ]);
        }
        c.extend([
            mh.local_call(0xaffc, json!([mh.ptr(0xaff8, 6)?]))?,
            engine(0x8011c2b0, json!([])),
            engine(0x8011c310, json!([])),
        ]);
    } else {
        c.push(mh.local_call(0xb014, json!([mh.ptr(0xb010, 6)?]))?);
    }
    Ok(json!({ "calls": c, "state": { "flags": flags, "saved": saved } }))
}

/// +0xB02C, the stage task's second display: the model drawn by +0x1594 at
/// the latched (or current) camera, lowered, plus a fixed offset.
pub fn display2(mh: &MetalHarbor<'_>, i: &DisplayInput) -> Result<Value> {
    let mut c = Vec::new();
    if i.flags as u8 & 1 == 0 {
        let mut v = if i.flags as u8 & 2 != 0 {
            mh.at(0xb0d4, 6)?;
            i.saved
        } else {
            [i.cam[0], mh.f(0xb074)?.to_bits(), i.cam[2]]
        };
        v[1] = (f32::from_bits(v[1]) - mh.f(0xb0f8)?).to_bits();
        let off = mh.at(0xb120, 5)? + 8; // addi r4,r4,8 at +0xB124
        let offset = [mh.word(5, off)?, mh.word(5, off + 4)?, mh.word(5, off + 8)?];
        c.extend([
            engine(0x80116098, json!([])),
            engine(0x8011538c, json!([0, v])),
            engine(0x8011538c, json!([0, offset])),
            mh.local_call(0xb12c, json!([]))?,
            engine(0x80115dc8, json!([1])),
        ]);
    }
    Ok(json!({ "calls": c, "state": { "flags": i.flags, "saved": i.saved } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DriftInput {
    /// .bss+0x78, the number of live entries.
    pub count: i32,
    /// .data+0x1170 (bit pattern).
    pub rate: u32,
    /// The five 24-byte entries at .bss+0: x, y, two words, age, birth.
    pub entries: [[u32; 6]; 5],
    /// f1, f2 (bit patterns).
    pub delta: [u32; 2],
}

/// +0x13DC: age every live entry, drop those older than
/// `fctiwz(6500 * rate)` frames (compacting the list) and move the rest by
/// `delta`. The list holds five entries; a larger count would overrun into
/// the count itself, so it is refused.
pub fn drift(mh: &MetalHarbor<'_>, i: &DriftInput) -> Result<Value> {
    mh.at(0x13f4, 6)?;
    mh.at(0x142c, 6)?;
    mh.at(0x1410, 5)?;
    if i.count > 5 {
        return Err(Error::invalid("stage 10 drift list holds five entries"));
    }
    let limit = fctiwz(mh.f(0x1404)? * f32::from_bits(i.rate));
    let (mut e, mut count) = (i.entries, i.count);
    let mut dst = 0usize;
    for src in 0..i.count.max(0) as usize {
        if src != dst {
            e[dst] = e[src];
        }
        e[dst][4] = e[dst][4].wrapping_add(1);
        let age = (e[dst][4] as i32).wrapping_sub(e[dst][5] as i32);
        if limit < age {
            count -= 1;
        } else {
            e[dst][0] = (f32::from_bits(e[dst][0]) + f32::from_bits(i.delta[0])).to_bits();
            e[dst][1] = (f32::from_bits(e[dst][1]) + f32::from_bits(i.delta[1])).to_bits();
            dst += 1;
        }
    }
    Ok(json!({ "calls": [], "state": { "count": count, "entries": e } }))
}

/// One record of the texture override table (.data+0x154404): an index into
/// 0x801E5D28's 8-byte entries, the halfword for +2 and the pointer for +4.
fn overrides(mh: &MetalHarbor<'_>, pc: u32) -> Result<Vec<(i16, i16, Value)>> {
    let base = mh.at(pc, 5)?;
    let mut out = Vec::new();
    for k in 0..4096u32 {
        let at = base + 8 * k;
        let id = mh.half(5, at)?;
        if id == -1 {
            return Ok(out);
        }
        let ptr = match mh.rel.local_pointer(5, at + 4)? {
            Some((sec, off)) => s(sec, off),
            None => json!(0),
        };
        out.push((id, mh.half(5, at + 2)?, ptr));
    }
    Err(Error::invalid("stage 10 override table is unterminated"))
}
type Entries = BTreeMap<String, (i16, Word)>;
fn entry(e: &mut Entries, id: i16) -> Result<&mut (i16, Word)> {
    e.get_mut(&id.to_string())
        .ok_or_else(|| Error::invalid(format!("texture entry {id} missing")))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallInput {
    /// 0x801E5D28 entries by index: [+2 halfword, +4 pointer].
    pub e: Entries,
}

/// +0xA3B0: give every free entry (pointer 0) its override.
pub fn install(mh: &MetalHarbor<'_>, i: &InstallInput) -> Result<Value> {
    let mut e = i.e.clone();
    for (id, half, ptr) in overrides(mh, 0xa3c4)? {
        if (0..300).contains(&id) {
            let slot = entry(&mut e, id)?;
            if slot.1 == json!(0) {
                *slot = (half, ptr);
            }
        }
    }
    Ok(json!({ "calls": [], "state": { "e": e } }))
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResetPlayer {
    /// Player work byte 0: the index into playerpwp whose +0x180 is flagged.
    pub index: i8,
    /// That player work's +0x180.
    pub flags: i16,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestoreInput {
    pub e: Entries,
    /// playerpwp[0] and [1] (None: null).
    pub players: [Option<ResetPlayer>; 2],
}

/// +0xA490: reset both players (fn_8003D7D4, flag 4 at +0x180,
/// fn_8005523C), then clear every entry still holding its override.
pub fn restore(mh: &MetalHarbor<'_>, i: &RestoreInput) -> Result<Value> {
    let mut c = Vec::new();
    let mut flags = i.players.map(|p| p.map(|p| p.flags));
    for (p, player) in i.players.iter().enumerate() {
        let Some(player) = player else { continue };
        let target = player.index;
        c.push(engine(0x8003d7d4, json!([format!("pwp{p}+0x174")])));
        let t = usize::try_from(target)
            .ok()
            .and_then(|t| flags.get_mut(t).and_then(Option::as_mut))
            .ok_or_else(|| Error::invalid("player index names no player"))?;
        *t |= 4;
        c.push(engine(
            0x8005523c,
            json!([format!("twp{p}"), format!("mwp{p}"), format!("pwp{p}")]),
        ));
    }
    let mut e = i.e.clone();
    for (id, _, ptr) in overrides(mh, 0xa4ac)? {
        if (0..300).contains(&id) {
            let slot = entry(&mut e, id)?;
            if slot.1 == ptr {
                *slot = (0, json!(0));
            }
        }
    }
    Ok(json!({ "calls": c, "state": { "e": e, "flags": flags } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpawnInput {
    pub player: i32,
    pub a: u32,
    pub b: u32,
    /// A parent task was passed (CreateChildTask instead of CreateElementalTask).
    pub parent: bool,
    /// playertwp[player] is set.
    pub twp: bool,
    /// The allocator returned a task.
    pub created: bool,
    /// playerpwp[player] +0x200..+0x218.
    pub pwp: [u32; 6],
}

/// +0x106A8: create the follower task (exec +0x107E4) for a player and seed
/// its work from the player's +0x200 block. Returns the task (`ret`), its
/// +0x10/+0x14/+0x18 callbacks and its first 11 work words.
pub fn spawn(mh: &MetalHarbor<'_>, i: &SpawnInput) -> Result<Value> {
    let mut c = Vec::new();
    let mut task = [json!(0), json!(0), json!(0)];
    let mut work = [0u32; 11];
    let empty = |c: Vec<Value>, task, work| {
        json!({ "calls": c, "state": {
        "ret": 0, "task": task, "work": work } })
    };
    if i.player < 0 {
        return Err(Error::invalid("negative player index"));
    }
    if i.player >= 2 || !i.twp {
        return Ok(empty(c, task, work));
    }
    if i.parent {
        c.push(engine(
            0x800709f8,
            json!([2, mh.ptr(0x1070c, 1)?, "parent"]),
        ));
    } else {
        c.push(engine(
            0x80071e0c,
            json!([2, 3, mh.ptr(0x10728, 1)?, mh.ptr(0x10730, 5)?]),
        ));
    }
    if !i.created {
        return Ok(empty(c, task, work));
    }
    task = [
        mh.ptr(0x10754, 1)?,
        mh.ptr(0x10748, 1)?,
        mh.ptr(0x10760, 1)?,
    ];
    work[0] = u32::from(i.player as u8) << 16;
    work[2] = i.a;
    work[3] = i.b;
    work[5..8].copy_from_slice(&i.pwp[3..6]);
    work[8..11].copy_from_slice(&i.pwp[0..3]);
    Ok(json!({ "calls": c, "state": { "ret": "task", "task": task, "work": work } }))
}

/// Run one routine by name on a JSON input (the oracle's case format).
pub fn replay(rel_bytes: &[u8], routine: &str, input: &Value) -> Result<Value> {
    let mh = MetalHarbor::read(rel_bytes)?;
    fn parse<T: serde::de::DeserializeOwned>(v: &Value) -> Result<T> {
        T::deserialize(v).map_err(|e| Error::invalid(format!("stage 10 request: {e}")))
    }
    match routine {
        "setup" => setup(&mh, &parse(input)?),
        "teardown" => teardown(&mh, &parse(input)?),
        "exec" => exec(&mh, parse::<ExecInput>(input)?.mode),
        "task_exec" => task_exec(&mh, &parse(input)?),
        "display" => display(&mh, &parse(input)?),
        "display2" => display2(&mh, &parse(input)?),
        "drift" => drift(&mh, &parse(input)?),
        "install" => install(&mh, &parse(input)?),
        "restore" => restore(&mh, &parse(input)?),
        "spawn" => spawn(&mh, &parse(input)?),
        _ => Err(Error::invalid(format!(
            "unknown stage 10 routine {routine}"
        ))),
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExecInput {
    mode: i8,
}
