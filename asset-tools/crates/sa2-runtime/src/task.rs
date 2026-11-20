//! The native task system: the 0x50-byte task header, the eight task lists,
//! creation, freeing and destruction, and the per-frame exec and display
//! passes (main.dol 0x800706B0–0x80072080; ENGINE-MAP.md section 1 and 2).
//!
//! What is abstracted, and why:
//! - **Memory.** Tasks live in an arena indexed by [`TaskId`] (creation order,
//!   never reused) instead of heap addresses. Native list order depends only on
//!   the order of insertion and unlinking, never on addresses, so links and
//!   call order are the same. `MAlloc`/`syCalloc` failure paths (which return 0
//!   and destroy the half-built task) are not reachable here.
//! - **Header word +0x48.** `GetTask` copies the block's previous +0x44 word
//!   (stale heap contents) into +0x48. That depends on the allocator's history,
//!   so it is kept as 0.
//! - **Work blocks** (+0x34, +0x38, +0x3C, +0x40) are zeroed byte buffers with
//!   the native sizes and big-endian layout, so ported routines can view them as
//!   they do in main.dol.
//! - **Callbacks** are native addresses. `no_op`, `DestroyTask` and `FreeTaskC`
//!   are built in; every other address goes to a [`Natives`] host. An address
//!   the host does not know fails closed: the call is recorded as
//!   [`Event::Unported`] and nothing runs.
//! - **Engine calls** the passes make (render-state selector, sort queue,
//!   collision release) are logged as [`Event::Engine`] and, where they return
//!   something, asked of a [`DisplayEngine`].
//! - **Use after free.** Where the native walk would read a task a callback
//!   just destroyed, this port reads it as gone (no children); where the native
//!   code would follow a dangling pointer into freed memory, the walk stops and
//!   [`Event::Anomaly`] is recorded instead of guessing.
use std::collections::BTreeMap;

/// `CreateElementalTask` 0x80071E0C.
pub const CREATE_ELEMENTAL_TASK: u32 = 0x8007_1E0C;
/// `CreateChildTask` 0x800709F8.
pub const CREATE_CHILD_TASK: u32 = 0x8007_09F8;
/// `FreeTask` 0x80071AE0.
pub const FREE_TASK: u32 = 0x8007_1AE0;
/// `DestroyTask` 0x80071AF0 (also stored as an exec by `FreeTask`).
pub const DESTROY_TASK: u32 = 0x8007_1AF0;
/// `FreeTaskC` 0x8007098C.
pub const FREE_TASK_C: u32 = 0x8007_098C;
/// `no_op` 0x80071F58, the default exec.
pub const NO_OP: u32 = 0x8007_1F58;
/// `fn_800067F8`: called by `DestroyTask` when twp +0x2C is set.
pub const TWP_COLLISION_RELEASE: u32 = 0x8000_67F8;
/// `fn_8012297C`: render-state selector of the display pass.
pub const RENDER_STATE: u32 = 0x8012_297C;
/// `fn_80032428`: whether the sorted group goes to the sort queue.
pub const SORT_QUEUE_ACTIVE: u32 = 0x8003_2428;
/// `fn_80032170(task, 0, f1)`: queue a task's +0x20 display.
pub const SORT_QUEUE_ADD: u32 = 0x8003_2170;
/// The float constant `fn_80070C54` passes in f1 to [`SORT_QUEUE_ADD`].
pub const SORT_QUEUE_F1: u32 = 0x803A_F524;
/// `fn_80030EA8`: flush the sort queue.
pub const SORT_QUEUE_FLUSH: u32 = 0x8003_0EA8;
/// `fn_800648C8`: called between the +0x1C and +0x24 groups.
pub const BETWEEN_GROUPS: u32 = 0x8006_48C8;
/// `btp`, the eight list heads.
pub const BTP: u32 = 0x801F_CEB8;
/// Number of task levels (`btp` entries).
pub const LEVELS: usize = 8;
/// Levels 0..=5 are executed and displayed; 6 is never walked; 7 is never linked.
pub const UNLINKED_LEVEL: u32 = 7;

/// A task, by creation order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskId(pub u32);

