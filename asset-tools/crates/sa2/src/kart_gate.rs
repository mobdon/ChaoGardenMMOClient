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
    tick_registered(&rel, s)
}
fn tick_registered(rel: &CityRel<'_>, s: GateTick) -> Result<GateResult> {
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
            r.story_cues = s.story_cues.map(|ctx| story_cues(rel, ctx)).transpose()?;
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateSequence {
    timer_word: u32,
    primary_kart_counter: u32,
    sequence: u32,
    registered_tasks: [u32; 6],
    #[serde(default)]
    controller_state: ControllerState,
    lap_clock: Option<crate::kart_lap_clock::Clock>,
    vehicle_state: Option<i32>,
    primary_charge: Option<i32>,
    controller_history: Option<crate::kart_drive_input::SteeringHistory>,
    updates: Vec<SequenceEvent>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GateObservation {
    placement: usize,
    collision_flags: u16,
    first_contact: u32,
    final_flags: u32,
}
#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ControllerState {
    elapsed: i32,
    flag_3ce24: i32,
    engine_byte_16: i8,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyUpdate {}
#[derive(Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
struct PrimaryUpdate {
    primary_update_velocity: [f32; 3],
    primary_update_position: Option<[f32; 3]>,
    primary_update_angles: Option<[i32; 3]>,
}
#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
enum SequenceEvent {
    Gate(GateObservation),
    UpdateStart(PrimaryUpdate),
    ContinueVehicle {
        continue_vehicle_update: EmptyUpdate,
    },
    SpinTick {
        spin_tick: crate::kart_drive_input::StorySpin,
    },
    DriftTick {
        drift_tick: crate::kart_drift::StoryTick,
    },
    ControllerSample {
        primary_stick_x: i16,
        buttons_held: Option<u32>,
        buttons_pressed: Option<u32>,
    },
    BoostTick {
        boost_tick: crate::kart_vehicle_init::StoryBoostInput,
    },
    BalloonReward {
        balloon_reward: BalloonReward,
    },
    SlotProgress {
        slot_progress: SlotTests,
    },
    PostMove {
        vehicle_post_move: Box<crate::kart_timeout::Frame>,
    },
    SpawnTarget {
        target_spawn: TargetSpawn,
    },
    UpdateTarget {
        target_tick: TargetTick,
    },
    Vehicle {
        vehicle_countdown: VehicleCountdown,
    },
    Timeout {
        timeout_tick: TimeoutTick,
    },
    Controller {
        controller: crate::kart_story_target::ControllerSignals,
        timing: Option<crate::kart_lap_clock::TimingSignals>,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BalloonReward {
    placement: usize,
    contact_task: u32,
    driver: i32,
    work_44: Option<u32>,
    clock: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SlotTests {
    work_44: Option<u32>,
    debug_enabled: bool,
    #[serde(default)]
    segment_tests: Vec<bool>,
    highest: Option<i32>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TargetSpawn {
    segment_xz: [[f32; 2]; 4],
    parent_task_y: f32,
    latch: i32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TargetTick {
    active_word: Option<i32>,
    parent_work_x: Option<f32>,
    parent_work_7c: Option<f32>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VehicleCountdown {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TimeoutTick {
    engine_halfword_64: i16,
}
/// Replay admitted active callbacks only. External countdown or traffic changes
/// between callbacks are not implied by the order of supplied contacts.
pub fn story_sequence(
    bytes: &[u8],
    exe: &[u8],
    set: &[u8],
    s: GateSequence,
) -> Result<serde_json::Value> {
    use serde_json::json;
    let rel = verified_module(bytes)?;
    let objects = crate::kart_objects::read(exe, set)?;
    let mode = match objects.set_file {
        "setCartTails.bin" | "setCartTails_hd.bin" => 0,
        "setCartRouge.bin" | "setCartRouge_hd.bin" => 1,
        _ => return Err(Error::invalid("expected a story kart layout")),
    };
    if s.updates.len() > 4096 {
        return Err(Error::invalid("too many gate callbacks"));
    }
    let mut latches = std::collections::BTreeMap::new();
    for (i, p) in objects.placements.iter().enumerate() {
        if p.object_id == 4 {
            latches.insert(i, 0u16);
        }
    }
    let (mut timer, mut count, mut seq) = (s.timer_word, s.primary_kart_counter, s.sequence);
    let mut updates = Vec::new();
    let mut clock = s.controller_state;
    let mut lap = s.lap_clock;
    let mut vehicle = s.vehicle_state;
    let mut timeout = None;
    let mut target_handle = None;
    let mut finish = None;
    let mut timer_words = None;
    let mut charge = s.primary_charge;
    let mut boost_timers = None;
    let mut snapshot = None;
    let mut completed_pose = None;
    let mut inherited_pose = None;
    let mut spin_carry = None;
    let mut windup: Option<crate::kart_drift::DriftCarry> = None;
    let mut history = s.controller_history;
    if let Some(h) = &history {
        h.validate()?;
    }
    let mut sample = None;
    let mut parent = None;
    let mut crossing_positions = None;
    let mut route = None;
    let mut progressed_route = None;
    let mut recovered_pose = None;
    let mut scan_flag = None;
    let mut highest = None;
    let mut target: Option<crate::kart_story_target::State> = None;
    let constants = rel.data(4)?;
    for event in s.updates {
        let event = match event {
            SequenceEvent::Gate(g) => g,
            SequenceEvent::ContinueVehicle {
                continue_vehicle_update: EmptyUpdate {},
            } => {
                let (position, velocity, angles) = completed_pose.take().ok_or_else(|| {
                    Error::invalid("continuation requires completed post-move frame")
                })?;
                if snapshot.is_some() {
                    return Err(Error::invalid("overlapping primary update snapshots"));
                }
                snapshot = Some(PrimaryUpdate {
                    primary_update_velocity: velocity,
                    primary_update_position: Some(position),
                    primary_update_angles: Some(angles),
                });
                inherited_pose = Some((position, velocity, angles[1]));
                updates.push(json!({"previousVelocity":velocity,"savedPosition":position,"savedAngles":angles,"work_10c":0}));
                continue;
            }
            SequenceEvent::UpdateStart(start) => {
                completed_pose = None;
                inherited_pose = None;
                if snapshot.replace(start).is_some() {
                    return Err(Error::invalid("overlapping primary update snapshots"));
                }
                updates
                    .push(json!({"previousVelocity":start.primary_update_velocity,"work_10c":0}));
                continue;
            }
            SequenceEvent::SpinTick { mut spin_tick } => {
                if vehicle != Some(4) {
                    return Err(Error::invalid("spin tick requires vehicle state 4"));
                }
                let carry = spin_carry
                    .take()
                    .ok_or_else(|| Error::invalid("missing preceding story spin impact"))?;
                let velocity = spin_tick.world_velocity();
                let context = spin_tick.contact.take();
                let audio = spin_tick.audio.take();
                let magnitude = spin_tick.magnitude();
                let r = spin_tick.advance(constants, carry)?;
                vehicle = Some(r.state());
                if r.state() == 4 {
                    spin_carry = Some(r.carry());
                }
                completed_pose = None;
                inherited_pose = None;
                recovered_pose = None;
                let start = snapshot.take();
                let mut event = json!({"spinTick":r});
                if let Some(a) = audio {
                    if scan_flag.is_some_and(|flag| flag != a.work_44 as u32)
                        || context.as_ref().is_some_and(|c| c.work_44 != a.work_44)
                    {
                        return Err(Error::invalid("spin audio conflicts with vehicle flag"));
                    }
                    event["spinAudio"] = json!(a.run(&rel, mode, &r, velocity, magnitude)?);
                }
                if let Some(mut ctx) = context {
                    if scan_flag.is_some_and(|flag| flag != ctx.work_44 as u32) {
                        return Err(Error::invalid("spin contact conflicts with vehicle flag"));
                    }
                    ctx.query
                        .bind_origin(start.and_then(|v| v.primary_update_position))?;
                    if let Some(rec) = &mut ctx.query.recovery {
                        rec.bind_progress(route, count)?;
                        if let Some(v) = start {
                            rec.bind_angles(v.primary_update_angles)?;
                        }
                    }
                    let response = ctx.query.apply(
                        constants,
                        bytes,
                        exe,
                        r.carry().0,
                        vehicle,
                        r.position(),
                        velocity,
                        ctx.work_28c,
                        ctx.work_44,
                        mode,
                        crate::kart_fall_recovery::ImpactCheck::Skip,
                    )?;
                    if let Some(recovered) = response.recovered_progress() {
                        vehicle = Some(recovered.state);
                        spin_carry =
                            (recovered.state == 4).then_some((recovered.work_68, r.carry().1));
                        route = Some(recovered.route);
                        progressed_route = route;
                        count = recovered.counter;
                        crossing_positions = Some(recovered.positions);
                        recovered_pose = Some(recovered);
                    }
                    if let Some(state) = response.wall_state().filter(|_| recovered_pose.is_none())
                    {
                        vehicle = Some(state);
                        if state != 4 {
                            spin_carry = None;
                        }
                    }
                    event["spinContact"] = json!(response);
                }
                if start.is_some() {
                    event["work_10c"] = json!(2);
                }
                sample = None;
                updates.push(event);
                continue;
            }
            SequenceEvent::DriftTick { mut drift_tick } => {
                completed_pose = None;
                if let Some((position, _, yaw)) = inherited_pose.take() {
                    drift_tick.bind_task_pose(position, yaw)?;
                }
                recovered_pose = None;
                let start = snapshot.take();
                let audio = drift_tick.audio.take();
                let mut context = drift_tick.contact.take();
                if let Some(ctx) = &mut context {
                    if scan_flag.is_some_and(|flag| flag != ctx.work_44 as u32) {
                        return Err(Error::invalid("drift contact conflicts with vehicle flag"));
                    }
                    ctx.query
                        .bind_origin(start.and_then(|v| v.primary_update_position))?;
                    if let Some(r) = &mut ctx.query.recovery {
                        r.bind_progress(route, count)?;
                        if let Some(v) = start {
                            r.bind_snapshot(v.primary_update_velocity, v.primary_update_angles)?;
                        }
                    }
                }
                let velocity = drift_tick.world_velocity();
                let state = vehicle
                    .filter(|s| matches!(s, 2 | 3))
                    .ok_or_else(|| Error::invalid("drift tick requires state 2 or 3"))?;
                let row = sample.take();
                let held = drift_tick.held(row.and_then(|(_, _, held, _)| held))?;
                let next = windup
                    .as_mut()
                    .ok_or_else(|| Error::invalid("missing normal-drive drift entry"))?
                    .advance(
                        bytes,
                        drift_tick,
                        held,
                        row.map(|(stick, _, _, _)| stick),
                        state,
                    )?;
                vehicle = Some(next.state());
                let mut event = json!({"driftTick":next});
                if let Some(a) = audio {
                    if scan_flag.is_some_and(|v| v != a.work_44 as u32)
                        || context
                            .as_ref()
                            .is_some_and(|c| c.work_44 != a.work_44 || c.work_28c != a.work_28c)
                    {
                        return Err(Error::invalid("drift audio conflicts with vehicle work"));
                    }
                    event["driftAudio"] = json!(a.run(&rel, mode, &next, velocity, held)?);
                }
                if let Some(ctx) = context {
                    let response = ctx.query.apply(
                        constants,
                        bytes,
                        exe,
                        next.work_68,
                        vehicle,
                        next.position,
                        velocity,
                        ctx.work_28c,
                        ctx.work_44,
                        mode,
                        crate::kart_fall_recovery::ImpactCheck::Run,
                    )?;
                    if let Some(recovered) = response.recovered_progress() {
                        vehicle = Some(recovered.state);
                        route = Some(recovered.route);
                        progressed_route = route;
                        count = recovered.counter;
                        crossing_positions = Some(recovered.positions);
                        recovered_pose = Some(recovered);
                    }
                    spin_carry = response.spin_carry();
                    if recovered_pose.is_none() {
                        vehicle = response.wall_state().or(vehicle);
                    }
                    event["driftContact"] = json!(response);
                }
                if !matches!(vehicle, Some(2 | 3)) {
                    windup = None;
                }
                if start.is_some() {
                    event["work_10c"] = json!(4);
                }
                updates.push(event);
                continue;
            }
            SequenceEvent::ControllerSample {
                primary_stick_x,
                buttons_held,
                buttons_pressed,
            } => {
                let sum = history
                    .as_mut()
                    .ok_or_else(|| Error::invalid("missing initial controller history"))?
                    .sample(primary_stick_x);
                sample = Some((primary_stick_x, sum, buttons_held, buttons_pressed));
                updates.push(json!({"primaryStickX":primary_stick_x,"steeringSum":sum}));
                continue;
            }
            SequenceEvent::BoostTick { mut boost_tick } => {
                completed_pose = None;
                if let Some((p, v, yaw)) = inherited_pose.take() {
                    boost_tick.bind_previous_frame(p, v, yaw)?;
                }
                let start = snapshot.take();
                if let Some(v) = start {
                    if let Some(d) = boost_tick.drive.as_mut() {
                        d.begin_update()?;
                    }
                    if let Some(c) = boost_tick.contact_mut() {
                        c.bind_origin(v.primary_update_position)?;
                    }
                    if let Some(r) = boost_tick.contact_mut().and_then(|c| c.recovery.as_mut()) {
                        r.bind_snapshot(v.primary_update_velocity, v.primary_update_angles)?;
                    }
                }
                recovered_pose = None;
                let row = sample.take();
                if let Some((stick, sum, held, pressed)) = row {
                    boost_tick.bind_pressed(pressed)?;
                    if let Some(d) = boost_tick.drive.as_mut() {
                        d.bind_sample((stick, sum), held)?;
                    }
                }
                if let Some(r) = boost_tick.contact_mut().and_then(|c| c.recovery.as_mut()) {
                    r.bind_progress(route, count)?;
                }
                let (next, timers, result) = crate::kart_vehicle_init::story_boost(
                    bytes,
                    exe,
                    boost_tick,
                    mode as usize,
                    charge.ok_or_else(|| Error::invalid("missing primary charge for boost"))?,
                    boost_timers,
                    scan_flag,
                    vehicle,
                    row.and_then(|(stick, _, held, _)| held.map(|h| (stick, h))),
                )?;
                if let Some(r) = &result.recovery_drive {
                    vehicle = Some(r.state());
                }
                if let Some(response) = result.contact() {
                    spin_carry = response.spin_carry();
                }
                if let Some(entry) = result.drive.as_ref().and_then(|d| d.windup_entry()) {
                    windup = Some(crate::kart_drift::DriftCarry::new(
                        entry,
                        result.drift_tuning(),
                    ));
                }
                if let Some(state) = result.drive.as_ref().and_then(|d| d.drift_state()) {
                    vehicle = Some(state);
                }
                if let Some(recovered) = result.contact().and_then(|c| c.recovered_progress()) {
                    vehicle = Some(recovered.state);
                    route = Some(recovered.route);
                    progressed_route = route;
                    count = recovered.counter;
                    crossing_positions = Some(recovered.positions);
                    recovered_pose = Some(recovered);
                }
                charge = Some(next);
                boost_timers = Some(timers);
                updates.push(json!({"boostTick":result}));
                continue;
            }
            SequenceEvent::BalloonReward { balloon_reward: v } => {
                let placement = objects
                    .placements
                    .get(v.placement)
                    .filter(|p| p.object_id == 9)
                    .ok_or_else(|| Error::invalid("reward placement is not a story balloon"))?;
                let kind = i32::from(placement.rotation[0] as i16);
                if !(1..=3).contains(&kind)
                    || crate::kart_registry::slot(&s.registered_tasks, v.contact_task) != Some(0)
                {
                    return Err(Error::invalid(
                        "expected fixed story balloon and primary kart contact",
                    ));
                }
                if scan_flag.zip(v.work_44).is_some_and(|(a, b)| a != b) {
                    return Err(Error::invalid(
                        "balloon vehicle flag conflicts with dispatch",
                    ));
                }
                let flag = scan_flag
                    .or(v.work_44)
                    .ok_or_else(|| Error::invalid("missing balloon vehicle flag"))?;
                let prior =
                    charge.ok_or_else(|| Error::invalid("missing initial primary charge"))?;
                let award = crate::kart_balloon::award(
                    kind,
                    v.driver,
                    flag as i32,
                    prior,
                    v.clock,
                    || unreachable!(),
                )
                .map_err(Error::invalid)?;
                let display = match award {
                    crate::kart_balloon::Award::None => None,
                    crate::kart_balloon::Award::Charge {
                        next,
                        display,
                        notify,
                    } => {
                        charge = Some(next);
                        Some((display, notify))
                    }
                    _ => return Err(Error::invalid("unexpected non-charge story reward")),
                };
                updates.push(json!({"balloonReward":{"placement":v.placement,"charge":charge,"display":display}}));
                continue;
            }
            SequenceEvent::SlotProgress { slot_progress: v } => {
                let (piece, course_length) = route
                    .ok_or_else(|| Error::invalid("slot progress requires vehicle route state"))?;
                if highest.zip(v.highest).is_some_and(|(a, b)| a != b) {
                    return Err(Error::invalid("conflicting highest checkpoint"));
                }
                let (task, work) = crossing_positions
                    .ok_or_else(|| Error::invalid("missing kart pose for course update"))?;
                if scan_flag.zip(v.work_44).is_some_and(|(a, b)| a != b) {
                    return Err(Error::invalid(
                        "course scan flag conflicts with vehicle dispatch",
                    ));
                }
                let stationary = crate::kart_progress::planar_direction(task, work)? == [0.; 3];
                let r = crate::kart_progress::update_motion(
                    crate::kart_progress::Input {
                        piece,
                        course_length,
                        counter: count as i32,
                        highest: highest
                            .or(v.highest)
                            .ok_or_else(|| Error::invalid("missing initial highest checkpoint"))?,
                        work_44: scan_flag
                            .or(v.work_44)
                            .ok_or_else(|| Error::invalid("missing course scan flag"))?,
                        debug_enabled: v.debug_enabled,
                        mode,
                        segment_tests: v.segment_tests,
                    },
                    stationary,
                )?;
                route = Some((r.piece, course_length));
                progressed_route = route;
                highest = Some(r.highest);
                count = r.counter as u32;
                updates.push(json!({"slotProgress":r}));
                continue;
            }
            SequenceEvent::PostMove {
                vehicle_post_move: mut frame,
            } => {
                if let Some(pose) = recovered_pose.take() {
                    frame.bind_recovery(pose)?;
                }
                if let Some(selected) = progressed_route.take() {
                    frame.bind_route(selected)?;
                }
                if let Some(prior) = timer_words {
                    frame.inherit_timers(prior);
                }
                frame.bind_finish(finish)?;
                frame.bind_story_dispatch(target.map(|t| t.latch), count, highest)?;
                if target_handle.is_some_and(|handle| handle != frame.work_ec) {
                    return Err(Error::invalid(
                        "vehicle target handle changed between post-move callbacks",
                    ));
                }
                let state = vehicle.ok_or_else(|| {
                    Error::invalid("vehicle_post_move requires initial vehicle_state")
                })?;
                let r = crate::kart_timeout::step_verified(
                    bytes,
                    crate::kart_timeout::Input::VehiclePostMove {
                        frame: *frame,
                        remaining: timer as i32,
                        vehicle_state: state,
                        mode,
                    },
                )?;
                vehicle = r.vehicle_state;
                if let Some(t) = r.timer {
                    timeout = Some(t);
                }
                if let Some(spawned) = r.spawned_target() {
                    if target.is_some() {
                        return Err(Error::invalid("pursuit target already spawned"));
                    }
                    target = Some(spawned);
                }
                target_handle = r.frame.as_ref().map(|f| f.work_ec);
                parent = r.frame.as_ref().and_then(|f| f.pursuit_parent());
                snapshot = None;
                inherited_pose = None;
                completed_pose = r.frame.as_ref().and_then(|f| f.next_update_pose());
                crossing_positions = r.frame.as_ref().and_then(|f| f.crossing_positions());
                scan_flag = r.frame.as_ref().and_then(|f| f.course_scan_flag());
                route = r.frame.as_ref().and_then(|f| f.route());
                if let Some(h) = r.frame.as_ref().and_then(|f| f.highest()) {
                    highest = Some(h);
                }
                timer_words = r.frame.as_ref().and_then(|f| f.timer_words());
                finish = r.frame.as_ref().and_then(|f| f.global_24d60);
                if let Some(n) = r.frame.as_ref().and_then(|f| f.checkpoint_counter()) {
                    count = n;
                }
                updates.push(json!({"vehiclePostMove":r}));
                continue;
            }
            SequenceEvent::SpawnTarget { target_spawn: v } => {
                if target.is_some() {
                    return Err(Error::invalid("pursuit target already spawned"));
                }
                let out = crate::kart_story_target::step_constants(
                    constants,
                    crate::kart_story_target::Input::Spawn {
                        segment_xz: v.segment_xz,
                        parent_task_y: v.parent_task_y,
                        mode,
                        latch: v.latch,
                    },
                )?;
                target = Some(out.state);
                updates.push(json!({"target":out}));
                continue;
            }
            SequenceEvent::UpdateTarget { target_tick: v } => {
                let state = target.ok_or_else(|| Error::invalid("missing pursuit target"))?;
                let active = timer as i32;
                if v.active_word.is_some_and(|w| w != active) {
                    return Err(Error::invalid(
                        "target active word conflicts with race clock",
                    ));
                }
                let resolve = |observation: Option<f32>, index: usize| -> Result<f32> {
                    let retained = parent.map(|p: [f32; 2]| p[index]);
                    if observation
                        .zip(retained)
                        .is_some_and(|(a, b)| a.to_bits() != b.to_bits())
                    {
                        return Err(Error::invalid(
                            "target parent observation conflicts with vehicle work",
                        ));
                    }
                    // At nonpositive time the native routine never reads parent motion.
                    if active <= 0 || (state.latch != 0 && index == 1) {
                        return Ok(0.0);
                    }
                    observation.or(retained).ok_or_else(|| {
                        Error::invalid("missing target parent motion before vehicle update")
                    })
                };
                let out = crate::kart_story_target::step_constants(
                    constants,
                    crate::kart_story_target::Input::Tick {
                        state,
                        active_word: active,
                        parent_work_x: resolve(v.parent_work_x, 0)?,
                        parent_work_7c: resolve(v.parent_work_7c, 1)?,
                        mode,
                    },
                )?;
                target = Some(out.state);
                updates.push(json!({"target":out}));
                continue;
            }
            SequenceEvent::Vehicle {
                vehicle_countdown: VehicleCountdown {},
            } => {
                let state = vehicle.ok_or_else(|| {
                    Error::invalid("vehicle_countdown requires initial vehicle_state")
                })?;
                let result = crate::kart_timeout::step_verified(
                    bytes,
                    crate::kart_timeout::Input::VehicleCountdown {
                        remaining: timer as i32,
                        vehicle_state: state,
                        mode,
                    },
                )?;
                vehicle = result.vehicle_state;
                if let Some(start) = result.timer {
                    timeout = Some(start);
                }
                updates.push(json!({"vehicleCountdown":result}));
                continue;
            }
            SequenceEvent::Timeout { timeout_tick } => {
                let t = timeout.ok_or_else(|| Error::invalid("timeout task has not spawned"))?;
                let result = crate::kart_timeout::step_verified(
                    bytes,
                    crate::kart_timeout::Input::Tick {
                        timer: t,
                        engine_halfword_64: timeout_tick.engine_halfword_64,
                    },
                )?;
                timeout = result.timer;
                updates.push(json!({"timeoutTick":result}));
                continue;
            }
            SequenceEvent::Controller { controller, timing } => {
                let mut input = controller.input(
                    timer as i32,
                    clock.elapsed,
                    clock.flag_3ce24,
                    clock.engine_byte_16,
                    target.map(|t| t.latch),
                    finish,
                    mode,
                )?;
                input.lap_clock=match (timing,lap){
                    (Some(signals),Some(c))=>Some(signals.with_clock(c,count as i32,highest)?),
                    (None,None)=>None,
                    _=>return Err(Error::invalid("lap replay requires both initial lap_clock and timing on controller events")),
                };
                let r = crate::kart_story_target::controller_verified(input)?;
                if let Some(c) = r.lap_clock {
                    lap = Some(c);
                }
                timer = r.remaining as u32;
                clock = ControllerState {
                    elapsed: r.elapsed,
                    flag_3ce24: r.flag_3ce24,
                    engine_byte_16: r.engine_byte_16,
                };
                updates.push(json!({"controller":r}));
                continue;
            }
        };
        let latch = latches
            .get_mut(&event.placement)
            .ok_or_else(|| Error::invalid("placement is not a story time gate"))?;
        let result = tick_registered(
            &rel,
            GateTick {
                latch: *latch,
                collision_flags: event.collision_flags,
                first_contact: event.first_contact,
                registered_tasks: s.registered_tasks,
                work_24: objects.placements[event.placement].parameters[1],
                timer_word: timer,
                primary_kart_counter: count,
                final_flags: event.final_flags,
                story_cues: Some(StoryCueContext {
                    mode,
                    sequence: seq,
                }),
            },
        )?;
        *latch = result.latch;
        timer = result.timer_word;
        count = result.primary_kart_counter;
        if let Some(cue) = &result.story_cues {
            seq = cue.next_sequence;
        }
        updates.push(json!({"placement":event.placement,"result":result}));
    }
    Ok(
        json!({"setFile":objects.set_file,"setSha256":manifest::sha256(set),"updates":updates,"latches":latches,
        "controllerHistory":history,"primaryCharge":charge,"primaryRoute":route,"highestCheckpoint":highest,"lapClock":lap,"finishFlag":finish,"targetHandle":target_handle,"target":target,"vehicleState":vehicle,"timeoutTimer":timeout,"controllerState":clock,"timerWord":timer,"primaryKartCounter":count,"sequence":seq}),
    )
}
