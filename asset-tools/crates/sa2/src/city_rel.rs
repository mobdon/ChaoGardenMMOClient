//! File-backed sections and relocation references of the pinned Steam stage modules.
use crate::{manifest, Error, Result};
use serde::Serialize;
use std::collections::BTreeMap;

pub const SHA256: &str = "bcb2f3e2dbead7bafebcda84063fba54bac831cbd9102fd0230d78a5b88b570b";
fn invalid() -> Error {
    Error::invalid("invalid or unsupported installed REL reference")
}

#[derive(Clone, Copy, Serialize)]
pub struct Section {
    pub file_offset: u32,
    pub size: u32,
    pub executable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Reference {
    pub module: u32,
    pub kind: u8,
    pub section: u8,
    pub addend: u32,
}

pub struct CityRel<'a> {
    bytes: &'a [u8],
    module: u32,
    sections: Vec<Section>,
    refs: BTreeMap<(u8, u32), Reference>,
}

impl<'a> CityRel<'a> {
    /// Distinct from the GSNE8P decompilation's expected disc build.
    pub fn read(bytes: &'a [u8]) -> Result<Self> {
        if bytes.len() != 3_304_912 || manifest::sha256(bytes) != SHA256 {
            return Err(Error::invalid("unsupported City Escape REL revision"));
        }
        Self::read_identified(bytes, 12)
    }
    /// Caller must first identify the released module by pinned digest.
    pub(crate) fn read_identified(bytes: &'a [u8], module: u32) -> Result<Self> {
        let word = |at: usize| -> Result<u32> {
            Ok(u32::from_be_bytes(
                bytes
                    .get(at..at + 4)
                    .ok_or_else(invalid)?
                    .try_into()
                    .unwrap(),
            ))
        };
        if word(0)? != module || word(28)? != 2 {
            return Err(invalid());
        }
        let dir = word(16)? as usize;
        let mut sections = Vec::new();
        for i in 0..word(12)? as usize {
            let flags = word(dir + i * 8)?;
            let section = Section {
                file_offset: flags & !1,
                size: word(dir + i * 8 + 4)?,
                executable: flags & 1 != 0,
            };
            if section.file_offset != 0
                && u64::from(section.file_offset) + u64::from(section.size) > bytes.len() as u64
            {
                return Err(invalid());
            }
            sections.push(section);
        }
        let mut refs = BTreeMap::new();
        let imports = word(40)? as usize;
        for off in (imports..imports + word(44)? as usize).step_by(8) {
            let imported_module = word(off)?;
            let mut stream = word(off + 4)? as usize;
            let mut src = None;
            let mut loc = 0u32;
            loop {
                let r = bytes.get(stream..stream + 8).ok_or_else(invalid)?;
                stream += 8;
                let (kind, section) = (r[2], r[3]);
                match kind {
                    203 => break, // END
                    202 => {
                        src = Some(section);
                        loc = 0;
                        continue;
                    } // SECTION
                    _ => {}
                }
                loc = loc
                    .checked_add(u16::from_be_bytes([r[0], r[1]]) as u32)
                    .ok_or_else(invalid)?;
                if kind == 201 {
                    continue;
                } // NOP advances the running offset
                let src = src.ok_or_else(invalid)?;
                let width = match kind {
                    1 | 10 => 4,
                    4 | 6 => 2,
                    _ => return Err(invalid()),
                };
                if loc.checked_add(width).ok_or_else(invalid)?
                    > sections.get(src as usize).ok_or_else(invalid)?.size
                {
                    return Err(invalid());
                }
                let addend = u32::from_be_bytes(r[4..].try_into().unwrap());
                if imported_module == module
                    && addend >= sections.get(section as usize).ok_or_else(invalid)?.size
                {
                    return Err(invalid());
                }
                if refs
                    .insert(
                        (src, loc),
                        Reference {
                            module: imported_module,
                            kind,
                            section,
                            addend,
                        },
                    )
                    .is_some()
                {
                    return Err(invalid());
                }
            }
        }
        Ok(Self {
            bytes,
            module,
            sections,
            refs,
        })
    }
    pub fn sections(&self) -> &[Section] {
        &self.sections
    }
    pub fn data(&self, section: u8) -> Result<&'a [u8]> {
        let s = self.sections.get(section as usize).ok_or_else(invalid)?;
        if s.file_offset == 0 {
            return Err(Error::invalid("REL section has no file data"));
        }
        Ok(&self.bytes[s.file_offset as usize..s.file_offset as usize + s.size as usize])
    }
    pub fn reference(&self, section: u8, offset: u32) -> Option<Reference> {
        self.refs.get(&(section, offset)).copied()
    }
    /// A missing relocation is a null pointer only if the source word is zero.
    /// Imported engine objects and instruction relocations are not local data pointers.
    pub fn local_pointer(&self, section: u8, offset: u32) -> Result<Option<(u8, u32)>> {
        let at = offset as usize;
        let word = self
            .data(section)?
            .get(at..at.checked_add(4).ok_or_else(invalid)?)
            .ok_or_else(invalid)?;
        match self.reference(section, offset) {
            Some(Reference {
                module,
                kind: 1,
                section,
                addend,
            }) if module == self.module => Ok(Some((section, addend))),
            None if word == [0; 4] => Ok(None),
            _ => Err(invalid()),
        }
    }
    /// Verify the paired HA/LO references used in the compiled resource selector.
    fn resource(&self, at: u32) -> Result<u32> {
        match (self.reference(1, at), self.reference(1, at + 4)) {
            (
                Some(Reference {
                    module: 12,
                    section: 5,
                    kind: 6,
                    addend: a,
                }),
                Some(Reference {
                    module: 12,
                    section: 5,
                    kind: 4,
                    addend: b,
                }),
            ) if a == b => Ok(a),
            _ => Err(invalid()),
        }
    }
    pub fn board_resources(&self) -> Result<Vec<BoardResources>> {
        let data = self.data(5)?;
        let local = |o| {
            self.local_pointer(5, o)?
                .filter(|(s, _)| *s == 5)
                .map(|(_, v)| v)
                .ok_or_else(invalid)
        };
        let mut result = Vec::new();
        for (variant, texture, model, other) in [
            ("single_player", 0x222, 0x23a, 0x22e),
            ("two_player_amy", 0x28a, 0x296, 0x2a2),
            ("two_player_metal_sonic", 0x2b2, 0x2be, 0x2ca),
            ("two_player_shadow", 0x2da, 0x2e6, 0x2f2),
            ("two_player_default", 0x302, 0x30e, 0x31a),
        ] {
            let texture = self.resource(texture)?;
            let names = local(texture)?;
            let i = texture as usize + 4;
            let count =
                u32::from_be_bytes(data.get(i..i + 4).ok_or_else(invalid)?.try_into().unwrap());
            if count > 256 {
                return Err(invalid());
            }
            let mut textures = Vec::new();
            for j in 0..count {
                let at = local(names + 12 * j)? as usize;
                let tail = data.get(at..).ok_or_else(invalid)?;
                let end = tail
                    .iter()
                    .take(256)
                    .position(|b| *b == 0)
                    .ok_or_else(invalid)?;
                textures.push(
                    std::str::from_utf8(&tail[..end])
                        .map_err(|_| invalid())?
                        .to_owned(),
                );
            }
            result.push(BoardResources {
                variant,
                texture_list: texture,
                model: self.resource(model)?,
                other_object: self.resource(other)?,
                textures,
            });
        }
        Ok(result)
    }
}