/// The eight callback words of the header, +0x10..+0x2C.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// +0x10
    Exec = 0,
    /// +0x14, first display group
    Display = 1,
    /// +0x18
    Destructor = 2,
    /// +0x1C, third display group
    DisplayDelayed = 3,
    /// +0x20, depth-sorted display group
    DisplaySorted = 4,
    /// +0x24, fourth display group
    DisplayLate = 5,
    /// +0x28, fifth display group
    DisplayLast = 6,
    /// +0x2C, pre-pass display (from fn_80070F5C)
    DisplayShadow = 7,
}

impl Slot {
    /// The header offset of this slot.
    pub fn offset(self) -> u32 {
        0x10 + 4 * self as u32
    }
    /// The slot at a header offset in +0x10..=+0x2C.
    pub fn from_offset(off: u32) -> Option<Slot> {
        const ALL: [Slot; 8] = [
            Slot::Exec,
            Slot::Display,
            Slot::Destructor,
            Slot::DisplayDelayed,
            Slot::DisplaySorted,
            Slot::DisplayLate,
            Slot::DisplayLast,
            Slot::DisplayShadow,
        ];
        if !(0x10..=0x2C).contains(&off) || !off.is_multiple_of(4) {
            return None;
        }
        ALL.get(((off - 0x10) / 4) as usize).copied()
    }
}

/// The four optional work blocks, by header offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkKind {
    /// +0x34, `syCalloc(1, 0x30)` for im bit 0x2.
    Twp,
    /// +0x38, `syCalloc(1, 0x40)` for im bit 0x1.
    W38,
    /// +0x3C, `syCalloc(2, 0x1C)` for im bit 0x4.
    W3C,
    /// +0x40, `syCalloc(1, 0x10)` for im bit 0x8 (player tasks put their
    /// player work here instead).
    W40,
}

impl WorkKind {
    /// Header offset.
    pub fn offset(self) -> u32 {
        match self {
            WorkKind::Twp => 0x34,
            WorkKind::W38 => 0x38,
            WorkKind::W3C => 0x3C,
            WorkKind::W40 => 0x40,
        }
    }
}

/// The task header (0x50 bytes natively).
#[derive(Debug, Clone, PartialEq)]
pub struct Task {
    /// +0x00
    pub next: Option<TaskId>,
    /// +0x04
    pub prev: Option<TaskId>,
    /// +0x08
    pub parent: Option<TaskId>,
    /// +0x0C, first child of the child ring
    pub child: Option<TaskId>,
    /// +0x10..+0x2C, native callback addresses (0 = none)
    pub slots: [u32; 8],
    /// +0x30, index into [`Runtime::set_flags`]
    pub set_entry: Option<usize>,
    /// +0x34
    pub twp: Option<Vec<u8>>,
    /// +0x38
    pub w38: Option<Vec<u8>>,
    /// +0x3C
    pub w3c: Option<Vec<u8>>,
    /// +0x40
    pub w40: Option<Vec<u8>>,
    /// +0x44, the name pointer (opaque)
    pub name: u32,
    /// +0x4C
    pub work: u32,
}

impl Task {
    /// The callback in a slot.
    pub fn slot(&self, s: Slot) -> u32 {
        self.slots[s as usize]
    }
    /// The work block at a header offset.
    pub fn work_block(&self, k: WorkKind) -> Option<&Vec<u8>> {
        match k {
            WorkKind::Twp => self.twp.as_ref(),
            WorkKind::W38 => self.w38.as_ref(),
            WorkKind::W3C => self.w3c.as_ref(),
            WorkKind::W40 => self.w40.as_ref(),
        }
    }
    /// Big-endian word in twp (0 without twp or out of range).
    pub fn twp_word(&self, off: usize) -> u32 {
        self.twp
            .as_ref()
            .and_then(|w| w.get(off..off + 4))
            .map_or(0, |b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
}

/// What the runtime did, in native order.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A callback was handed to the host.
    Call { addr: u32, task: TaskId },
    /// A callback the host does not know: not run.
    Unported { addr: u32, task: TaskId },
    /// `syFree` of a work block.
    FreeWork { task: TaskId, kind: WorkKind },
    /// `syFree` of the task itself.
    FreeTask { task: TaskId },
    /// An engine routine the pass calls (address, arguments).
    Engine { addr: u32, args: Vec<u32> },
    /// The native code would read freed or unlinked memory here.
    Anomaly(&'static str),
}

/// Errors from task creation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskError {
    /// `GetTask` indexes `btp` without a bound; levels above 7 would write past it.
    LevelOutOfRange(u32),
    /// The task does not exist (destroyed or never created).
    NoTask(TaskId),
}

/// The host that runs ported callbacks.
pub trait Natives {
    /// Run the routine at `addr` for `task`; false when it is not ported.
    fn run(&mut self, rt: &mut Runtime, addr: u32, task: TaskId) -> bool;
}

/// A ported routine.
pub type NativeFn = fn(&mut Runtime, &mut dyn Natives, TaskId);

/// A [`Natives`] host built from a table of ported routines.
#[derive(Default, Clone)]
pub struct Registry {
    map: BTreeMap<u32, NativeFn>,
}

impl Registry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }
    /// Register a ported routine at its native address.
    pub fn insert(&mut self, addr: u32, f: NativeFn) {
        self.map.insert(addr, f);
    }
    /// Whether an address is ported.
    pub fn contains(&self, addr: u32) -> bool {
        self.map.contains_key(&addr)
    }
}

