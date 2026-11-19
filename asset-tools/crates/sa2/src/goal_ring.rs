//! GOALRING's verified collision setup and contact branch, not the mission result handler.
use crate::{city_rel::CityRel, fan_stage::MountedStage, manifest, stage_land, Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
const ROWS: &[(u16, u32)] = include!("goal_ring_rows.rs");
const HASH: &str = "130c8b883671a80dd937bd8765bfea39d119219294ce34da987678a364647396";
#[derive(Debug, Serialize)]
pub struct Program {
    pub level: u16,
    pub module: u32,
    pub object_init: u32,
    pub contact_offset: u32,
    pub local_effect: u32,
    pub slot_words_offset: u32,
    #[serde(skip)]
    dispatch_codes: Vec<Option<u16>>,
    pub fallback_message_offset: u32,
    pub initial_slot_words: [u32; 2],
    #[serde(skip)]
    child_callbacks: [u32; 3],
    #[serde(skip)]
    child_handlers: [u32; 5],
    #[serde(skip)]
    child_mode_one: [f32; 4],
    #[serde(skip)]
    child_idle_step: f32,
    #[serde(skip)]
    child_actions: Vec<(u32, u32)>,
    #[serde(skip)]
    action_services: Vec<ActionRoutine>,
    #[serde(skip)]
    child_effect: [f32; 4],
    #[serde(skip)]
    child_helpers: [u32; 2],
    #[serde(skip)]
    child_collision_descriptor: u32,
    pub child_entry: u32,
    pub child_parent_callback: u32,
    pub child_scale_bits: u32,
    pub radii: [f32; 2],
    pub spin: [i32; 2],
    pub rise: f32,
}
fn invalid() -> Error {
    Error::invalid("unverified goal ring module or callback")
}
fn word(bytes: &[u8], off: usize) -> Result<u32> {
    Ok(u32::from_be_bytes(
        bytes
            .get(off..off + 4)
            .ok_or_else(invalid)?
            .try_into()
            .unwrap(),
    ))
}
#[derive(Debug)]
enum ActionRoutine {
    Scaled(ActionServices),
    Player(u32),
    Linked,
    Position(u32),
    Empty,
}
#[derive(Debug)]
struct ActionServices {
    first: u16,
    second: Option<u16>,
    scalar: u32,
}
impl Program {
    pub fn read(level: u16, bytes: &[u8]) -> Result<Self> {
        let &(_, row) = ROWS
            .iter()
            .find(|(id, _)| *id == level)
            .ok_or_else(invalid)?;
        let name = format!("stg{level:02}D.rel");
        let info = stage_land::catalog()
            .iter()
            .find(|i| i.filename == name)
            .ok_or_else(invalid)?;
        if bytes.len() != info.size || manifest::sha256(bytes) != info.sha256 {
            return Err(invalid());
        }
        let rel = CityRel::read_identified(bytes, info.module)?;
        let (s, n) = rel.local_pointer(5, row)?.ok_or_else(invalid)?;
        if rel.data(s)?.get(n as usize..n as usize + 9) != Some(b"GOALRING\0") {
            return Err(invalid());
        }
        let (s, init) = rel.local_pointer(5, row - 4)?.ok_or_else(invalid)?;
        if s != 1 {
            return Err(invalid());
        }
        let start = init + 0x844;
        let mut code = rel
            .data(1)?
            .get(start as usize..start as usize + 0xfc)
            .ok_or_else(invalid)?
            .to_vec();
        let bl = word(&code, 0xac)?;
        let delta = ((bl & 0x03ff_fffc) << 6) as i32 >> 6;
        let target = (start + 0xac)
            .checked_add_signed(delta)
            .ok_or_else(invalid)?;
        code[0xac..0xb0].copy_from_slice(&(bl & 0xfc000003).to_be_bytes());
        if manifest::sha256(&code) != HASH {
            return Err(invalid());
        }
        let helper = rel
            .data(1)?
            .get(target as usize..target as usize + 40)
            .ok_or_else(invalid)?;
        if manifest::sha256(helper)
            != "68168108a24436dd9c7506e8fa5dc0d8eb61a1cd6dd1746856363cf9518f64e6"
        {
            return Err(invalid());
        }
        let hi = rel.reference(1, target + 0x16).ok_or_else(invalid)?;
        let lo = rel.reference(1, target + 0x1a).ok_or_else(invalid)?;
        if (
            hi.module, hi.kind, hi.section, lo.module, lo.kind, lo.section, lo.addend,
        ) != (info.module, 6, 5, info.module, 4, 5, hi.addend)
        {
            return Err(invalid());
        }
        let slot_words_offset = hi.addend;
        let data = rel.data(5)?;
        let at = slot_words_offset as usize;
        let initial_slot_words = [word(data, at)?, word(data, at + 4)?];
        // Locate the switch by its engine query and the preceding read of our slot array.
        let instructions = rel.data(1)?;
        let switch = (0..instructions.len() as u32)
            .step_by(4)
            .find(|&off| {
                off >= 0x50
                    && rel
                        .reference(1, off)
                        .is_some_and(|r| r.module == 0 && r.kind == 10 && r.addend == 0x8001dcf4)
                    && rel.reference(1, off - 0x4e).is_some_and(|r| {
                        r.module == info.module
                            && r.section == 5
                            && r.kind == 4
                            && r.addend == slot_words_offset
                    })
            })
            .ok_or_else(invalid)?;
        let table = rel
            .reference(1, switch + 0x16)
            .filter(|r| r.module == info.module && r.kind == 4 && r.section == 5)
            .ok_or_else(invalid)?;
        let mut message_helper = None;
        let mut dispatch_codes = Vec::with_capacity(93);
        for i in 0..93 {
            let (section, branch) = rel
                .local_pointer(5, table.addend + i * 4)?
                .ok_or_else(invalid)?;
            if section != 1 {
                return Err(invalid());
            }
            let ins = word(instructions, branch as usize)?;
            dispatch_codes.push(if branch == switch + 0x2b8 {
                None
            } else if ins >> 16 == 0x3860 {
                let site = branch + 12;
                let call = word(instructions, site as usize)?;
                if call & 0xfc00_0003 != 0x4800_0001 {
                    return Err(invalid());
                }
                let dest = site
                    .checked_add_signed(((call & 0x03ff_fffc) << 6) as i32 >> 6)
                    .ok_or_else(invalid)?;
                if message_helper.replace(dest).is_some_and(|v| v != dest) {
                    return Err(invalid());
                }
                Some(ins as u16)
            } else {
                return Err(invalid());
            });
        }
        let h = message_helper.ok_or_else(invalid)?;
        if manifest::sha256(
            instructions
                .get(h as usize..h as usize + 0x98)
                .ok_or_else(invalid)?,
        ) != "d311986b7441f00116f026399a3a013990f84219e0fb1faf056b139591e6a06c"
        {
            return Err(invalid());
        }
        for (off, engine) in [(0x60, 0x800dfe0c), (0x84, 0x800dfe3c)] {
            if !rel
                .reference(1, h + off)
                .is_some_and(|r| r.module == 0 && r.kind == 10 && r.addend == engine)
            {
                return Err(invalid());
            }
        }
        let fallback_message_offset = rel
            .reference(1, h + 0x7a)
            .filter(|r| r.module == info.module && r.kind == 4 && r.section == 5)
            .ok_or_else(invalid)?
            .addend;
        let child = init + 0x110;
        if manifest::sha256(
            instructions
                .get(child as usize..child as usize + 0x80)
                .ok_or_else(invalid)?,
        ) != "d9442669cc51ee371849de8fec3f5e240622c0f4d1a34f9af172ea963f7881ca"
            || !rel
                .reference(1, child + 0x1c)
                .is_some_and(|r| r.module == 0 && r.kind == 10 && r.addend == 0x80070790)
        {
            return Err(invalid());
        }
        let child_ref = |off, section| {
            rel.reference(1, child + off)
                .filter(|r| r.module == info.module && r.section == section && r.kind == 4)
                .map(|r| r.addend)
                .ok_or_else(invalid)
        };
        let child_entry = child_ref(0x16, 1)?;
        let entry = child_entry as usize;
        let dispatcher = instructions.get(entry..entry + 0x13c).ok_or_else(invalid)?;
        if manifest::sha256(dispatcher)
            != if matches!(level, 16 | 18) {
                "6a67b40ddea1924d939cae361130487a98bd6c1f185df5053ea5077e74e49fa6"
            } else {
                "d875337a8d45e3c2ccc46a5ae9a46c08668761af044c4ef014549f48b027c02e"
            }
        {
            return Err(invalid());
        }
        for (off, engine) in [(0x30, 0x8006538c), (0x10c, 0x8000f4ec)] {
            if !rel
                .reference(1, child_entry + off)
                .is_some_and(|r| r.kind == 10 && r.module == 0 && r.addend == engine)
            {
                return Err(invalid());
            }
        }
        let callback = |off| {
            rel.reference(1, child_entry + off)
                .filter(|r| r.kind == 4 && r.section == 1 && r.module == info.module)
                .map(|r| r.addend)
                .ok_or_else(invalid)
        };
        let child_callbacks = [callback(0x62)?, callback(0x6e)?, callback(0x8e)?];
        let handler = |off| -> Result<u32> {
            let ins = word(dispatcher, off)?;
            if ins & 0xfc000003 != 0x48000001 {
                return Err(invalid());
            }
            (child_entry + off as u32)
                .checked_add_signed(((ins & 0x03fffffc) << 6) as i32 >> 6)
                .ok_or_else(invalid)
        };
        let child_handlers = [
            handler(0xdc)?,
            handler(0xe8)?,
            handler(0xf4)?,
            handler(0x100)?,
            handler(0x118)?,
        ];
        let two = child_handlers[2];
        if manifest::sha256(
            instructions
                .get(two as usize..two as usize + 0x94)
                .ok_or_else(invalid)?,
        ) != "81673aa59887f6d1db6fee246b846cf74a9e41e69fbad2259265725bcff910ac"
        {
            return Err(invalid());
        }
        for (offset, addr) in [
            (0x50, 0x800066f4),
            (0x60, 0x80006574),
            (0x74, 0x80037c74),
            (0x1e4, 0x8000f4ec),
        ] {
            if !rel
                .reference(1, two + offset)
                .is_some_and(|r| r.module == 0 && r.kind == 10 && r.addend == addr)
            {
                return Err(invalid());
            }
        }
        let idle_constant = rel
            .reference(1, two + 0x20a)
            .filter(|r| r.module == info.module && r.section == 4 && r.kind == 4)
            .ok_or_else(invalid)?;
        let child_idle_step = f32::from_bits(word(rel.data(4)?, idle_constant.addend as usize)?);
        let local_ref = |off, section| {
            rel.reference(1, two + off)
                .filter(|r| r.kind == 4 && r.module == info.module && r.section == section)
                .map(|r| r.addend)
                .ok_or_else(invalid)
        };
        let table = local_ref(0xca, 5)?;
        let mut child_actions = Vec::new();
        let mut action_services = Vec::new();
        for i in 0..11 {
            let at = table + i * 8;
            let (section, func) = rel.local_pointer(5, at + 4)?.ok_or_else(invalid)?;
            if section != 1 {
                return Err(invalid());
            }
            let service = if matches!(i, 1 | 3 | 4 | 7) {
                let imm = |off| -> Result<u16> {
                    let ins = word(instructions, (func + off) as usize)?;
                    let expected = if off == 0x14 { 0x3860 } else { 0x3880 };
                    if ins >> 16 != expected {
                        return Err(invalid());
                    }
                    Ok(ins as u16)
                };
                let short = i == 7;
                for (off, address) in [
                    (0x18, 0x80063524),
                    (if short { 0x2c } else { 0x38 }, 0x80035a88),
                ] {
                    if !rel
                        .reference(1, func + off)
                        .is_some_and(|r| r.module == 0 && r.kind == 10 && r.addend == address)
                    {
                        return Err(invalid());
                    }
                }
                if !short
                    && !rel
                        .reference(1, func + 0x24)
                        .is_some_and(|r| r.module == 0 && r.kind == 10 && r.addend == 0x80063584)
                {
                    return Err(invalid());
                }
                let r = rel
                    .reference(1, func + if short { 0x26 } else { 0x32 })
                    .filter(|r| r.module == info.module && r.section == 4 && r.kind == 4)
                    .ok_or_else(invalid)?;
                ActionRoutine::Scaled(ActionServices {
                    first: imm(0x14)?,
                    second: if short { None } else { Some(imm(0x20)?) },
                    scalar: word(rel.data(4)?, r.addend as usize)?,
                })
            } else {
                let engine = |off| {
                    rel.reference(1, func + off)
                        .filter(|r| r.module == 0 && r.kind == 10)
                        .map(|r| r.addend)
                        .ok_or_else(invalid)
                };
                if i != 9
                    && (engine(if i == 2 { 0x20 } else { 0x18 })? != 0x80063524
                        || word(
                            instructions,
                            (func + if i == 2 { 0x1c } else { 0x14 }) as usize,
                        )? != 0x386000c8)
                {
                    return Err(invalid());
                }
                match i {
                    0 | 5 | 8 | 10 => ActionRoutine::Player(engine(0x20)?),
                    2 => {
                        if engine(0x2c)? != 0x80063f50
                            || engine(0x54)? != 0x800651ac
                            || engine(0x6c)? != 0x800651ac
                        {
                            return Err(invalid());
                        }
                        ActionRoutine::Linked
                    }
                    6 => {
                        let site = func + 0x24;
                        let ins = word(instructions, site as usize)?;
                        if ins & 0xfc000003 != 0x48000001 {
                            return Err(invalid());
                        }
                        ActionRoutine::Position(
                            site.checked_add_signed(((ins & 0x03fffffc) << 6) as i32 >> 6)
                                .ok_or_else(invalid)?,
                        )
                    }
                    9 if word(instructions, func as usize)? == 0x4e800020 => ActionRoutine::Empty,
                    _ => return Err(invalid()),
                }
            };
            action_services.push(service);
            child_actions.push((word(rel.data(5)?, at as usize)?, func));
        }
        let scalar = |off| -> Result<f32> {
            Ok(f32::from_bits(word(
                rel.data(4)?,
                local_ref(off, 4)? as usize,
            )?))
        };
        let child_effect = [
            scalar(0x12a)?,
            scalar(0x13e)?,
            scalar(0x14e)?,
            scalar(0x16e)?,
        ];
        let branch = |off| -> Result<u32> {
            let site: u32 = two + off;
            let ins = word(instructions, site as usize)?;
            if ins & 0xfc000003 != 0x48000001 {
                return Err(invalid());
            }
            site.checked_add_signed(((ins & 0x03fffffc) << 6) as i32 >> 6)
                .ok_or_else(invalid)
        };
        let child_helpers = [branch(0xd4)?, branch(0x174)?];
        let one = child_handlers[1];
        if manifest::sha256(
            instructions
                .get(one as usize..one as usize + 0x98)
                .ok_or_else(invalid)?,
        ) != "8afd654d6ffeeaf1a180f9411e6677dcce45c9cdf6746541f33d44bb4ae3f102"
            || !rel
                .reference(1, one + 0xa)
                .is_some_and(|r| r.module == 0 && r.kind == 4 && r.addend == 0x801cbd88)
        {
            return Err(invalid());
        }
        let constant = |off| -> Result<f32> {
            let r = rel
                .reference(1, one + off)
                .filter(|r| r.module == info.module && r.section == 4 && r.kind == 4)
                .ok_or_else(invalid)?;
            Ok(f32::from_bits(word(rel.data(4)?, r.addend as usize)?))
        };
        let child_mode_one = [
            constant(0x22)?,
            constant(0x42)?,
            constant(0x5a)?,
            constant(0x86)?,
        ];
        let setup = child_handlers[3];
        let release = child_handlers[4];
        for (offset, length, hash) in [
            (
                setup,
                0x5c,
                "a418a5f1e98378b0227a93db9c1a55de77ffa9318852f3679ba3ffdc70ca5e56",
            ),
            (
                release,
                0x38,
                "5c80053ca87f0e47b8ea6fd54a6b6e4d1a254de78c7a33cd144865e8c4cd9c2f",
            ),
        ] {
            let start = offset as usize;
            if manifest::sha256(
                instructions
                    .get(start..start + length)
                    .ok_or_else(invalid)?,
            ) != hash
            {
                return Err(invalid());
            }
        }
        for (site, address) in [
            (setup + 0x20, 0x800067f8),
            (setup + 0x38, 0x8000f7d8),
            (release + 0x18, 0x80018b88),
        ] {
            if !rel
                .reference(1, site)
                .is_some_and(|r| r.kind == 10 && r.module == 0 && r.addend == address)
            {
                return Err(invalid());
            }
        }
        let child_collision_descriptor = rel
            .reference(1, setup + 0x2e)
            .filter(|r| r.kind == 4 && r.module == info.module && r.section == 5)
            .ok_or_else(invalid)?
            .addend;
        let child_parent_callback = child_ref(0x2e, 1)?;
        let child_scale_bits = word(rel.data(4)?, child_ref(0x3a, 4)? as usize)?;
        // Verify the actual callees even though the code hashes cover unrelocated branches.
        for (off, addr) in [
            (0x14, 0x800066f4),
            (0x20, 0x80037c74),
            (0x7c, 0x8001c78c),
            (0xa0, 0x8006ad94),
            (0xcc, 0x8006ad94),
            (0xd4, 0x8001baa4),
            (0xf8, 0x8000f4ec),
        ] {
            let r = rel.reference(1, start + off).ok_or_else(invalid)?;
            if (r.module, r.kind, r.addend) != (0, 10, addr) {
                return Err(invalid());
            }
        }
        let r = rel.reference(1, init + 0x28a).ok_or_else(invalid)?;
        if (r.module, r.kind) != (info.module, 6) {
            return Err(invalid());
        }
        let data = rel.data(r.section)?;
        let mut radii = [0.; 2];
        for (i, radius) in radii.iter_mut().enumerate() {
            let at = r.addend as usize + 48 * i;
            if word(data, at)? != 0x7000
                || word(data, at + 4)? != 0x8000
                || (8..20)
                    .step_by(4)
                    .any(|o| word(data, at + o).ok() != Some(0))
            {
                return Err(invalid());
            }
            *radius = f32::from_bits(word(data, at + 20)?);
            if !radius.is_finite() || *radius <= 0. {
                return Err(invalid());
            }
        }
        let instructions = rel.data(1)?;
        for (offset, length, hash) in [
            (
                0x5f8,
                0x68,
                "96efb20d8a54e6dbe19cb4d2020ca30e248e21466929d49fba85e4bd0dc11650",
            ),
            (
                0xacc,
                0x3c,
                "1db9e90fdb5660cfc98be6ff0bb8616f263ae76343219f3c6438841b6bf4d1fd",
            ),
        ] {
            let at = init as usize + offset;
            if manifest::sha256(instructions.get(at..at + length).ok_or_else(invalid)?) != hash {
                return Err(invalid());
            }
        }
        let constant = |hi, lo| -> Result<u32> {
            let a = rel.reference(1, init + hi).ok_or_else(invalid)?;
            let b = rel.reference(1, init + lo).ok_or_else(invalid)?;
            if (
                a.module, a.section, a.kind, b.module, b.section, b.kind, b.addend,
            ) != (info.module, 5, 6, info.module, 5, 4, a.addend)
            {
                return Err(invalid());
            }
            word(rel.data(5)?, a.addend as usize)
        };
        let spin = [
            constant(0x64e, 0x652)? as i32,
            constant(0x60e, 0x612)? as i32,
        ];
        let rise = f32::from_bits(constant(0x632, 0x636)?);
        for (offset, addr) in [(0xaf8, 0x80065148), (0xb04, 0x80071878)] {
            let r = rel.reference(1, init + offset).ok_or_else(invalid)?;
            if (r.module, r.kind, r.addend) != (0, 10, addr) {
                return Err(invalid());
            }
        }
        Ok(Self {
            level,
            module: info.module,
            object_init: init,
            contact_offset: start,
            local_effect: target,
            dispatch_codes,
            fallback_message_offset,
            slot_words_offset,
            initial_slot_words,
            child_callbacks,
            child_handlers,
            child_mode_one,
            child_idle_step,
            child_actions,
            action_services,
            child_effect,
            child_helpers,
            child_collision_descriptor,
            child_entry,
            child_parent_callback,
            child_scale_bits,
            radii,
            spin,
            rise,
        })
    }
}
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Hit {
    None,
    Unidentified,
    Player { slot: u8 },
}
#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum Call {
    CheckHit,
    Identify,
    Register,
    Main {
        address: u32,
        slot: u8,
    },
    Sound {
        code: u16,
        priority: u8,
        duration: u16,
    },
    Local {
        offset: u32,
        slot: u8,
        argument: u8,
    },
}
#[derive(Default, Debug, Serialize)]
pub struct Contact {
    pub latched: bool,
}
impl Contact {
    /// Only after native initialization and once the task reaches its contact code.
    pub fn step(
        &mut self,
        angle_x: i32,
        game_byte: i8,
        hit: Hit,
        local_effect: u32,
    ) -> Result<Vec<Call>> {
        if matches!(hit,Hit::Player{slot} if slot>1) {
            return Err(Error::invalid("goal contact slot must be 0 or 1"));
        }
        if self.latched {
            return Ok(vec![Call::Register]);
        }
        let mut out = vec![Call::CheckHit];
        if !matches!(hit, Hit::None) {
            out.push(Call::Identify);
        }
        let Hit::Player { slot } = hit else {
            out.push(Call::Register);
            return Ok(out);
        };
        if game_byte == 1 || (game_byte == 2 && angle_x % 3 == 0) {
            out.push(Call::Sound {
                code: 0x1014,
                priority: 30,
                duration: 180,
            });
            out.push(Call::Main {
                address: 0x8001baa4,
                slot,
            });
        } else {
            out.push(Call::Main {
                address: 0x8001c78c,
                slot,
            });
            out.push(Call::Sound {
                code: 0x1013,
                priority: 30,
                duration: 180,
            });
            out.push(Call::Local {
                offset: local_effect,
                slot,
                argument: 2,
            });
        }
        self.latched = true;
        Ok(out)
    }
}
/// Apply the module-local handoff made by this goal to its two player words.
pub fn apply_slot_writes(words: &mut [u32; 2], calls: &[Call], helper: u32) {
    for call in calls {
        if let Call::Local {
            offset,
            slot,
            argument,
        } = *call
        {
            if offset == helper && slot < 2 && argument < 4 {
                words[usize::from(slot)] = u32::from(argument);
            }
        }
    }
}
impl Program {
    /// Only the observed word-2 branch of the stage task; other modes have separate paths.
    pub fn consume_goal_word(
        &self,
        words: &mut [u32; 2],
        slot: u8,
        query: i32,
    ) -> Result<Option<u16>> {
        let w = words
            .get_mut(usize::from(slot))
            .filter(|v| **v == 2)
            .ok_or_else(|| Error::invalid("expected a pending goal word in the selected slot"))?;
        let index = query.wrapping_sub(3) as u32;
        let code = self.dispatch_codes.get(index as usize).copied().flatten();
        *w = 0;
        Ok(code)
    }
}
/// First invocation on a fresh SET task. A child route has its own executor.
#[derive(Debug, PartialEq, Serialize)]
pub enum InitRoute {
    Contact { angle_x: i32 },
    Deferred,
    ChildTask,
    OutOfRange,
    NoWork,
}
pub fn initialize(
    angle_x: i32,
    character: i8,
    game_byte: i8,
    slot_zero_timer: Option<u16>,
    range_out: bool,
    allocated: bool,
) -> InitRoute {
    let mut x = angle_x;
    let mut bypass_range = false;
    if matches!(character, 4 | 5) {
        if game_byte == 1 {
            x = 2;
            if !slot_zero_timer.is_some_and(|t| t >= 120) {
                return InitRoute::Deferred;
            }
            bypass_range = true;
        } else if x % 3 != 1 {
            return InitRoute::Deferred;
        }
    }
    if game_byte != 2 && x % 3 == 1 {
        return InitRoute::ChildTask;
    }
    if !bypass_range && range_out {
        return InitRoute::OutOfRange;
    }
    if allocated {
        InitRoute::Contact { angle_x: x }
    } else {
        InitRoute::NoWork
    }
}
/// Regular (angle X divisible by three) task fields updated before contact detection.
#[derive(Clone, Copy, Serialize)]
pub struct RegularMotion {
    pub yaw: i32,
    pub y: f32,
    pub timer: u16,
}
impl RegularMotion {
    pub fn tick(&mut self, latched: bool, game_byte_37: i8, spin: [i32; 2], rise: f32) {
        if game_byte_37 == 0 {
            self.yaw = self.yaw.wrapping_add(spin[usize::from(latched)]);
            if latched {
                self.timer = self.timer.wrapping_add(1);
                self.y += rise;
            }
        }
    }
    pub fn cleanup_call(self, latched: bool, task_field_30_present: bool) -> Option<u32> {
        (latched && self.timer >= 60).then_some(if task_field_30_present {
            0x80065148
        } else {
            0x80071878
        })
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LifecycleInput {
    game_byte_37: i8,
    task_field_30_present: bool,
    initial_timer: u16,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MessageContext {
    pub count: i32,
    pub handle: u32,
    pub work_address: u32,
    pub section5_address: u32,
    pub indexed: Option<IndexedMessage>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexedMessage {
    pub pointer: u32,
    pub game_byte_17: i8,
}
impl Program {
    pub fn message_call(&self, code: u16, m: MessageContext) -> Result<Value> {
        let work = m.work_address.wrapping_add(0x3c);
        if i32::from(code) < m.count {
            let entry = m
                .indexed
                .ok_or_else(|| Error::invalid("missing indexed goal message"))?;
            Ok(
                json!({"engine":0x800dfe0cu32,"args":[m.handle,entry.pointer,120,entry.game_byte_17,work,0]}),
            )
        } else {
            if m.indexed.is_some() {
                return Err(Error::invalid(
                    "fallback does not read indexed goal message",
                ));
            }
            Ok(
                json!({"engine":0x800dfe3cu32,"args":[m.handle,m.section5_address.wrapping_add(self.fallback_message_offset),120,0,work]}),
            )
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConsumerInput {
    message: Option<MessageContext>,
    slot: u8,
    query: i32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChildObservation {
    pub existing: bool,
    pub allocation: Option<bool>,
    /// Parent child link re-read after the optional engine allocation call.
    pub linked_after: bool,
}
impl Program {
    /// Consecutive observed invocations of the mode-1 handler, stopping at its transition.
    pub fn child_mode_one(
        &self,
        mut work36: f32,
        mut work40: f32,
        game_bytes: &[i8],
    ) -> Result<Value> {
        if !(1..=2048).contains(&game_bytes.len()) {
            return Err(Error::invalid("expected 1..2048 child mode-1 updates"));
        }
        let [forced, step, limit, increment] = self.child_mode_one;
        let mut frames = Vec::new();
        for &byte in game_bytes {
            let advance = if byte != 0 {
                work40 = forced;
                true
            } else {
                work40 += step;
                let advance = work40 >= limit;
                if advance {
                    work40 = limit;
                }
                work36 += increment;
                advance
            };
            frames.push(json!({"mode":if advance{2}else{1},"work36Bits":work36.to_bits(),"work40Bits":work40.to_bits()}));
            if advance {
                break;
            }
        }
        Ok(json!(frames))
    }
    fn action_calls(&self, index: usize, slot: i32, a: &ChildActivation) -> Result<Vec<Value>> {
        let routine = self.action_services.get(index).ok_or_else(invalid)?;
        if a.action_links.is_some() && !matches!(routine, ActionRoutine::Linked) {
            return Err(invalid());
        }
        let mut calls = vec![json!(["engine", 0x80063524u32, 200])];
        match routine {
            ActionRoutine::Scaled(s) => {
                calls[0] = json!(["engine", 0x80063524u32, s.first]);
                if let Some(v) = s.second {
                    calls.push(json!(["engine", 0x80063584u32, slot, v]));
                }
                calls.push(json!(["engine",0x80035a88u32,slot,{"f1Bits":s.scalar}]));
            }
            ActionRoutine::Empty => calls.clear(),
            ActionRoutine::Player(addr) => calls.push(json!(["engine", addr, slot])),
            ActionRoutine::Position(addr) => calls.push(json!(["local", addr, "work+20"])),
            ActionRoutine::Linked => {
                calls.push(json!(["engine", 0x80063f50u32, slot, 1]));
                let l = a
                    .action_links
                    .as_ref()
                    .ok_or_else(|| Error::invalid("missing row-2 action link observations"))?;
                if l.parent_link.is_some() != (l.byte1 != 0)
                    || l.task_link.is_some() != (l.parent_link != Some(true))
                {
                    return Err(invalid());
                }
                if l.parent_link == Some(true) {
                    calls.push(json!(["engine", 0x800651acu32, "parent"]));
                } else if l.task_link == Some(true) {
                    calls.push(json!(["engine", 0x800651acu32, "task"]));
                }
            }
        }
        Ok(calls)
    }
    pub fn child_contact(&self, c: ChildContact) -> Result<Value> {
        let bad = || Error::invalid("inconsistent child mode-2 contact observations");
        let mut calls = Vec::new();
        let cached = i32::from(c.cached);
        let direct = c.flags & 4 != 0 && cached != -1;
        let mut slot = cached;
        if direct {
            if c.primary.is_some() || c.secondary.is_some() || c.identified.is_some() {
                return Err(bad());
            }
        } else {
            calls.push(0x800066f4u32);
            let hit = if c.primary.ok_or_else(bad)? {
                if c.secondary.is_some() {
                    return Err(bad());
                }
                true
            } else {
                calls.push(0x80006574);
                c.secondary.ok_or_else(bad)?
            };
            if hit {
                calls.push(0x80037c74);
                let id = c.identified.ok_or_else(bad)?;
                if id >= 0 {
                    slot = id;
                }
            } else {
                if c.identified.is_some() {
                    return Err(bad());
                }
                slot = -1;
            }
        }
        if slot < 0 {
            if c.activation.is_some() {
                return Err(bad());
            }
            let idle = c.idle.ok_or_else(bad)?;
            calls.push(0x8000f4ec);
            let v = if idle.game_byte_55 == 0 {
                idle.work36 + self.child_idle_step
            } else {
                idle.work36
            };
            Ok(json!({"calls":calls,"playerIndex":null,"mode":2,"work36Bits":v.to_bits()}))
        } else {
            if c.idle.is_some() {
                return Err(bad());
            }
            if let Some(a) = c.activation {
                let mut events = Vec::new();
                if a.player_flags & 0x4000 == 0 {
                    let [before, after] = a.indices.ok_or_else(bad)?;
                    let &(code, _) = self.child_actions.get(before).ok_or_else(bad)?;
                    let &(_, callback) = self.child_actions.get(after).ok_or_else(bad)?;
                    events.push(json!(["local", self.child_helpers[0], slot, code]));
                    events.push(json!(["local", callback, "task", slot]));
                    events.extend(self.action_calls(after, slot, &a)?);
                    events.push(json!(["engine", 0x800168c8u32, slot]));
                } else if a.indices.is_some() || a.action_links.is_some() {
                    return Err(bad());
                }
                let [dy, xz, y, strength] = self.child_effect;
                let [x, py, z] = a.position;
                events.push(json!([
                    "local",
                    self.child_helpers[1],
                    [x, py + dy, z],
                    [xz, y, xz],
                    strength
                ]));
                events.push(json!(["engine", 0x8006b710u32, 0x800d, 0, 0, 0]));
                events.push(json!(["engine", 0x8002f8c4u32, slot, 1, 0x30, 0]));
                if a.linked_task {
                    events.push(json!(["engine", 0x8006521cu32, "task"]));
                } else if a.parent_linked == Some(true) {
                    events.push(json!(["engine", 0x8006521cu32, "parent"]));
                }
                if a.parent_linked.is_some() != (!a.linked_task && a.byte1 != 0) {
                    return Err(bad());
                }
                events.push(json!(["engine", 0x8000f4ecu32, "task"]));
                let v = if a.tail.game_byte_55 == 0 {
                    a.tail.work36 + self.child_idle_step
                } else {
                    a.tail.work36
                };
                return Ok(
                    json!({"calls":calls,"playerIndex":slot,"events":events,"mode":3,"work36Bits":v.to_bits()}),
                );
            }
            Ok(json!({"calls":calls,"playerIndex":slot,"continueAt":self.child_handlers[2]+0x94}))
        }
    }
    pub fn child_dispatch(&self, mode: i8, byte1: i8, range_out: Option<bool>) -> Result<Value> {
        if (byte1 == 0) != range_out.is_some() {
            return Err(Error::invalid(
                "range observation required only for child byte 1 equal to zero",
            ));
        }
        let mut writes = None;
        let mut call = None;
        let mut reset = false;
        if range_out != Some(true) {
            writes = match mode {
                0..=2 => Some([self.child_callbacks[0], self.child_callbacks[1]]),
                3..=4 => Some([self.child_callbacks[2], 0]),
                _ => None,
            };
            call = match mode {
                0..=3 => Some(json!({"local":self.child_handlers[mode as usize]})),
                4 | 5 => Some(json!({"engine":0x8000f4ecu32})),
                6 => Some(json!({"local":self.child_handlers[4]})),
                _ => None,
            };
            reset = !(0..=5).contains(&mode);
        }
        let mut report = json!({"callbackWrites20And32":writes,"taskCall":call,"resetModeAfterCall":reset,"rangeChecked":byte1==0});
        if range_out != Some(true) {
            match mode {
                3 => {
                    report["handler"] = json!({"calls":[[0x800067f8u32,"task"],[0x8000f7d8u32,"task",self.child_collision_descriptor,2,4]],"modeWrite":4})
                }
                6 => report["handler"] = json!({"calls":[[0x80018b88u32,"task"]],"task60Write":0}),
                _ => {}
            }
        }
        Ok(report)
    }
    pub fn child_placement(&self, v: ChildObservation, position: [f32; 3]) -> Result<Value> {
        if v.existing == v.allocation.is_some() || (v.existing && !v.linked_after) {
            return Err(Error::invalid(
                "inconsistent goal child allocation observations",
            ));
        }
        let create=v.allocation.map(|_|json!({"engine":0x80070790u32,"priority":2,"entry":self.child_entry,"parent":"goalTask"}));
        let initialized = if v.allocation == Some(true) {
            Some(
                json!({"parentCallback32":self.child_parent_callback,"childWork32Bits":self.child_scale_bits,"childWorkByte1":1}),
            )
        } else {
            None
        };
        Ok(
            json!({"create":create,"initialized":initialized,"linkedChildPosition":v.linked_after.then_some(position)}),
        )
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModeOneInput {
    work36: f32,
    work40: f32,
    game_bytes_55: Vec<i8>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionLinks {
    pub byte1: i8,
    pub parent_link: Option<bool>,
    pub task_link: Option<bool>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChildActivation {
    pub action_links: Option<ActionLinks>,
    pub player_flags: u16,
    pub indices: Option<[usize; 2]>,
    pub position: [f32; 3],
    pub linked_task: bool,
    pub byte1: i8,
    pub parent_linked: Option<bool>,
    pub tail: ChildIdle,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChildContact {
    pub activation: Option<ChildActivation>,
    pub flags: u16,
    pub cached: i8,
    pub primary: Option<bool>,
    pub secondary: Option<bool>,
    pub identified: Option<i32>,
    pub idle: Option<ChildIdle>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChildIdle {
    pub game_byte_55: i8,
    pub work36: f32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChildDispatch {
    contact: Option<ChildContact>,
    mode_one: Option<ModeOneInput>,
    mode: i8,
    byte1: i8,
    range_out: Option<bool>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    child_dispatch: Option<ChildDispatch>,
    child: Option<ChildObservation>,
    post_contact_consumer: Option<ConsumerInput>,
    slot_zero_timer: Option<u16>,
    range_out: bool,
    allocation_ok: bool,
    initial_slot_words: Option<[u32; 2]>,
    game_byte_23: i8,
    frames: Vec<Hit>,
    lifecycle: Option<LifecycleInput>,
    probe: Option<Probe>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Probe {
    center: [f64; 3],
    radius: f64,
}
pub fn replay(
    stage: &MountedStage<'_>,
    layer: usize,
    index: usize,
    module: &[u8],
    request: &[u8],
) -> Result<Value> {
    let (row, def) = stage.resolve(layer, index).ok_or_else(invalid)?;
    let def = def.ok_or_else(invalid)?;
    if (def.function_va, def.name.as_str()) != (0x6c63c0, "GOALRING") {
        return Err(invalid());
    }
    let p = Program::read(stage.dispatch.level, module)?;
    let r: Request = serde_json::from_slice(request)?;
    if !(1..=2048).contains(&r.frames.len()) {
        return Err(Error::invalid("expected 1..2048 goal frames"));
    }
    let init = initialize(
        i32::from(row.placement.rotation[0] as i16),
        stage.dispatch.character,
        r.game_byte_23,
        r.slot_zero_timer,
        r.range_out,
        r.allocation_ok,
    );
    if r.child_dispatch.is_some() && (!matches!(init, InitRoute::ChildTask) || r.child.is_none()) {
        return Err(Error::invalid(
            "child dispatch requires child placement context",
        ));
    }
    if let Some(child) = r.child {
        if !matches!(init, InitRoute::ChildTask) {
            return Err(Error::invalid(
                "child observations require child goal route",
            ));
        }
        let linked = child.linked_after;
        let writes = p.child_placement(child, row.placement.position)?;
        let dispatch = if let Some(d) = r.child_dispatch {
            if !linked {
                return Err(Error::invalid("cannot dispatch an absent linked child"));
            }
            let mut report = p.child_dispatch(d.mode, d.byte1, d.range_out)?;
            if let Some(c) = d.contact {
                if d.mode != 2 || d.range_out == Some(true) {
                    return Err(Error::invalid("contact requires an active mode-2 handler"));
                }
                report["contact"] = p.child_contact(c)?;
            }
            if let Some(input) = d.mode_one {
                if d.mode != 1 || d.range_out == Some(true) {
                    return Err(Error::invalid(
                        "mode-1 observations require an active mode-1 handler",
                    ));
                }
                report["handlerFrames"] =
                    p.child_mode_one(input.work36, input.work40, &input.game_bytes_55)?;
            }
            Some(report)
        } else {
            None
        };
        return Ok(
            json!({"program":p,"init":init,"child":writes,"childDispatch":dispatch,"dispatchedFrames":0,"scope":"Child placement and optional dispatch including mode 3 and 6 handler writes; engine calls are observations."}),
        );
    }
    let InitRoute::Contact { angle_x } = init else {
        return Ok(
            json!({"program":p,"init":init,"dispatchedFrames":0,"scope":"Fresh native goal task did not enter the contact executor."}),
        );
    };
    let radius = p.radii[usize::from(angle_x % 3 == 1)];
    let center = row.placement.position;
    let probe = if let Some(probe) = r.probe {
        if !probe.radius.is_finite()
            || probe.radius < 0.
            || !probe.center.iter().all(|v| v.is_finite())
        {
            return Err(Error::invalid("invalid probe sphere"));
        }
        let d = (probe.center[0] - f64::from(center[0]))
            .hypot(probe.center[1] - f64::from(center[1]))
            .hypot(probe.center[2] - f64::from(center[2]));
        let gap = d - probe.radius - f64::from(radius);
        if !gap.is_finite() {
            return Err(Error::invalid("unrepresentable sphere separation"));
        }
        Some(json!({"signedSeparation":gap,"intersectsOrTouches":gap<=0.}))
    } else {
        None
    };
    let mut state = Contact::default();
    if r.lifecycle.is_some() && angle_x % 3 != 0 {
        return Err(Error::invalid("lifecycle requires a regular goal ring"));
    }
    let mut motion = RegularMotion {
        yaw: i32::from(row.placement.rotation[1] as i16),
        y: center[1],
        timer: r.lifecycle.as_ref().map_or(0, |v| v.initial_timer),
    };
    let mut slot_words = r.initial_slot_words.unwrap_or(p.initial_slot_words);
    let mut frames = vec![];
    for hit in r.frames {
        let was_latched = state.latched;
        if let Some(input) = &r.lifecycle {
            motion.tick(was_latched, input.game_byte_37, p.spin, p.rise);
        }
        let calls = state.step(angle_x, r.game_byte_23, hit, p.local_effect)?;
        let mut frame = json!({"calls":calls,"latched":state.latched,"resetScaleY":!was_latched && state.latched});
        apply_slot_writes(&mut slot_words, &calls, p.local_effect);
        frame["moduleSlotWords"] = json!(slot_words);
        let cleanup = r
            .lifecycle
            .as_ref()
            .and_then(|v| motion.cleanup_call(state.latched, v.task_field_30_present));
        if r.lifecycle.is_some() {
            frame["regularMotion"] = json!(motion);
            frame["cleanupCall"] = json!(cleanup);
        }
        frames.push(frame);
        if cleanup.is_some() {
            break;
        }
    }
    let consumer = if let Some(c) = r.post_contact_consumer {
        let code = p.consume_goal_word(&mut slot_words, c.slot, c.query)?;
        let mut out =
            json!({"localCallArgument":code,"moduleSlotWords":slot_words,"work52":120,"mode":0});
        if let Some(m) = c.message {
            out["messageCall"] = p.message_call(
                code.ok_or_else(|| Error::invalid("no message selected"))?,
                m,
            )?;
        }
        Some(out)
    } else {
        None
    };
    Ok(
        json!({"postContactConsumer":consumer,"program":p,"setAngleX":row.placement.rotation[0] as i16,"effectiveAngleX":angle_x,"placement":row.placement_index,"center":center,"radius":radius,"placementSphereProbe":probe,"frames":frames,"scope":"Contact branch after native initialization. Placement-space geometry is diagnostic. Contacts are supplied; effective angle follows fresh SET initialization; no mission clear or save is committed."}),
    )
}
