//! MSGER, ObjectMessenger (object list row 22): Omochao, the hint robot, and
//! the message tasks its spawner starts.
//!
//! stg42D.rel section 1 (decomp split `stg42D_39.c`, `OBJECT/o_messenger.c`):
//!
//! * 0x182AC, the object-list function and Omochao's set-up ([`create`]);
//! * 0x18620, the spawner other code calls to show a message or bring in
//!   Omochao ([`spawn`]);
//! * 0x1850C, Omochao's destructor ([`destroy`]);
//! * 0x1826C, the exec of Omochao's collision child ([`collision_child_exec`]);
//! * 0x185C8 / 0x185F4, exec and destructor of a message task
//!   ([`message_exec`], [`message_destroy`]);
//! * 0x18040, the sorted display (task +0x20) that moves the propeller
//!   trails ([`display_trails`]);
//! * 0x188FC, the shadow display (task +0x2C) ([`display_shadow`]);
//! * 0x141B8, the message-bank lookup ([`message_text`]);
//! * 0x6F28, the depth clip both displays use ([`clip`]);
//! * and the ribbon-trail library in [`trail`].
//!
//! Omochao's exec (0x156CC, 0x1D9C bytes) and main display (0x17468, 0xBD8
//! bytes) are not ported here: [`create`] only installs them. The module's
//! shadow-texture helpers (0x7344, 0x7400, 0x6F94, 0x72D0, 0x7440), its land
//! collision set-up and release (0x4B44, 0x4BA8) and Omochao's placement
//! next to the player (0x14608, 0x148A4) are not ported either: they are
//! the [`Event`]s and [`MessengerHost`] methods named after their offsets.
//! The message window itself is main.dol's (fn_800E00E8).
//!
//! The data the routines read from the stage REL is [`StageData`]
//! ([`STG42D`] for the reference copy; each stage has its own copy).
pub mod trail;

use super::host::ObjectHost;
use crate::enemy_lib::e_ai::translate;
use crate::ninja_matrix::{concat, Axis, MatrixStack, Mtx, IDENTITY};
use crate::ninja_trig::nj_sin;
use trail::{Trail, TrailDesc, TrailHost};

pub type V3 = [f32; 3];