impl Natives for Registry {
    fn run(&mut self, rt: &mut Runtime, addr: u32, task: TaskId) -> bool {
        match self.map.get(&addr).copied() {
            Some(f) => {
                f(rt, self, task);
                true
            }
            None => false,
        }
    }
}

/// What the display pass asks of the rest of the engine.
pub trait DisplayEngine {
    /// The byte G+0x3D (0x801CC1A5).
    fn screen_mode(&mut self) -> u8;
    /// The camera/viewport index 0x803ADAD0.
    fn camera_index(&mut self) -> i32;
    /// `fn_80032428()`.
    fn sort_queue_active(&mut self) -> bool;
    /// View-space Z of a twp position (`njCalcPoint` with the current matrix,
    /// third component).
    fn view_z(&mut self, pos: [f32; 3]) -> f32;
}

/// The task system state.
#[derive(Debug, Clone, Default)]
pub struct Runtime {
    tasks: Vec<Option<Task>>,
    /// `btp` 0x801FCEB8.
    pub btp: [Option<TaskId>; LEVELS],
    /// `makethread` 0x803ADA48: the callback being run (0 outside one).
    pub makethread: u32,
    /// SET entry flag halfwords (+0x2 of each entry); `DestroyTask` clears bit 0.
    pub set_flags: Vec<u16>,
    /// Everything that happened, in order.
    pub events: Vec<Event>,
}

impl Runtime {
    /// An empty runtime (`clearBtp` state).
    pub fn new() -> Self {
        Self::default()
    }

    /// The task, if it exists.
    pub fn task(&self, id: TaskId) -> Option<&Task> {
        self.tasks.get(id.0 as usize).and_then(Option::as_ref)
    }

    /// The task, mutably.
    pub fn task_mut(&mut self, id: TaskId) -> Option<&mut Task> {
        self.tasks.get_mut(id.0 as usize).and_then(Option::as_mut)
    }

    /// Ids of all live tasks, in creation order.
    pub fn live(&self) -> Vec<TaskId> {
        (0..self.tasks.len() as u32)
            .map(TaskId)
            .filter(|&t| self.task(t).is_some())
            .collect()
    }

    /// Tasks of a level in list order, starting at the head (bounded).
    pub fn level_order(&self, level: usize) -> Vec<TaskId> {
        let Some(head) = self.btp.get(level).copied().flatten() else {
            return vec![];
        };
        self.ring_from(head)
    }

    /// A parent's children in ring order.
    pub fn children(&self, parent: TaskId) -> Vec<TaskId> {
        match self.task(parent).and_then(|t| t.child) {
            Some(c) => self.ring_from(c),
            None => vec![],
        }
    }

    fn ring_from(&self, head: TaskId) -> Vec<TaskId> {
        let mut out = vec![head];
        let mut t = head;
        while out.len() <= self.tasks.len() {
            match self.task(t).and_then(|x| x.next) {
                Some(n) if n != head => {
                    out.push(n);
                    t = n;
                }
                _ => break,
            }
        }
        out
    }

    fn next_of(&self, t: TaskId) -> Option<TaskId> {
        self.task(t).and_then(|x| x.next)
    }

    fn child_of(&self, t: TaskId) -> Option<TaskId> {
        self.task(t).and_then(|x| x.child)
    }

