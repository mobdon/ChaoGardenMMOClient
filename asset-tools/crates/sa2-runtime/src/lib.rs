//! The SA2B engine core, ported from main.dol (GSNE8P, SHA-1
//! `e4c50f32242a3cd36cdc8ae776661601a52130a2`).
//!
//! Milestones E1 (task system), E2 (frame step) and E3 (PAD path) of
//! `sa2/docs/ENGINE-CORE-PLAN.md`. The
//! engine map it follows is `sa2/docs/ENGINE-MAP.md`. Game routines stay in the
//! `sa2` crate and are registered into the runtime by their native address.
#![forbid(unsafe_code)]

pub mod camera;
pub mod dvd;
pub mod follow;
pub mod frame;
pub mod game;
pub mod host_pad;
pub mod input;
pub mod objects;
pub mod player;
pub mod session;
pub mod set;
pub mod sound;
pub mod stage;
pub mod steam_tex;
pub mod task;
pub mod walker_slot;

pub use frame::{FrameHost, GameState};
pub use input::{PadContext, PadRef, PadSystem};
pub use task::{
    DisplayEngine, Event, Natives, Registry, Runtime, Slot, Task, TaskError, TaskId, WorkKind,
};
