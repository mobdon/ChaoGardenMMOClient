//! Ring lines and circles (RING_LINEAR, RING_CIRCLE), ported from the
//! installed stage module (City Escape: stg13D.rel section 1, 0x27CA8 to
//! 0x285F8, o_degring_linear in the GSNE8P split).
//!
//! A group allocates a 0xA0-byte work block: eight 8-byte ring entries
//! (flags word and child task) followed by eight positions. On its first run
//! it takes the ring count from Scale.z (clamped to 1..=8), restores which
//! rings were already collected from the SET state, and computes every ring
//! position through the Ninja matrix stack. Each later update spawns a RING
//! child for every uncollected ring without one, and the group removes
//! itself once every ring is collected.
//!
//! Engine services are boundaries: the clip gate, the allocator, the SET
//! state, the Ninja sine, cosine and matrix calls, and child-task creation.
//! The matrix calls are reported in order with their arguments, and the
//! caller supplies `calc_point`.
use crate::{city_rel::CityRel, manifest, stage_land, Error, Result};
use serde::Serialize;

/// Section 1 range holding both callbacks and their helpers.
pub const CODE_START: u32 = 0x27ca8;
pub const CODE_LEN: u32 = 0x950;
/// SHA-256 of that range with relocated words zeroed.
pub const CODE_HASH: &str = "774d303d924ed508fb67a6b552fc1bccab8e71fb4ec6e8600fdd35abd5f2e77e";

/// Engine imports at offsets from `CODE_START`: (offset, address).
const IMPORTS: &[(u32, u32)] = &[
    (0x74, 0x8011_5d50),  // linear: njPushMatrix
    (0x7c, 0x8011_509c),  // linear: unit matrix
    (0x84, 0x8011_5e24),  // linear: translate
    (0x90, 0x8011_5560),  // linear: rotate (angle Z)
    (0x9c, 0x8011_54e0),  // linear: rotate (angle Y)
    (0xa8, 0x8011_5918),  // linear: rotate (angle X)
    (0xf0, 0x8011_2050),  // linear: sine
    (0x108, 0x8011_4bc4), // linear: calc point
    (0x12c, 0x8011_5d9c), // linear: pop
    (0x280, 0x8006_538c), // linear init: clip gate
    (0x338, 0x8011_94b4), // work allocation
    (0x37c, 0x8006_5120), // SET state read
    (0x4c0, 0x8007_1878), // destructor: free child
    (0x510, 0x8006_5134), // destructor: save SET state
    (0x520, 0x8011_94e0), // destructor: free work
    (0x5b4, 0x8006_538c), // update: clip gate
    (0x618, 0x8007_1ba4), // update: create child task
    (0x6ac, 0x8006_5148), // update: mark SET entry done
    (0x6b8, 0x8007_1878), // update: free task
];

fn invalid() -> Error {
    Error::invalid("unsupported ring group module")
}

