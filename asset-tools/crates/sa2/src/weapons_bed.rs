//! Weapons Bed (story level 12, stg12D.rel): the stage program "STG12" that
//! `_prolog` (+0x5F50) hands to LoadSubprogram and the background task its
//! setup creates. All offsets are section-1 (code) offsets of stg12D.rel;
//! engine calls are reported at their GSNE8P main.dol addresses.
//!
//! | Offset | Size | Routine |
//! | --- | --- | --- |
//! | +0x5F7C | 0x32C | stage setup (the program's first entry) |
//! | +0x62A8 | 0x130 | stage teardown (second entry) |
//! | +0x63D8 | 0x4 | stage program exec (third entry): `blr`, no effect |
//! | +0x63DC | 0x30 | background task exec: drifts the five-entry list by (0, 0) |
//! | +0x640C | 0x4 | background task destructor: `blr`, no effect |
//! | +0x64B8 | 0xDC | background task display: sky model at the camera |
//! | +0x6410 | 0xA8 | background task second display: far model at the camera |
//!
//! +0x6410 is the stg12D copy of the stage 20 routine `stage20` replays as
//! its secondary background callback (fn_20_1CC4); that port reads stg20D's
//! own relocations, so this one reads stg12D's. The drift list routine +0x0
//! (shared with stg15/20/51, the same code as Metal Harbor's +0x13DC) and
//! the far model draw +0xF4 are reported as `module` calls.
//!
//! Constants and data pointers are read from the module through the
//! relocation of the instruction that loads them. The Steam
//! `resource/gd_PC/stg12D.rel` has the same section bytes as the GameCube
//! disc copy, so either can be passed; engine addresses are always the
//! GSNE8P ones.
use crate::{
    city_rel::{CityRel, Reference},
    manifest, Error, Result,
};
use serde::Deserialize;
use serde_json::{json, Value};

/// GameCube disc `stg12D.rel` (module 12).
pub const GC_REL_SHA256: &str = "9c421f5814c6b05c04ddec058cfa41013c5e45bb8af268b38aa28fb8ff253dfa";
/// Installed Steam `resource/gd_PC/stg12D.rel` (module 11, same section bytes).
pub const STEAM_REL_SHA256: &str =
    "e1b85a8f1614c2c63f0777143987abdfbea305817fbd8d86d474d0d46e74de42";
pub const REL_LEN: usize = 1_300_884;

fn invalid() -> Error {
    Error::invalid("unexpected stage 12 code")
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

pub struct WeaponsBed<'a> {
    rel: CityRel<'a>,
}

