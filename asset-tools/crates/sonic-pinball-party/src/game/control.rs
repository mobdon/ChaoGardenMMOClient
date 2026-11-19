//! Native A3VE callback-stack bookkeeping and input edges/repeat.
//!
//! These are pure Rust translations of bounded helper routines, not an execution
//! engine for the callback addresses. No callback is executed or identified as a
//! completed game screen by storing its original address here.

use serde::Serialize;
use std::fmt;

pub const CALLBACK_CAPACITY: usize = 10;
pub const CALLBACK_HALFWORDS: usize = 32;
pub const INPUT_HALFWORDS: usize = 45;
pub const PER_KEY_MASKS: [u16; 10] = [0x40, 0x80, 0x20, 0x10, 1, 2, 0x200, 0x100, 8, 4];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlError {
    SnapshotLength { actual: usize, expected: usize },
    CallbackDepth(u16),
}

impl fmt::Display for ControlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ControlError {}

/// Original pointer, including its Thumb bit, used only as an opaque identifier.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct CallbackId(pub u32);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct CallbackFrame {
    pub callback: CallbackId,
    pub parameter: u16,
}

impl CallbackFrame {
    /// Native screen wrappers load this stored halfword with LDRSH.
    pub fn signed_parameter(self) -> i16 {
        self.parameter as i16
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CallbackStack {
    depth: u16,
    reserved: u16,
    slots: [CallbackFrame; CALLBACK_CAPACITY],
}

impl CallbackStack {
    /// Load the scalar RAM layout: count, reserved, ten u32 IDs, ten parameters.
    /// A corrupted depth is rejected rather than reproducing an out-of-bounds
    /// native memory access. Inactive slots are retained verbatim.
    pub fn from_halfwords(words: &[u16]) -> Result<Self, ControlError> {
        if words.len() != CALLBACK_HALFWORDS {
            return Err(ControlError::SnapshotLength {
                actual: words.len(),
                expected: CALLBACK_HALFWORDS,
            });
        }
        if usize::from(words[0]) > CALLBACK_CAPACITY {
            return Err(ControlError::CallbackDepth(words[0]));
        }
        let mut slots = [CallbackFrame::default(); CALLBACK_CAPACITY];
        for (index, slot) in slots.iter_mut().enumerate() {
            slot.callback = CallbackId(
                u32::from(words[2 + index * 2]) | (u32::from(words[3 + index * 2]) << 16),
            );
            slot.parameter = words[22 + index];
        }
        Ok(Self {
            depth: words[0],
            reserved: words[1],
            slots,
        })
    }

    pub fn to_halfwords(&self) -> [u16; CALLBACK_HALFWORDS] {
        let mut words = [0; CALLBACK_HALFWORDS];
        words[0] = self.depth;
        words[1] = self.reserved;
        for (index, slot) in self.slots.iter().enumerate() {
            words[2 + index * 2] = slot.callback.0 as u16;
            words[3 + index * 2] = (slot.callback.0 >> 16) as u16;
            words[22 + index] = slot.parameter;
        }
        words
    }

    pub fn len(&self) -> usize {
        usize::from(self.depth)
    }

    pub fn is_empty(&self) -> bool {
        self.depth == 0
    }

    /// Native 0x2EC0 only clears count; it does not zero callback storage.
    pub fn clear(&mut self) {
        self.depth = 0;
    }

    /// Native 0x2ECC truncates the parameter to u16 and ignores pushes at depth10.
    /// The bool is a Rust observation of the native branch, not its R0 return ABI.
    pub fn push(&mut self, callback: CallbackId, parameter: u32) -> bool {
        if self.len() == CALLBACK_CAPACITY {
            return false;
        }
        self.slots[self.len()] = CallbackFrame {
            callback,
            parameter: parameter as u16,
        };
        self.depth += 1;
        true
    }

    /// Native 0x2F00 decrements nonzero count and leaves the popped slot intact.
    pub fn pop(&mut self) -> Option<CallbackFrame> {
        let frame = self.top()?;
        self.depth -= 1;
        Some(frame)
    }

    /// Native 0x2F14 calls pop then push, so replace on an empty stack pushes.
    pub fn replace(&mut self, callback: CallbackId, parameter: u32) {
        self.pop();
        self.push(callback, parameter);
    }

    pub fn top(&self) -> Option<CallbackFrame> {
        self.len().checked_sub(1).map(|index| self.slots[index])
    }

    /// Native callback wrappers mutate the top frame's stored state halfword.
    pub fn top_mut(&mut self) -> Option<&mut CallbackFrame> {
        self.len()
            .checked_sub(1)
            .map(|index| &mut self.slots[index])
    }
}

/// Scalar layout of the native input block at IWRAM 0x03000510.
/// Halfwords 0/1 and repeated(+8) survive native initialization unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InputState {
    reserved: [u16; 2],
    pub held: u16,
    pub pressed: u16,
    pub repeated: u16,
    pub previous_repeat_held: u16,
    pub initial_delay: u16,
    pub minimum_delay: u16,
    pub acceleration: u16,
    pub directional_period: u16,
    pub directional_countdown: u16,
    pub button_period: u16,
    pub button_countdown: u16,
    pub per_key_period: [u16; 16],
    pub per_key_countdown: [u16; 16],
}

impl Default for InputState {
    fn default() -> Self {
        Self::from_halfwords(&[0; INPUT_HALFWORDS]).expect("fixed input layout")
    }
}

impl InputState {
    pub fn from_halfwords(words: &[u16]) -> Result<Self, ControlError> {
        if words.len() != INPUT_HALFWORDS {
            return Err(ControlError::SnapshotLength {
                actual: words.len(),
                expected: INPUT_HALFWORDS,
            });
        }
        let mut per_key_period = [0; 16];
        let mut per_key_countdown = [0; 16];
        per_key_period.copy_from_slice(&words[13..29]);
        per_key_countdown.copy_from_slice(&words[29..45]);
        Ok(Self {
            reserved: [words[0], words[1]],
            held: words[2],
            pressed: words[3],
            repeated: words[4],
            previous_repeat_held: words[5],
            initial_delay: words[6],
            minimum_delay: words[7],
            acceleration: words[8],
            directional_period: words[9],
            directional_countdown: words[10],
            button_period: words[11],
            button_countdown: words[12],
            per_key_period,
            per_key_countdown,
        })
    }