#[derive(Serialize)]
pub struct BoardResources {
    pub variant: &'static str,
    /// All three offsets are relative to data section 5.
    pub texture_list: u32,
    pub model: u32,
    pub other_object: u32,
    pub textures: Vec<String>,
}
/// Recover the descriptor passed by the standard stage registration prolog.
/// Offsets refer to REL sections, never process addresses in the Rust host.
pub fn stage_registration(bytes: &[u8]) -> Result<serde_json::Value> {
    let digest = manifest::sha256(bytes);
    // Stage 42 has a registration descriptor but no mapped terrain in the land catalog.
    let module = if bytes.len() == 345422
        && digest == "51d7b743c95cc8533e402533d73d43abab04c4de2e08770d615e143e1925385d"
    {
        41
    } else {
        crate::stage_land::catalog()
            .iter()
            .find(|s| s.filename.ends_with(".rel") && s.size == bytes.len() && s.sha256 == digest)
            .ok_or_else(invalid)?
            .module
    };
    let rel = CityRel::read_identified(bytes, module)?;
    let entry = u32::from_be_bytes(bytes[52..56].try_into().unwrap());
    if bytes[48] != 1 {
        return Err(invalid());
    }
    let code = rel.data(1)?;
    let body = code
        .get(entry as usize..)
        .and_then(|b| b.get(..40))
        .ok_or_else(invalid)?;
    for (word, expected) in body.as_chunks::<4>().0.iter().zip([
        0x7c0802a6, 0x90010004, 0x9421fff8, 0x3c600000, 0x38630000, 0x48000001, 0x8001000c,
        0x38210008, 0x7c0803a6, 0x4e800020,
    ]) {
        if u32::from_be_bytes(*word) != expected {
            return Err(invalid());
        }
    }
    let hi = rel.reference(1, entry + 14).ok_or_else(invalid)?;
    let lo = rel.reference(1, entry + 18).ok_or_else(invalid)?;
    let call = rel.reference(1, entry + 20).ok_or_else(invalid)?;
    if (hi.module, hi.section, hi.kind) != (module, 5, 6)
        || lo != (Reference { kind: 4, ..hi })
        || (call.module, call.kind, call.addend) != (0, 10, 0x8001a01c)
    {
        return Err(invalid());
    }
    let descriptor = hi.addend;
    let data = rel.data(5)?;
    let label = data
        .get(descriptor as usize..)
        .and_then(|b| b.get(..12))
        .ok_or_else(invalid)?;
    let end = label.iter().position(|b| *b == 0).ok_or_else(invalid)?;
    let name = std::str::from_utf8(&label[..end]).map_err(|_| invalid())?;
    let callback = |delta| -> Result<Option<u32>> {
        match rel.local_pointer(5, descriptor + delta)? {
            Some((1, off)) if off % 4 == 0 && (off as usize) < code.len() => Ok(Some(off)),
            None => Ok(None),
            _ => Err(invalid()),
        }
    };
    let mut registration = serde_json::json!({"module":module,"prolog":entry,"descriptor":descriptor,"name":name,
        "registerTarget":call.addend,"startup":callback(12)?,"teardown":callback(16)?,"entry":callback(20)?});
    if module == 82 {
        // Section 1:6f64 assembles a stack vector for the direct call at 6fa8.
        // The SHA-256 gate above pins the instructions and register assignments.
        let pair = |at, section| -> Result<u32> {
            let hi = rel.reference(1, at).ok_or_else(invalid)?;
            let lo = rel.reference(1, at + 4).ok_or_else(invalid)?;
            if (hi.module, hi.section, hi.kind) != (module, section, 6)
                || lo != (Reference { kind: 4, ..hi })
            {
                return Err(invalid());
            }
            Ok(hi.addend)
        };
        let scalar = |at| -> Result<f32> {
            let off = pair(at, 4)? as usize;
            Ok(f32::from_be_bytes(
                rel.data(4)?
                    .get(off..off + 4)
                    .ok_or_else(invalid)?
                    .try_into()
                    .unwrap(),
            ))
        };
        let x = scalar(0x6f7a)?;
        let yz = scalar(0x6f8a)?;
        registration["bossTaskSetup"] = serde_json::json!({
            "constructorSection1Offset":0, "position":[x,yz,yz], "yaw":-16384,
            "nextTaskSection1Offset":pair(0x6fae, 1)?
        });
        // The executor indexes a relocated jump table with the signed mode byte.
        // Four entries jump directly to the common continuation.
        let table = pair(0x5bd2, 5)?;
        let mut handlers = Vec::new();
        for index in 0..16 {
            let (1, arm) = rel
                .local_pointer(5, table + index * 4)?
                .ok_or_else(invalid)?
            else {
                return Err(invalid());
            };
            if arm == 0x5c74 {
                handlers.push(None);
                continue;
            }
            let pc = arm as usize + 4;
            let branch = u32::from_be_bytes(
                code.get(pc..pc + 4)
                    .ok_or_else(invalid)?
                    .try_into()
                    .unwrap(),
            );
            if branch & 0xfc00_0003 != 0x4800_0001 {
                return Err(invalid());
            }
            let displacement = ((branch << 6) as i32 >> 6) & !3;
            let target = (pc as u32)
                .checked_add_signed(displacement)
                .ok_or_else(invalid)?;
            if target as usize >= code.len() {
                return Err(invalid());
            }
            handlers.push(Some(target));
        }
        registration["modeHandlerSection1Offsets"] = serde_json::json!(handlers);
        // Intro routine 1:c80, submode zero: three scalar stores to slot zero.
        let v = scalar(0xd42)?;
        registration["introPlayerWrite"] = serde_json::json!({"slot":0,"position":[v,v,v]});
    }
    Ok(registration)
}