    /// `clearBtp` 0x80071F2C: zero the eight heads (tasks are not freed).
    pub fn clear_btp(&mut self) {
        self.btp = [None; LEVELS];
    }

    /// `GetTask` 0x80071D04.
    fn get_task(&mut self, level: u32, exec: u32, name: u32) -> Result<TaskId, TaskError> {
        if level > UNLINKED_LEVEL {
            return Err(TaskError::LevelOutOfRange(level));
        }
        let id = TaskId(self.tasks.len() as u32);
        let mut slots = [0; 8];
        slots[Slot::Exec as usize] = if exec != 0 { exec } else { NO_OP };
        self.tasks.push(Some(Task {
            next: None,
            prev: None,
            parent: None,
            child: None,
            slots,
            set_entry: None,
            twp: None,
            w38: None,
            w3c: None,
            w40: None,
            name,
            work: 0,
        }));
        if level != UNLINKED_LEVEL {
            let l = level as usize;
            match self.btp[l] {
                None => {
                    self.set_links(id, Some(id), Some(id));
                    self.btp[l] = Some(id);
                }
                Some(head) => {
                    // Tail insert before the head.
                    let tail = self.task(head).and_then(|h| h.prev);
                    self.set_links(id, Some(head), tail);
                    if let Some(h) = self.task_mut(head) {
                        h.prev = Some(id);
                    }
                    if let Some(tail) = tail {
                        if let Some(t) = self.task_mut(tail) {
                            t.next = Some(id);
                        }
                    }
                }
            }
        }
        Ok(id)
    }

    fn set_links(&mut self, id: TaskId, next: Option<TaskId>, prev: Option<TaskId>) {
        if let Some(t) = self.task_mut(id) {
            t.next = next;
            t.prev = prev;
        }
    }

    /// `CreateElementalTask(im, level, exec, name)` 0x80071E0C.
    pub fn create_elemental_task(
        &mut self,
        im: u16,
        level: u32,
        exec: u32,
        name: u32,
    ) -> Result<TaskId, TaskError> {
        let id = self.get_task(level, exec, name)?;
        if let Some(t) = self.task_mut(id) {
            // Native order: +0x34, +0x3C, +0x38, +0x40.
            if im & 0x2 != 0 {
                t.twp = Some(vec![0; 0x30]);
            }
            if im & 0x4 != 0 {
                t.w3c = Some(vec![0; 2 * 0x1C]);
            }
            if im & 0x1 != 0 {
                t.w38 = Some(vec![0; 0x40]);
            }
            if im & 0x8 != 0 {
                t.w40 = Some(vec![0; 0x10]);
            }
        }
        Ok(id)
    }

    /// `CreateChildTask(im, exec, parent)` 0x800709F8.
    pub fn create_child_task(
        &mut self,
        im: u16,
        exec: u32,
        parent: TaskId,
    ) -> Result<TaskId, TaskError> {
        if self.task(parent).is_none() {
            return Err(TaskError::NoTask(parent));
        }
        let id = self.create_elemental_task(im, UNLINKED_LEVEL, exec, 0)?;
        match self.child_of(parent) {
            Some(first) => {
                let last = self.task(first).and_then(|c| c.prev);
                if let Some(c) = self.task_mut(first) {
                    c.prev = Some(id);
                }
                if let Some(last) = last {
                    if let Some(l) = self.task_mut(last) {
                        l.next = Some(id);
                    }
                }
                self.set_links(id, Some(first), last);
            }
            None => {
                if let Some(p) = self.task_mut(parent) {
                    p.child = Some(id);
                }
                self.set_links(id, Some(id), Some(id));
            }
        }
        if let Some(t) = self.task_mut(id) {
            t.parent = Some(parent);
        }
        // twp +0x08..+0x1F come from the parent's twp when both exist.
        let src = self
            .task(parent)
            .and_then(|p| p.twp.as_ref())
            .and_then(|w| w.get(0x08..0x20))
            .map(<[u8]>::to_vec);
        if let (Some(src), Some(dst)) = (src, self.task_mut(id).and_then(|t| t.twp.as_mut())) {
            if let Some(d) = dst.get_mut(0x08..0x20) {
                d.copy_from_slice(&src);
            }
        }
        Ok(id)
    }