    pub fn to_halfwords(&self) -> [u16; INPUT_HALFWORDS] {
        let mut words = [0; INPUT_HALFWORDS];
        words[..13].copy_from_slice(&[
            self.reserved[0],
            self.reserved[1],
            self.held,
            self.pressed,
            self.repeated,
            self.previous_repeat_held,
            self.initial_delay,
            self.minimum_delay,
            self.acceleration,
            self.directional_period,
            self.directional_countdown,
            self.button_period,
            self.button_countdown,
        ]);
        words[13..29].copy_from_slice(&self.per_key_period);
        words[29..45].copy_from_slice(&self.per_key_countdown);
        words
    }

    /// Native 0x2C64: halfword truncation, including all16 per-key slots.
    pub fn initialize(&mut self, initial_delay: u32, minimum_delay: u32, acceleration: u32) {
        self.held = 0;
        self.pressed = 0;
        self.previous_repeat_held = 0;
        self.initial_delay = initial_delay as u16;
        self.minimum_delay = minimum_delay as u16;
        self.acceleration = acceleration as u16;
        self.directional_period = self.initial_delay;
        self.directional_countdown = 0;
        self.button_period = self.initial_delay;
        self.button_countdown = 0;
        self.per_key_period.fill(self.initial_delay);
        self.per_key_countdown.fill(self.initial_delay);
    }

    /// Native 0x2C9C reads active-low KEYINPUT and XORs 0x03FF. It does not
    /// additionally mask high input bits; tests preserve that exact arithmetic.
    pub fn poll_keyinput(&mut self, keyinput: u16) {
        let held = keyinput ^ 0x03ff;
        self.pressed = held & !self.held;
        self.held = held;
    }

    /// Native 0x2CC0: directions and buttons have separate accelerating clocks.
    /// Both clocks recover even if their requested mask is empty.
    pub fn repeat_grouped(&mut self, mask: u16) {
        repeat_clock(
            self.held,
            self.pressed,
            self.previous_repeat_held,
            mask & 0x00f0,
            self.initial_delay,
            self.minimum_delay,
            self.acceleration,
            &mut self.repeated,
            &mut self.directional_period,
            &mut self.directional_countdown,
        );
        repeat_clock(
            self.held,
            self.pressed,
            self.previous_repeat_held,
            mask & 0x030f,
            self.initial_delay,
            self.minimum_delay,
            self.acceleration,
            &mut self.repeated,
            &mut self.button_period,
            &mut self.button_countdown,
        );
        self.previous_repeat_held = self.held;
    }

