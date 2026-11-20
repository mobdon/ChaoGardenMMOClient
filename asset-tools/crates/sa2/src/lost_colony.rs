//! Lost Colony (story level 27, stg27D.rel): the stage program "STG27" that
//! `_prolog` (+0x632C) hands to LoadSubprogram, the stage task it creates,
//! the spot-light hook it installs at 0x803ADC30 and the light tasks that
//! hook spawns. All offsets are section-1 (code) offsets of stg27D.rel;
//! engine calls are reported at their GSNE8P main.dol addresses.
//!
//! | Offset | Size | Routine |
//! | --- | --- | --- |
//! | +0x6358 | 0x368 | stage setup (the program's first entry) |
//! | +0x66C0 | 0x114 | stage teardown (second entry) |
//! | +0x6DC0 | 0x234 | stage program exec (third entry): light level, fog blend |
//! | +0x12E18 | 0x38 | fog blend request (four .bss words) |
//! | +0x7184 | 0x20 | stage task first exec: installs +0x6FF4 |
//! | +0x6FF4 | 0x190 | stage task exec: per-player mode latch, glow pulse |
//! | +0x71A4 | 0x84 | stage task destructor: lights off |
//! | +0x7228 | 0x3F4 | stage task display: player spot light, light land table |
//! | +0x761C | 0x318 | stage task second display: glowing and animated land tables |
//! | +0x6AE0 | 0x2E0 | spot-light hook (0x803ADC30): bias, light task spawn |
//! | +0x67D4 | 0xD4 | light task exec |
//! | +0x68A8 | 0x7C | light task destructor |
//! | +0x6924 | 0x1BC | light task display |
//!
//! Constants and data pointers are read from the module through the
//! relocation of the instruction that loads them (the offsets are recorded
//! at each use). The Steam `resource/gd_PC/stg27D.rel` has the same section
//! bytes as the GameCube disc copy, so either can be passed; engine
//! addresses are always the GSNE8P ones (the Steam module links another
//! main program). REL-local helpers that are not part of this port are
//! reported as `module` calls. Values the engine computes (njCalcPoint,
//! njCalcVector, njDistanceP2P) are inputs where this code branches or does
//! arithmetic on them, and names (`"v"`, `"p"`) where it only passes them on.
// `!(a < b)` mirrors `fcmpo` + `bcf`: an unordered (NaN) compare takes the
// same branch as on the GameCube.
#![allow(clippy::neg_cmp_op_on_partial_ord)]
use crate::{
    city_rel::{CityRel, Reference},
    manifest,
    ninja_trig::{fctiwz, nj_sin},
    Error, Result,
};
use serde::Deserialize;
use serde_json::{json, Value};

/// GameCube disc `stg27D.rel` (module 27).
pub const GC_REL_SHA256: &str = "a41ae2d3b90a71547f263b5f20748c7c45ee9b2cf356912be40d09ec3031144f";
/// Installed Steam `resource/gd_PC/stg27D.rel` (module 26, same section bytes).
pub const STEAM_REL_SHA256: &str =
    "ec0ab22aa45f0deca139ba500fea2e74b7e26beeb9bd38a67dc8231f401ab344";
pub const REL_LEN: usize = 1_434_536;

fn invalid() -> Error {
    Error::invalid("unexpected stage 27 code")
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
fn bits(v: [f32; 3]) -> [u32; 3] {
    v.map(f32::to_bits)
}
fn fl(v: [u32; 3]) -> [f32; 3] {
    v.map(f32::from_bits)
}

pub struct LostColony<'a> {
    rel: CityRel<'a>,
}

