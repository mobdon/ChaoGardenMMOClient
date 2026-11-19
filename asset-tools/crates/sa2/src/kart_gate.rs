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
pub fn time_extension(bytes: &[u8]) -> Result<serde_json::Value> {
    let rel = verified_module(bytes)?;
    for (site, target) in [(0xc83a, 0xc858), (0xc9ce, 0xca10)] {
        if !rel
            .reference(1, site)
            .is_some_and(|r| (r.module, r.section, r.kind, r.addend) == (80, 1, 4, target))
        {
            return Err(Error::invalid("unexpected time extension callback"));
        }
    }
    let mut state = TimeExtension::default();
    let mut frames = Vec::new();
    for _ in 0..256 {
        let delete = state.tick();
        frames.push(serde_json::json!({"state":state,"delete":delete}));
        if delete {
            return Ok(serde_json::json!({"frames":frames}));
        }
    }
    Err(Error::invalid("time extension task did not release"))
}