/// The routines this file ports (stg42D.rel section 1).
pub const ROUTINES: &[super::Routine] = &[
    super::Routine {
        name: "trail midpoint (0x5C30)",
        stg42d: 0x5c30,
        len: 0x54,
        masked_sha256: "0236df174b4cf2e693a0d59f19ecaa8dcb64b4ee3bc751e6e47b2c8142f0d05c",
    },
    super::Routine {
        name: "trail alloc (0x635C)",
        stg42d: 0x635c,
        len: 0x78,
        masked_sha256: "8ec537b561fc78bdbd798f7d83f515fe6566f0142b79d31066f729b3931e0bf4",
    },
    super::Routine {
        name: "trail curves (0x63D4..0x6478)",
        stg42d: 0x63d4,
        len: 0xa4,
        masked_sha256: "ca627942074f0257b415cfa48f8f2b7b70dda71a8441707acb2ee897ec98e9e1",
    },
    super::Routine {
        name: "trail step plain (0x6478)",
        stg42d: 0x6478,
        len: 0x4c0,
        masked_sha256: "a8af0eeb421b92792df4197804f4ac21eb14baf4325ba700ba56fc399920c8bb",
    },
    super::Routine {
        name: "trail step textured (0x6938)",
        stg42d: 0x6938,
        len: 0x518,
        masked_sha256: "efc42708516c6a27f2e2e4badeccededec5a58597fab864b242895285a484ddf",
    },
    super::Routine {
        name: "trail step (0x6E50)",
        stg42d: 0x6e50,
        len: 0x54,
        masked_sha256: "59124011ef579706deb4626d7eb492d3dde32eb4c1c53ef4cbbb5aa7f8754485",
    },
    super::Routine {
        name: "trail free (0x6EA4)",
        stg42d: 0x6ea4,
        len: 0x40,
        masked_sha256: "aaaeeb1f1fc4a1dee8b732ef20e22eaca8b66e1a89593522615ab7476dc103a1",
    },
    super::Routine {
        name: "trail drop oldest (0x6EE4)",
        stg42d: 0x6ee4,
        len: 0x44,
        masked_sha256: "d5c739b112921f0f2b718d93429eeea62cc8cb6c9ca26ea87dbc4970072dc866",
    },
    super::Routine {
        name: "depth clip (0x6F28)",
        stg42d: 0x6f28,
        len: 0x6c,
        masked_sha256: "11084051788a8d1e54c9b104a0fd82d159966ae42a7d9eaee5f91f12c81ae32a",
    },
    super::Routine {
        name: "MSGER message lookup (0x141B8)",
        stg42d: 0x141b8,
        len: 0x3c,
        masked_sha256: "f2e23cda4ea927e37c8b09593a572868d782bbacf438d7b31b1cc1a214d7135c",
    },
    super::Routine {
        name: "MSGER trail display (0x18040)",
        stg42d: 0x18040,
        len: 0x22c,
        masked_sha256: "e71567dc9b0bfd41cc181e92c6b6c8f572de10bc27fd6a6bc21ae42c9982c58e",
    },
    super::Routine {
        name: "MSGER collision child (0x1826C)",
        stg42d: 0x1826c,
        len: 0x40,
        masked_sha256: "c26d51ee4e20fa023dd77f085d94a6000b31cc77014c6e3a2e4c18aa3c4d304f",
    },
    super::Routine {
        name: "MSGER create (0x182AC)",
        stg42d: 0x182ac,
        len: 0x260,
        masked_sha256: "a417326095d5c951ac9a1b3b0e90cfa515f016d153a02789a7cb179ffdc7f49f",
    },
    super::Routine {
        name: "MSGER destroy (0x1850C)",
        stg42d: 0x1850c,
        len: 0xbc,
        masked_sha256: "4a3ebcd2693e70751fa4df819d01de8c20f7047eb5e2de2a95ecba5fc95359b3",
    },
    super::Routine {
        name: "MSGER message task (0x185C8)",
        stg42d: 0x185c8,
        len: 0x58,
        masked_sha256: "4a041519e9295b6c4ed3675020f873cedfc54a0b915db3219f5c222ced985e7e",
    },
    super::Routine {
        name: "MSGER spawner (0x18620)",
        stg42d: 0x18620,
        len: 0x2dc,
        masked_sha256: "86ff464defafc1f904a30ee9f5abf1337e867fbdd702aaca480295cd3d15d73e",
    },
    super::Routine {
        name: "MSGER shadow display (0x188FC)",
        stg42d: 0x188fc,
        len: 0x188,
        masked_sha256: "f13ac361fa2ad2589cbb12e9e8917c4f3e2a41e1200c2f48c4bc0ccd33f50080",
    },
];

/// main.dol DestroyTask (0x80071AF0); also the exec a dying task has.
pub const DESTROY_TASK: u32 = 0x8007_1af0;

/// The stage REL's data the messenger code reads (section 5 offsets).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StageData {
    /// CCL_Init rows: Omochao from the object list (+0x174CC) or spawned
    /// (+0x1752C); its collision child likewise (+0x1758C, +0x175BC).
    pub rows: [u32; 4],
    /// CreateElementalTask names: message task (+0x177E4), Omochao
    /// (+0x177F8).
    pub names: [u32; 2],
    /// The text shown when the message index has no entry (+0x175F0).
    pub default_text: u32,
    /// The window kind of a mode-1 message (the word at +0x17494).
    pub window_kind_1: i32,
    /// What Omochao's set-up writes to mwp +0x30 (the word at +0x177E0).
    pub mwp_30: u32,
    /// The propeller tips in Omochao's frame (+0x177C8, +0x177D4).
    pub tips: [V3; 2],
    /// The shadow model (the pointer at +0x17338, to +0x17310) and its
    /// texlist (+0x129B4).
    pub shadow_model: u32,
    pub shadow_texlist: u32,
    /// The trail descriptor (+0x173BC).
    pub trail: TrailDesc,
}

