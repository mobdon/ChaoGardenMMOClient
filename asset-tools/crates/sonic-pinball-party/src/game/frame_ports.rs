//! Bounded native-address adapter for the completed A3VE frame routines.
//! Every global is read from the supplied snapshot. Missing calls, bindings,
//! BIOS services or sound execution stop the frame explicitly.
/// Exact native helper arguments, including consumed stack arguments, are
/// forwarded synchronously through the lifecycle/table-event helper contract.
pub use super::lifecycle::LifecycleCalls as FrameHelpers;
use super::{
    collision, flippers, frame, lifecycle, lifecycle_tables, math, nudge, table_events, table_frame,
};
use frame::{CanonicalState, FrameCalls, FrameError, FrameMemory, NativeCall};

/// A source-resolved mask allocation, identified by its original ROM pointer.
/// Samples must come from `collision::read_mask_planes` on the verified ROM.
pub struct MaskBinding<'a> {
    pub address: u32,
    pub samples: &'a [u16],
}

pub struct PortResources<'a, 'rom> {
    pub geometry: &'a flippers::Geometry,
    pub trig: &'a math::TrigTable,
    pub response: &'a collision::ResponseTables,
    pub lookups: &'a collision::FlipperLookups,
    pub regions: &'a collision::MapRegions<'rom>,
    pub masks: &'a [MaskBinding<'a>],
}

/// Executes the native force-channel-2 sound wrapper at 0x38d4. An audio
/// implementation must carry the resulting native player effects through its
/// own strict boundary; collecting a request alone is not completed execution.
pub trait FrameAudio {
    fn force_channel2(&mut self, memory: &mut dyn FrameMemory, song: u16)
        -> Result<(), FrameError>;
}
pub struct RejectFrameAudio;
impl FrameAudio for RejectFrameAudio {
    fn force_channel2(&mut self, _: &mut dyn FrameMemory, song: u16) -> Result<(), FrameError> {
        Err(FrameError::UnportedNativeCall(NativeCall {
            entry_offset: 0x38d4,
            argument0: Some(u32::from(song)),
        }))
    }
}

pub struct RejectFrameHelpers;
impl FrameHelpers for RejectFrameHelpers {
    fn call(
        &mut self,
        _: &mut dyn FrameMemory,
        offset: u32,
        args: &[u32],
    ) -> Result<(), FrameError> {
        Err(FrameError::Dependency {
            call: NativeCall {
                entry_offset: offset,
                argument0: args.first().copied(),
            },
            reason: format!("unported native lifecycle/table helper with arguments {args:#x?}"),
        })
    }
}

pub struct FramePorts<'a, 'rom, B, A = RejectFrameAudio, H = RejectFrameHelpers> {
    resources: PortResources<'a, 'rom>,
    pub bios: B,
    pub audio: A,
    pub helpers: H,
    nudge_curve: Option<&'a nudge::Curve>,
    /// Exact address of the incoming resolver stack halfword, entry SP - 80.
    /// This is a snapshot binding, not an invented initialized scratch value.
    pub collision_scratch_address: Option<u32>,
}
impl<'a, 'rom, B: math::BiosMath> FramePorts<'a, 'rom, B> {
    pub fn new(
        resources: PortResources<'a, 'rom>,
        bios: B,
        collision_scratch_address: Option<u32>,
    ) -> Self {
        Self {
            resources,
            bios,
            audio: RejectFrameAudio,
            helpers: RejectFrameHelpers,
            nudge_curve: None,
            collision_scratch_address,
        }
    }
}
impl<'a, 'rom, B: math::BiosMath, A: FrameAudio, H: FrameHelpers> FramePorts<'a, 'rom, B, A, H> {
    pub fn with_audio<T: FrameAudio>(self, audio: T) -> FramePorts<'a, 'rom, B, T, H> {
        FramePorts {
            resources: self.resources,
            bios: self.bios,
            audio,
            helpers: self.helpers,
            nudge_curve: self.nudge_curve,
            collision_scratch_address: self.collision_scratch_address,
        }
    }
    pub fn with_helpers<T: FrameHelpers>(self, helpers: T) -> FramePorts<'a, 'rom, B, A, T> {
        FramePorts {
            resources: self.resources,
            bios: self.bios,
            audio: self.audio,
            helpers,
            nudge_curve: self.nudge_curve,
            collision_scratch_address: self.collision_scratch_address,
        }
    }
    pub fn with_nudge_curve(mut self, curve: &'a nudge::Curve) -> Self {
        self.nudge_curve = Some(curve);
        self
    }
}

fn dependency(call: NativeCall, reason: impl Into<String>) -> FrameError {
    FrameError::Dependency {
        call,
        reason: reason.into(),
    }
}
fn read_bytes<const N: usize>(
    memory: &dyn FrameMemory,
    address: u32,
) -> Result<[u8; N], FrameError> {
    let mut result = [0; N];
    for (index, byte) in result.iter_mut().enumerate() {
        *byte = memory.read_u8(address.wrapping_add(index as u32))?;
    }
    Ok(result)
}
fn write_changed(
    memory: &mut dyn FrameMemory,
    address: u32,
    before: &[u8],
    after: &[u8],
) -> Result<(), FrameError> {
    for (index, (&old, &new)) in before.iter().zip(after).enumerate() {
        if old != new {
            memory.write_u8(address.wrapping_add(index as u32), new)?;
        }
    }
    Ok(())
}
fn write_u32_changed(
    memory: &mut dyn FrameMemory,
    address: u32,
    before: u32,
    after: u32,
) -> Result<(), FrameError> {
    write_changed(memory, address, &before.to_le_bytes(), &after.to_le_bytes())
}
fn load_state(
    memory: &dyn FrameMemory,
    call: NativeCall,
) -> Result<(u32, CanonicalState), FrameError> {
    let pointer = memory.read_u32(frame::STATE_POINTER)?;
    let last = pointer
        .checked_add(flippers::STATE_BYTES as u32 - 1)
        .ok_or_else(|| dependency(call, "native state pointer overflow"))?;
    if !((0x02000000..=0x0203ffff).contains(&pointer) && last <= 0x0203ffff
        || (0x03000000..=0x03007fff).contains(&pointer) && last <= 0x03007fff)
    {
        return Err(dependency(
            call,
            format!("native state prefix outside work RAM: {pointer:08x}"),
        ));
    }
    Ok((
        pointer,
        CanonicalState::from_bytes(read_bytes(memory, pointer)?),
    ))
}

