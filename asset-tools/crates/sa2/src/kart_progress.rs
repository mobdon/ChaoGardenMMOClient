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
pub(crate) fn verify(bytes: &[u8]) -> Result<()> {
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

/// Expand the source-local crossing quads in the order CartD's 0x7388 loop
/// calls its transform helper. Array positions are native segment indices.
pub fn crossing_directory(bytes: &[u8], exe: &[u8]) -> Result<serde_json::Value> {
    verify(bytes)?;
    let rel = crate::city_rel::CityRel::read_identified(bytes, 80)?;
    let invalid = || Error::invalid("invalid CartD crossing directory");
    let directory = |at| -> Result<u32> {
        let hi = rel.reference(1, at).ok_or_else(invalid)?;
        let lo = rel.reference(1, at + 4).ok_or_else(invalid)?;
        if (hi.module, hi.section, hi.kind, lo.kind) != (80, 5, 6, 4)
            || (hi.module, hi.section, hi.addend) != (lo.module, lo.section, lo.addend)
        {
            return Err(invalid());
        }
        Ok(hi.addend)
    };
    let crossings = directory(0x715a)?;
    let models = directory(0x7172)? as usize;
    let data = rel.data(5)?;
    let mut pieces = Vec::new();
    for id in 0..85 {
        let row = crossings + id * 8;
        let at = row as usize + 4;
        let count = u32::from_be_bytes(
            data.get(at..at + 4)
                .ok_or_else(invalid)?
                .try_into()
                .unwrap(),
        );
        let mut quads = Vec::new();
        if count != 0 {
            let (sec, start) = rel.local_pointer(5, row)?.ok_or_else(invalid)?;
            let end = start
                .checked_add(count.checked_mul(32).ok_or_else(invalid)?)
                .ok_or_else(invalid)?;
            for record in rel
                .data(sec)?
                .get(start as usize..end as usize)
                .ok_or_else(invalid)?
                .chunks_exact(32)
            {
                let mut quad = [[0f32; 2]; 4];
                for (dest, src) in quad.iter_mut().flatten().zip(record.chunks_exact(4)) {
                    *dest = f32::from_be_bytes(src.try_into().unwrap());
                }
                quads.push(quad);
            }
        }
        pieces.push(quads);
    }
    let mut routes = Vec::new();
    for course in crate::kart_course::courses(exe)? {
        let mut segments = Vec::new();
        let mut segment_event_modes = Vec::new();
        let mut translation = [0f32; 3];
        let mut yaw = 0i32;
        let mut poses = Vec::new();
        let mut projected = Vec::new();
        for (occurrence, &id) in course.piece_ids.iter().enumerate() {
            let row = models + usize::from(id) * 108;
            let fields = data.get(row + 8..row + 24).ok_or_else(invalid)?;
            let word = |at: usize| u32::from_be_bytes(fields[at..at + 4].try_into().unwrap());
            // Native task poses and transformed vectors use single precision.
            // The imported engine sine/cosine and matrix rounding are unverified.
            let (sin, cos) = (f64::from(yaw as u16) * std::f64::consts::TAU / 65536.).sin_cos();
            let rotate = |x: f32, z: f32| {
                [
                    (cos * f64::from(x) + sin * f64::from(z)) as f32,
                    (-sin * f64::from(x) + cos * f64::from(z)) as f32,
                ]
            };
            poses.push(serde_json::json!({"translation":translation,"yaw_bams":yaw}));
            for (local, quad) in pieces
                .get(id as usize)
                .ok_or_else(invalid)?
                .iter()
                .enumerate()
            {
                segments.push([occurrence, local]);
                segment_event_modes.push(crate::kart_vehicle_init::course_mode(id));
                projected.push(quad.map(|[x, z]| {
                    let [rx, rz] = rotate(x, z);
                    [translation[0] + rx, translation[2] + rz]
                }));
            }
            let [dx, dz] = rotate(f32::from_bits(word(0)), f32::from_bits(word(8)));
            translation[0] += dx;
            translation[1] += f32::from_bits(word(4));
            translation[2] += dz;
            yaw = yaw.wrapping_add(word(12) as i32);
        }
        routes.push(
            serde_json::json!({"course":course.name, "piece_ids":course.piece_ids,
            "projected_occurrence_poses":poses,"projected_segment_xz":projected,"segment_event_modes":segment_event_modes,"segment_count":segments.len(), "segment_occurrence_and_local_index":segments}),
        );
    }
    Ok(serde_json::json!({"piece_local_xz_quads":pieces,"courses":routes}))
}

/// Inputs immediately before the two imported vector-length results are combined.
/// Slots are the contiguous occupied prefix (CartD stops at the first null).
#[derive(Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StandingSlot {
    segment: i32,
    counter: i32,
    segment_length: f32,
    distance_to_next: f32,
}

#[derive(Serialize)]
pub struct Standings {
    /// Raw single-precision values also preserve unordered scores (zero / zero).
    score_bits: Vec<u32>,
    places_by_slot: Vec<usize>,
}

pub fn standings(bytes: &[u8], slots: &[StandingSlot]) -> Result<Standings> {
    verify(bytes)?;
    rank_slots(slots)
}

