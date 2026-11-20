//! Dry Lagoon (story level 18, stg18D.rel): the stage program "STG18" that
//! `_prolog` (+0xD748) hands to LoadSubprogram, the stage tasks it creates
//! (BgClip, BgExec, Bg2Exec) and the camera-region helpers they share. All
//! offsets are section-1 (code) offsets of the GameCube disc stg18D.rel;
//! engine calls are reported at their GSNE8P main.dol addresses.
//!
//! | Offset | Size | Routine |
//! | --- | --- | --- |
//! | +0xD774 | 0x46C | stage setup (the program's first entry) |
//! | +0xDBE0 | 0x208 | stage teardown (second entry) |
//! | +0xDDE8 | 0xC4 | stage program exec (third entry; creates Bg2Exec, then hands over to +0xDEAC) |
//! | +0xDEAC | 0x20 | stage program exec after the first frame |
//! | +0xE704 | 0x24 | BgClip task exec: stores camera 0's region |
//! | +0xDECC | 0x1BC | BgExec task exec: the lagoon's ambient sound |
//! | +0xE088 | 0x5E8 | BgExec task display: water switch and the animated water layers |
//! | +0xE670 | 0x20 | BgExec destructor |
//! | +0xE690 | 0x4 | Bg2Exec task exec (returns at once) |
//! | +0xE694 | 0x50 | Bg2Exec display |
//! | +0xE6E4 | 0x20 | Bg2Exec destructor |
//! | +0x98C0 | 0x17C | camera region of a camera |
//! | +0x9A3C | 0x110 | store a camera's region in the game state |
//! | +0x9B4C | 0xAC | water height of camera 0's region |
//! | +0x9D48 | 0x170 | is the water surface in view from above it |
//! | +0xA80 | 0x190 | water task spawn (exec +0xC10) |
//! | +0x17128 | 0x2C | set the watched object (shared with stg07/08/10/24/38/57) |
//!
//! Constants and data pointers are read from the module through the
//! relocation of the instruction that loads them (offsets recorded at each
//! use). The Steam `resource/gd_PC/stg18D.rel` (module 17) has the same data
//! sections but its code section is 0x4C bytes shorter: the setup's install
//! of hook slot 0x801A2A18+0x14 (GC +0xDA88..+0xDAAC) and the teardown's
//! matching restore (GC +0xDC3C..+0xDC64) are absent. The Steam module is
//! followed: neither copy installs that slot here. Offsets are still named in
//! GC coordinates; reads of the Steam module are mapped across the two cuts.
use crate::{
    city_rel::{CityRel, Reference},
    manifest, Error, Result,
};
use serde::Deserialize;
use serde_json::{json, Value};

/// GameCube disc `stg18D.rel` (module 18).
pub const GC_REL_SHA256: &str = "6dc9427c3b8554a9540e5b5d72cde29cc23ff443030339e8bcc041d5d66f04fc";
/// Installed Steam `resource/gd_PC/stg18D.rel` (module 17).
pub const STEAM_REL_SHA256: &str =
    "edca61dc5bf1d6f7b376ef6569ee1c0bb6f44fd7d34b1224254b2b427bbefc00";

/// Code the Steam module leaves out, as (GC offset, length).
const STEAM_CUTS: [(u32, u32); 2] = [(0xda88, 0x24), (0xdc3c, 0x28)];

fn invalid() -> Error {
    Error::invalid("unexpected stage 18 code")
}
fn s(section: u8, offset: u32) -> Value {
    json!(format!("s{section}+0x{offset:x}"))
}
fn module_call(offset: u32, args: Value) -> Value {
    json!({ "module": format!("s1+0x{offset:x}"), "args": args })
}

pub struct DryLagoon<'a> {
    rel: CityRel<'a>,
    steam: bool,
}

