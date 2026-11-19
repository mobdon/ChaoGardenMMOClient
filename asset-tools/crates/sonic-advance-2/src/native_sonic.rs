//! Native Sonic no-target airborne A and buffered Forward Thrust boundaries.
//!
//! The host retains collision, frame ordering, animation and effect tasks.
//! Pinned evidence and original-C parity scope are in docs/NATIVE-SONIC.md.

use crate::native_player::{
    ANIMATION_OVER, Callback, Character, FACING_LEFT, FLIP_WITH_MOVE_DIR, IGNORE_INPUT, IN_AIR,
    JUMP_HELD_STATE, LEFT, PlayerState, RIGHT, SPIN_ATTACK,
};
use crate::{Error, VerifiedRom, bounded_range};

pub const SOME_ATTACK: u32 = 0x2000_0000;
pub const INSTA_SHIELD_STATE: i16 = 18;
pub const FORWARD_THRUST_STATE: i16 = 80;
pub const SE_FORWARD_THRUST: u16 = 112;
pub const SE_INSTA_SHIELD: u16 = 113;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AirAction {
    None,
    /// The original returns FALSE: continue the current jump/uncurl callback.
    InstaShield,
    /// Run Player_Uncurl immediately instead of the remaining jumping body.
    ForwardThrust {
        stop_sound_281: bool,
    },
}

impl AirAction {
    pub fn sound(self) -> Option<u16> {
        match self {
            Self::None => None,
            Self::InstaShield => Some(SE_INSTA_SHIELD),
            Self::ForwardThrust { .. } => Some(SE_FORWARD_THRUST),
        }
    }

    pub fn returns_early(self) -> bool {
        matches!(self, Self::ForwardThrust { .. })
    }
}

/// Call in non-collect-rings normal-stage Jumping, before jump release, or in
/// Uncurl after its conditional air-input operation. The caller must establish
/// that there is no homing target and must dispatch unsupported attack-button
/// actions separately; this function never manufactures target information.
pub fn try_no_target_air_action(
    p: &mut PlayerState,
    jump_button: u16,
    inverted: bool,
) -> AirAction {
    if p.character != Character::Sonic || p.move_state & JUMP_HELD_STATE == 0 {
        return AirAction::None;
    }
    // Player_Jumping/Uncurl tests this before Player_TryMidAirAction's
    // SOME_ATTACK gate. A buffered thrust therefore wins over an A edge.
    if p.unk71 == 1 {
        let stop_sound_281 = initialize_forward_thrust(p, inverted);
        return AirAction::ForwardThrust { stop_sound_281 };
    }
    if p.move_state & SOME_ATTACK == 0 && p.frame_input & jump_button != 0 {
        p.move_state |= SOME_ATTACK;
        p.char_state = INSTA_SHIELD_STATE;
        return AirAction::InstaShield;
    }
    AirAction::None
}

/// Original initialization through the immediate Player_Uncurl call boundary.
/// Returns whether the transition also requests stopping native sound 281.
fn initialize_forward_thrust(p: &mut PlayerState, inverted: bool) -> bool {
    if p.move_state & FLIP_WITH_MOVE_DIR != 0 {
        if p.held_input & LEFT != 0 {
            p.move_state |= FACING_LEFT;
        }
        if p.held_input & RIGHT != 0 {
            p.move_state &= !FACING_LEFT;
        }
    }
    let stop_sound_281 = p.move_state & 0x20000 != 0;
    // Player_TransitionCancelFlyingAndBoost, followed by ForwardThrust's mask.
    p.move_state &= !0x310e_e730;
    p.move_state &= !0x0200_0000;
    p.unk61 = 0;
    p.unk62 = 0;
    p.unk63 = 0;
    p.unk70 = false;
    p.unk71 = 0;
    p.move_state |= IN_AIR;
    p.move_state &= !(0x1000000 | 0x20 | SPIN_ATTACK);
    p.change_sprite_offsets(6, 14, inverted);
    p.char_state = FORWARD_THRUST_STATE;
    p.q_speed_air_x = p
        .q_speed_air_x
        .wrapping_add(if p.move_state & FACING_LEFT != 0 {
            -576
        } else {
            576
        });
    p.q_speed_air_y = 0;
    p.rotation = 0;
    p.unk6e = 0;
    p.sprite_frame_flags &= !ANIMATION_OVER;
    p.callback = Callback::Uncurl;
    stop_sound_281
}

