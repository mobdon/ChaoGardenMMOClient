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
pub fn tick(bytes: &[u8], s: GateTick) -> Result<GateResult> {
    if manifest::sha256(bytes) != "71826645bae2a4064cd5ad4402facaf4da6c5caee0be7e19894ed7ceb2c2ee6c"
    {
        return Err(Error::invalid("unsupported CartD.rel revision"));
    }
    let rel = CityRel::read_identified(bytes, 80)?;
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