/// stg42D's copy (section 5).
pub const STG42D: StageData = StageData {
    rows: [0x174cc, 0x1752c, 0x1758c, 0x175bc],
    names: [0x177e4, 0x177f8],
    default_text: 0x175f0,
    window_kind_1: 1,
    mwp_30: 0x1100,
    tips: [
        [
            f32::from_bits(0xbf33_3333),
            f32::from_bits(0x3dcc_cccd),
            0.0,
        ],
        [
            f32::from_bits(0x3f33_3333),
            f32::from_bits(0x3dcc_cccd),
            0.0,
        ],
    ],
    shadow_model: 0x17310,
    shadow_texlist: 0x129b4,
    trail: trail::MESSENGER_TRAIL,
};

/// Omochao's callbacks, section 1 offsets (task +0x10 exec, +0x14 display,
/// +0x18 destructor, +0x20 sorted display, +0x2C shadow display).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Callbacks {
    pub exec: u32,
    pub disp: u32,
    pub dest: u32,
    pub disp_sort: u32,
    pub disp_shad: u32,
}

/// What [`create`] installs.
pub const CALLBACKS: Callbacks = Callbacks {
    exec: 0x156cc,
    disp: 0x17468,
    dest: 0x1850c,
    disp_sort: 0x18040,
    disp_shad: 0x188fc,
};

/// The task work (twp) fields this code touches.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Twp {
    /// +0 (Omochao's mode).
    pub mode: u8,
    /// +1 (bit 2 set by the trail display when within the clip depth).
    pub flag1: u8,
    /// +4 (bit 0x8000: riding on the player).
    pub flags: i16,
    /// +8, +0xC, +0x10.
    pub ang: [i32; 3],
    /// +0x14.
    pub pos: V3,
    /// +0x20 (the message index, as a float), +0x24 (no trails at 1.0 or
    /// more), +0x28.
    pub scale: V3,
}

/// Omochao's work (the 0xDC-byte block at task +0x40).
#[derive(Debug, Clone, PartialEq)]
pub struct Work {
    /// The block's allocation.
    pub id: u32,
    /// +0.
    pub pos: V3,
    /// +0x24 (the bob angle the displays take njSin of).
    pub bob: i32,
    /// +0x30.
    pub f30: f32,
    /// +0x3C (a task the destructor ends).
    pub sub_task: u32,
    /// +0x40.
    pub home: V3,
    /// +0x4C.
    pub w4c: i32,
    /// +0x50.
    pub w50: u32,
    /// +0x54 (the player Omochao rides on).
    pub player: i32,
    /// +0x74, +0x78, +0x7C, +0x80: the propeller trails (left tip for
    /// screens 0 and 1, then the right tip for screens 0 and 1).
    pub trails: [Option<Trail>; 4],
}

impl Work {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            pos: [0.0; 3],
            bob: 0,
            f30: 0.0,
            sub_task: 0,
            home: [0.0; 3],
            w4c: 0,
            w50: 0,
            player: 0,
            trails: [None, None, None, None],
        }
    }
}

/// An Omochao task.
#[derive(Debug, Clone, PartialEq)]
pub struct MessengerTask {
    pub id: u32,
    /// task +0x4C: 0 from the object list, 2 from the spawner.
    pub mode: i32,
    /// task +0x38 is set (the set-up does nothing without it).
    pub has_mwp: bool,
    /// mwp +0x30.
    pub mwp_30: u32,
    /// cwp +0x60, when the task has a collision work.
    pub cwp_flags: Option<u32>,
    pub callbacks: Option<Callbacks>,
    pub twp: Twp,
    /// task +0x40.
    pub work: Option<Work>,
}

