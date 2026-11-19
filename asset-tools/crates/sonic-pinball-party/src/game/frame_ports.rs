//! Bounded native-address adapter for the four completed A3VE frame leaves.
//! Every global is read from the supplied snapshot. Missing calls, bindings,
//! BIOS services or sound execution stop the frame explicitly.
use super::{collision, flippers, frame, math};
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

pub struct FramePorts<'a, 'rom, B, A = RejectFrameAudio> {
    resources: PortResources<'a, 'rom>,
    pub bios: B,
    pub audio: A,
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
            collision_scratch_address,
        }
    }
}
impl<'a, 'rom, B: math::BiosMath, A: FrameAudio> FramePorts<'a, 'rom, B, A> {
    pub fn with_audio<T: FrameAudio>(self, audio: T) -> FramePorts<'a, 'rom, B, T> {
        FramePorts {
            resources: self.resources,
            bios: self.bios,
            audio,
            collision_scratch_address: self.collision_scratch_address,
        }
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

impl<B: math::BiosMath, A: FrameAudio> FrameCalls for FramePorts<'_, '_, B, A> {
    /// The frame controller ignores return registers from all four leaves.
    /// `Ok(0)` conveys successful completion, not a reconstructed ARM R0 value.
    fn call(&mut self, memory: &mut dyn FrameMemory, call: NativeCall) -> Result<u32, FrameError> {
        if !matches!(call.entry_offset, 0x2ac18 | 0x2a04c | 0x2d698 | 0x2ba18) {
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
            _ => unreachable!("checked native leaf"),
        }
        write_changed(memory, pointer, before.as_bytes(), state.as_bytes())?;
        Ok(0)
    }
}

impl<B: math::BiosMath, A: FrameAudio> FramePorts<'_, '_, B, A> {
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
        let vectors: Value =
            serde_json::from_str(include_str!("../../tests/collision-vectors.json")).unwrap();
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
        let vectors: Value =
            serde_json::from_str(include_str!("../../tests/ball-vectors.json")).unwrap();
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
            entry_offset: 0x2e1a0,
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
            serde_json::from_str(include_str!("../../tests/flipper-native-vectors.json")).unwrap();
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
}