/// Run Hot Shot's slot-zero stage constraint (section 1:7050 -> 6fcc).
pub fn hot_shot_arena(bytes: &[u8], mut position: [f32; 3]) -> Result<[f32; 3]> {
    identify_hot_shot(bytes)?;
    if !position.iter().all(|v| v.is_finite()) {
        return Err(Error::invalid("expected finite player coordinates"));
    }
    let rel = CityRel::read_identified(bytes, 82)?;
    let bound = |offset| -> Result<f32> {
        let hi = rel.reference(1, offset).ok_or_else(invalid)?;
        if (hi.module, hi.section, hi.kind) != (82, 4, 6)
            || rel.reference(1, offset + 4) != Some(Reference { kind: 4, ..hi })
        {
            return Err(invalid());
        }
        let start = hi.addend as usize;
        Ok(f32::from_be_bytes(
            rel.data(4)?
                .get(start..start + 4)
                .ok_or_else(invalid)?
                .try_into()
                .unwrap(),
        ))
    };
    for (axis, lo, hi) in [(0, 0x6ffe, 0x6fe2), (2, 0x7036, 0x701a)] {
        position[axis] = position[axis].clamp(bound(lo)?, bound(hi)?);
    }
    Ok(position)
}

fn identify_hot_shot(bytes: &[u8]) -> Result<()> {
    if bytes.len() != 1_180_821
        || manifest::sha256(bytes)
            != "9e0caebeefbdeb2e2829f17510b44d9238956342b3eed10a2ae7e1ff56358960"
    {
        return Err(invalid());
    }
    Ok(())
}