/// A task the engine created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewTask {
    pub id: u32,
    /// The task has an mwp (task +0x38).
    pub has_mwp: bool,
}

/// A call the messenger code makes whose result it does not read.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// 0x7344(work +0x84, 0x20): the shadow texture is set up.
    ShadowInit { work: u32, size: u32 },
    /// 0x7400(work +0x84): the shadow texture is released.
    ShadowFree { work: u32 },
    /// 0x4B44(task): the land collision is set up.
    LandInit { task: u32 },
    /// 0x4BA8(task): the land collision is released.
    LandFree { task: u32 },
    /// 0x148A4(task, a, b): Omochao is placed (a, b from 0x14608).
    Place { task: u32, a: V3, b: V3 },
    /// The collision child's twp position is set (and its twp +2 becomes 6
    /// after its CCL_Init).
    ChildInit { task: u32, pos: V3 },
    /// DestroyTask (0x80071AF0).
    DestroyTask(u32),
    /// CCL_Entry (0x8000F4EC).
    CollisionEntry { task: u32 },
    /// SE_Call(id, 0, 0, 0) (0x8006B978).
    Sound(u32),
    /// njEnableFog (0x8011C2B0, true) / njDisableFog (0x8011C2BC, false).
    Fog(bool),
    /// gjSetFog (0x8011C310).
    SetFog,
    /// __njColorBlendingMode(target, mode) (0x801179F8).
    Blend(u32, u32),
    /// 0x6F94(work +0x84, 1.0, pos, 0, 0, sin): the shadow texture capture.
    ShadowCapture { work: u32, pos: V3, sin: f32 },
    /// njSetTexture(texlist) (0x80116E18), a section-5 offset.
    SetTexture(u32),
    /// gjDrawModel(model) (0x80121BDC) with this current matrix.
    DrawModel { model: u32, matrix: Mtx },
    /// 0x72D0(work +0x84).
    ShadowEnd { work: u32 },
    /// 0x7440(kind, pos, size, work +0x84).
    ShadowDraw {
        kind: u32,
        pos: V3,
        size: f32,
        work: u32,
    },
}

/// What the messenger code asks of the engine and of the module code this
/// port leaves out.
pub trait MessengerHost: ObjectHost + TrailHost {
    /// CCL_Init(task, rows, count, list) (0x8000F8B8): returns the task's
    /// cwp +0x60 when it has a collision work afterwards.
    fn ccl_init(&mut self, task: u32, rows: u32, count: u8, list: u8) -> Option<u32>;
    /// 0x14608(0, a, b): the two vectors it writes.
    fn place_init(&mut self) -> (V3, V3);
    /// CreateChildTask(flags, exec, parent) (0x800709F8); `exec` is a
    /// section-1 offset.
    fn create_child_task(&mut self, flags: u8, exec: u32, parent: u32) -> Option<NewTask>;
    /// CreateElementalTask(flags, level, exec, name) (0x80071E0C); `name`
    /// is a section-5 offset.
    fn create_elemental_task(
        &mut self,
        flags: u8,
        level: u8,
        exec: u32,
        name: u32,
    ) -> Option<NewTask>;
    /// fn_800E00E8(kind, text, time, language, &owner->+0x4C, 0): a message
    /// window opens; returns what it stores at the owner's +0x4C.
    fn message_open(&mut self, kind: i32, text: u32, time: u32, language: i8, owner: u32) -> u32;
    /// (s8)lbl_801CC168[0x11]: the text language.
    fn language(&mut self) -> i8;
    /// The message bank's entry count (section 5 +0x1747C).
    fn message_count(&mut self) -> i32;
    /// Entry `i` of the loaded message bank (the table at section 6
    /// +0x698).
    fn message_entry(&mut self, i: i32) -> u32;
    /// playertwp[n] (0x801E7768) angles (+8, +0xC, +0x10).
    fn player_angles(&mut self, n: i32) -> [i32; 3];
    /// lbl_803ADAD0: the screen being drawn.
    fn screen(&mut self) -> i32;
    /// A call with no result.
    fn msg_event(&mut self, e: Event);
}

