//! Original A3VE frame controller, including the mode-10 multi-ball branch.
//! Dependencies are source ports supplied by the host; absent routines stop the
//! controller. The wrapper never supplies launch, scoring, camera or draw logic.
use super::{ball::MainBall, flippers::Flippers};

pub const OFFSET: usize = 0xa438;
pub const LENGTH: usize = 0x230;
pub const SHA256: &str = "22e671310c7d77a36a67a0ad3190e54db07edbc61037b428062adddde38b3ce1";
pub const STATE_POINTER: u32 = 0x030040b0;
pub const TABLE_CODE: u32 = 0x03003068;
pub const PAUSE_A: u32 = 0x020319ac;
pub const PAUSE_B: u32 = 0x020319a4;
pub const INITIAL_GATE: u32 = 0x0300296c;
pub const BALL_Y: u32 = 0x02031ae4;
pub const MULTIBALL: u32 = 0x020328c0;

/// A single authoritative prefix shared by the existing ball/flipper ports.
/// Accessors copy the same bytes; they do not establish independent simulation
/// state. Applying a pair merges the ball prefix and only the flipper suffix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalState {
    bytes: [u8; super::flippers::STATE_BYTES],
}
impl CanonicalState {
    pub fn from_bytes(bytes: [u8; super::flippers::STATE_BYTES]) -> Self {
        Self { bytes }
    }
    pub fn as_bytes(&self) -> &[u8; super::flippers::STATE_BYTES] {
        &self.bytes
    }
    pub fn as_bytes_mut(&mut self) -> &mut [u8; super::flippers::STATE_BYTES] {
        &mut self.bytes
    }
    pub fn ball(&self) -> MainBall {
        MainBall::from_bytes(
            self.bytes[..super::ball::STATE_BYTES]
                .try_into()
                .expect("fixed prefix"),
        )
    }
    pub fn flippers(&self) -> Flippers {
        Flippers::from_bytes(self.bytes)
    }
    pub fn apply_ball(&mut self, ball: &MainBall) {
        self.bytes[..super::ball::STATE_BYTES].copy_from_slice(ball.as_bytes());
    }
    pub fn apply_flippers(&mut self, flippers: &Flippers) {
        self.bytes.copy_from_slice(flippers.as_bytes());
    }
    pub fn apply_collision_pair(&mut self, ball: &MainBall, flippers: &Flippers) {
        self.apply_ball(ball);
        self.bytes[super::ball::STATE_BYTES..]
            .copy_from_slice(&flippers.as_bytes()[super::ball::STATE_BYTES..]);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeCall {
    pub entry_offset: u32,
    /// Only an argument explicitly prepared by this controller is represented.
    /// `None` does not assert a value for the incoming caller-saved register.
    pub argument0: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    MissingRead {
        address: u32,
        width: u8,
    },
    MissingWrite {
        address: u32,
        width: u8,
    },
    UnportedNativeCall(NativeCall),
    Dependency {
        call: NativeCall,
        reason: String,
    },
    /// The native signed loop counter cannot terminate for counts above 0x7fff.
    MultiballCount(u16),
}
impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for FrameError {}

/// Exact bounded reads/writes required by the original wrapper. Implementors
/// must fail unbound addresses rather than returning fabricated zeroes.
pub trait FrameMemory {
    fn read_u8(&self, address: u32) -> Result<u8, FrameError>;
    fn read_u16(&self, address: u32) -> Result<u16, FrameError>;
    fn read_u32(&self, address: u32) -> Result<u32, FrameError>;
    fn write_u8(&mut self, address: u32, value: u8) -> Result<(), FrameError>;
    fn write_u16(&mut self, address: u32, value: u16) -> Result<(), FrameError>;
}

pub trait FrameCalls {
    /// Mutations become visible to the next original load. The returned word is
    /// read only for routine 0x43d5c, whose low halfword controls the loop break.
    fn call(&mut self, memory: &mut dyn FrameMemory, call: NativeCall) -> Result<u32, FrameError>;
}
pub struct RejectUnportedFrameCalls;
impl FrameCalls for RejectUnportedFrameCalls {
    fn call(&mut self, _: &mut dyn FrameMemory, call: NativeCall) -> Result<u32, FrameError> {
        Err(FrameError::UnportedNativeCall(call))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrameOutcome {
    pub integration_substeps: u32,
    pub collision_calls: u32,
    pub selected_balls: u16,
    pub multiball_break: bool,
}

fn invoke(
    memory: &mut dyn FrameMemory,
    calls: &mut dyn FrameCalls,
    offset: u32,
) -> Result<(), FrameError> {
    calls.call(
        memory,
        NativeCall {
            entry_offset: offset,
            argument0: None,
        },
    )?;
    Ok(())
}
fn argument(
    memory: &mut dyn FrameMemory,
    calls: &mut dyn FrameCalls,
    offset: u32,
    argument0: u32,
) -> Result<u32, FrameError> {
    calls.call(
        memory,
        NativeCall {
            entry_offset: offset,
            argument0: Some(argument0),
        },
    )
}
fn paused(memory: &dyn FrameMemory) -> Result<bool, FrameError> {
    // Native short-circuit matters: the second word need not be bound when A=1.
    Ok(memory.read_u32(PAUSE_A)? == 1 || memory.read_u32(PAUSE_B)? == 1)
}
fn state_u16(memory: &dyn FrameMemory, offset: u32) -> Result<u16, FrameError> {
    memory.read_u16(memory.read_u32(STATE_POINTER)?.wrapping_add(offset))
}
fn substeps(
    memory: &mut dyn FrameMemory,
    calls: &mut dyn FrameCalls,
    outcome: &mut FrameOutcome,
) -> Result<(), FrameError> {
    for index in 0..4 {
        let address = memory.read_u32(STATE_POINTER)?.wrapping_add(2);
        memory.write_u8(address, index)?;
        invoke(memory, calls, 0x2a04c)?;
        outcome.integration_substeps += 1;
        let address = memory.read_u32(STATE_POINTER)?.wrapping_add(3);
        if memory.read_u8(address)? == 0 {
            invoke(memory, calls, 0x2ba18)?;
            outcome.collision_calls += 1;
        }
    }
    Ok(())
}
fn table_and_tail(
    memory: &mut dyn FrameMemory,
    calls: &mut dyn FrameCalls,
) -> Result<(), FrameError> {
    match memory.read_u16(TABLE_CODE)? {
        0 | 1 => invoke(memory, calls, 0x1fc30)?,
        2 => invoke(memory, calls, 0x20528)?,
        3 => invoke(memory, calls, 0x20ca8)?,
        _ => {}
    }
    let value = memory.read_u32(PAUSE_A)?;
    argument(memory, calls, 0x461f8, value)?;
    invoke(memory, calls, 0x274b8)
}

/// Port of the complete original control flow at 0xa438..0xa664. Errors retain
/// prior native-order effects so a missing dependency can be diagnosed exactly;
/// the host must not present an incomplete frame as a completed game tick.
pub fn run(
    memory: &mut dyn FrameMemory,
    calls: &mut dyn FrameCalls,
) -> Result<FrameOutcome, FrameError> {
    let mut outcome = FrameOutcome::default();
    if memory.read_u32(INITIAL_GATE)? == 1 {
        invoke(memory, calls, 0x3c170)?;
    }
    if state_u16(memory, 0x6e)? != 10 {
        if !paused(memory)? {
            for offset in [0x2ac18, 0x2e1a0, 0x2ea04] {
                invoke(memory, calls, offset)?;
            }
            let velocity = memory.read_u32(STATE_POINTER)?.wrapping_add(0xa8);
            argument(memory, calls, 0x2d698, velocity)?;
            invoke(memory, calls, 0x39ee4)?;
            substeps(memory, calls, &mut outcome)?;
            invoke(memory, calls, 0x29c7c)?;
            if state_u16(memory, 0x28)? != 0 {
                invoke(memory, calls, 0x27180)?;
            }
            memory.write_u16(BALL_Y, state_u16(memory, 0x9a)?)?;
        }
        for offset in [0x1fa30, 0x20fb0, 0x264a0] {
            invoke(memory, calls, offset)?;
        }
        if !paused(memory)? {
            for offset in [0x50a08, 0x31190, 0x2fd68] {
                invoke(memory, calls, offset)?;
            }
        }
        table_and_tail(memory, calls)?;
        if memory.read_u32(PAUSE_A)? != 1 {
            invoke(memory, calls, 0x43d30)?;
        }
    } else {
        if !paused(memory)? {
            for offset in [0x2ac18, 0x2e1a0, 0x39ee4] {
                invoke(memory, calls, offset)?;
            }
            let mut count = memory.read_u16(MULTIBALL + 0xa8)?;
            if count > 0x7fff {
                return Err(FrameError::MultiballCount(count));
            }
            let mut index: u16 = 0;
            // Count is reloaded after each iteration in the original routine.
            while index < count {
                argument(memory, calls, 0x3e0b0, u32::from(index))?;
                outcome.selected_balls += 1;
                let active = memory.read_u16(MULTIBALL + u32::from(index) * 56 + 0x30)?;
                if active != 0 {
                    invoke(memory, calls, 0x2ea04)?;
                    let velocity = memory.read_u32(STATE_POINTER)?.wrapping_add(0xa8);
                    argument(memory, calls, 0x2d698, velocity)?;
                    substeps(memory, calls, &mut outcome)?;
                    invoke(memory, calls, 0x50a08)?;
                    invoke(memory, calls, 0x31190)?;
                    argument(memory, calls, 0x3e10c, u32::from(index))?;
                    if argument(memory, calls, 0x43d5c, u32::from(index))? as u16 != 0 {
                        outcome.multiball_break = true;
                        break;
                    }
                }
                index += 1;
                count = memory.read_u16(MULTIBALL + 0xa8)?;
                if count > 0x7fff {
                    return Err(FrameError::MultiballCount(count));
                }
            }
            invoke(memory, calls, 0x3e44c)?;
            invoke(memory, calls, 0x3e158)?;
            memory.write_u16(BALL_Y, state_u16(memory, 0x9a)?)?;
        }
        for offset in [0x1fa30, 0x20fb0, 0x264a0, 0x2fd68] {
            invoke(memory, calls, offset)?;
        }
        table_and_tail(memory, calls)?;
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::collections::BTreeMap;

    fn bytes(hex: &str) -> Vec<u8> {
        assert_eq!(hex.len() % 2, 0);
        (0..hex.len() / 2)
            .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
            .collect()
    }
    #[derive(Default)]
    struct Memory {
        cells: BTreeMap<u32, u8>,
    }
    impl Memory {
        fn from_spans(spans: &Value) -> Self {
            let mut result = Self::default();
            for span in spans.as_array().unwrap() {
                let address = span["address"].as_u64().unwrap() as u32;
                for (index, value) in bytes(span["bytes"].as_str().unwrap())
                    .into_iter()
                    .enumerate()
                {
                    assert!(
                        result.cells.insert(address + index as u32, value).is_none(),
                        "overlapping synthetic spans"
                    );
                }
            }
            result
        }
        fn read<const N: usize>(&self, address: u32) -> Result<[u8; N], FrameError> {
            let mut result = [0; N];
            for (index, value) in result.iter_mut().enumerate() {
                *value = *self.cells.get(&address.wrapping_add(index as u32)).ok_or(
                    FrameError::MissingRead {
                        address,
                        width: N as u8,
                    },
                )?;
            }
            Ok(result)
        }
        fn write(&mut self, address: u32, value: &[u8]) -> Result<(), FrameError> {
            for index in 0..value.len() {
                if !self.cells.contains_key(&address.wrapping_add(index as u32)) {
                    return Err(FrameError::MissingWrite {
                        address,
                        width: value.len() as u8,
                    });
                }
            }
            for (index, value) in value.iter().enumerate() {
                self.cells
                    .insert(address.wrapping_add(index as u32), *value);
            }
            Ok(())
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
            self.write(address, &[value])
        }
        fn write_u16(&mut self, address: u32, value: u16) -> Result<(), FrameError> {
            self.write(address, &value.to_le_bytes())
        }
    }
    struct ScriptedCalls<'a> {
        actions: &'a [Value],
        occurrences: BTreeMap<u32, usize>,
        calls: Vec<Value>,
        geometry: Option<&'a super::super::flippers::Geometry>,
    }
    impl FrameCalls for ScriptedCalls<'_> {
        fn call(
            &mut self,
            memory: &mut dyn FrameMemory,
            call: NativeCall,
        ) -> Result<u32, FrameError> {
            let occurrence = self.occurrences.entry(call.entry_offset).or_default();
            *occurrence += 1;
            let pointer = memory.read_u32(STATE_POINTER)?;
            self.calls.push(json!({"offset": call.entry_offset, "argument0": call.argument0,
                "pointer": pointer, "byte2": memory.read_u8(pointer + 2)?, "byte3": memory.read_u8(pointer + 3)?}));
            if let Some(geometry) = self.geometry {
                if matches!(call.entry_offset, 0x2ac18 | 0x2a04c) {
                    let mut data = [0; super::super::flippers::STATE_BYTES];
                    for (index, value) in data.iter_mut().enumerate() {
                        *value = memory.read_u8(pointer + index as u32)?;
                    }
                    let mut state = CanonicalState::from_bytes(data);
                    if call.entry_offset == 0x2ac18 {
                        let mut flippers = state.flippers();
                        let sounds = flippers
                            .update(
                                super::super::flippers::Inputs {
                                    table_code: memory.read_u16(TABLE_CODE)?,
                                    left_mask: memory.read_u16(0x030040c4)?,
                                    right_mask: memory.read_u16(0x030040c6)?,
                                    held: memory.read_u16(0x03000514)?,
                                    pressed: memory.read_u16(0x03000516)?,
                                    audio_enabled: memory.read_u16(pointer + 0x6e2)?,
                                },
                                geometry,
                            )
                            .map_err(|error| FrameError::Dependency {
                                call,
                                reason: error.to_string(),
                            })?;
                        assert!(sounds.is_empty(), "test inputs do not request sound");
                        state.apply_flippers(&flippers);
                    } else {
                        let mut ball = state.ball();
                        ball.integrate(memory.read_u16(TABLE_CODE)?, memory.read_u32(0x020328bc)?);
                        state.apply_ball(&ball);
                    }
                    for (index, value) in state.as_bytes().iter().enumerate() {
                        memory.write_u8(pointer + index as u32, *value)?;
                    }
                    return Ok(0);
                }
            }
            let mut result = 0;
            for action in self.actions {
                if action["offset"].as_u64().unwrap() as u32 == call.entry_offset
                    && action["occurrence"].as_u64().unwrap() as usize == *occurrence
                {
                    if let Some(writes) = action["writes"].as_array() {
                        for write in writes {
                            let address = write[0].as_u64().unwrap() as u32;
                            let width = write[1].as_u64().unwrap() as usize;
                            let data = (write[2].as_u64().unwrap() as u32).to_le_bytes();
                            for (index, value) in data[..width].iter().enumerate() {
                                memory.write_u8(address + index as u32, *value)?;
                            }
                        }
                    }
                    if let Some(value) = action["return"].as_u64() {
                        result = value as u32;
                    }
                }
            }
            Ok(result)
        }
    }
    fn check_vectors(vectors: &Value, geometry: Option<&super::super::flippers::Geometry>) {
        for (index, case) in vectors["cases"].as_array().unwrap().iter().enumerate() {
            let mut memory = Memory::from_spans(&case["input"]);
            let mut calls = ScriptedCalls {
                actions: case["actions"].as_array().unwrap(),
                occurrences: BTreeMap::new(),
                calls: vec![],
                geometry,
            };
            let outcome = run(&mut memory, &mut calls).unwrap();
            assert_eq!(
                calls.calls,
                *case["calls"].as_array().unwrap(),
                "call order/gates case {index}"
            );
            for span in case["spans"].as_array().unwrap() {
                let address = span["address"].as_u64().unwrap() as u32;
                let expected = bytes(span["bytes"].as_str().unwrap());
                let actual = (0..expected.len())
                    .map(|i| memory.read_u8(address + i as u32).unwrap())
                    .collect::<Vec<_>>();
                assert_eq!(actual, expected, "memory case {index} at {address:08x}");
            }
            let count = |offset| {
                calls
                    .calls
                    .iter()
                    .filter(|call| call["offset"].as_u64().unwrap() == offset)
                    .count()
            };
            assert_eq!(outcome.integration_substeps as usize, count(0x2a04c));
            assert_eq!(outcome.collision_calls as usize, count(0x2ba18));
            assert_eq!(outcome.selected_balls as usize, count(0x3e0b0));
        }
    }
    #[test]
    fn complete_wrapper_matches_128_original_instruction_cases_with_intercepted_dependencies() {
        let vectors: Value =
            serde_json::from_str(include_str!("../../tests/frame-vectors.json")).unwrap();
        assert_eq!(vectors["cases"].as_array().unwrap().len(), 128);
        check_vectors(&vectors, None);
    }
    #[test]
    #[ignore = "Requires local SPP_ROM; original frame + flipper/integrator leaves and native geometry"]
    fn frame_with_real_ball_and_flipper_ports_matches_48_original_instruction_cases() {
        let rom = std::fs::read(std::env::var_os("SPP_ROM").expect("Set local SPP_ROM")).unwrap();
        let geometry = super::super::flippers::Geometry::load(&rom).unwrap();
        assert_eq!(crate::sha256(&rom[OFFSET..OFFSET + LENGTH]), SHA256);
        let vectors: Value =
            serde_json::from_str(include_str!("../../tests/frame-native-leaf-vectors.json"))
                .unwrap();
        assert_eq!(vectors["cases"].as_array().unwrap().len(), 48);
        check_vectors(&vectors, Some(&geometry));
    }
    #[test]
    fn canonical_pair_has_one_shared_ball_prefix() {
        let mut state = CanonicalState::from_bytes([0x11; super::super::flippers::STATE_BYTES]);
        let mut ball = state.ball();
        ball.as_bytes_mut().fill(0x22);
        let mut flippers = state.flippers();
        flippers.as_bytes_mut().fill(0x33);
        state.apply_collision_pair(&ball, &flippers);
        assert!(state.as_bytes()[..super::super::ball::STATE_BYTES]
            .iter()
            .all(|byte| *byte == 0x22));
        assert!(state.as_bytes()[super::super::ball::STATE_BYTES..]
            .iter()
            .all(|byte| *byte == 0x33));
        assert_eq!(state.ball(), ball);
    }
    #[test]
    fn missing_dependencies_and_unbounded_multiball_count_fail_explicitly() {
        let vectors: Value =
            serde_json::from_str(include_str!("../../tests/frame-vectors.json")).unwrap();
        let case = &vectors["cases"][0];
        let mut memory = Memory::from_spans(&case["input"]);
        assert_eq!(
            run(&mut Memory::default(), &mut RejectUnportedFrameCalls),
            Err(FrameError::MissingRead {
                address: INITIAL_GATE,
                width: 4
            })
        );
        assert_eq!(
            run(&mut memory, &mut RejectUnportedFrameCalls),
            Err(FrameError::UnportedNativeCall(NativeCall {
                entry_offset: 0x2ac18,
                argument0: None
            }))
        );
        memory.write_u16(0x0200006e, 10).unwrap();
        memory.write_u16(MULTIBALL + 0xa8, 0xffff).unwrap();
        let mut calls = ScriptedCalls {
            actions: &[],
            occurrences: BTreeMap::new(),
            calls: vec![],
            geometry: None,
        };
        assert_eq!(
            run(&mut memory, &mut calls),
            Err(FrameError::MultiballCount(0xffff))
        );
        assert_eq!(calls.calls.len(), 3);
    }
}
