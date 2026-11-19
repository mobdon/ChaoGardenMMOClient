//! CartLimousineExec: task initialization and active update in installed CartD.
use crate::{kart_progress, Result};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    position: [f32; 3],
    lead: f32,
    latch: i32,
}

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Input {
    Spawn {
        segment_xz: [[f32; 2]; 4],
        parent_task_y: f32,
        mode: i32,
        latch: i32,
    },
    Tick {
        state: State,
        active_word: i32,
        parent_work_x: f32,
        parent_work_7c: f32,
        mode: i32,
    },
}

#[derive(Serialize)]
pub struct Output {
    state: State,
    cue_id: Option<i32>,
    installs_callbacks: bool,
}

pub fn step(bytes: &[u8], input: Input) -> Result<Output> {
    kart_progress::verify(bytes)?;
    let constants = crate::city_rel::CityRel::read_identified(bytes, 80)?.data(4)?;
    let f = |i: usize| f32::from_be_bytes(constants[i..i + 4].try_into().unwrap());
    let mut cue_id = None;
    let (state, installs_callbacks) = match input {
        Input::Spawn {
            segment_xz,
            parent_task_y,
            mode,
            latch,
        } => {
            let [x, z] = segment_xz[2];
            (
                State {
                    position: [
                        x + f(0x478),
                        if mode == 1 {
                            parent_task_y + f(0x47c)
                        } else {
                            parent_task_y
                        },
                        z,
                    ],
                    lead: f(0x478),
                    latch,
                },
                true,
            )
        }
        Input::Tick {
            mut state,
            active_word,
            parent_work_x,
            parent_work_7c,
            mode,
        } => {
            if active_word > 0 {
                if state.latch != 0 {
                    state.position[0] = f(0x47c) + parent_work_x;
                } else {
                    state.lead += f(0x480) - parent_work_7c;
                    if state.lead > f(0x484) {
                        state.lead = f(0x484);
                    } else if state.lead < f(0x47c) {
                        cue_id = match mode {
                            0 => Some(0x701),
                            1 => Some(0x708),
                            _ => None,
                        };
                        state.latch = 1;
                    }
                    state.position[0] = parent_work_x + state.lead;
                }
            }
            (state, active_word > 0)
        }
    };
    Ok(Output {
        state,
        cue_id,
        installs_callbacks,
    })
}

/// Controller prelude after the target task, before the shared timing code at 0x861c.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControllerInput {
    lap_clock: Option<crate::kart_lap_clock::ClockInput>,
    race_results: Option<ResultContext>,
    remove_task: bool,
    pursuit_latch: i32,
    race_finish_latch: i32,
    pub(crate) elapsed: i32,
    pub(crate) remaining: i32,
    engine_byte_09: i8,
    engine_byte_42: i8,
    pub(crate) flag_3ce24: i32,
    pub(crate) engine_byte_16: i8,
}

/// Inputs at a scheduled controller callback; clock words come from the shared state.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ControllerSignals {
    remove_task: bool,
    pursuit_latch: i32,
    race_finish_latch: i32,
    engine_byte_09: i8,
    engine_byte_42: i8,
}
impl ControllerSignals {
    pub(crate) fn input(
        self,
        remaining: i32,
        elapsed: i32,
        flag_3ce24: i32,
        engine_byte_16: i8,
    ) -> ControllerInput {
        ControllerInput {
            remaining,
            elapsed,
            flag_3ce24,
            engine_byte_16,
            remove_task: self.remove_task,
            pursuit_latch: self.pursuit_latch,
            race_finish_latch: self.race_finish_latch,
            engine_byte_09: self.engine_byte_09,
            engine_byte_42: self.engine_byte_42,
            race_results: None,
            lap_clock: None,
        }
    }
}
#[derive(Serialize)]
pub struct ControllerUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    lap_clock: Option<crate::kart_lap_clock::Clock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    race_results: Option<ResultActions>,
    pub(crate) elapsed: i32,
    pub(crate) remaining: i32,
    pub(crate) flag_3ce24: i32,
    pub(crate) engine_byte_16: i8,
    calls: Vec<(&'static str, Option<i32>)>,
    next: &'static str,
}

