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
    pub radii: [f32; 2],
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
        Ok(Self {
            level,
            module: info.module,
            object_init: init,
            contact_offset: start,
            local_effect: target,
            radii,
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
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    slot_zero_timer: Option<u16>,
    range_out: bool,
    allocation_ok: bool,
    game_byte_23: i8,
    frames: Vec<Hit>,
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
    let mut frames = vec![];
    for hit in r.frames {
        let was_latched = state.latched;
        let calls = state.step(angle_x, r.game_byte_23, hit, p.local_effect)?;
        frames.push(json!({"calls":calls,"latched":state.latched,"resetScaleY":!was_latched && state.latched}));
    }
    Ok(
        json!({"program":p,"setAngleX":row.placement.rotation[0] as i16,"effectiveAngleX":angle_x,"placement":row.placement_index,"center":center,"radius":radius,"placementSphereProbe":probe,"frames":frames,"scope":"Contact branch after native initialization. Placement-space geometry is diagnostic. Contacts are supplied; effective angle follows fresh SET initialization; no mission clear or save is committed."}),
    )
}
