//! Active GATE2 callback in the installed CartD.rel, after the entry cull.
use crate::{city_rel::CityRel, manifest, Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateTick {
    pub latch: u16,
    pub collision_flags: u16,
    pub first_contact: u32,
    pub registered_tasks: [u32; 6],
    pub work_24: f32,
    pub timer_word: u32,
    pub primary_kart_counter: u32,
    /// External global word observed after the callback's other calls.
    pub final_flags: u32,
    pub story_cues: Option<StoryCueContext>,
}
#[derive(Serialize)]
pub struct GateResult {
    pub latch: u16,
    pub timer_word: u32,
    pub primary_kart_counter: u32,
    pub work_byte_3: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub story_cues: Option<StoryCueResult>,
    /// Calls are listed in execution order. Local addresses are section-1 offsets.
    pub calls: Vec<&'static str>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryCueContext {
    mode: i32,
    /// Separate from the primary kart checkpoint counter (section 6 +0x3ce18).
    sequence: u32,
}
#[derive(Serialize)]
pub struct StoryCueResult {
    next_sequence: u32,
    time_extension_task: u32,
    cue: u32,
    delayed: Option<DelayedCue>,
}
#[derive(Serialize)]
pub struct DelayedCue {
    pub index: u8,
    pub timer: u16,
    pub cue: u32,
}
impl DelayedCue {
    /// CartD +0xba48 tests the old timer, then plays the cue and releases the task.
    pub fn tick(&mut self) -> Option<u32> {
        let expired = self.timer == 0;
        self.timer = self.timer.wrapping_sub(1);
        expired.then_some(self.cue)
    }
}
fn story_cues(rel: &CityRel<'_>, ctx: StoryCueContext) -> Result<StoryCueResult> {
    if !matches!(ctx.mode, 0 | 1) || ctx.sequence >= 3 {
        return Err(Error::invalid(
            "unsupported story gate dialogue table index",
        ));
    }
    let at = rel
        .reference(1, 0xb9b2)
        .filter(|r| (r.module, r.section, r.kind) == (80, 5, 4))
        .ok_or_else(|| Error::invalid("missing gate cue table"))?
        .addend as usize;
    let index = ctx.sequence + 3 * ctx.mode as u32;
    let data = rel.data(5)?;
    let entry = data
        .get(at + 8 * index as usize..)
        .and_then(|s| s.get(..8))
        .ok_or_else(|| Error::invalid("truncated gate cue row"))?;
    let cue = u32::from_be_bytes(entry[..4].try_into().unwrap());
    let delayed = matches!(index, 1 | 4).then(|| DelayedCue {
        index: index as u8,
        timer: 240,
        cue: u32::from_be_bytes(entry[4..].try_into().unwrap()),
    });
    Ok(StoryCueResult {
        next_sequence: ctx.sequence + 1,
        time_extension_task: 0xc858,
        cue,
        delayed,
    })
}
fn verified_module(bytes: &[u8]) -> Result<CityRel<'_>> {
    if manifest::sha256(bytes) != "71826645bae2a4064cd5ad4402facaf4da6c5caee0be7e19894ed7ceb2c2ee6c"
    {
        return Err(Error::invalid("unsupported CartD.rel revision"));
    }
    CityRel::read_identified(bytes, 80)
}
pub fn tick(bytes: &[u8], s: GateTick) -> Result<GateResult> {
    let rel = verified_module(bytes)?;
    if rel.local_pointer(5, 0x1620)? != Some((1, 0xb5cc)) {
        return Err(Error::invalid("unknown GATE2 callback"));
    }
    let mut r = GateResult {
        story_cues: None,
        latch: s.latch,
        timer_word: s.timer_word,
        primary_kart_counter: s.primary_kart_counter,
        work_byte_3: u8::from(s.final_flags & 4 != 0),
        calls: Vec::new(),
    };
    if s.latch == 0 {
        if s.collision_flags & 1 != 0
            && s.first_contact != 0
            && s.registered_tasks.contains(&s.first_contact)
        {
            let ticks = f64::from(s.work_24).trunc();
            if !ticks.is_finite() || ticks < f64::from(i32::MIN) || ticks > f64::from(i32::MAX) {
                return Err(Error::invalid(
                    "gate parameter outside verified conversion range",
                ));
            }
            r.latch = 1;
            r.calls.push("main:8006b710(25,0,0,0)");
            r.timer_word = r
                .timer_word
                .wrapping_add((ticks as i32 as u32).wrapping_mul(60));
            r.primary_kart_counter = r.primary_kart_counter.wrapping_add(1);
            r.calls.extend(["cart:1:c820", "cart:1:b940"]);
            r.story_cues = s.story_cues.map(|ctx| story_cues(&rel, ctx)).transpose()?;
        }
        r.calls.push("main:8000f4ec(gate_task)");
    }
    Ok(r)
}
/// Active GATE (slot 3), distinct from the story GATE2 latch.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RacingGateTick {
    pub counter: u16,
    pub byte_1: u8,
    pub byte_3: u8,
    pub timer: f32,
    pub phase_active: bool,
    pub phase_word: i32,
    pub override_active: bool,
    pub previous_max: i32,
    pub kart_counters: [i32; 6],
}
#[derive(Serialize)]
pub struct RacingGateResult {
    pub counter: u16,
    pub byte_1: u8,
    pub byte_3: u8,
    pub timer: f32,
    pub previous_max: i32,
    pub call_8006b710: Option<[u32; 4]>,
}
pub fn racing_tick(bytes: &[u8], s: RacingGateTick) -> Result<RacingGateResult> {
    let rel = verified_module(bytes)?;
    if rel.local_pointer(5, 0x1610)? != Some((1, 0xb014)) {
        return Err(Error::invalid("unknown GATE callback"));
    }
    if !s.timer.is_finite() {
        return Err(Error::invalid("nonfinite racing gate timer"));
    }
    let data = rel.data(4)?;
    let constant = |at: usize| f32::from_be_bytes(data[at..at + 4].try_into().unwrap());
    let mut r = RacingGateResult {
        counter: s.counter.wrapping_add(1),
        byte_1: s.byte_1,
        byte_3: s.byte_3,
        timer: s.timer,
        previous_max: s.previous_max,
        call_8006b710: None,
    };
    let max_counter = || s.kart_counters.into_iter().fold(-1, i32::max);
    if s.phase_active {
        r.byte_3 = 3_i32.wrapping_sub(s.phase_word) as u8;
        if i32::from(s.byte_1 as i8) != i32::from(r.byte_3) {
            r.byte_1 = r.byte_3;
            r.call_8006b710 = Some([if s.phase_word == 0 { 25 } else { 24 }, 0, 0, 0]);
        }
        r.timer = constant(0x3f0);
        r.previous_max = max_counter();
    } else if s.override_active {
        r.byte_3 = 7;
    } else {
        r.timer -= constant(0x3e8);
        let current = max_counter();
        if r.timer < constant(0x3f4) {
            r.byte_3 = if current == 2 { 6 } else { 4 };
        }
        if current != s.previous_max {
            r.timer = constant(0x3f0);
            r.byte_3 = 5;
        }
        r.previous_max = current;
    }
    Ok(r)
}

