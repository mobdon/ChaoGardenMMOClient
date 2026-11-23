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
    #[serde(skip)]
    consumer_scale: [f32; 2],
    #[serde(skip)]
    consumer_blend: [f32; 2],
    #[serde(skip)]
    vector_constants: [f32; 5],
    #[serde(skip)]
    gate_constants: [f32; 4],
    #[serde(skip)]
    spawn_constants: [f32; 6],
    spawn_helper: u32,
    #[serde(skip)]
    record_constants: [f32; 5],
    #[serde(skip)]
    record_pool: u32,
    #[serde(skip)]
    effect_textures: Vec<String>,
    #[serde(skip)]
    effect_update: (u32, [f32; 6]),
    pub fallback_message_offset: u32,
    pub initial_slot_words: [u32; 2],
    #[serde(skip)]
    child_callbacks: [u32; 3],
    #[serde(skip)]
    child_handlers: [u32; 5],
    #[serde(skip)]
    child_mode_one: [f32; 4],
    #[serde(skip)]
    child_start_data: [u32; 3],
    #[serde(skip)]
    shape_hashes: [(String, String); 2],
    #[serde(skip)]
    child_shapes: [[crate::collision_record::CollisionRecord; 2]; 2],
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
    #[serde(skip)]
    regular_shapes: [crate::collision_record::CollisionRecord; 2],
    #[serde(skip)]
    regular_shape_offset: u32,
    #[serde(skip)]
    regular_callbacks: [u32; 5],
    #[serde(skip)]
    regular_work40: u32,
    #[serde(skip)]
    texture_lod_bits: [u32; 2],
    pub radii: [f32; 2],
    pub spin: [i32; 2],
    alternate_steps: [f32; 2],
    alternate_lengths: [u32; 2],
    shadow_unset: u32,
    shadow_lift: f32,
    update_period: u32,
    update_window: u32,
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
    Position(PositionRoutine),
    Empty,
}
#[derive(Debug)]
struct PositionRoutine {
    entry: u32,
    task_entry: u32,
    label: u32,
    callbacks: [u32; 2],
    hook: u32,
    initial: [u32; 2],
    step: f32,
    display_helper: u32,
    model_draw: PositionModel,
}
#[derive(Debug)]
struct PositionModel {
    vertex_header: u32,
    positions: Vec<[u8; 12]>,
    model: u32,
    texture: u32,
    base: f32,
    bias: f32,
    colors: [u32; 2],
    submit: u32,
    source: u32,
    prepare: u32,
}
impl PositionModel {
    fn trace(&self, radius: f32, ratio: f32, color: Option<u32>) -> Value {
        let scale = (radius / self.base).to_bits();
        let mut result = json!({"model":self.model,"field20During":(self.bias+radius).to_bits(),"calls":[
        ["engine",0x80116ad0u32,self.texture],["engine",0x80115d50u32],["engine",0x80114e30u32,0,scale,scale,scale],
        ["engine",0x80033248u32,self.colors[0],self.colors[1],ratio.to_bits()],
        ["local",self.submit,self.model,"previousReturn"],
        ["local",self.prepare,0x80fffe60u32,self.source,"previousReturn",65535],
        ["engine",0x8011de10u32,self.model,{"modelPointerDuring":0x80fffe60u32}],["engine",0x80115d9cu32]],"modelPointerRestored":self.source,"field20Restored":self.base.to_bits()});
        if let Some(color) = color {
            let count = self.positions.len();
            let header = (((4 * (count as u32) + 1) << 16) | 0x23).to_be_bytes();
            let mut bytes = Vec::with_capacity(12 + count * 16);
            bytes.extend(header);
            bytes.extend(self.vertex_header.to_be_bytes());
            for xyz in &self.positions {
                bytes.extend(xyz);
                bytes.extend(color.to_be_bytes());
            }
            bytes.extend(255u32.to_be_bytes());
            result["preparedStream"] = json!({"packedColor":color,"vertexCount":count,"bytes":bytes.len(),"sha256":manifest::sha256(&bytes)});
        }
        result
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PositionInput {
    pub manager: u32,
    pub created: Option<u32>,
    pub node: Option<u32>,
    pub hook: Option<u32>,
    pub old_head: Option<u32>,
    pub schedule: Option<ManagerSchedule>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PositionNode {
    pub position: Option<[f32; 3]>,
    pub address: u32,
    pub value: f32,
    pub limit: f32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagerSchedule {
    pub display_word: Option<u32>,
    pub display_colors: Option<Vec<u32>>,
    pub query: Option<[f32; 3]>,
    pub previous: Vec<PositionNode>,
    pub updates: u8,
    pub cleanup: Option<[bool; 2]>,
}
impl PositionRoutine {
    fn run(&self, p: &PositionInput, pos: [f32; 3]) -> Result<Value> {
        let bad = || Error::invalid("inconsistent position-list allocation observations");
        if p.created.is_some() != (p.manager == 0) {
            return Err(bad());
        }
        let manager = p.created.unwrap_or(p.manager);
        if p.node.is_some() != (manager != 0) {
            return Err(bad());
        }
        let node = p.node.unwrap_or(0);
        if p.hook.is_some() != (node != 0) || p.old_head.is_some() != (node != 0) {
            return Err(bad());
        }
        let mut calls = Vec::new();
        if p.manager == 0 {
            calls.push(json!([0x80071ba4u32, 2, 3, self.task_entry, self.label]));
        }
        if manager != 0 {
            calls.push(json!([0x801194b4u32, 1, 24]));
        }
        let mut result = json!({"calls":calls,"manager":manager,"callbackWrites":p.created.filter(|v|*v!=0).map(|_|self.callbacks),
            "hookWrite":p.hook.filter(|v|*v==0).map(|_|self.hook),"node":node,
            "insert":p.old_head.map(|old|json!({"positionBits":pos.map(f32::to_bits),"work12Bits":self.initial[0],"work16Bits":self.initial[1],"next":old,"newHead":node}))});
        if let Some(schedule) = &p.schedule {
            if manager == 0
                || schedule.previous.len() > 1024
                || (node != 0
                    && p.old_head != Some(schedule.previous.first().map_or(0, |n| n.address)))
            {
                return Err(bad());
            }
            let mut seen = std::collections::HashSet::new();
            let mut list = Vec::new();
            if node != 0 {
                seen.insert(node);
                list.push((
                    node,
                    f32::from_bits(self.initial[0]),
                    f32::from_bits(self.initial[1]),
                    Some(pos),
                ));
            }
            for n in &schedule.previous {
                if n.address == 0 || !seen.insert(n.address) {
                    return Err(bad());
                }
                if (schedule.query.is_some() || schedule.display_word.is_some())
                    && n.position.is_none()
                {
                    return Err(bad());
                }
                list.push((n.address, n.value, n.limit, n.position));
            }
            let mut frames = Vec::new();
            if schedule.display_colors.is_some() && schedule.display_word.is_none() {
                return Err(bad());
            }
            let mut colors = schedule.display_colors.as_ref().map(|c| c.iter());
            let mut display_records = 0;
            for _ in 0..schedule.updates {
                let mut freed = Vec::new();
                list.retain_mut(|(addr, value, limit, _)| {
                    *value += self.step;
                    if *value > *limit {
                        freed.push(json!([addr, limit.to_bits()]));
                        false
                    } else {
                        true
                    }
                });
                frames.push(json!({"released":freed,"remaining":list.iter().map(|(a,v,_,_)|[ *a,v.to_bits() ]).collect::<Vec<_>>(),"removeManager":list.is_empty()}));
                if let Some(point) = schedule.query {
                    let mut visits = Vec::new();
                    let mut hit = false;
                    for &(addr, radius, _, center) in &list {
                        let distance = crate::board::point_distance(center.ok_or_else(bad)?, point);
                        visits.push(json!([addr, distance.to_bits()]));
                        if distance < radius {
                            hit = true;
                            break;
                        }
                    }
                    frames.last_mut().unwrap()["rangeQuery"] = json!({"visited":visits,"hit":hit});
                }
                if let Some(word) = schedule.display_word {
                    display_records += list.len();
                    if display_records > 8192 {
                        return Err(Error::invalid(
                            "position display trace exceeds 8192 records",
                        ));
                    }
                    let mut display = Vec::new();
                    for &(addr, radius, limit, center) in &list {
                        display.push(json!(["engine", 0x80115d50u32]));
                        display.push(json!([
                            "engine",
                            0x80115e24u32,
                            addr,
                            center.ok_or_else(bad)?.map(f32::to_bits)
                        ]));
                        display.push(json!([
                            "engine",
                            0x801154e0u32,
                            0,
                            word.wrapping_mul(0x120)
                        ]));
                        display.push(json!([
                            "local",
                            self.display_helper,
                            radius.to_bits(),
                            (radius / limit).to_bits()
                        ]));
                        display.push(json!([
                            "modelDraw",
                            self.model_draw.trace(
                                radius,
                                radius / limit,
                                colors
                                    .as_mut()
                                    .map(|it| it.next().copied().ok_or_else(bad))
                                    .transpose()?
                            )
                        ]));
                        display.push(json!(["engine", 0x80115d9cu32]));
                    }
                    frames.last_mut().unwrap()["display"] = json!(display);
                }
                if list.is_empty() {
                    break;
                }
            }
            if colors.is_some_and(|mut c| c.next().is_some()) {
                return Err(bad());
            }
            let cleanup=schedule.cleanup.map(|[owns_manager,owns_hook]|json!({"released":list.iter().map(|n|n.0).collect::<Vec<_>>(),"clearManager":owns_manager,"clearHook":owns_hook}));
            result["schedule"] = json!({"frames":frames,"cleanup":cleanup});
        }
        Ok(result)
    }
}
#[derive(Debug)]
struct ActionServices {
    first: u16,
    second: Option<u16>,
    scalar: u32,
}
impl Program {
    /// Pre-contact animation and sound branch. The caller resets work +0x24
    /// when the subsequent contact check latches for the first time.
    pub fn alternate_motion(
        &self,
        angle: i32,
        latched: bool,
        gate: i8,
        timer: u16,
        phase: &mut f32,
    ) -> Result<Option<(u16, i32, bool)>> {
        if angle % 3 == 0 {
            return Err(Error::invalid(
                "alternate motion requires a nonregular goal",
            ));
        }
        if gate != 0 {
            return Ok(None);
        }
        let doubled = 2 * i32::from(timer);
        if angle % 3 == 1 {
            let i = usize::from(latched);
            *phase += self.alternate_steps[i];
            if *phase > (self.alternate_lengths[i] - 1) as f32 {
                *phase = 0.;
            }
            Ok(Some(if latched {
                (0x100e, 30, true)
            } else {
                (0x100d, 30 - doubled, true)
            }))
        } else {
            Ok(Some((0x1012, i32::from((-doubled) as i16), false)))
        }
    }
    /// None: the range helper ended this invocation before the byte-3 store.
    pub fn update_entry(
        &self,
        latched: bool,
        mode: u8,
        range_out: Option<bool>,
        tick: u32,
        game_byte: i8,
    ) -> Result<Option<u8>> {
        if (!latched && mode == 0) != range_out.is_some() {
            return Err(Error::invalid(
                "range return required only for an unlatched mode-zero goal",
            ));
        }
        Ok(if range_out == Some(true) {
            None
        } else if tick % self.update_period >= self.update_window {
            Some(0)
        } else if matches!(game_byte, 1 | 2) {
            Some(2)
        } else {
            Some(1)
        })
    }
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
        let tail = switch + 0x2ec;
        if manifest::sha256(
            instructions
                .get(tail as usize..tail as usize + 0x68)
                .ok_or_else(invalid)?,
        ) != "e3b60013ab7a1eba2322458dd8f9256f7334dc88244578ab314780bb2c3e5fea"
        {
            return Err(invalid());
        }
        for (off, kind, addr) in [
            (2, 6, 0x801cbd88),
            (6, 4, 0x801cbd88),
            (0x10, 10, 0x80112050),
        ] {
            let r = rel.reference(1, tail + off).ok_or_else(invalid)?;
            if (r.module, r.kind, r.addend) != (0, kind, addr) {
                return Err(invalid());
            }
        }
        let scalar = |off| -> Result<f32> {
            let hi = rel.reference(1, tail + off).ok_or_else(invalid)?;
            let lo = rel.reference(1, tail + off + 4).ok_or_else(invalid)?;
            if (hi.module, hi.section, hi.kind) != (info.module, 4, 6)
                || (lo.module, lo.section, lo.kind, lo.addend) != (info.module, 4, 4, hi.addend)
            {
                return Err(invalid());
            }
            Ok(f32::from_bits(word(rel.data(4)?, lo.addend as usize)?))
        };
        let consumer_scale = [scalar(0x16)?, scalar(0x26)?];
        let site = tail + 0x64;
        let b = word(instructions, site as usize)?;
        if b & 0xfc000003 != 0x48000000 {
            return Err(invalid());
        }
        let common = site
            .checked_add_signed(((b & 0x03fffffc) << 6) as i32 >> 6)
            .ok_or_else(invalid)?;
        if manifest::sha256(
            instructions
                .get(common as usize..common as usize + 0xa0)
                .ok_or_else(invalid)?,
        ) != "a93edd56db744762b599737dba77ef42ec85ac64b2487c1ca7d8605d04e9959d"
        {
            return Err(invalid());
        }
        let delta = common.checked_sub(tail).ok_or_else(invalid)?;
        let consumer_blend = [scalar(delta + 0x62)?, scalar(delta + 0x8a)?];
        let local_target = |site: u32| -> Result<u32> {
            let b = word(instructions, site as usize)?;
            if b & 0xfc000003 != 0x48000001 {
                return Err(invalid());
            }
            site.checked_add_signed(((b & 0x03fffffc) << 6) as i32 >> 6)
                .ok_or_else(invalid)
        };
        let helper = local_target(local_target(common + 0xa0)? + 0x34)?;
        if manifest::sha256(
            instructions
                .get(helper as usize..helper as usize + 0x29c)
                .ok_or_else(invalid)?,
        ) != "2e4c0c2e4c2108442a11724f6dc643504cdf300c52d228db40526fd7825d5a7b"
        {
            return Err(invalid());
        }
        let constant = |off| -> Result<f32> {
            let h = rel.reference(1, helper + off).ok_or_else(invalid)?;
            let l = rel.reference(1, helper + off + 4).ok_or_else(invalid)?;
            if (h.module, h.kind, h.section) != (info.module, 6, 4)
                || (l.module, l.kind, l.section, l.addend) != (info.module, 4, 4, h.addend)
            {
                return Err(invalid());
            }
            Ok(f32::from_bits(word(rel.data(4)?, h.addend as usize)?))
        };
        for (off, kind, addr) in [
            (0x34, 10, 0x80037da0),
            (0x50, 10, 0x80037da0),
            (0x1f2, 6, 0x801e7368),
            (0x1f6, 4, 0x801e7368),
        ] {
            let r = rel.reference(1, helper + off).ok_or_else(invalid)?;
            if (r.module, r.kind, r.addend) != (0, kind, addr) {
                return Err(invalid());
            }
        }
        let vector_constants = [
            constant(0xc2)?,
            constant(0x102)?,
            constant(0x146)?,
            constant(0x20a)?,
            constant(0x24a)?,
        ];
        let gateway = local_target(common + 0xa0)? + 0x48;
        let branch = word(instructions, gateway as usize)?;
        if branch & 0xffff0003 != 0x41820000 {
            return Err(invalid());
        }
        let gate = gateway
            .checked_add_signed((branch as u16 as i16) as i32)
            .ok_or_else(invalid)?;
        if manifest::sha256(
            instructions
                .get(gate as usize..gate as usize + 0xa0)
                .ok_or_else(invalid)?,
        ) != "7e4c6b45c1436bf3590e47f72b50af3a0073119136b6e179ce97a874d2d56bb1"
        {
            return Err(invalid());
        }
        for (off, addr) in [(0x1c, 0x8010d940), (0x6c, 0x80114eac)] {
            let r = rel.reference(1, gate + off).ok_or_else(invalid)?;
            if (r.module, r.kind, r.addend) != (0, 10, addr) {
                return Err(invalid());
            }
        }
        let g = gate.checked_sub(helper).ok_or_else(invalid)?;
        let bias = rel.reference(1, gate + 0x26).ok_or_else(invalid)?.addend;
        if constant(g + 0x22)?.to_bits() != 0x43300000
            || word(rel.data(4)?, bias as usize + 4)? != 0x80000000
        {
            return Err(invalid());
        }
        let gate_constants = [
            constant(g + 0x46)?,
            constant(g + 0x56)?,
            constant(g + 0x72)?,
            constant(g + 0x8a)?,
        ];
        let body = gate + 0xa0;
        let mut code = instructions
            .get(body as usize..body as usize + 0x2f0)
            .ok_or_else(invalid)?
            .to_vec();
        let spawn_helper = local_target(body + 0x1c8)?;
        code[0x1c8..0x1cc].fill(0);
        if manifest::sha256(&code)
            != "3e6875c1fe0ccf104460a87910c1223c4d2ba222217faa106c789d0135b9b1f2"
        {
            return Err(invalid());
        }
        for off in [0x18, 0x80, 0xd4, 0x13c, 0x188, 0x1e0, 0x228, 0x288] {
            let r = rel.reference(1, body + off).ok_or_else(invalid)?;
            if (r.module, r.kind, r.addend) != (0, 10, 0x8010d940) {
                return Err(invalid());
            }
        }
        let k = |off| constant(g + 0xa0 + off);
        let spawn_constants = [k(0x46)?, k(0x56)?, k(0x66)?, k(0xb6)?, k(0x112)?, k(0x122)?];
        for off in [0x1e, 0x86, 0xda, 0x142, 0x18e, 0x1e6, 0x22e, 0x28e] {
            if k(off)?.to_bits() != 0x43300000
                || rel.reference(1, body + off + 4).ok_or_else(invalid)?.addend != bias
            {
                return Err(invalid());
            }
        }
        for off in [0x252, 0x2b2] {
            if k(off)? != gate_constants[0] {
                return Err(invalid());
            }
        }
        for off in [0x262, 0x2c2] {
            if k(off)? != spawn_constants[0] {
                return Err(invalid());
            }
        }
        if manifest::sha256(
            instructions
                .get(spawn_helper as usize..spawn_helper as usize + 0x174)
                .ok_or_else(invalid)?,
        ) != "9f13765c1a6f6c64789609fd9b17df55278cace3ec6be04dc82fdea64deafc25"
        {
            return Err(invalid());
        }
        let record_ref = |off, section| -> Result<u32> {
            let h = rel.reference(1, spawn_helper + off).ok_or_else(invalid)?;
            let l = rel
                .reference(1, spawn_helper + off + 4)
                .ok_or_else(invalid)?;
            if (h.module, h.kind, h.section) != (info.module, 6, section)
                || (l.module, l.kind, l.section, l.addend) != (info.module, 4, section, h.addend)
            {
                return Err(invalid());
            }
            Ok(h.addend)
        };
        for (off, addr) in [(0x34, 0x80032910), (0x78, 0x8010d940), (0xf0, 0x8010d940)] {
            let r = rel.reference(1, spawn_helper + off).ok_or_else(invalid)?;
            if (r.module, r.kind, r.addend) != (0, 10, addr) {
                return Err(invalid());
            }
        }
        let record_pool = record_ref(0x2e, 5)?;
        for off in [0x7e, 0xf6] {
            let at = record_ref(off, 4)? as usize;
            if word(rel.data(4)?, at)? != 0x43300000 || word(rel.data(4)?, at + 4)? != 0x80000000 {
                return Err(invalid());
            }
        }
        let scalar = |off| -> Result<f32> {
            Ok(f32::from_bits(word(
                rel.data(4)?,
                record_ref(off, 4)? as usize,
            )?))
        };
        let record_constants = [
            scalar(0xa2)?,
            scalar(0xb2)?,
            scalar(0xc2)?,
            scalar(0xd2)?,
            scalar(0x11e)?,
        ];
        let (1, update) = rel
            .local_pointer(5, record_pool + 32)?
            .ok_or_else(invalid)?
        else {
            return Err(invalid());
        };
        if manifest::sha256(
            instructions
                .get(update as usize..update as usize + 0x198)
                .ok_or_else(invalid)?,
        ) != "9fd6ff66108eb265b3c2381ed3e4c2338c99f71a1bc586bd6093549250f78648"
        {
            return Err(invalid());
        }
        let call = rel.reference(1, update + 0xb8).ok_or_else(invalid)?;
        if (call.module, call.kind, call.addend) != (0, 10, 0x80112050) {
            return Err(invalid());
        }
        let uref = |off| -> Result<usize> {
            let h = rel.reference(1, update + off).ok_or_else(invalid)?;
            let l = rel.reference(1, update + off + 4).ok_or_else(invalid)?;
            if (h.module, h.section, h.kind) != (info.module, 4, 6)
                || (l.module, l.section, l.kind, l.addend) != (info.module, 4, 4, h.addend)
            {
                return Err(invalid());
            }
            Ok(h.addend as usize)
        };
        let floats = rel.data(4)?;
        let bias = uref(0x122)?;
        if word(floats, bias)? != 0x43300000 || word(floats, bias + 4)? != 0 {
            return Err(invalid());
        }
        let pooldata = rel.data(5)?;
        let ptr = |at| -> Result<u32> {
            match rel.local_pointer(5, at)? {
                Some((5, p)) => Ok(p),
                _ => Err(invalid()),
            }
        };
        let list = ptr(record_pool + 4)?;
        let entries = ptr(list)?;
        let count = word(pooldata, list as usize + 4)?;
        if count != word(pooldata, record_pool as usize + 12)? || !(1..=256).contains(&count) {
            return Err(invalid());
        }
        let mut effect_textures = Vec::new();
        for i in 0..count {
            let off = ptr(entries + i * 12)? as usize;
            let tail = pooldata.get(off..).ok_or_else(invalid)?;
            let end = tail
                .iter()
                .take(64)
                .position(|&c| c == 0)
                .ok_or_else(invalid)?;
            if end == 0 {
                return Err(invalid());
            }
            effect_textures.push(
                std::str::from_utf8(&tail[..end])
                    .map_err(|_| invalid())?
                    .to_owned(),
            );
        }
        let poolword = |off: usize| word(pooldata, record_pool as usize + off);
        let effect_update = (
            poolword(12)?,
            [
                f32::from_bits(poolword(16)?),
                f32::from_bits(poolword(20)?),
                f32::from_bits(poolword(24)?),
                f32::from_bits(poolword(28)?),
                f32::from_bits(word(floats, uref(0xbe)?)?),
                f32::from_bits(word(floats, uref(0xce)?)?),
            ],
        );
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
                        let entry = site
                            .checked_add_signed(((ins & 0x03fffffc) << 6) as i32 >> 6)
                            .ok_or_else(invalid)?;
                        let data = |off, section| {
                            rel.reference(1, entry + off)
                                .filter(|r| {
                                    r.module == info.module && r.kind == 4 && r.section == section
                                })
                                .map(|r| r.addend)
                                .ok_or_else(invalid)
                        };
                        for (offset, target) in [(0x4c, 0x80071ba4), (0x98, 0x801194b4)] {
                            if !rel.reference(1, entry + offset).is_some_and(|r| {
                                r.module == 0 && r.kind == 10 && r.addend == target
                            }) {
                                return Err(invalid());
                            }
                        }
                        let constant = |off| word(rel.data(4)?, data(off, 4)? as usize);
                        let draw = data(0x66, 1)?;
                        for (off, target) in [
                            (0x3c, 0x80115d50),
                            (0x44, 0x80115e24),
                            (0x54, 0x801154e0),
                            (0x64, 0x80115d9c),
                        ] {
                            if !rel.reference(1, draw + off).is_some_and(|r| {
                                r.module == 0 && r.kind == 10 && r.addend == target
                            }) {
                                return Err(invalid());
                            }
                        }
                        let site = draw + 0x60;
                        let ins = word(instructions, site as usize)?;
                        if ins & 0xfc000003 != 0x48000001 {
                            return Err(invalid());
                        }
                        let display_helper = site
                            .checked_add_signed(((ins & 0x03fffffc) << 6) as i32 >> 6)
                            .ok_or_else(invalid)?;
                        let h = display_helper;
                        let field = |off, section| {
                            rel.reference(1, h + off)
                                .filter(|r| {
                                    r.module == info.module && r.section == section && r.kind == 4
                                })
                                .map(|r| r.addend)
                                .ok_or_else(invalid)
                        };
                        for (off, dest) in [
                            (0x54, 0x80116ad0),
                            (0x58, 0x80115d50),
                            (0x6c, 0x80114e30),
                            (0x8c, 0x80033248),
                            (0x9c, 0x80115d9c),
                        ] {
                            if !rel
                                .reference(1, h + off)
                                .is_some_and(|r| r.module == 0 && r.kind == 10 && r.addend == dest)
                            {
                                return Err(invalid());
                            }
                        }
                        let model = field(0x2a, 5)?;
                        let site = h + 0x98;
                        let ins = word(instructions, site as usize)?;
                        if ins & 0xfc000003 != 0x48000001 {
                            return Err(invalid());
                        }
                        let submit = site
                            .checked_add_signed(((ins & 0x03fffffc) << 6) as i32 >> 6)
                            .ok_or_else(invalid)?;
                        let sub = instructions
                            .get(submit as usize..submit as usize + 0x6c)
                            .ok_or_else(invalid)?;
                        let mut signature = sub.to_vec();
                        let branch = word(sub, 0x3c)?;
                        if branch & 0xfc000003 != 0x48000001 {
                            return Err(invalid());
                        }
                        signature[0x3c..0x40].copy_from_slice(&0x48000001u32.to_be_bytes());
                        if manifest::sha256(&signature)
                            != "5efdaa2e2578ff6ed7a5847f8147275ae761b99f85a685fa708d51f19f3ce012"
                            || !rel.reference(1, submit + 0x48).is_some_and(|r| {
                                r.module == 0 && r.kind == 10 && r.addend == 0x8011de10
                            })
                        {
                            return Err(invalid());
                        }
                        let prepare = (submit + 0x3c)
                            .checked_add_signed(((branch & 0x03fffffc) << 6) as i32 >> 6)
                            .ok_or_else(invalid)?;
                        let (5, source) = rel.local_pointer(5, model)?.ok_or_else(invalid)? else {
                            return Err(invalid());
                        };
                        if manifest::sha256(
                            instructions
                                .get(prepare as usize..prepare as usize + 0x1bc)
                                .ok_or_else(invalid)?,
                        ) != "d3b6c552a802b74e949aefe4904a4b06d6ad653e535ffb52064730c0b405b766"
                        {
                            return Err(invalid());
                        }
                        let vertices = rel.data(5)?;
                        let start = source as usize;
                        let vertex_header = word(vertices, start + 4)?;
                        let count = (vertex_header >> 16) as usize;
                        let end = start + 8 + count * 24;
                        if word(vertices, end)? != 255 {
                            return Err(invalid());
                        }
                        let positions = vertices[start + 8..end]
                            .as_chunks::<24>()
                            .0
                            .iter()
                            .map(|chunk| chunk[..12].try_into().unwrap())
                            .collect();
                        let model_draw = PositionModel {
                            positions,
                            vertex_header,
                            model,
                            texture: field(0x52, 5)?,
                            base: f32::from_bits(word(rel.data(5)?, model as usize + 20)?),
                            bias: f32::from_bits(word(rel.data(4)?, field(0x3a, 4)? as usize)?),
                            colors: [
                                word(rel.data(5)?, field(0x76, 5)? as usize)?,
                                word(rel.data(5)?, field(0x82, 5)? as usize)?,
                            ],
                            submit,
                            prepare,
                            source,
                        };
                        let tick = data(0x42, 1)?;
                        let hook = data(0xc2, 1)?;
                        if manifest::sha256(
                            instructions
                                .get(hook as usize..hook as usize + 0x8c)
                                .ok_or_else(invalid)?,
                        ) != "f5f9cd8d2ee5822a0cda8f90c980b8e2eb7eb917852b4c32722d9a8875f89585"
                            || !rel.reference(1, hook + 0x44).is_some_and(|r| {
                                r.module == 0 && r.kind == 10 && r.addend == 0x8011b314
                            })
                        {
                            return Err(invalid());
                        }
                        let cleanup = data(0x72, 1)?;
                        for (site, target) in [
                            (tick + 0x80, 0x801194e0),
                            (tick + 0xac, 0x80071888),
                            (cleanup + 0x28, 0x801194e0),
                        ] {
                            if !rel.reference(1, site).is_some_and(|r| {
                                r.kind == 10 && r.module == 0 && r.addend == target
                            }) {
                                return Err(invalid());
                            }
                        }
                        let step = rel
                            .reference(1, tick + 0x32)
                            .filter(|r| r.kind == 4 && r.module == info.module && r.section == 4)
                            .ok_or_else(invalid)?;
                        let step = f32::from_bits(word(rel.data(4)?, step.addend as usize)?);
                        ActionRoutine::Position(PositionRoutine {
                            entry,
                            task_entry: tick,
                            display_helper,
                            model_draw,
                            step,
                            label: data(0x4a, 5)?,
                            callbacks: [data(0x66, 1)?, data(0x72, 1)?],
                            hook: data(0xc2, 1)?,
                            initial: [constant(0xf2)?, constant(0x10a)?],
                        })
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
        let zero = child_handlers[0];
        if manifest::sha256(
            instructions
                .get(zero as usize..zero as usize + 0x200)
                .ok_or_else(invalid)?,
        ) != "7a15b3e684e484e0e8b943be392e32f4ff438f4b1669966f0fb7e43cc004c930"
        {
            return Err(invalid());
        }
        let reference = |off, section| {
            rel.reference(1, zero + off)
                .filter(|r| r.module == info.module && r.section == section && r.kind == 4)
                .map(|r| r.addend)
                .ok_or_else(invalid)
        };
        if reference(0xee, 5)? != table
            || reference(0x1ca, 1)? != child_handlers[4]
            || reference(0x1d6, 1)? != child_callbacks[0]
            || reference(0x1e2, 1)? != child_callbacks[1]
        {
            return Err(invalid());
        }
        let child_start_data = [
            word(rel.data(4)?, reference(0x2a, 4)? as usize)?,
            reference(0x18a, 5)?,
            reference(0x1ae, 5)?,
        ];
        let shape_hash = |off: u32| -> Result<_> {
            let src = data
                .get(off as usize..)
                .and_then(|tail| tail.get(..96))
                .ok_or_else(invalid)?;
            let mut little = src.to_vec();
            for record in little.as_chunks_mut::<48>().0 {
                for word in record[4..].as_chunks_mut::<4>().0 {
                    word.reverse();
                }
            }
            Ok((manifest::sha256(src), manifest::sha256(&little)))
        };
        let shape_hashes = [
            shape_hash(child_start_data[1])?,
            shape_hash(child_start_data[2])?,
        ];
        let shape_pair = |offset: u32| -> Result<_> {
            let second = offset.checked_add(48).ok_or_else(invalid)?;
            Ok([
                crate::collision_record::collision_record(data, offset)?,
                crate::collision_record::collision_record(data, second)?,
            ])
        };
        let child_shapes = [
            shape_pair(child_start_data[1])?,
            shape_pair(child_start_data[2])?,
        ];
        for (off, addr) in [
            (0x88, 0x800651e4),
            (0x98, 0x8006519c),
            (0xc4, 0x800651e4),
            (0xd4, 0x8006519c),
            (0x148, 0x800651e4),
            (0x174, 0x800651e4),
            (0x194, 0x8000f7d8),
            (0x1b8, 0x8000f7d8),
            (0x1c0, 0x80018d28),
        ] {
            if !rel
                .reference(1, zero + off)
                .is_some_and(|r| r.module == 0 && r.kind == 10 && r.addend == addr)
            {
                return Err(invalid());
            }
        }
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
        if r.section != 5 {
            return Err(invalid());
        }
        for (off, kind) in [(0x28e, 4), (0x2aa, 6), (0x2ae, 4)] {
            let other = rel.reference(1, init + off).ok_or_else(invalid)?;
            if (other.module, other.section, other.kind, other.addend)
                != (r.module, r.section, kind, r.addend)
            {
                return Err(invalid());
            }
        }
        for off in [0x29c, 0x2b8] {
            let call = rel.reference(1, init + off).ok_or_else(invalid)?;
            if (call.module, call.kind, call.addend) != (0, 10, 0x8000f8b8) {
                return Err(invalid());
            }
        }
        let paired = |off, section| -> Result<u32> {
            let hi = rel.reference(1, init + off).ok_or_else(invalid)?;
            let lo = rel.reference(1, init + off + 4).ok_or_else(invalid)?;
            if (hi.module, hi.section, hi.kind) != (info.module, section, 6)
                || (lo.module, lo.section, lo.kind, lo.addend)
                    != (info.module, section, 4, hi.addend)
            {
                return Err(invalid());
            }
            Ok(lo.addend)
        };
        let regular_callbacks = [
            paired(0x1fe, 1)?,
            paired(0x1da, 1)?,
            paired(0x1f2, 1)?,
            paired(0x1e6, 1)?,
            paired(0x24a, 1)?,
        ];
        let release = regular_callbacks[2];
        let raw = instructions
            .get(release as usize..release as usize + 0x44)
            .ok_or_else(invalid)?;
        let branch = word(raw, 0x1c)?;
        if branch & 0xfc000003 != 0x48000001 {
            return Err(invalid());
        }
        let helper = (release + 0x1c)
            .checked_add_signed(((branch & 0x03fffffc) << 6) as i32 >> 6)
            .ok_or_else(invalid)?;
        let mut normalized = raw.to_vec();
        normalized[0x1c..0x20].fill(0);
        if manifest::sha256(&normalized)
            != "985f7141046f18f151e467c82158a2168b939b603d1d616647d7a7da49402cf4"
            || manifest::sha256(
                instructions
                    .get(helper as usize..helper as usize + 0x40)
                    .ok_or_else(invalid)?,
            ) != "6c08d3fc09ccc70c20bb904c229f74c6ebdcec7dcceb184b998632aa2df8b8a6"
        {
            return Err(invalid());
        }
        for site in [release + 0x24, helper + 0x20] {
            let call = rel.reference(1, site).ok_or_else(invalid)?;
            if (call.module, call.kind, call.addend) != (0, 10, 0x801194e0) {
                return Err(invalid());
            }
        }
        let site = init + 0x1d4;
        if word(instructions, site as usize - 8)? != 0x3863003c
            || word(instructions, site as usize - 4)? != 0x38800040
        {
            return Err(invalid());
        }
        let b = word(instructions, site as usize)?;
        if b & 0xfc000003 != 0x48000001 {
            return Err(invalid());
        }
        let texture = site
            .checked_add_signed(((b & 0x03fffffc) << 6) as i32 >> 6)
            .ok_or_else(invalid)?;
        if manifest::sha256(
            instructions
                .get(texture as usize..texture as usize + 0xbc)
                .ok_or_else(invalid)?,
        ) != "423e318810239b2fd44a0e67076fdf63e73e2e97e54cb83981f638bf8999314b"
        {
            return Err(invalid());
        }
        for (off, target) in [
            (0x30, 0x80102de0),
            (0x3c, 0x801194b4),
            (0x68, 0x80103004),
            (0xa0, 0x801032c0),
        ] {
            let call = rel.reference(1, texture + off).ok_or_else(invalid)?;
            if (call.module, call.kind, call.addend) != (0, 10, target) {
                return Err(invalid());
            }
        }
        let lod = |off| -> Result<u32> {
            let hi = rel.reference(1, texture + off).ok_or_else(invalid)?;
            let lo = rel.reference(1, texture + off + 4).ok_or_else(invalid)?;
            if (hi.module, hi.section, hi.kind) != (info.module, 4, 6)
                || (lo.module, lo.section, lo.kind, lo.addend) != (info.module, 4, 4, hi.addend)
            {
                return Err(invalid());
            }
            word(rel.data(4)?, lo.addend as usize)
        };
        let texture_lod_bits = [lod(0x7a)?, lod(0x86)?];
        let regular_work40 = word(rel.data(4)?, paired(0x2c6, 4)? as usize)?;
        for off in [0x230, 0x23c] {
            let call = rel.reference(1, init + off).ok_or_else(invalid)?;
            if (call.module, call.kind, call.addend) != (0, 10, 0x8001dcf4) {
                return Err(invalid());
            }
        }
        let regular_shape_offset = r.addend;
        let regular_shapes = shape_pair(regular_shape_offset)?;
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
                0x354,
                0xa8,
                "5f53be3b11c6a6c740dc6ad10951e8e332d7d91425cc658989992ff56e2c6755",
            ),
            (
                0x3fc,
                0x64,
                "f71c40ff683f066ca642043ea644764f38da6e86d8c5ddb926f04972dcd08e34",
            ),
            (
                0x46c,
                0x18c,
                "9445d49a04160d24969efde631ef257a6834dd7fa4d7aa4971669c5aac51895d",
            ),
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
        let section_field = |hi, lo, section, field: usize| -> Result<u32> {
            let a = rel.reference(1, init + hi).ok_or_else(invalid)?;
            let b = rel.reference(1, init + lo).ok_or_else(invalid)?;
            if (
                a.module, a.section, a.kind, b.module, b.section, b.kind, b.addend,
            ) != (info.module, section, 6, info.module, section, 4, a.addend)
            {
                return Err(invalid());
            }
            word(rel.data(section)?, a.addend as usize + field)
        };
        let section_word = |hi, lo, section| section_field(hi, lo, section, 0);
        let constant = |hi, lo| section_word(hi, lo, 5);
        let alternate_steps = [
            f32::from_bits(constant(0x546, 0x54a)?),
            f32::from_bits(constant(0x4c2, 0x4c6)?),
        ];
        let alternate_lengths = [
            section_field(0x55e, 0x562, 5, 4)?,
            section_field(0x4da, 0x4de, 5, 4)?,
        ];
        if alternate_lengths.contains(&0) {
            return Err(invalid());
        }
        let shadow_unset = section_word(0x3fe, 0x402, 4)?;
        if section_word(0x2c6, 0x2ca, 4)? != shadow_unset {
            return Err(invalid());
        }
        let shadow_lift = f32::from_bits(section_word(0x43e, 0x442, 4)?);
        let update_period = constant(0x396, 0x39a)?;
        let update_window = constant(0x3ae, 0x3b2)?;
        let range = rel.reference(1, init + 0x378).ok_or_else(invalid)?;
        if update_period == 0 || (range.module, range.kind, range.addend) != (0, 10, 0x8006538c) {
            return Err(invalid());
        }
        let spin = [
            constant(0x64e, 0x652)? as i32,
            constant(0x60e, 0x612)? as i32,
        ];
        let rise = f32::from_bits(constant(0x632, 0x636)?);
        for (offset, addr) in [
            (0x538, 0x8006ad94),
            (0x5c4, 0x8006ad94),
            (0x5f0, 0x8006ad94),
            (0x458, 0x80067888),
            (0xaf8, 0x80065148),
            (0xb04, 0x80071878),
        ] {
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
            consumer_scale,
            consumer_blend,
            vector_constants,
            gate_constants,
            spawn_constants,
            spawn_helper,
            record_constants,
            effect_update,
            record_pool,
            effect_textures,
            fallback_message_offset,
            slot_words_offset,
            initial_slot_words,
            child_callbacks,
            child_handlers,
            child_mode_one,
            child_start_data,
            shape_hashes,
            child_shapes,
            child_idle_step,
            child_actions,
            action_services,
            child_effect,
            child_helpers,
            child_collision_descriptor,
            child_entry,
            child_parent_callback,
            child_scale_bits,
            regular_callbacks,
            regular_work40,
            texture_lod_bits,
            regular_shapes,
            regular_shape_offset,
            radii,
            spin,
            alternate_steps,
            alternate_lengths,
            shadow_unset,
            shadow_lift,
            update_period,
            update_window,
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
    pub fn consumer_vectors(&self, v: ConsumerVectors) -> Result<Value> {
        if v.player > 1 || (v.point18.is_none() && v.point3.is_some()) {
            return Err(Error::invalid("invalid player vector observations"));
        }
        let [scale, y_scale, reach, present, absent] = self.vector_constants;
        let queries = if v.point18.is_some() {
            vec![18, 3]
        } else {
            vec![18]
        };
        let (position, delta) = if let (Some(a), Some(b)) = (v.point18, v.point3) {
            if v.player_position.is_some() {
                return Err(Error::invalid("unused fallback player position"));
            }
            let mut diff: [f32; 3] = std::array::from_fn(|i| (b[i] - a[i]) * scale);
            diff[1] *= y_scale;
            (std::array::from_fn(|i| diff[i] * reach + b[i]), diff)
        } else if let Some(pos) = v.player_position {
            (pos, [present; 3])
        } else {
            ([absent; 3], [absent; 3])
        };
        Ok(
            json!({"player":v.player,"queries":queries,"positionBits":position.map(f32::to_bits),"deltaBits":delta.map(f32::to_bits)}),
        )
    }
    pub fn effect_texture_receipt(&self, prs: &[u8]) -> Result<Value> {
        let data = crate::archive::decompress_prs(prs, 32 * 1024 * 1024)?;
        let entries = crate::archive::read_gvm(&data)?;
        let mut frames = Vec::new();
        for name in &self.effect_textures {
            let mut candidates = entries.iter().filter(|e| e.name.eq_ignore_ascii_case(name));
            let e = candidates
                .next()
                .ok_or_else(|| Error::invalid("missing native effect frame"))?;
            if candidates.next().is_some() {
                return Err(Error::invalid("ambiguous native effect frame"));
            }
            let img = crate::texture::decode_gvr(e.data, None)?;
            let alpha = img
                .rgba
                .iter()
                .skip(3)
                .step_by(4)
                .copied()
                .collect::<Vec<_>>();
            frames.push(json!({"name":name,"index":e.index,"gvmOffset":e.offset,"globalIndex":e.global_index,"gvrSha256":manifest::sha256(e.data),
                "width":img.width,"height":img.height,"rgbaSha256":manifest::sha256(&img.rgba),"alphaSha256":manifest::sha256(&alpha)}));
        }
        Ok(
            json!({"prsSha256":manifest::sha256(prs),"gvmSha256":manifest::sha256(&data),"frames":frames}),
        )
    }
    pub fn consumer_spawn(&self, s: ConsumerSpawn) -> Result<Value> {
        if s.created != s.color_random.is_some() {
            return Err(Error::invalid(
                "three color draws required only for a created effect",
            ));
        }
        let [mid, spread, height, gain, up, vertical] = self.spawn_constants;
        let norm = |x: i32| (x as f32) * self.gate_constants[0];
        let [a, b, c, d, e] = s.random.map(norm);
        let mut pos = s.position;
        pos[1] += height + spread * (a - mid);
        let [x, y, z] = s.motion;
        let velocity = [
            gain * x + gain * (b - mid),
            vertical * y + (up + gain * (c - mid)),
            gain * z + gain * (d - mid),
        ];
        let color = s.color_random.map(|values| {
            let [g, b, a] = values.map(|r| if norm(r) > mid { 255u8 } else { 224 });
            u32::from_be_bytes([255, g, b, a])
        });
        let scale = gain + gain * e;
        let mut out = json!({"local":self.spawn_helper,"positionBits":pos.map(f32::to_bits),"velocityBits":velocity.map(f32::to_bits),
            "scaleBits":scale.to_bits(),"colorWrite20":color});
        if let Some(r) = s.record {
            if (r.address != 0) != s.created || s.created != r.random.is_some() {
                return Err(Error::invalid("inconsistent effect record observations"));
            }
            let [unit, span, center, angle, half] = self.record_constants;
            let mut frames = None;
            if r.update.is_some() && !s.created {
                return Err(Error::invalid("missing effect record for update"));
            }
            let fields = if let Some([a, b]) = r.random {
                let one = angle * ((a as f32 * unit) * span - center);
                let two = angle * (b as f32 * unit - half);
                if ![one, two]
                    .into_iter()
                    .all(|v| (-2147483648.0..2147483648.0).contains(&v))
                {
                    return Err(Error::invalid("effect angle outside verified range"));
                }
                if let Some(u) = r.update {
                    if u.returns.is_empty() || u.returns.len() > 2048 {
                        return Err(Error::invalid("expected 1..2048 observed effect updates"));
                    }
                    let (limit, [advance, drag, gravity, growth, amplitude, factor]) =
                        self.effect_update;
                    let (mut p, mut v, mut size, mut rotation, mut age, mut phase, mut tint) = (
                        pos,
                        velocity,
                        scale,
                        one as i32,
                        u.frame,
                        0u32,
                        color.ok_or_else(invalid)?,
                    );
                    let mut updates = Vec::new();
                    let mut live = true;
                    for val in u.returns {
                        if !live {
                            return Err(Error::invalid(
                                "update supplied after effect returned zero",
                            ));
                        }
                        rotation = rotation.wrapping_add(two as i32);
                        size += growth;
                        for (pos, speed) in p.iter_mut().zip(&mut v) {
                            *pos += *speed;
                            *speed *= drag;
                        }
                        v[1] += gravity;
                        age += advance;
                        let tilt = factor * (amplitude * val);
                        if ![age, tilt]
                            .into_iter()
                            .all(|x| (-2147483648.0..2147483648.0).contains(&x))
                        {
                            return Err(Error::invalid(
                                "effect update conversion outside verified range",
                            ));
                        }
                        if ((age as i32 as i16) as i32 as u32) >= limit {
                            age = limit.wrapping_sub(1) as f32;
                            tint = (tint & 0xffffff) | ((tint >> 1) & 0xff000000);
                            live = tint >> 24 != 0;
                        }
                        updates.push(json!({"query":phase,"word0":rotation,"scaleBits":size.to_bits(),"positionBits":p.map(f32::to_bits),
                            "velocityBits":v.map(f32::to_bits),"frameBits":age.to_bits(),"word28":(tilt as i32).wrapping_add(0x4000),"color":tint,"keep":live,"word56":phase.wrapping_add(0x222)}));
                        phase = phase.wrapping_add(0x222);
                    }
                    frames = Some(updates);
                }
                Some(
                    json!({"word0":one as i32,"scale4":scale.to_bits(),"position8":pos.map(f32::to_bits),
                    "velocity32":velocity.map(f32::to_bits),"word44":two as i32,"word56":0}),
                )
            } else {
                None
            };
            out["record"] = json!({"request":[0x80032910u32,self.record_pool],"returned":r.address,"writes":fields});
            if let Some(frames) = frames {
                out["record"]["updates"] = json!(frames);
            }
        }
        Ok(out)
    }
    pub fn consumer_gate(&self, byte1: u8, g: ConsumerGate) -> Result<Value> {
        if byte1 & 2 != 0 {
            return Err(Error::invalid("expected cleared countdown bit"));
        }
        let [scale, r_limit, v_limit, w_limit] = self.gate_constants;
        let first = (g.random_return as f32) * scale < r_limit;
        if first != g.vector_return.is_some() {
            return Err(Error::invalid(
                "vector return required only after random gate",
            ));
        }
        let second = g.vector_return.is_some_and(|v| v > v_limit);
        if second != g.work36.is_some() {
            return Err(Error::invalid("work36 required only after vector gate"));
        }
        let mut calls = vec![0x8010d940u32];
        if first {
            calls.push(0x80114eac);
        }
        Ok(
            json!({"flags96":g.flags96 & !0x8000,"byte1":byte1 & !1,"calls":calls,"continueEffect":g.work36.is_some_and(|v|v<w_limit)}),
        )
    }
    pub fn consumer_countdown(&self, timer: i32, c: ConsumerCountdown) -> Value {
        let active = timer != 0;
        let [blend, decay] = self.consumer_blend;
        let next = c.work88 + blend * (c.work92 - c.work88);
        json!({"work52":if active {timer.wrapping_sub(1)} else {timer},
               "flags":if active {c.flags & !4} else {c.flags},
               "linkedFlags":c.linked_flags.map(|f| if active {f & !4} else {f}),
               "byte1":if active {c.byte1 & !3} else {c.byte1},
               "work88Bits":next.to_bits(),"work92Bits":(c.work92*decay).to_bits()})
    }
    pub fn consumer_update(&self, frame: i32, value: f32, counter: i32) -> Result<Value> {
        let scaled = (self.consumer_scale[0] * value) * self.consumer_scale[1];
        if !scaled.is_finite() || !(-2147483648.0..2147483648.0).contains(&scaled) {
            return Err(Error::invalid(
                "consumer return outside verified conversion range",
            ));
        }
        Ok(
            json!({"call":[0x80112050u32,frame.wrapping_mul(0x380)],"work44":scaled as i32,
                  "work112":if counter<1000 {counter+1} else {counter}}),
        )
    }
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
    let mut work = GoalInitWork { angle_x, mode: 0 };
    work.attempt(character, game_byte, slot_zero_timer, range_out, allocated)
}

#[derive(Serialize)]
struct GoalInitWork {
    angle_x: i32,
    mode: u8,
}

impl GoalInitWork {
    fn attempt(
        &mut self,
        character: i8,
        game_byte: i8,
        slot_zero_timer: Option<u16>,
        range_out: bool,
        allocated: bool,
    ) -> InitRoute {
        if matches!(character, 4 | 5) {
            if game_byte == 1 {
                self.angle_x = 2;
                self.mode = 1;
                if !slot_zero_timer.is_some_and(|t| t >= 120) {
                    return InitRoute::Deferred;
                }
            } else if self.angle_x % 3 != 1 {
                return InitRoute::Deferred;
            }
        }
        let x = self.angle_x;
        if game_byte != 2 && x % 3 == 1 {
            return InitRoute::ChildTask;
        }
        if self.mode == 0 && range_out {
            return InitRoute::OutOfRange;
        }
        if allocated {
            InitRoute::Contact { angle_x: x }
        } else {
            InitRoute::NoWork
        }
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
struct ReleaseObservation {
    work56: u32,
    nested: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LifecycleInput {
    release: Option<ReleaseObservation>,
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
pub struct ConsumerCountdown {
    flags: u16,
    linked_flags: Option<u16>,
    byte1: u8,
    work88: f32,
    work92: f32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsumerVectors {
    player: u8,
    point18: Option<[f32; 3]>,
    point3: Option<[f32; 3]>,
    player_position: Option<[f32; 3]>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsumerGate {
    flags96: u32,
    random_return: i32,
    vector_return: Option<f32>,
    work36: Option<f32>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordUpdate {
    frame: f32,
    returns: Vec<f32>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordInput {
    update: Option<RecordUpdate>,
    address: u32,
    random: Option<[i32; 2]>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsumerSpawn {
    record: Option<RecordInput>,
    position: [f32; 3],
    motion: [f32; 3],
    random: [i32; 5],
    created: bool,
    color_random: Option<[i32; 3]>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConsumerUpdate {
    spawn: Option<ConsumerSpawn>,
    gate: Option<ConsumerGate>,
    vectors: Option<ConsumerVectors>,
    countdown: Option<ConsumerCountdown>,
    frame: i32,
    service_return: f32,
    counter: i32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConsumerInput {
    update: Option<ConsumerUpdate>,
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
        if a.position_list.is_some() && !matches!(routine, ActionRoutine::Position(_)) {
            return Err(invalid());
        }
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
            ActionRoutine::Position(r) => {
                calls.push(json!(["local", r.entry, "work+20"]));
                if let Some(p) = &a.position_list {
                    calls.push(json!(["positionList", r.run(p, a.position)?]));
                }
            }
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
                } else if a.indices.is_some()
                    || a.action_links.is_some()
                    || a.position_list.is_some()
                {
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
    /// Compare raw templates only. A content match does not establish PC callback use.
    pub fn pc_shape_templates(&self, exe: &[u8]) -> Result<Value> {
        let pe = crate::pe::read_pe(exe)?;
        let mut pairs = Vec::new();
        for (i, (route, rva)) in [("alternate", 0x74d0c0), ("regular", 0x74d060)]
            .into_iter()
            .enumerate()
        {
            let at = pe.rva_to_offset(rva, 96)?;
            let bytes = exe.get(at..at + 96).ok_or_else(invalid)?;
            let pc_hash = manifest::sha256(bytes);
            let (source, normalized) = &self.shape_hashes[i];
            pairs.push(
                json!({"route":route,"pcRva":rva,"pcFileOffset":at,"pcSha256":pc_hash,
                "relSection":5,"relOffset":self.child_start_data[i+1],"relSha256":source,
                "matchesAfterEndianConversion":&pc_hash == normalized}),
            );
        }
        Ok(json!(pairs))
    }
    /// Big-endian words after the two GX reference calls for the goal's fixed IA4 setup.
    /// This is reference-derived state, not a capture from the installed engine.
    pub fn gx_goal_object_words(&self, image: u32) -> [u32; 8] {
        let [min, max] = self.texture_lod_bits.map(f32::from_bits);
        let lod_byte = |x: f32| (x.clamp(0.0, 10.0) * 16.0) as u32;
        let bias = if min >= 4.0 { 3.99 } else { min.max(-4.0) };
        let bias_bits = (bias * 32.0) as i32 as u8;
        let mode0 = (1 << 4) | (4 << 5) | (1 << 8) | (u32::from(bias_bits) << 9);
        let mode1 = lod_byte(min) | (lod_byte(max) << 8);
        let image0 = 63 | (63 << 10) | (2 << 20);
        [
            mode0,
            mode1,
            image0,
            (image >> 5) & 0x01ff_ffff,
            0,
            2,
            0,
            (128 << 16) | (2 << 8) | 2,
        ]
    }
    pub fn regular_texture_setup(
        &self,
        work56: u32,
        size_return: u32,
        allocation_return: u32,
    ) -> Result<Value> {
        if work56 == 0 {
            return Err(Error::invalid("missing allocated goal work"));
        }
        let obj = work56.wrapping_add(0x6c);
        let [min, max] = self.texture_lod_bits;
        Ok(
            json!({"calls":[[0x80102de0u32,64,64,2,0,0],[0x801194b4u32,1,size_return],
            [0x80103004u32,obj,allocation_return,64,64,2,0,0,0],[0x801032c0u32,obj,1,1,0,0,0]],
            "lodFloatBits":[min,max,min],"work140":64,"work144":allocation_return}),
        )
    }
    pub fn regular_release(&self, work56: u32, nested: u32) -> Result<Value> {
        if work56 == 0 {
            return Err(Error::invalid("goal release dereferences missing work"));
        }
        let mut events = Vec::new();
        if nested != 0 {
            events.push(json!(["free", 0x801194e0u32, nested]));
            events.push(json!(["clearNested", work56.wrapping_add(0x90)]));
        }
        events.push(json!(["free", 0x801194e0u32, work56]));
        events.push(json!(["clearTask56"]));
        Ok(json!({"callback":self.regular_callbacks[2],"events":events}))
    }
    pub fn regular_task_bind(&self, angle: i32, queries: &[i32]) -> Result<Value> {
        let extra = match (angle % 3, queries) {
            (1, []) => false,
            (1, _) => return Err(Error::invalid("unused goal binding query")),
            (_, [57]) => true,
            (_, [first, second]) if *first != 57 => *second == 6,
            _ => return Err(Error::invalid("expected observed goal binding queries")),
        };
        Ok(json!({"callbacks16_20_24_32":&self.regular_callbacks[..4],
            "callback44Write":extra.then_some(self.regular_callbacks[4]),
            "queryReturns":queries,"workByte3":0,"workByte1":0,"work40Bits":self.regular_work40}))
    }
    pub fn regular_collision_setup(&self, effective_angle_x: i32) -> Value {
        let i = usize::from(effective_angle_x % 3 == 1);
        json!({"engine":0x8000f8b8u32,"target":"task","relOffset":self.regular_shape_offset+48*i as u32,
               "count":1,"id":4,"shape":self.regular_shapes[i]})
    }
    pub fn child_start(&self, byte1: i8, input: ChildStart) -> Result<Value> {
        self.child_start_with_mode(byte1, input)
            .map(|(_, report)| report)
    }
    fn child_start_with_mode(&self, byte1: i8, input: ChildStart) -> Result<(i8, Value)> {
        if byte1 == 0 && input.parent_link {
            return Err(Error::invalid(
                "parent link unavailable for standalone child",
            ));
        }
        let mut answers = input.query_returns.into_iter();
        let mut calls = Vec::new();
        let mut query = |addr: u32, target: &str| -> Result<bool> {
            let answer = answers
                .next()
                .ok_or_else(|| Error::invalid("missing child initialization query return"))?;
            calls.push(json!([addr, target]));
            Ok(answer != 0)
        };
        let mut index = (input.work32 as i32).clamp(0, 10) as usize;
        let parent = byte1 != 0 && input.parent_link;
        let special =
            (input.task_link && !query(0x800651e4, "task")? && query(0x8006519c, "task")?)
                || (parent && !query(0x800651e4, "parent")? && query(0x8006519c, "parent")?);
        if special && self.child_actions[index].0 == 10 {
            if let Some(i) = self.child_actions.iter().position(|a| a.0 == 7) {
                index = i;
            }
        }
        let alt = (input.task_link && query(0x800651e4, "task")?)
            || (parent && query(0x800651e4, "parent")?);
        if answers.next().is_some() {
            return Err(Error::invalid("unused child initialization query returns"));
        }
        let [initial, alternate, regular] = self.child_start_data;
        calls.push(json!([
            0x8000f7d8u32,
            "task",
            if alt { alternate } else { regular },
            2,
            if alt { 4 } else { 2 }
        ]));
        calls.push(json!([0x80018d28u32, "task"]));
        let mode = if alt { 4 } else { 1 };
        Ok((
            mode,
            json!({"mode":mode,"task60":index,"work36Bits":initial,"work40Bits":initial,"calls":calls,
            "collisionShapes":self.child_shapes[usize::from(!alt)],"callbacks20_24_32":[self.child_callbacks[0],self.child_handlers[4],self.child_callbacks[1]]}),
        ))
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
    pub position_list: Option<PositionInput>,
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
pub struct ChildStart {
    pub work32: f32,
    pub task_link: bool,
    pub parent_link: bool,
    pub query_returns: Vec<i32>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FirstUpdate {
    range_out: Option<bool>,
    game_byte_55: Option<i8>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChildDispatch {
    first_update: Option<FirstUpdate>,
    start: Option<ChildStart>,
    contact: Option<ChildContact>,
    mode_one: Option<ModeOneInput>,
    mode: i8,
    byte1: i8,
    range_out: Option<bool>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TextureSetup {
    work56: u32,
    size_return: Option<u32>,
    allocation_return: u32,
}
/// Old recordings supply just the hit. A sampled frame can also supply
/// the live global bytes read by the native update and contact branches.
#[derive(Deserialize)]
#[serde(untagged)]
enum GoalFrame {
    Sample(GoalSample),
    Hit(Hit),
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateEntry {
    shadow_return_bits: Option<u32>,
    frame_counter: u32,
    range_out: Option<bool>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GoalSample {
    alternate_timer: Option<u16>,
    entry: Option<UpdateEntry>,
    hit: Hit,
    game_byte_23: i8,
    game_byte_37: Option<i8>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InitAttempt {
    game_byte_23: i8,
    slot_zero_timer: Option<u16>,
    range_out: bool,
    allocation_ok: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    #[serde(default)]
    track_shadow: bool,
    #[serde(default)]
    init_retries: Vec<InitAttempt>,
    texture_setup: Option<TextureSetup>,
    bind_query_returns: Option<Vec<i32>>,
    child_dispatch: Option<ChildDispatch>,
    child: Option<ChildObservation>,
    post_contact_consumer: Option<ConsumerInput>,
    slot_zero_timer: Option<u16>,
    range_out: bool,
    allocation_ok: bool,
    initial_slot_words: Option<[u32; 2]>,
    game_byte_23: i8,
    frames: Vec<GoalFrame>,
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
    exe: &[u8],
    effect_archive: &[u8],
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
    if r.init_retries.len() > 2047 {
        return Err(Error::invalid("too many goal initialization attempts"));
    }
    let mut work = GoalInitWork {
        angle_x: i32::from(row.placement.rotation[0] as i16),
        mode: 0,
    };
    let mut game_byte = r.game_byte_23;
    let mut init = work.attempt(
        stage.dispatch.character,
        game_byte,
        r.slot_zero_timer,
        r.range_out,
        r.allocation_ok,
    );
    let mut attempts = vec![json!({"route":init,"work":work})];
    for retry in r.init_retries {
        if !matches!(init, InitRoute::Deferred | InitRoute::NoWork) {
            return Err(Error::invalid(
                "retry requires deferred initialization or failed allocation",
            ));
        }
        game_byte = retry.game_byte_23;
        init = work.attempt(
            stage.dispatch.character,
            game_byte,
            retry.slot_zero_timer,
            retry.range_out,
            retry.allocation_ok,
        );
        attempts.push(json!({"route":init,"work":work}));
    }
    let texture_setup = match (&init, &r.texture_setup) {
        (InitRoute::Contact { .. }, Some(t)) => {
            let reference_size = crate::texture::gx_buffer_size(64, 64, 2, false, 0)?;
            let mut setup = p.regular_texture_setup(
                t.work56,
                t.size_return.unwrap_or(reference_size),
                t.allocation_return,
            )?;
            setup["sizeSource"] = json!(if t.size_return.is_some() {
                "observed"
            } else {
                "GX reference"
            });
            setup["gxReferenceBytes"] = json!(reference_size);
            setup["gxReferenceObjectWords"] = json!(p.gx_goal_object_words(t.allocation_return));
            Some(setup)
        }
        (_, None) => None,
        _ => {
            return Err(Error::invalid(
                "texture setup requires allocated contact task",
            ))
        }
    };
    let task_bind = match (&init, &r.bind_query_returns) {
        (InitRoute::Contact { angle_x }, Some(q)) => Some(p.regular_task_bind(*angle_x, q)?),
        (_, None) => None,
        _ => {
            return Err(Error::invalid(
                "binding queries require successful contact initialization",
            ))
        }
    };
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
            if d.first_update.is_some() && d.start.is_none() {
                return Err(Error::invalid("first update requires child initialization"));
            }
            if let Some(input) = d.start {
                if d.mode != 0 || d.range_out == Some(true) {
                    return Err(Error::invalid("initialization requires active mode 0"));
                }
                let (mode, start) = p.child_start_with_mode(d.byte1, input)?;
                report["start"] = start;
                if let Some(next) = d.first_update {
                    let mut update = p.child_dispatch(mode, d.byte1, next.range_out)?;
                    if (mode == 1 && next.range_out != Some(true)) != next.game_byte_55.is_some() {
                        return Err(Error::invalid(
                            "game byte required only for an active mode-1 update",
                        ));
                    }
                    if let Some(byte) = next.game_byte_55 {
                        let initial = f32::from_bits(p.child_start_data[0]);
                        update["handlerFrames"] = p.child_mode_one(initial, initial, &[byte])?;
                    }
                    report["firstUpdate"] = update;
                }
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
            json!({"pcCollisionTemplates":p.pc_shape_templates(exe)?,"program":p,"init":init,"initAttempts":attempts,"child":writes,"childDispatch":dispatch,"dispatchedFrames":0,"scope":"Child placement and optional dispatch including mode 3 and 6 handler writes; engine calls are observations."}),
        );
    }
    let InitRoute::Contact { angle_x } = init else {
        return Ok(
            json!({"program":p,"init":init,"initAttempts":attempts,"dispatchedFrames":0,"scope":"Fresh native goal task did not enter the contact executor."}),
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
    let mut shadow_bits = p.shadow_unset;
    if r.track_shadow && r.lifecycle.is_none() {
        return Err(Error::invalid(
            "shadow tracking requires regular goal lifecycle",
        ));
    }
    let mut slot_words = r.initial_slot_words.unwrap_or(p.initial_slot_words);
    let mut frames = vec![];
    let alternate = r
        .frames
        .iter()
        .any(|f| matches!(f,GoalFrame::Sample(s) if s.alternate_timer.is_some()));
    let mut phase = 0f32;
    for observation in r.frames {
        let (hit, game_byte, motion_gate, entry, alt_timer) = match observation {
            GoalFrame::Hit(hit) => (hit, game_byte, None, None, None),
            GoalFrame::Sample(s) => (
                s.hit,
                s.game_byte_23,
                s.game_byte_37,
                s.entry,
                s.alternate_timer,
            ),
        };
        if alternate && (alt_timer.is_none() || motion_gate.is_none() || angle_x % 3 == 0) {
            return Err(Error::invalid(
                "alternate tracking requires a nonregular goal and timer/gate on every frame",
            ));
        }
        if motion_gate.is_some() && r.lifecycle.is_none() && !alternate {
            return Err(Error::invalid(
                "per-frame motion gate requires regular goal lifecycle",
            ));
        }
        if r.track_shadow && entry.is_none() {
            return Err(Error::invalid(
                "shadow tracking requires sampled update entries",
            ));
        }
        let mut shadow = None;
        let entry = if let Some(input) = entry {
            let Some(byte3) = p.update_entry(
                state.latched,
                work.mode,
                input.range_out,
                input.frame_counter,
                game_byte,
            )?
            else {
                if input.shadow_return_bits.is_some() {
                    return Err(Error::invalid("range exit precedes shadow query"));
                }
                frames.push(json!({"rangeOut":true,"calls":[],"latched":state.latched,"moduleSlotWords":slot_words}));
                break;
            };
            let query =
                r.track_shadow && shadow_bits == p.shadow_unset && input.frame_counter & 31 == 0;
            if query != input.shadow_return_bits.is_some() {
                return Err(Error::invalid(
                    "supply shadow return bits only on a scheduled tracked query",
                ));
            }
            if r.track_shadow {
                let mut call = None;
                if let Some(bits) = input.shadow_return_bits {
                    call = Some(
                        json!({"target":0x80067888u32,"positionBits":[center[0].to_bits(),(motion.y+p.shadow_lift).to_bits(),center[2].to_bits()],"rotation":[0,motion.yaw,0]}),
                    );
                    shadow_bits = bits;
                }
                shadow = Some(json!({"query":call,"work40Bits":shadow_bits}));
            }
            let checked = input.range_out.is_some();
            Some(json!({"rangeChecked":checked,"workByte3":byte3}))
        } else {
            None
        };
        let was_latched = state.latched;
        if let Some(input) = &r.lifecycle {
            motion.tick(
                was_latched,
                motion_gate.unwrap_or(input.game_byte_37),
                p.spin,
                p.rise,
            );
        }
        let alt_sound = if let Some(timer) = alt_timer {
            p.alternate_motion(angle_x,was_latched,motion_gate.unwrap(),timer,&mut phase)?.map(|(code,r6,task)|json!({"target":0x8006ad94u32,"code":code,"owner":if task {"task"}else{"work"},"r5":1,"r6":r6,"r7":30,"position":center}))
        } else {
            None
        };
        let calls = state.step(angle_x, game_byte, hit, p.local_effect)?;
        let mut frame = json!({"calls":calls,"latched":state.latched,"resetScaleY":!was_latched && state.latched});
        if alternate {
            if !was_latched && state.latched {
                phase = 0.;
            }
            frame["alternateMotion"] =
                json!({"phaseBits":phase.to_bits(),"soundBeforeContact":alt_sound});
        }
        if let Some(shadow) = shadow {
            frame["shadow"] = shadow;
        }
        if let Some(entry) = entry {
            frame["entry"] = entry;
        }
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
    let released = if let Some(obs) = r.lifecycle.as_ref().and_then(|l| l.release.as_ref()) {
        if !frames.last().is_some_and(|f| !f["cleanupCall"].is_null()) {
            return Err(Error::invalid(
                "observed release requires a cleanup request",
            ));
        }
        Some(p.regular_release(obs.work56, obs.nested)?)
    } else {
        None
    };
    let consumer = if let Some(c) = r.post_contact_consumer {
        let code = p.consume_goal_word(&mut slot_words, c.slot, c.query)?;
        let mut out =
            json!({"localCallArgument":code,"moduleSlotWords":slot_words,"work52":120,"mode":0});
        if let Some(u) = c.update {
            out["update"] = p.consumer_update(u.frame, u.service_return, u.counter)?;
            if u.spawn.is_some() && u.gate.is_none() {
                return Err(Error::invalid("effect spawn requires observed gate"));
            }
            if u.gate.is_some() && u.vectors.is_none() {
                return Err(Error::invalid("post-vector gate requires vector helper"));
            }
            if let Some(v) = u.vectors {
                if u.countdown.is_none() {
                    return Err(Error::invalid(
                        "vector helper requires the common countdown update",
                    ));
                }
                out["update"]["vectors"] = p.consumer_vectors(v)?;
            }
            if let Some(fields) = u.countdown {
                if let Some(g) = u.gate {
                    out["update"]["gate"] = p.consumer_gate(fields.byte1 & !3, g)?;
                    if let Some(s) = u.spawn {
                        if out["update"]["gate"]["continueEffect"] != true {
                            return Err(Error::invalid("effect setup skipped by gate"));
                        }
                        out["update"]["spawn"] = p.consumer_spawn(s)?;
                    }
                }
                out["update"]["countdown"] = p.consumer_countdown(120, fields);
            }
        }
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
        json!({"initAttempts":attempts,"effectTextures":p.effect_texture_receipt(effect_archive)?,"textureSetup":texture_setup,"observedRelease":released,"taskBind":task_bind,"collisionSetup":p.regular_collision_setup(angle_x),"postContactConsumer":consumer,"program":p,"setAngleX":row.placement.rotation[0] as i16,"effectiveAngleX":angle_x,"placement":row.placement_index,"center":center,"radius":radius,"placementSphereProbe":probe,"frames":frames,"scope":"Contact branch after native initialization. Placement-space geometry is diagnostic. Contacts are supplied; effective angle follows fresh SET initialization; no mission clear or save is committed."}),
    )
}