    /// Native 0x2DCC: ten native key masks, with an independent clock per key.
    /// Unrequested key clocks remain unchanged. The trailing six initialized
    /// slots are retained but never visited by the terminated native mask table.
    pub fn repeat_per_key(&mut self, mask: u16) {
        for (index, key) in PER_KEY_MASKS.into_iter().enumerate() {
            if key & mask == 0 {
                continue;
            }
            repeat_clock(
                self.held,
                self.pressed,
                self.previous_repeat_held,
                key,
                self.initial_delay,
                self.minimum_delay,
                self.acceleration,
                &mut self.repeated,
                &mut self.per_key_period[index],
                &mut self.per_key_countdown[index],
            );
        }
        self.previous_repeat_held = self.held;
    }
}

#[allow(clippy::too_many_arguments)]
fn repeat_clock(
    held: u16,
    pressed: u16,
    previous: u16,
    mask: u16,
    maximum: u16,
    minimum: u16,
    acceleration: u16,
    repeated: &mut u16,
    period: &mut u16,
    countdown: &mut u16,
) {
    *repeated = (*repeated & !mask) | (pressed & mask);
    let active = held & mask;
    if active != 0 && active == previous & mask {
        *countdown = countdown.wrapping_sub(1);
        if (*countdown as i16) > 0 {
            return;
        }
        *period = period.wrapping_sub(acceleration);
        if (*period as i16) < minimum as i16 {
            *period = minimum;
        }
        *countdown = *period;
        *repeated = (*repeated & !mask) | active;
    } else {
        *period = period.wrapping_add(acceleration);
        if (*period as i16) > maximum as i16 {
            *period = maximum;
        }
        *countdown = *period;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn words(value: &Value) -> Vec<u16> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|word| u16::try_from(word.as_u64().unwrap()).unwrap())
            .collect()
    }

    #[test]
    fn native_unicorn_callback_vectors() {
        let fixture: Value = serde_json::from_str(include_str!("../../tests/control-vectors.json"))
            .expect("scalar native oracle fixtures");
        for case in fixture["stackCases"].as_array().unwrap() {
            let mut stack = CallbackStack::from_halfwords(&words(&case["initial"])).unwrap();
            for (operation, expected) in case["operations"]
                .as_array()
                .unwrap()
                .iter()
                .zip(case["snapshots"].as_array().unwrap())
            {
                match operation["kind"].as_str().unwrap() {
                    "clear" => stack.clear(),
                    "pop" => {
                        stack.pop();
                    }
                    "push" | "replace" => {
                        let callback = CallbackId(operation["callback"].as_u64().unwrap() as u32);
                        let parameter = operation["parameter"].as_u64().unwrap() as u32;
                        if operation["kind"] == "push" {
                            stack.push(callback, parameter);
                        } else {
                            stack.replace(callback, parameter);
                        }
                    }
                    _ => panic!("unknown fixture operation"),
                }
                assert_eq!(
                    stack.to_halfwords().as_slice(),
                    words(expected),
                    "{}",
                    case["name"]
                );
            }
        }
    }

    #[test]
    fn native_unicorn_input_vectors() {
        let fixture: Value = serde_json::from_str(include_str!("../../tests/control-vectors.json"))
            .expect("scalar native oracle fixtures");
        for case in fixture["inputCases"].as_array().unwrap() {
            let mut state = InputState::from_halfwords(&words(&case["initial"])).unwrap();
            for (operation, expected) in case["operations"]
                .as_array()
                .unwrap()
                .iter()
                .zip(case["snapshots"].as_array().unwrap())
            {
                match operation["kind"].as_str().unwrap() {
                    "initialize" => state.initialize(
                        operation["initialDelay"].as_u64().unwrap() as u32,
                        operation["minimumDelay"].as_u64().unwrap() as u32,
                        operation["acceleration"].as_u64().unwrap() as u32,
                    ),
                    "poll" => state.poll_keyinput(operation["raw"].as_u64().unwrap() as u16),
                    "group" => state.repeat_grouped(operation["mask"].as_u64().unwrap() as u16),
                    "per-key" => state.repeat_per_key(operation["mask"].as_u64().unwrap() as u16),
                    _ => panic!("unknown fixture operation"),
                }
                assert_eq!(
                    state.to_halfwords().as_slice(),
                    words(expected),
                    "{} operation {operation}",
                    case["name"]
                );
            }
        }
    }

    #[test]
    fn snapshots_reject_invalid_lengths_and_stack_depth() {
        assert!(CallbackStack::from_halfwords(&[]).is_err());
        let mut bad = [0; CALLBACK_HALFWORDS];
        bad[0] = 11;
        assert_eq!(
            CallbackStack::from_halfwords(&bad),
            Err(ControlError::CallbackDepth(11))
        );
        assert!(InputState::from_halfwords(&[]).is_err());
    }

    #[test]
    fn opaque_callback_ids_are_never_dispatched() {
        let mut stack = CallbackStack::default();
        stack.replace(CallbackId(0x0800_81e9), 0xffff_8000);
        assert_eq!(stack.top().unwrap().signed_parameter(), i16::MIN);
        assert_eq!(stack.top().unwrap().callback.0, 0x0800_81e9);
        assert_eq!(stack.len(), 1);
    }
}