/// Initialize the native per-instance model for each story GATE2. The
/// registration receives the initial byte 3 before the shared flag override.
pub fn story_layout(bytes: &[u8], exe: &[u8], set: &[u8]) -> Result<serde_json::Value> {
    use serde_json::json;
    let rel = verified_module(bytes)?;
    let objects = crate::kart_objects::read(exe, set)?;
    let invalid = || Error::invalid("unexpected story gate registration");
    if rel.local_pointer(5, 0x1620)? != Some((1, 0xb5cc))
        || objects.definitions.get(4).map(|v| v.name.as_str()) != Some("GATE2")
    {
        return Err(invalid());
    }
    let target = |site, sec| {
        rel.reference(1, site)
            .filter(|r| (r.module, r.section, r.kind) == (80, sec, 4))
            .map(|r| r.addend)
            .ok_or_else(invalid)
    };
    let template = target(0xb68e, 5)?;
    let data = rel.data(5)?;
    let slice = data
        .get(template as usize..)
        .and_then(|s| s.get(..56))
        .ok_or_else(invalid)?;
    let mut original = [0u32; 14];
    for (word, b) in original.iter_mut().zip(slice.chunks_exact(4)) {
        *word = u32::from_be_bytes(b.try_into().unwrap());
    }
    // Represent relocated pointers as section-relative offsets rather than host addresses.
    for index in [1usize, 11, 12] {
        if let Some((section, off)) = rel.local_pointer(5, template + 4 * index as u32)? {
            if section != 5 {
                return Err(invalid());
            }
            original[index] = off;
        }
    }
    let threshold = target(0xb626, 4)? as usize;
    let threshold = f32::from_be_bytes(rel.data(4)?[threshold..threshold + 4].try_into().unwrap());
    let mut gates = Vec::new();
    for (i, p) in objects
        .placements
        .iter()
        .enumerate()
        .filter(|(_, p)| p.object_id == 4)
    {
        let mut node = original;
        node[0] &= !3;
        node[2..5].copy_from_slice(&p.position.map(f32::to_bits));
        node[5..8].copy_from_slice(&[0, i32::from(p.rotation[1] as i16) as u32, 0]);
        let time = (p.parameters[1] as i32).wrapping_mul(60);
        gates.push(
            json!({"placement":i,"source":p,"initialByte3":u8::from(p.parameters[0]>=threshold),
            "mode":1,"latch":0,"nodeWords":node,"timerTicksOnFirstPass":time}),
        );
    }
    Ok(
        json!({"setFile":objects.set_file,"setSha256":crate::manifest::sha256(set),"callback":0xb5cc,
        "display":target(0xb64a,1)?,"cleanup":target(0xb656,1)?,"collisionDescriptor":target(0xb666,5)?,
        "collisionCount":1,"collisionList":5,"template":template,"registrationFlags":0x20000001,"gates":gates}),
    )
}