    /// `FreeTask` 0x80071AE0: the next exec pass destroys the task.
    pub fn free_task(&mut self, id: TaskId) {
        if let Some(t) = self.task_mut(id) {
            t.slots[Slot::Exec as usize] = DESTROY_TASK;
        }
    }

    /// `FreeTaskC` 0x8007098C.
    pub fn free_task_c(&mut self, n: &mut dyn Natives, id: TaskId) {
        let Some(t) = self.task(id) else { return };
        if t.slot(Slot::Exec) == FREE_TASK_C {
            if t.child.is_none() {
                self.free_task(id);
            }
            return;
        }
        while let Some(c) = self.child_of(id) {
            self.destroy_task(n, c);
        }
    }

    /// `DestroyTask` 0x80071AF0.
    pub fn destroy_task(&mut self, n: &mut dyn Natives, id: TaskId) {
        let Some(dest) = self.task(id).map(|t| t.slot(Slot::Destructor)) else {
            self.events
                .push(Event::Anomaly("DestroyTask on a task that is gone"));
            return;
        };
        self.makethread = dest;
        if dest != 0 {
            self.dispatch(n, dest, id);
        }
        let Some(t) = self.task_mut(id) else {
            self.events
                .push(Event::Anomaly("task destroyed by its own destructor"));
            return;
        };
        t.slots = [0; 8];
        let (next, prev) = (t.next, t.prev);
        // The FreeTaskC test reads +0x10 after it was zeroed: never taken.
        while let Some(c) = self.child_of(id) {
            self.destroy_task(n, c);
        }
        if let Some(nx) = next.and_then(|x| self.task_mut(x)) {
            nx.prev = prev;
        }
        if let Some(pv) = prev.and_then(|x| self.task_mut(x)) {
            pv.next = next;
        }
        let parent = self.task(id).and_then(|t| t.parent);
        let replacement = if next == Some(id) { None } else { next };
        match parent {
            Some(p) => {
                if let Some(pt) = self.task_mut(p) {
                    if pt.child == Some(id) {
                        pt.child = replacement;
                    }
                }
            }
            None => {
                if let Some(l) = self.btp.iter().position(|&h| h == Some(id)) {
                    self.btp[l] = replacement;
                }
            }
        }
        let twp_collision = self.task(id).map_or(0, |t| t.twp_word(0x2C));
        if self.task(id).and_then(|t| t.twp.as_ref()).is_some() {
            if twp_collision != 0 {
                self.events.push(Event::Engine {
                    addr: TWP_COLLISION_RELEASE,
                    args: vec![id.0],
                });
            }
            self.free_work(id, WorkKind::Twp);
        }
        for k in [WorkKind::W3C, WorkKind::W38, WorkKind::W40] {
            if self.task(id).and_then(|t| t.work_block(k)).is_some() {
                self.free_work(id, k);
            }
        }
        if let Some(e) = self.task_mut(id).and_then(|t| t.set_entry.take()) {
            if let Some(f) = self.set_flags.get_mut(e) {
                *f &= !1;
            }
        }
        self.events.push(Event::FreeTask { task: id });
        if let Some(slot) = self.tasks.get_mut(id.0 as usize) {
            *slot = None;
        }
        self.makethread = 0;
    }

    fn free_work(&mut self, id: TaskId, kind: WorkKind) {
        if let Some(t) = self.task_mut(id) {
            match kind {
                WorkKind::Twp => t.twp = None,
                WorkKind::W38 => t.w38 = None,
                WorkKind::W3C => t.w3c = None,
                WorkKind::W40 => t.w40 = None,
            }
        }
        self.events.push(Event::FreeWork { task: id, kind });
    }

    /// Run one callback address (built-ins, then the host, else fail closed).
    fn dispatch(&mut self, n: &mut dyn Natives, addr: u32, task: TaskId) {
        match addr {
            0 | NO_OP => {}
            DESTROY_TASK => self.destroy_task(n, task),
            FREE_TASK_C => self.free_task_c(n, task),
            _ => {
                self.events.push(Event::Call { addr, task });
                if !n.run(self, addr, task) {
                    self.events.push(Event::Unported { addr, task });
                }
            }
        }
    }