pub fn controller(bytes: &[u8], s: ControllerInput) -> Result<ControllerUpdate> {
    kart_progress::verify(bytes)?;
    controller_verified(s)
}
pub(crate) fn controller_verified(s: ControllerInput) -> Result<ControllerUpdate> {
    let mut r = ControllerUpdate {
        lap_clock: None,
        race_results: None,
        elapsed: s.elapsed,
        remaining: s.remaining,
        flag_3ce24: s.flag_3ce24,
        engine_byte_16: s.engine_byte_16,
        calls: vec![],
        next: "shared_timing_861c",
    };
    if s.remove_task {
        r.calls.push(("engine_80071888_task", None));
        r.next = "return";
    } else if s.pursuit_latch != 0 {
        r.calls.push(("engine_800629e4", None));
        r.elapsed = r.elapsed.wrapping_add(1);
        if r.elapsed == 300 {
            r.calls.push(("engine_8005eb24", Some(0)));
        }
        r.flag_3ce24 = 1;
        r.engine_byte_16 = 1;
    } else if s.race_finish_latch != 0 {
        if let Some(ctx) = s.race_results {
            r.race_results = Some(results(&mut r, ctx)?);
        } else {
            r.next = "race_results_83bc";
        }
    } else if s.engine_byte_09 == 0 || s.engine_byte_42 != 0 {
        r.remaining = r.remaining.wrapping_sub(1);
    }
    if r.next == "shared_timing_861c" {
        if let Some(clock) = s.lap_clock {
            r.lap_clock = Some(crate::kart_lap_clock::advance(
                clock,
                s.race_finish_latch != 0,
            )?);
            r.next = "slot_progress_8824";
        }
    }
    Ok(r)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResultContext {
    mode: i32,
    first_slot_place: Option<i32>,
    multiplayer_byte: i8,
    engine_byte_23: i8,
    menu_active: bool,
    engine_word_b4: i32,
    time: [i8; 3],
    record: [i8; 3],
    record_index: i32,
    completion_index: i32,
    check_return: Option<i32>,
}
#[derive(Serialize)]
struct ResultActions {
    /// Offsets relative to the original in-memory record base. No save I/O.
    writes: Vec<(i32, Vec<i8>)>,
    engine_byte_47_write: Option<i8>,
    calls: Vec<(&'static str, Vec<i32>)>,
}

fn results(r: &mut ControllerUpdate, s: ResultContext) -> Result<ResultActions> {
    r.calls.push(("engine_800629e4", None));
    r.elapsed = r.elapsed.wrapping_add(1);
    r.engine_byte_16 = 1;
    let mut a = ResultActions {
        writes: vec![],
        engine_byte_47_write: None,
        calls: vec![],
    };
    match r.elapsed {
        300 => {
            r.flag_3ce24 = 1;
            if s.engine_byte_23 != 1 {
                a.calls.push(("engine_8001ef18", vec![8]));
            }
        }
        200 if s.menu_active => r.elapsed = 199,
        60 if s.first_slot_place == Some(1) && s.multiplayer_byte == 0 && s.mode >= 2 => {
            let time_value =
                |[m, sec, cs]: [i8; 3]| i32::from(m) * 6000 + i32::from(sec) * 100 + i32::from(cs);
            if s.mode == 5 {
                if s.engine_word_b4 == 1 || time_value(s.time) >= time_value(s.record) {
                    return Ok(a);
                }
                a.writes.push((
                    s.record_index.wrapping_mul(3).wrapping_add(0x349a),
                    s.time.to_vec(),
                ));
            } else {
                a.writes.push((
                    s.completion_index.wrapping_mul(13).wrapping_add(0x2fbc),
                    vec![1],
                ));
                let [m, sec, cs] = s.time.map(i32::from);
                a.calls.push(("engine_8005a8fc", vec![0, m, sec, cs]));
            }
            a.calls.push(("engine_8005aacc", vec![0]));
            let byte = s.check_return.ok_or_else(|| {
                crate::Error::invalid("missing observed return from engine_8005aacc")
            })? as i8;
            a.engine_byte_47_write = Some(byte);
            if byte == 0 {
                a.calls.push(("engine_8005ba74", vec![]));
            }
        }
        _ => {}
    }
    Ok(a)
}

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum PresentationInput {
    Display {
        position: [f32; 3],
        mode: i32,
        shared_frame: f32,
    },
    Delete,
}

#[derive(Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum Presentation {
    Display {
        position: [f32; 3],
        rotation_y: i32,
        texture_bank: String,
        texture_list_section5: usize,
        object_section5: usize,
        motion_section5: Option<usize>,
        submitted_frame: Option<f32>,
        shared_frame: f32,
    },
    Delete {
        task_38: u32,
        task_40: u32,
    },
}

/// CartD 0xdbe0 and 0xdd00. Offsets identify native resources without exporting them.
pub fn presentation(bytes: &[u8], input: PresentationInput) -> Result<Presentation> {
    kart_progress::verify(bytes)?;
    match input {
        PresentationInput::Delete => Ok(Presentation::Delete {
            task_38: 0,
            task_40: 0,
        }),
        PresentationInput::Display {
            position,
            mode,
            shared_frame,
        } => {
            let rel = crate::city_rel::CityRel::read_identified(bytes, 80)?;
            let data = rel.data(5)?;
            let constants = rel.data(4)?;
            let f = |at: usize| f32::from_be_bytes(constants[at..at + 4].try_into().unwrap());
            let animated = mode != 0;
            let (texture_bank, texture_list_section5, _) = bank(&rel, mode)?;
            let (object_section5, rotation_y) = if animated {
                (0x29cc3c, 0x8000)
            } else {
                (0x290174, -0x4000)
            };
            let next = if animated {
                let frames = u32::from_be_bytes(data[0x29e5d0..0x29e5d4].try_into().unwrap());
                let advanced = shared_frame + f(0x488);
                if f64::from(advanced) >= f64::from(frames.wrapping_sub(1)) {
                    f(0x48c)
                } else {
                    advanced
                }
            } else {
                shared_frame
            };
            Ok(Presentation::Display {
                position,
                rotation_y,
                texture_bank,
                texture_list_section5,
                object_section5,
                motion_section5: animated.then_some(0x29e5cc),
                submitted_frame: animated.then_some(shared_frame),
                shared_frame: next,
            })
        }
    }
}

fn bank(rel: &crate::city_rel::CityRel<'_>, mode: i32) -> Result<(String, usize, usize)> {
    let (name, list) = if mode == 0 {
        (0xdd36, 0xdd3e)
    } else {
        (0xdd4e, 0xdd56)
    };
    let target = |site| {
        rel.reference(1, site)
            .filter(|r| r.module == 80 && r.section == 5 && r.kind == 4)
            .map(|r| r.addend as usize)
            .ok_or_else(|| crate::Error::invalid("missing pursuit texture registration"))
    };
    let data = rel.data(5)?;
    let name = std::ffi::CStr::from_bytes_until_nul(&data[target(name)?..])
        .unwrap()
        .to_str()
        .unwrap();
    let offset = target(list)?;
    let count = u32::from_be_bytes(data[offset + 4..offset + 8].try_into().unwrap());
    Ok((name.to_owned(), offset, count as usize))
}

pub fn texture_resource(bytes: &[u8], mode: i32) -> Result<(String, usize, &'static str)> {
    kart_progress::verify(bytes)?;
    let (name, _, count) = bank(&crate::city_rel::CityRel::read_identified(bytes, 80)?, mode)?;
    let hash = if mode == 0 {
        "5578ba03cce6eb265ecf2374b178749c56d94358e877fbb8eae382b752c2dea8"
    } else {
        "c09e3bab59cc6d6078899a9f4aa1a754c82651ca3ba3c862c644e553b3be4a35"
    };
    Ok((format!("{name}.prs"), count, hash))
}

/// Preserve native preorder and transform words. Geometry pointers refer to local REL data.
pub fn model(bytes: &[u8], mode: i32) -> Result<serde_json::Value> {
    use crate::Error;
    kart_progress::verify(bytes)?;
    let rel = crate::city_rel::CityRel::read_identified(bytes, 80)?;
    let site = if mode == 0 { 0xdc3a } else { 0xdc66 };
    let root = rel
        .reference(1, site)
        .filter(|r| r.module == 80 && r.kind == 4 && r.section == 5)
        .ok_or_else(|| Error::invalid("missing pursuit object reference"))?
        .addend;
    let data = rel.data(5)?;
    let mut pending = vec![(root, None::<usize>)];
    let mut seen = std::collections::BTreeSet::new();
    let mut nodes = Vec::new();
    let mut material = crate::visual::Material::default();
    while let Some((offset, parent)) = pending.pop() {
        if !seen.insert(offset) {
            return Err(Error::invalid("repeated pursuit object node"));
        }
        let row = data
            .get(offset as usize..)
            .and_then(|s| s.get(..52))
            .ok_or_else(|| Error::invalid("truncated pursuit object"))?;
        let word = |i: usize| u32::from_be_bytes(row[i..i + 4].try_into().unwrap());
        let xyz = |i: usize| {
            [
                f32::from_bits(word(i)),
                f32::from_bits(word(i + 4)),
                f32::from_bits(word(i + 8)),
            ]
        };
        let ptr = |field: u32| -> Result<Option<u32>> {
            rel.local_pointer(5, offset + field)?
                .map(|(s, o)| {
                    if s == 5 {
                        Ok(o)
                    } else {
                        Err(Error::invalid("unexpected pursuit node section"))
                    }
                })
                .transpose()
        };
        let index = nodes.len();
        nodes.push(serde_json::json!({"parent":parent,"offset":offset,"flags":word(0),"attach":ptr(4)?,
            "position":xyz(8),"angles":[word(20) as i32,word(24) as i32,word(28) as i32],"scale":xyz(32)}));
        if let Some(attach) = ptr(4)? {
            nodes[index]["geometry"] = vertices(&rel, attach, &mut material)?;
        }
        if let Some(sibling) = ptr(48)? {
            pending.push((sibling, parent));
        }
        if let Some(child) = ptr(44)? {
            pending.push((child, Some(index)));
        }
    }
    let motion = if mode == 0 {
        None
    } else {
        Some(bind_motion(&rel, &mut nodes)?)
    };
    let (texture_bank, _, texture_count) = bank(&rel, mode)?;
    Ok(
        serde_json::json!({"root":root,"section":5,"nodes":nodes,"motion":motion,"textureBank":texture_bank,"textureCount":texture_count}),
    )
}

fn bind_motion(
    rel: &crate::city_rel::CityRel<'_>,
    nodes: &mut [serde_json::Value],
) -> Result<serde_json::Value> {
    let invalid = || crate::Error::invalid("unexpected pursuit motion layout");
    let data = rel.data(5)?;
    let word = |at: usize| -> Result<u32> {
        Ok(u32::from_be_bytes(
            data.get(at..at + 4)
                .ok_or_else(invalid)?
                .try_into()
                .unwrap(),
        ))
    };
    let ptr = |at: usize| -> Result<usize> {
        rel.local_pointer(5, at as u32)?
            .filter(|(s, _)| *s == 5)
            .map(|(_, o)| o as usize)
            .ok_or_else(invalid)
    };
    let offset = rel
        .reference(1, 0xdc6e)
        .filter(|r| r.module == 80 && r.kind == 4 && r.section == 5)
        .ok_or_else(invalid)?
        .addend as usize;
    let table = ptr(offset)?;
    let frames = word(offset + 4)?;
    if word(offset + 8)? != 0x00070003 || table + nodes.len() * 24 != offset {
        return Err(invalid());
    }
    for (i, node) in nodes.iter_mut().enumerate() {
        let row = table + i * 24;
        for (j, name) in ["position", "angles", "scale"].iter().enumerate() {
            let start = ptr(row + 4 * j)?;
            let count = word(row + 12 + 4 * j)? as usize;
            let mut keys = Vec::new();
            for k in 0..count {
                let key = start + k * 16;
                let frame = word(key)?;
                let xyz = [word(key + 4)?, word(key + 8)?, word(key + 12)?];
                let value = if j == 1 {
                    serde_json::json!(xyz.map(|v| v as i32))
                } else {
                    serde_json::json!(xyz.map(f32::from_bits))
                };
                keys.push(serde_json::json!({"frame":frame,"value":value}));
            }
            node["motion"][*name] = keys.into();
        }
    }
    Ok(serde_json::json!({"offset":offset,"frames":frames,"type":7,"interpolation_word":3}))
}

pub(crate) fn vertices(
    rel: &crate::city_rel::CityRel<'_>,
    attach: u32,
    material: &mut crate::visual::Material,
) -> Result<serde_json::Value> {
    vertices_from_data(rel, rel.data(5)?, attach, material)
}

pub(crate) fn vertices_from_data(
    rel: &crate::city_rel::CityRel<'_>,
    data: &[u8],
    attach: u32,
    material: &mut crate::visual::Material,
) -> Result<serde_json::Value> {
    let invalid = || crate::Error::invalid("unexpected pursuit vertex stream");
    let word = |at: usize| -> Result<u32> {
        Ok(u32::from_be_bytes(
            data.get(at..at + 4)
                .ok_or_else(invalid)?
                .try_into()
                .unwrap(),
        ))
    };
    let xyz = |at| -> Result<[f32; 3]> {
        Ok([word(at)?, word(at + 4)?, word(at + 8)?].map(f32::from_bits))
    };
    let mut chunks = Vec::new();
    let mut indices = std::collections::BTreeMap::new();
    if let Some((section, start)) = rel.local_pointer(5, attach)? {
        if section != 5 {
            return Err(invalid());
        }
        let mut cursor = start as usize;
        loop {
            let header = word(cursor)?;
            if header as u8 == 255 {
                break;
            }
            let stride = match header as u8 {
                35 => 4,
                41 => 6,
                _ => return Err(invalid()),
            };
            let index_word = word(cursor + 4)?;
            let count = (index_word >> 16) as usize;
            if header >> 16 != (1 + count * stride) as u32 {
                return Err(invalid());
            }
            let mut entries = Vec::with_capacity(count);
            for i in 0..count {
                let index = (index_word as u16)
                    .checked_add(i as u16)
                    .ok_or_else(invalid)?;
                indices.insert(index, usize::from(index));
                let at = cursor + 8 + i * stride * 4;
                entries.push(if stride == 6 {
                    serde_json::json!({"position":xyz(at)?,"normal":xyz(at+12)?})
                } else {
                    let [a, r, g, b] = word(at + 12)?.to_be_bytes();
                    serde_json::json!({"position":xyz(at)?,"diffuseRgba":[r,g,b,a]})
                });
            }
            chunks.push(serde_json::json!({"flags":(header>>8) as u8,"index_offset":index_word as u16,"vertices":entries}));
            cursor += 8 + count * stride * 4;
        }
    }
    Ok(
        serde_json::json!({"center":xyz(attach as usize+8)?,"radius":f32::from_bits(word(attach as usize+20)?),"chunks":chunks,"polygons":polygons(rel,data,attach,&indices,material)?}),
    )
}

fn polygons(
    rel: &crate::city_rel::CityRel<'_>,
    data: &[u8],
    attach: u32,
    indices: &std::collections::BTreeMap<u16, usize>,
    material: &mut crate::visual::Material,
) -> Result<Vec<serde_json::Value>> {
    let invalid = || crate::Error::invalid("unexpected pursuit polygon stream");
    let mut result = Vec::new();
    let Some((5, start)) = rel.local_pointer(5, attach + 4)? else {
        return Err(invalid());
    };

    let word = |at: usize| -> Result<u16> {
        Ok(u16::from_be_bytes(
            data.get(at..at + 2)
                .ok_or_else(invalid)?
                .try_into()
                .unwrap(),
        ))
    };
    let mut at = start as usize;
    loop {
        let header = word(at)?;
        let kind = header as u8;
        let words = match kind {
            255 => break,
            8 => 2,
            17..=21 | 23 | 64 | 65 => 2 + usize::from(word(at + 2)?),
            _ => return Err(invalid()),
        };
        let raw = (0..words)
            .map(|i| word(at + 2 * i))
            .collect::<Result<Vec<_>>>()?;
        let diffuse_rgba = if (16..32).contains(&kind) && kind & 1 != 0 {
            if raw.len() < 4 {
                return Err(invalid());
            }
            let [b, g] = raw[2].to_le_bytes();
            let [r, a] = raw[3].to_le_bytes();
            Some([r, g, b, a])
        } else {
            None
        };
        let chunk = crate::model::PolygonChunk {
            offset: at as u32,
            kind,
            flags: (header >> 8) as u8,
            length: words * 2,
            texture_id: (kind == 8).then(|| raw[1] & 0x1fff),
            diffuse_rgba,
            raw_words: raw,
        };
        let strips = if kind >= 64 {
            Some(crate::visual::read_strips(&chunk, indices)?)
        } else {
            None
        };
        let mut command = serde_json::json!({"chunk":chunk,"strips":strips});
        if kind >= 64 {
            command["material"] = serde_json::to_value(&*material)?;
        } else {
            material.apply(&chunk)?;
        }
        result.push(command);
        at += words * 2;
    }
    Ok(result)
}