/// 0x141B8: entry `i` of the message bank, or 0 when `i` is outside
/// `0..count`.
pub fn message_text(i: i32, host: &mut impl MessengerHost) -> u32 {
    if i < 0 || i >= host.message_count() {
        return 0;
    }
    host.message_entry(i)
}

/// 0x6F28: the point through the current matrix is within the clip depth
/// (`z <= r`; false for NaN).
pub fn clip(stack: &MatrixStack, pos: V3, r: f32) -> bool {
    stack.calc_point(pos)[2] <= r
}

/// 0x1826C: the collision child's exec. It takes part in collisions
/// (CCL_Entry) unless its parent is being destroyed (its exec is
/// DestroyTask). `parent_exec` is None without a parent.
pub fn collision_child_exec(task: u32, parent_exec: Option<u32>, host: &mut impl MessengerHost) {
    if parent_exec != Some(DESTROY_TASK) {
        host.msg_event(Event::CollisionEntry { task });
    }
}

/// 0x185C8: a message task ends itself once its window is gone (+0x4C 0).
pub fn message_exec(task: u32, window: u32, host: &mut impl MessengerHost) {
    if window == 0 {
        host.msg_event(Event::DestroyTask(task));
    }
}

/// 0x185F4: a message task's destructor ends its window (+0x4C).
pub fn message_destroy(window: u32, host: &mut impl MessengerHost) {
    if window != 0 {
        host.msg_event(Event::DestroyTask(window));
    }
}

/// 0x182AC: Omochao's set-up (the object-list function, also called by the
/// spawner). Nothing happens without an mwp, or (from the object list)
/// out of range (CheckRangeOut), or when the 0xDC-byte work cannot be
/// allocated.
pub fn create(task: &mut MessengerTask, data: &StageData, host: &mut impl MessengerHost) {
    if !task.has_mwp {
        return;
    }
    if task.mode == 0 && host.range_out() {
        return;
    }
    let Some(id) = host.sy_calloc(1, 0xdc) else {
        return;
    };
    let mut w = Work::new(id);
    host.msg_event(Event::ShadowInit {
        work: id,
        size: 0x20,
    });
    for t in w.trails.iter_mut() {
        *t = trail::alloc(8, host);
    }
    task.callbacks = Some(CALLBACKS);
    w.home = task.twp.pos;
    w.w4c = -1;
    task.work = Some(w);
    let (a, b) = host.place_init();
    let spawned = task.mode != 0;
    let cwp = host.ccl_init(task.id, data.rows[usize::from(spawned)], 2, 9);
    host.msg_event(Event::LandInit { task: task.id });
    task.mwp_30 = data.mwp_30;
    task.cwp_flags = cwp.map(|f| f | 0x10);
    host.msg_event(Event::Place {
        task: task.id,
        a,
        b,
    });
    if let Some(child) = host.create_child_task(2, 0x1826c, task.id) {
        host.msg_event(Event::ChildInit {
            task: child.id,
            pos: task.twp.pos,
        });
        host.ccl_init(child.id, data.rows[2 + usize::from(spawned)], 1, 1);
    }
}

/// 0x1850C: Omochao's destructor. (The original reads through a NULL
/// work; here that does nothing.)
pub fn destroy(task: &mut MessengerTask, host: &mut impl MessengerHost) {
    let Some(mut w) = task.work.take() else {
        return;
    };
    if w.sub_task != 0 {
        host.msg_event(Event::DestroyTask(w.sub_task));
    }
    host.msg_event(Event::ShadowFree { work: w.id });
    host.msg_event(Event::LandFree { task: task.id });
    for t in w.trails.iter_mut() {
        trail::free(t.take(), host);
    }
    host.sy_free(w.id);
}

/// What [`spawn`] made.
#[derive(Debug, Clone, PartialEq)]
pub enum Spawned {
    /// Modes 0 and 1: a message task and the window handle at its +0x4C.
    Message { task: u32, window: u32 },
    /// Mode 2: Omochao.
    Messenger(Box<MessengerTask>),
}