/// State after N consecutive intro callbacks beginning in submode zero.
/// Engine calls are reported at their call sites; their effects are not simulated.
pub fn hot_shot_intro_clock(bytes: &[u8], updates: u16) -> Result<serde_json::Value> {
    identify_hot_shot(bytes)?;
    let rel = CityRel::read_identified(bytes, 82)?;
    let code = rel.data(1)?;
    let immediate = |at: usize| u16::from_be_bytes([code[at + 2], code[at + 3]]);
    let initial = immediate(0xdb8);
    if updates == 0 || updates > initial + 1 {
        return Err(Error::invalid(
            "expected 1 through 451 consecutive intro updates",
        ));
    }
    let timer = initial - (updates - 1);
    let exited = timer == 0;
    Ok(serde_json::json!({
        "updates":updates, "mode":if exited {2} else {1},
        "submode":if exited {0} else {1}, "timer":timer,
        "callsEngine8006ab94":timer < immediate(0xeb8) && timer > immediate(0xec0),
        "entersNextMode":exited
    }))
}

/// Replay the submode-specific opening of mode two, stopping before the shared tail.
pub fn hot_shot_mode_two_opening(
    bytes: &[u8],
    polls: u16,
    ready_on_last: bool,
) -> Result<serde_json::Value> {
    identify_hot_shot(bytes)?;
    if ready_on_last && polls == 0 {
        return Err(Error::invalid(
            "readiness requires at least one submode-one poll",
        ));
    }
    let rel = CityRel::read_identified(bytes, 82)?;
    let scalar = |at| hot_shot_scalar(&rel, at);
    let mut amount = scalar(0x1142)?;
    let step = scalar(0x11b2)?;
    let cap = scalar(0x11ca)?;
    for _ in 0..polls {
        amount = (amount + step).min(cap);
    }
    Ok(serde_json::json!({
        "mode":2,"submode":if ready_on_last {2} else {1},
        "timer":0u16.wrapping_sub(polls), "workB8":amount,
        "entryWork58":scalar(0x1132)?, "entryRequest":8,
        "lastLocal790Vector":(polls != 0).then_some([amount;3]),
        "readyBranchWork10Write":if ready_on_last {Some(scalar(0x120e)?)} else {None},
        "scope":"Submode-specific writes before the common tail; engine readiness is supplied and helper effects are not simulated."
    }))
}

