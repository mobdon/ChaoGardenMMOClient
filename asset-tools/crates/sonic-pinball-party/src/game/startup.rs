//! Source translation of A3VE's initial callback wrapper at ROM 0x81E8.
//!
//! Underlying intro initialization/update routines are explicit dependencies.
//! An unported dependency returns an error; this wrapper never calls ROM code,
//! substitutes another screen, or declares that the native intro is playable.

use super::control::CallbackStack;
use serde::Serialize;
use std::fmt;

pub const INITIAL_CALLBACK: u32 = 0x0800_81e9;
pub const NEXT_CALLBACK: u32 = 0x0800_91c5;
pub const CONTEXT_POINTER_SLOT: u32 = 0x0200_1ca0;
pub const INITIAL_CONTEXT: u32 = 0x0201_99a0;
pub const WRAPPER_OFFSET: usize = 0x81e8;
pub const WRAPPER_LENGTH: usize = 0x78;
pub const WRAPPER_SHA256: &str = "634a6001b14c77aaa88ff8cc8e1bca4874d14b4ad53b07c673f156541caf6b54";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct StartupCall {
    /// Even ROM file offset of the original BL target, not a callable pointer.
    pub entry_offset: usize,
    pub argument0: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupError {
    EmptyCallbackStack,
    MissingWord(u32),
    UnportedNativeCall(StartupCall),
}

impl fmt::Display for StartupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for StartupError {}

/// Only explicit source-level word accesses; no ROM instruction execution.
pub trait StartupMemory {
    fn read_word(&self, address: u32) -> Result<u32, StartupError>;
    fn write_word(&mut self, address: u32, value: u32) -> Result<(), StartupError>;
}

/// Implementations must dispatch to completed Rust ports or return Unported.
/// Oracle tests intercept these dependencies to test wrapper ordering alone;
/// successful intercepted calls do not establish dependency implementation.
pub trait StartupCalls {
    fn invoke(
        &mut self,
        call: StartupCall,
        memory: &mut dyn StartupMemory,
    ) -> Result<(), StartupError>;
}

#[derive(Debug, Default)]
pub struct RejectUnportedStartupCalls;

impl StartupCalls for RejectUnportedStartupCalls {
    fn invoke(
        &mut self,
        call: StartupCall,
        _memory: &mut dyn StartupMemory,
    ) -> Result<(), StartupError> {
        Err(StartupError::UnportedNativeCall(call))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum StartupOutcome {
    Initialized,
    Resumed,
    Updated,
    ReplacedByNextCallback,
}

/// Exact wrapper state transitions, including stores before external calls.
/// Errors may therefore leave the pre-call mutations visible, matching the
/// source point where a strict diagnostic host stops at an unported call.
pub fn initial_callback(
    callbacks: &mut CallbackStack,
    auxiliary_callbacks: &mut CallbackStack,
    memory: &mut dyn StartupMemory,
    calls: &mut dyn StartupCalls,
) -> Result<StartupOutcome, StartupError> {
    let state = callbacks
        .top()
        .ok_or(StartupError::EmptyCallbackStack)?
        .signed_parameter();
    match state {
        0 | 1 => {
            callbacks
                .top_mut()
                .ok_or(StartupError::EmptyCallbackStack)?
                .parameter = 2;
            memory.write_word(CONTEXT_POINTER_SLOT, INITIAL_CONTEXT)?;
            calls.invoke(
                StartupCall {
                    entry_offset: if state == 0 { 0x6ef4 } else { 0x7840 },
                    argument0: INITIAL_CONTEXT,
                },
                memory,
            )?;
            Ok(if state == 0 {
                StartupOutcome::Initialized
            } else {
                StartupOutcome::Resumed
            })
        }
        _ => {
            let pointer = memory.read_word(CONTEXT_POINTER_SLOT)?;
            calls.invoke(
                StartupCall {
                    entry_offset: 0x8168,
                    argument0: pointer,
                },
                memory,
            )?;
            // Native code reloads the global pointer after update; a completed
            // dependency may change it, so do not reuse the pre-call pointer.
            let pointer = memory.read_word(CONTEXT_POINTER_SLOT)?;
            if memory.read_word(pointer)? != 0 {
                callbacks.replace(super::control::CallbackId(NEXT_CALLBACK), 0);
                auxiliary_callbacks.clear();
                Ok(StartupOutcome::ReplacedByNextCallback)
            } else {
                Ok(StartupOutcome::Updated)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::collections::BTreeMap;

    #[derive(Debug, Default)]
    struct Memory(BTreeMap<u32, u32>);
    impl StartupMemory for Memory {
        fn read_word(&self, address: u32) -> Result<u32, StartupError> {
            self.0
                .get(&address)
                .copied()
                .ok_or(StartupError::MissingWord(address))
        }
        fn write_word(&mut self, address: u32, value: u32) -> Result<(), StartupError> {
            self.0.insert(address, value);
            Ok(())
        }
    }

    struct InterceptedDependencies {
        effects: Vec<StartupCall>,
        return_pointer: u32,
        return_completion: u32,
    }
    impl StartupCalls for InterceptedDependencies {
        fn invoke(
            &mut self,
            call: StartupCall,
            memory: &mut dyn StartupMemory,
        ) -> Result<(), StartupError> {
            self.effects.push(call);
            if call.entry_offset == 0x8168 {
                memory.write_word(CONTEXT_POINTER_SLOT, self.return_pointer)?;
                memory.write_word(self.return_pointer, self.return_completion)?;
            }
            Ok(())
        }
    }

    fn words(value: &Value) -> Vec<u16> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u16)
            .collect()
    }
    fn integer(value: &Value) -> u32 {
        value.as_u64().unwrap() as u32
    }

    #[test]
    fn native_wrapper_vectors_with_explicitly_intercepted_dependencies() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/startup-vectors.json")).unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let mut callbacks =
                CallbackStack::from_halfwords(&words(&case["mainInitial"])).unwrap();
            let mut auxiliary = CallbackStack::from_halfwords(&words(&case["auxInitial"])).unwrap();
            let pointer = integer(&case["pointerInitial"]);
            let mut memory = Memory::default();
            memory.write_word(CONTEXT_POINTER_SLOT, pointer).unwrap();
            memory
                .write_word(pointer, integer(&case["completionInitial"]))
                .unwrap();
            let mut dependencies = InterceptedDependencies {
                effects: Vec::new(),
                return_pointer: integer(&case["returnPointer"]),
                return_completion: integer(&case["returnCompletion"]),
            };
            initial_callback(
                &mut callbacks,
                &mut auxiliary,
                &mut memory,
                &mut dependencies,
            )
            .unwrap();
            assert_eq!(
                callbacks.to_halfwords().as_slice(),
                words(&case["mainFinal"])
            );
            assert_eq!(
                auxiliary.to_halfwords().as_slice(),
                words(&case["auxFinal"])
            );
            assert_eq!(
                memory.read_word(CONTEXT_POINTER_SLOT).unwrap(),
                integer(&case["pointerFinal"])
            );
            assert_eq!(
                serde_json::to_value(dependencies.effects).unwrap(),
                case["calls"]
            );
        }
    }

    #[test]
    fn unported_dependency_stops_after_native_pre_call_stores() {
        let mut callbacks = CallbackStack::default();
        callbacks.push(super::super::control::CallbackId(INITIAL_CALLBACK), 0);
        let mut auxiliary = CallbackStack::default();
        let mut memory = Memory::default();
        let error = initial_callback(
            &mut callbacks,
            &mut auxiliary,
            &mut memory,
            &mut RejectUnportedStartupCalls,
        )
        .unwrap_err();
        assert_eq!(
            error,
            StartupError::UnportedNativeCall(StartupCall {
                entry_offset: 0x6ef4,
                argument0: INITIAL_CONTEXT
            })
        );
        assert_eq!(callbacks.top().unwrap().parameter, 2);
        assert_eq!(
            memory.read_word(CONTEXT_POINTER_SLOT).unwrap(),
            INITIAL_CONTEXT
        );
    }

    #[test]
    fn rejects_empty_stack_before_memory_or_dependency_calls() {
        assert_eq!(
            initial_callback(
                &mut CallbackStack::default(),
                &mut CallbackStack::default(),
                &mut Memory::default(),
                &mut RejectUnportedStartupCalls
            ),
            Err(StartupError::EmptyCallbackStack)
        );
    }
}