impl<'a> DryLagoon<'a> {
    pub fn read(bytes: &'a [u8]) -> Result<Self> {
        let (module, steam) = match manifest::sha256(bytes).as_str() {
            GC_REL_SHA256 => (18, false),
            STEAM_REL_SHA256 => (17, true),
            _ => return Err(Error::invalid("unsupported stage 18 REL revision")),
        };
        Ok(Self {
            rel: CityRel::read_identified(bytes, module)?,
            steam,
        })
    }
    pub fn is_steam(&self) -> bool {
        self.steam
    }
    /// The file's code offset of GC offset `pc`.
    fn rpc(&self, pc: u32) -> Result<u32> {
        if !self.steam {
            return Ok(pc);
        }
        let mut shift = 0;
        for (start, len) in STEAM_CUTS {
            if pc >= start + len {
                shift += len;
            } else if pc >= start {
                return Err(Error::invalid(format!(
                    "+0x{pc:x} is not in the Steam stage 18 module"
                )));
            }
        }
        Ok(pc - shift)
    }
    /// The GC offset of the file's code offset `off`.
    fn gc_off(&self, mut off: u32) -> u32 {
        if self.steam {
            for (start, len) in STEAM_CUTS {
                if off >= start {
                    off += len;
                }
            }
        }
        off
    }
    fn word(&self, section: u8, at: u32) -> Result<u32> {
        let d = self.rel.data(section)?;
        d.get(at as usize..at as usize + 4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
            .ok_or_else(invalid)
    }
    /// The instruction at GC offset `pc`.
    fn ins(&self, pc: u32) -> Result<u32> {
        self.word(1, self.rpc(pc)?)
    }
    /// The signed 16-bit immediate of the instruction at `pc` (`li`, `mulli`).
    fn imm(&self, pc: u32) -> Result<i32> {
        Ok(i32::from(self.ins(pc)? as u16 as i16))
    }
    /// The section offset the `addi` at `pc` (LO half, relocated at `pc + 2`)
    /// points at; code offsets are given in GC coordinates.
    fn at(&self, pc: u32, section: u8) -> Result<u32> {
        match self.rel.reference(1, self.rpc(pc)? + 2) {
            Some(Reference {
                module,
                kind: 4,
                section: sec,
                addend,
            }) if sec == section && module == self.rel.module_id() => Ok(if section == 1 {
                self.gc_off(addend)
            } else {
                addend
            }),
            _ => Err(Error::invalid(format!(
                "unexpected stage 18 code at +0x{pc:x}"
            ))),
        }
    }
    fn ptr(&self, pc: u32, section: u8) -> Result<Value> {
        Ok(s(section, self.at(pc, section)?))
    }
    /// The single-precision .rodata constant the `addi` at `pc` addresses.
    fn f(&self, pc: u32) -> Result<f32> {
        Ok(f32::from_bits(self.word(4, self.at(pc, 4)?)?))
    }
    /// The double-precision .rodata constant the `addi` at `pc` addresses.
    fn d(&self, pc: u32) -> Result<f64> {
        let a = self.at(pc, 4)?;
        Ok(f64::from_bits(
            u64::from(self.word(4, a)?) << 32 | u64::from(self.word(4, a + 4)?),
        ))
    }
    /// Check that the `addi` at `pc` addresses main-program data `address`
    /// (GC module: its relocation names it; Steam: some main-program symbol).
    fn dol(&self, pc: u32, address: u32) -> Result<()> {
        match self.rel.reference(1, self.rpc(pc)? + 2) {
            Some(Reference {
                module: 0,
                kind: 4,
                addend,
                ..
            }) if self.steam || addend == address => Ok(()),
            _ => Err(Error::invalid(format!(
                "+0x{pc:x} does not load 0x{address:08x}"
            ))),
        }
    }
    /// An engine call: the `bl` at `pc` must be relocated to main-program
    /// routine `address` (checked on the GC module, whose imports are GSNE8P).
    fn e(&self, pc: u32, address: u32, args: Value) -> Result<Value> {
        match self.rel.reference(1, self.rpc(pc)?) {
            Some(Reference {
                module: 0,
                kind: 10,
                addend,
                ..
            }) if self.steam || addend == address => {
                Ok(json!({ "engine": format!("0x{address:08x}"), "args": args }))
            }
            _ => Err(Error::invalid(format!(
                "+0x{pc:x} does not call 0x{address:08x}"
            ))),
        }
    }
    /// The GC offset the module-local `bl` at `pc` calls.
    fn local(&self, pc: u32) -> Result<u32> {
        let ins = self.ins(pc)?;
        if ins & 0xfc00_0003 != 0x4800_0001 || self.rel.reference(1, self.rpc(pc)?).is_some() {
            return Err(Error::invalid(format!("no local call at +0x{pc:x}")));
        }
        let disp = ((ins & 0x03ff_fffc) as i32) << 6 >> 6;
        let target = self.rpc(pc)?.checked_add_signed(disp).ok_or_else(invalid)?;
        Ok(self.gc_off(target))
    }
    fn m(&self, pc: u32, args: Value) -> Result<Value> {
        Ok(module_call(self.local(pc)?, args))
    }
    /// A data word as a value: a module address when it carries a relocation
    /// to this module, else its value.
    fn data_value(&self, section: u8, at: u32) -> Result<Value> {
        Ok(match self.rel.local_pointer(section, at) {
            Ok(Some((sec, off))) => s(sec, if sec == 1 { self.gc_off(off) } else { off }),
            Ok(None) => json!(0),
            Err(_) if self.rel.reference(section, at).is_none() => json!(self.word(section, at)?),
            Err(e) => return Err(e),
        })
    }
}

/// A camera: position and the X angle at +0xC (as read through
/// lbl_80175378[lbl_803ADAD0]).
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Camera {
    pub pos: [f32; 3],
    #[serde(default)]
    pub rot_x: i32,
}

/// Region codes +0x98C0 returns (and +0x9A3C stores in the game state).
pub mod region {
    pub const LAGOON: u32 = 2;
    pub const INNER: u32 = 0x400;
    pub const WEST: u32 = 0x800;
    pub const EAST: u32 = 0x1000;
    pub const NORTH_EAST: u32 = 0x10_0000;
    pub const LOW: u32 = 0x20_0000;
    pub const CAVE: u32 = 0x40_0000;
}

/// +0x98C0: the region of a camera position, from the rodata bounds at
/// 0x27C..0x2A4. Region names are descriptive only; the codes are native.
pub fn camera_region(dl: &DryLagoon<'_>, cam: [f32; 3]) -> Result<u32> {
    let [x, y, z] = cam;
    dl.dol(0x98cc, 0x8017_5378)?;
    if z < dl.f(0x98f8)? {
        return Ok(if x > dl.f(0x9910)? {
            0x10_0000
        } else {
            region::LAGOON
        });
    }
    if x < dl.f(0x9938)? {
        return Ok(region::WEST);
    }
    if x > dl.f(0x9954)? {
        return Ok(region::EAST);
    }
    if (x < dl.f(0x9970)? && z < dl.f(0x9984)?) || y < dl.f(0x999c)? {
        return Ok(region::LOW);
    }
    if dl.f(0x99b8)? < x && x < dl.f(0x99d0)? && z < dl.f(0x99e4)? {
        let c = dl.f(0x99f8)?;
        if z < c || (c <= z && y < dl.f(0x9a18)?) {
            return Ok(region::CAVE);
        }
    }
    Ok(region::INNER)
}

/// +0x9A3C(n): store camera n's region at game state 0x801CC168 +0xA0 (n 0)
/// or +0xA4 (n 1). Any other n writes through an unset register in the
/// original, so it is refused. Returns the (offset, value) written.
pub fn store_region(dl: &DryLagoon<'_>, n: i32, cam: [f32; 3]) -> Result<(u32, u32)> {
    let offset = match n {
        0 => {
            dl.dol(0x9a58, 0x801c_c168)?;
            0xa0
        }
        1 => {
            dl.dol(0x9a70, 0x801c_c168)?;
            0xa4
        }
        _ => return Err(Error::invalid("stage 18 region slot must be 0 or 1")),
    };
    if dl.local(0x9a78)? != 0x98c0 {
        return Err(invalid());
    }
    Ok((offset, camera_region(dl, cam)?))
}

/// +0x9B4C: the water height for camera 0's region.
pub fn water_level(dl: &DryLagoon<'_>, cam: [f32; 3]) -> Result<f32> {
    if dl.local(0x9b58)? != 0x98c0 {
        return Err(invalid());
    }
    match camera_region(dl, cam)? {
        region::LAGOON | region::NORTH_EAST => dl.f(0x9bc0),
        0x2000 | region::INNER | region::LOW | region::CAVE => dl.f(0x9bd0),
        _ => dl.f(0x9be0),
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceInput {
    pub cam: Camera,
    /// The water height (f1).
    pub level: f32,
    /// tanf's result.
    #[serde(default)]
    pub tan: f32,
    /// The Y of the point njCalcPoint returns.
    #[serde(default)]
    pub point_y: f32,
}

/// +0x9D48: whether the water surface at `level` is seen from a camera above
/// it. Returns the calls and `visible` (0 or 1).
pub fn surface_visible(dl: &DryLagoon<'_>, i: &SurfaceInput) -> Result<Value> {
    dl.dol(0x9d74, 0x803a_dad0)?;
    dl.dol(0x9d84, 0x8017_5378)?;
    let cam_y = i.cam.pos[1];
    let done = |c: Vec<Value>, v: u32| json!({ "calls": c, "state": { "visible": v } });
    if cam_y < i.level {
        return Ok(done(vec![], 0));
    }
    let depth = dl.f(0x9dac)?;
    if i.level + dl.f(0x9dbc)? * depth.abs() < cam_y {
        return Ok(done(vec![], 0));
    }
    let mut c = vec![dl.e(0x9dec, 0x8011_1390, json!([dl.f(0x9de4)?.to_bits()]))?];
    let f31 = dl.f(0x9df4)? / i.tan;
    let drop = dl.f(0x9e0c)? * depth.abs() / f31;
    let point = [dl.f(0x9e20)?.to_bits(), (-drop).to_bits(), depth.to_bits()];
    c.extend([
        dl.e(0x9e3c, 0x8011_6098, json!([]))?,
        dl.e(0x9e44, 0x8011_53e4, json!([0]))?,
        dl.e(0x9e4c, 0x8011_616c, json!([i.cam.pos.map(f32::to_bits)]))?,
        dl.e(0x9e68, 0x8011_5c60, json!([0, i.cam.rot_x]))?,
        dl.e(0x9e78, 0x8011_4f0c, json!([0, point, "out"]))?,
        dl.e(0x9e7c, 0x8011_60e4, json!([]))?,
    ]);
    Ok(done(c, u32::from(i.point_y < i.level)))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupInput {
    /// fn_80065048's return, kept at .bss+0x5C.
    pub handle: u32,
    /// Game state 0x801CC168 +0 (2-player flag byte).
    pub players: i8,
    /// CreateElementalTask returned the BgExec task.
    pub task: bool,
    /// .bss+0x54 before the setup.
    pub bg: u32,
    /// The ten words at 0x801A2A18 +0..+0x24 before the setup.
    pub slots: [u32; 10],
    /// .bss+0x60 (the water task) before the setup.
    pub water: u32,
    /// fn_8008C868's return, kept at .bss+0x50.
    pub sound: u32,
}

/// The hook slots the setup fills when empty: (slot index, pc of the addi
/// that loads our routine). Slot 5 (+0x14) is filled only by the GC module
/// and is left out (Steam is followed).
const SLOTS: [(usize, u32); 7] = [
    (0, 0xd9a4),
    (1, 0xd9c8),
    (2, 0xd9ec),
    (3, 0xda10),
    (4, 0xda34),
    (7, 0xda58),
    (9, 0xda7c),
];

/// +0xD774. Returns the ordered calls and every location written.
pub fn setup(dl: &DryLagoon<'_>, i: &SetupInput) -> Result<Value> {
    let p5 = |pc| dl.ptr(pc, 5);
    let f = |pc| dl.f(pc).map(f32::to_bits);
    let mut c = vec![
        dl.e(
            0xd7a0,
            0x8007_1e0c,
            json!([2, 0, dl.ptr(0xd794, 1)?, p5(0xd79c)?]),
        )?,
        dl.e(0xd7ac, 0x8002_493c, json!([p5(0xd7a8)?]))?,
        dl.e(0xd7c8, 0x8007_2270, json!([p5(0xd7b4)?, p5(0xd7c0)?]))?,
        dl.e(0xd7d4, 0x8007_2218, json!([p5(0xd7b4)?, p5(0xd7c0)?]))?,
        dl.e(0xd7f0, 0x8007_2270, json!([p5(0xd7dc)?, p5(0xd7e8)?]))?,
        dl.e(0xd7fc, 0x8007_2218, json!([p5(0xd7dc)?, p5(0xd7e8)?]))?,
        dl.e(0xd808, 0x8006_5048, json!([0, dl.imm(0xd804)?]))?,
        dl.e(0xd824, 0x8006_638c, json!([p5(0xd81c)?, i.handle]))?,
        dl.e(0xd828, 0x8001_a8a8, json!([]))?,
        dl.e(0xd82c, 0x8003_3710, json!([]))?,
        dl.e(
            0xd84c,
            0x800c_3cd8,
            json!([f(0xd834)?, f(0xd840)?, f(0xd834)?]),
        )?,
        dl.e(0xd860, 0x8011_e154, json!([f(0xd840)?, f(0xd858)?]))?,
        dl.e(
            0xd870,
            0x8011_e150,
            json!([f(0xd840)?, f(0xd840)?, f(0xd840)?]),
        )?,
    ];
    dl.at(0xd810, 6)?;
    let light = dl.at(0xd87c, 5)?;
    // Light 1 is the block plus 0x20 (addi at +0xD88C); 0, 2 and 3 share it.
    for (k, pc) in [(0, 0xd884), (1, 0xd890), (2, 0xd89c), (3, 0xd8a8)] {
        let off = if k == 1 { light + 0x20 } else { light };
        c.push(dl.e(pc, 0x8002_506c, json!([k, s(5, off)]))?);
    }
    c.extend([
        dl.e(0xd8b4, 0x8002_5028, json!([0, 0]))?,
        dl.e(0xd8c0, 0x8002_5028, json!([0, 1]))?,
        dl.e(0xd8c8, 0x8002_5048, json!([0]))?,
        dl.e(0xd8d0, 0x8002_503c, json!([2]))?,
        dl.m(0xd8dc, json!([p5(0xd8d8)?]))?,
        dl.e(0xd8e4, 0x8001_1a04, json!([0]))?,
    ]);
    dl.dol(0xd8ec, 0x801c_c168)?;
    c.push(if i.players != 0 {
        dl.e(0xd908, 0x8001_1050, json!([p5(0xd904)?]))?
    } else {
        dl.e(0xd918, 0x8001_1050, json!([p5(0xd914)?]))?
    });
    c.push(dl.e(0xd924, 0x8005_d0ec, json!([p5(0xd920)?]))?);
    c.push(dl.e(
        0xd940,
        0x8007_1e0c,
        json!([2, 1, dl.ptr(0xd934, 1)?, p5(0xd93c)?]),
    )?);
    let mut bg = json!(i.bg);
    let mut task = Value::Null;
    if i.task {
        dl.at(0xd950, 6)?;
        if i.bg == 0 {
            bg = json!("task");
        }
        task = json!({ "0x20": dl.ptr(0xd968, 1)?, "0x18": dl.ptr(0xd974, 1)? });
    }
    c.push(dl.e(0xd984, 0x8003_bcc4, json!([p5(0xd980)?]))?);
    c.push(dl.m(0xd988, json!([]))?);
    let mut slots: Vec<Value> = i.slots.iter().map(|v| json!(v)).collect();
    dl.dol(0xd990, 0x801a_2a18)?;
    for (k, pc) in SLOTS {
        if i.slots[k] == 0 {
            slots[k] = dl.ptr(pc, 1)?;
        }
    }
    c.push(dl.m(0xda84, json!([]))?);
    c.push(dl.m(0xdabc, json!([p5(0xdab0)?, dl.ptr(0xdab8, 6)?]))?);
    c.extend([
        dl.e(0xdac0, 0x8011_c2bc, json!([]))?,
        dl.e(0xdac4, 0x8011_c310, json!([]))?,
        dl.m(0xdad0, json!([0, 0]))?,
    ]);
    dl.at(0xdad8, 6)?;
    if i.water == 0 {
        c.push(dl.m(0xdaf0, json!([0, dl.imm(0xdaec)?]))?);
    }
    c.extend([
        dl.e(0xdafc, 0x8008_c8f0, json!([p5(0xdaf8)?]))?,
        dl.e(0xdb08, 0x8008_c868, json!([p5(0xdb04)?]))?,
    ]);
    dl.at(0xdb10, 6)?;
    c.extend([
        dl.m(0xdb24, json!([p5(0xdb1c)?, dl.imm(0xdb20)?]))?,
        dl.m(0xdb30, json!([p5(0xdb2c)?]))?,
        dl.m(0xdb3c, json!([p5(0xdb38)?]))?,
    ]);
    // Trail plugin table 0x801E77D8: fn_800E014C, three module routines, zeros.
    dl.dol(0xdb44, 0x800e_014c)?;
    dl.dol(0xdb4c, 0x801e_77d8)?;
    let mut trail = vec![json!(0x800e_014c_u32)];
    for pc in [0xdb58, 0xdb64, 0xdb70] {
        trail.push(dl.ptr(pc, 1)?);
    }
    trail.resize(14, json!(0));
    c.push(dl.e(
        0xdbb4,
        0x8006_d500,
        json!([p5(0xdba8)?, dl.ptr(0xdbb0, 4)?]),
    )?);
    dl.dol(0xdbc0, 0x801c_c168)?;
    Ok(json!({ "calls": c, "state": {
        "handle": i.handle, "bg": bg, "task": task, "slots": slots,
        "sound": i.sound, "trail": trail, "game+0x3b": dl.imm(0xdbb8)? } }))
}

/// A word that is either a value or `"ours"` (the routine the setup put there).
pub type Word = Value;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeardownInput {
    pub slots: [Word; 10],
    /// .bss+0x50, +0x5C and +0x60.
    pub sound: u32,
    pub handle: u32,
    pub water: u32,
}

/// +0xDBE0: release the stage's resources and clear every hook slot that
/// still holds the setup's routine.
pub fn teardown(dl: &DryLagoon<'_>, i: &TeardownInput) -> Result<Value> {
    let p5 = |pc| dl.ptr(pc, 5);
    let mut slots = Vec::new();
    for v in &i.slots {
        if v != "ours" && !v.is_u64() {
            return Err(Error::invalid("teardown slot must be \"ours\" or a word"));
        }
        slots.push(v.clone());
    }
    let mut c = vec![
        dl.m(0xdbf0, json!([]))?,
        dl.m(0xdbf4, json!([]))?,
        dl.m(0xdbf8, json!([]))?,
        dl.e(0xdc08, 0x8011_9828, json!([i.sound]))?,
        dl.e(0xdc14, 0x8008_c8e4, json!([]))?,
    ];
    dl.at(0xdc00, 6)?;
    dl.at(0xdc1c, 6)?;
    if i.water != 0 {
        c.push(dl.e(0xdc2c, 0x8007_1af0, json!([i.water]))?);
    }
    c.extend([
        dl.m(0xdc30, json!([]))?,
        dl.e(0xdc34, 0x8011_c2bc, json!([]))?,
        dl.e(0xdc38, 0x8011_c310, json!([]))?,
    ]);
    let mut clear = |k: usize, pc: u32| -> Result<()> {
        dl.at(pc, 1)?;
        if slots[k] == "ours" {
            slots[k] = json!(0);
        }
        Ok(())
    };
    clear(9, 0xdc78)?;
    c.push(dl.m(0xdc8c, json!([]))?);
    for (k, pc) in [
        (7, 0xdca4),
        (0, 0xdcc8),
        (1, 0xdcf0),
        (2, 0xdd18),
        (3, 0xdd40),
        (4, 0xdd68),
    ] {
        clear(k, pc)?;
    }
    c.extend([
        dl.e(0xdd7c, 0x8003_36dc, json!([]))?,
        dl.e(0xdd90, 0x8007_2118, json!([p5(0xdd84)?, p5(0xdd8c)?]))?,
        dl.e(0xdda4, 0x8007_2118, json!([p5(0xdd98)?, p5(0xdda0)?]))?,
        dl.e(0xdda8, 0x8006_6364, json!([]))?,
    ]);
    dl.at(0xddb0, 6)?;
    if i.handle != 0 {
        c.push(dl.e(0xddc0, 0x8001_a2b0, json!([i.handle]))?);
    }
    c.extend([
        dl.e(0xddcc, 0x8002_45b8, json!([]))?,
        dl.m(0xddd0, json!([]))?,
    ]);
    Ok(json!({ "calls": c, "state": {
        "slots": slots, "sound": 0, "handle": 0, "water": i.water } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecInput {
    /// CreateElementalTask returned the Bg2Exec task.
    pub task: bool,
    /// .bss+0x58 before.
    pub bg2: u32,
}

/// +0xDDE8: create Bg2Exec (placed at the rodata 0x464..0x46C position), then
/// make +0xDEAC the program's exec.
pub fn exec(dl: &DryLagoon<'_>, i: &ExecInput) -> Result<Value> {
    let c = vec![dl.e(
        0xde14,
        0x8007_1e0c,
        json!([2, 2, dl.ptr(0xde08, 1)?, dl.ptr(0xde10, 5)?]),
    )?];
    let mut bg2 = json!(i.bg2);
    let mut task = Value::Null;
    if i.task {
        dl.at(0xde24, 6)?;
        if i.bg2 == 0 {
            bg2 = json!("task");
        }
        task = json!({
            "0x20": dl.ptr(0xde3c, 1)?, "0x18": dl.ptr(0xde48, 1)?,
            "work+0x14": [dl.f(0xde54)?.to_bits(), dl.f(0xde68)?.to_bits(), dl.f(0xde7c)?.to_bits()],
        });
    }
    Ok(json!({ "calls": c, "state": {
        "bg2": bg2, "task": task, "exec": dl.ptr(0xde90, 1)? } }))
}

/// +0xDEAC: the program exec from the second frame on (one module call).
pub fn exec_after(dl: &DryLagoon<'_>) -> Result<Value> {
    Ok(json!({ "calls": [dl.m(0xdeb8, json!([]))?], "state": {} }))
}

/// +0xE704, BgClip's exec: store camera 0's region at game state +0xA0.
pub fn bg_clip(dl: &DryLagoon<'_>, cam: [f32; 3]) -> Result<Value> {
    if dl.local(0xe714)? != 0x9a3c || dl.imm(0xe710)? != 0 {
        return Err(invalid());
    }
    let (off, v) = store_region(dl, 0, cam)?;
    Ok(json!({ "calls": [], "state": { format!("game+0x{off:x}"): v } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AmbientInput {
    pub cam: [f32; 3],
    /// Player 0's position (playertwp[0] +0x14).
    pub player: [f32; 3],
    /// Game state +0x7C, the frame clock.
    pub clock: u32,
    /// rand()'s result, when it is called.
    #[serde(default)]
    pub rand: i32,
}

/// +0xDECC, BgExec's exec: above the water, a player inside the lagoon box
/// hears the lagoon loop, with a one-in-four chance every 90 frames of an
/// extra sound; elsewhere the loop plays with the other volume.
pub fn ambient(dl: &DryLagoon<'_>, i: &AmbientInput) -> Result<Value> {
    if dl.local(0xdee4)? != 0x9b4c {
        return Err(invalid());
    }
    dl.dol(0xdeec, 0x803a_dad0)?;
    dl.dol(0xdefc, 0x8017_5378)?;
    let level = water_level(dl, i.cam)?;
    let mut c = Vec::new();
    // bcf gt: an unordered compare also returns.
    if i.cam[1].partial_cmp(&level) != Some(std::cmp::Ordering::Greater) {
        return Ok(json!({ "calls": c, "state": {} }));
    }
    dl.dol(0xdf24, 0x801e_7768)?;
    let [x, _, z] = i.player;
    let byte = |pc| -> Result<i32> { Ok(i32::from(dl.word(5, dl.at(pc, 5)?)? as u8 as i8)) };
    if dl.f(0xdf18)? <= x && x <= dl.f(0xdf40)? && dl.f(0xdf58)? <= z && z <= dl.f(0xdf74)? {
        dl.dol(0xdf8c, 0x801c_c168)?;
        let period = dl.imm(0xdfb0)? as u32;
        if i.clock.is_multiple_of(period) {
            c.push(dl.e(0xdfc0, 0x8010_dc88, json!([]))?);
            let magic = dl.d(0xdfc8)?;
            let r = f64::from_bits(0x4330_0000_0000_0000 | u64::from(i.rand as u32 ^ 0x8000_0000));
            let v = (r - magic) as f32 * dl.f(0xdfec)?;
            if f64::from(v) < dl.d(0xdffc)? {
                c.push(dl.e(0xe01c, 0x8006_b978, json!([dl.imm(0xe00c)?, "task", 0, 0]))?);
                c.push(dl.e(0xe038, 0x8006_9a5c, json!(["task", -1, byte(0xe02c)?]))?);
            }
        }
    } else {
        c.push(dl.e(
            0xe054,
            0x8006_b1fc,
            json!([dl.imm(0xe040)?, "task", 1, 0, dl.imm(0xe050)?]),
        )?);
        c.push(dl.e(0xe070, 0x8006_9a5c, json!(["task", -1, byte(0xe064)?]))?);
    }
    Ok(json!({ "calls": c, "state": {} }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayInput {
    pub cam: Camera,
    /// Player 0's position.
    pub player: [f32; 3],
    /// The water task at .bss+0x60 is set.
    pub water: bool,
    /// Game state +0xA0 (the stored region) and +0x7C (the frame clock).
    pub flags: u32,
    pub clock: u32,
    /// +0x9D48's inputs when it is reached (tanf, njCalcPoint's Y).
    #[serde(default)]
    pub tan: f32,
    #[serde(default)]
    pub point_y: f32,
}

/// One animated water layer: frame `(clock >> 1) & 15` of the texture-
/// animation table pair, its speed from .data+0xD22E0, then the draw +0x11358
/// with the word at .data+0xD22DC.
fn layer(dl: &DryLagoon<'_>, c: &mut Vec<Value>, clock: u32, pcs: [u32; 6]) -> Result<()> {
    let [magic_pc, speed_pc, t1_pc, t2_pc, cvt, call] = pcs;
    let frame = (clock >> 1 & 15) * 4;
    let magic = dl.d(magic_pc)?;
    let speed = f32::from_bits(dl.word(5, dl.at(speed_pc, 5)?)?);
    let v = (f64::from_bits(0x4330_0000_0000_0000 | u64::from(clock)) - magic) as f32 * speed;
    c.push(dl.e(cvt, 0x8010_af3c, json!([v.to_bits()]))?);
    let a = dl.data_value(5, dl.at(t1_pc, 5)? + frame)?;
    let b = dl.data_value(5, dl.at(t2_pc, 5)? + frame)?;
    c.push(dl.e(cvt + 0x28, 0x8007_3028, json!([a, b, "cvt"]))?);
    let word = dl.word(5, dl.at(0xe300, 5)?)?;
    c.push(dl.m(call, json!([a, word]))?);
    Ok(())
}
fn layer_open(dl: &DryLagoon<'_>, c: &mut Vec<Value>, pc: u32) -> Result<()> {
    c.extend([
        dl.e(pc, 0x8011_c2bc, json!([]))?,
        dl.e(pc + 4, 0x8011_c310, json!([]))?,
        dl.e(pc + 8, 0x8002_b304, json!([]))?,
        dl.e(pc + 0xc, 0x8002_b35c, json!([]))?,
        dl.e(pc + 0x14, 0x8002_b2d8, json!([dl.imm(pc + 0x10)?]))?,
        dl.e(pc + 0x1c, 0x8002_b2e8, json!([dl.imm(pc + 0x18)?]))?,
        dl.e(pc + 0x2c, 0x8012_18c8, json!([255, 0x8300]))?,
    ]);
    Ok(())
}
fn layer_close(dl: &DryLagoon<'_>, c: &mut Vec<Value>, pc: u32) -> Result<()> {
    c.extend([
        dl.e(pc, 0x8011_60e4, json!([]))?,
        dl.e(pc + 8, 0x8011_c754, json!([dl.imm(pc + 4)?]))?,
        dl.e(pc + 0x14, 0x8011_79f8, json!([0, 8]))?,
        dl.e(pc + 0x20, 0x8011_79f8, json!([1, 6]))?,
        dl.e(pc + 0x24, 0x8002_b348, json!([]))?,
        dl.e(pc + 0x28, 0x8002_b2f8, json!([]))?,
        dl.e(pc + 0x2c, 0x8011_c2b0, json!([]))?,
        dl.e(pc + 0x30, 0x8011_c310, json!([]))?,
    ]);
    Ok(())
}
fn translate(dl: &DryLagoon<'_>, pc: u32, a: u32, b: u32, c: u32) -> Result<Value> {
    dl.e(
        pc,
        0x8011_5bd8,
        json!([
            0,
            dl.f(a)?.to_bits(),
            dl.f(b)?.to_bits(),
            dl.f(c)?.to_bits()
        ]),
    )
}

/// +0xE088, BgExec's display: decide whether camera 0 sees the water from
/// above (+0x9B4C, +0x9D48), set the water task's switch byte (work +0) from
/// the region and the player's box, run +0xC028 (above) or +0xBAA0 (below),
/// then draw the animated water layers the stored region asks for.
pub fn display(dl: &DryLagoon<'_>, i: &DisplayInput) -> Result<Value> {
    if dl.local(0xe0a0)? != 0x9b4c || dl.local(0xe0d4)? != 0x9d48 || dl.local(0xe118)? != 0x98c0 {
        return Err(invalid());
    }
    let level = water_level(dl, i.cam.pos)?;
    let mut c = Vec::new();
    let above = if i.cam.pos[1] <= level {
        true
    } else {
        let v = surface_visible(
            dl,
            &SurfaceInput {
                cam: i.cam,
                level,
                tan: i.tan,
                point_y: i.point_y,
            },
        )?;
        c.extend(v["calls"].as_array().cloned().unwrap_or_default());
        v["state"]["visible"] != 0
    };
    let mut switch = Value::Null;
    if above {
        dl.dol(0xe0e4, 0x801e_7768)?;
        c.push(dl.e(0xe104, 0x8011_c2a4, json!([]))?);
        c.push(dl.m(0xe110, json!([dl.ptr(0xe10c, 6)?]))?);
        let [x, y, z] = i.player;
        let inside = camera_region(dl, i.cam.pos)? == region::LAGOON || {
            let low = dl.f(0xe128)?;
            low < x && x < dl.f(0xe140)? && dl.f(0xe154)? < z && z < dl.f(0xe16c)? && y < low
        };
        if inside {
            if !i.water {
                return Err(Error::invalid("stage 18 water task is missing"));
            }
            switch = json!(1);
        } else if i.water {
            switch = json!(0);
        }
        c.push(dl.m(0xe1c4, json!([]))?);
    } else {
        c.push(dl.e(0xe1cc, 0x8011_c254, json!([]))?);
        if i.water {
            switch = json!(1);
        }
        c.push(dl.m(0xe1f0, json!([]))?);
    }
    dl.dol(0xe1f8, 0x801c_c168)?;
    if i.flags & 2 != 0 {
        layer_open(dl, &mut c, 0xe210)?;
        c.push(dl.e(0xe240, 0x8011_6098, json!([]))?);
        c.push(translate(dl, 0xe26c, 0xe24c, 0xe258, 0xe264)?);
        c.push(dl.e(0xe278, 0x8011_6e18, json!([dl.ptr(0xe274, 5)?]))?);
        layer(
            dl,
            &mut c,
            i.clock,
            [0xe290, 0xe2b0, 0xe2c8, 0xe2d8, 0xe2bc, 0xe308],
        )?;
        layer_close(dl, &mut c, 0xe30c)?;
    }
    let mask = (dl.imm(0xe344)? as u32) << 16 | dl.imm(0xe348)? as u32;
    if i.flags & mask != 0 {
        layer_open(dl, &mut c, 0xe358)?;
        c.push(dl.e(0xe390, 0x8011_6e18, json!([dl.ptr(0xe38c, 5)?]))?);
        c.push(dl.e(0xe394, 0x8011_6098, json!([]))?);
        c.push(translate(dl, 0xe3c0, 0xe3a0, 0xe3ac, 0xe3b8)?);
        layer(
            dl,
            &mut c,
            i.clock,
            [0xe3e0, 0xe400, 0xe418, 0xe428, 0xe40c, 0xe458],
        )?;
        layer(
            dl,
            &mut c,
            i.clock,
            [0xe46c, 0xe400, 0xe498, 0xe4a8, 0xe48c, 0xe4d0],
        )?;
        layer_close(dl, &mut c, 0xe4d4)?;
    }
    // The third block tests the same mask again (lis 0x20 / addi 0x400 at +0xE50C).
    let mask3 = (dl.imm(0xe50c)? as u32) << 16 | dl.imm(0xe510)? as u32;
    if i.flags & mask3 != 0 {
        layer_open(dl, &mut c, 0xe520)?;
        c.push(dl.e(0xe558, 0x8011_6e18, json!([dl.ptr(0xe554, 5)?]))?);
        c.push(dl.e(0xe55c, 0x8011_6098, json!([]))?);
        c.push(translate(dl, 0xe588, 0xe568, 0xe574, 0xe580)?);
        layer(
            dl,
            &mut c,
            i.clock,
            [0xe5a8, 0xe5c8, 0xe5e0, 0xe5f0, 0xe5d4, 0xe620],
        )?;
        layer_close(dl, &mut c, 0xe624)?;
    }
    Ok(json!({ "calls": c, "state": { "water+0": switch } }))
}

/// +0xE670 (BgExec) and +0xE6E4 (Bg2Exec) destructors: clear .bss+0x54 or
/// +0x58 if it names the dying task. Returns the new value.
pub fn destructor(dl: &DryLagoon<'_>, bg2: bool, current: Word) -> Result<Value> {
    dl.at(if bg2 { 0xe6e8 } else { 0xe674 }, 6)?;
    Ok(if current == "task" { json!(0) } else { current })
}

/// +0xE694, Bg2Exec's display: draw model .data+0xD1E60 with texture list
/// .data+0xD1C3C at the work's +0x14 position.
pub fn bg2_display(dl: &DryLagoon<'_>, pos: [f32; 3]) -> Result<Value> {
    Ok(json!({ "calls": [
        dl.e(0xe6a8, 0x8011_6098, json!([]))?,
        dl.e(0xe6b0, 0x8011_616c, json!([pos.map(f32::to_bits)]))?,
        dl.e(0xe6bc, 0x8011_6e18, json!([dl.ptr(0xe6b8, 5)?]))?,
        dl.e(0xe6c8, 0x8011_e158, json!([dl.ptr(0xe6c4, 5)?]))?,
        dl.e(0xe6cc, 0x8011_60e4, json!([]))?,
    ], "state": {} }))
}

/// +0x17128: .bss+0x6A4 = the argument, .bss+0x6A8 and +0x6B0 = 0.
pub fn watch(dl: &DryLagoon<'_>, arg: Value) -> Result<Value> {
    let k = |pc| dl.at(pc, 6).map(|o| format!("s6+0x{o:x}"));
    Ok(json!({ "calls": [], "state": {
        k(0x1712c)?: arg, k(0x1713c)?: 0, k(0x17148)?: 0 } }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpawnInput {
    pub player: u32,
    /// Number of 12-byte entries.
    pub count: i32,
    /// Optional out pointer (r5) is set.
    pub out: bool,
    /// playertwp[player] is set; the task, the 16-byte and the entry
    /// allocations succeeded.
    pub twp: bool,
    pub task: bool,
    pub block: bool,
    pub entries: bool,
}

/// +0xA80(player, count, out): create the water task (exec +0xC10) for a
/// player, give it a 16-byte block and `count` 12-byte entries, each set up
/// by +0x894 with the .data+0x680 value. Every failure frees what was made.
pub fn water_spawn(dl: &DryLagoon<'_>, i: &SpawnInput) -> Result<Value> {
    dl.dol(0xaa8, 0x801e_7768)?;
    let mut c = Vec::new();
    let fail = |c: Vec<Value>| json!({ "calls": c, "state": { "ret": 0 } });
    if i.player >= 8 {
        return Err(Error::invalid("stage 18 player index out of range"));
    }
    if !i.twp {
        return Ok(fail(c));
    }
    let exec = dl.ptr(0xad0, 1)?;
    c.push(dl.e(0xae0, 0x8007_1e0c, json!([2, 1, exec, dl.ptr(0xadc, 5)?]))?);
    if !i.task {
        return Ok(fail(c));
    }
    c.push(dl.m(0xaf8, json!(["work+0x14"]))?);
    c.push(dl.e(0xb0c, 0x8011_97fc, json!([1, dl.imm(0xb08)?]))?);
    if !i.block {
        c.push(dl.e(0xb24, 0x8007_1af0, json!(["task"]))?);
        return Ok(fail(c));
    }
    c.push(dl.e(0xb38, 0x8011_97fc, json!([i.count, dl.imm(0xb34)?]))?);
    if !i.entries {
        c.push(dl.e(0xb54, 0x8011_9828, json!(["block"]))?);
        c.push(dl.e(0xb64, 0x8007_1af0, json!(["task"]))?);
        return Ok(fail(c));
    }
    let value = dl.word(5, dl.at(0xba4, 5)?)?;
    for k in 0..i.count.max(0) {
        c.push(dl.m(
            0xbc4,
            json!([format!("entries+0x{:x}", k * dl.imm(0xb98)?), value, 0]),
        )?);
    }
    Ok(json!({ "calls": c, "state": {
        "ret": "task", "work+3": i.player as u8,
        "task": { "0x1c": dl.ptr(0xb74, 1)?, "0x10": exec, "0x18": dl.ptr(0xb84, 1)? },
        "block": ["entries", i.count, if i.out { json!("out") } else { json!(0) }],
        "out": if i.out { json!("task") } else { Value::Null } } }))
}

/// Run one routine by name on a JSON input.
pub fn replay(rel_bytes: &[u8], routine: &str, input: &Value) -> Result<Value> {
    let dl = DryLagoon::read(rel_bytes)?;
    fn parse<T: serde::de::DeserializeOwned>(v: &Value) -> Result<T> {
        T::deserialize(v).map_err(|e| Error::invalid(format!("stage 18 request: {e}")))
    }
    #[derive(Deserialize)]
    struct Cam {
        cam: [f32; 3],
    }
    match routine {
        "setup" => setup(&dl, &parse(input)?),
        "teardown" => teardown(&dl, &parse(input)?),
        "exec" => exec(&dl, &parse(input)?),
        "exec_after" => exec_after(&dl),
        "bg_clip" => bg_clip(&dl, parse::<Cam>(input)?.cam),
        "ambient" => ambient(&dl, &parse(input)?),
        "display" => display(&dl, &parse(input)?),
        "surface_visible" => surface_visible(&dl, &parse(input)?),
        "water_spawn" => water_spawn(&dl, &parse(input)?),
        _ => Err(Error::invalid(format!(
            "unknown stage 18 routine {routine}"
        ))),
    }
}
