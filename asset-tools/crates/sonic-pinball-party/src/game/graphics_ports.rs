//! Bind completed OBJ queue/compositor/affine ports to the same supplied RAM.
//! Native DMA and display hardware remain explicit backend calls.
use super::{
    affine,
    frame::{FrameError, FrameMemory, NativeCall},
    graphics_frame::GraphicsFrameState,
    lifecycle::LifecycleCalls,
    oam::{self, OamSubmissionQueue, Submission},
    oam_render::{self, StagedOam},
};
fn error(offset: u32, reason: impl ToString) -> FrameError {
    FrameError::Dependency {
        call: NativeCall {
            entry_offset: offset,
            argument0: None,
        },
        reason: reason.to_string(),
    }
}
fn word(m: &mut dyn FrameMemory, a: u32, v: u32) -> Result<(), FrameError> {
    m.write_u16(a, v as u16)?;
    m.write_u16(a + 2, (v >> 16) as u16)
}
fn queue(m: &dyn FrameMemory) -> Result<OamSubmissionQueue, FrameError> {
    let mut records = [[0; oam::RECORD_WORDS]; oam::CAPACITY];
    for (i, row) in records.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            *v = m.read_u32(oam::RECORD_BASE + (i * 28 + j * 4) as u32)?;
        }
    }
    let mut order = [0; oam::CAPACITY + 1];
    for (i, v) in order.iter_mut().enumerate() {
        *v = m.read_u32(oam::ORDER_BASE + i as u32 * 4)?;
    }
    OamSubmissionQueue::from_words(m.read_u8(oam::COUNT_ADDRESS)?, &records, &order)
        .map_err(|e| error(0x2614, e))
}
fn write_queue(
    m: &mut dyn FrameMemory,
    before: &OamSubmissionQueue,
    after: &OamSubmissionQueue,
) -> Result<(), FrameError> {
    for (i, (old, new)) in before.records().iter().zip(after.records()).enumerate() {
        for (j, (&a, &b)) in old.iter().zip(new).enumerate() {
            if a != b {
                word(m, oam::RECORD_BASE + (i * 28 + j * 4) as u32, b)?;
            }
        }
    }
    for (i, (&a, &b)) in before.order().iter().zip(after.order()).enumerate() {
        if a != b {
            word(m, oam::ORDER_BASE + i as u32 * 4, b)?;
        }
    }
    if before.count() != after.count() {
        m.write_u8(oam::COUNT_ADDRESS, after.count())?;
    }
    Ok(())
}
fn staged(m: &dyn FrameMemory) -> Result<StagedOam, FrameError> {
    let mut entries = [[0; 4]; oam::CAPACITY];
    for (i, row) in entries.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            *v = m.read_u16(oam_render::OUTPUT_BASE + (i * 8 + j * 2) as u32)?;
        }
    }
    Ok(StagedOam {
        entries,
        count: m.read_u8(oam_render::OUTPUT_COUNT_ADDRESS)?,
    })
}
fn write_staged(
    m: &mut dyn FrameMemory,
    before: &StagedOam,
    after: &StagedOam,
) -> Result<(), FrameError> {
    for (i, (a, b)) in before.entries.iter().zip(after.entries.iter()).enumerate() {
        for (j, (&a, &b)) in a.iter().zip(b).enumerate() {
            if a != b {
                m.write_u16(oam_render::OUTPUT_BASE + (i * 8 + j * 2) as u32, b)?;
            }
        }
    }
    if before.count != after.count {
        m.write_u8(oam_render::OUTPUT_COUNT_ADDRESS, after.count)?;
    }
    Ok(())
}
fn begin(m: &mut dyn FrameMemory, offset: u32, argument: u32) -> Result<(), FrameError> {
    let before = staged(m)?;
    let mut state = GraphicsFrameState {
        staged: before.clone(),
        affine_count: m.read_u8(0x0300005c)?,
        submission_count: m.read_u8(oam::COUNT_ADDRESS)?,
        tree_root: m.read_u32(0x030023b0)?,
    };
    match offset {
        0x1dc4 => state.begin_frame(argument),
        0x1ddc => state.hide_objects(),
        0x1e28 => state.initialize_affine(argument),
        _ => unreachable!(),
    }
    write_staged(m, &before, &state.staged)?;
    if offset != 0x1ddc {
        m.write_u8(0x0300005c, state.affine_count)?;
    }
    Ok(())
}
struct Descriptors<'a>(&'a dyn FrameMemory);
impl oam_render::DescriptorSource for Descriptors<'_> {
    fn halfword(&self, a: u32) -> Option<u16> {
        self.0.read_u16(a).ok()
    }
}

