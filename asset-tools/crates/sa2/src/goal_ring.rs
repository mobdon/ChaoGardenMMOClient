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
    pub initial_slot_words: [u32; 2],
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
            slot_words_offset,
            initial_slot_words,
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
struct Request {
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
    Ok(
        json!({"program":p,"setAngleX":row.placement.rotation[0] as i16,"effectiveAngleX":angle_x,"placement":row.placement_index,"center":center,"radius":radius,"placementSphereProbe":probe,"frames":frames,"scope":"Contact branch after native initialization. Placement-space geometry is diagnostic. Contacts are supplied; effective angle follows fresh SET initialization; no mission clear or save is committed."}),
    )
}
