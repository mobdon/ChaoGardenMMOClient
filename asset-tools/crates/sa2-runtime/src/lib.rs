//! The SA2B engine core, ported from main.dol (GSNE8P, SHA-1
//! `e4c50f32242a3cd36cdc8ae776661601a52130a2`).
//!
//! Milestones E1 (task system) and E2 (frame step) of
//! `sa2/docs/ENGINE-CORE-PLAN.md`. The
//! engine map it follows is `sa2/docs/ENGINE-MAP.md`. Game routines stay in the
//! `sa2` crate and are registered into the runtime by their native address.
#![forbid(unsafe_code)]

pub mod frame;
pub mod task;

pub use frame::{FrameHost, GameState};
pub use task::{
    DisplayEngine, Event, Natives, Registry, Runtime, Slot, Task, TaskError, TaskId, WorkKind,
};