fn word(d: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_be_bytes(
        d.get(at..at + 4).ok_or_else(invalid)?.try_into().unwrap(),
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Linear,
    Circle,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Program {
    pub module: u32,
    pub linear_init: u32,
    pub circle_init: u32,
    /// Added to Scale.x: the line spacing and the circle radius.
    pub spacing_base: f32,
    /// Circle span in BAMS is int(bams_per_degree * degrees * (one + Scale.y)).
    pub bams_per_degree: f32,
    pub degrees: f32,
    pub one: f32,
    /// The line's quarter-wave span in BAMS.
    pub linear_span: i32,
    pub work_size: u32,
}

impl Program {
    /// Read and verify the ring-group code of an installed stage REL.
    pub fn read(filename: &str, bytes: &[u8]) -> Result<Self> {
        let info = stage_land::catalog()
            .iter()
            .find(|i| i.filename == filename)
            .ok_or_else(invalid)?;
        if bytes.len() != info.size || manifest::sha256(bytes) != info.sha256 {
            return Err(invalid());
        }
        let rel = CityRel::read_identified(bytes, info.module)?;
        let row = |want: &[u8]| -> Result<u32> {
            let d5 = rel.data(5)?;
            for row in (4..d5.len() as u32).step_by(4) {
                let Ok(Some((s, n))) = rel.local_pointer(5, row) else {
                    continue;
                };
                let name = rel.data(s)?.get(n as usize..).unwrap_or_default();
                let name = name.split(|&c| c == 0).next().unwrap_or_default();
                if name.trim_ascii_end() == want {
                    if let Ok(Some((1, init))) = rel.local_pointer(5, row - 4) {
                        return Ok(init);
                    }
                }
            }
            Err(invalid())
        };
        let linear_init = row(b"RING_LINEAR")?;
        let circle_init = row(b"RING_CIRCLE")?;
        if (linear_init, circle_init) != (CODE_START + 0x260, CODE_START + 0x6d4) {
            return Err(invalid());
        }
        let code = rel
            .data(1)?
            .get(CODE_START as usize..(CODE_START + CODE_LEN) as usize)
            .ok_or_else(invalid)?;
        let mut masked = code.to_vec();
        for off in (0..CODE_LEN).step_by(2) {
            if rel.reference(1, CODE_START + off).is_some() {
                let w = (off & !3) as usize;
                masked[w..w + 4].fill(0);
            }
        }
        if manifest::sha256(&masked) != CODE_HASH {
            return Err(invalid());
        }
        for &(off, addr) in IMPORTS {
            let r = rel.reference(1, CODE_START + off).ok_or_else(invalid)?;
            if (r.module, r.kind, r.addend) != (0, 10, addr) {
                return Err(invalid());
            }
        }
        let constant = |at: u32| -> Result<f32> {
            let hi = rel.reference(1, CODE_START + at + 2).ok_or_else(invalid)?;
            let lo = rel.reference(1, CODE_START + at + 6).ok_or_else(invalid)?;
            if (
                hi.module, hi.kind, hi.section, lo.module, lo.kind, lo.section,
            ) != (info.module, 6, 4, info.module, 4, 4)
                || hi.addend != lo.addend
            {
                return Err(invalid());
            }
            Ok(f32::from_bits(word(rel.data(4)?, hi.addend as usize)?))
        };
        let spacing_base = constant(0x2e4)?;
        if constant(0x788)? != spacing_base {
            return Err(invalid());
        }
        Ok(Self {
            module: info.module,
            linear_init,
            circle_init,
            spacing_base,
            bams_per_degree: constant(0x748)?,
            degrees: constant(0x754)?,
            one: constant(0x760)?,
            linear_span: word(code, 0x2d4)? as u16 as i16 as i32,
            work_size: word(code, 0x334)? & 0xffff,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "call", rename_all = "camelCase")]
pub enum Call {
    Allocate {
        size: u32,
    },
    ReadSetState,
    PushMatrix,
    UnitMatrix,
    Translate {
        position: [f32; 3],
    },
    /// The three rotation imports in call order: (import, angle).
    Rotate {
        import: u32,
        angle: i32,
    },
    Sine {
        angle: i32,
    },
    Cosine {
        angle: i32,
    },
    CalcPoint {
        local: [f32; 3],
    },
    PopMatrix,
    /// CreateChildTask(2, 1, RING, "Ring") for ring `index` at `position`.
    SpawnRing {
        index: usize,
        position: [f32; 3],
    },
    MarkSetEntryDone,
    FreeTask,
    /// Destructor: the ring's unlink helper, then the child is freed.
    UnlinkAndFreeRing {
        index: usize,
    },
    SaveSetState {
        mask: i8,
    },
    FreeWork,
}

/// One ring entry of the work block.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// Bit 1: collected. Bit 0 is cleared when a child is unlinked.
    pub flags: u32,
    /// The RING child linked to this entry (caller-chosen id).
    pub child: Option<u32>,
}

impl Entry {
    pub fn collected(&self) -> bool {
        self.flags & 2 != 0
    }
}

/// Engine services the group calls into.
pub trait Engine {
    /// The clip gate: nonzero means skip this update.
    fn clipped(&mut self) -> bool;
    fn allocate(&mut self) -> bool;
    /// The SET state byte, when the task has a SET entry.
    fn set_state(&mut self) -> Option<i8>;
    fn sine(&mut self, angle: i32) -> f32;
    fn cosine(&mut self, angle: i32) -> f32;
    fn calc_point(&mut self, local: [f32; 3]) -> [f32; 3];
    /// CreateChildTask: the new child's id, or None when it fails.
    fn spawn_ring(&mut self) -> Option<u32>;
    /// The task has a SET entry (task + 0x30).
    fn has_set_entry(&self) -> bool;
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RingGroup {
    pub kind: Kind,
    pub position: [f32; 3],
    /// Rotation words as stored (only the low 16 bits are used).
    pub angles: [u32; 3],
    pub scale: [f32; 3],
    pub count: u8,
    /// None until the work block is allocated.
    pub entries: Option<[Entry; 8]>,
    pub positions: [[f32; 3]; 8],
    pub started: bool,
}

impl RingGroup {
    pub fn new(kind: Kind, position: [f32; 3], angles: [u32; 3], scale: [f32; 3]) -> Self {
        Self {
            kind,
            position,
            angles,
            scale,
            count: 0,
            entries: None,
            positions: [[0.0; 3]; 8],
            started: false,
        }
    }

    /// One run of the task: the init callback first, the update afterwards.
    pub fn step(&mut self, p: &Program, e: &mut impl Engine) -> Vec<Call> {
        let mut calls = vec![];
        if !self.started {
            if e.clipped() {
                return calls;
            }
            self.started = true;
            self.init(p, e, &mut calls);
        } else {
            self.update(e, &mut calls);
        }
        calls
    }

    fn init(&mut self, p: &Program, e: &mut impl Engine, calls: &mut Vec<Call>) {
        calls.push(Call::Allocate { size: p.work_size });
        if !e.allocate() {
            return;
        }
        let mut entries = [Entry::default(); 8];
        // fctiwz, then clamped to 1..=8 and stored as a byte.
        let z = self.scale[2];
        let n = if z.is_nan() { i32::MIN } else { z as i32 };
        self.count = n.clamp(1, 8) as u8;
        let mut state = 0u32;
        if e.has_set_entry() {
            calls.push(Call::ReadSetState);
            state = e.set_state().unwrap_or(0) as i32 as u32;
        }
        // Bit 0 restores entry 7, bit 7 entry 0.
        for k in 0..8 {
            if (state >> k) & 1 != 0 {
                entries[7 - k].flags = 2;
            }
        }
        self.entries = Some(entries);
        let a = self.angles.map(|v| (v & 0xffff) as i32);
        let n = self.count as i32;
        calls.extend([
            Call::PushMatrix,
            Call::UnitMatrix,
            Call::Translate {
                position: self.position,
            },
            Call::Rotate {
                import: 0x8011_5560,
                angle: a[2],
            },
            Call::Rotate {
                import: 0x8011_54e0,
                angle: a[1],
            },
            Call::Rotate {
                import: 0x8011_5918,
                angle: a[0],
            },
        ]);
        match self.kind {
            Kind::Linear => {
                let step = p.linear_span / n;
                let spacing = p.spacing_base + self.scale[0];
                let mut local = [0.0f32; 3];
                let mut angle = 0;
                for slot in self.positions.iter_mut().take(n as usize) {
                    calls.push(Call::Sine { angle });
                    local[1] = self.scale[1] * e.sine(angle);
                    calls.push(Call::CalcPoint { local });
                    *slot = e.calc_point(local);
                    local[2] -= spacing;
                    angle += step;
                }
            }
            Kind::Circle => {
                let span = p.bams_per_degree * (p.degrees * (p.one + self.scale[1]));
                let span = if span.is_nan() { i32::MIN } else { span as i32 };
                let step = span.wrapping_div(n);
                let radius = p.spacing_base + self.scale[0];
                let mut local = [0.0f32; 3];
                let mut angle = 0i32;
                for slot in self.positions.iter_mut().take(n as usize) {
                    calls.push(Call::Sine { angle });
                    local[1] = radius * e.sine(angle);
                    calls.push(Call::Cosine { angle });
                    local[2] = radius * e.cosine(angle);
                    calls.push(Call::CalcPoint { local });
                    *slot = e.calc_point(local);
                    angle = angle.wrapping_add(step);
                }
            }
        }
        calls.push(Call::PopMatrix);
    }

    /// The destructor: unlink and free every child, save which rings were
    /// collected (entry 0 in bit 7) and free the work block.
    pub fn destroy(&mut self, e: &mut impl Engine) -> Vec<Call> {
        let mut calls = vec![];
        let Some(entries) = self.entries.as_mut() else {
            return calls;
        };
        let mut mask = 0u32;
        for (i, x) in entries.iter_mut().enumerate() {
            if x.child.is_some() {
                x.flags &= !1;
                x.child = None;
                calls.push(Call::UnlinkAndFreeRing { index: i });
            }
            mask = (mask << 1) | u32::from(x.collected());
        }
        if e.has_set_entry() {
            calls.push(Call::SaveSetState { mask: mask as i8 });
        }
        calls.push(Call::FreeWork);
        self.entries = None;
        calls
    }

    fn update(&mut self, e: &mut impl Engine, calls: &mut Vec<Call>) {
        let Some(entries) = self.entries.as_mut() else {
            return;
        };
        let n = self.count as usize;
        let any_child = entries[..n].iter().any(|x| x.child.is_some());
        if !any_child && e.clipped() {
            return;
        }
        let mut remaining = 0;
        for (i, x) in entries[..n].iter_mut().enumerate() {
            if !x.collected() && x.child.is_none() {
                calls.push(Call::SpawnRing {
                    index: i,
                    position: self.positions[i],
                });
                if let Some(id) = e.spawn_ring() {
                    // The ring's link helper: a fresh child (mode 0) takes
                    // this entry and moves to mode 2.
                    x.flags = 0;
                    x.child = Some(id);
                }
            }
            if !x.collected() {
                remaining += 1;
            }
        }
        if remaining == 0 {
            calls.push(if e.has_set_entry() {
                Call::MarkSetEntryDone
            } else {
                Call::FreeTask
            });
        }
    }
}