impl<B: math::BiosMath, A: FrameAudio, H: FrameHelpers> FrameCalls for FramePorts<'_, '_, B, A, H> {
    /// The frame controller ignores return registers from these routines.
    /// `Ok(0)` conveys successful completion, not a reconstructed ARM R0 value.
    fn call(&mut self, memory: &mut dyn FrameMemory, call: NativeCall) -> Result<u32, FrameError> {
        // These source ports operate directly on the same supplied memory and
        // reload its native pointer after synchronous helpers. Loading a cached
        // CanonicalState first would overwrite those pointer-switch effects.
        if call.entry_offset == 0x2e1a0 {
            lifecycle::dispatch(memory, &mut lifecycle_tables::TableCalls(&mut self.helpers))?;
            return Ok(0);
        }
        if call.entry_offset == 0x2ea04 {
            table_events::dispatch(memory, &mut self.helpers)?;
            return Ok(0);
        }
        if call.entry_offset == 0x2fd68 {
            table_frame::presentation(memory, &mut self.helpers)?;
            return Ok(0);
        }
        if !matches!(
            call.entry_offset,
            0x2ac18 | 0x2a04c | 0x2d698 | 0x2ba18 | 0x39ee4
        ) {
            return Err(FrameError::UnportedNativeCall(call));
        }
        let (pointer, mut state) = load_state(memory, call)?;
        let before = state.clone();
        match call.entry_offset {
            0x2ac18 => {
                let input = flippers::Inputs {
                    table_code: memory.read_u16(frame::TABLE_CODE)?,
                    left_mask: memory.read_u16(0x030040c4)?,
                    right_mask: memory.read_u16(0x030040c6)?,
                    held: memory.read_u16(0x03000514)?,
                    pressed: memory.read_u16(0x03000516)?,
                    audio_enabled: memory.read_u16(pointer.wrapping_add(0x6e2))?,
                };
                let mut flippers = state.flippers();
                let sounds = flippers
                    .update(input, self.resources.geometry)
                    .map_err(|error| dependency(call, error.to_string()))?;
                for song in sounds {
                    self.audio.force_channel2(memory, song)?;
                }
                state.apply_flippers(&flippers);
            }
            0x2a04c => {
                let mut ball = state.ball();
                ball.integrate(
                    memory.read_u16(frame::TABLE_CODE)?,
                    memory.read_u32(0x020328bc)?,
                );
                state.apply_ball(&ball);
            }
            0x2d698 => {
                if call.argument0 != Some(pointer.wrapping_add(0xa8)) {
                    return Err(dependency(
                        call,
                        "velocity pointer does not match the current canonical state + 0xa8",
                    ));
                }
                let mut ball = state.ball();
                ball.limit_velocity(self.resources.trig, &mut self.bios)
                    .map_err(|error| dependency(call, error.to_string()))?;
                state.apply_ball(&ball);
            }
            0x2ba18 => {
                self.collision(memory, pointer, &mut state, call)?;
            }
            0x39ee4 => {
                let curve = self.nudge_curve.ok_or_else(|| {
                    dependency(call, "missing verified native nudge curve binding")
                })?;
                let before_globals =
                    read_bytes::<{ nudge::GLOBAL_BYTES }>(memory, nudge::GLOBAL_ADDRESS)?;
                let mut nudge = nudge::Nudge::from_bytes(before_globals);
                let input = nudge::Inputs {
                    pressed: memory.read_u16(0x03000516)?,
                    left_mask: memory.read_u16(0x030040c8)?,
                    right_mask: memory.read_u16(0x030040ca)?,
                    up_mask: memory.read_u16(0x030040cc)?,
                };
                let mut ball = state.ball();
                nudge
                    .update(&mut ball, input, curve)
                    .map_err(|error| dependency(call, error.to_string()))?;
                state.apply_ball(&ball);
                write_changed(
                    memory,
                    nudge::GLOBAL_ADDRESS,
                    &before_globals,
                    nudge.as_bytes(),
                )?;
            }
            _ => unreachable!("checked native leaf"),
        }
        write_changed(memory, pointer, before.as_bytes(), state.as_bytes())?;
        Ok(0)
    }
}

