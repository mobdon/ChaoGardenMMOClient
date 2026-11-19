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
}
#[derive(Serialize)]
pub struct GateResult {
    pub latch: u16,
    pub timer_word: u32,
    pub primary_kart_counter: u32,
    pub work_byte_3: u8,
    /// Calls are listed in execution order. Local addresses are section-1 offsets.
    pub calls: Vec<&'static str>,
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