#[cfg(test)]
mod cue_tests {
    use super::*;
    #[test]
    fn release_uses_timer_before_decrement() {
        let mut task = DelayedCue {
            index: 1,
            timer: 240,
            cue: 0x704,
        };
        for _ in 0..240 {
            assert_eq!(task.tick(), None);
        }
        assert_eq!(task.tick(), Some(0x704));
        assert_eq!(task.timer, u16::MAX);
    }
}

/// Frame state of CartD timeExtExec (+0xc858), starting from a zeroed task.
#[derive(Default, Clone, Copy, Serialize)]
pub struct TimeExtension {
    mode: u8,
    phase: u8,
    timer: u16,
    position: [f32; 2],
    display: Option<u32>,
}
impl TimeExtension {
    /// True requests task deletion after this update.
    pub fn tick(&mut self) -> bool {
        if self.mode == 0 {
            self.display = Some(0xca10);
            self.position = [700., 200.];
            self.mode = 1;
            return false;
        }
        match self.phase {
            0 | 2 => {
                self.position[0] = (700 - 25 * i32::from(self.timer)) as f32;
                self.timer = self.timer.wrapping_add(1);
                if self.phase == 2 {
                    return self.timer > 40;
                }
                if self.timer > 20 {
                    self.timer = 0;
                    self.phase = 1;
                }
            }
            1 => {
                self.position[0] = 200.;
                self.timer = self.timer.wrapping_add(1);
                if self.timer > 120 {
                    self.timer = 20;
                    self.phase = 2;
                }
            }
            _ => (),
        }
        false
    }
}
pub fn time_extension(bytes: &[u8], pak: &[u8]) -> Result<serde_json::Value> {
    let rel = verified_module(bytes)?;
    for (site, target) in [(0xc83a, 0xc858), (0xc9ce, 0xca10)] {
        if !rel
            .reference(1, site)
            .is_some_and(|r| (r.module, r.section, r.kind, r.addend) == (80, 1, 4, target))
        {
            return Err(Error::invalid("unexpected time extension callback"));
        }
    }
    let data = rel.data(5)?;
    let invalid = || Error::invalid("unexpected checkpoint HUD resource");
    let target = |at| {
        rel.reference(1, at)
            .filter(|r| (r.module, r.section, r.kind) == (80, 5, 4))
            .map(|r| r.addend)
            .ok_or_else(invalid)
    };
    let ptr = |at| {
        rel.local_pointer(5, at)?
            .filter(|(s, _)| *s == 5)
            .map(|(_, v)| v)
            .ok_or_else(invalid)
    };
    let list = target(0xc3aa)?;
    let anim = target(0xc3b6)?;
    let mut sprites = Vec::new();
    for index in [6u32, 5] {
        let at = (anim + 20 * index) as usize;
        let row = data.get(at..at + 20).ok_or_else(invalid)?;
        let h = |i: usize| u16::from_be_bytes([row[2 * i], row[2 * i + 1]]);
        sprites.push(
            serde_json::json!({"index":index,"size":[h(0),h(1)],"pivot":[h(2) as i16,h(3) as i16],
            "uv":[h(4),h(5),h(6),h(7)],"textureId":h(8),"attributes":h(9)}),
        );
    }
    let slot = sprites[0]["textureId"].as_u64().ok_or_else(invalid)? as u32;
    if sprites[0]["textureId"] != sprites[1]["textureId"]
        || slot
            >= u32::from_be_bytes(
                data[list as usize + 4..list as usize + 8]
                    .try_into()
                    .unwrap(),
            )
    {
        return Err(invalid());
    }
    let at = ptr(ptr(list)? + 12 * slot)? as usize;
    let name = std::ffi::CStr::from_bytes_until_nul(&data[at..])
        .map_err(|_| invalid())?
        .to_str()
        .map_err(|_| invalid())?
        .trim_end();
    let member = format!("course\\{name}.dds");
    let bank = crate::archive::read_pak(pak)?;
    let entry = bank
        .iter()
        .find(|e| e.name.eq_ignore_ascii_case(&member))
        .ok_or_else(invalid)?;
    let image = crate::texture::decode_dds(entry.data)?;
    let texture = serde_json::json!({"name":entry.name,"offset":entry.offset,"length":entry.length,"width":image.width,"height":image.height,
        "ddsSha256":crate::manifest::sha256(entry.data),"rgbaSha256":crate::manifest::sha256(&image.rgba)});
    let mut draws = Vec::new();
    let mut state = TimeExtension::default();
    let mut frames = Vec::new();
    for _ in 0..256 {
        let delete = state.tick();
        let [x, y] = state.position;
        draws.push([
            [(1.0 + (400.0 - x)) as i32, (1.0 + y) as i32, -1],
            [x as i32, y as i32, 0],
        ]);
        frames.push(serde_json::json!({"state":state,"delete":delete}));
        if delete {
            return Ok(
                serde_json::json!({"frames":frames,"displayAtState":draws,"drawFlags":32,"sprites":sprites,"texture":texture}),
            );
        }
    }
    Err(Error::invalid("time extension task did not release"))
}

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Presentation {
    Display {
        current_view_metric: f32,
        byte_3: u8,
        position: [f32; 3],
        angles: [i32; 3],
        /// Live shared polygon word from the previous draw, if any.
        material_word: Option<u16>,
    },
    Cleanup {
        task: u32,
        work_38: u32,
    },
}

