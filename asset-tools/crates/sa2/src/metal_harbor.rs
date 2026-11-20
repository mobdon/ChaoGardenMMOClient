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

// ---- MSBRILONG, MSBRISHORT, MSBRIMINI, MSBRISLOPE (object list rows at
// .data+0x156540..0x156570): four loaders around one constructor +0x85C4.

/// The four bridge loaders, in object-list order.
pub const BRIDGE_LOADERS: [u32; 4] = [0x8464, 0x84bc, 0x8514, 0x856c];

impl MetalHarbor<'_> {
    /// A data word as the oracle names it: a module address when the word
    /// carries an ADDR32 relocation to this module, else its value.
    fn data_value(&self, section: u8, at: u32) -> Result<Value> {
        Ok(match self.rel.reference(section, at) {
            Some(Reference {
                module,
                kind: 1,
                section: s2,
                addend,
            }) if module == self.rel.module_id() => s(s2, addend),
            Some(_) => return Err(invalid()),
            None => json!(self.word(section, at)?),
        })
    }
    /// The loader's constructor arguments: model, .bss word holding the
    /// texture list, the texture argument, the NJS_OBJECT template, the
    /// collision info and its count.
    fn bridge(&self, bridge: usize) -> Result<(u32, u32, u32, u32, u32, i16)> {
        let l = *BRIDGE_LOADERS
            .get(bridge)
            .ok_or_else(|| Error::invalid("bridge index"))?;
        if self.local(l + 0x44)? != 0x85c4 {
            return Err(invalid());
        }
        let count = self.word(1, l + 0x40)?;
        if count >> 16 != 0x3920 {
            return Err(invalid()); // li r9,count
        }
        Ok((
            self.at(l + 0x18, 5)?,
            self.at(l + 0x20, 6)?,
            self.at(l + 0x2c, 5)?,
            self.at(l + 0x34, 5)?,
            self.at(l + 0x3c, 5)?,
            count as u16 as i16,
        ))
    }
    /// The model the template object points at (its +4), with its centre
    /// (+0x18) and radius (+0x24).
    fn bridge_model(&self, template: u32) -> Result<(u32, [u32; 3], f32)> {
        let model = match self.rel.local_pointer(5, template + 4)? {
            Some((5, off)) => off,
            _ => return Err(invalid()),
        };
        let w = |k| self.word(5, model + k);
        Ok((
            model,
            [w(0x18)?, w(0x1c)?, w(0x20)?],
            f32::from_bits(w(0x24)?),
        ))
    }
}
/// Mode 3 or 4 of the frame task (.bss+0xB8) after 44 frames hides the
/// bridges and drops their collision flag.
fn bridges_hidden(frame: Option<Frame>) -> bool {
    frame.is_some_and(|f| (f.mode == 3 || f.mode == 4) && f.timer > 44)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BridgeLoadInput {
    pub bridge: usize,
    /// CheckRangeOut's answer.
    pub range_out: bool,
    /// syCalloc returned memory.
    pub alloc: bool,
    /// The .bss word the loader passes (the bridge's texture list).
    pub texlist: u32,
    /// Work +0xC (Y angle) and +0x14 position (bit patterns).
    pub rot: u32,
    pub pos: [u32; 3],
    /// Work +0 before the loader (kept when it stops early).
    pub mode: i8,
}

/// A bridge loader and the constructor +0x85C4: allocate the 84-byte
/// dynamic collision block (`dyn`), copy the template object into it at the
/// task's place, register it (fn_800232A4) with a size class from the model
/// radius, transform the model centre into `dyn+0x38`, and set up collision.
pub fn bridge_load(mh: &MetalHarbor<'_>, i: &BridgeLoadInput) -> Result<Value> {
    let (model_arg, _texlist, texture, template, colli, count) = mh.bridge(i.bridge)?;
    let mut c = vec![engine(0x800655f4, json!(["task"]))];
    if i.range_out {
        return Ok(json!({ "calls": c, "state": {
            "task": [0, 0, 0, 0, 0], "mode": i.mode, "dyn": Value::Null } }));
    }
    c.push(engine(0x801197fc, json!([1, 84])));
    if !i.alloc {
        return Ok(json!({ "calls": c, "state": {
            "task": [0, 0, 0, 0, 0], "mode": i.mode, "dyn": Value::Null } }));
    }
    let mut d: Vec<Value> = (0..14)
        .map(|k| mh.data_value(5, template + 4 * k))
        .collect::<Result<_>>()?;
    let first = d[0].as_u64().ok_or_else(invalid)? as u32;
    d[0] = json!(first & !3);
    d.splice(2..5, i.pos.map(|v| json!(v)));
    d[5] = json!(0);
    d[6] = json!(i.rot);
    d[7] = json!(0);
    d.extend((14..21).map(|_| json!(0)));
    d[17] = s(5, texture);
    d[20] = json!(i.texlist);
    let (_, centre, radius) = mh.bridge_model(template)?;
    let r = mh.f(0x8730)? + radius;
    let size = if r > mh.f(0x8748)? {
        0
    } else if r > mh.f(0x8760)? {
        0x2000_0000
    } else if r > mh.f(0x877c)? {
        0x4000_0000
    } else {
        0x6000_0000
    };
    c.extend([
        engine(0x800232a4, json!([0x801 | size, "task", "dyn"])),
        engine(0x80116098, json!([])),
        engine(0x801153e4, json!([0])),
        engine(0x8011616c, json!([i.pos])),
        engine(0x80115828, json!([0, i.rot])),
        engine(0x80114f0c, json!([0, centre, "dyn+0x38"])),
        engine(0x801160e4, json!([])),
    ]);
    if colli != 0 {
        c.push(engine(0x8000f8b8, json!(["task", s(5, colli), count, 4])));
    }
    Ok(json!({ "calls": c, "state": {
        "task": [mh.ptr(0x8658, 1)?, mh.ptr(0x864c, 1)?, mh.ptr(0x8664, 1)?, "dyn", s(5, model_arg)],
        "mode": 0, "dyn": d } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BridgeExecInput {
    pub bridge: usize,
    pub range_out: bool,
    /// `dyn`+4 (the registered model) is set.
    pub obj: bool,
    /// _rename_EitherPlayerWithinSphere's answer.
    pub within: bool,
    pub frame: Option<Frame>,
    /// Work +4.
    pub flags: i16,
    /// Work +0x2C (collision) is set.
    pub colli: bool,
}

/// +0x8880: while a player is near and the bridge is not hidden, set flag
/// 0x100 of work +4 (else clear it); then feed collision.
pub fn bridge_exec(mh: &MetalHarbor<'_>, i: &BridgeExecInput) -> Result<Value> {
    let mut c = vec![engine(0x800655f4, json!(["task"]))];
    let mut flags = i.flags;
    if i.range_out {
        return Ok(json!({ "calls": c, "state": { "flags": flags } }));
    }
    if i.obj {
        let (.., template, _, _) = mh.bridge(i.bridge)?;
        let (_, _, radius) = mh.bridge_model(template)?;
        let r = mh.f(0x88c8)? + radius;
        c.push(engine(0x800398d8, json!(["dyn+0x38", r.to_bits()])));
        mh.at(0x88f0, 6)?;
        if i.within && !bridges_hidden(i.frame) {
            flags |= 0x100;
        } else {
            flags &= !0x100;
        }
    }
    if i.colli {
        c.push(engine(0x8000f4ec, json!(["task"])));
    }
    Ok(json!({ "calls": c, "state": { "flags": flags } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeDisplayInput {
    pub frame: Option<Frame>,
    pub rot: u32,
    pub pos: [u32; 3],
    /// `dyn`+0x44 and task +0x40.
    pub texture: u32,
    pub model: u32,
}

/// +0x89A4: draw the bridge model unless hidden.
pub fn bridge_display(mh: &MetalHarbor<'_>, i: &BridgeDisplayInput) -> Result<Value> {
    mh.at(0x89d0, 6)?;
    let mut c = vec![engine(0x80116e18, json!([i.texture]))];
    if !bridges_hidden(i.frame) {
        c.extend([
            engine(0x80116098, json!([])),
            engine(0x8011616c, json!([i.pos])),
            engine(0x80115828, json!([0, i.rot])),
            engine(0x8002b2e8, json!([0x2400])),
            engine(0x80031a28, json!([i.model])),
            engine(0x8002b2d8, json!([0x2400])),
            engine(0x801160e4, json!([])),
        ]);
    }
    Ok(json!({ "calls": c, "state": {} }))
}

/// +0x881C: unregister the collision piece if it was registered, free it.
pub fn bridge_dest(obj: bool) -> Value {
    let mut c = Vec::new();
    if obj {
        c.push(engine(0x800231cc, json!(["task", "dyn"])));
    }
    c.push(engine(0x80119828, json!(["dyn"])));
    json!({ "calls": c, "state": { "task": [0, 0] } })
}

/// Loaders that only pass module data to a shared constructor (owned by the
/// shared stage objects and reported as a `module` call): COVER, DAMMY,
/// CBRI, NET, ISLAND, and PIC (+0x8A88, to the shared +0xFDB0).
pub const THIN_LOADERS: [u32; 6] = [0x5278, 0x52c8, 0x5314, 0x5360, 0x53a8, 0x8a88];

/// The shared constructors and how many arguments after the task they take.
const THIN_TARGETS: [(u32, usize); 3] = [(0xdc24, 5), (0xe020, 6), (0xfdb0, 4)];

/// A thin loader, decoded from its own code: each of r4..r9 is an
/// immediate (`li`), a module address (`lis`/`addi` with relocation) or the
/// word such an address points at (`lwz rX,d(rX)`).
pub fn thin_load(mh: &MetalHarbor<'_>, loader: u32) -> Result<Value> {
    if !THIN_LOADERS.contains(&loader) {
        return Err(Error::invalid("not a thin stage 10 loader"));
    }
    let mut regs: [Option<Value>; 10] = Default::default();
    let mut loc: [Option<(u8, u32)>; 10] = [None; 10];
    for pc in (loader..loader + 0x60).step_by(4) {
        let ins = mh.word(1, pc)?;
        let (op, d, a) = (
            ins >> 26,
            (ins >> 21 & 31) as usize,
            (ins >> 16 & 31) as usize,
        );
        let imm = ins as u16 as i16;
        if op == 18 {
            let target = mh.local(pc)?;
            let (_, n) = THIN_TARGETS
                .iter()
                .find(|(t, _)| *t == target)
                .ok_or_else(invalid)?;
            let mut args = vec![json!("task")];
            for r in regs.iter().skip(4).take(*n) {
                args.push(r.clone().ok_or_else(invalid)?);
            }
            return Ok(json!({ "calls": [module_call(target, json!(args))], "state": {} }));
        }
        if !(4..=9).contains(&d) {
            continue; // prologue: mflr/stw/stwu/lwz r3
        }
        match op {
            14 if a == 0 => regs[d] = Some(json!(imm)),
            14 if a == d => {
                let r = mh.rel.reference(1, pc + 2).ok_or_else(invalid)?;
                if r.module != mh.rel.module_id() || r.kind != 4 || imm != 0 {
                    return Err(invalid());
                }
                loc[d] = Some((r.section, r.addend));
                regs[d] = Some(s(r.section, r.addend));
            }
            15 => {}
            32 if a == d => {
                let (sec, off) = loc[d].take().ok_or_else(invalid)?;
                let at = off.checked_add_signed(imm.into()).ok_or_else(invalid)?;
                regs[d] = Some(mh.data_value(sec, at)?);
            }
            _ => return Err(invalid()),
        }
    }
    Err(invalid())
}

// ---- Ported straight from the disc code without an oracle (mob's rule for
// the remaining objects). Calls name a pointer argument by the field it
// addresses ("work+0x14": the task work at +0x34 plus 0x14; "dyn": the
// block at task +0x38; "colli": the collision info at task +0x3C; "child":
// the task at task +0xC) and pass loaded values from the input.

impl MetalHarbor<'_> {
    /// The single-precision constant every `addi` in `pcs` addresses; they
    /// must all name the same .rodata word.
    fn fs(&self, pcs: &[u32]) -> Result<f32> {
        let first = self.at(*pcs.first().ok_or_else(invalid)?, 4)?;
        for &pc in pcs {
            if self.at(pc, 4)? != first {
                return Err(Error::invalid(format!(
                    "stage 10 constant differs at +0x{pc:x}"
                )));
            }
        }
        Ok(f32::from_bits(self.word(4, first)?))
    }
    /// The immediate of the `li rD,imm` (`addi rD,0,imm`) at `pc`.
    fn li(&self, pc: u32, reg: u32) -> Result<i16> {
        let ins = self.word(1, pc)?;
        if ins >> 16 != 0x3800 | reg << 5 {
            return Err(Error::invalid(format!("no li r{reg} at +0x{pc:x}")));
        }
        Ok(ins as u16 as i16)
    }
    /// The displacement of the load/store at `pc` (its low halfword).
    fn disp(&self, pc: u32) -> Result<i16> {
        Ok(self.word(1, pc)? as u16 as i16)
    }
    /// A data word (`lwz rX,d(rX)` after a relocated `addi`): the module
    /// address it holds, or its value.
    fn loaded(&self, pc: u32, d: u32) -> Result<Value> {
        self.data_value(5, self.at(pc, 5)? + d)
    }
    /// The model a template object (section 5) points at, with its centre
    /// (+0x18) and radius (+0x24).
    fn template_model(&self, template: u32) -> Result<(u32, f32)> {
        let (model, _, r) = self.bridge_model(template)?;
        Ok((model, r))
    }
}

/// The 0x801 size class fn_800232A4 takes for a collision model, from its
/// radius plus the routine's pad (the three thresholds follow it in
/// .rodata order, each read through its own `addi`).
fn size_class(r: f32, t: [f32; 3]) -> u32 {
    if r > t[0] {
        0
    } else if r > t[1] {
        0x2000_0000
    } else if r > t[2] {
        0x4000_0000
    } else {
        0x6000_0000
    }
}

// ---- MHCONT, MHCONT0, MHCONTB and TRUCK: four loaders of one shape
// (+0x2414, +0x2D4C, +0x36AC, +0x9A00), each with its own exec, display and
// a shared destructor shape.

/// Loader, exec, display, destructor of MHCONT, MHCONT0, MHCONTB, TRUCK.
pub const CONTAINERS: [[u32; 4]; 4] = [
    [0x2414, 0x26d4, 0x2cb8, 0x2670],
    [0x2d4c, 0x300c, 0x35f0, 0x2fa8],
    [0x36ac, 0x396c, 0x3df4, 0x3908],
    [0x9a00, 0x9cc0, 0xa250, 0x9c5c],
];
fn container(exec_or_loader: u32) -> Result<[u32; 4]> {
    CONTAINERS
        .into_iter()
        .find(|c| c.contains(&exec_or_loader))
        .ok_or_else(|| Error::invalid("not a stage 10 container routine"))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ContainerLoadInput {
    pub loader: u32,
    pub range_out: bool,
    pub alloc: bool,
    /// Work +8, +0xC, +0x10 (angles) and +0x14 position.
    pub rot: [u32; 3],
    pub pos: [u32; 3],
    /// Task +0xC (the child fn_10_D9C0 attaches) is set afterwards.
    pub child: bool,
}

/// A container loader: allocate the block (`dyn`, 68 or 72 bytes), copy the
/// 14-word template object into it at the task's place, register it with
/// fn_800232A4, attach the child (module +0xD9C0, shared), copy the place
/// to the child's work and set task +0x40 to the start speed.
pub fn container_load(mh: &MetalHarbor<'_>, i: &ContainerLoadInput) -> Result<Value> {
    let [l, exec, display, dest] = container(i.loader)?;
    if l != i.loader {
        return Err(Error::invalid("not a stage 10 container loader"));
    }
    let mut c = vec![engine(0x800655f4, json!(["task"]))];
    let none =
        |c: Vec<Value>| json!({ "calls": c, "state": { "task": Value::Null, "dyn": Value::Null } });
    if i.range_out {
        return Ok(none(c));
    }
    let size = mh.li(l + 0x3c, 4)?; // li r4,68 / 72
    c.push(engine(0x801197fc, json!([1, size])));
    if !i.alloc {
        return Ok(none(c));
    }
    let template = mh.at(l + 0x88, 5)?;
    let mut d: Vec<Value> = (0..14)
        .map(|k| mh.data_value(5, template + 4 * k))
        .collect::<Result<_>>()?;
    let first = d[0].as_u64().ok_or_else(invalid)? as u32;
    d[0] = json!(first & !3);
    d[2..5].clone_from_slice(&i.pos.map(|v| json!(v)));
    d[5] = json!(0);
    d[6] = json!(i.rot[1]);
    d[7] = json!(0);
    d.resize(size as usize / 4, json!(0));
    let (_, radius) = mh.template_model(template)?;
    let r = mh.f(l + 0x144)? + radius;
    let class = size_class(r, [mh.f(l + 0x15c)?, mh.f(l + 0x174)?, mh.f(l + 0x190)?]);
    c.push(engine(
        0x800232a4,
        json!([0x0800_0001 | class, "task", "dyn"]),
    ));
    c.push(mh.local_call(
        l + 0x1d8,
        json!(["task", 0, 0, mh.ptr(l + 0x1cc, 5)?, 1, 0]),
    )?);
    let child = i
        .child
        .then(|| json!({ "work+0x14": i.pos, "work+0x8": [i.rot[0], i.rot[1], i.rot[2]] }));
    Ok(json!({ "calls": c, "state": {
        "task": [mh.ptr(l + 0x64, 1)?, mh.ptr(l + 0x58, 1)?, mh.ptr(l + 0x70, 1)?],
        "mode": 0, "speed": mh.f(l + 0x22c)?.to_bits(), "dyn": d, "child": child } }))
    .and_then(|v| {
        // The callbacks the loader installs are the container's own.
        let t = &v["state"]["task"];
        if t != &json!([s(1, exec), s(1, display), s(1, dest)]) {
            return Err(invalid());
        }
        Ok(v)
    })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ContainerExecInput {
    pub exec: u32,
    pub range_out: bool,
    /// Work +0, +4, +6.
    pub mode: i8,
    pub flags: i16,
    pub timer: u16,
    /// Work +0xC (Y angle).
    pub rot: u32,
    /// Work +0x20 (travel) and +0x24 (acceleration), bit patterns.
    pub range: u32,
    pub accel: u32,
    /// Task +0x40, `dyn`+0x38..+0x44 offset, `dyn`+0x44 (TRUCK wheel angle).
    pub speed: u32,
    pub off: [u32; 3],
    #[serde(default)]
    pub wheel: i32,
    /// Task +0xC is set; `dyn`+4 (the registered model) is set.
    pub child: bool,
    pub obj: bool,
    /// _rename_EitherPlayerWithinSphere's answer.
    pub within: bool,
}

/// Where each container exec keeps its constants (the `addi` of every use).
struct Slide {
    axis: u32,
    /// Spring (bounce) factor, damping, acceleration pad, rest speed, the
    /// travel above which a sound plays, the sphere pad.
    spring: &'static [u32],
    damp: &'static [u32],
    pad: &'static [u32],
    rest: &'static [u32],
    sound: &'static [u32],
    sphere: u32,
    /// TRUCK's wheel: scale pcs and data factor pcs.
    wheel: Option<(&'static [u32], &'static [u32])>,
}
const SLIDE_26D4: Slide = Slide {
    axis: 0x2740,
    spring: &[0x2774, 0x27a8, 0x2960, 0x2994],
    damp: &[0x2794, 0x2980],
    pad: &[
        0x2840, 0x284c, 0x2870, 0x288c, 0x2a2c, 0x2a38, 0x2a5c, 0x2a78,
    ],
    rest: &[0x27f8, 0x29e4],
    sound: &[0x280c, 0x28e8, 0x29f8, 0x2adc],
    sphere: 0x2c64,
    wheel: None,
};
const SLIDE_300C: Slide = Slide {
    axis: 0x3078,
    spring: &[0x30ac, 0x30e0, 0x3298, 0x32cc],
    damp: &[0x30cc, 0x32b8],
    pad: &[
        0x3178, 0x3184, 0x31a8, 0x31c4, 0x3364, 0x3370, 0x3394, 0x33b0,
    ],
    rest: &[0x3130, 0x331c],
    sound: &[0x3144, 0x3220, 0x3330, 0x3414],
    sphere: 0x359c,
    wheel: None,
};
const SLIDE_9CC0: Slide = Slide {
    axis: 0x9d2c,
    spring: &[0x9d60, 0x9d94, 0x9f2c, 0x9f60],
    damp: &[0x9d80, 0x9f4c],
    pad: &[
        0x9dfc, 0x9e08, 0x9e2c, 0x9e48, 0x9fc8, 0x9fd4, 0x9ff8, 0xa014,
    ],
    rest: &[0x9de4, 0x9fb0],
    sound: &[],
    sphere: 0xa1fc,
    wheel: Some((&[0x9ea8, 0xa02c], &[0x9eb8, 0xa03c])),
};

/// The sliding offset's axis: the displacement of the `lfs` reading
/// `dyn`+0x38 (x) or +0x40 (z).
fn slide_axis(mh: &MetalHarbor<'_>, pc: u32) -> Result<usize> {
    match mh.disp(pc)? {
        56 => Ok(0),
        64 => Ok(2),
        _ => Err(invalid()),
    }
}

/// +0x26D4 (MHCONT, along z), +0x300C (MHCONT0, along x), +0x9CC0 (TRUCK):
/// slide out to the travel and spring back after 30 frames, then the other
/// way; +0x396C (MHCONTB) moves end to end with a 30-frame rest. Then the
/// child follows the offset point and the collision piece is moved.
pub fn container_exec(mh: &MetalHarbor<'_>, i: &ContainerExecInput) -> Result<Value> {
    let [loader, ..] = container(i.exec)?;
    let mut c = vec![engine(0x800655f4, json!(["task"]))];
    let (mut mode, mut timer, mut flags, mut wheel) = (i.mode, i.timer, i.flags, i.wheel);
    let mut speed = f32::from_bits(i.speed);
    let mut off = i.off.map(f32::from_bits);
    if i.range_out {
        return Ok(json!({ "calls": c, "state": { "mode": mode, "timer": timer,
            "flags": flags, "speed": i.speed, "off": i.off, "wheel": wheel } }));
    }
    let old = off;
    let range = f32::from_bits(i.range);
    let accel = f32::from_bits(i.accel);
    let (ex, sphere) = if i.exec == 0x396c {
        // MHCONTB: accelerate to the cap, stop at the end, rest 30 frames.
        let ax = slide_axis(mh, 0x3a34)?;
        let pad = mh.fs(&[
            0x39d0, 0x39dc, 0x3a00, 0x3a1c, 0x3b14, 0x3b20, 0x3b44, 0x3b60,
        ])?;
        let rest = mh.fs(&[0x3a68, 0x3bb4])?;
        let sound = mh.fs(&[0x3a88, 0x3ae0, 0x3bd4, 0x3c2c])?;
        let top = pad + accel;
        speed += pad * top;
        if speed > top {
            speed = top;
        }
        let dir = if mode != 2 { 1.0 } else { -1.0 };
        if mode != 2 {
            off[ax] += speed;
        } else {
            off[ax] -= speed;
        }
        let end = if mode != 2 {
            off[ax] > range
        } else {
            off[ax] < -range
        };
        if end {
            off[ax] = dir * range;
            speed = rest;
            if timer == 0 && range > sound {
                c.push(engine(0x8006b7ec, json!([2, 0, 0, 0, "work+0x14"])));
            }
            let t = timer;
            timer = timer.wrapping_add(1);
            if t > 30 {
                mode = if mode != 2 { 2 } else { 1 };
                timer = 0;
                if range > sound {
                    let n = if mode == 2 { 1 } else { 0 };
                    c.push(engine(0x8006b7ec, json!([n, 0, 0, 0, "work+0x14"])));
                }
            }
        }
        (true, mh.f(0x3da0)?)
    } else {
        let k = match i.exec {
            0x26d4 => &SLIDE_26D4,
            0x300c => &SLIDE_300C,
            0x9cc0 => &SLIDE_9CC0,
            _ => return Err(invalid()),
        };
        let ax = slide_axis(mh, k.axis)?;
        let spring = mh.fs(k.spring)?;
        let damp = mh.fs(k.damp)?;
        let pad = mh.fs(k.pad)?;
        let rest = mh.fs(k.rest)?;
        let sound = if k.sound.is_empty() {
            None
        } else {
            Some(mh.fs(k.sound)?)
        };
        let se = |c: &mut Vec<Value>, n: i32| {
            if sound.is_some_and(|s| range > s) {
                c.push(engine(0x8006b7ec, json!([n, 0, 0, 0, "work+0x14"])));
            }
        };
        let fwd = mode != 2;
        if timer != 0 {
            // Spring back toward the far end for 30 frames.
            if fwd {
                off[ax] += speed;
                if off[ax] > range {
                    off[ax] = range;
                    speed = spring * -speed;
                } else {
                    speed = damp * speed + spring * (range - off[ax]);
                }
            } else {
                off[ax] -= speed;
                if off[ax] < -range {
                    off[ax] = -range;
                    speed = spring * -speed;
                } else {
                    speed = damp * speed + spring * (range + off[ax]);
                }
            }
            let t = timer;
            timer = timer.wrapping_add(1);
            if t > 30 {
                mode = if fwd { 2 } else { 1 };
                timer = 0;
                speed = rest;
                se(&mut c, if fwd { 1 } else { 0 });
            }
        } else {
            let top = pad + accel;
            speed += pad * top;
            if speed > top {
                speed = top;
            }
            if let Some((scale, factor)) = k.wheel {
                // dyn+0x44 turns by fctiwz(scale * (speed * factor)); going
                // back it turns before the move (+0xA028), forward after.
                let at = mh.at(factor[0], 5)?;
                for &pc in factor {
                    if mh.at(pc, 5)? != at {
                        return Err(invalid());
                    }
                }
                let turn = fctiwz(mh.fs(scale)? * (speed * f32::from_bits(mh.word(5, at)?)));
                wheel = if fwd {
                    wheel.wrapping_add(turn)
                } else {
                    wheel.wrapping_sub(turn)
                };
            }
            if fwd {
                off[ax] += speed;
                if off[ax] > range {
                    off[ax] = range;
                    timer = timer.wrapping_add(1);
                    se(&mut c, 2);
                }
            } else {
                off[ax] -= speed;
                if off[ax] < -range {
                    off[ax] = -range;
                    timer = timer.wrapping_add(1);
                    se(&mut c, 2);
                }
            }
        }
        (i.exec == 0x9cc0, mh.f(k.sphere)?)
    };
    // The child at the offset point, then the collision piece (+0x2B04 on).
    let (push, pop) = (
        engine(0x80116098, json!([])),
        if ex {
            engine(0x801160e4, json!([]))
        } else {
            engine(0x80115dc8, json!([1]))
        },
    );
    let translate = |p: Value| {
        if ex {
            engine(0x8011616c, json!([p]))
        } else {
            engine(0x8011538c, json!([0, p]))
        }
    };
    let place = |c: &mut Vec<Value>| {
        c.extend([
            push.clone(),
            engine(0x801153e4, json!([0])),
            translate(json!("work+0x14")),
            engine(0x80115828, json!([0, i.rot])),
        ]);
    };
    let o: [u32; 3] = off.map(f32::to_bits);
    if i.child {
        place(&mut c);
        c.push(engine(
            0x80114f0c,
            json!([0, "dyn+0x38", "child.work+0x14"]),
        ));
        c.push(pop.clone());
    }
    if i.obj {
        let template = mh.at(loader + 0x88, 5)?;
        let (model, radius) = mh.template_model(template)?;
        let delta: [u32; 3] = [0, 1, 2].map(|k| (off[k] - old[k]).to_bits());
        place(&mut c);
        c.extend([
            engine(0x80114f0c, json!([0, "dyn+0x38", "dyn+0x8"])),
            engine(0x801154ec, json!([0, delta, "colli+0x10"])),
            translate(s(5, model + 0x18)),
            engine(0x80114f0c, json!([0, "dyn+0x38", "sphere"])),
            pop,
            engine(0x800398d8, json!(["sphere", (sphere + radius).to_bits()])),
        ]);
        if i.within {
            flags |= 0x100;
        } else {
            flags &= !0x100;
        }
    }
    Ok(
        json!({ "calls": c, "state": { "mode": mode, "timer": timer, "flags": flags,
        "speed": speed.to_bits(), "off": o, "wheel": wheel,
        "colli+0x2c": if i.obj { json!("colli+0x10") } else { Value::Null } } }),
    )
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContainerDisplayInput {
    pub display: u32,
    pub rot: u32,
    /// MHCONT0: .bss+0xB0 (a clipped model) when set.
    #[serde(default)]
    pub clip: Option<u32>,
    /// TRUCK: game state 0x801CC168 +0x7C and `dyn`+0x44.
    #[serde(default)]
    pub clock: u32,
    #[serde(default)]
    pub wheel: i32,
}

/// +0x2CB8, +0x35F0, +0x3DF4, +0xA250: draw the container at its place
/// moved by the offset; TRUCK adds a blinking lamp and two wheels.
pub fn container_display(mh: &MetalHarbor<'_>, i: &ContainerDisplayInput) -> Result<Value> {
    let d = i.display;
    container(d)?;
    let ex = d == 0x3df4;
    let mut c = vec![engine(0x80116e18, json!([mh.ptr(d + 0x20, 5)?]))];
    let tr = |p: Value| {
        if ex || d == 0xa250 {
            engine(0x8011616c, json!([p]))
        } else {
            engine(0x8011538c, json!([0, p]))
        }
    };
    c.push(if ex {
        engine(0x80116098, json!([]))
    } else {
        engine(0x80115d60, json!([0]))
    });
    c.extend([
        tr(json!("work+0x14")),
        engine(0x80115828, json!([0, i.rot])),
        tr(json!("dyn+0x38")),
        engine(0x8002b2e8, json!([0x2400])),
    ]);
    match d {
        0x2cb8 => c.push(engine(0x80031a28, json!([mh.ptr(0x2d1c, 5)?]))),
        0x35f0 => {
            mh.at(0x3654, 6)?; // .bss+0xB0
            c.push(match i.clip {
                Some(m) if m != 0 => engine(0x8011f514, json!([m])),
                _ => engine(0x80031a28, json!([mh.ptr(0x367c, 5)?])),
            });
        }
        0x3df4 => c.push(engine(0x80031a28, json!([mh.ptr(0x3e4c, 5)?]))),
        _ => c.push(engine(0x80031a28, json!([mh.loaded(0xa2ac, 4)?]))),
    }
    c.push(engine(0x8002b2d8, json!([0x2400])));
    if d == 0xa250 {
        let colour = blink(i.clock, 48, 20);
        let at = |pc| -> Result<Value> { Ok(s(5, mh.at(pc, 5)? + 8)) };
        c.extend([
            engine(0x80116098, json!([])),
            engine(0x8011616c, json!([at(0xa2c8)?])),
            engine(0x80031260, json!([mh.loaded(0xa2d8, 4)?, colour])),
            engine(0x801160e4, json!([])),
        ]);
        for (t, m) in [(0xa32c, 0xa34c), (0xa364, 0xa384)] {
            c.extend([
                engine(0x80116098, json!([])),
                engine(0x8011616c, json!([at(t)?])),
                engine(0x80115c60, json!([0, i.wheel])),
                engine(0x80031a28, json!([mh.loaded(m, 4)?])),
                engine(0x801160e4, json!([])),
            ]);
        }
        c.push(engine(0x80115dc8, json!([2])));
    } else {
        c.push(if ex {
            engine(0x801160e4, json!([]))
        } else {
            engine(0x80115dc8, json!([1]))
        });
    }
    Ok(json!({ "calls": c, "state": {} }))
}

/// +0x2670, +0x2FA8, +0x3908, +0x9C5C: unregister the collision piece if
/// registered, clear task +0x40, free the block.
pub fn container_dest(dest: u32, obj: bool) -> Result<Value> {
    if container(dest)?[3] != dest {
        return Err(Error::invalid("not a stage 10 container destructor"));
    }
    let mut c = Vec::new();
    if obj {
        c.push(engine(0x800231cc, json!(["task", "dyn"])));
    }
    c.push(engine(0x80119828, json!(["dyn"])));
    Ok(json!({ "calls": c, "state": { "task+0x40": 0, "dyn": 0 } }))
}

impl MetalHarbor<'_> {
    /// The model of the object a data word points at: the `addi` at `pc`
    /// names an object, its word `d` points at another object (section 5),
    /// whose +4 is returned (`lwz rX,d(rX); lwz rX,4(rX)`).
    fn child_model(&self, pc: u32, d: u32) -> Result<Value> {
        let obj = self.at(pc, 5)? + d;
        match self.rel.local_pointer(5, obj)? {
            Some((5, off)) => self.data_value(5, off + 4),
            _ => Err(invalid()),
        }
    }
}

/// The lamp colour fn_80031260 gets from the game clock (0x801CC168
/// +0x7C): 0xFF808080 (dim) while `clock % period` is at most `dim`, else
/// white (-1); the period, threshold and colours are the routine's
/// immediates.
fn blink(clock: u32, period: u32, dim: u32) -> u32 {
    if clock % period <= dim {
        0xff80_8080
    } else {
        u32::MAX
    }
}

// ---- MHSLIDER, STOPPER, TANK: three loaders of one shape (+0x8FEC,
// +0x93F8, +0x97DC) whose block (`dyn`) lives at task +0x40.

/// Loader, exec, display, destructor of MHSLIDER, STOPPER and TANK.
pub const SLIDERS: [[u32; 4]; 3] = [
    [0x8fec, 0x8f40, 0x8de8, 0x8ed8],
    [0x93f8, 0x934c, 0x9210, 0x92d4],
    [0x97dc, 0x9730, 0x961c, 0x96c8],
];
fn slider(routine: u32) -> Result<[u32; 4]> {
    SLIDERS
        .into_iter()
        .find(|c| c.contains(&routine))
        .ok_or_else(|| Error::invalid("not a stage 10 slider routine"))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SliderLoadInput {
    pub loader: u32,
    pub range_out: bool,
    pub alloc: bool,
    /// Work +0xC (Y angle) and +0x14 position.
    pub rot: u32,
    pub pos: [u32; 3],
}

/// A slider loader: allocate the 68-byte block at task +0x40, copy the
/// template object into it at the task's place, register it (size class
/// from the model radius), transform the model centre into `dyn`+0x38 and
/// install the callbacks.
pub fn slider_load(mh: &MetalHarbor<'_>, i: &SliderLoadInput) -> Result<Value> {
    let [l, exec, display, dest] = slider(i.loader)?;
    if l != i.loader {
        return Err(Error::invalid("not a stage 10 slider loader"));
    }
    let mut c = vec![engine(0x800655f4, json!(["task"]))];
    let none =
        |c: Vec<Value>| json!({ "calls": c, "state": { "task": Value::Null, "dyn": Value::Null } });
    if i.range_out {
        return Ok(none(c));
    }
    let size = mh.li(l + 0x3c, 4)?;
    c.push(engine(0x801197fc, json!([1, size])));
    if !i.alloc {
        return Ok(none(c));
    }
    let template = mh.at(l + 0x5c, 5)?;
    let mut d: Vec<Value> = (0..14)
        .map(|k| mh.data_value(5, template + 4 * k))
        .collect::<Result<_>>()?;
    let first = d[0].as_u64().ok_or_else(invalid)? as u32;
    d[0] = json!(first & !3);
    d[2..5].clone_from_slice(&i.pos.map(|v| json!(v)));
    d[5] = json!(0);
    d[6] = json!(i.rot);
    d[7] = json!(0);
    d.resize(size as usize / 4, json!(0));
    let (model, radius) = mh.template_model(template)?;
    let r = mh.f(l + 0x114)? + radius;
    let class = size_class(r, [mh.f(l + 0x12c)?, mh.f(l + 0x144)?, mh.f(l + 0x160)?]);
    let base = mh.li(l + 0x10c, 29)? as u32; // li r29,1
    c.extend([
        engine(0x800232a4, json!([base | class, "task", "dyn"])),
        engine(0x80116098, json!([])),
        engine(0x801153e4, json!([0])),
        engine(0x8011538c, json!([0, "dyn+0x8"])),
        engine(0x80115828, json!([0, i.rot])),
        engine(0x80114f0c, json!([0, s(5, model + 0x18), "dyn+0x38"])),
        engine(0x80115dc8, json!([1])),
    ]);
    let task = [
        mh.ptr(l + 0x1ec, 1)?,
        mh.ptr(l + 0x1e0, 1)?,
        mh.ptr(l + 0x1f8, 1)?,
    ];
    if task != [s(1, exec), s(1, display), s(1, dest)] {
        return Err(invalid());
    }
    Ok(json!({ "calls": c, "state": { "task": task, "dyn": d } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SliderExecInput {
    pub exec: u32,
    pub range_out: bool,
    /// Task +0x40 is set; its +4 (the registered model) is set.
    pub dyn_set: bool,
    pub obj: bool,
    pub within: bool,
    /// Work +4.
    pub flags: i16,
}

/// +0x8F40, +0x934C, +0x9730: set flag 0x100 of work +4 while a player is
/// within the model's sphere (pad + radius around `dyn`+0x38), else clear it.
pub fn slider_exec(mh: &MetalHarbor<'_>, i: &SliderExecInput) -> Result<Value> {
    let [l, e, ..] = slider(i.exec)?;
    if e != i.exec {
        return Err(Error::invalid("not a stage 10 slider exec"));
    }
    let mut c = vec![engine(0x800655f4, json!(["task"]))];
    let mut flags = i.flags;
    if !i.range_out && i.dyn_set && i.obj {
        let (_, radius) = mh.template_model(mh.at(l + 0x5c, 5)?)?;
        let r = mh.f(e + 0x54)? + radius;
        c.push(engine(0x800398d8, json!(["dyn+0x38", r.to_bits()])));
        if i.within {
            flags |= 0x100;
        } else {
            flags &= !0x100;
        }
    }
    Ok(json!({ "calls": c, "state": { "flags": flags } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SliderDisplayInput {
    pub display: u32,
    pub rot: u32,
    /// Game state 0x801CC168 +0x7C (MHSLIDER, STOPPER).
    #[serde(default)]
    pub clock: u32,
    /// Work +0x24 (TANK's level, bit pattern).
    #[serde(default)]
    pub level: u32,
}

/// +0x8DE8 (MHSLIDER: body and a blinking lamp raised by a fixed offset),
/// +0x9210 (STOPPER: body and blinking lamp), +0x961C (TANK: body and the
/// part raised to work +0x24).
pub fn slider_display(mh: &MetalHarbor<'_>, i: &SliderDisplayInput) -> Result<Value> {
    let d = i.display;
    if slider(d)?[2] != d {
        return Err(Error::invalid("not a stage 10 slider display"));
    }
    let mut c = vec![
        engine(0x80116e18, json!([mh.ptr(d + 0x20, 5)?])),
        engine(0x80115d60, json!([0])),
        engine(0x8011538c, json!([0, "work+0x14"])),
        engine(0x80115828, json!([0, i.rot])),
        engine(0x80031a28, json!([mh.loaded(d + 0x4c, 4)?])),
    ];
    match d {
        0x8de8 => {
            let yz = mh.fs(&[0x8e54, 0x8e60])?.to_bits();
            c.extend([
                engine(0x80115bd8, json!([0, mh.f(0x8e48)?.to_bits(), yz, yz])),
                engine(
                    0x80031260,
                    json!([mh.child_model(0x8e70, 44)?, blink(i.clock, 48, 20)]),
                ),
            ]);
        }
        0x9210 => c.push(engine(
            0x80031260,
            json!([mh.child_model(0x926c, 44)?, blink(i.clock, 48, 20)]),
        )),
        _ => {
            let k = mh.fs(&[0x967c, 0x968c])?.to_bits();
            c.extend([
                engine(0x80115bd8, json!([0, k, i.level, k])),
                engine(0x80031a28, json!([mh.child_model(0x969c, 44)?])),
            ]);
        }
    }
    c.push(engine(0x80115dc8, json!([1])));
    Ok(json!({ "calls": c, "state": {} }))
}

/// +0x8ED8, +0x92D4, +0x96C8: when task +0x40 is set, unregister it if its
/// model is registered, free it and clear task +0x40.
pub fn slider_dest(dest: u32, dyn_set: bool, obj: bool) -> Result<Value> {
    if slider(dest)?[3] != dest {
        return Err(Error::invalid("not a stage 10 slider destructor"));
    }
    let mut c = Vec::new();
    if dyn_set {
        if obj {
            c.push(engine(0x800231cc, json!(["task", "dyn"])));
        }
        c.push(engine(0x80119828, json!(["dyn"])));
    }
    Ok(json!({ "calls": c, "state": { "task+0x40": 0 } }))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SliderDestInput {
    dest: u32,
    dyn_set: bool,
    obj: bool,
}

impl MetalHarbor<'_> {
    /// The constant a `lis rD,hi` at `pc` and `addi rD,rD,lo` after it build.
    fn lis_addi(&self, pc: u32) -> Result<u32> {
        let (hi, lo) = (self.word(1, pc)?, self.word(1, pc + 4)?);
        if hi >> 26 != 15 || lo >> 26 != 14 || (hi >> 21) & 31 != (lo >> 21) & 31 {
            return Err(Error::invalid(format!("no lis/addi at +0x{pc:x}")));
        }
        Ok((hi << 16).wrapping_add(lo as u16 as i16 as i32 as u32))
    }
}

// ---- BOARD (+0x1AD4): a board that follows its player while the player
// is not boarding (actions 121..=144) and is drawn with the player's board
// texture.

/// +0x1AD4: work +1 = bit 0 of work +8 (the player index); install the
/// exec +0x1B1C, display +0x1CA8 and destructor +0x1F18.
pub fn board_load(mh: &MetalHarbor<'_>, flags: u32) -> Result<Value> {
    Ok(json!({ "calls": [], "state": {
        "index": flags & 1,
        "task": [mh.ptr(0x1af0, 1)?, mh.ptr(0x1b08, 1)?, mh.ptr(0x1afc, 1)?] } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardPlayer {
    /// playertwp: +8, +0xC, +0x10 angles, +0x14 position.
    pub rot: [u32; 3],
    pub pos: [u32; 3],
    /// playermwp +0..+8.
    pub mwp: [u32; 3],
    /// playerpwp +0x174, +0x178 (action), +0x184 (bit pattern).
    pub p174: i16,
    pub action: i16,
    pub p184: u32,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardExecInput {
    /// Work +0 and +1 (player index).
    pub mode: i8,
    pub index: i8,
    /// playertwp[index] (None: null).
    pub player: Option<BoardPlayer>,
}

/// +0x1B1C: outside actions 121..=144 copy the player's place (Y angle
/// mirrored as 0x8000 - angle) and motion words, mode 1, and the board
/// texture index (+0x178) and +0x184 when +0x174 > 2; during them mode 1
/// becomes 2, and mode 2 calls fn_800DE3F8 and CheckRangeOut.
pub fn board_exec(i: &BoardExecInput) -> Result<Value> {
    if i.index < 0 {
        return Err(Error::invalid("negative player index"));
    }
    let mut c = Vec::new();
    let mut st = json!({ "mode": i.mode });
    if let Some(p) = &i.player {
        if (121..=144).contains(&p.action) {
            let mut mode = i.mode;
            if mode == 1 {
                mode = 2;
            }
            if mode == 2 {
                c.extend([
                    engine(0x800de3f8, json!(["task"])),
                    engine(0x800655f4, json!(["task"])),
                ]);
            }
            st["mode"] = json!(mode);
        } else {
            st = json!({ "mode": 1, "pos": p.pos,
                "rot": [p.rot[0], 0x8000u32.wrapping_sub(p.rot[1]), p.rot[2]],
                "dyn": p.mwp });
            if p.p174 > 2 {
                st["dyn+0x18"] = json!(i32::from(p.action));
                st["dyn+0x1c"] = json!(p.p184);
            }
        }
    }
    Ok(json!({ "calls": c, "state": st }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BoardDisplayInput {
    pub mode: i8,
    pub index: i8,
    /// playertwp[index] is set.
    pub twp: bool,
    /// Work +8, +0xC, +0x10.
    pub rot: [u32; 3],
    /// playerpwp +0x174, +0x178, +0x184 (bit pattern).
    pub p174: i16,
    pub action: i16,
    pub p184: u32,
    /// `dyn`+0x18 (texture index) and +0x1C (bit pattern).
    pub tex: i32,
    pub blend: u32,
    /// 0x801E5D28 entry +4 at index action + 24 and at tex + 24.
    pub entry_action: u32,
    pub entry_tex: u32,
}

/// +0x1CA8 (with the matrix callback +0x1C80): draw the board at its place,
/// in mode 1 with the player's board texture (fn_8011E214, or the blend
/// fn_8011E19C for texture indexes 145..=168 when +0x174 is 2).
pub fn board_display(mh: &MetalHarbor<'_>, i: &BoardDisplayInput) -> Result<Value> {
    if i.index < 0 {
        return Err(Error::invalid("negative player index"));
    }
    let mut c = Vec::new();
    if !i.twp {
        return Ok(json!({ "calls": c, "state": {} }));
    }
    let obj = mh.ptr(0x1d90, 5)?;
    let rot_ex = s(5, mh.at(0x1d9c, 5)? + 20); // addi r3,r3,20 at +0x1DA0
    let matrix = mh.ptr(0x1db0, 6)?;
    let place = |c: &mut Vec<Value>| {
        c.extend([
            engine(0x8011616c, json!(["work+0x14"])),
            engine(0x801158a8, json!([0, i.rot[2]])),
            engine(0x80115c60, json!([0, i.rot[0]])),
            engine(0x80115828, json!([0, i.rot[1].wrapping_add(0x4000)])),
        ]);
    };
    c.extend([
        engine(0x8011c3a0, json!([mh.ptr(0x1d34, 1)?])),
        engine(0x80116e18, json!([mh.ptr(0x1d40, 5)?])),
        engine(0x80116098, json!([])),
    ]);
    let plain = |c: &mut Vec<Value>| -> Result<()> {
        c.extend([
            engine(0x8011df44, json!([obj.clone()])),
            engine(0x80115f68, json!([rot_ex.clone(), 0])),
            engine(0x80114f6c, json!([matrix.clone()])),
        ]);
        Ok(())
    };
    if i.mode != 1 {
        place(&mut c);
        plain(&mut c)?;
    } else if i.entry_action != 0 {
        place(&mut c);
        if i.p174 != 2 {
            c.push(engine(0x8011e214, json!([obj, i.entry_action, i.p184])));
        } else if (145..=168).contains(&i.tex.wrapping_add(24)) {
            let w = mh.f(0x1e78)?.to_bits();
            c.push(engine(
                0x8011e19c,
                json!([obj, [i.entry_tex, i.entry_action, i.blend, w], i.p184]),
            ));
        } else {
            plain(&mut c)?;
        }
    }
    c.extend([
        engine(0x8002b2e8, json!([0x2400])),
        engine(0x80115928, json!([0, mh.ptr(0x1ed8, 6)?])),
        engine(0x8011e244, json!([mh.ptr(0x1ee4, 5)?])),
        engine(0x8002b2d8, json!([0x2400])),
        engine(0x801160e4, json!([])),
        engine(0x8011c3a0, json!([0])),
    ]);
    Ok(json!({ "calls": c, "state": {} }))
}

/// +0x1C80, the matrix callback the BOARD display installs: njGetMatrix
/// into .bss+0x80.
pub fn board_matrix(mh: &MetalHarbor<'_>) -> Result<Value> {
    Ok(json!({ "calls": [engine(0x80114f6c, json!([mh.ptr(0x1c90, 6)?]))], "state": {} }))
}

// ---- BOAB (+0x1F2C): a breakable box that bursts (module +0x12958) when
// its collision is hit.

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BoabLoadInput {
    pub range_out: bool,
    /// Work +0 before the loader.
    pub mode: i8,
    /// Task +0x30 (the set entry) is set, and fn_80065388's answer.
    pub set_entry: bool,
    pub condition: i8,
}

/// +0x1F2C: install +0x2040/+0x21D4/+0x1FF4, read the set entry's condition
/// flag into work +0 and set up the collision (the second list when set).
pub fn boab_load(mh: &MetalHarbor<'_>, i: &BoabLoadInput) -> Result<Value> {
    let mut c = vec![engine(0x800655f4, json!(["task"]))];
    if i.range_out {
        return Ok(json!({ "calls": c, "state": { "task": Value::Null } }));
    }
    let mut mode = i.mode;
    if i.set_entry {
        c.push(engine(0x80065388, json!(["task"])));
        mode = i.condition;
    }
    let colli = if mode != 0 {
        s(5, mh.at(0x1fac, 5)? + 48) // addi r4,r4,48 at +0x1FB0
    } else {
        mh.ptr(0x1fcc, 5)?
    };
    c.push(engine(0x8000f8b8, json!(["task", colli, 1, 4])));
    Ok(json!({ "calls": c, "state": { "mode": mode,
        "task": [mh.ptr(0x1f68, 1)?, mh.ptr(0x1f5c, 1)?, mh.ptr(0x1f74, 1)?] } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BoabExecInput {
    pub range_out: bool,
    pub mode: i8,
    /// Work +0x2C (collision) is set and its +4 bit 0 (hit).
    pub colli: bool,
    pub hit: bool,
    /// Work +0xC, +0x14 position; playermwp[0] +4 and playertwp[0] +0x18
    /// (bit patterns).
    pub rot: u32,
    pub pos: [u32; 3],
    pub mwp_y: u32,
    pub player_y: u32,
}

/// +0x2040: when hit, burst (module +0x12958 with the debris table
/// .data+0x8B844), play the sounds and free the task; else feed the
/// collision while unbroken or while player 0 is below it.
pub fn boab_exec(mh: &MetalHarbor<'_>, i: &BoabExecInput) -> Result<Value> {
    let mut c = vec![engine(0x800655f4, json!(["task"]))];
    let mut mode = i.mode;
    if !i.range_out {
        if mode == 0 && i.colli && i.hit {
            let a = mh.fs(&[0x20b8, 0x20ec])?;
            let up = (mh.f(0x20c8)? + f32::from_bits(i.mwp_y)).to_bits();
            mode = 1;
            let flags = mh.lis_addi(0x213c)?; // lis r8,1; addi r8,r8,-32752
            c.extend([
                mh.local_call(
                    0x2150,
                    json!([
                        i.pos,
                        i.rot,
                        mh.ptr(0x210c, 5)?,
                        mh.li(0x2110, 6)?,
                        mh.fs(&[0x2118, 0x2124])?.to_bits(),
                        mh.fs(&[0x2118, 0x2124])?.to_bits(),
                        mh.f(0x2130)?.to_bits(),
                        [a.to_bits(), up, a.to_bits()],
                        flags,
                        mh.f(0x2148)?.to_bits()
                    ]),
                )?,
                engine(0x8006b978, json!([5, 0, 0, 0])),
                engine(0x8002fb2c, json!([0, 6, 25, 0])),
                engine(0x80071ae0, json!(["task"])),
            ]);
        } else if mode == 0 || f32::from_bits(i.player_y) < f32::from_bits(i.pos[1]) {
            c.push(engine(0x8000f4ec, json!(["task"])));
        }
    }
    Ok(json!({ "calls": c, "state": { "mode": mode } }))
}

/// +0x21D4: draw the box while unbroken.
pub fn boab_display(mh: &MetalHarbor<'_>, mode: i8, rot: u32) -> Result<Value> {
    let mut c = Vec::new();
    if mode == 0 {
        c.extend([
            engine(0x80116098, json!([])),
            engine(0x8011616c, json!(["work+0x14"])),
            engine(0x80115828, json!([0, rot])),
            engine(0x80116e18, json!([mh.ptr(0x221c, 5)?])),
            engine(0x80031a28, json!([mh.loaded(0x2228, 4)?])),
            engine(0x801160e4, json!([])),
        ]);
    }
    Ok(json!({ "calls": c, "state": {} }))
}

/// +0x1FF4: store work +0 back as the set entry's condition flag.
pub fn boab_dest(set_entry: bool, mode: i8) -> Value {
    let c = if set_entry {
        vec![engine(0x8006539c, json!(["task", mode as u8]))]
    } else {
        Vec::new()
    };
    json!({ "calls": c, "state": {} })
}

// ---- BCOL (+0x23BC): an invisible trigger that makes the player who
// touches it act (SetInputP 34 or 15) and may advance the frame task.

/// +0x23BC: install the exec +0x2260 and destructor +0x224C when in range.
pub fn bcol_load(mh: &MetalHarbor<'_>, range_out: bool) -> Result<Value> {
    let c = vec![engine(0x800655f4, json!(["task"]))];
    let task = if range_out {
        Value::Null
    } else {
        json!([mh.ptr(0x23ec, 1)?, Value::Null, mh.ptr(0x23f8, 1)?])
    };
    Ok(json!({ "calls": c, "state": { "task": task } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BcolExecInput {
    pub range_out: bool,
    pub mode: i8,
    /// Work +8 (bit 0: the strong kind), +0x20 radius (bit pattern).
    pub kind: u32,
    pub radius: u32,
    /// _rename_EitherPlayerWithinSphere's answer (player number + 1).
    pub within: i32,
    /// Task +0x30 is set.
    pub set_entry: bool,
    /// The frame task (.bss+0xB8): its work mode, if present.
    pub frame: Option<i8>,
}

/// +0x2260.
pub fn bcol_exec(mh: &MetalHarbor<'_>, i: &BcolExecInput) -> Result<Value> {
    let mut c = vec![engine(0x800655f4, json!(["task"]))];
    let mut mode = i.mode;
    let mut frame = i.frame.map(|m| json!({ "mode": m }));
    if !i.range_out && mode == 0 {
        let r = mh.f(0x22a8)? + f32::from_bits(i.radius);
        c.push(engine(0x800398d8, json!(["work+0x14", r.to_bits()])));
        if i.within != 0 {
            mode = 1;
            let strong = i.kind & 1 != 0;
            let act = if strong {
                mh.li(0x22e4, 4)?
            } else {
                mh.li(0x2320, 4)?
            };
            c.push(engine(0x8003a508, json!([i.within - 1, act, 0])));
            if strong {
                c.push(engine(
                    if i.set_entry { 0x800653b0 } else { 0x80071ae0 },
                    json!(["task"]),
                ));
            }
            mh.at(0x2330, 6)?; // .bss+0xB8
            if let Some(m) = i.frame {
                if m != 6 && strong {
                    frame = Some(json!({ "mode": 6, "timer": 0 }));
                }
            }
        }
    }
    Ok(json!({ "calls": c, "state": { "mode": mode, "frame": frame } }))
}

// ---- RADER (+0x8AE0): a radar dish turning by work +0xC every frame;
// SIGNAL (+0x8C40): a lamp post with collision; PIC (+0x8A88) is a thin
// loader (see THIN_LOADERS).

/// +0x8AE0 (RADER) and +0x8C40 (SIGNAL, SIGNAL_F) loaders.
pub fn rader_signal_load(mh: &MetalHarbor<'_>, loader: u32, range_out: bool) -> Result<Value> {
    let mut c = vec![engine(0x800655f4, json!(["task"]))];
    let base = match loader {
        0x8ae0 | 0x8c40 => loader,
        _ => return Err(Error::invalid("not RADER or SIGNAL")),
    };
    if range_out {
        return Ok(json!({ "calls": c, "state": { "task": Value::Null } }));
    }
    let task = [
        mh.ptr(base + 0x3c, 1)?,
        mh.ptr(base + 0x30, 1)?,
        mh.ptr(base + 0x48, 1)?,
    ];
    if base == 0x8c40 {
        c.push(engine(
            0x8000f8b8,
            json!(["task", mh.ptr(0x8c98, 5)?, 1, 4]),
        ));
    }
    Ok(json!({ "calls": c, "state": { "task": task } }))
}

/// +0x8B58: work +6 (16 bits) += work +0xC.
pub fn rader_exec(range_out: bool, angle: u16, speed: u32) -> Value {
    let c = vec![engine(0x800655f4, json!(["task"]))];
    let angle = if range_out {
        angle
    } else {
        angle.wrapping_add(speed as u16)
    };
    json!({ "calls": c, "state": { "angle": angle } })
}

/// +0x8BAC: the base, then the dish at its child object's place turned by
/// work +6.
pub fn rader_display(mh: &MetalHarbor<'_>, angle: u16) -> Result<Value> {
    let obj = mh.at(0x8bdc, 5)?;
    let child = match mh.rel.local_pointer(5, obj + 44)? {
        Some((5, off)) => off,
        _ => return Err(invalid()),
    };
    Ok(json!({ "calls": [
        engine(0x80116e18, json!([mh.ptr(0x8bd0, 5)?])),
        engine(0x80115d60, json!([0])),
        engine(0x8011538c, json!([0, "work+0x14"])),
        engine(0x80121bdc, json!([mh.data_value(5, obj + 4)?])),
        engine(0x8011538c, json!([0, s(5, child + 8)])),
        engine(0x80115828, json!([0, angle])),
        engine(0x80121bdc, json!([mh.data_value(5, child + 4)?])),
        engine(0x80115dc8, json!([1])),
    ], "state": {} }))
}

/// +0x8CD0: feed the collision while in range.
pub fn signal_exec(range_out: bool) -> Value {
    let mut c = vec![engine(0x800655f4, json!(["task"]))];
    if !range_out {
        c.push(engine(0x8000f4ec, json!(["task"])));
    }
    json!({ "calls": c, "state": {} })
}

/// +0x8D18: the post, then its lamp at the child object's place, white for
/// the first 20 of every 40 frames of the game clock, else 0xFF808080.
pub fn signal_display(mh: &MetalHarbor<'_>, rot: u32, clock: u32) -> Result<Value> {
    let obj = mh.at(0x8d48, 5)?;
    let child = match mh.rel.local_pointer(5, obj + 44)? {
        Some((5, off)) => off,
        _ => return Err(invalid()),
    };
    let colour = if clock % 40 < 20 {
        u32::MAX
    } else {
        0xff80_8080
    };
    Ok(json!({ "calls": [
        engine(0x80116e18, json!([mh.ptr(0x8d3c, 5)?])),
        engine(0x80115d60, json!([0])),
        engine(0x8011538c, json!([0, "work+0x14"])),
        engine(0x80115828, json!([0, rot])),
        engine(0x80031a28, json!([mh.data_value(5, obj + 4)?])),
        engine(0x8011538c, json!([0, s(5, child + 8)])),
        engine(0x80031260, json!([mh.data_value(5, child + 4)?, colour])),
        engine(0x80115dc8, json!([1])),
    ], "state": {} }))
}

/// The destructors +0x1F18 (BOARD), +0x224C (BCOL), +0x8B44 (RADER),
/// +0x8CBC (SIGNAL) and +0xAE94 (stage task) only read their work pointer
/// into their own frame: no effect.
pub const NO_EFFECT_DESTRUCTORS: [u32; 5] = [0x1f18, 0x224c, 0x8b44, 0x8cbc, 0xae94];

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SmallInput {
    #[serde(default)]
    range_out: bool,
    #[serde(default)]
    loader: u32,
    #[serde(default)]
    flags: u32,
    #[serde(default)]
    mode: i8,
    #[serde(default)]
    rot: u32,
    #[serde(default)]
    angle: u16,
    #[serde(default)]
    speed: u32,
    #[serde(default)]
    clock: u32,
    #[serde(default)]
    set_entry: bool,
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
        "bridge_load" => bridge_load(&mh, &parse(input)?),
        "thin_load" => thin_load(&mh, parse::<ThinInput>(input)?.loader),
        "bridge_exec" => bridge_exec(&mh, &parse(input)?),
        "bridge_display" => bridge_display(&mh, &parse(input)?),
        "bridge_dest" => Ok(bridge_dest(parse::<BridgeDestInput>(input)?.obj)),
        "container_load" => container_load(&mh, &parse(input)?),
        "container_exec" => container_exec(&mh, &parse(input)?),
        "container_display" => container_display(&mh, &parse(input)?),
        "container_dest" => {
            let i: DestInput = parse(input)?;
            container_dest(i.dest, i.obj)
        }
        "slider_load" => slider_load(&mh, &parse(input)?),
        "slider_exec" => slider_exec(&mh, &parse(input)?),
        "slider_display" => slider_display(&mh, &parse(input)?),
        "slider_dest" => {
            let i: SliderDestInput = parse(input)?;
            slider_dest(i.dest, i.dyn_set, i.obj)
        }
        "board_load" => board_load(&mh, parse::<SmallInput>(input)?.flags),
        "board_exec" => board_exec(&parse(input)?),
        "board_display" => board_display(&mh, &parse(input)?),
        "board_matrix" => board_matrix(&mh),
        "boab_load" => boab_load(&mh, &parse(input)?),
        "boab_exec" => boab_exec(&mh, &parse(input)?),
        "boab_display" => {
            let i: SmallInput = parse(input)?;
            boab_display(&mh, i.mode, i.rot)
        }
        "boab_dest" => {
            let i: SmallInput = parse(input)?;
            Ok(boab_dest(i.set_entry, i.mode))
        }
        "bcol_load" => bcol_load(&mh, parse::<SmallInput>(input)?.range_out),
        "bcol_exec" => bcol_exec(&mh, &parse(input)?),
        "rader_signal_load" => {
            let i: SmallInput = parse(input)?;
            rader_signal_load(&mh, i.loader, i.range_out)
        }
        "rader_exec" => {
            let i: SmallInput = parse(input)?;
            Ok(rader_exec(i.range_out, i.angle, i.speed))
        }
        "rader_display" => rader_display(&mh, parse::<SmallInput>(input)?.angle),
        "signal_exec" => Ok(signal_exec(parse::<SmallInput>(input)?.range_out)),
        "signal_display" => {
            let i: SmallInput = parse(input)?;
            signal_display(&mh, i.rot, i.clock)
        }
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
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BridgeDestInput {
    obj: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DestInput {
    dest: u32,
    obj: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ThinInput {
    loader: u32,
}