pub struct GraphicsCalls<'a>(pub &'a mut dyn LifecycleCalls);
impl LifecycleCalls for GraphicsCalls<'_> {
    fn call(
        &mut self,
        m: &mut dyn FrameMemory,
        offset: u32,
        args: &[u32],
    ) -> Result<(), FrameError> {
        match (offset, args) {
            (0x236c, &[index, angle, scale_x, scale_y, flags]) => affine::write(
                m,
                affine::Request {
                    index,
                    angle,
                    scale_x,
                    scale_y,
                    flags,
                },
            ),
            (0x1dc4 | 0x1e28, &[arg]) => begin(m, offset, arg),
            (0x1ddc, []) => begin(m, offset, 0),
            (0x2580, []) => m.write_u8(oam::COUNT_ADDRESS, 0),
            (0x28ec, []) => {
                m.write_u8(oam::COUNT_ADDRESS, 0)?;
                word(m, 0x030023b0, 0)
            }
            (0x2614, &[sort_key, x, y, descriptor_address, flags, auxiliary]) => {
                // Source full-count path does not touch a descriptor or records.
                if m.read_u8(oam::COUNT_ADDRESS)? & 0x80 != 0 {
                    return Ok(());
                }
                let at = descriptor_address
                    .checked_add(4)
                    .ok_or_else(|| error(offset, "descriptor extent overflow"))?;
                let descriptor_halfword_at_4 = m.read_u16(at)?;
                let before = queue(m)?;
                let mut after = before.clone();
                after.submit(Submission {
                    sort_key,
                    x,
                    y,
                    descriptor_address,
                    descriptor_halfword_at_4,
                    flags,
                    auxiliary,
                });
                write_queue(m, &before, &after)
            }
            (0x2700, []) => {
                let queue = queue(m)?;
                let before = staged(m)?;
                let result = oam_render::compose(&queue, &Descriptors(m), &before)
                    .map_err(|e| error(offset, e))?;
                for (i, (&a, &b)) in queue
                    .order()
                    .iter()
                    .zip(result.sorted_order.iter())
                    .enumerate()
                {
                    if a != b {
                        word(m, oam::ORDER_BASE + i as u32 * 4, b)?;
                    }
                }
                write_staged(m, &before, &result.staged)
            }
            (0x236c | 0x1dc4 | 0x1e28 | 0x1ddc | 0x2580 | 0x28ec | 0x2614 | 0x2700, _) => {
                Err(error(offset, "unexpected native graphics argument count"))
            }
            _ => self.0.call(m, offset, args),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{
        frame_ports::RejectFrameHelpers,
        memory::{MemoryImage, Region},
    };
    use serde_json::Value;
    #[test]
    #[ignore = "Requires local SPP_ROM; composed native graphics instructions"]
    fn composed_native_affine_submission_sorting_preserves_shared_ram() {
        use std::io::Read;
        let mut rom = Vec::new();
        std::fs::File::open(std::env::var_os("SPP_ROM").expect("Set local SPP_ROM"))
            .unwrap()
            .take((crate::catalog::ROM_LEN + 1) as u64)
            .read_to_end(&mut rom)
            .unwrap();
        let v: Value =
            serde_json::from_str(include_str!("../../tests/graphics_ports-vectors.json")).unwrap();
        for c in v["cases"].as_array().unwrap() {
            let seed = c["seed"].as_u64().unwrap() as u8;
            let mut m = MemoryImage::new(
                &rom,
                vec![
                    Region {
                        base: 0x02000000,
                        bytes: vec![0; 0x40000],
                        writable: true,
                    },
                    Region {
                        base: 0x03000000,
                        bytes: (0..0x8000)
                            .map(|i| seed.wrapping_add((i as u8).wrapping_mul(37)))
                            .collect(),
                        writable: true,
                    },
                ],
            )
            .unwrap();
            for p in c["patches"].as_array().unwrap() {
                let a = p[0].as_u64().unwrap() as u32;
                let n = p[1].as_u64().unwrap() as usize;
                let val = p[2].as_u64().unwrap() as u32;
                m.write_bytes(a, &val.to_le_bytes()[..n]).unwrap();
            }
            let mut reject = RejectFrameHelpers;
            let mut calls = GraphicsCalls(&mut reject);
            for op in c["operations"].as_array().unwrap() {
                let entry = op[0].as_u64().unwrap() as u32;
                let args: Vec<u32> = op[1]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap() as u32)
                    .collect();
                calls.call(&mut m, entry, &args).unwrap();
            }
            assert_eq!(
                crate::sha256(m.bytes(0x03000000, 0x7000).unwrap()),
                c["iwramSha256"].as_str().unwrap()
            );
            assert_eq!(
                crate::sha256(m.bytes(0x02000000, 0x40000).unwrap()),
                c["ewramSha256"].as_str().unwrap()
            );
        }
    }
}