impl<B: math::BiosMath, A: FrameAudio, H: FrameHelpers> FramePorts<'_, '_, B, A, H> {
    fn collision(
        &mut self,
        memory: &mut dyn FrameMemory,
        pointer: u32,
        state: &mut CanonicalState,
        call: NativeCall,
    ) -> Result<(), FrameError> {
        let scratch_address = self.collision_scratch_address.ok_or_else(|| {
            dependency(
                call,
                "missing native caller stack halfword binding (resolver entry SP - 80)",
            )
        })?;
        let scratch = memory.read_u16(scratch_address)?;
        let table_id = memory.read_u16(frame::TABLE_CODE)?;
        let mut region_flags = [0; 11];
        for (index, flag) in region_flags.iter_mut().enumerate() {
            *flag = memory.read_u16(0x02032980 + index as u32 * 2)?;
        }
        let mut layers = [[0; 3]; 2];
        for (layer, banks) in layers.iter_mut().enumerate() {
            for (kind, value) in banks.iter_mut().enumerate() {
                *value = memory.read_u32(
                    pointer.wrapping_add([0x118, 0x158, 0x198][kind] + layer as u32 * 4),
                )?;
            }
        }
        let mut origins = [[0; 2]; 4];
        for (index, origin) in origins.iter_mut().enumerate() {
            *origin = [
                memory.read_u16(pointer.wrapping_add(0x110 + index as u32 * 2))? as i16,
                memory.read_u16(pointer.wrapping_add([0x10a, 0x10a, 0x10c, 0x10e][index]))? as i16,
            ];
        }
        let main_pointer = memory.read_u32(pointer.wrapping_add(0x1d8))?;
        let mask = |address| {
            self.resources
                .masks
                .iter()
                .find(|binding| binding.address == address)
                .map(|binding| binding.samples)
                .ok_or_else(|| {
                    dependency(
                        call,
                        format!("unbound native flipper mask allocation {address:08x}"),
                    )
                })
        };
        let uses_extra = table_id == 1
            || (table_id == 2
                && u16::from_le_bytes([state.as_bytes()[0x28], state.as_bytes()[0x29]]) == 0);
        // Original classifier visits two sides on this branch, so the extra
        // allocation is never read. An empty slice does not fabricate a mask.
        let extra = if uses_extra {
            mask(memory.read_u32(pointer.wrapping_add(0x1dc))?)?
        } else {
            &[]
        };
        let masks = collision::FlipperMasks {
            main: mask(main_pointer)?,
            extra,
        };
        let angle = memory.read_u32(0x02032770)?;
        let counter = memory.read_u32(0x030045c4)?;
        let mut history = [0; 256];
        for (index, value) in history.iter_mut().enumerate() {
            *value = memory.read_u32(0x030040f0 + index as u32 * 4)?;
        }
        let mut events = [0; 30];
        for (index, value) in events.iter_mut().enumerate() {
            *value = memory.read_u16(0x02034f50 + index as u32 * 2)?;
        }
        let mut globals = collision::ResponseState {
            angle,
            counter,
            history,
            events,
        };
        let old_globals = globals.clone();
        let mut field = collision::CartridgeField {
            table_id,
            region_flags,
            layers,
            origins,
            flippers: state.flippers(),
            masks,
            lookups: self.resources.lookups,
            regions: self.resources.regions,
        };
        let mut ball = state.ball();
        collision::resolve_with_bios(
            &mut field,
            table_id,
            &mut ball,
            scratch,
            self.resources.response,
            &mut globals,
            &mut self.bios,
        )
        .map_err(|error| dependency(call, format!("native collision stopped: {error:?}")))?;
        state.apply_collision_pair(&ball, &field.flippers);
        for (layer, banks) in layers.iter().enumerate() {
            for (kind, value) in banks.iter().enumerate() {
                write_u32_changed(
                    memory,
                    pointer.wrapping_add([0x118, 0x158, 0x198][kind] + layer as u32 * 4),
                    *value,
                    field.layers[layer][kind],
                )?;
            }
        }
        write_u32_changed(memory, 0x02032770, old_globals.angle, globals.angle)?;
        write_u32_changed(memory, 0x030045c4, old_globals.counter, globals.counter)?;
        for (index, (&old, &new)) in old_globals.history.iter().zip(&globals.history).enumerate() {
            write_u32_changed(memory, 0x030040f0 + index as u32 * 4, old, new)?;
        }
        for (index, (&old, &new)) in old_globals.events.iter().zip(&globals.events).enumerate() {
            write_changed(
                memory,
                0x02034f50 + index as u32 * 2,
                &old.to_le_bytes(),
                &new.to_le_bytes(),
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::collections::{BTreeMap, VecDeque};

    const STATE: u32 = 0x02000000;
    const SCRATCH: u32 = 0x03007600 - 80;
    #[derive(Default, Clone, PartialEq, Eq)]
    struct Memory(BTreeMap<u32, u8>);
    impl Memory {
        fn bind(&mut self, address: u32, bytes: &[u8]) {
            for (index, value) in bytes.iter().enumerate() {
                self.0.insert(address + index as u32, *value);
            }
        }
        fn bind_u16(&mut self, address: u32, value: u16) {
            self.bind(address, &value.to_le_bytes());
        }
        fn bind_u32(&mut self, address: u32, value: u32) {
            self.bind(address, &value.to_le_bytes());
        }
        fn read<const N: usize>(&self, address: u32) -> Result<[u8; N], FrameError> {
            let mut value = [0; N];
            for (index, byte) in value.iter_mut().enumerate() {
                *byte = *self
                    .0
                    .get(&(address + index as u32))
                    .ok_or(FrameError::MissingRead {
                        address,
                        width: N as u8,
                    })?;
            }
            Ok(value)
        }
    }
    impl FrameMemory for Memory {
        fn read_u8(&self, address: u32) -> Result<u8, FrameError> {
            Ok(self.read::<1>(address)?[0])
        }
        fn read_u16(&self, address: u32) -> Result<u16, FrameError> {
            Ok(u16::from_le_bytes(self.read(address)?))
        }
        fn read_u32(&self, address: u32) -> Result<u32, FrameError> {
            Ok(u32::from_le_bytes(self.read(address)?))
        }
        fn write_u8(&mut self, address: u32, value: u8) -> Result<(), FrameError> {
            *self
                .0
                .get_mut(&address)
                .ok_or(FrameError::MissingWrite { address, width: 1 })? = value;
            Ok(())
        }
        fn write_u16(&mut self, address: u32, value: u16) -> Result<(), FrameError> {
            self.read_u16(address)?;
            self.bind_u16(address, value);
            Ok(())
        }
    }
    struct ControlledBios(VecDeque<[i64; 4]>);
    #[derive(Default)]
    struct CapturedTestAudio(Vec<u16>);
    impl FrameAudio for CapturedTestAudio {
        fn force_channel2(&mut self, _: &mut dyn FrameMemory, song: u16) -> Result<(), FrameError> {
            // Test-only capture matches the oracle's intercepted native call;
            // this is never presented as executing or playing native sound.
            self.0.push(song);
            Ok(())
        }
    }
    impl math::BiosMath for ControlledBios {
        fn sqrt(&mut self, value: u32) -> Result<u16, math::MathError> {
            let row = self.0.pop_front().expect("unexpected adapter BIOS sqrt");
            assert_eq!((row[0], row[1], row[2]), (8, i64::from(value), 0));
            Ok(row[3] as u16)
        }
        fn atan2(&mut self, x: i16, y: i16) -> Result<u16, math::MathError> {
            let row = self.0.pop_front().expect("unexpected adapter BIOS atan2");
            assert_eq!((row[0], row[1], row[2]), (10, i64::from(x), i64::from(y)));
            Ok(row[3] as u16)
        }
    }
    fn hex_bytes(hex: &str) -> Vec<u8> {
        (0..hex.len() / 2)
            .map(|index| u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).unwrap())
            .collect()
    }
    fn resources<'a, 'rom>(
        geometry: &'a flippers::Geometry,
        trig: &'a math::TrigTable,
        response: &'a collision::ResponseTables,
        lookups: &'a collision::FlipperLookups,
        regions: &'a collision::MapRegions<'rom>,
        masks: &'a [MaskBinding<'a>],
    ) -> PortResources<'a, 'rom> {
        PortResources {
            geometry,
            trig,
            response,
            lookups,
            regions,
            masks,
        }
    }
    #[test]
    fn absent_audio_and_native_memory_are_explicit_errors() {
        let call = NativeCall {
            entry_offset: 0x2a04c,
            argument0: None,
        };
        assert!(matches!(
            load_state(&Memory::default(), call),
            Err(FrameError::MissingRead {
                address: frame::STATE_POINTER,
                width: 4
            })
        ));
        assert_eq!(
            RejectFrameAudio.force_channel2(&mut Memory::default(), 0x39),
            Err(FrameError::UnportedNativeCall(NativeCall {
                entry_offset: 0x38d4,
                argument0: Some(0x39)
            }))
        );
        let mut memory = Memory::default();
        memory.bind_u32(frame::STATE_POINTER, 0x08000000);
        assert!(matches!(
            load_state(&memory, call),
            Err(FrameError::Dependency { .. })
        ));
    }
    #[test]
    #[ignore = "Requires local SPP_ROM; original native-map dispatch cases with controlled BIOS returns"]
    fn adapter_matches_1000_original_full_collision_dispatch_cases() {
        let rom = std::fs::read(std::env::var_os("SPP_ROM").expect("Set local SPP_ROM")).unwrap();
        let geometry = flippers::Geometry::load(&rom).unwrap();
        let trig = math::TrigTable::from_rom(&rom).unwrap();
        let response = collision::ResponseTables::from_rom(&rom).unwrap();
        let lookups = collision::FlipperLookups::from_rom(&rom).unwrap();
        let regions = collision::MapRegions::from_rom(&rom).unwrap();
        let samples = collision::read_mask_planes(&rom, 0x0866a0b8, 21).unwrap();
        let masks = [MaskBinding {
            address: 0x0866a0b8,
            samples: &samples,
        }];
        let vectors: Value = serde_json::from_str(vectors!("collision-vectors.json")).unwrap();
        let cases = vectors["retail"].as_array().unwrap();
        assert_eq!(cases.len(), 1000);
        for (index, case) in cases.iter().enumerate() {
            let table = case[0].as_u64().unwrap() as u16;
            let mut memory = Memory::default();
            // Explicit synthetic snapshots reproduce collision_reference.py's
            // initialized inputs, including its zeroed history/event arrays.
            memory.bind(STATE, &[0; 0x200]);
            memory.bind(0x030040f0, &[0; 1024]);
            memory.bind(0x02034f50, &[0; 60]);
            memory.bind_u32(frame::STATE_POINTER, STATE);
            memory.bind_u16(frame::TABLE_CODE, table);
            memory.bind_u16(SCRATCH, 0xbabe);
            memory.bind_u32(0x02032770, 0x12345678);
            memory.bind_u32(0x030045c4, 255);
            for byte in case[4].as_array().unwrap() {
                memory.bind(
                    STATE + byte[0].as_u64().unwrap() as u32,
                    &[byte[1].as_u64().unwrap() as u8],
                );
            }
            for (side, origin) in case[1].as_array().unwrap().iter().enumerate() {
                memory.bind_u16(
                    STATE + 0x110 + side as u32 * 2,
                    origin[0].as_i64().unwrap() as u16,
                );
                memory.bind_u16(
                    STATE + [0x10a, 0x10a, 0x10c, 0x10e][side],
                    origin[1].as_i64().unwrap() as u16,
                );
            }
            for (flag, value) in case[2].as_array().unwrap().iter().enumerate() {
                memory.bind_u16(0x02032980 + flag as u32 * 2, value.as_u64().unwrap() as u16);
            }
            for (layer, banks) in case[3].as_array().unwrap().iter().enumerate() {
                for (kind, value) in banks.as_array().unwrap().iter().enumerate() {
                    memory.bind_u32(
                        STATE + [0x118, 0x158, 0x198][kind] + layer as u32 * 4,
                        value.as_u64().unwrap() as u32,
                    );
                }
            }
            memory.bind_u32(STATE + 0x1d8, 0x0866a0b8);
            memory.bind_u32(STATE + 0x1dc, if table <= 2 { 0x0866a0b8 } else { 0 });
            let mut expected = memory.clone();
            for byte in case[5].as_array().unwrap() {
                expected.bind(
                    STATE + byte[0].as_u64().unwrap() as u32,
                    &[byte[1].as_u64().unwrap() as u8],
                );
            }
            for (layer, banks) in case[6].as_array().unwrap().iter().enumerate() {
                for (kind, value) in banks.as_array().unwrap().iter().enumerate() {
                    expected.bind_u32(
                        STATE + [0x118, 0x158, 0x198][kind] + layer as u32 * 4,
                        value.as_u64().unwrap() as u32,
                    );
                }
            }
            expected.bind_u32(0x030045c4, case[7].as_u64().unwrap() as u32);
            expected.bind_u32(0x02032770, case[8].as_u64().unwrap() as u32);
            for row in case[9].as_array().unwrap() {
                expected.bind_u16(
                    0x02034f50 + row[0].as_u64().unwrap() as u32 * 2,
                    row[1].as_u64().unwrap() as u16,
                );
            }
            for row in case[10].as_array().unwrap() {
                expected.bind_u32(
                    0x030040f0 + row[0].as_u64().unwrap() as u32 * 4,
                    row[1].as_u64().unwrap() as u32,
                );
            }
            let trace = case[11]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| std::array::from_fn(|i| row[i].as_i64().unwrap()))
                .collect();
            let mut ports = FramePorts::new(
                resources(&geometry, &trig, &response, &lookups, &regions, &masks),
                ControlledBios(trace),
                Some(SCRATCH),
            );
            ports
                .call(
                    &mut memory,
                    NativeCall {
                        entry_offset: 0x2ba18,
                        argument0: None,
                    },
                )
                .unwrap();
            assert!(
                ports.bios.0.is_empty(),
                "unconsumed native BIOS boundary case {index}"
            );
            assert_eq!(
                memory.0, expected.0,
                "complete adapter native-address state case {index}"
            );
        }
    }
    #[test]
    #[ignore = "Requires local SPP_ROM; verifies actual adapter leaf bindings and strict error boundaries"]
    fn adapter_runs_native_flipper_integrator_and_cap_and_rejects_unknown_calls() {
        let rom = std::fs::read(std::env::var_os("SPP_ROM").expect("Set local SPP_ROM")).unwrap();
        let geometry = flippers::Geometry::load(&rom).unwrap();
        let trig = math::TrigTable::from_rom(&rom).unwrap();
        let response = collision::ResponseTables::from_rom(&rom).unwrap();
        let lookups = collision::FlipperLookups::from_rom(&rom).unwrap();
        let regions = collision::MapRegions::from_rom(&rom).unwrap();
        let mut ports = FramePorts::new(
            resources(&geometry, &trig, &response, &lookups, &regions, &[]),
            math::RejectBiosMath,
            None,
        );
        let vectors: Value = serde_json::from_str(vectors!("ball-vectors.json")).unwrap();
        for (index, case) in vectors["integrator"].as_array().unwrap().iter().enumerate() {
            let mut memory = Memory::default();
            memory.bind(STATE, &[0x55; flippers::STATE_BYTES]);
            memory.bind(STATE, &hex_bytes(case["input"].as_str().unwrap()));
            memory.bind_u32(frame::STATE_POINTER, STATE);
            memory.bind_u16(frame::TABLE_CODE, case["table"].as_u64().unwrap() as u16);
            memory.bind_u32(0x020328bc, case["freeze"].as_u64().unwrap() as u32);
            ports
                .call(
                    &mut memory,
                    NativeCall {
                        entry_offset: 0x2a04c,
                        argument0: None,
                    },
                )
                .unwrap();
            assert_eq!(
                read_bytes::<{ super::super::ball::STATE_BYTES }>(&memory, STATE)
                    .unwrap()
                    .as_slice(),
                hex_bytes(case["state"].as_str().unwrap()),
                "adapter integrator {index}"
            );
            assert!(
                read_bytes::<{ flippers::STATE_BYTES }>(&memory, STATE).unwrap()
                    [super::super::ball::STATE_BYTES..]
                    .iter()
                    .all(|byte| *byte == 0x55)
            );
        }
        let mut empty = Memory::default();
        let unknown = NativeCall {
            entry_offset: 0x29c7c,
            argument0: None,
        };
        assert_eq!(
            ports.call(&mut empty, unknown),
            Err(FrameError::UnportedNativeCall(unknown))
        );
        let mut memory = Memory::default();
        memory.bind(STATE, &[0; flippers::STATE_BYTES]);
        memory.bind_u32(frame::STATE_POINTER, STATE);
        let before = memory.clone();
        assert!(ports
            .call(
                &mut memory,
                NativeCall {
                    entry_offset: 0x2d698,
                    argument0: Some(STATE + 0xa8)
                }
            )
            .is_err());
        assert_eq!(
            memory.0, before.0,
            "unavailable BIOS cannot mutate canonical velocity"
        );
        assert!(ports
            .call(
                &mut memory,
                NativeCall {
                    entry_offset: 0x2ba18,
                    argument0: None
                }
            )
            .is_err());
        assert_eq!(
            memory.0, before.0,
            "missing collision stack binding cannot fabricate globals"
        );
        let flipper_vectors: Value =
            serde_json::from_str(vectors!("flipper-native-vectors.json")).unwrap();
        let mut ports = ports.with_audio(CapturedTestAudio::default());
        for (index, case) in flipper_vectors["cases"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            let mut memory = Memory::default();
            memory.bind(STATE, &hex_bytes(case["input"].as_str().unwrap()));
            memory.bind_u32(frame::STATE_POINTER, STATE);
            let input = &case["inputs"];
            for (address, field) in [
                (frame::TABLE_CODE, "table_code"),
                (0x030040c4, "left_mask"),
                (0x030040c6, "right_mask"),
                (0x03000514, "held"),
                (0x03000516, "pressed"),
                (STATE + 0x6e2, "audio_enabled"),
            ] {
                memory.bind_u16(address, input[field].as_u64().unwrap() as u16);
            }
            ports.audio.0.clear();
            ports
                .call(
                    &mut memory,
                    NativeCall {
                        entry_offset: 0x2ac18,
                        argument0: None,
                    },
                )
                .unwrap();
            assert_eq!(
                read_bytes::<{ flippers::STATE_BYTES }>(&memory, STATE)
                    .unwrap()
                    .as_slice(),
                hex_bytes(case["state"].as_str().unwrap()),
                "adapter flippers {index}"
            );
            assert_eq!(
                ports.audio.0,
                case["sound_calls"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|value| value.as_u64().unwrap() as u16)
                    .collect::<Vec<_>>(),
                "explicit intercepted audio boundary {index}"
            );
        }
        for (index, case) in vectors["velocity_cap"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            let mut memory = Memory::default();
            memory.bind(STATE, &[0x55; flippers::STATE_BYTES]);
            memory.bind(STATE, &hex_bytes(case["input"].as_str().unwrap()));
            memory.bind_u32(frame::STATE_POINTER, STATE);
            let trace = case["bios_calls"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| {
                    [
                        row["service"].as_i64().unwrap(),
                        row["x"].as_i64().unwrap(),
                        row["y"].as_i64().unwrap_or(0),
                        row["result"].as_i64().unwrap(),
                    ]
                })
                .collect();
            let mut capped = FramePorts::new(
                resources(&geometry, &trig, &response, &lookups, &regions, &[]),
                ControlledBios(trace),
                None,
            );
            capped
                .call(
                    &mut memory,
                    NativeCall {
                        entry_offset: 0x2d698,
                        argument0: Some(STATE + 0xa8),
                    },
                )
                .unwrap();
            assert!(capped.bios.0.is_empty());
            assert_eq!(
                read_bytes::<{ super::super::ball::STATE_BYTES }>(&memory, STATE)
                    .unwrap()
                    .as_slice(),
                hex_bytes(case["state"].as_str().unwrap()),
                "adapter cap with controlled BIOS {index}"
            );
        }
    }

