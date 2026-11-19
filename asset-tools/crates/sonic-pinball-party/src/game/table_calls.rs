//! Compose completed table routines against one native memory image. Unknown
//! graphics/audio/game callbacks remain synchronous backend dependencies.
use super::{
    frame::{FrameError, FrameMemory, NativeCall},
    lifecycle::{self, LifecycleCalls},
    lifecycle_tables, scoring, table_actions, table_bootstrap, table_collision_setup, table_events,
    table_frame, table_objects, table_presentation, table_presentation_frame, table_triggers,
    table_triggers_next,
};

pub struct TableCalls<B> {
    pub backend: B,
}
impl<B> TableCalls<B> {
    pub fn new(backend: B) -> Self {
        Self { backend }
    }
}
impl<B: LifecycleCalls> LifecycleCalls for TableCalls<B> {
    fn call(
        &mut self,
        memory: &mut dyn FrameMemory,
        offset: u32,
        args: &[u32],
    ) -> Result<(), FrameError> {
        match (offset, args) {
            (0x2e1a0, []) => lifecycle::dispatch(memory, &mut lifecycle_tables::TableCalls(self)),
            (0x2ea04, []) => table_events::dispatch(memory, self),
            (0x2fd68, []) => table_frame::presentation(memory, self),
            (0xabb4, []) => table_frame::control(memory, self),
            (0xab78, &[context]) => table_frame::update(memory, self, context),
            (0x1fc30, _) => table_objects::TableObjectCalls(self).call(memory, offset, args),
            (0x30c58 | 0x33d40, _) => scoring::ScoringCalls(self).call(memory, offset, args),
            (0x34044 | 0x34168, _) => table_triggers::TriggerCalls(self).call(memory, offset, args),
            (0x344e8 | 0x347dc, _) => {
                table_triggers_next::DirectionalTriggerCalls(self).call(memory, offset, args)
            }
            (0x2e51c | 0x5c544, []) => {
                lifecycle_tables::TableCalls(self).call(memory, offset, args)
            }
            (0x2dff8 | 0x2f9b4 | 0x2f634 | 0x3de48 | 0x468a8, _) => {
                table_actions::ActionCalls(self).call(memory, offset, args)
            }
            (0x3046c | 0x2ff40, _) => {
                table_presentation::PresentationCalls(self).call(memory, offset, args)
            }
            (0x304a0, _) => {
                table_presentation_frame::AnimationCalls(self).call(memory, offset, args)
            }
            (0xa668 | 0xa808 | 0xa85c | 0xa9f0 | 0xaa20 | 0x2f68 | 0x2f74 | 0x2f92c, _) => {
                table_bootstrap::BootstrapCalls(self).call(memory, offset, args)
            }
            (0x2b824 | 0x2b8ac | 0x2b920, _) => {
                table_collision_setup::CollisionSetupCalls(self).call(memory, offset, args)
            }
            (0x2e1a0 | 0x2ea04 | 0x2fd68 | 0xabb4 | 0xab78 | 0x2e51c | 0x5c544, _) => {
                Err(FrameError::Dependency {
                    call: NativeCall {
                        entry_offset: offset,
                        argument0: args.first().copied(),
                    },
                    reason: format!("unexpected native argument count: {args:#x?}"),
                })
            }
            _ => self.backend.call(memory, offset, args),
        }
    }
}