fn hot_shot_scalar(rel: &CityRel, at: u32) -> Result<f32> {
    let data = rel.data(4)?;
    let hi = rel.reference(1, at).ok_or_else(invalid)?;
    if (hi.module, hi.section, hi.kind) != (82, 4, 6)
        || rel.reference(1, at + 4) != Some(Reference { kind: 4, ..hi })
    {
        return Err(invalid());
    }
    let offset = hi.addend as usize;
    Ok(f32::from_be_bytes(
        data.get(offset..offset + 4)
            .ok_or_else(invalid)?
            .try_into()
            .unwrap(),
    ))
}

/// One mode-two, submode-two tick. External readiness is observed only when queried.
pub fn hot_shot_mode_two_step(bytes: &[u8], request: &[u8]) -> Result<serde_json::Value> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct State {
        work04: f32,
        work10: f32,
        work_bc: u8,
        flags: u16,
        controller_state: i32,
        ready: Option<bool>,
    }
    let s: State = serde_json::from_slice(request)?;
    if s.ready.is_some() != (s.controller_state == 1)
        || s.work_bc >= 15
        || !s.work04.is_finite()
        || !s.work10.is_finite()
    {
        return Err(Error::invalid(
            "expected finite work fields, counter 0..14 and readiness only for controller state 1",
        ));
    }
    identify_hot_shot(bytes)?;
    let rel = CityRel::read_identified(bytes, 82)?;
    let scalar = |at| hot_shot_scalar(&rel, at);
    let request_two = s.ready == Some(true);
    let flags = if request_two { s.flags & !1 } else { s.flags };
    let mut work10 = s.work10 * scalar(0x12b6)? + scalar(0x12ce)?;
    let work04 = s.work04 * scalar(0x12e6)?;
    if flags & 1 == 0 {
        work10 += scalar(0x15be)?;
    }
    let pair = s.work_bc > 0 && s.work_bc % 5 == 0;
    let next = if s.work_bc == 0 || s.work_bc == 14 {
        0
    } else {
        s.work_bc + 1
    };
    Ok(
        serde_json::json!({"mode":2,"submode":if work04<0. {3} else {2},
        "work04":work04,"work10":work10,"workBC":next,"flags":flags,
        "requestedControllerState":request_two.then_some(2),"local51ee0Calls":if pair {2} else {0},
        "scope":"Scalar and control writes; helper effects and subsequent boss executor work are excluded."}),
    )
}

/// Mode 2 submode 4 writes through the jump to the shared tail at 1:15a8.
pub fn hot_shot_mode_two_exit(bytes: &[u8], request: &[u8]) -> Result<serde_json::Value> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct State {
        x: f32,
        private28: f32,
        work_angles: [u32; 3],
        flags: u16,
        timer: u16,
        ready: bool,
    }
    let s: State = serde_json::from_slice(request)?;
    if !s.x.is_finite() || !s.private28.is_finite() {
        return Err(Error::invalid("expected finite boss state"));
    }
    identify_hot_shot(bytes)?;
    let rel = CityRel::read_identified(bytes, 82)?;
    let scalar = |at| hot_shot_scalar(&rel, at);
    let mode = if !s.ready {
        2
    } else if s.private28 < scalar(0x1542)? {
        9
    } else {
        3
    };
    let angles = [0x210, 0x250, 0x300]
        .into_iter()
        .zip(s.work_angles)
        .map(|(inc, v)| v.wrapping_add(inc))
        .collect::<Vec<_>>();
    Ok(
        serde_json::json!({"mode":mode,"submode":if s.ready {0} else {4},
        "workAngles":angles,"work00":if s.x>scalar(0x14ea)? {scalar(0x14fe)?} else {scalar(0x1512)?},
        "flags":if s.ready {s.flags & !0x200} else {s.flags},
        "timer":if s.ready {0} else {s.timer}, "workByte3Write":s.ready.then_some(0),
        "privateByte2Write":(mode==3).then_some(2),"requestedControllerState":s.ready.then_some(3),
        "scope":"Submode four writes before shared tail; readiness is supplied, helper effects are not simulated."}),
    )
}

