//! What the shared stage objects ask of the rest of the game. Each method
//! or event stands for one main.dol routine the objects call and this module
//! does not port; it is named after that routine (GSNE8P addresses). A
//! session implements the trait; the harness tests record it.
pub type V3 = [f32; 3];

/// Something an object starts or changes outside its own task state.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// CCL_Init(task, rows, count, list) (0x8000F8B8): the collision entries
    /// are set up from the stage REL's CollisionData rows at this section-5
    /// offset.
    CollisionInit { rows: u32, count: u8, list: u8 },
    /// The first collision info's a, b, c (`cwp->info +0x14..+0x1C`) are
    /// written.
    CollisionSizes(V3),
    /// fn_800068E4(cwp): `CCL_CalcRange` on the collision work.
    CollisionCalcRange,
    /// CCL_Entry(task) (0x8000F4EC): the collision takes part this frame.
    CollisionEntry,
}

pub trait ObjectHost {
    /// CheckRangeOut(task) (0x800655F4). When it returns true it has
    /// already set the task's exec to DeadOut.
    fn range_out(&mut self) -> bool;
    /// CheckRangeOutWithR(task, r) (0x80065498).
    fn range_out_with_r(&mut self, r: f32) -> bool;
    /// A call the object makes, in order.
    fn event(&mut self, e: Event);
}