impl<'a> WeaponsBed<'a> {
    pub fn read(bytes: &'a [u8]) -> Result<Self> {
        if bytes.len() != REL_LEN {
            return Err(Error::invalid("unsupported stage 12 REL revision"));
        }
        let module = match manifest::sha256(bytes).as_str() {
            GC_REL_SHA256 => 12,
            STEAM_REL_SHA256 => 11,
            _ => return Err(Error::invalid("unsupported stage 12 REL revision")),
        };
        Ok(Self {
            rel: CityRel::read_identified(bytes, module)?,
        })
    }
    /// The section offset the `addi` at `pc` (relocated at `pc + 2`) points at.
    fn at(&self, pc: u32, section: u8) -> Result<u32> {
        match self.rel.reference(1, pc + 2) {
            Some(Reference {
                module,
                kind: 4,
                section: s,
                addend,
            }) if s == section && module == self.rel.module_id() => Ok(addend),
            _ => Err(Error::invalid(format!(
                "unexpected stage 12 code at +0x{pc:x}"
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
    /// The single-precision constant the `addi` at `pc` addresses (.rodata).
    fn f(&self, pc: u32) -> Result<u32> {
        self.word(4, self.at(pc, 4)?)
    }
    /// The target of the module-local `bl` at `pc` (no relocation).
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

/// A word that is either a plain value or one of the module's own
/// addresses (`"s1+0x63dc"`).
pub type Word = Value;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupInput {
    /// fn_80065048's return, kept at .bss+0x84.
    pub handle: u32,
    /// CreateElementalTask returned a task.
    pub task: bool,
    /// The words at 0x801A2A18 +0, +8, +0x24 and +0x28 before the setup.
    pub slots: [u32; 4],
}

/// +0x5F7C. Returns the ordered calls and the final value of every location
/// it writes: the 0x801A2A18 hooks, the trail plugin table 0x801E77D8 and
/// the background task's +0x20/+0x14/+0x18 callbacks.
pub fn setup(wb: &WeaponsBed<'_>, i: &SetupInput) -> Result<Value> {
    let p5 = |pc| wb.ptr(pc, 5);
    let f = |pc| wb.f(pc);
    wb.at(0x6000, 6)?; // .bss+0x84 = handle
    let mut c = vec![
        engine(0x800324e4, json!([128, p5(0x5f98)?])),
        engine(0x80072270, json!([p5(0x5fa4)?, p5(0x5fb0)?])),
        engine(0x80072218, json!([p5(0x5fa4)?, p5(0x5fb0)?])),
        engine(0x80072270, json!([p5(0x5fcc)?, p5(0x5fd8)?])),
        engine(0x80072218, json!([p5(0x5fcc)?, p5(0x5fd8)?])),
        engine(0x80065048, json!([0, 0x800])),
        engine(0x8006638c, json!([p5(0x600c)?, i.handle])),
        engine(0x8001a8a8, json!([])),
        engine(0x80033710, json!([])),
        engine(0x800c3cd8, json!([f(0x6024)?, f(0x6030)?, f(0x6024)?])),
        engine(0x8011e154, json!([f(0x6030)?, f(0x6048)?])),
        engine(0x8011e150, json!([f(0x6030)?, f(0x6030)?, f(0x6030)?])),
    ];
    // One light block for lights 0, 2 and 3; light 1 takes it plus 0x20
    // (addi r4,r31,32 at +0x607C).
    let lights = wb.at(0x606c, 5)?;
    for k in 0..4u32 {
        let off = if k == 1 { lights + 0x20 } else { lights };
        c.push(engine(0x8002506c, json!([k, s(5, off)])));
    }
    c.extend([
        engine(0x80025028, json!([0, 0])),
        engine(0x80025028, json!([0, 1])),
        engine(0x80025048, json!([0])),
        engine(0x8002503c, json!([2])),
        wb.local_call(0x60cc, json!([p5(0x60c8)?]))?,
        engine(0x80011a04, json!([0])),
        engine(0x80011050, json!([p5(0x60dc)?])),
        engine(0x8005d0ec, json!([p5(0x60e8)?])),
        engine(0x80071e0c, json!([2, 1, wb.ptr(0x60fc, 1)?, p5(0x6104)?])),
    ]);
    let task = if i.task {
        Some([wb.ptr(0x6118, 1)?, wb.ptr(0x6124, 1)?, wb.ptr(0x6130, 1)?])
    } else {
        None
    };
    c.push(engine(0x8003bcc4, json!([p5(0x613c)?])));
    c.push(wb.local_call(0x6144, json!([]))?);
    let mut slots: Vec<Word> = i.slots.iter().map(|v| json!(v)).collect();
    // +0, +8 and +0x24 are filled, then +0xB854 runs, then +0x28.
    for (k, pc) in [(0, 0x6160), (1, 0x6184), (2, 0x61a8)] {
        if i.slots[k] == 0 {
            slots[k] = wb.ptr(pc, 1)?;
        }
    }
    c.push(wb.local_call(0x61b0, json!([]))?);
    if i.slots[3] == 0 {
        slots[3] = wb.ptr(0x61d0, 1)?;
    }
    c.extend([
        wb.local_call(0x61e8, json!([p5(0x61dc)?, wb.ptr(0x61e4, 6)?]))?,
        engine(0x8011c2bc, json!([])),
        engine(0x8011c310, json!([])),
        wb.local_call(0x61fc, json!([p5(0x61f8)?]))?,
        wb.local_call(0x6208, json!([p5(0x6204)?]))?,
    ]);
    // Trail plugin table: GSNE8P fn_800E014C, three module routines, zeros.
    let mut trail = vec![json!(0x800e014c_u32)];
    for pc in [0x6224, 0x6230, 0x623c] {
        trail.push(wb.ptr(pc, 1)?);
    }
    trail.resize(14, json!(0));
    c.extend([
        wb.local_call(0x6278, json!([0, 0]))?,
        engine(0x8006d500, json!([p5(0x6280)?, wb.ptr(0x6288, 4)?])),
    ]);
    Ok(json!({ "calls": c, "state": {
        "handle": i.handle, "slots": slots, "trail": trail, "task": task } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeardownInput {
    /// 0x801A2A18 +0, +8, +0x24, +0x28: `"ours"` (what setup installed) or a word.
    pub slots: [Word; 4],
    /// .bss+0x84.
    pub handle: u32,
}

/// +0x62A8: undo the setup's hooks and release the stage's resources.
pub fn teardown(wb: &WeaponsBed<'_>, i: &TeardownInput) -> Result<Value> {
    let p5 = |pc| wb.ptr(pc, 5);
    let mut c = vec![
        wb.local_call(0x62b8, json!([]))?,
        wb.local_call(0x62bc, json!([]))?,
        wb.local_call(0x62c0, json!([]))?,
        engine(0x8011c2bc, json!([])),
        engine(0x8011c310, json!([])),
    ];
    // Each slot is cleared only while it still holds the setup's routine
    // (compared at +0x62E0, +0x6304, +0x632C, +0x6354).
    for pc in [0x62e0, 0x6304, 0x632c, 0x6354] {
        wb.at(pc, 1)?;
    }
    let mut slots = Vec::new();
    for v in &i.slots {
        slots.push(if v == "ours" {
            json!(0)
        } else if v.is_u64() {
            v.clone()
        } else {
            return Err(Error::invalid("teardown slot must be \"ours\" or a word"));
        });
    }
    c.extend([
        wb.local_call(0x6368, json!([]))?,
        engine(0x800336dc, json!([])),
        engine(0x80072118, json!([p5(0x6374)?, p5(0x637c)?])),
        engine(0x80072118, json!([p5(0x6388)?, p5(0x6390)?])),
        engine(0x80066364, json!([])),
    ]);
    wb.at(0x63a0, 6)?;
    if i.handle != 0 {
        c.push(engine(0x8001a2b0, json!([i.handle])));
    }
    c.push(engine(0x800245b8, json!([])));
    c.push(wb.local_call(0x63c0, json!([]))?);
    Ok(json!({ "calls": c, "state": { "handle": 0, "slots": slots } }))
}

/// +0x63DC, the background task exec: the drift list routine +0x0 with
/// both offsets .rodata+0x2B0 (0.0).
pub fn task_exec(wb: &WeaponsBed<'_>) -> Result<Value> {
    let z = wb.f(0x63ec)?;
    Ok(json!({ "calls": [wb.local_call(0x63f8, json!([z, z]))?], "state": {} }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayInput {
    /// The camera position 0x80175378[0x803ADAD0] points at (bit patterns;
    /// only x and z are read).
    pub cam: [u32; 3],
    /// .bss+0x80: draw the clipped model.
    pub clip: bool,
    /// The word at .data+0x515B0 (.data+0x515A4 +0xC; filled at run time).
    pub model: u32,
}

fn at_camera(wb: &WeaponsBed<'_>, cam: [u32; 3], pc: u32) -> Result<[u32; 3]> {
    Ok([cam[0], wb.f(pc)?, cam[2]])
}

/// +0x64B8, the background task display: the sky model at the camera's
/// x/z, clipped (ds_DrawModelClip) while .bss+0x80 is set.
pub fn display(wb: &WeaponsBed<'_>, i: &DisplayInput) -> Result<Value> {
    let v = at_camera(wb, i.cam, 0x64f0)?;
    let mut c = vec![
        engine(0x8011c2bc, json!([])),
        engine(0x8011c310, json!([])),
        engine(0x80116098, json!([])),
        engine(0x8011538c, json!([0, v])),
        engine(0x80116e18, json!([wb.ptr(0x6520, 5)?])),
    ];
    wb.at(0x652c, 6)?;
    let sky = wb.at(0x6544, 5)?;
    if i.clip {
        wb.at(0x6554, 5)?; // .data+0x515A4, read at +0xC
        c.extend([
            engine(0x80116098, json!([])),
            engine(0x8011616c, json!([s(5, sky + 8)])),
            engine(0x8011f514, json!([i.model])),
            engine(0x801160e4, json!([])),
        ]);
    } else {
        c.push(engine(0x8011df44, json!([wb.ptr(0x656c, 5)?])));
    }
    c.extend([
        engine(0x80115dc8, json!([1])),
        engine(0x8011c2b0, json!([])),
        engine(0x8011c310, json!([])),
    ]);
    Ok(json!({ "calls": c, "state": {} }))
}

/// +0x6410, the background task's second display: the far model (+0xF4)
/// at the camera's x/z plus the offset at .data+0x5143C +8.
pub fn display2(wb: &WeaponsBed<'_>, cam: [u32; 3]) -> Result<Value> {
    let v = at_camera(wb, cam, 0x6448)?;
    let off = wb.at(0x6488, 5)? + 8; // addi r4,r4,8 at +0x648C
    let offset = [wb.word(5, off)?, wb.word(5, off + 4)?, wb.word(5, off + 8)?];
    let c = vec![
        engine(0x8011c2bc, json!([])),
        engine(0x8011c310, json!([])),
        engine(0x80116098, json!([])),
        engine(0x8011538c, json!([0, v])),
        engine(0x80116e18, json!([wb.ptr(0x6478, 5)?])),
        engine(0x8011538c, json!([0, offset])),
        wb.local_call(0x6494, json!([]))?,
        engine(0x80115dc8, json!([1])),
        engine(0x8011c2b0, json!([])),
        engine(0x8011c310, json!([])),
    ];
    Ok(json!({ "calls": c, "state": {} }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CamInput {
    cam: [u32; 3],
}

/// Run one routine by name on a JSON input.
pub fn replay(rel_bytes: &[u8], routine: &str, input: &Value) -> Result<Value> {
    let wb = WeaponsBed::read(rel_bytes)?;
    fn parse<T: serde::de::DeserializeOwned>(v: &Value) -> Result<T> {
        T::deserialize(v).map_err(|e| Error::invalid(format!("stage 12 request: {e}")))
    }
    match routine {
        "setup" => setup(&wb, &parse(input)?),
        "teardown" => teardown(&wb, &parse(input)?),
        "exec" | "task_dest" => Ok(json!({ "calls": [], "state": {} })),
        "task_exec" => task_exec(&wb),
        "display" => display(&wb, &parse(input)?),
        "display2" => display2(&wb, parse::<CamInput>(input)?.cam),
        _ => Err(Error::invalid(format!(
            "unknown stage 12 routine {routine}"
        ))),
    }
}