/// The spawner's arguments (r3..r8).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpawnArgs {
    /// r3 (Omochao's position).
    pub pos: V3,
    /// r4 (Omochao's twp +0xC angle).
    pub yaw: i32,
    /// r5 (Omochao's work +0x50).
    pub w50: u32,
    /// r6: the message bank index.
    pub message: i32,
    /// r7: 0 or 1 message window, 2 Omochao.
    pub mode: i32,
    /// r8: the parent task, if any.
    pub parent: Option<u32>,
}

/// 0x18620: the spawner. Modes 0 and 1 open a message window (kind 5, or
/// the stage's `window_kind_1`) showing entry `message` of the bank (or
/// the default text) for 600 frames, owned by a new message task; mode 2
/// brings in Omochao at `pos` facing `yaw`, with the message index (as a
/// float) at twp +0x20 and `w50` in its work, and plays sound 0x800B. With
/// a parent the task is its child, else an elemental task. Other modes
/// return NULL, as does a failed task creation.
pub fn spawn(a: &SpawnArgs, data: &StageData, host: &mut impl MessengerHost) -> Option<Spawned> {
    match a.mode {
        0 | 1 => {
            let t = match a.parent {
                Some(p) => host.create_child_task(2, 0x185c8, p),
                None => host.create_elemental_task(2, 2, 0x185c8, data.names[0]),
            }?;
            let text = match message_text(a.message, host) {
                0 => data.default_text,
                _ => message_text(a.message, host),
            };
            let kind = if a.mode == 0 { 5 } else { data.window_kind_1 };
            let language = host.language();
            let window = host.message_open(kind, text, 600, language, t.id);
            Some(Spawned::Message { task: t.id, window })
        }
        2 => {
            let t = match a.parent {
                Some(p) => host.create_child_task(3, 0x182ac, p),
                None => host.create_elemental_task(3, 2, 0x182ac, data.names[1]),
            }?;
            let mut task = MessengerTask {
                id: t.id,
                mode: 2,
                has_mwp: t.has_mwp,
                mwp_30: 0,
                cwp_flags: None,
                callbacks: None,
                twp: Twp {
                    pos: a.pos,
                    ang: [0, a.yaw, 0],
                    scale: [a.message as f32, 1.0, 0.0],
                    ..Twp::default()
                },
                work: None,
            };
            create(&mut task, data, host);
            // The original writes these through task +0x40 even when the
            // set-up allocated no work; here they are skipped.
            if let Some(w) = task.work.as_mut() {
                w.w50 = a.w50;
                w.f30 = 1.0;
            }
            task.twp.mode = 6;
            if let Some(w) = task.work.as_mut() {
                w.pos = task.twp.pos;
            }
            host.msg_event(Event::Sound(0x800b));
            Some(Spawned::Messenger(Box::new(task)))
        }
        _ => None,
    }
}

/// njTranslate(NULL, ...) / njTranslateEx on the current matrix.
fn nj_translate(stack: &mut MatrixStack, v: V3) {
    translate(stack.current_mut(), v);
}

/// The frame both displays use when Omochao rides on the player: translate
/// to the position, then the player's Z and X angles.
fn ride(stack: &mut MatrixStack, pos: V3, player: i32, host: &mut impl MessengerHost) {
    nj_translate(stack, pos);
    let a = host.player_angles(player);
    stack.rotate(Axis::Z, a[2]);
    stack.rotate(Axis::X, a[0]);
}