impl<'a> LostColony<'a> {
    pub fn read(bytes: &'a [u8]) -> Result<Self> {
        if bytes.len() != REL_LEN {
            return Err(Error::invalid("unsupported stage 27 REL revision"));
        }
        let module = match manifest::sha256(bytes).as_str() {
            GC_REL_SHA256 => 27,
            STEAM_REL_SHA256 => 26,
            _ => return Err(Error::invalid("unsupported stage 27 REL revision")),
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
                "unexpected stage 27 code at +0x{pc:x}"
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
    /// The single-precision word of .data the `addi` at `pc` addresses.
    fn fd(&self, pc: u32) -> Result<f32> {
        Ok(f32::from_bits(self.word(5, self.at(pc, 5)?)?))
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
    /// A data word: a module address when it carries an ADDR32 relocation to
    /// this module, else its value.
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
    /// A .data word that must be a pointer into this module's .data.
    fn data_ptr(&self, at: u32) -> Result<u32> {
        match self.rel.local_pointer(5, at)? {
            Some((5, off)) => Ok(off),
            _ => Err(invalid()),
        }
    }
}

/// A word that is either a plain value or one of the module's own
/// addresses (`"s1+0x6ae0"`).
pub type Word = Value;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupInput {
    /// fn_80065048's return, kept at .bss+0x4.
    pub handle: u32,
    /// CreateElementalTask returned a task.
    pub task: bool,
    /// The words at 0x801A2A18 +0, +8 and +0x28 before the setup.
    pub slots: [u32; 3],
}

/// +0x6358. Returns the ordered calls and the final value of every location
/// it writes: .bss+0x8C8 (object fog off), the 0x801A2A18 hooks, the trail
/// plugin table 0x801E77D8, the spot-light hook 0x803ADC30 and the created
/// stage task's +0x14/+0x20/+0x18 callbacks (also kept at .bss+0x8).
pub fn setup(lc: &LostColony<'_>, i: &SetupInput) -> Result<Value> {
    let f = |pc| lc.f(pc).map(f32::to_bits);
    let p5 = |pc| lc.ptr(pc, 5);
    lc.at(0x6374, 6)?; // .bss+0x8C8 = 1
    lc.at(0x6410, 6)?; // .bss+0x4 = handle
    let mut c = vec![
        engine(0x800324e4, json!([128, p5(0x6384)?])),
        engine(
            0x800245ec,
            json!([p5(0x6390)?, p5(0x6398)?, p5(0x63a0)?, p5(0x63a8)?]),
        ),
        engine(0x80072270, json!([p5(0x63b4)?, p5(0x63c0)?])),
        engine(0x80072218, json!([p5(0x63b4)?, p5(0x63c0)?])),
        engine(0x80072270, json!([p5(0x63dc)?, p5(0x63e8)?])),
        engine(0x80072218, json!([p5(0x63dc)?, p5(0x63e8)?])),
        engine(0x80065048, json!([0, 0x800])),
        engine(0x8006638c, json!([p5(0x641c)?, i.handle])),
        engine(0x8001a8a8, json!([])),
        engine(0x80033710, json!([])),
        engine(0x800c3cd8, json!([f(0x6434)?, f(0x6440)?, f(0x6434)?])),
        engine(0x8011e154, json!([f(0x6440)?, f(0x6458)?])),
        engine(0x8011e150, json!([f(0x6440)?, f(0x6440)?, f(0x6440)?])),
    ];
    // One light block for lights 0, 2 and 3; light 1 takes it plus 0x20
    // (addi r4,r31,32 at +0x648C).
    let lights = lc.at(0x647c, 5)?;
    for k in 0..4u32 {
        let off = if k == 1 { lights + 0x20 } else { lights };
        c.push(engine(0x8002506c, json!([k, s(5, off)])));
    }
    c.extend([
        engine(0x80025028, json!([0, 0])),
        engine(0x80025028, json!([0, 1])),
        engine(0x80025048, json!([0])),
        engine(0x8002503c, json!([2])),
        lc.local_call(0x64dc, json!([p5(0x64d8)?]))?,
        engine(0x80011a04, json!([0])),
        engine(0x80011050, json!([p5(0x64ec)?])),
        engine(0x8005d0ec, json!([p5(0x64f8)?])),
        engine(0x80071e0c, json!([2, 1, lc.ptr(0x650c, 1)?, p5(0x6514)?])),
    ]);
    let task = if i.task {
        lc.at(0x6528, 6)?; // .bss+0x8 = task
        Some([lc.ptr(0x6534, 1)?, lc.ptr(0x6540, 1)?, lc.ptr(0x654c, 1)?])
    } else {
        None
    };
    c.push(engine(0x8003bcc4, json!([p5(0x6558)?])));
    c.push(lc.local_call(0x6560, json!([]))?);
    let mut slots: Vec<Word> = i.slots.iter().map(|v| json!(v)).collect();
    for (k, pc) in [(0, 0x657c), (1, 0x65a0), (2, 0x65c4)] {
        if i.slots[k] == 0 {
            slots[k] = lc.ptr(pc, 1)?;
        }
    }
    c.extend([
        engine(0x8011c2b0, json!([])),
        engine(0x8011c310, json!([])),
        lc.local_call(0x65dc, json!([p5(0x65d8)?]))?,
        lc.local_call(0x65e8, json!([p5(0x65e4)?]))?,
    ]);
    // Trail plugin table: GSNE8P fn_800E014C, three module routines, zeros.
    let mut trail = vec![json!(0x800e014c_u32)];
    for pc in [0x6604, 0x6610, 0x661c] {
        trail.push(lc.ptr(pc, 1)?);
    }
    trail.resize(14, json!(0));
    c.extend([
        lc.local_call(0x6660, json!([p5(0x6654)?, lc.ptr(0x665c, 6)?]))?,
        lc.local_call(0x6674, json!([p5(0x6668)?, lc.ptr(0x6670, 6)?]))?,
        lc.local_call(0x6678, json!([]))?,
        lc.local_call(0x6690, json!([]))?,
        engine(0x8006d500, json!([p5(0x6698)?, lc.ptr(0x66a0, 4)?])),
    ]);
    Ok(json!({ "calls": c, "state": {
        "objectFogOff": 1, "handle": i.handle, "slots": slots, "trail": trail,
        "hook": lc.ptr(0x6680, 1)?, "task": task } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeardownInput {
    /// 0x801A2A18 +0, +8, +0x28: `"ours"` (what setup installed) or a word.
    pub slots: [Word; 3],
    /// .bss+0x4.
    pub handle: u32,
}

/// +0x66C0: clear the spot-light hook, undo the setup's hooks and release
/// the stage's resources.
pub fn teardown(lc: &LostColony<'_>, i: &TeardownInput) -> Result<Value> {
    let p5 = |pc| lc.ptr(pc, 5);
    let mut c = vec![
        lc.local_call(0x66e0, json!([]))?,
        lc.local_call(0x66e4, json!([]))?,
        engine(0x8011c2bc, json!([])),
        engine(0x8011c310, json!([])),
        lc.local_call(0x66f0, json!([]))?,
        lc.local_call(0x66f4, json!([]))?,
    ];
    // Each slot is cleared only while it still holds the routine setup put
    // there (+0x670C, +0x672C, +0x6754).
    lc.at(0x670c, 1)?;
    lc.at(0x672c, 1)?;
    lc.at(0x6754, 1)?;
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
        engine(0x800336dc, json!([])),
        engine(0x80072118, json!([p5(0x6770)?, p5(0x6778)?])),
        engine(0x80072118, json!([p5(0x6784)?, p5(0x678c)?])),
        engine(0x80066364, json!([])),
    ]);
    if i.handle != 0 {
        c.push(engine(0x8001a2b0, json!([i.handle])));
    }
    c.push(engine(0x800245b8, json!([])));
    c.push(lc.local_call(0x67bc, json!([]))?);
    Ok(json!({ "calls": c, "state": { "hook": 0, "handle": 0, "slots": slots } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecInput {
    /// The program work's +0x20, the displayed light level (bit pattern).
    pub level: u32,
    /// The program task's +0x4C, the light target (bit pattern).
    pub target: u32,
    /// Game state 0x801CC168 bytes +0x26 and +0x9.
    pub pause: [i8; 2],
    /// .bss+0xC, the light-task bits.
    pub lights: u32,
    /// .bss+0x90, the bias the spot-light hook adds (bit pattern).
    pub bias: u32,
    /// .bss+0x94C, a count another module routine (+0x1F8B4) raises.
    pub count: i32,
}

/// +0x6DC0, the stage program's exec: ease the light level toward the
/// target, request the fog blend (+0x12E18) from it, then decay the target
/// and add the bias and count collected this frame.
pub fn exec(lc: &LostColony<'_>, i: &ExecInput) -> Result<Value> {
    let [l0, t0] = [i.level, i.target].map(f32::from_bits);
    let level = l0 + lc.fd(0x6de0)? * (t0 - l0);
    let (r2b0, r2b4) = (lc.f(0x6e68)?, lc.f(0x6e30)?);
    let r2e0 = lc.f(0x6e04)?;
    let mut x = level;
    if x > r2e0 {
        x -= r2e0;
        if x > lc.f(0x6e1c)? {
            x = r2b4;
        } else {
            x = r2e0 + lc.f(0x6e40)? * ((x - r2e0) / lc.f(0x6e50)?);
        }
    }
    if x < r2b0 {
        x = r2b0;
    }
    let mut c = Vec::new();
    let a = lc.ptr(0x6ea8, 6)?;
    let (b, w) = if i.pause[0] != 0 || i.pause[1] != 0 {
        (a.clone(), lc.f(0x6eb4)?)
    } else {
        let angle = fctiwz(lc.f(0x6ec8)? * (lc.f(0x6ed4)? * x));
        c.push(engine(0x80112398, json!([angle])));
        lc.at(0x6ef8, 6)?;
        (lc.ptr(0x6f00, 6)?, nj_sin(angle))
    };
    c.push(lc.local_call(0x6ebc, json!([a, b, w.to_bits()]))?);
    let blend = blend_state(lc, a.clone(), b, w)?;
    let mut t = t0
        * if i.lights != 0 {
            lc.fd(0x6f24)?
        } else {
            lc.fd(0x6f40)?
        };
    t += f32::from_bits(i.bias) + lc.fd(0x6f64)? * i.count as f32;
    let two = lc.f(0x6fcc)?;
    if t > two {
        t = two;
    }
    lc.at(0x6f58, 6)?;
    lc.at(0x6f70, 6)?;
    Ok(json!({ "calls": c, "state": {
        "level": level.to_bits(), "target": t.to_bits(),
        "bias": lc.f(0x6fb8)?.to_bits(), "count": 0, "blend": blend } }))
}

fn blend_state(lc: &LostColony<'_>, a: Value, b: Value, w: f32) -> Result<Value> {
    for pc in [0x12e1c, 0x12e28, 0x12e34, 0x12e44] {
        lc.at(pc, 6)?;
    }
    Ok(json!({ "from": a, "to": b, "weight": w.to_bits(), "clock": 0 }))
}

/// +0x12E18: store the two blend sources (.bss+0x8E4/+0x8E8), the weight
/// (.bss+0x8EC) and restart the clock (.bss+0x8F0).
pub fn blend(lc: &LostColony<'_>, a: Word, b: Word, weight: u32) -> Result<Value> {
    Ok(json!({ "calls": [],
        "state": blend_state(lc, a, b, f32::from_bits(weight))? }))
}

/// +0x7184, the stage task's first exec: make +0x6FF4 the exec and set
/// 0x803AD718 to 1.
pub fn task_start(lc: &LostColony<'_>) -> Result<Value> {
    Ok(json!({ "calls": [], "state": {
        "exec": lc.ptr(0x7188, 1)?, "lightMode": 1 } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskExecInput {
    /// playertwp[0] and [1] are set.
    pub players: [bool; 2],
    /// .data+0x673F4: a mode switch was requested for the player.
    pub pending: [i32; 2],
    /// The player mode bytes at 0x803AD924.
    pub mode: [i8; 2],
    /// .data+0x673FC: the mode to restore.
    pub saved: [i32; 2],
    /// .data+0x673EC: the switch is in effect.
    pub latched: [i32; 2],
    /// Stage task work +6.
    pub timer: u16,
}

/// +0x6FF4, the stage task exec. For each present player, a requested
/// switch saves the mode and selects mode 3 (fn_80025028), and the next
/// frame without one restores it; then the three glowing models of
/// .data+0x673CC pulse with `0.5 + 0.5 sin(timer * period + phase << 14)`.
pub fn task_exec(lc: &LostColony<'_>, i: &TaskExecInput) -> Result<Value> {
    for pc in [0x701c, 0x702c, 0x7034] {
        lc.at(pc, 5)?;
    }
    let (mut pending, mut saved, mut latched) = (i.pending, i.saved, i.latched);
    let mut c = Vec::new();
    for k in 0..2 {
        if !i.players[k] {
            continue;
        }
        if pending[k] != 0 {
            if i.mode[k] != 3 {
                saved[k] = i.mode[k].into();
                c.push(engine(0x80025028, json!([3, k])));
            }
            pending[k] = 0;
            latched[k] = 1;
        } else {
            if latched[k] != 0 && i.mode[k] == 3 {
                c.push(engine(0x80025028, json!([saved[k], k])));
            }
            latched[k] = 0;
        }
    }
    let rows = lc.at(0x70e8, 5)?;
    let period = lc.word(5, lc.at(0x70f4, 5)?)?;
    let half = lc.f(0x70fc)?;
    for j in 0..3 {
        let row = rows + 8 * j;
        let obj = lc.data_ptr(row)?;
        let phase = lc.word(5, row + 4)?;
        let angle = (u32::from(i.timer).wrapping_mul(period)).wrapping_add(phase << 14) as i32;
        c.push(engine(0x80112398, json!([angle])));
        let glow = half + half * nj_sin(angle);
        c.push(engine(
            0x800334b0,
            json!([glow.to_bits(), 0xff00_0000u32, -1]),
        ));
        let model = lc.data_ptr(obj + 4)?;
        c.push(lc.local_call(0x714c, json!([lc.data_value(5, model)?, "ret 0x800334b0"]))?);
    }
    Ok(json!({ "calls": c, "state": {
        "pending": pending, "saved": saved, "latched": latched,
        "timer": i.timer.wrapping_add(1) } }))
}

/// +0x71A4, the stage task destructor: 0x803AD718 = 2, lights 1–4 off,
/// and .bss+0x8 cleared if it still names this task (`own`).
pub fn task_dest(lc: &LostColony<'_>, own: bool) -> Result<Value> {
    lc.at(0x71fc, 6)?;
    let c: Vec<Value> = (1..=4).map(|k| engine(0x8011fc34, json!([k, 0]))).collect();
    Ok(json!({ "calls": c, "state": { "lightMode": 2, "cleared": own } }))
}

/// A land table the displays walk: its 32-byte COL records (centre,
/// radius, object at +0x10, mask at +0x18).
struct LandList {
    base: u32,
    count: usize,
}
impl LostColony<'_> {
    fn land(&self, pc: u32) -> Result<LandList> {
        let t = self.at(pc, 5)?;
        let count = self.half(5, t)?;
        if count < 0 {
            return Err(invalid());
        }
        Ok(LandList {
            base: self.data_ptr(t + 0x10)?,
            count: count as usize,
        })
    }
}

/// Walk one land table: draw each record whose mask meets `mask` and whose
/// view depth (njCalcPoint z, `depth[i]`) lies between -radius and `far`.
/// `glow` adds the pulse colour (+0x761C's first list) before each draw.
fn walk(
    lc: &LostColony<'_>,
    list: &LandList,
    mask: u32,
    depth: &[u32],
    far: f32,
    glow: Option<(u32, f32, u32, f32, u32)>,
    c: &mut Vec<Value>,
) -> Result<()> {
    if depth.len() != list.count {
        return Err(Error::invalid(format!(
            "stage 27 land table needs {} depths",
            list.count
        )));
    }
    for (k, z) in depth.iter().enumerate() {
        let rec = list.base + 32 * k as u32;
        let m = lc.word(5, rec + 0x18)?;
        if m != 0 && m & mask == 0 {
            continue;
        }
        c.push(engine(0x80114f0c, json!([0, s(5, rec), "p"])));
        let d = -f32::from_bits(*z);
        let radius = f32::from_bits(lc.word(5, rec + 0xc)?);
        if !(d < far) || !(d > -radius) {
            continue;
        }
        let obj = lc.data_ptr(rec + 0x10)?;
        c.push(engine(0x8011616c, json!([s(5, obj + 8)])));
        c.push(engine(0x80115f68, json!([s(5, obj + 0x14), 0])));
        let model = lc.data_value(5, obj + 4)?;
        if let Some((timer, scale, period, half, colour)) = glow {
            // Three calls of the module's fabsf (+0x398C): |y|, |x| + it, |z| + that.
            let r = |o| lc.word(5, rec + o).map(|w| f32::from_bits(w).abs());
            let sum = r(8)? + (r(0)? + r(4)?);
            let angle = fctiwz(scale * sum).wrapping_add(timer.wrapping_mul(period) as i32);
            c.push(engine(0x80112398, json!([angle])));
            let v = half + half * nj_sin(angle);
            c.push(engine(0x800334b0, json!([v.to_bits(), colour, -1])));
            let m = lc.data_ptr(obj + 4)?;
            c.push(module_call(
                0xc44c,
                json!([lc.data_value(5, m)?, "ret 0x800334b0"]),
            ));
        }
        c.extend([
            engine(0x8011e158, json!([model])),
            engine(0x801160e4, json!([])),
            engine(0x80116098, json!([])),
        ]);
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayInput {
    /// 0x803ADAD0, the screen being drawn.
    pub screen: u32,
    /// playertwp[screen] is set; its rotation Y (work +0xC).
    pub player: Option<i32>,
    /// playertwp[0]'s position (work +0x14, bit patterns).
    pub pos: [u32; 3],
    /// The light direction after njCalcVector, and playertwp[0]'s raised
    /// position after njCalcPoint (bit patterns).
    pub dir: [u32; 3],
    pub point: [u32; 3],
    /// The camera position 0x80175378[screen] points at (bit patterns).
    pub cam: [u32; 3],
    /// .bss+0x0: draw the clipped model instead of the land list.
    pub clip: bool,
    /// The word at .data+0x66E38 (the clipped model; filled at run time).
    pub model: u32,
    /// Game state 0x801CC168 +0xA8 and +0xAC (the land masks per screen).
    pub masks: [u32; 2],
    /// njCalcPoint z of each record of .data+0x522F0's land table.
    pub depth: Vec<u32>,
}

/// +0x7228, the stage task display: the player's spot light (GX light 2)
/// and, away from the colony centre, the light land table.
pub fn display(lc: &LostColony<'_>, i: &DisplayInput) -> Result<Value> {
    let mut c = Vec::new();
    if let Some(rot) = i.player {
        let v = [lc.f(0x7268)?, lc.f(0x7278)?, lc.f(0x7288)?];
        c.extend([
            engine(0x80115d24, json!([bits(v)])),
            engine(0x80116098, json!([])),
            engine(0x80115828, json!([0, 0x8000i32.wrapping_sub(rot)])),
            engine(0x801154ec, json!([0, "v", "v"])),
            engine(0x801160e4, json!([])),
        ]);
        let mut p = fl(i.pos);
        p[1] += lc.f(0x7300)?;
        c.push(engine(0x80114f0c, json!([0, bits(p), "p"])));
        let k = lc.f(0x7328)?;
        let (d, q) = (fl(i.dir), fl(i.point));
        let q = [0, 1, 2].map(|j| q[j] + k * -d[j]);
        let light = 0x8035_7918u32 + 0x80;
        c.extend([
            engine(0x8010283c, json!([light, lc.f(0x7390)?.to_bits(), 2])),
            engine(
                0x801029bc,
                json!([light, lc.f(0x73a8)?.to_bits(), lc.f(0x73b4)?.to_bits(), 3]),
            ),
            engine(0x8011fc34, json!([2, 1])),
            engine(0x8011fdec, json!([2, i.dir])),
            engine(
                0x8011fd30,
                json!([
                    2,
                    lc.f(0x73ec)?.to_bits(),
                    lc.f(0x73ec)?.to_bits(),
                    lc.f(0x73fc)?.to_bits()
                ]),
            ),
            engine(0x8011fcbc, json!([2, bits(q)])),
            engine(0x8011fe9c, json!([2])),
        ]);
    }
    let cam = fl(i.cam);
    let dx = lc.f(0x7428)? - cam[0];
    let dz = lc.f(0x745c)? - cam[2];
    if dx * dx + dz * dz < lc.f(0x747c)? {
        return Ok(json!({ "calls": c, "state": {} }));
    }
    c.extend([
        engine(0x8011c2bc, json!([])),
        engine(0x8011c310, json!([])),
        engine(0x80116e18, json!([lc.ptr(0x7498, 5)?])),
        engine(0x80116098, json!([])),
        engine(0x8011616c, json!(["cam"])),
        engine(0x80116098, json!([])),
    ]);
    lc.at(0x74c0, 6)?;
    if i.clip {
        lc.at(0x74d4, 5)?;
        c.push(engine(0x8011f514, json!([i.model])));
    } else {
        c.push(engine(0x8011e17c, json!([lc.ptr(0x74e8, 5)?])));
    }
    c.extend([
        engine(0x801160e4, json!([])),
        engine(0x80116098, json!([])),
        engine(0x8011c2bc, json!([])),
        engine(0x8011c310, json!([])),
        engine(0x80116098, json!([])),
        engine(0x80116e18, json!([lc.ptr(0x7548, 5)?])),
    ]);
    let mask = if i.screen == 0 {
        i.masks[0]
    } else {
        i.masks[1]
    };
    let far = lc.f(0x7558)?;
    walk(lc, &lc.land(0x7534)?, mask, &i.depth, far, None, &mut c)?;
    c.extend([
        engine(0x801160e4, json!([])),
        engine(0x80115dc8, json!([2])),
        engine(0x8011c2b0, json!([])),
        engine(0x8011c310, json!([])),
    ]);
    Ok(json!({ "calls": c, "state": {} }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Display2Input {
    pub screen: u32,
    pub masks: [u32; 2],
    /// Stage task work +6.
    pub timer: u16,
    /// njCalcPoint z of each record of .data+0x5316C's and .data+0x52380's
    /// land tables.
    pub glow_depth: Vec<u32>,
    pub anim_depth: Vec<u32>,
}

/// +0x761C, the stage task's second display: the glowing land table, each
/// model's colour pulsing with a phase from its centre, then the land
/// table whose texture steps through 13 frames every two frames.
pub fn display2(lc: &LostColony<'_>, i: &Display2Input) -> Result<Value> {
    let mut c = vec![
        engine(0x8011c2bc, json!([])),
        engine(0x8011c310, json!([])),
        engine(0x80116098, json!([])),
        engine(0x80116e18, json!([lc.ptr(0x7688, 5)?])),
    ];
    let mask = if i.screen == 0 {
        i.masks[0]
    } else {
        i.masks[1]
    };
    let glow = (
        u32::from(i.timer),
        lc.fd(0x7698)?,
        lc.word(5, lc.at(0x76a0, 5)?)?,
        lc.f(0x76a8)?,
        0xff20_2020u32, // lis r27,0xff20; addi r3,r27,0x2020
    );
    let far = lc.f(0x76b4)?;
    walk(
        lc,
        &lc.land(0x7674)?,
        mask,
        &i.glow_depth,
        far,
        Some(glow),
        &mut c,
    )?;
    c.extend([
        engine(0x801160e4, json!([])),
        engine(0x8011c2b0, json!([])),
        engine(0x8011c310, json!([])),
        engine(0x80116098, json!([])),
    ]);
    // The texture list at .data+0x4B46C, copied with its list advanced by
    // 12 * ((timer / 2) % 13) bytes and its count set to 1.
    let tex = lc.data_ptr(lc.at(0x7814, 5)?)?;
    let frame = (u32::from(i.timer) / 2) % 13;
    c.push(engine(0x80116e18, json!([[s(5, tex + 12 * frame), 1]])));
    walk(
        lc,
        &lc.land(0x7800)?,
        mask,
        &i.anim_depth,
        lc.f(0x787c)?,
        None,
        &mut c,
    )?;
    c.push(engine(0x801160e4, json!([])));
    Ok(json!({ "calls": c, "state": {} }))
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Light {
    /// njDistanceP2P from the light task's work +0x14 to the hook's point.
    pub dist: u32,
    /// The light task's +0x4C (bit pattern).
    pub level: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HookInput {
    /// r3: the point (bit patterns) and f1, the strength.
    pub pos: [u32; 3],
    pub strength: u32,
    /// njDistanceP2P from the camera (0x80175378[0]) to the point.
    pub cam_dist: u32,
    /// .bss+0x90 and .bss+0xC.
    pub bias: u32,
    pub lights: u32,
    /// .bss+0x10 [3] and [4]: the light tasks (None: null).
    pub slots: [Option<Light>; 2],
    /// CreateElementalTask returned a task.
    pub created: bool,
}

/// +0x6AE0, installed at 0x803ADC30: something lit at `pos`. Near the
/// camera it raises the bias (.bss+0x90); it then brightens the nearest
/// light task within 150 units, or spawns a light task (+0x67D4) in the
/// first free slot 3 or 4.
pub fn hook(lc: &LostColony<'_>, i: &HookInput) -> Result<Value> {
    let strength = f32::from_bits(i.strength);
    let mut c = vec![engine(0x8011b65c, json!(["cam", i.pos]))];
    let r2e4 = lc.f(0x6b1c)?;
    let near = (r2e4 - f32::from_bits(i.cam_dist)) / r2e4;
    let k = lc.fd(0x6b50)?;
    let mut bias = f32::from_bits(i.bias);
    lc.at(0x6b44, 6)?;
    if near > lc.f(0x6b30)? {
        bias += k * (near * strength);
    }
    let gain = strength * lc.fd(0x6b6c)?;
    let mut best: Option<(usize, f32)> = None;
    let mut best_d = lc.f(0x6b7c)?;
    let mut free = 0;
    lc.at(0x6b94, 6)?;
    lc.at(0x6ba4, 6)?;
    for (n, slot) in i.slots.iter().enumerate() {
        if i.lights & (1 << (n + 3)) == 0 {
            continue;
        }
        match slot {
            Some(l) => {
                c.push(engine(
                    0x8011b65c,
                    json!([format!("light{}+0x14", n + 3), i.pos]),
                ));
                let d = f32::from_bits(l.dist);
                if d < best_d {
                    best = Some((n, d));
                    best_d = d;
                }
            }
            None => free += 1,
        }
    }
    let mut state = json!({ "bias": bias.to_bits(), "lights": i.lights });
    if !(gain > lc.f(0x6c10)?) {
        return Ok(json!({ "calls": c, "state": state }));
    }
    let two = lc.f(0x6c4c)?;
    let close = best_d < lc.f(0x6c2c)?;
    if let Some((n, _)) = best.filter(|_| close || free == 0) {
        let l = i.slots[n].ok_or_else(invalid)?;
        let lvl = f32::from_bits(l.level);
        if lvl < two {
            let up = lvl + gain;
            let up = if up > two { two } else { up };
            state["raise"] = json!([n + 3, up.to_bits()]);
        }
        return Ok(json!({ "calls": c, "state": state }));
    }
    for k in 3..=4u32 {
        if i.lights & (1 << k) != 0 {
            continue;
        }
        c.push(engine(
            0x80071e0c,
            json!([2, 3, lc.ptr(0x6cc0, 1)?, lc.ptr(0x6cc8, 5)?]),
        ));
        if !i.created {
            break;
        }
        let level = if free == 2 {
            let three = lc.f(0x6d1c)?;
            lc.f(0x6d34)? * if gain > three { three } else { gain }
        } else if gain > two {
            two
        } else {
            gain
        };
        lc.at(0x6cdc, 6)?;
        lc.at(0x6d88, 6)?;
        state["lights"] = json!(i.lights | 1 << k);
        state["spawn"] = json!({
            "slot": k, "display": lc.ptr(0x6cf0, 1)?, "dest": lc.ptr(0x6cfc, 1)?,
            "level": level.to_bits(), "pos": i.pos });
        break;
    }
    Ok(json!({ "calls": c, "state": state }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LightInput {
    /// Light task work +1 (its slot), +0x20 (level shown) and task +0x4C.
    pub slot: i8,
    pub shown: u32,
    pub level: u32,
    /// .bss+0xC.
    pub lights: u32,
}

fn slot(k: i8) -> Result<u32> {
    // .bss+0x10 holds 32 slots (.bss+0x90 follows).
    u32::try_from(k)
        .ok()
        .filter(|k| *k < 32)
        .ok_or_else(|| Error::invalid("stage 27 light slot out of range"))
}

/// +0x67D4, the light task exec: ease the shown level, decay the level
/// (cap 5), keep its bit and slot, and destroy the task below 0.1.
pub fn light_exec(lc: &LostColony<'_>, i: &LightInput) -> Result<Value> {
    let k = slot(i.slot)?;
    let (s0, l0) = (f32::from_bits(i.shown), f32::from_bits(i.level));
    let shown = s0 + lc.f(0x67ec)? * (l0 - s0);
    let mut level = l0 * lc.f(0x6810)?;
    lc.at(0x6824, 6)?;
    lc.at(0x6854, 6)?;
    let five = lc.f(0x6868)?;
    if level > five {
        level = five;
    }
    let c = if level < lc.f(0x6884)? {
        vec![engine(0x80071af0, json!(["task"]))]
    } else {
        vec![]
    };
    Ok(json!({ "calls": c, "state": {
        "shown": shown.to_bits(), "level": level.to_bits(),
        "lights": i.lights | 1 << k, "slot": [k, "task"] } }))
}

/// +0x68A8, the light task destructor: light off, clear bits 0..=slot of
/// .bss+0xC (`li r3,-2; slw`) and the slot.
pub fn light_dest(lc: &LostColony<'_>, slot_byte: i8, lights: u32) -> Result<Value> {
    let k = slot(slot_byte)?;
    lc.at(0x68d0, 6)?;
    lc.at(0x6904, 6)?;
    Ok(json!({ "calls": [engine(0x8011fc34, json!([k, 0]))],
        "state": { "lights": lights & (0xffff_fffeu32 << k), "slot": [k, 0] } }))
}

/// +0x6924, the light task display: GX light `slot` pointing down from
/// 180 units above the task, its cone and brightness from the shown level.
pub fn light_display(
    lc: &LostColony<'_>,
    slot_byte: i8,
    shown: u32,
    pos: [u32; 3],
) -> Result<Value> {
    let k = slot(slot_byte)?;
    let s = f32::from_bits(shown);
    let one = lc.f(0x697c)?;
    let b = if s > one { one } else { s };
    let mut p = fl(pos);
    p[1] += lc.f(0x69b0)?;
    let v = [lc.f(0x6944)?, lc.f(0x6954)?, lc.f(0x6944)?];
    let light = 0x8035_7918u32 + 64 * k;
    let c = vec![
        engine(0x80115d24, json!([bits(v)])),
        engine(0x80114f0c, json!([0, bits(p), "p"])),
        engine(0x801154ec, json!([0, "v", "v"])),
        engine(
            0x8010283c,
            json!([light, (lc.f(0x69fc)? + lc.f(0x6a08)? * s).to_bits(), 3]),
        ),
        engine(
            0x801029bc,
            json!([light, lc.f(0x6a38)?.to_bits(), lc.f(0x6a44)?.to_bits(), 2]),
        ),
        engine(0x8011fc34, json!([k, 1])),
        engine(0x8011fdec, json!([k, "v"])),
        engine(
            0x8011fd30,
            json!([k, b.to_bits(), b.to_bits(), (lc.f(0x6a90)? * b).to_bits()]),
        ),
        engine(0x8011fcbc, json!([k, "p"])),
        engine(0x8011fe9c, json!([k])),
    ];
    Ok(json!({ "calls": c, "state": {} }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BlendInput {
    a: Word,
    b: Word,
    weight: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnInput {
    own: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LightDestInput {
    slot: i8,
    lights: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LightDisplayInput {
    slot: i8,
    shown: u32,
    pos: [u32; 3],
}

/// Run one routine by name on a JSON input.
pub fn replay(rel_bytes: &[u8], routine: &str, input: &Value) -> Result<Value> {
    let lc = LostColony::read(rel_bytes)?;
    fn parse<T: serde::de::DeserializeOwned>(v: &Value) -> Result<T> {
        T::deserialize(v).map_err(|e| Error::invalid(format!("stage 27 request: {e}")))
    }
    match routine {
        "setup" => setup(&lc, &parse(input)?),
        "teardown" => teardown(&lc, &parse(input)?),
        "exec" => exec(&lc, &parse(input)?),
        "blend" => {
            let b: BlendInput = parse(input)?;
            blend(&lc, b.a, b.b, b.weight)
        }
        "task_start" => task_start(&lc),
        "task_exec" => task_exec(&lc, &parse(input)?),
        "task_dest" => task_dest(&lc, parse::<OwnInput>(input)?.own),
        "display" => display(&lc, &parse(input)?),
        "display2" => display2(&lc, &parse(input)?),
        "hook" => hook(&lc, &parse(input)?),
        "light_exec" => light_exec(&lc, &parse(input)?),
        "light_dest" => {
            let d: LightDestInput = parse(input)?;
            light_dest(&lc, d.slot, d.lights)
        }
        "light_display" => {
            let d: LightDisplayInput = parse(input)?;
            light_display(&lc, d.slot, d.shown, d.pos)
        }
        _ => Err(Error::invalid(format!(
            "unknown stage 27 routine {routine}"
        ))),
    }
}
