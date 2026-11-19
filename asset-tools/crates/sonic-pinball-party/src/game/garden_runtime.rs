//! Compose completed embedded-Garden Rust ports on one bounded memory image.
//! Unknown boot, care, task, audio, save and hardware calls remain errors.
use super::{
    frame::{FrameError, FrameMemory, NativeCall},
    garden_frame, garden_graphics, garden_palette,
    garden_payload::{self, BoardCalls, BoardError, BoardSetupTables, NativeBoardCalls},
};

/// Only this backend may supply still-unported payload services. Return R0 is
/// retained for callers such as the native task allocator; it is never guessed.
pub trait Backend {
    fn invoke(
        &mut self,
        m: &mut dyn FrameMemory,
        offset: u32,
        args: &[u32],
    ) -> Result<u32, FrameError>;
}
pub struct RejectBackend;
impl Backend for RejectBackend {
    fn invoke(
        &mut self,
        _: &mut dyn FrameMemory,
        offset: u32,
        args: &[u32],
    ) -> Result<u32, FrameError> {
        Err(FrameError::Dependency {
            call: NativeCall {
                entry_offset: 0x02000000 + offset,
                argument0: args.first().copied(),
            },
            reason: format!("unported Garden payload service with arguments {args:#x?}"),
        })
    }
}
pub struct Runtime<'a, B = RejectBackend> {
    resources: garden_graphics::GardenPayload<'a>,
    tables: BoardSetupTables,
    pub backend: B,
    depth: u8,
}
impl<'a, B: Backend> Runtime<'a, B> {
    pub fn new(payload: &'a [u8], backend: B) -> Result<Self, garden_graphics::Error> {
        let resources = garden_graphics::GardenPayload::new(payload)?;
        let tables = BoardSetupTables::from_payload(payload)
            .map_err(|_| garden_graphics::Error::PayloadIdentity)?;
        Ok(Self {
            resources,
            tables,
            backend,
            depth: 0,
        })
    }
    /// Exact even offsets within the decoded payload, never cartridge offsets.
    pub fn call(
        &mut self,
        m: &mut dyn FrameMemory,
        offset: u32,
        args: &[u32],
    ) -> Result<u32, FrameError> {
        if offset & 1 != 0 || offset as usize >= crate::garden::PAYLOAD_LENGTH || self.depth >= 64 {
            return Err(error(offset, "invalid Garden entry or call-depth limit"));
        }
        self.depth += 1;
        let result = self.dispatch(m, offset, args);
        self.depth -= 1;
        result
    }
    fn dispatch(
        &mut self,
        m: &mut dyn FrameMemory,
        offset: u32,
        args: &[u32],
    ) -> Result<u32, FrameError> {
        match (offset, args) {
            (0xd294, []) => {
                let mut resources = self.resources.clone();
                garden_graphics::initialize(m, &mut resources, self)
                    .map_err(|e| error(offset, e))?;
            }
            (0xd504 | 0xd7cc, []) => {
                let tables = self.tables.clone();
                garden_payload::dispatch_board(m, &tables, 0x02000001 + offset, self)
                    .map_err(|e| board_error(offset, e))?;
            }
            (0x1d68, []) => garden_palette::update(m, self)?,
            (0x168, []) => garden_frame::initialize(m, self)?,
            (0xbd0, []) => garden_frame::poll_input(m, self)?,
            (0xc6c, []) => garden_frame::initialize_input(m)?,
            (0xc90 | 0xd04 | 0x167c | 0x1698 | 0x16d8 | 0x12e0, _) => {
                return NativeBoardCalls
                    .invoke(0x02000000 + offset, args, m)
                    .map_err(|e| board_error(offset, e));
            }
            (0x163c, &[source, count]) => garden_graphics::apply_register_script(m, source, count)
                .map_err(|e| error(offset, e))?,
            (0x1718, &[source, dest, width, height, delta]) => {
                garden_graphics::stamp_map(m, source, dest, width, height, delta)
                    .map_err(|e| error(offset, e))?
            }
            (0xd294 | 0xd504 | 0xd7cc | 0x1d68 | 0x168 | 0xbd0 | 0xc6c | 0x163c | 0x1718, _) => {
                return Err(error(offset, "invalid native Garden arguments"));
            }
            _ => return self.backend.invoke(m, offset, args),
        }
        // These source routines do not expose a consumed return register.
        Ok(0)
    }
    pub fn tick(&mut self, m: &mut dyn FrameMemory) -> Result<(), FrameError> {
        garden_frame::tick(m, self)
    }
}
fn error(offset: u32, reason: impl ToString) -> FrameError {
    FrameError::Dependency {
        call: NativeCall {
            entry_offset: 0x02000000 + offset,
            argument0: None,
        },
        reason: reason.to_string(),
    }
}
fn board_error(offset: u32, e: BoardError) -> FrameError {
    match e {
        BoardError::Memory(e) => e,
        BoardError::UnportedCall { entry, args } => FrameError::Dependency {
            call: NativeCall {
                entry_offset: entry,
                argument0: args.first().copied(),
            },
            reason: format!("unported Garden call with arguments {args:#x?}"),
        },
        _ => error(offset, e),
    }
}
impl<B: Backend> garden_graphics::Calls for Runtime<'_, B> {
    fn call(
        &mut self,
        m: &mut dyn FrameMemory,
        offset: u32,
        args: &[u32],
    ) -> Result<(), FrameError> {
        Runtime::call(self, m, offset, args).map(|_| ())
    }
}
impl<B: Backend> BoardCalls for Runtime<'_, B> {
    fn invoke(
        &mut self,
        entry: u32,
        args: &[u32],
        m: &mut dyn FrameMemory,
    ) -> Result<u32, BoardError> {
        let offset = entry
            .checked_sub(0x02000000)
            .ok_or_else(|| BoardError::UnportedCall {
                entry,
                args: args.to_vec(),
            })?;
        self.call(m, offset, args).map_err(BoardError::Memory)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::memory::{MemoryImage, Region};
    fn reg(base: u32, bytes: Vec<u8>) -> Region {
        Region {
            base,
            bytes,
            writable: true,
        }
    }
    #[test]
    #[ignore = "Requires local SPP_ROM; complete puzzle setup using original tables, no service stubs"]
    fn source_board_setup_runs_without_mock_callbacks_then_stops_at_actual_frame_dependency() {
        use std::io::Read;
        let mut rom = vec![];
        std::fs::File::open(std::env::var_os("SPP_ROM").expect("set SPP_ROM"))
            .unwrap()
            .take(0x800001)
            .read_to_end(&mut rom)
            .unwrap();
        let payload = crate::catalog::extract_garden(&rom).unwrap().bytes;
        for round in 0..4 {
            let mut m = MemoryImage::new(
                &rom,
                vec![
                    reg(0x03000000, vec![0; 0x7000]),
                    reg(0x06000000, vec![0; 0x18000]),
                    reg(0x07000000, vec![0; 0x400]),
                    reg(0x04000000, vec![0; 0x400]),
                ],
            )
            .unwrap();
            m.write_u8(0x03001ad1, round).unwrap();
            let mut runtime = Runtime::new(&payload, RejectBackend).unwrap();
            runtime.call(&mut m, 0xd504, &[]).unwrap();
            assert_eq!(m.read_u32(garden_frame::GLOBALS).unwrap(), 0x0200d7cd);
            assert_eq!(m.read_u8(0x03000606).unwrap(), 4);
            // Executed register/OAM services remain visible before the real
            // unresolved map-write queue. No callback is silently discarded.
            let stop = runtime.call(&mut m, 0xd7cc, &[]).unwrap_err();
            assert!(matches!(
                stop,
                FrameError::Dependency {
                    call: NativeCall {
                        entry_offset: 0x02001420,
                        ..
                    },
                    ..
                }
            ));
            assert_eq!(m.read_u16(0x04000000).unwrap(), 8000);
            assert_eq!(m.read_u32(garden_frame::GLOBALS).unwrap(), 0x0200d7cd);
        }
    }
}