/// The leading Player_Uncurl condition; this callback has no jump-release cap.
pub fn uncurl_accepts_air_input(p: &PlayerState) -> bool {
    p.unk6e != 1 || p.q_speed_air_y > 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PatternStep {
    input: u8,
    mask: u8,
    frames: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Pattern {
    result: u8,
    steps: Vec<PatternStep>,
}

fn pattern(bytes: &[u8], offset: usize) -> Result<Pattern, Error> {
    let head = bounded_range(bytes, offset, 2)?;
    let count = usize::from(head[0]);
    if !(1..=16).contains(&count) {
        return Err(Error("Native input pattern must have 1..16 steps".into()));
    }
    let data = bounded_range(bytes, offset + 2, count * 3)?;
    Ok(Pattern {
        result: head[1],
        steps: data
            .as_chunks::<3>()
            .0
            .iter()
            .map(|b| PatternStep {
                input: b[0],
                mask: b[1],
                frames: b[2],
            })
            .collect(),
    })
}

fn pointer(bytes: &[u8], offset: usize) -> Result<usize, Error> {
    let raw = u32::from_le_bytes(bounded_range(bytes, offset, 4)?.try_into().unwrap());
    crate::tables::pointer_offset(raw, 0x0800_0000, bytes.len())
}

/// Sonic's actual ROM combo patterns plus the native global input history.
/// Only Sonic is represented; another character is rejected before mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SonicInputBuffer {
    patterns: Vec<Pattern>,
    recent: [u8; 4],
    recent_index: u8,
    counters: [[u8; 2]; 32],
    counter_index: u8,
}

impl SonicInputBuffer {
    pub fn from_rom(rom: &VerifiedRom<'_>) -> Result<Self, Error> {
        let bytes = rom.rom_bytes();
        let table = pointer(bytes, 0xc871d4)?;
        let mut patterns = Vec::new();
        for index in 0..8 {
            let offset = table + index * 4;
            let raw = u32::from_le_bytes(bounded_range(bytes, offset, 4)?.try_into().unwrap());
            if raw == u32::MAX {
                return Ok(Self::new(patterns));
            }
            patterns.push(pattern(
                bytes,
                crate::tables::pointer_offset(raw, 0x0800_0000, bytes.len())?,
            )?);
        }
        Err(Error("Unterminated Sonic input-pattern table".into()))
    }

    fn new(patterns: Vec<Pattern>) -> Self {
        Self {
            patterns,
            recent: [0; 4],
            recent_index: 0,
            counters: [[0; 2]; 32],
            counter_index: 0,
        }
    }

    /// Native InitNewInputCounters clears 124 of 128 bytes, preserving the last
    /// record and the four-frame input ring. Preserve that exact reset boundary.
    pub fn reset_counters(&mut self) {
        self.counter_index = 0;
        self.counters[..31].fill([0; 2]);
    }

    pub fn handle_frame(&mut self, p: &mut PlayerState) -> Result<(), Error> {
        if p.character != Character::Sonic {
            return Err(Error("Sonic input buffer requires Sonic".into()));
        }
        if p.move_state & (IGNORE_INPUT | 0x80) != 0 {
            self.new_frame_input(0, 0);
            return Ok(());
        }
        self.new_frame_input(p.frame_input, p.frame_input);
        if !p.unk70 {
            return Ok(());
        }
        for pattern in &self.patterns {
            if p.unk71 == pattern.result {
                break;
            }
            let mut cursor = self.counter_index;
            let mut remaining = pattern.steps.len();
            while remaining != 0 {
                remaining -= 1;
                let step = pattern.steps[remaining];
                let mut frames = i32::from(step.frames);
                let matched = loop {
                    let record = self.counters[usize::from(cursor)];
                    let duration = i32::from(record[1]);
                    if frames < duration {
                        break false;
                    }
                    cursor = cursor.wrapping_sub(1) & 31;
                    if step.input == record[0] & step.mask {
                        break true;
                    }
                    frames -= 1 + duration;
                };
                if !matched {
                    break;
                }
            }
            // The original tests the decremented count even after a failed
            // comparison on the earliest step; do not rewrite this as all().
            if remaining == 0 {
                p.unk71 = pattern.result;
                break;
            }
        }
        Ok(())
    }

    pub fn new_frame_input(&mut self, input1: u16, input2: u16) {
        let packed = (input1 & 0xf0) | (input2 % 4) | ((input2 & 0x100) >> 6);
        self.recent_index = (self.recent_index + 1) & 3;
        self.recent[usize::from(self.recent_index)] = packed as u8;
        let recent = self.recent[usize::from(self.recent_index)]
            | self.recent[usize::from(self.recent_index.wrapping_sub(1) & 3)]
            | self.recent[usize::from(self.recent_index.wrapping_sub(2) & 3)];
        let value = (u16::from(recent) & input1) as u8;
        let current = &mut self.counters[usize::from(self.counter_index)];
        if current[0] == value && current[1] != u8::MAX {
            current[1] += 1;
        } else {
            self.counter_index = (self.counter_index + 1) & 31;
            self.counters[usize::from(self.counter_index)] = [value, 0];
        }
    }

    /// Native state words for deterministic parity, without source pointers.
    pub fn state_bytes(&self) -> [u8; 70] {
        let mut bytes = [0; 70];
        for (record, out) in self.counters.iter().zip(bytes[..64].as_chunks_mut::<2>().0) {
            out.copy_from_slice(record);
        }
        bytes[64..68].copy_from_slice(&self.recent);
        bytes[68] = self.counter_index;
        bytes[69] = self.recent_index;
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_target_a_is_one_shot_shield_and_keeps_jump_callback() {
        let mut p = PlayerState::new_at(Character::Sonic, 0, 0);
        p.move_state = IN_AIR | JUMP_HELD_STATE | SPIN_ATTACK;
        p.frame_input = 1;
        p.callback = Callback::Airborne;
        assert_eq!(
            try_no_target_air_action(&mut p, 1, false),
            AirAction::InstaShield
        );
        assert_eq!(p.callback, Callback::Airborne);
        assert_eq!(p.char_state, INSTA_SHIELD_STATE);
        assert!(!AirAction::InstaShield.returns_early());
        assert_eq!(try_no_target_air_action(&mut p, 1, false), AirAction::None);
    }

    #[test]
    fn buffered_thrust_wins_and_right_overrides_left_before_cancel() {
        let mut p = PlayerState::new_at(Character::Sonic, 20 * 256, 40 * 256);
        p.move_state = JUMP_HELD_STATE
            | IN_AIR
            | FACING_LEFT
            | FLIP_WITH_MOVE_DIR
            | SPIN_ATTACK
            | SOME_ATTACK
            | 0x20000;
        p.held_input = LEFT | RIGHT;
        p.frame_input = 1;
        p.sprite_offset_y = 9;
        p.q_speed_air_x = 32700;
        p.unk71 = 1;
        assert_eq!(
            try_no_target_air_action(&mut p, 1, false),
            AirAction::ForwardThrust {
                stop_sound_281: true
            }
        );
        assert_eq!(p.q_speed_air_x, 32700_i16.wrapping_add(576));
        assert_eq!(p.q_world_y, 35 * 256);
        assert_eq!(p.q_speed_air_y, 0);
        assert_eq!(p.callback, Callback::Uncurl);
        assert_eq!(
            p.move_state & (JUMP_HELD_STATE | SPIN_ATTACK | SOME_ATTACK | FACING_LEFT),
            0
        );
        assert_eq!(p.char_state, FORWARD_THRUST_STATE);
    }

    #[test]
    fn uncurl_air_input_gate_and_no_jump_release_are_explicit() {
        let mut p = PlayerState::new_at(Character::Sonic, 0, 0);
        p.unk6e = 1;
        p.q_speed_air_y = 0;
        assert!(!uncurl_accepts_air_input(&p));
        p.q_speed_air_y = 1;
        assert!(uncurl_accepts_air_input(&p));
    }

    #[test]
    fn input_counter_saturation_rollover_reset_and_character_rejection() {
        let mut buffer = SonicInputBuffer::new(Vec::new());
        for _ in 0..256 {
            buffer.new_frame_input(0, 0);
        }
        assert_eq!(buffer.counter_index, 1);
        assert_eq!(buffer.counters[0], [0, 255]);
        buffer.counters[31] = [9, 7];
        buffer.new_frame_input(0x10, 0x10);
        let recent = buffer.recent;
        buffer.reset_counters();
        assert_eq!(buffer.counters[31], [9, 7]);
        assert_eq!(buffer.recent, recent);
        let before = buffer.clone();
        let mut other = PlayerState::new_at(Character::Cream, 0, 0);
        assert!(buffer.handle_frame(&mut other).is_err());
        assert_eq!(buffer, before);
    }

    #[test]
    fn malformed_patterns_are_bounded() {
        for bytes in [vec![], vec![1], vec![0, 1], vec![17, 1], vec![1, 1, 0, 0]] {
            assert!(pattern(&bytes, 0).is_err());
        }
        assert!(pattern(&[1, 7, 0, 0, 0], usize::MAX).is_err());
    }
}