    /// Call a task's exec directly, as `fn_8001E89C` does for one player
    /// task (no `makethread` update, no null check natively).
    pub fn run_exec(&mut self, n: &mut dyn Natives, task: TaskId) {
        match self.task(task).map(|t| t.slot(Slot::Exec)) {
            Some(0) | None => self
                .events
                .push(Event::Anomaly("direct exec of a missing task or null exec")),
            Some(exec) => self.dispatch(n, exec, task),
        }
    }

    /// `fn_80071A40(level)`: exec walk of one level.
    pub fn exec_level(&mut self, n: &mut dyn Natives, level: usize) {
        let Some(mut first) = self.btp.get(level).copied().flatten() else {
            return;
        };
        let mut t = first;
        loop {
            let Some(task) = self.task(t) else {
                self.events
                    .push(Event::Anomaly("exec walk reached a freed task"));
                return;
            };
            let (exec, next) = (task.slot(Slot::Exec), task.next);
            self.makethread = exec;
            if exec != 0 {
                self.dispatch(n, exec, t);
            }
            self.makethread = 0;
            if self.child_of(t).is_some() {
                self.loop_task_c(n, t);
            }
            let Some(next) = next else {
                self.events.push(Event::Anomaly("exec walk: unlinked task"));
                return;
            };
            if next == first {
                return;
            }
            match self.btp.get(level).copied().flatten() {
                Some(h) => first = h,
                None => return,
            }
            t = next;
        }
    }

    /// `LoopTaskC` 0x800708D8: exec walk of a child ring.
    pub fn loop_task_c(&mut self, n: &mut dyn Natives, parent: TaskId) {
        let Some(mut first) = self.child_of(parent) else {
            return;
        };
        let mut t = first;
        loop {
            let Some(task) = self.task(t) else {
                self.events
                    .push(Event::Anomaly("child exec walk reached a freed task"));
                return;
            };
            let (exec, next) = (task.slot(Slot::Exec), task.next);
            self.makethread = exec;
            if exec != 0 {
                self.dispatch(n, exec, t);
            }
            self.makethread = 0;
            if self.child_of(t).is_some() {
                self.loop_task_c(n, t);
            }
            let Some(next) = next else {
                self.events
                    .push(Event::Anomaly("child exec walk: unlinked task"));
                return;
            };
            if next == first {
                return;
            }
            match self.child_of(parent) {
                Some(c) => first = c,
                None => return,
            }
            t = next;
        }
    }

    /// `fn_80071214`: the exec pass, levels 0..=5.
    pub fn exec_pass(&mut self, n: &mut dyn Natives) {
        for level in 0..6 {
            self.exec_level(n, level);
        }
    }

    /// A display walk of one level for one slot (the template of fn_80071998,
    /// fn_80071858, fn_80071718, fn_800715D8, fn_80071498, fn_80071358).
    pub fn display_level(&mut self, n: &mut dyn Natives, level: usize, slot: Slot) {
        if let Some(head) = self.btp.get(level).copied().flatten() {
            self.display_ring(n, head, slot);
        }
    }

    /// The child walk of the same template (fn_80071900 and its siblings).
    pub fn display_children(&mut self, n: &mut dyn Natives, parent: TaskId, slot: Slot) {
        if let Some(c) = self.child_of(parent) {
            self.display_ring(n, c, slot);
        }
    }

    fn display_ring(&mut self, n: &mut dyn Natives, first: TaskId, slot: Slot) {
        let mut t = first;
        loop {
            let Some(task) = self.task(t) else {
                self.events
                    .push(Event::Anomaly("display walk reached a freed task"));
                return;
            };
            let (cb, next) = (task.slot(slot), task.next);
            self.makethread = cb;
            if cb != 0 {
                self.dispatch(n, cb, t);
            }
            self.makethread = 0;
            if self.child_of(t).is_some() {
                self.display_children(n, t, slot);
            }
            match next {
                Some(nx) if nx == first => return,
                Some(nx) => t = nx,
                None => {
                    self.events
                        .push(Event::Anomaly("display walk: unlinked task"));
                    return;
                }
            }
        }
    }

