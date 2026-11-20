//! Camera task creation (milestone E4 part 1; ENGINE-MAP.md section 6):
//! `fn_800C4458(viewports)`. It creates the camera task once, sets the
//! viewport count and resets the per-viewport camera blocks. The camera mode
//! routines it calls are not ported yet: they are logged as [`Event::Engine`]
//! and not run.
use crate::task::{Runtime, TaskId};

/// The camera task's first exec.
pub const CAMERA_EXEC: u32 = 0x800C_3CF0;
/// Its destructor.
pub const CAMERA_DEST: u32 = 0x800C_3D90;
/// Its task name pointer.
pub const CAMERA_NAME: u32 = 0x8017_5388;
/// Camera blocks: 0x801FF424 + 0x24D8 * viewport; the table 0x80175378 holds
/// each block's +0x194.
pub const BLOCKS: u32 = 0x801F_F424;
pub const BLOCK_SIZE: u32 = 0x24D8;
const CREATED: u32 = 0x800C_34C4;
const ALWAYS: [u32; 3] = [0x800C_5914, 0x800C_6F70, 0x800C_91C4];
const VIEW_RESET: u32 = 0x800C_36A4;
const MODE_SET: u32 = 0x800C_3A94;
const AFTER: u32 = 0x800C_3C38;
const AFTER_2: u32 = 0x800C_76F4;

/// The camera globals `fn_800C4458` writes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Camera {
    /// 0x803ADAF8: the camera task.
    pub task: Option<TaskId>,
    /// 0x803ADAD4.
    pub viewports: i32,
    /// 0x803ADAD0.
    pub index: i32,
    /// 0x803ADB1C, 0x803ADB00, 0x803ADB04, 0x803ADAFC.
    pub adb1c: u32,
    pub adb00: u32,
    pub adb04: u32,
    pub adafc: u32,
    /// 0x80175378: per viewport, the block's +0x194 (0 when unused).
    pub table: [u32; 4],
    /// Words +0, +4, +8 of each camera block.
    pub blocks: [[u32; 3]; 4],
}

/// `fn_800C4458(count)`.
pub fn camera_init(rt: &mut Runtime, cam: &mut Camera, stage: i16, count: i32) {
    if cam.task.is_none() {
        if let Ok(t) = rt.create_elemental_task(2, 0, CAMERA_EXEC, CAMERA_NAME) {
            cam.task = Some(t);
            if let Some(x) = rt.task_mut(t) {
                x.slots[2] = CAMERA_DEST;
            }
        }
        rt.engine(CREATED, vec![]);
    }
    for a in ALWAYS {
        rt.engine(a, vec![]);
    }
    cam.viewports = count;
    cam.index = 0;
    cam.adb1c = 0;
    for i in 0..4 {
        if (i as i32) < count {
            cam.table[i] = BLOCKS + BLOCK_SIZE * i as u32 + 0x194;
            cam.blocks[i] = [0x31C7, 0x31C7, 0x4000];
            rt.engine(VIEW_RESET, vec![i as u32]);
            rt.engine(MODE_SET, vec![i as u32, 0, 0xD]);
        } else {
            cam.table[i] = 0;
        }
        if cam.adb00 == 0 && stage < 0x3C {
            rt.engine(MODE_SET, vec![i as u32, 0xE, 0x13]);
        }
    }
    cam.adb00 = 0;
    cam.adb04 = 0;
    rt.engine(AFTER, vec![]);
    rt.engine(AFTER_2, vec![0]);
    cam.adafc = 0;
}
