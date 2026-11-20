//! Compose completed embedded-Garden Rust ports on one bounded memory image.
//! Unknown boot, care, task, audio, save and hardware calls remain errors.
use super::{
    frame::{FrameError, FrameMemory, NativeCall},
    garden_audio, garden_audio_init, garden_care, garden_frame, garden_graphics, garden_home,
    garden_home_tasks, garden_palette,
    garden_payload::{self, BoardCalls, BoardError, BoardSetupTables, NativeBoardCalls},
    garden_puzzle_objects, garden_save, garden_services, garden_shop, garden_transfer,
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
    audio: garden_audio::Bank,
    pub backend: B,
    depth: u8,
}
impl<'a, B: Backend> Runtime<'a, B> {
    pub fn new(payload: &'a [u8], backend: B) -> Result<Self, garden_graphics::Error> {
        let resources = garden_graphics::GardenPayload::new(payload)?;
        let tables = BoardSetupTables::from_payload(payload)
            .map_err(|_| garden_graphics::Error::PayloadIdentity)?;
        let audio = garden_audio::Bank::load(payload)
            .map_err(|_| garden_graphics::Error::PayloadIdentity)?;
        Ok(Self {
            resources,
            tables,
            audio,
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
        if offset & 1 != 0 || offset as usize >= crate::garden::PAYLOAD_LENGTH {
            return Err(FrameError::Dependency {
                call: NativeCall {
                    entry_offset: offset,
                    argument0: args.first().copied(),
                },
                reason: "invalid Garden payload offset; address retained as supplied".into(),
            });
        }
        if self.depth >= 64 {
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
            (0x105e8 | 0x10660 | 0x108a4 | 0x109e8 | 0x10b84 | 0x10c70 | 0x10cec, _) => {
                let bank = self.audio.clone();
                garden_audio_init::execute(m, self, &bank, offset, args).map_err(|e| match e {
                    garden_audio_init::Error::Memory(e) => e,
                    garden_audio_init::Error::Audio(garden_audio::Error::Memory(e)) => e,
                    e => error(offset, e),
                })?;
            }
            (
                0x480 | 0x590 | 0x6b8 | 0x988 | 0xa40 | 0xaf0 | 0xb0c | 0xb64 | 0xbb0 | 0x2eb8
                | 0x338c | 0x2364 | 0x2510,
                _,
            ) => {
                if matches!(offset, 0x2eb8 | 0x338c) {
                    task_argument(offset, args)?;
                }
                return garden_transfer::dispatch(m, self, offset, args);
            }
            (
                0x105ac | 0x105c8 | 0x1066c | 0x10698 | 0x106e4 | 0x10738 | 0x1076c | 0x107a0
                | 0x107cc | 0x107d8 | 0x10804 | 0x10814 | 0x10834 | 0x10d28 | 0x10da0 | 0x10e84
                | 0x10240,
                _,
            ) => {
                let n = match offset {
                    0x10d28 => 3,
                    0x10240 | 0x10da0 | 0x105c8 | 0x10804 | 0x10814 | 0x10834 => 2,
                    0x107a0 | 0x107d8 => 0,
                    _ => 1,
                };
                if args.len() != n {
                    return Err(error(offset, "invalid Garden audio arguments"));
                }
                let mut a = [0; 3];
                a[..n].copy_from_slice(args);
                let bank = self.audio.clone();
                garden_audio::execute(m, self, &bank, offset, a).map_err(|e| match e {
                    garden_audio::Error::Memory(e) => e,
                    e => error(offset, e),
                })?;
            }
            (
                0x1420 | 0xeac | 0xed4 | 0xf30 | 0x13dc | 0x1374 | 0x1398 | 0x117c | 0x1088
                | 0x151c | 0x1528 | 0x1534 | 0x17a4 | 0x17b0 | 0x1254 | 0xdb0 | 0xe28 | 0xf34
                | 0xfd4,
                _,
            ) => {
                if offset == 0xed4 {
                    task_argument(offset, args)?;
                }
                return garden_services::dispatch(m, self, offset, args);
            }
            (0xea5c, &[row, col]) => {
                return garden_payload::flip_bracketed_balls(m, row as u8, col as u8, self)
                    .map(u32::from)
                    .map_err(|e| board_error(offset, e))
            }
            (0x41e0 | 0x4418 | 0x5e58 | 0x5efc | 0x6570, _) => {
                task_argument(offset, args)?;
                return garden_care::dispatch(m, offset, args, self);
            }
            (
                0x8a74 | 0x8ab8 | 0x7240 | 0x73a8 | 0x6a54 | 0x3424 | 0x35cc | 0x74c0 | 0x89b0
                | 0x83e4 | 0x3e04 | 0x8a04 | 0x218c | 0x89d4 | 0x88b0 | 0x84ec | 0x3eb4 | 0x8b1c
                | 0x292c | 0x3748 | 0x3614 | 0x36ec | 0x3c24 | 0x3d58 | 0x85e0 | 0x8648 | 0x8d90,
                _,
            ) => {
                task_argument(offset, args)?;
                return garden_home_tasks::dispatch(m, self, offset, args);
            }
            (0xc4 | 0x19c | 0x228 | 0x18a0 | 0x18b4 | 0x18c8 | 0x1c60 | 0xbe24 | 0xd27c, _) => {
                let mut resources = self.resources.clone();
                return garden_home::dispatch(m, &mut resources, self, offset, args);
            }
            (0x248, []) => {
                garden_save::load(m, self).map_err(|e| save_error(offset, e))?;
            }
            (0x3ac, []) => {
                garden_save::write(m, self).map_err(|e| save_error(offset, e))?;
            }
            (0x43c, []) => {
                garden_save::initialize(m).map_err(|e| save_error(offset, e))?;
            }
            (
                0x7500 | 0x7688 | 0x7978 | 0x7b00 | 0x7c4c | 0x7da0 | 0x8010 | 0x80e0 | 0x8324
                | 0x8b3c | 0x8b7c | 0x8bc4 | 0x8c0c | 0x8c50 | 0x8c9c | 0x8cd4 | 0x8d04 | 0x8d3c,
                _,
            ) => {
                task_argument(offset, args)?;
                return garden_shop::dispatch(m, self, offset, args);
            }
            (
                0xe02c | 0xe08c | 0xe134 | 0xe430 | 0xf194 | 0xf218 | 0xf404 | 0xf518 | 0xf72c
                | 0xf778 | 0xf624 | 0xf814 | 0xf8d0 | 0xf8f8 | 0xeb08 | 0xeb9c | 0xecc8 | 0xed90
                | 0xeef8 | 0xef50 | 0xf010 | 0xf0f0 | 0xf688 | 0xf6bc | 0xdfbc | 0xe3b4 | 0xd8a4
                | 0xd9a8 | 0xda04 | 0xda58 | 0xdac0 | 0xf7dc | 0xdb28 | 0xdba8 | 0xde24 | 0xe1f0
                | 0xe6d8 | 0xe888 | 0xf2dc | 0xf85c | 0xf888,
                _,
            ) => {
                if !matches!(
                    offset,
                    0xf624
                        | 0xf8f8
                        | 0xeb08
                        | 0xeb9c
                        | 0xecc8
                        | 0xed90
                        | 0xeef8
                        | 0xef50
                        | 0xf010
                        | 0xf0f0
                        | 0xdb28
                        | 0xe6d8
                        | 0xe888
                        | 0xf2dc
                ) {
                    task_argument(offset, args)?;
                }
                return garden_puzzle_objects::dispatch(m, self, offset, args);
            }
            (0xd294, []) => {
                let mut resources = self.resources.clone();
                garden_graphics::initialize(m, &mut resources, self)
                    .map_err(|e| garden_graphics::frame_error(offset, e))?;
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
                .map_err(|e| garden_graphics::frame_error(offset, e))?,
            (0x1718, &[source, dest, width, height, delta]) => {
                garden_graphics::stamp_map(m, source, dest, width, height, delta)
                    .map_err(|e| garden_graphics::frame_error(offset, e))?
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
fn task_argument(offset: u32, args: &[u32]) -> Result<(), FrameError> {
    let Some(&task) = args.first() else {
        return Err(error(offset, "missing task argument"));
    };
    if task
        .checked_sub(0x03005320)
        .is_none_or(|delta| delta >= 72 * 36 || delta % 36 != 0)
    {
        return Err(error(
            offset,
            "task pointer is outside the aligned native pool",
        ));
    }
    Ok(())
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
fn save_error(offset: u32, e: garden_save::Error) -> FrameError {
    match e {
        garden_save::Error::Memory(e) => e,
        garden_save::Error::Unported(call) => FrameError::Dependency {
            call: NativeCall {
                entry_offset: 0x02000000 + call.entry_offset(),
                argument0: call.arguments().first().copied(),
            },
            reason: "unported Garden save hardware".into(),
        },
        e => error(offset, e),
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
impl<B: Backend> garden_services::Calls for Runtime<'_, B> {
    fn invoke(
        &mut self,
        m: &mut dyn FrameMemory,
        offset: u32,
        args: &[u32],
    ) -> Result<u32, FrameError> {
        self.call(m, offset, args)
    }
}
impl<B: Backend> garden_care::Calls for Runtime<'_, B> {
    fn call(
        &mut self,
        m: &mut dyn FrameMemory,
        offset: u32,
        args: &[u32],
    ) -> Result<u32, FrameError> {
        Runtime::call(self, m, offset, args)
    }
}
impl<B: Backend> garden_home::Calls for Runtime<'_, B> {
    fn invoke(
        &mut self,
        m: &mut dyn FrameMemory,
        offset: u32,
        args: &[u32],
    ) -> Result<u32, FrameError> {
        self.call(m, offset, args)
    }
}
impl<B: Backend> garden_shop::Calls for Runtime<'_, B> {
    fn invoke(
        &mut self,
        m: &mut dyn FrameMemory,
        offset: u32,
        args: &[u32],
    ) -> Result<u32, FrameError> {
        self.call(m, offset, args)
    }
}
impl<B: Backend> garden_save::Calls for Runtime<'_, B> {
    fn call(
        &mut self,
        m: &mut dyn FrameMemory,
        call: garden_save::Call,
    ) -> Result<u32, garden_save::Error> {
        Runtime::call(self, m, call.entry_offset(), &call.arguments())
            .map_err(garden_save::Error::Memory)
    }
}
impl<B: Backend> garden_payload::BoardEffects for Runtime<'_, B> {
    fn invoke(
        &mut self,
        effect: garden_payload::BoardEffect,
        m: &mut dyn FrameMemory,
    ) -> Result<(), BoardError> {
        let (off, args): (_, &[u32]) = match effect {
            garden_payload::BoardEffect::RedrawMarks => (0xe6d8, &[]),
            garden_payload::BoardEffect::QueueBg0VerticalOffset => (0x1698, &[0x12, 0]),
        };
        self.call(m, off, args)
            .map(|_| ())
            .map_err(BoardError::Memory)
    }
}
impl<B: Backend> garden_audio::Calls for Runtime<'_, B> {
    fn call(
        &mut self,
        m: &mut dyn FrameMemory,
        call: garden_audio::Call,
    ) -> Result<(), garden_audio::Error> {
        let offset = call
            .target()
            .checked_sub(0x02000001)
            .filter(|x| x & 1 == 0)
            .ok_or_else(|| garden_audio::Error::Pointer(call.target()))?;
        Runtime::call(self, m, offset, &call.arguments())
            .map(|_| ())
            .map_err(garden_audio::Error::Memory)
    }
}
impl<B: Backend> garden_audio_init::Calls for Runtime<'_, B> {
    fn invoke(
        &mut self,
        m: &mut dyn FrameMemory,
        call: garden_audio_init::Call,
    ) -> Result<u32, garden_audio_init::Error> {
        use garden_audio_init::{Call, Error};
        match call {
            Call::ReadRegister { address, width } => match width {
                1 => m.read_u8(address).map(u32::from),
                2 => m.read_u16(address).map(u32::from),
                4 => m.read_u32(address),
                _ => return Err(Error::Pointer(address)),
            }
            .map_err(Error::Memory),
            Call::WriteRegister {
                address,
                width,
                value,
            } => {
                match width {
                    1 => m.write_u8(address, value as u8),
                    2 => m.write_u16(address, value as u16),
                    4 => m.write_bytes(address, &value.to_le_bytes()),
                    _ => return Err(Error::Pointer(address)),
                }
                .map_err(Error::Memory)?;
                Ok(0)
            }
            Call::Frequency { mode } => self.call(m, 0x10ae0, &[mode]).map_err(Error::Memory),
            Call::Mixer => self.call(m, 0xf938, &[]).map_err(Error::Memory),
            Call::Player(call) => garden_audio::Calls::call(self, m, call)
                .map(|_| 0)
                .map_err(Error::Audio),
        }
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
    fn raw_task_apis_reject_overflow_before_any_write_or_dependency() {
        struct Never;
        impl garden_shop::Calls for Never {
            fn invoke(
                &mut self,
                _: &mut dyn FrameMemory,
                _: u32,
                _: &[u32],
            ) -> Result<u32, FrameError> {
                panic!("invalid task reached dependency")
            }
        }
        impl garden_home::Calls for Never {
            fn invoke(
                &mut self,
                _: &mut dyn FrameMemory,
                _: u32,
                _: &[u32],
            ) -> Result<u32, FrameError> {
                panic!("invalid task reached dependency")
            }
        }
        let mut m = MemoryImage::fixture(vec![reg(0x03000000, vec![0x5a; 0x8000])]);
        let before = crate::sha256(m.bytes(0x03000000, 0x8000).unwrap());
        assert!(garden_shop::dispatch(&mut m, &mut Never, 0x7500, &[u32::MAX]).is_err());
        assert!(garden_puzzle_objects::dispatch(&mut m, &mut Never, 0xf8d0, &[u32::MAX]).is_err());
        assert!(garden_transfer::dispatch(&mut m, &mut Never, 0x2eb8, &[u32::MAX]).is_err());
        assert_eq!(before, crate::sha256(m.bytes(0x03000000, 0x8000).unwrap()));
    }
    #[test]
    #[ignore = "Requires local SPP_ROM; joined callback execution with original tables and no inner stubs"]
    fn joined_board_frames_match_native_tasks_controls_rules_graphics_and_sound_routing() {
        use std::io::Read;
        let mut rom = vec![];
        std::fs::File::open(std::env::var_os("SPP_ROM").expect("set SPP_ROM"))
            .unwrap()
            .take(0x800001)
            .read_to_end(&mut rom)
            .unwrap();
        let payload = crate::catalog::extract_garden(&rom).unwrap().bytes;
        let f: serde_json::Value =
            serde_json::from_str(vectors!("garden-runtime-vectors.json")).unwrap();
        let regions = [
            (0x02000000, 0x40000),
            (0x03000000, 0x7000),
            (0x04000000, 0x400),
            (0x05000000, 0x400),
            (0x06000000, 0x18000),
            (0x07000000, 0x400),
        ];
        let digest = |m: &MemoryImage<'_>| {
            crate::sha256(
                &regions
                    .into_iter()
                    .flat_map(|(a, n)| m.bytes(a, n).unwrap().iter().copied())
                    .collect::<Vec<_>>(),
            )
        };
        for c in f["cases"].as_array().unwrap() {
            let mut ewram = vec![0; 0x40000];
            ewram[..payload.len()].copy_from_slice(&payload);
            let mut m = MemoryImage::new(
                &rom,
                vec![
                    reg(0x02000000, ewram),
                    reg(0x03000000, vec![0; 0x8000]),
                    reg(0x06000000, vec![0; 0x18000]),
                    reg(0x07000000, vec![0; 0x400]),
                    reg(0x04000000, vec![0; 0x400]),
                    reg(0x05000000, vec![0; 0x400]),
                ],
            )
            .unwrap();
            m.write_u8(0x03001ad1, c["round"].as_u64().unwrap() as u8)
                .unwrap();
            m.write_bytes(
                0x030013a8,
                &(c["seed"].as_u64().unwrap() as u32).to_le_bytes(),
            )
            .unwrap();
            let mut runtime = Runtime::new(&payload, RejectBackend).unwrap();
            for bad in [u32::MAX, 0x80000000, 1] {
                assert!(
                    matches!(runtime.call(&mut m,bad,&[]),Err(FrameError::Dependency{call,..})if call.entry_offset==bad)
                );
            }
            for bad_task in [0x03004aa0, 0x03005321, 0x03005d40, u32::MAX] {
                assert!(runtime.call(&mut m, 0x4418, &[bad_task]).is_err());
            }
            runtime.call(&mut m, 0xd504, &[]).unwrap();
            assert_eq!(m.read_u32(garden_frame::GLOBALS).unwrap(), 0x0200d7cd);
            assert_eq!(m.read_u8(0x03000606).unwrap(), 4);
            assert_eq!(digest(&m), c["afterSetup"].as_str().unwrap());
            for (tick, frame) in c["frames"].as_array().unwrap().iter().enumerate() {
                let held = frame["held"].as_u64().unwrap() as u16;
                m.write_u16(garden_frame::INPUT, held).unwrap();
                m.write_u16(garden_frame::INPUT + 2, !held).unwrap();
                m.write_u16(
                    garden_frame::INPUT + 4,
                    frame["pressed"].as_u64().unwrap() as u16,
                )
                .unwrap();
                runtime.call(&mut m, 0xd7cc, &[]).unwrap();
                assert_eq!(
                    digest(&m),
                    frame["sha256"].as_str().unwrap(),
                    "round{} profile{} tick{tick}",
                    c["round"],
                    c["profile"]
                );
            }
        }
        for c in f["homeCases"].as_array().unwrap() {
            let mut ewram = vec![0; 0x40000];
            ewram[..payload.len()].copy_from_slice(&payload);
            let mut source_regions = vec![reg(0x02000000, ewram)];
            source_regions.extend(
                regions[1..]
                    .iter()
                    .map(|&(a, n)| reg(a, vec![0; if a == 0x03000000 { 0x8000 } else { n }])),
            );
            let mut m = MemoryImage::new(&rom, source_regions).unwrap();
            let mut runtime = Runtime::new(&payload, RejectBackend).unwrap();
            runtime.call(&mut m, 0x43c, &[]).unwrap();
            m.write_u8(0x03004260, c["language"].as_u64().unwrap() as u8)
                .unwrap();
            if c["present"].as_bool().unwrap() {
                m.write_u8(0x03004af8, 0).unwrap();
            }
            m.write_bytes(0x03004abf, &[88, 104]).unwrap();
            m.write_bytes(0x03006194, &1000u32.to_le_bytes()).unwrap();
            runtime.call(&mut m, 0x18c8, &[]).unwrap();
            assert_eq!(digest(&m), c["afterSetup"].as_str().unwrap());
            for (tick, frame) in c["frames"].as_array().unwrap().iter().enumerate() {
                runtime.call(&mut m, 0x1d68, &[]).unwrap();
                assert_eq!(
                    digest(&m),
                    frame.as_str().unwrap(),
                    "home language{} present{} tick{tick}",
                    c["language"],
                    c["present"]
                );
            }
        }
    }
}