/// Mode 2 submode 3, up to the shared tail, using the position observed after helper calls.
pub fn hot_shot_settle(bytes: &[u8], request: &[u8]) -> Result<serde_json::Value> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct State {
        y: f32,
        work04: f32,
        work10: f32,
        angles: [u32; 2],
        flags: u16,
        timer: u16,
        controller_state: i32,
        ready: Option<bool>,
    }
    let s: State = serde_json::from_slice(request)?;
    if ![s.y, s.work04, s.work10].iter().all(|v| v.is_finite())
        || (s.controller_state == 1) != s.ready.is_some()
    {
        return Err(Error::invalid(
            "expected finite state and readiness only for controller state one",
        ));
    }
    identify_hot_shot(bytes)?;
    let rel = CityRel::read_identified(bytes, 82)?;
    let c = |at| hot_shot_scalar(&rel, at);
    let target = c(0x13ce)?;
    let delta = (s.y - target).clamp(c(0x1412)?, c(0x13f6)?);
    let damped = s.work04 * c(0x13e2)?;
    let corrected = s.work10 - c(0x142e)? * delta;
    let (lo, hi) = (c(0x1446)?, c(0x145a)?);
    let settled = delta > lo && delta < hi && damped > lo && damped < hi;
    let ready = s.ready == Some(true);
    Ok(
        serde_json::json!({"mode":2,"submode":if settled {4} else {3},
        "timer":if settled {30} else {s.timer},"y":if settled {target} else {s.y},
        "work04":if settled {0.} else {damped},"work10":if settled {0.} else {corrected},
        "angles":[s.angles[0].wrapping_add(0x210),s.angles[1].wrapping_add(0x250)],
        "flags":if ready {s.flags & !1} else {s.flags},"requestedControllerState":ready.then_some(2),
        "scope":"Submode-three writes before shared tail; position is observed after helpers."}),
    )
}

