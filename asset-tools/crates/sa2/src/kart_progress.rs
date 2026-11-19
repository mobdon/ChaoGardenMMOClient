//! One occupied kart slot of the CartD course-index update (0x885C–0x8D60).
use crate::{manifest, Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub course_length: i32,
    pub piece: i32,
    pub counter: i32,
    pub highest: i32,
    pub work_44: u32,
    pub mode: i32,
    pub debug_enabled: bool,
    /// Results from the native segment test (0x9220), in invocation order.
    pub segment_tests: Vec<bool>,
}
#[derive(Serialize)]
pub struct Update {
    pub piece: i32,
    pub counter: i32,
    pub highest: i32,
    pub tested_indices: Vec<i32>,
    pub debug_indices: Vec<i32>,
}
fn verify(bytes: &[u8]) -> Result<()> {
    if manifest::sha256(bytes) != "71826645bae2a4064cd5ad4402facaf4da6c5caee0be7e19894ed7ceb2c2ee6c"
    {
        return Err(Error::invalid("unsupported CartD.rel revision"));
    }
    Ok(())
}
pub fn update(bytes: &[u8], s: Input) -> Result<Update> {
    verify(bytes)?;
    if s.course_length <= 0 || s.segment_tests.len() > 11 {
        return Err(Error::invalid(
            "invalid course length or segment observation count",
        ));
    }
    let mut r = Update {
        piece: s.piece,
        counter: s.counter,
        highest: s.highest,
        tested_indices: vec![],
        debug_indices: vec![],
    };
    let mut observations = s.segment_tests.into_iter();
    let mut test = |idx| {
        r.tested_indices.push(idx);
        observations
            .next()
            .ok_or_else(|| Error::invalid("missing segment test result"))
    };
    let mut forward = false;
    for step in 1..=if s.work_44 == 0 { 3 } else { 9 } {
        let next = s.piece.wrapping_add(step);
        if test(next % s.course_length)? {
            r.piece = next;
            if next >= s.course_length {
                if step == 1 {
                    r.piece = 0;
                }
                r.counter = r.counter.wrapping_add(1);
            }
            forward = true;
            break;
        }
    }
    if !forward && s.work_44 != 0 {
        let index =
            |delta| s.piece.wrapping_add(s.course_length).wrapping_add(delta) % s.course_length;
        if s.debug_enabled {
            r.debug_indices = [1, 2, 0, -1, -2].map(index).to_vec();
        }
        for step in [1, 2] {
            if test(index(-step))? {
                r.piece = s.piece.wrapping_sub(step + 1);
                if r.piece < 0 {
                    r.piece = r.piece.wrapping_add(s.course_length);
                    if matches!(s.mode, 0 | 1) {
                        r.piece = 0;
                    } else {
                        r.counter = r.counter.wrapping_sub(1);
                    }
                }
                break;
            }
        }
    }
    if observations.next().is_some() {
        return Err(Error::invalid("unused segment test result"));
    }
    r.highest = r.highest.max(r.counter);
    Ok(r)
}
/// Runtime record at section 6 + 0x26D84 + index * 32 is four X/Z pairs.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SegmentInput {
    pub task_position: [f32; 3],
    pub work_position: Option<[f32; 3]>,
    pub segment_xz: [[f32; 2]; 4],
}
#[derive(Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SegmentGeometry {
    Miss,
    RequiresIntersection {
        origin: [f32; 3],
        direction: [f32; 3],
        triangle: [[f32; 3]; 3],
    },
}
/// Reconstruct the operands at the first imported geometry call of 0x9220.
/// No work pointer or zero planar displacement returns zero before that call.
pub fn segment_geometry(bytes: &[u8], s: SegmentInput) -> Result<SegmentGeometry> {
    verify(bytes)?;
    let Some(work) = s.work_position else {
        return Ok(SegmentGeometry::Miss);
    };
    let [x, _, z] = s.task_position;
    let direction = [work[0] - x, 0., work[2] - z];
    if ![x, z, direction[0], direction[2]]
        .into_iter()
        .all(f32::is_finite)
    {
        return Err(Error::invalid("invalid kart segment movement"));
    }
    if direction[0] == 0. && direction[2] == 0. {
        return Ok(SegmentGeometry::Miss);
    }
    let rel = crate::city_rel::CityRel::read_identified(bytes, 80)?;
    let constants = rel.data(4)?;
    let number = |at: usize| f32::from_be_bytes(constants[at..at + 4].try_into().unwrap());
    let [a, _, c, b] = s.segment_xz;
    let dx = number(0x340) * (b[0] - a[0]);
    let dz = number(0x340) * (b[1] - a[1]);
    let triangle = [
        [a[0] - dx, number(0x344), a[1] - dz],
        [b[0] + dx, number(0x344), b[1] + dz],
        [c[0], number(0x334), c[1]],
    ];
    if !triangle.iter().flatten().all(|n| n.is_finite()) {
        return Err(Error::invalid("nonfinite kart crossing triangle"));
    }
    Ok(SegmentGeometry::RequiresIntersection {
        origin: [x, 0., z],
        direction,
        triangle,
    })
}
