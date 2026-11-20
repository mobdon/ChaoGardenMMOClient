//! Camera task creation (milestone E4 part 1; ENGINE-MAP.md section 6):
//! `fn_800C4458(viewports)`. It creates the camera task once, sets the
//! viewport count and resets the per-viewport camera blocks. The camera mode
//! routines it calls are not ported yet: they are logged as [`Event::Engine`]
//! and not run.
use crate::follow::{Follow, FollowHost, Inputs, Player};
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

/// Mode 0x13 "INIT" (fn_800C96BC): how far behind the player (r2-0x6290)
/// and how high above (r2-0x628C) it puts the camera.
const INIT_BACK: f32 = 60.0;
const INIT_UP: f32 = 30.0;
/// INIT hands over to slot 0 once its slot has run this many frames.
const INIT_FRAMES: u32 = 5;

/// What a camera mode writes into the view block 0x801FF3E4: the camera
/// position (+0xC..+0x14) and the point it looks at (+0x30..+0x38).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub pos: [f32; 3],
    pub target: [f32; 3],
}

/// One frame of mode 0x13 "INIT" (fn_800C96BC) for a player at `pos` with
/// twp angles `ang` and player work +0x13C `height`: the target is the
/// player raised by `height`; the camera sits `INIT_BACK` back along the
/// player's facing (fn_800C5524: (1,0,0) turned by Z, X, then -Y) and
/// `INIT_UP` higher.
pub fn init_view(pos: [f32; 3], ang: [i32; 3], height: f32) -> View {
    use sa2_asset_tools::{ninja_matrix, ninja_trig::fnmsubs};
    let mut stack = ninja_matrix::MatrixStack::new();
    let dir = ninja_matrix::rotate_by_angles(&mut stack, ang, [1.0, 0.0, 0.0]);
    let mut cam: [f32; 3] = std::array::from_fn(|k| fnmsubs(INIT_BACK, dir[k], pos[k]));
    cam[1] += INIT_UP;
    View {
        pos: cam,
        target: [pos[0], pos[1] + height, pos[2]],
    }
}

/// Viewport 0's default camera as `fn_800C4458` sets it up: INIT in slot
/// 0xE over CARMEL (mode 0xD, fn_800C86A4) in slot 0.
#[derive(Debug, Clone, Default)]
pub struct DefaultCamera {
    /// Slot 0xE's frame counter (+0x260), counted after each call.
    frames: u32,
    /// INIT dropped its slot (fn_800C3780).
    dropped: bool,
    /// CARMEL in slot 0 and the exec state it reads back.
    pub follow: Follow,
}

impl DefaultCamera {
    /// One camera exec (fn_800C3DA0, viewport 0): load last frame's view
    /// from the block, run the top slot (INIT, then CARMEL), derive the
    /// angles (fn_800C7590), keep the view (fn_800C7464) and the trail
    /// (fn_800C6B38).
    pub fn step(&mut self, host: &mut dyn FollowHost, p: &Player, inp: &Inputs<'_>) -> View {
        let f = &mut self.follow;
        f.unported.clear();
        let (kind, w4, w8) = (f.view.kind, f.view.w4, f.view.w8);
        f.view = f.block.view;
        (f.view.kind, f.view.w4, f.view.w8) = (kind, w4, w8);
        if self.dropped {
            f.carmel(host, p, inp);
            f.frames = f.frames.wrapping_add(1);
        } else {
            let v = init_view(p.pos, p.ang, p.height);
            f.view.pos = v.pos;
            f.view.target = v.target;
            if self.frames > INIT_FRAMES {
                f.reset_trail(inp.stage, p);
                self.dropped = true;
            }
            f.view.kind = 0;
            f.view.ang[2] = 0;
            f.view.w4 = 0;
            f.view.w8 = 0;
            self.frames = self.frames.saturating_add(1);
        }
        f.derive();
        f.keep();
        f.record(p, inp);
        f.execs = f.execs.wrapping_add(1);
        View {
            pos: f.view.pos,
            target: f.view.target,
        }
    }

    /// INIT has handed over to slot 0 (it drops itself on the frame its
    /// counter is past `INIT_FRAMES`).
    pub fn handed_over(&self) -> bool {
        self.dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_puts_the_camera_behind_and_above() {
        // Angle 0 faces +X, so the camera is 60 back along -X and 30 up.
        let v = init_view([10.0, 0.0, 5.0], [0; 3], 12.0);
        assert_eq!(v.target, [10.0, 12.0, 5.0]);
        assert_eq!(v.pos, [-50.0, 30.0, 5.0]);
        // A quarter turn (0x4000 BAMS) moves it onto the Z axis.
        let q = init_view([0.0; 3], [0, 0x4000, 0], 0.0);
        assert!(q.pos[0].abs() < 1e-3 && (q.pos[2].abs() - 60.0).abs() < 1e-3);
    }

    struct Open;
    impl FollowHost for Open {
        fn line(&mut self, _: [f32; 3], _: [f32; 3]) -> i32 {
            0
        }
        fn search_angles(&mut self, _: i32) -> Option<[i32; 2]> {
            None
        }
    }

    #[test]
    fn init_hands_over_to_carmel() {
        let (g, pad, hist) = (
            vec![0u8; 0xF0],
            vec![0u8; 0x34],
            vec![[10.0, 0.0, 5.0]; 0x100],
        );
        let inp = Inputs {
            stage: 11,
            g: &g,
            pad: &pad,
            second_player: false,
            history_index: 0,
            history_pos: &hist,
        };
        let p = Player {
            pos: [10.0, 0.0, 5.0],
            height: 12.0,
            character: 7,
            ..Player::default()
        };
        let mut c = DefaultCamera::default();
        // INIT runs while its counter is 0..=5 and drops itself on the 7th
        // call (counter 6).
        for _ in 0..=INIT_FRAMES {
            assert_eq!(c.step(&mut Open, &p, &inp).pos, [-50.0, 30.0, 5.0]);
            assert!(!c.handed_over());
        }
        c.step(&mut Open, &p, &inp);
        assert!(c.handed_over());
        // fn_800C6CDC fills the trail with the player; fn_800C6B38 then
        // adds the raised position (12 up is more than one unit away).
        assert_eq!(c.follow.trail[1..], [p.pos; 49]);
        assert_eq!(c.follow.trail[0], [10.0, 12.0, 5.0]);
        // CARMEL's first frame: state 2, the follow routine eases the
        // target from INIT's and keeps the camera within 30..70.
        let v = c.step(&mut Open, &p, &inp);
        assert_eq!(c.follow.work.state, 2);
        assert!(c.follow.unported.is_empty());
        let r = ((v.pos[0] - 10.0).powi(2) + (v.pos[2] - 5.0).powi(2)).sqrt();
        assert!((30.0..=70.0).contains(&r), "{r}");
        assert_eq!(v.target, [10.0, 12.0, 5.0]);
    }
}
