//! Executable native engine primitives, independent of the MMO authority.
//!
//! Format/behavior evidence: SAT-R/sa2 commit
//! 2dd3f17d30cd3c156e508aac0017620ae485b9ba, src/core.c GetInput,
//! include/core.h PSEUDO_RANDOM_32, src/game/math.c Random/SeedRng,
//! include/global.h Q/I arithmetic, and src/task.c/include/task.h.
//! Rust task handles replace raw IWRAM addresses; heap layout, interrupt/audio
//! execution, recorder storage and complete game callbacks are separate ports.

use serde::{Deserialize, Serialize};
use std::fmt;

pub const A_BUTTON: u16 = 0x001;
pub const B_BUTTON: u16 = 0x002;
pub const SELECT_BUTTON: u16 = 0x004;
pub const START_BUTTON: u16 = 0x008;
pub const DPAD_RIGHT: u16 = 0x010;
pub const DPAD_LEFT: u16 = 0x020;
pub const DPAD_UP: u16 = 0x040;
pub const DPAD_DOWN: u16 = 0x080;
pub const R_BUTTON: u16 = 0x100;
pub const L_BUTTON: u16 = 0x200;
pub const KEYS_MASK: u16 = 0x3ff;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputFrame {
    pub current: u16,
    pub physical: u16,
    pub pressed: u16,
    pub released: u16,
    pub repeated: u16,
    pub counters: [u8; 10],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputState {
    pub current: u16,
    /// Native gPrevInput is distinct from mutable per-frame gInput.
    pub previous: u16,
    pub physical: u16,
    pub pressed: u16,
    pub released: u16,
    pub repeated: u16,
    pub counters: [u8; 10],
    pub first_intervals: [u8; 10],
    pub continued_intervals: [u8; 10],
}

impl Default for InputState {
    fn default() -> Self {
        // Engine initialization sets intervals but leaves BSS counters zero.
        // Holding a key on the very first poll therefore differs from polling
        // an initial released frame. Preserve that native detail.
        Self {
            current: 0,
            previous: 0,
            physical: 0,
            pressed: 0,
            released: 0,
            repeated: 0,
            counters: [0; 10],
            first_intervals: [20; 10],
            continued_intervals: [8; 10],
        }
    }
}

impl InputState {
    pub fn frame(&self) -> InputFrame {
        InputFrame {
            current: self.current,
            physical: self.physical,
            pressed: self.pressed,
            released: self.released,
            repeated: self.repeated,
            counters: self.counters,
        }
    }

    /// Hardware KEYINPUT is active-low; playback replaces game input only
    /// after physical input is recorded. Native recorded words are u16, so
    /// playback's upper bits are retained rather than silently masked.
    pub fn poll_keyinput(&mut self, register: u16, playback: Option<u16>) -> InputFrame {
        self.physical = !register & KEYS_MASK;
        let next = playback.unwrap_or(self.physical);
        self.pressed = (next ^ self.previous) & next;
        self.released = (next ^ self.previous) & self.previous;
        self.current = next;
        self.previous = next;
        self.repeated = self.pressed;
        for bit in 0..10 {
            if next & (1 << bit) == 0 {
                self.counters[bit] = self.first_intervals[bit];
            } else if self.counters[bit] != 0 {
                self.counters[bit] -= 1;
            } else {
                self.repeated |= 1 << bit;
                self.counters[bit] = self.continued_intervals[bit];
            }
        }
        self.frame()
    }

    pub fn poll_held(&mut self, held: u16) -> InputFrame {
        self.poll_keyinput(!held, None)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeRng {
    pub previous: u32,
    pub current: u32,
}

impl NativeRng {
    pub const fn seeded(previous: u32, current: u32) -> Self {
        Self { previous, current }
    }

    pub fn next_u16(&mut self) -> u16 {
        let next = self
            .previous
            .wrapping_add(0x3711_9371)
            .wrapping_add(self.current);
        self.previous = self.current;
        self.current = next;
        (next >> 16) as u16
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PseudoRng {
    pub state: u32,
}

impl PseudoRng {
    pub const fn seeded(state: u32) -> Self {
        Self { state }
    }

    pub fn next_u32(&mut self) -> u32 {
        self.state = self
            .state
            .wrapping_mul(0x0019_6225)
            .wrapping_add(0x3c6e_f35f);
        self.state
    }
}

pub const fn q24_8_from_int(value: i32) -> i32 {
    value.wrapping_mul(256)
}

pub const fn q24_8_to_int(raw: i32) -> i32 {
    raw >> 8
}

pub const fn q24_8_fraction(raw: i32) -> u8 {
    raw as u8
}

/// Explicit 32-bit target arithmetic; this does not substitute an i64 product
/// or floating point calculation for the native ARM multiply and shift.
pub const fn q24_8_mul(left: i32, right: i32) -> i32 {
    left.wrapping_mul(right) >> 8
}

pub const fn q24_8_mul_neg(left: i32, right: i32) -> i32 {
    left.wrapping_mul(right).wrapping_neg() >> 8
}

pub fn q24_8_div(left: i32, right: i32) -> Result<i32, RuntimeError> {
    if right == 0 {
        return Err(RuntimeError::DivisionByZero);
    }
    Ok(left.wrapping_shl(8).wrapping_div(right))
}

pub const TASK_CAPACITY: usize = 128;
pub const GAME_TASK_CAPACITY: usize = TASK_CAPACITY - 2;
pub const TASK_INACTIVE: u16 = 0x0001;
pub const TASK_DESTROY_DISABLED: u16 = 0x0002;
pub const TASK_RUN_DURING_FLAGS_800: u16 = 0x0004;
pub const FLAGS_800: u32 = 0x0800;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    DivisionByZero,
    TaskCapacity,
    ReservedPriority,
    StaleTask,
    SentinelHasNoData,
    ReentrantExecution,
    ReentrantDestruction,
    DispatchBudgetExceeded,
    HandleGenerationExhausted,
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for RuntimeError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TaskId {
    pub slot: u8,
    pub generation: u64,
}

pub type TaskCallback<T> = fn(&mut TaskScheduler<T>, TaskId);

struct Task<T> {
    id: TaskId,
    parent: Option<TaskId>,
    previous: Option<TaskId>,
    next: Option<TaskId>,
    main: TaskCallback<T>,
    destructor: Option<TaskCallback<T>>,
    priority: u16,
    flags: u16,
    data: Option<T>,
    destroying: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TaskMetadata {
    pub id: TaskId,
    pub parent: Option<TaskId>,
    pub priority: u16,
    pub flags: u16,
}

/// Native priority/list dispatch with a safe host storage binding.
/// Two sentinels occupy slots 0/1. Equal priorities are FIFO. The next task
/// is captured before a callback; destruction updates that pending pointer.
/// The caller supplies a host dispatch budget to stop runaway callback code.
pub struct TaskScheduler<T> {
    tasks: [Option<Task<T>>; TASK_CAPACITY],
    allocation_slots: [usize; TASK_CAPACITY],
    generations: [u64; TASK_CAPACITY],
    allocated_count: usize,
    current: Option<TaskId>,
    pending: Option<TaskId>,
    executing: bool,
}

fn dummy<T>(_: &mut TaskScheduler<T>, _: TaskId) {}

impl<T> Default for TaskScheduler<T> {
    fn default() -> Self {
        let head = TaskId {
            slot: 0,
            generation: 1,
        };
        let tail = TaskId {
            slot: 1,
            generation: 1,
        };
        let mut tasks = std::array::from_fn(|_| None);
        for (id, previous, next, priority) in [
            (head, None, Some(tail), 0),
            (tail, Some(head), None, u16::MAX),
        ] {
            tasks[usize::from(id.slot)] = Some(Task {
                id,
                parent: None,
                previous,
                next,
                main: dummy,
                destructor: None,
                priority,
                flags: 0,
                data: None,
                destroying: false,
            });
        }
        let mut generations = [0; TASK_CAPACITY];
        generations[0] = 1;
        generations[1] = 1;
        Self {
            tasks,
            allocation_slots: std::array::from_fn(|index| index),
            generations,
            allocated_count: 2,
            current: None,
            pending: None,
            executing: false,
        }
    }
}

impl<T> TaskScheduler<T> {
    fn get(&self, id: TaskId) -> Result<&Task<T>, RuntimeError> {
        self.tasks
            .get(usize::from(id.slot))
            .and_then(Option::as_ref)
            .filter(|task| task.id == id)
            .ok_or(RuntimeError::StaleTask)
    }

    fn get_mut(&mut self, id: TaskId) -> Result<&mut Task<T>, RuntimeError> {
        self.tasks
            .get_mut(usize::from(id.slot))
            .and_then(Option::as_mut)
            .filter(|task| task.id == id)
            .ok_or(RuntimeError::StaleTask)
    }

    pub fn current_task(&self) -> Option<TaskId> {
        self.current
    }
    pub fn game_task_count(&self) -> usize {
        self.allocated_count - 2
    }

    pub fn metadata(&self, id: TaskId) -> Result<TaskMetadata, RuntimeError> {
        let task = self.get(id)?;
        Ok(TaskMetadata {
            id,
            parent: task.parent,
            priority: task.priority,
            flags: task.flags,
        })
    }

    pub fn data(&self, id: TaskId) -> Result<&T, RuntimeError> {
        self.get(id)?
            .data
            .as_ref()
            .ok_or(RuntimeError::SentinelHasNoData)
    }

    pub fn data_mut(&mut self, id: TaskId) -> Result<&mut T, RuntimeError> {
        self.get_mut(id)?
            .data
            .as_mut()
            .ok_or(RuntimeError::SentinelHasNoData)
    }

    pub fn set_flags(&mut self, id: TaskId, flags: u16) -> Result<(), RuntimeError> {
        self.get_mut(id)?.flags = flags;
        Ok(())
    }

    pub fn set_main(&mut self, id: TaskId, main: TaskCallback<T>) -> Result<(), RuntimeError> {
        self.get_mut(id)?.main = main;
        Ok(())
    }

    pub fn create(
        &mut self,
        priority: u16,
        flags: u16,
        data: T,
        main: TaskCallback<T>,
        destructor: Option<TaskCallback<T>>,
    ) -> Result<TaskId, RuntimeError> {
        // Retail TaskCreate cannot insert priority FFFF before its FFFF tail.
        // Reject this unsupported/orphan case instead of manufacturing a task.
        if priority == u16::MAX {
            return Err(RuntimeError::ReservedPriority);
        }
        if self.allocated_count == TASK_CAPACITY {
            return Err(RuntimeError::TaskCapacity);
        }
        let slot = self.allocation_slots[self.allocated_count];
        let generation = self.generations[slot]
            .checked_add(1)
            .ok_or(RuntimeError::HandleGenerationExhausted)?;
        let id = TaskId {
            slot: slot as u8,
            generation,
        };
        let mut previous = self.tasks[0].as_ref().expect("head sentinel").id;
        let mut next = self.get(previous)?.next.expect("tail sentinel");
        while self.get(next)?.priority <= priority {
            previous = next;
            next = self.get(next)?.next.expect("tail priority is greater");
        }
        self.generations[slot] = generation;
        self.allocated_count += 1;
        self.tasks[slot] = Some(Task {
            id,
            parent: self.current,
            previous: Some(previous),
            next: Some(next),
            main,
            destructor,
            priority,
            flags,
            data: Some(data),
            destroying: false,
        });
        self.get_mut(previous)?.next = Some(id);
        self.get_mut(next)?.previous = Some(id);
        Ok(id)
    }

    /// Returns false for protected tasks/sentinels. Destructors execute before
    /// unlinking, and may destroy other tasks or create new tasks.
    pub fn destroy(&mut self, id: TaskId) -> Result<bool, RuntimeError> {
        let task = self.get(id)?;
        if id.slot < 2 || task.flags & TASK_DESTROY_DISABLED != 0 {
            return Ok(false);
        }
        if task.destroying {
            return Err(RuntimeError::ReentrantDestruction);
        }
        let destructor = task.destructor;
        self.get_mut(id)?.destroying = true;
        if let Some(callback) = destructor {
            callback(self, id);
        }
        let task = self.get(id)?;
        let previous = task.previous.expect("game task has previous");
        let next = task.next.expect("game task has next");
        if self.pending == Some(id) {
            self.pending = Some(next);
        }
        self.get_mut(previous)?.next = Some(next);
        self.get_mut(next)?.previous = Some(previous);
        self.tasks[usize::from(id.slot)] = None;
        self.allocated_count -= 1;
        self.allocation_slots[self.allocated_count] = usize::from(id.slot);
        Ok(true)
    }

    /// Half-open native priority range [lower, upper); protected tasks remain.
    pub fn destroy_in_priority_range(
        &mut self,
        lower: u16,
        upper: u16,
    ) -> Result<usize, RuntimeError> {
        let mut cursor = self.tasks[0].as_ref().expect("head sentinel").next;
        let mut removed = 0;
        while let Some(id) = cursor {
            let task = self.get(id)?;
            if task.priority >= upper {
                break;
            }
            cursor = task.next;
            if task.priority >= lower && self.destroy(id)? {
                removed += 1;
            }
        }
        Ok(removed)
    }

    pub fn execute(
        &mut self,
        engine_flags: u32,
        max_dispatches: usize,
    ) -> Result<usize, RuntimeError> {
        if self.executing {
            return Err(RuntimeError::ReentrantExecution);
        }
        self.executing = true;
        self.current = self.tasks[0].as_ref().map(|task| task.id);
        let result = self.execute_inner(engine_flags, max_dispatches);
        self.current = None;
        self.pending = None;
        self.executing = false;
        result
    }

    fn execute_inner(
        &mut self,
        engine_flags: u32,
        max_dispatches: usize,
    ) -> Result<usize, RuntimeError> {
        let mut dispatched = 0;
        while let Some(id) = self.current {
            let (next, flags, callback) = {
                let task = self.get(id)?;
                (task.next, task.flags, task.main)
            };
            self.pending = next;
            if flags & TASK_INACTIVE == 0
                && (engine_flags & FLAGS_800 == 0 || flags & TASK_RUN_DURING_FLAGS_800 != 0)
            {
                if dispatched == max_dispatches {
                    return Err(RuntimeError::DispatchBudgetExceeded);
                }
                dispatched += 1;
                callback(self, id);
            }
            self.current = self.pending;
        }
        Ok(dispatched)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_edges_repeat_timing_and_physical_playback_are_native() {
        let mut input = InputState::default();
        input.poll_held(0);
        let pressed = input.poll_held(A_BUTTON);
        assert_eq!(
            (pressed.pressed, pressed.repeated, pressed.counters[0]),
            (1, 1, 19)
        );
        for _ in 0..19 {
            assert_eq!(input.poll_held(1).repeated, 0);
        }
        assert_eq!(input.poll_held(1).repeated, 1);
        for _ in 0..8 {
            assert_eq!(input.poll_held(1).repeated, 0);
        }
        assert_eq!(input.poll_held(1).repeated, 1);
        let playback = input.poll_keyinput(!DPAD_LEFT, Some(0x8002));
        assert_eq!(playback.physical, DPAD_LEFT);
        assert_eq!(playback.current, 0x8002);
        assert_eq!(playback.released, A_BUTTON);
        assert_eq!(playback.pressed, 0x8002);
        assert_eq!(input.poll_held(0).released, 0x8002);
        let mut first = InputState::default();
        assert_eq!(first.poll_held(1).counters[0], 8);
        first.current = 0;
        assert_eq!(
            first.poll_held(1).pressed,
            0,
            "native gPrevInput survives game-side gInput edits"
        );
    }

    #[test]
    fn fixed_point_keeps_target_wrapping_and_negative_rounding() {
        assert_eq!(q24_8_to_int(-1), -1);
        assert_eq!(q24_8_fraction(-1), 255);
        assert_eq!(q24_8_from_int(i32::MAX), -256);
        assert_eq!(q24_8_mul(-257, 129), -130);
        assert_eq!(q24_8_mul_neg(-257, 129), 129);
        assert_eq!(q24_8_div(-257, 129).unwrap(), -510);
        assert!(q24_8_div(1, 0).is_err());
    }

    fn idle(scheduler: &mut TaskScheduler<Vec<i32>>, id: TaskId) {
        scheduler.data_mut(id).unwrap().push(1);
    }

    #[test]
    fn scheduler_bounds_flags_reuse_and_half_open_destruction() {
        let mut scheduler = TaskScheduler::default();
        let a = scheduler.create(10, 0, vec![], idle, None).unwrap();
        let b = scheduler
            .create(10, TASK_INACTIVE, vec![], idle, None)
            .unwrap();
        let c = scheduler
            .create(
                20,
                TASK_DESTROY_DISABLED | TASK_RUN_DURING_FLAGS_800,
                vec![],
                idle,
                None,
            )
            .unwrap();
        scheduler.execute(0, 128).unwrap();
        assert_eq!(scheduler.data(a).unwrap(), &[1]);
        assert!(scheduler.data(b).unwrap().is_empty());
        scheduler.execute(FLAGS_800, 128).unwrap();
        assert_eq!(scheduler.data(a).unwrap(), &[1]);
        assert_eq!(scheduler.data(c).unwrap(), &[1, 1]);
        assert_eq!(scheduler.destroy_in_priority_range(10, 20).unwrap(), 2);
        assert!(!scheduler.destroy(c).unwrap());
        let replacement = scheduler.create(0, 0, vec![], idle, None).unwrap();
        assert_eq!(replacement.slot, b.slot);
        assert_ne!(replacement.generation, b.generation);
        assert!(scheduler.data(b).is_err());
        assert!(matches!(
            scheduler.create(u16::MAX, 0, vec![], idle, None),
            Err(RuntimeError::ReservedPriority)
        ));
        while scheduler.game_task_count() < GAME_TASK_CAPACITY {
            scheduler.create(1, 0, vec![], idle, None).unwrap();
        }
        assert!(matches!(
            scheduler.create(1, 0, vec![], idle, None),
            Err(RuntimeError::TaskCapacity)
        ));
        assert!(matches!(
            scheduler.execute(0, 0),
            Err(RuntimeError::DispatchBudgetExceeded)
        ));
        assert!(scheduler.current_task().is_none());
    }
}