    /// Explicit zeroed synthetic RAM snapshots used by the original wrapper
    /// generators. Addresses outside these supplied snapshots/source patches
    /// fail rather than acquiring a zero value through a sparse-map fallback.
    #[derive(Clone, PartialEq, Eq)]
    struct RouterSnapshot {
        ewram: Vec<u8>,
        iwram: Vec<u8>,
        source: BTreeMap<u32, u8>,
    }
    impl RouterSnapshot {
        fn initialized_oracle_ram() -> Self {
            Self {
                ewram: vec![0; 0x40000],
                iwram: vec![0; 0x8000],
                source: BTreeMap::new(),
            }
        }
        fn bind(&mut self, address: u32, width: usize, value: u32) {
            for (index, byte) in value.to_le_bytes()[..width].iter().enumerate() {
                let address = address + index as u32;
                if let Some(byte_at) = self.ram_mut(address) {
                    *byte_at = *byte;
                } else {
                    assert!((0x08000000..0x08800000).contains(&address));
                    self.source.insert(address, *byte);
                }
            }
        }
        fn ram_mut(&mut self, address: u32) -> Option<&mut u8> {
            if (0x02000000..0x02040000).contains(&address) {
                self.ewram.get_mut((address - 0x02000000) as usize)
            } else if (0x03000000..0x03008000).contains(&address) {
                self.iwram.get_mut((address - 0x03000000) as usize)
            } else {
                None
            }
        }
        fn patches(&mut self, value: &Value) {
            for row in value.as_array().into_iter().flatten() {
                self.bind(
                    row[0].as_u64().unwrap() as u32,
                    row[1].as_u64().unwrap() as usize,
                    row[2].as_u64().unwrap() as u32,
                );
            }
        }
        fn initialized_case(vectors: &Value, case: &Value) -> Self {
            let mut result = Self::initialized_oracle_ram();
            result.patches(&vectors["common"]);
            result.patches(&case["init"]);
            for (address, value) in case["reads"].as_object().into_iter().flatten() {
                result.bind(address.parse().unwrap(), 4, value.as_u64().unwrap() as u32);
            }
            result
        }
        fn expected_after(&self, case: &Value, router_suffix: bool) -> Self {
            let mut expected = self.clone();
            for row in case["diff"].as_array().unwrap() {
                expected.bind(
                    row[0].as_u64().unwrap() as u32,
                    1,
                    row[1].as_u64().unwrap() as u32,
                );
            }
            if router_suffix {
                let pointer = expected.read_u32(frame::STATE_POINTER).unwrap();
                let layer = expected.read_u8(pointer + 6).unwrap() as i8 as i16 as u16;
                expected.bind(pointer + 0x6c, 2, u32::from(layer));
            }
            expected
        }
        fn assert_matches(&self, expected: &Self, label: &str, index: usize) {
            assert!(self.ewram == expected.ewram, "EWRAM {label} case {index}");
            assert!(self.iwram == expected.iwram, "IWRAM {label} case {index}");
            assert_eq!(self.source, expected.source, "source {label} case {index}");
        }
    }
    impl FrameMemory for RouterSnapshot {
        fn read_u8(&self, address: u32) -> Result<u8, FrameError> {
            let value = if (0x02000000..0x02040000).contains(&address) {
                self.ewram.get((address - 0x02000000) as usize)
            } else if (0x03000000..0x03008000).contains(&address) {
                self.iwram.get((address - 0x03000000) as usize)
            } else {
                self.source.get(&address)
            };
            value
                .copied()
                .ok_or(FrameError::MissingRead { address, width: 1 })
        }
        fn read_u16(&self, address: u32) -> Result<u16, FrameError> {
            Ok(u16::from_le_bytes([
                self.read_u8(address)?,
                self.read_u8(address + 1)?,
            ]))
        }
        fn read_u32(&self, address: u32) -> Result<u32, FrameError> {
            Ok(u32::from_le_bytes([
                self.read_u8(address)?,
                self.read_u8(address + 1)?,
                self.read_u8(address + 2)?,
                self.read_u8(address + 3)?,
            ]))
        }
        fn write_u8(&mut self, address: u32, value: u8) -> Result<(), FrameError> {
            *self
                .ram_mut(address)
                .ok_or(FrameError::MissingWrite { address, width: 1 })? = value;
            Ok(())
        }
        fn write_u16(&mut self, address: u32, value: u16) -> Result<(), FrameError> {
            self.write_u8(address, value as u8)?;
            self.write_u8(address + 1, (value >> 8) as u8)
        }
    }
    struct CapturedHelpers<'a> {
        calls: Vec<Value>,
        mutations: &'a Value,
    }
    impl FrameHelpers for CapturedHelpers<'_> {
        fn call(
            &mut self,
            memory: &mut dyn FrameMemory,
            offset: u32,
            args: &[u32],
        ) -> Result<(), FrameError> {
            // This is the same explicit test-only helper interception as the
            // native controller oracle, including its pointer-switch scripts.
            self.calls.push(serde_json::json!([offset, args]));
            for row in self.mutations[offset.to_string()]
                .as_array()
                .into_iter()
                .flatten()
            {
                let address = row[0].as_u64().unwrap() as u32;
                let width = row[1].as_u64().unwrap() as usize;
                let value = row[2].as_u64().unwrap() as u32;
                for (index, byte) in value.to_le_bytes()[..width].iter().enumerate() {
                    memory.write_u8(address + index as u32, *byte)?;
                }
            }
            Ok(())
        }
    }
    #[test]
    fn default_helpers_reject_and_preserve_all_consumed_arguments() {
        let error = RejectFrameHelpers
            .call(
                &mut Memory::default(),
                0x2614,
                &[0, 12, 13, 0x08600000, 7, 9],
            )
            .unwrap_err();
        let FrameError::Dependency { call, reason } = error else {
            panic!("explicit native helper boundary")
        };
        assert_eq!(
            call,
            NativeCall {
                entry_offset: 0x2614,
                argument0: Some(0)
            }
        );
        assert!(reason.contains("0x8600000") && reason.contains("0x7") && reason.contains("0x9"));
        let snapshot = RouterSnapshot::initialized_oracle_ram();
        assert!(
            snapshot.read_u16(0x086e93a8).is_err(),
            "unbound ROM resources cannot become fabricated zeroes"
        );
    }
    #[test]
    #[ignore = "Requires local SPP_ROM; source router parity with explicitly intercepted helper dependencies"]
    fn adapter_lifecycle_and_events_preserve_original_memory_calls_and_pointer_switches() {
        let rom = std::fs::read(std::env::var_os("SPP_ROM").expect("Set local SPP_ROM")).unwrap();
        let geometry = flippers::Geometry::load(&rom).unwrap();
        let trig = math::TrigTable::from_rom(&rom).unwrap();
        let response = collision::ResponseTables::from_rom(&rom).unwrap();
        let lookups = collision::FlipperLookups::from_rom(&rom).unwrap();
        let regions = collision::MapRegions::from_rom(&rom).unwrap();
        for (label, source, entry, suffix) in [
            (
                "lifecycle01",
                vectors!("lifecycle-vectors.json"),
                0x2e1a0,
                false,
            ),
            (
                "lifecycle_tables",
                vectors!("lifecycle_tables-vectors.json"),
                0x2e1a0,
                true,
            ),
            (
                "table_events",
                vectors!("table_events-vectors.json"),
                0x2ea04,
                false,
            ),
        ] {
            let vectors: Value = serde_json::from_str(source).unwrap();
            let mut checked = 0;
            for (index, case) in vectors["cases"].as_array().unwrap().iter().enumerate() {
                let mut memory = RouterSnapshot::initialized_case(&vectors, case);
                if label == "lifecycle01" && memory.read_u16(frame::TABLE_CODE).unwrap() > 1 {
                    continue;
                }
                let expected = memory.expected_after(case, suffix);
                let helpers = CapturedHelpers {
                    calls: vec![],
                    mutations: &case["mutations"],
                };
                let mut ports = FramePorts::new(
                    resources(&geometry, &trig, &response, &lookups, &regions, &[]),
                    math::RejectBiosMath,
                    None,
                )
                .with_helpers(helpers);
                ports
                    .call(
                        &mut memory,
                        NativeCall {
                            entry_offset: entry,
                            argument0: None,
                        },
                    )
                    .unwrap();
                assert_eq!(
                    ports.helpers.calls,
                    *case["calls"].as_array().unwrap(),
                    "helper sequence {label} case {index}"
                );
                memory.assert_matches(&expected, label, index);
                checked += 1;
            }
            assert!(checked >= 144, "native controller evidence is exercised");
        }
        let mut missing = Memory::default();
        missing.bind_u32(frame::STATE_POINTER, STATE);
        missing.bind_u16(frame::TABLE_CODE, 1);
        missing.bind_u16(STATE + 0x28, 10);
        missing.bind_u16(STATE + 0x6e, 0);
        let mut ports = FramePorts::new(
            resources(&geometry, &trig, &response, &lookups, &regions, &[]),
            math::RejectBiosMath,
            None,
        );
        assert!(
            matches!(
                ports.call(
                    &mut missing,
                    NativeCall {
                        entry_offset: 0x2e1a0,
                        argument0: None
                    }
                ),
                Err(FrameError::MissingWrite {
                    address: 0x0203516e,
                    ..
                } | FrameError::MissingRead {
                    address: 0x0203516e,
                    ..
                })
            ),
            "launch globals require actual supplied snapshot bindings"
        );
        let mut memory = RouterSnapshot::initialized_oracle_ram();
        memory.bind(frame::STATE_POINTER, 4, STATE);
        memory.bind(frame::TABLE_CODE, 2, 2);
        memory.bind(STATE + 0x28, 2, 3);
        memory.bind(0x03000516, 2, 1);
        memory.bind(0x030040c2, 2, 1);
        assert!(
            matches!(
                ports.call(
                    &mut memory,
                    NativeCall {
                        entry_offset: 0x2e1a0,
                        argument0: None
                    }
                ),
                Err(FrameError::Dependency {
                    call: NativeCall {
                        entry_offset: 0x3046c,
                        ..
                    },
                    ..
                })
            ),
            "table2 is source-dispatched and its UI helper remains strict"
        );
    }
    #[test]
    #[ignore = "Requires local SPP_ROM; original nudge curve and complete stateful decay sequences"]
    fn adapter_nudge_matches_704_native_calls_without_fabricated_globals() {
        let rom = std::fs::read(std::env::var_os("SPP_ROM").expect("Set local SPP_ROM")).unwrap();
        let geometry = flippers::Geometry::load(&rom).unwrap();
        let trig = math::TrigTable::from_rom(&rom).unwrap();
        let response = collision::ResponseTables::from_rom(&rom).unwrap();
        let lookups = collision::FlipperLookups::from_rom(&rom).unwrap();
        let regions = collision::MapRegions::from_rom(&rom).unwrap();
        let curve = nudge::Curve::load(&rom).unwrap();
        let mut ports = FramePorts::new(
            resources(&geometry, &trig, &response, &lookups, &regions, &[]),
            math::RejectBiosMath,
            None,
        )
        .with_nudge_curve(&curve);
        let vectors: Value = serde_json::from_str(vectors!("nudge-native-vectors.json")).unwrap();
        let mut checked = 0;
        let bind_input = |memory: &mut Memory, input: &Value| {
            for (address, field) in [
                (0x03000516, "pressed"),
                (0x030040c8, "left_mask"),
                (0x030040ca, "right_mask"),
                (0x030040cc, "up_mask"),
            ] {
                memory.bind_u16(address, input[field].as_u64().unwrap() as u16);
            }
        };
        let check = |memory: &Memory, case: &Value| {
            assert_eq!(
                read_bytes::<{ super::super::ball::STATE_BYTES }>(memory, STATE)
                    .unwrap()
                    .as_slice(),
                hex_bytes(case["state"].as_str().unwrap())
            );
            assert_eq!(
                read_bytes::<{ nudge::GLOBAL_BYTES }>(memory, nudge::GLOBAL_ADDRESS)
                    .unwrap()
                    .as_slice(),
                hex_bytes(case["globals"].as_str().unwrap())
            );
            assert!(
                read_bytes::<{ flippers::STATE_BYTES }>(memory, STATE).unwrap()
                    [super::super::ball::STATE_BYTES..]
                    .iter()
                    .all(|byte| *byte == 0x55)
            );
        };
        for case in vectors["cases"].as_array().unwrap() {
            let mut memory = Memory::default();
            memory.bind(STATE, &[0x55; flippers::STATE_BYTES]);
            memory.bind(STATE, &hex_bytes(case["input_state"].as_str().unwrap()));
            memory.bind(
                nudge::GLOBAL_ADDRESS,
                &hex_bytes(case["input_globals"].as_str().unwrap()),
            );
            memory.bind_u32(frame::STATE_POINTER, STATE);
            bind_input(&mut memory, &case["inputs"]);
            ports
                .call(
                    &mut memory,
                    NativeCall {
                        entry_offset: 0x39ee4,
                        argument0: None,
                    },
                )
                .unwrap();
            check(&memory, case);
            checked += 1;
        }
        for sequence in vectors["sequences"].as_array().unwrap() {
            let mut memory = Memory::default();
            memory.bind(STATE, &[0x55; flippers::STATE_BYTES]);
            memory.bind(STATE, &hex_bytes(sequence["input_state"].as_str().unwrap()));
            memory.bind(
                nudge::GLOBAL_ADDRESS,
                &hex_bytes(sequence["input_globals"].as_str().unwrap()),
            );
            memory.bind_u32(frame::STATE_POINTER, STATE);
            for case in sequence["frames"].as_array().unwrap() {
                bind_input(&mut memory, &case["inputs"]);
                ports
                    .call(
                        &mut memory,
                        NativeCall {
                            entry_offset: 0x39ee4,
                            argument0: None,
                        },
                    )
                    .unwrap();
                check(&memory, case);
                checked += 1;
            }
        }
        assert_eq!(checked, 704);
        let mut missing = Memory::default();
        missing.bind(STATE, &[0; flippers::STATE_BYTES]);
        missing.bind_u32(frame::STATE_POINTER, STATE);
        let before = missing.clone();
        assert!(matches!(
            ports.call(
                &mut missing,
                NativeCall {
                    entry_offset: 0x39ee4,
                    argument0: None
                }
            ),
            Err(FrameError::MissingRead {
                address: nudge::GLOBAL_ADDRESS,
                ..
            })
        ));
        assert_eq!(
            missing.0, before.0,
            "missing nudge state cannot be replaced with zeroes"
        );
    }
}
