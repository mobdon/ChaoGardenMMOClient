//! The o_ring_8 selection and request to the external CalcAdvanceAsPossible routine.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Player {
    pub position: [f32; 3],
    pub collision_center: [f32; 3],
    pub speed: [f32; 3],
}
#[derive(Debug, PartialEq, Serialize)]
pub struct Advance {
    pub center: [f32; 3],
    pub step: f32,
}
#[derive(Debug, PartialEq, Serialize)]
pub struct Attraction {
    pub slot: usize,
    pub advance: Option<Advance>,
}
fn norm(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}
pub fn select(
    position: [f32; 3],
    timer: u16,
    items: [Option<u16>; 2],
    players: [Option<Player>; 2],
) -> Result<Option<Attraction>> {
    let mut eligible = [None; 2];
    for i in 0..2 {
        if items[i].is_some_and(|v| v & 2 != 0) {
            let p = players[i]
                .ok_or_else(|| Error::invalid("missing magnetic player's native observations"))?;
            if !position
                .iter()
                .chain(&p.position)
                .chain(&p.speed)
                .chain(&p.collision_center)
                .all(|v| v.is_finite())
            {
                return Err(Error::invalid("nonfinite magnetic ring observation"));
            }
            let distance = norm(std::array::from_fn(|axis| {
                position[axis] - p.position[axis]
            }));
            eligible[i] = Some((distance, p));
        }
    }
    let (slot, d, p) = match eligible {
        [Some((a, p)), Some((b, _))] if a < b => (0, a, p),
        [Some((a, _)), Some((b, p))] if a > b => (1, b, p),
        [Some((d, p)), None] => (0, d, p),
        [None, Some((d, p))] => (1, d, p),
        _ => return Ok(None),
    };
    let advance = if d < 50. || timer >= 1 {
        let step = (d.min(50.) * 1.3 / 50.).clamp(0.85, 5.) * (1. + norm(p.speed) * 0.5);
        if !step.is_finite() {
            return Err(Error::invalid("nonfinite magnetic advance"));
        }
        Some(Advance {
            center: p.collision_center,
            step,
        })
    } else {
        None
    };
    Ok(Some(Attraction { slot, advance }))
}