/// Control, timer and common arithmetic of executor mode 9 (section 1:4ae0).
pub fn hot_shot_mode_nine(bytes: &[u8], request: &[u8]) -> Result<serde_json::Value> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct State {
        submode: u8,
        timer: u16,
        remaining: u8,
        private28: f32,
        flags: u16,
        collision_flags: u32,
        work10: f32,
        angles: [u32; 3],
        entry_player_position: Option<[f32; 3]>,
    }
    let mut s: State = serde_json::from_slice(request)?;
    if s.submode > 4 || !s.private28.is_finite() || !s.work10.is_finite() {
        return Err(Error::invalid(
            "expected a mode-nine submode 0..4 and finite work fields",
        ));
    }
    identify_hot_shot(bytes)?;
    let rel = CityRel::read_identified(bytes, 82)?;
    let target = if let Some(pos) = s.entry_player_position {
        if s.submode != 0 || !pos.iter().all(|v| v.is_finite()) {
            return Err(Error::invalid(
                "target construction requires a finite player position on mode-nine entry",
            ));
        }
        let address = |at, section| -> Result<u32> {
            let hi = rel.reference(1, at).ok_or_else(invalid)?;
            if (hi.module, hi.section, hi.kind) != (82, section, 6)
                || rel.reference(1, at + 4) != Some(Reference { kind: 4, ..hi })
            {
                return Err(invalid());
            }
            Ok(hi.addend)
        };
        let unit = hot_shot_scalar(&rel, 0x4ce3e)?;
        Some(serde_json::json!({
            "sourcePlayerSlot":0,"position":pos,"savedPosition":pos,"scale":vec![unit;3],
            "work3c":hot_shot_scalar(&rel,0x4b86)?,"work44":hot_shot_scalar(&rel,0x4ce56)?,
            "updateSection1":address(0x4cdb6,1)?,"displaySection1":address(0x4ce92,1)?,
            "destroySection1":address(0x4ce9e,1)?,"collisionSection5":address(0x4ce6a,5)?,
            "workSize":84,"bossWork54":pos
        }))
    } else {
        None
    };
    let mut mode = 9;
    let mut next_variant = None;
    // Entry deliberately falls through into the first countdown on this same update.
    if s.submode == 0 {
        s.remaining = if s.private28 > hot_shot_scalar(&rel, 0x4b42)? {
            1
        } else {
            3
        };
        s.timer = 300;
        s.collision_flags &= !0x10;
        s.submode = 1;
    }
    match s.submode {
        1 => {
            s.timer = s.timer.wrapping_sub(1);
            if s.timer == 0 {
                s.submode = 2;
            }
        }
        2 => s.submode = 3,
        3 => {
            s.remaining = s.remaining.wrapping_sub(1);
            if s.remaining == 0 {
                s.timer = 60;
                s.submode = 4;
            } else {
                s.timer = 300;
                s.submode = 1;
            }
        }
        4 => {
            s.timer = s.timer.wrapping_sub(1);
            if s.timer == 0 {
                next_variant = Some(if s.private28 > hot_shot_scalar(&rel, 0x4d26)? {
                    2
                } else {
                    1
                });
                mode = 3;
                s.submode = 0;
                s.flags &= !0x200;
                s.collision_flags |= 0x10;
            }
        }
        _ => unreachable!(),
    }
    Ok(
        serde_json::json!({"mode":mode,"submode":s.submode,"timer":s.timer,"remaining":s.remaining,
        "targetOnSuccessfulAllocation":target,"privateByte2Write":next_variant,"flags":s.flags,"collisionFlags":s.collision_flags,
        "work10":s.work10+hot_shot_scalar(&rel,0x4dc6)?,
        "angles":[s.angles[0].wrapping_add(0x210),s.angles[1].wrapping_add(0x250),s.angles[2].wrapping_add(0x300)],
        "scope":"Mode-nine timing and selected writes. Target tracking, helper lifetimes and attack effects are not simulated."}),
    )
}

/// Tracking helper at 1:4cef4, called on the enabled, unlocked target update path.
pub fn hot_shot_target_follow(bytes: &[u8], request: &[u8]) -> Result<serde_json::Value> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct State {
        position: [f32; 3],
        saved_position: [f32; 3],
        player_position: [f32; 3],
        engine_length: f32,
        normalized: Option<[f32; 3]>,
    }
    let s: State = serde_json::from_slice(request)?;
    identify_hot_shot(bytes)?;
    let rel = CityRel::read_identified(bytes, 82)?;
    let speed = hot_shot_scalar(&rel, 0x4b86)?;
    let limited = s.engine_length > speed;
    if !s.engine_length.is_finite()
        || s.engine_length < 0.
        || limited != s.normalized.is_some()
        || !s
            .position
            .iter()
            .chain(&s.saved_position)
            .chain(&s.player_position)
            .chain(s.normalized.iter().flatten())
            .all(|v| v.is_finite())
    {
        return Err(Error::invalid(
            "expected finite tracking state and a normalization result only on the limited branch",
        ));
    }
    let mut delta: [f32; 3] = std::array::from_fn(|i| s.saved_position[i] - s.position[i]);
    if let Some(direction) = s.normalized {
        let [x, y, z] = delta;
        let squared = z * z + (x * x + y * y);
        if !squared.is_finite() {
            return Err(Error::invalid("target distance overflow"));
        }
        let step = if squared.sqrt() > hot_shot_scalar(&rel, 0x4cf7e)? {
            speed * hot_shot_scalar(&rel, 0x4cf92)?
        } else {
            speed
        };
        delta = direction.map(|v| v * step);
    }
    let position: [f32; 3] = std::array::from_fn(|i| s.position[i] + delta[i]);
    if !position.iter().all(|v| v.is_finite()) {
        return Err(Error::invalid("target position overflow"));
    }
    Ok(
        serde_json::json!({"position":position,"savedPosition":s.player_position,
        "limited":limited,"scope":"Unlocked target movement; imported length and normalization results are supplied."}),
    )
}