fn rank_slots(slots: &[StandingSlot]) -> Result<Standings> {
    if slots.len() > 6 {
        return Err(Error::invalid(
            "kart standings have at most six occupied slots",
        ));
    }
    let mut scores = Vec::new();
    for slot in slots {
        if [slot.segment_length, slot.distance_to_next]
            .iter()
            .any(|&x| !x.is_finite() || x < 0.)
        {
            return Err(Error::invalid("expected finite nonnegative kart distances"));
        }
        let fraction = 100. - 100. * slot.distance_to_next / slot.segment_length;
        let base = slot
            .counter
            .wrapping_add(10)
            .wrapping_mul(1_000_000)
            .wrapping_add(slot.segment.wrapping_mul(1000));
        scores.push(base as f32 + if fraction < 0. { 0. } else { fraction });
    }
    // 0x94f4 uses a halving-gap sort. A stable library sort gives different
    // places for some tied slots; unordered comparisons also never swap.
    let mut order: Vec<_> = (0..slots.len()).collect();
    let mut gap = order.len() / 2;
    while gap > 0 {
        for i in gap..order.len() {
            let mut j = i;
            while j >= gap && scores[order[j - gap]] > scores[order[j]] {
                order.swap(j - gap, j);
                j -= gap;
            }
        }
        gap /= 2;
    }
    let mut places_by_slot = vec![0; slots.len()];
    for (i, slot) in order.into_iter().enumerate() {
        places_by_slot[slot] = slots.len() - i;
    }
    Ok(Standings {
        score_bits: scores.into_iter().map(f32::to_bits).collect(),
        places_by_slot,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RaceField {
    pub finish_latched: bool,
    pub post_finish_ticks: i32,
    pub suppress_place_calls: bool,
    pub restrict_finish: bool,
    pub race_clock: u32,
    pub slots: Vec<RaceSlot>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RaceSlot {
    #[serde(skip_serializing)]
    standing: StandingSlot,
    /// Original kart work +0x48; an absent observation must not imply slot zero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    work_48: Option<u8>,
    work_44: u32,
    place: i32,
    first_ticks: i32,
    sixth_ticks: i32,
    cooldown: i32,
}

#[derive(Serialize)]
pub struct RaceFieldUpdate {
    finish_latched: bool,
    post_finish_ticks: i32,
    slots: Vec<RaceSlot>,
    cue_requests: Vec<CueRequest>,
    calls: Vec<(usize, &'static str, Option<u8>)>,
}

#[derive(Serialize)]
struct CueRequest {
    slot: usize,
    kind: u8,
    work_48: Option<u8>,
    resolution: CueResolution,
}
#[derive(Serialize)]
#[serde(rename_all = "snake_case", tag = "state")]
enum CueResolution {
    NeedsKartIndex,
    Silent,
    EngineRequest { id: i32 },
}

fn resolve_cues(
    bytes: &[u8],
    slots: &[RaceSlot],
    calls: &[(usize, &str, Option<u8>)],
) -> Result<Vec<CueRequest>> {
    let module = crate::city_rel::CityRel::read_identified(bytes, 80)?;
    let data = module.data(5)?;
    let mut requests = Vec::new();
    for &(slot, name, kind) in calls {
        if name != "cart_5c38" {
            continue;
        }
        let kind = kind.ok_or_else(|| Error::invalid("missing kart cue selector"))?;
        let work_48 = slots[slot].work_48;
        let resolution = match work_48 {
            None => CueResolution::NeedsKartIndex,
            Some(index @ 0..=15) => {
                // Section 1 +0x5c72, 0x5c8e and 0x5caa all relocate to this table.
                let at = 0x3924 + usize::from(index) * 24 + 8 + usize::from(kind) * 4;
                let id = i32::from_be_bytes(data[at..at + 4].try_into().unwrap());
                if id < 0 {
                    CueResolution::Silent
                } else {
                    CueResolution::EngineRequest { id }
                }
            }
            _ => return Err(Error::invalid("unknown kart cue table index")),
        };
        requests.push(CueRequest {
            slot,
            kind,
            work_48,
            resolution,
        });
    }
    Ok(requests)
}

/// CartD 0x8d70–0x913c, after the occupied slots' progress counters are updated.
pub fn field_update(bytes: &[u8], mut field: RaceField) -> Result<RaceFieldUpdate> {
    verify(bytes)?;
    if field.slots.len() > 6 {
        return Err(Error::invalid("at most six occupied kart slots"));
    }
    let mut calls = Vec::new();
    if field.finish_latched {
        field.post_finish_ticks = field.post_finish_ticks.wrapping_add(1);
    } else {
        let slots = field.slots.iter().map(|s| s.standing).collect::<Vec<_>>();
        let ranks = rank_slots(&slots)?;
        field.post_finish_ticks = 0;
        let mut order: Vec<_> = (0..slots.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse(ranks.places_by_slot[i]));
        for i in order {
            let s = &mut field.slots[i];
            let old = s.place;
            s.place = ranks.places_by_slot[i] as i32;
            if !field.finish_latched && !field.suppress_place_calls && s.work_44 != 0 {
                let timer = match s.place {
                    1 => Some((&mut s.first_ticks, 1)),
                    6 => Some((&mut s.sixth_ticks, 2)),
                    _ => None,
                };
                if let Some((ticks, kind)) = timer {
                    if old != s.place {
                        *ticks = 0;
                    } else {
                        *ticks = ticks.wrapping_add(1);
                        if *ticks == 15 && field.race_clock > 700 && s.cooldown > 300 {
                            calls.push((i, "cart_5c38", Some(kind)));
                            s.cooldown = 0;
                        }
                    }
                }
            }
            s.cooldown = s.cooldown.wrapping_add(1);
            if s.standing.counter >= 3 && (!field.restrict_finish || s.work_44 != 0) {
                field.finish_latched = true;
                calls.push((i, "cart_7040", None));
                if field.restrict_finish {
                    calls.push((i, "cart_d4f8", None));
                }
                calls.push((i, "cart_5c38", Some(0)));
            }
        }
    }
    let cue_requests = resolve_cues(bytes, &field.slots, &calls)?;
    Ok(RaceFieldUpdate {
        cue_requests,
        finish_latched: field.finish_latched,
        post_finish_ticks: field.post_finish_ticks,
        slots: field.slots,
        calls,
    })
}
