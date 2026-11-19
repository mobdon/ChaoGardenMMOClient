//! Source-level A3VE engine port in progress; never a fallback emulator.
pub mod affine;
pub mod animation;
pub mod ball;
pub mod board;
pub mod collision;
pub mod control;
pub mod control_ports;
pub mod display;
pub mod flippers;
pub mod frame;
pub mod frame_ports;
pub mod garden_audio;
pub mod garden_audio_init;
pub mod garden_audit;
pub mod garden_care;
pub mod garden_frame;
pub mod garden_graphics;
pub mod garden_home;
pub mod garden_home_tasks;
pub mod garden_palette;
pub mod garden_payload;
pub mod garden_puzzle_objects;
pub mod garden_runtime;
pub mod garden_save;
pub mod garden_services;
pub mod garden_shop;
pub mod garden_transfer;
pub mod graphics_frame;
pub mod graphics_ports;
pub mod interpolation;
pub mod intro;
pub mod intro_callbacks;
pub mod intro_sequence;
pub mod lifecycle;
pub mod lifecycle_tables;
pub mod math;
pub mod memory;
pub mod mode_menu;
pub mod mplay;
pub mod notes;
pub mod nudge;
pub mod oam;
pub mod oam_render;
pub mod oam_tree;
pub mod palette;
pub mod provenance;
pub mod psg;
pub mod scoring;
pub mod sequence;
pub mod sequence_extended;
pub mod sequencer;
pub mod sound;
pub mod sound_dma;
pub mod sprites;
pub mod startup;
pub mod table_actions;
pub mod table_bootstrap;
pub mod table_calls;
pub mod table_collision_setup;
pub mod table_events;
pub mod table_frame;
pub mod table_initialize;
pub mod table_objects;
pub mod table_presentation;
pub mod table_presentation_frame;
pub mod table_triggers;
pub mod table_triggers_next;
pub mod title;

pub use startup::INITIAL_CALLBACK;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchError {
    EmptyCallbackStack,
    UnportedCallback(u32),
    Startup(startup::StartupError),
}

impl std::fmt::Display for DispatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyCallbackStack => write!(f, "callback stack is empty"),
            Self::UnportedCallback(address) => write!(f, "unported callback 0x{address:08X}"),
            Self::Startup(error) => write!(f, "startup stopped: {error}"),
        }
    }
}
impl std::error::Error for DispatchError {}

/// Dispatch completed Rust callback wrappers by exact native ID. Dependencies
/// must also be Rust ports; an unknown ID or dependency stops execution.
pub fn dispatch_ported_callback(
    callbacks: &mut control::CallbackStack,
    auxiliary: &mut control::CallbackStack,
    memory: &mut dyn startup::StartupMemory,
    calls: &mut dyn startup::StartupCalls,
) -> Result<startup::StartupOutcome, DispatchError> {
    let address = callbacks
        .top()
        .ok_or(DispatchError::EmptyCallbackStack)?
        .callback
        .0;
    match address {
        INITIAL_CALLBACK => startup::initial_callback(callbacks, auxiliary, memory, calls)
            .map_err(DispatchError::Startup),
        _ => Err(DispatchError::UnportedCallback(address)),
    }
}

/// Bounded source-level storage for the startup wrapper's known global word.
/// The intro's context body is not fabricated by this diagnostic boundary.
#[derive(Debug, Default)]
pub struct StartupPointer {
    pub context: u32,
}
impl startup::StartupMemory for StartupPointer {
    fn read_word(&self, address: u32) -> Result<u32, startup::StartupError> {
        if address != startup::CONTEXT_POINTER_SLOT {
            return Err(startup::StartupError::MissingWord(address));
        }
        Ok(self.context)
    }
    fn write_word(&mut self, address: u32, value: u32) -> Result<(), startup::StartupError> {
        if address != startup::CONTEXT_POINTER_SLOT {
            return Err(startup::StartupError::MissingWord(address));
        }
        self.context = value;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dispatcher_runs_source_wrapper_and_reports_actual_missing_dependency() {
        let mut callbacks = control::CallbackStack::default();
        callbacks.push(control::CallbackId(INITIAL_CALLBACK), 0);
        let mut auxiliary = control::CallbackStack::default();
        let mut memory = StartupPointer::default();
        assert_eq!(
            dispatch_ported_callback(
                &mut callbacks,
                &mut auxiliary,
                &mut memory,
                &mut startup::RejectUnportedStartupCalls
            ),
            Err(DispatchError::Startup(
                startup::StartupError::UnportedNativeCall(startup::StartupCall {
                    entry_offset: 0x6ef4,
                    argument0: startup::INITIAL_CONTEXT,
                })
            ))
        );
        assert_eq!(callbacks.top().unwrap().parameter, 2);
        assert_eq!(memory.context, startup::INITIAL_CONTEXT);
        callbacks.replace(control::CallbackId(startup::NEXT_CALLBACK), 0);
        assert_eq!(
            dispatch_ported_callback(
                &mut callbacks,
                &mut auxiliary,
                &mut memory,
                &mut startup::RejectUnportedStartupCalls
            ),
            Err(DispatchError::UnportedCallback(startup::NEXT_CALLBACK))
        );
    }
}