/// Native gate render/cleanup callbacks +0xb834/+0xb914.
pub fn presentation(bytes: &[u8], request: Presentation) -> Result<serde_json::Value> {
    use serde_json::json;
    let rel = verified_module(bytes)?;
    let invalid = || Error::invalid("unresolved story gate display reference");
    let target = |site| {
        rel.reference(1, site)
            .filter(|r| (r.module, r.section, r.kind) == (80, 5, 4))
            .map(|r| r.addend)
            .ok_or_else(invalid)
    };
    let data = rel.data(5)?;
    let ptr = |at| {
        rel.local_pointer(5, at)?
            .filter(|(s, _)| *s == 5)
            .map(|(_, v)| v)
            .ok_or_else(invalid)
    };
    match request {
        Presentation::Cleanup { task, work_38 } => {
            let call = rel
                .reference(1, 0xb92c)
                .filter(|r| r.module == 0 && r.kind == 10)
                .ok_or_else(invalid)?;
            Ok(
                json!({"call":(work_38!=0).then(||json!({"address":call.addend,"args":[task,work_38]}))}),
            )
        }
        Presentation::Display {
            current_view_metric,
            byte_3,
            position,
            angles,
            material_word,
        } => {
            let limit = f32::from_be_bytes(rel.data(4)?[0x1c0..0x1c4].try_into().unwrap());
            if current_view_metric >= limit {
                return Ok(json!({"draw":null,"writes":[]}));
            }
            let model = target(0xb8f6)?;
            let mut table = target(0xb87a)?;
            let mut writes = Vec::new();
            let mut runtime = data.to_vec();
            {
                while let Some((_, object)) = rel.local_pointer(5, table)? {
                    if object == model {
                        let count = i32::from_be_bytes(
                            data[table as usize + 4..table as usize + 8]
                                .try_into()
                                .unwrap(),
                        );
                        let entries = ptr(table + 8)?;
                        for i in 0..count {
                            let entry = ptr(entries + 4 * i as u32)? as usize;
                            if data[entry..entry + 4] != 73u32.to_be_bytes() {
                                continue;
                            }
                            let at = ptr(entry as u32 + 4)?;
                            let before = material_word.unwrap_or_else(|| {
                                u16::from_be_bytes(
                                    data[at as usize..at as usize + 2].try_into().unwrap(),
                                )
                            });
                            let after = (before & 0xc000) + if byte_3 == 0 { 69 } else { 73 };
                            if byte_3 <= 1 {
                                runtime[at as usize..at as usize + 2]
                                    .copy_from_slice(&after.to_be_bytes());
                                writes.push(json!({"offset":at,"before":before,"after":after}));
                            } else {
                                runtime[at as usize..at as usize + 2]
                                    .copy_from_slice(&before.to_be_bytes());
                            }
                        }
                    }
                    table += 12;
                }
            }
            if rel.local_pointer(5, model + 44)?.is_some()
                || rel.local_pointer(5, model + 48)?.is_some()
            {
                return Err(invalid());
            }
            let attach = ptr(model + 4)?;
            let geometry = crate::kart_story_target::vertices_from_data(
                &rel,
                &runtime,
                attach,
                &mut crate::visual::Material::default(),
            )?;
            let node = &data[model as usize..model as usize + 52];
            let w = |i: usize| u32::from_be_bytes(node[i..i + 4].try_into().unwrap());
            let v = |i: usize| [w(i), w(i + 4), w(i + 8)];
            let pose = json!({"flags":w(0),"position":(v(8).map(f32::from_bits)),"angles":(v(20).map(|x|x as i32)),"scale":(v(32).map(f32::from_bits))});
            Ok(
                json!({"draw":{"model":model,"attach":attach,"node":pose,"geometry":geometry,"textureList":target(0xb8ea)?,"position":position,
                "rotations":[["z",angles[2]],["x",angles[0]],["y",angles[1]]]},"writes":writes}),
            )
        }
    }
}