/// 0x18040: the sorted display (task +0x20), which moves the propeller
/// trails. Within the clip depth (100.0, section 4 +0x788) it marks twp +1
/// bit 2, puts the two tips into world space (bobbing by 2.3 *
/// njSin(work +0x24), +0x858) with a fresh matrix and, while twp +0x24 is
/// below 1.0, steps the drawn screen's two trails with them (through the
/// current matrix, which the caller set).
// The negated comparison keeps the NaN behaviour (NaN steps the trails).
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn display_trails(
    task: &mut MessengerTask,
    data: &StageData,
    stack: &mut MatrixStack,
    host: &mut impl MessengerHost,
) {
    if !clip(stack, task.twp.pos, 100.0) {
        return;
    }
    task.twp.flag1 |= 4;
    // (The original reads through a NULL work; here that stops.)
    let Some(w) = task.work.as_mut() else {
        return;
    };
    let twp = &mut task.twp;
    let bob = 2.3f32 * nj_sin(w.bob);
    host.msg_event(Event::Fog(false));
    host.msg_event(Event::SetFog);
    host.msg_event(Event::Blend(0, 8));
    host.msg_event(Event::Blend(1, 10));
    stack.push();
    *stack.current_mut() = IDENTITY;
    if twp.flags as u16 & 0x8000 != 0 {
        ride(stack, twp.pos, w.player, host);
        nj_translate(stack, [0.0, bob, 0.0]);
    } else {
        nj_translate(stack, [twp.pos[0], twp.pos[1] + bob, twp.pos[2]]);
    }
    stack.rotate(Axis::Y, twp.ang[1]);
    stack.rotate(Axis::X, twp.ang[0]);
    stack.rotate(Axis::Z, twp.ang[2]);
    let left = stack.calc_point(data.tips[0]);
    let right = stack.calc_point(data.tips[1]);
    stack.pop();
    if !(twp.scale[1] >= 1.0) {
        let s = host.screen();
        // The original indexes work +0x74 + 4 * screen (and +0x7C) without
        // a check; screens are 0 and 1.
        let pick = |k: i32| usize::try_from(k).ok().filter(|k| *k < 4);
        if let Some(k) = pick(s) {
            trail::step(Some(&data.trail), w.trails[k].as_mut(), left, stack, host);
        }
        if let Some(k) = pick(s.wrapping_add(2)) {
            trail::step(Some(&data.trail), w.trails[k].as_mut(), right, stack, host);
        }
    }
    host.msg_event(Event::Blend(0, 8));
    host.msg_event(Event::Blend(1, 6));
    host.msg_event(Event::Fog(true));
    host.msg_event(Event::SetFog);
}

/// 0x188FC: the shadow display (task +0x2C). Within the clip depth the
/// shadow texture is captured and the shadow model drawn at the position
/// (riding the player's Z and X angles when twp +4 bit 0x8000 is set),
/// scaled by (1.1, 1.0, 1.1) (section 4 +0x864, +0x7FC); then the shadow
/// is drawn with size 82.0 (+0x868).
pub fn display_shadow(
    task: &MessengerTask,
    data: &StageData,
    stack: &mut MatrixStack,
    host: &mut impl MessengerHost,
) {
    let twp = &task.twp;
    if !clip(stack, twp.pos, 100.0) {
        return;
    }
    let Some(w) = task.work.as_ref() else {
        return;
    };
    host.msg_event(Event::ShadowCapture {
        work: w.id,
        pos: twp.pos,
        sin: nj_sin(w.bob),
    });
    host.msg_event(Event::SetTexture(data.shadow_texlist));
    stack.push();
    if twp.flags as u16 & 0x8000 != 0 {
        ride(stack, twp.pos, w.player, host);
    } else {
        nj_translate(stack, twp.pos);
    }
    // njScale (0x80115178): PSMTXScale, then PSMTXConcat(m, s, m).
    let s = 1.1f32;
    let scale: Mtx = [[s, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, s, 0.0]];
    *stack.current_mut() = concat(stack.current(), &scale);
    host.msg_event(Event::DrawModel {
        model: data.shadow_model,
        matrix: *stack.current(),
    });
    stack.pop();
    host.msg_event(Event::ShadowEnd { work: w.id });
    host.msg_event(Event::ShadowDraw {
        kind: 2,
        pos: twp.pos,
        size: 82.0,
        work: w.id,
    });
}
