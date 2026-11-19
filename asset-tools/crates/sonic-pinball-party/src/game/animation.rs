//! A3VE sprite animation dispatcher and verified native command handlers.
//!
//! Commands begin with original Thumb handler IDs, not inferred byte opcodes.
//! Only handlers ported below execute. Unknown IDs stop explicitly; repeated
//! immediate commands consume a caller-supplied work budget. Native object
//! bytes, including inactive loop slots and unnamed fields, are preserved.

use super::oam_render::DescriptorSource;
use std::fmt;

pub const STATE_BYTES: usize = 0x58;
pub const INITIALIZE_OFFSET: usize = 0x5b9c;
pub const DISPATCH_OFFSET: usize = 0x5bd0;
pub const END_HANDLER: u32 = 0x0800_5bf5;
pub const FRAME_HANDLER: u32 = 0x0800_5c01;
pub const BLANK_HANDLER: u32 = 0x0800_5c31;
pub const IMMEDIATE_FRAME_HANDLER: u32 = 0x0800_5c61;
pub const WAIT_HANDLER: u32 = 0x0800_5c75;
pub const LOOP_BEGIN_HANDLER: u32 = 0x0800_5c9d;
pub const LOOP_END_HANDLER: u32 = 0x0800_5cc9;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnimationError {
    StateLength(usize),
    MisalignedCommand(u32),
    AddressOverflow(u32),
    MissingHalfword(u32),
    UnknownHandler(u32),
    InvalidLoopDepth(u8),
    WorkBudgetExhausted { limit: usize },
}
impl fmt::Display for AnimationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for AnimationError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationState {
    bytes: [u8; STATE_BYTES],
}
impl Default for AnimationState {
    fn default() -> Self {
        Self {
            bytes: [0; STATE_BYTES],
        }
    }
}
impl AnimationState {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, AnimationError> {
        let bytes = bytes
            .try_into()
            .map_err(|_| AnimationError::StateLength(bytes.len()))?;
        Ok(Self { bytes })
    }
    pub fn as_bytes(&self) -> &[u8; STATE_BYTES] {
        &self.bytes
    }
    fn word(&self, offset: usize) -> u32 {
        u32::from_le_bytes(self.bytes[offset..offset + 4].try_into().unwrap())
    }
    fn halfword(&self, offset: usize) -> u16 {
        u16::from_le_bytes(self.bytes[offset..offset + 2].try_into().unwrap())
    }
    fn put_word(&mut self, offset: usize, value: u32) {
        self.bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn put_halfword(&mut self, offset: usize, value: u16) {
        self.bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    pub fn script_address(&self) -> u32 {
        self.word(0x30)
    }
    pub fn command_address(&self) -> u32 {
        self.word(0x34)
    }
    pub fn descriptor_address(&self) -> u32 {
        self.word(0x28)
    }
    pub fn timer(&self) -> i16 {
        self.halfword(0x18) as i16
    }
    pub fn loop_depth(&self) -> u8 {
        self.bytes[0x13]
    }

    /// Original 0x5b9c initializer. In particular +0x26 and the inactive loop
    /// stack at +0x38 are not cleared by the source routine.
    pub fn initialize(&mut self) {
        for offset in [0x0c, 0x28, 0x30, 0x34] {
            self.put_word(offset, 0);
        }
        self.put_word(0x2c, 0x8000_0000);
        for offset in [0x14, 0x18, 0x1a, 0x1c, 0x1e, 0x20, 0x22, 0x24] {
            self.put_halfword(offset, 0);
        }
        self.put_halfword(0x16, 0x100);
        self.bytes[0x10] = 0;
        self.bytes[0x11] = 0;
        self.bytes[0x12] = 1;
        self.bytes[0x13] = 0;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DispatchOutcome {
    pub commands_executed: usize,
}

fn at(address: u32, offset: u32) -> Result<u32, AnimationError> {
    address
        .checked_add(offset)
        .ok_or(AnimationError::AddressOverflow(address))
}
fn halfword(
    source: &dyn DescriptorSource,
    address: u32,
    offset: u32,
) -> Result<u16, AnimationError> {
    let address = at(address, offset)?;
    source
        .halfword(address)
        .ok_or(AnimationError::MissingHalfword(address))
}
fn word(source: &dyn DescriptorSource, address: u32, offset: u32) -> Result<u32, AnimationError> {
    Ok(u32::from(halfword(source, address, offset)?)
        | (u32::from(halfword(
            source,
            address,
            offset
                .checked_add(2)
                .ok_or(AnimationError::AddressOverflow(address))?,
        )?) << 16))
}
fn timed_advance(
    state: &mut AnimationState,
    duration: i16,
    amount: u32,
) -> Result<(), AnimationError> {
    if duration != 0 {
        let counter = state.halfword(0x18).wrapping_add(1);
        state.put_halfword(0x18, counter);
        if counter as i16 >= duration {
            state.put_halfword(0x18, 0);
            state.put_word(0x34, at(state.command_address(), amount)?);
        }
    }
    Ok(())
}

fn execute(
    state: &mut AnimationState,
    handler: u32,
    source: &dyn DescriptorSource,
) -> Result<bool, AnimationError> {
    let command = state.command_address();
    match handler {
        END_HANDLER => {
            state.put_word(0x30, 0);
            state.put_word(0x34, 0);
            Ok(false)
        }
        FRAME_HANDLER => {
            state.put_word(0x28, word(source, command, 4)?);
            state.put_halfword(0x20, halfword(source, command, 10)?);
            timed_advance(state, halfword(source, command, 8)? as i16, 12)?;
            Ok(false)
        }
        BLANK_HANDLER => {
            state.put_word(0x28, 0);
            timed_advance(state, halfword(source, command, 4)? as i16, 8)?;
            Ok(false)
        }
        IMMEDIATE_FRAME_HANDLER => {
            state.put_word(0x28, word(source, command, 4)?);
            state.put_halfword(0x20, word(source, command, 8)? as u16);
            state.put_word(0x34, at(command, 12)?);
            Ok(true)
        }
        WAIT_HANDLER => {
            timed_advance(state, halfword(source, command, 4)? as i16, 8)?;
            Ok(false)
        }
        LOOP_BEGIN_HANDLER => {
            let depth = state.loop_depth();
            if depth < 4 {
                state.bytes[0x13] = depth + 1;
                let offset = 0x38 + usize::from(depth) * 8;
                state.put_word(
                    offset + 4,
                    halfword(source, command, 4)? as i16 as i32 as u32,
                );
                let next = at(command, 8)?;
                state.put_word(0x34, next);
                state.put_word(offset, next);
            }
            // At depth >=4 native returns true without advancing. The
            // dispatcher budget safely captures that original infinite loop.
            Ok(true)
        }
        LOOP_END_HANDLER => {
            let depth = state.loop_depth();
            if depth > 4 {
                return Err(AnimationError::InvalidLoopDepth(depth));
            }
            if depth != 0 {
                let offset = 0x38 + usize::from(depth - 1) * 8;
                let counter = state.word(offset + 4);
                let next = if counter == 1 {
                    state.put_word(offset + 4, 0);
                    state.bytes[0x13] = depth - 1;
                    at(command, 4)?
                } else {
                    if counter != 0 {
                        state.put_word(offset + 4, counter.wrapping_sub(1));
                    }
                    state.word(offset)
                };
                state.put_word(0x34, next);
            }
            Ok(true)
        }
        _ => Err(AnimationError::UnknownHandler(handler)),
    }
}

/// Execute one actual handler. The bool is the original continue-immediately
/// return value. A malformed command does not partially commit this handler;
/// successful handlers preceding a later dispatcher error remain committed.
pub fn step_handler(
    state: &mut AnimationState,
    handler: u32,
    source: &dyn DescriptorSource,
) -> Result<bool, AnimationError> {
    let mut next = state.clone();
    let result = execute(&mut next, handler, source)?;
    *state = next;
    Ok(result)
}

/// Original 0x5bd0 dispatch gate and repeated-handler loop. The budget is an
/// explicit bounded-host adaptation; unknown handlers are never substituted.
pub fn dispatch(
    state: &mut AnimationState,
    source: &dyn DescriptorSource,
    max_commands: usize,
) -> Result<DispatchOutcome, AnimationError> {
    if state.script_address() == 0 || (state.word(0x0c) as i32) < 0 {
        return Ok(DispatchOutcome {
            commands_executed: 0,
        });
    }
    let mut executed = 0;
    loop {
        if executed == max_commands {
            return Err(AnimationError::WorkBudgetExhausted {
                limit: max_commands,
            });
        }
        let command = state.command_address();
        if command & 3 != 0 {
            return Err(AnimationError::MisalignedCommand(command));
        }
        let handler = word(source, command, 0)?;
        let again = step_handler(state, handler, source)?;
        executed += 1;
        if !again {
            return Ok(DispatchOutcome {
                commands_executed: executed,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::oam_render::DescriptorBytes;
    use super::*;
    use serde_json::Value;

    fn integer(value: &Value) -> u32 {
        value.as_u64().unwrap() as u32
    }
    fn bytes(words: &Value) -> Vec<u8> {
        words
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|word| (integer(word) as u16).to_le_bytes())
            .collect()
    }
    fn state(words: &Value) -> AnimationState {
        AnimationState::from_bytes(&bytes(words)).unwrap()
    }

    #[test]
    fn native_initializer_handlers_and_dispatcher_match_complete_synthetic_objects() {
        let data: Value =
            serde_json::from_str(include_str!("../../tests/animation-vectors.json")).unwrap();
        for case in data["initializers"].as_array().unwrap() {
            let mut object = state(&case["initial"]);
            object.initialize();
            assert_eq!(object.as_bytes().as_slice(), bytes(&case["final"]));
        }
        for case in data["handlers"].as_array().unwrap() {
            let mut object = state(&case["initial"]);
            let commands = bytes(&case["commands"]);
            let source = DescriptorBytes {
                base_address: integer(&data["commandBase"]),
                bytes: &commands,
            };
            assert_eq!(
                step_handler(&mut object, integer(&case["handler"]), &source).unwrap(),
                case["continue"].as_bool().unwrap()
            );
            assert_eq!(
                object.as_bytes().as_slice(),
                bytes(&case["final"]),
                "{}",
                case["name"]
            );
        }
        for case in data["dispatchers"].as_array().unwrap() {
            let mut object = state(&case["initial"]);
            let commands = bytes(&case["commands"]);
            let source = DescriptorBytes {
                base_address: integer(&data["commandBase"]),
                bytes: &commands,
            };
            for (index, snapshot) in case["snapshots"].as_array().unwrap().iter().enumerate() {
                let outcome = dispatch(&mut object, &source, 64).unwrap();
                assert_eq!(
                    outcome.commands_executed as u32,
                    integer(&case["commandCounts"][index])
                );
                assert_eq!(
                    object.as_bytes().as_slice(),
                    bytes(snapshot),
                    "{}",
                    case["name"]
                );
            }
        }
    }

    #[test]
    fn strict_handler_and_memory_boundaries_preserve_state_and_bound_native_loops() {
        let mut object = AnimationState::default();
        object.put_word(0x30, 0x1000);
        object.put_word(0x34, 0x1000);
        let unknown = DescriptorBytes {
            base_address: 0x1000,
            bytes: &0x0800_5d09u32.to_le_bytes(),
        };
        let before = object.clone();
        assert_eq!(
            dispatch(&mut object, &unknown, 3),
            Err(AnimationError::UnknownHandler(0x0800_5d09))
        );
        assert_eq!(object, before);
        let empty = DescriptorBytes {
            base_address: 0x1000,
            bytes: &[],
        };
        assert_eq!(
            dispatch(&mut object, &empty, 3),
            Err(AnimationError::MissingHalfword(0x1000))
        );
        assert_eq!(
            step_handler(&mut object, FRAME_HANDLER, &empty),
            Err(AnimationError::MissingHalfword(0x1004))
        );
        assert_eq!(object, before);
        let begin = DescriptorBytes {
            base_address: 0x1000,
            bytes: &LOOP_BEGIN_HANDLER.to_le_bytes(),
        };
        object.bytes[0x13] = 4;
        assert_eq!(
            dispatch(&mut object, &begin, 7),
            Err(AnimationError::WorkBudgetExhausted { limit: 7 })
        );
        assert_eq!(object.command_address(), 0x1000);
        let end = DescriptorBytes {
            base_address: 0x1000,
            bytes: &LOOP_END_HANDLER.to_le_bytes(),
        };
        object.bytes[0x13] = 0;
        assert_eq!(
            dispatch(&mut object, &end, 9),
            Err(AnimationError::WorkBudgetExhausted { limit: 9 })
        );
        object.bytes[0x13] = 5;
        assert_eq!(
            dispatch(&mut object, &end, 1),
            Err(AnimationError::InvalidLoopDepth(5))
        );
        object.put_word(0x34, 0x1001);
        assert_eq!(
            dispatch(&mut object, &empty, 1),
            Err(AnimationError::MisalignedCommand(0x1001))
        );
        object.put_word(0x30, 0);
        assert_eq!(
            dispatch(&mut object, &empty, 0).unwrap().commands_executed,
            0
        );
        assert_eq!(
            AnimationState::from_bytes(&[]),
            Err(AnimationError::StateLength(0))
        );
    }
}