pub fn bind_draw_textures(bytes: &[u8], pak: &[u8], report: &mut serde_json::Value) -> Result<()> {
    let rel = verified_module(bytes)?;
    let data = rel.data(5)?;
    let invalid = || Error::invalid("unresolved story gate texture");
    let ptr = |at| {
        rel.local_pointer(5, at)?
            .filter(|(s, _)| *s == 5)
            .map(|(_, v)| v)
            .ok_or_else(invalid)
    };
    let draw = &mut report["draw"];
    let list =
        u32::try_from(draw["textureList"].as_u64().ok_or_else(invalid)?).map_err(|_| invalid())?;
    let names = ptr(list)?;
    let count = u32::from_be_bytes(
        data.get(list as usize + 4..list as usize + 8)
            .ok_or_else(invalid)?
            .try_into()
            .unwrap(),
    );
    let bank = crate::archive::read_pak(pak)?;
    let mut textures = std::collections::BTreeMap::new();
    for polygon in draw["geometry"]["polygons"]
        .as_array()
        .ok_or_else(invalid)?
    {
        let Some(slot) = polygon["material"]["textureId"].as_u64() else {
            continue;
        };
        if slot >= count as u64 {
            return Err(invalid());
        }
        if textures.contains_key(&slot) {
            continue;
        }
        let at = ptr(names + 12 * slot as u32)? as usize;
        let name = std::ffi::CStr::from_bytes_until_nul(&data[at..])
            .map_err(|_| invalid())?
            .to_str()
            .map_err(|_| invalid())?
            .trim_end();
        let member = format!("course\\{name}.dds");
        let entry = bank
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(&member))
            .ok_or_else(invalid)?;
        let image = crate::texture::decode_dds(entry.data)?;
        textures.insert(slot,serde_json::json!({"entry":entry.name,"offset":entry.offset,"length":entry.length,"width":image.width,
            "height":image.height,"ddsSha256":crate::manifest::sha256(entry.data),"rgbaSha256":crate::manifest::sha256(&image.rgba)}));
    }
    draw["textures"] = serde_json::to_value(textures)?;
    Ok(())
}