    /// `fn_800706B0(level)`: the depth-sorted +0x20 group of one level.
    pub fn display_sorted(&mut self, n: &mut dyn Natives, e: &mut dyn DisplayEngine, level: usize) {
        let Some(head) = self.btp.get(level).copied().flatten() else {
            return;
        };
        let mut ids = vec![];
        let mut z = vec![];
        for t in self.ring_from(head) {
            let Some(task) = self.task(t) else { continue };
            if task.slot(Slot::DisplaySorted) != 0 && task.twp.is_some() {
                let f = |o| f32::from_bits(task.twp_word(o));
                let pos = [f(0x14), f(0x18), f(0x1C)];
                ids.push(t);
                z.push(pos);
            }
        }
        if ids.is_empty() {
            return;
        }
        let mut z: Vec<f32> = z.into_iter().map(|p| e.view_z(p)).collect();
        cocktail_sort(&mut z, &mut ids);
        for t in ids {
            // Called without a null check and without setting makethread.
            let cb = self.task(t).map_or(0, |x| x.slot(Slot::DisplaySorted));
            if cb == 0 {
                self.events
                    .push(Event::Anomaly("sorted display: callback gone"));
                continue;
            }
            self.dispatch(n, cb, t);
            if self.child_of(t).is_some() {
                self.display_children(n, t, Slot::DisplaySorted);
            }
        }
    }

    /// Log an engine routine call (not run here).
    pub fn engine(&mut self, addr: u32, args: Vec<u32>) {
        self.events.push(Event::Engine { addr, args });
    }

    /// The levels 2..=5 viewport skip of `fn_80070C54`.
    fn skip_upper(e: &mut dyn DisplayEngine) -> bool {
        let mode = e.screen_mode();
        (mode == 1 && e.camera_index() == 0) || (mode == 2 && e.camera_index() == 1) || mode == 3
    }

    fn display_group(&mut self, n: &mut dyn Natives, e: &mut dyn DisplayEngine, slot: Slot) {
        self.display_level(n, 0, slot);
        self.display_level(n, 1, slot);
        if !Self::skip_upper(e) {
            for level in 2..6 {
                self.display_level(n, level, slot);
            }
        }
    }

    /// `fn_80070C54`: the display pass.
    pub fn display_pass(&mut self, n: &mut dyn Natives, e: &mut dyn DisplayEngine) {
        self.engine(RENDER_STATE, vec![3]);
        self.display_level(n, 0, Slot::Display);
        self.engine(RENDER_STATE, vec![1]);
        self.display_level(n, 1, Slot::Display);
        self.engine(RENDER_STATE, vec![3]);
        if !Self::skip_upper(e) {
            for level in 2..6 {
                self.display_level(n, level, Slot::Display);
            }
        }
        self.engine(RENDER_STATE, vec![2]);
        self.display_sorted(n, e, 0);
        self.display_level(n, 1, Slot::DisplaySorted);
        let active = e.sort_queue_active();
        self.engine(SORT_QUEUE_ACTIVE, vec![active as u32]);
        if active {
            if !Self::skip_upper(e) {
                for level in 2..6 {
                    let Some(head) = self.btp[level] else {
                        continue;
                    };
                    for t in self.ring_from(head) {
                        let queued = self
                            .task(t)
                            .is_some_and(|x| x.slot(Slot::DisplaySorted) != 0 && x.twp.is_some());
                        if queued {
                            self.engine(SORT_QUEUE_ADD, vec![t.0, 0, SORT_QUEUE_F1]);
                        }
                    }
                }
                self.engine(SORT_QUEUE_FLUSH, vec![]);
            }
        } else if !Self::skip_upper(e) {
            for level in 2..6 {
                self.display_sorted(n, e, level);
            }
        }
        self.display_group(n, e, Slot::DisplayDelayed);
        self.engine(BETWEEN_GROUPS, vec![]);
        self.display_group(n, e, Slot::DisplayLate);
        self.display_group(n, e, Slot::DisplayLast);
        self.engine(RENDER_STATE, vec![3]);
    }

    /// The +0x2C pre-pass `fn_80070F5C` runs: fn_80071358 for levels 0..=5.
    pub fn shadow_pass(&mut self, n: &mut dyn Natives) {
        for level in 0..6 {
            self.display_level(n, level, Slot::DisplayShadow);
        }
    }

    /// The level-purge template (fn_80070ABC, fn_80070B30, fn_80071250):
    /// destroy the head, then the saved next, while the head is non-empty.
    pub fn destroy_level(&mut self, n: &mut dyn Natives, level: usize) {
        let Some(mut t) = self.btp.get(level).copied().flatten() else {
            return;
        };
        loop {
            let next = self.next_of(t);
            self.destroy_task(n, t);
            if self.btp.get(level).copied().flatten().is_none() {
                return;
            }
            match next {
                Some(nx) => t = nx,
                None => return,
            }
        }
    }

    /// `fn_80070ABC`: destroy every task of levels 0..=7.
    pub fn destroy_all(&mut self, n: &mut dyn Natives) {
        for level in 0..LEVELS {
            self.destroy_level(n, level);
        }
    }

    /// `fn_80070B30`: destroy every task of levels 2..=5.
    pub fn destroy_stage_levels(&mut self, n: &mut dyn Natives) {
        for level in 2..6 {
            self.destroy_level(n, level);
        }
    }
}

/// The bidirectional bubble sort of `fn_800706B0` (ascending by `fcmpo`, so
/// NaN never swaps), keeping the task array in step.
fn cocktail_sort(z: &mut [f32], ids: &mut [TaskId]) {
    let n = z.len();
    if n == 0 {
        return;
    }
    let (mut lo, mut hi, mut last) = (0usize, n - 1, 0usize);
    while lo < hi {
        for i in lo..hi {
            if z[i] > z[i + 1] {
                z.swap(i, i + 1);
                ids.swap(i, i + 1);
                last = i;
            }
        }
        hi = last;
        let mut i = last;
        while i > lo {
            if z[i] < z[i - 1] {
                z.swap(i, i - 1);
                ids.swap(i, i - 1);
                last = i;
            }
            i -= 1;
        }
        lo = last;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct None_;
    impl Natives for None_ {
        fn run(&mut self, _: &mut Runtime, _: u32, _: TaskId) -> bool {
            false
        }
    }

    #[test]
    fn tail_insert_and_unlinked_level() {
        let mut rt = Runtime::new();
        let a = rt.create_elemental_task(0, 2, 0, 0).unwrap();
        let b = rt.create_elemental_task(0, 2, 0, 0).unwrap();
        let c = rt.create_elemental_task(0, 2, 0, 0).unwrap();
        let u = rt.create_elemental_task(0, 7, 0, 0).unwrap();
        assert_eq!(rt.level_order(2), vec![a, b, c]);
        assert_eq!(rt.task(a).unwrap().prev, Some(c));
        assert_eq!(rt.task(u).unwrap().next, None);
        assert_eq!(rt.task(a).unwrap().slot(Slot::Exec), NO_OP);
        assert_eq!(
            rt.create_elemental_task(0, 8, 0, 0),
            Err(TaskError::LevelOutOfRange(8))
        );
    }

    #[test]
    fn unported_callbacks_fail_closed() {
        let mut rt = Runtime::new();
        let a = rt.create_elemental_task(0, 0, 0x8100_0000, 0).unwrap();
        rt.exec_pass(&mut None_);
        assert_eq!(
            rt.events,
            vec![
                Event::Call {
                    addr: 0x8100_0000,
                    task: a
                },
                Event::Unported {
                    addr: 0x8100_0000,
                    task: a
                }
            ]
        );
    }

    #[test]
    fn free_task_destroys_on_next_exec() {
        let mut rt = Runtime::new();
        let a = rt.create_elemental_task(0xF, 3, 0, 0).unwrap();
        rt.free_task(a);
        rt.exec_pass(&mut None_);
        assert!(rt.task(a).is_none());
        assert_eq!(rt.btp[3], None);
        assert_eq!(
            rt.events,
            vec![
                Event::FreeWork {
                    task: a,
                    kind: WorkKind::Twp
                },
                Event::FreeWork {
                    task: a,
                    kind: WorkKind::W3C
                },
                Event::FreeWork {
                    task: a,
                    kind: WorkKind::W38
                },
                Event::FreeWork {
                    task: a,
                    kind: WorkKind::W40
                },
                Event::FreeTask { task: a },
            ]
        );
    }

    #[test]
    fn sort_matches_native_order() {
        let mut z = [3.0, f32::NAN, 1.0, 2.0, -5.0];
        let mut ids = [0, 1, 2, 3, 4].map(TaskId);
        cocktail_sort(&mut z, &mut ids);
        // NaN never compares, so it stays between the parts it split.
        assert_eq!(ids.map(|t| t.0), [0, 1, 4, 2, 3]);
    }
}
