//! KNUDAI, the Mystic Melody shrine (stg09D.rel 0x2979C set-up, 0x29AA0
//! exec, 0x2A048 draw, 0x29980 delete).
//!
//! Set-up registers the shrine's model as a dynamic land object (a copy of
//! the COL entry at sec5+0x62D98 at the placement, yaw only) and files the
//! task in the shrine table (8 slots at sec5+0x62DD0, index Rotation.x mod
//! 8).
//!
//! Exec, when a player's collision centre is within 38 (0x800398D8):
//! * The hint pair for that player's character comes from the loaded
//!   message table (`hints`: entries +0x70 + 8 * character, when the file
//!   has at least 0x72 entries); without it the module's own placeholder
//!   texts are used (sec5+0x62DF8, +0x62E10, +0x62E38).
//! * If the shrine's melody bit (1 << (Rotation.x mod 8) in the word at
//!   GSNE8P 0x803ADC14) is clear, by the player's task Action: 0x2D plays
//!   sound 0x101E, sets the bit and restarts the shrine's counter (twp +3);
//!   0x2E plays 0x101E and then, like 0x2F, once the counter is above 10 and
//!   player 1's motion pair (+0x178, +0x17A of its 0x801E7728 entry) is
//!   (0x53, 0x52), shows hint 0 for 600 frames (placeholder 0x62E10 for
//!   200) and restarts the counter; 0x30 does the same without the motion
//!   test; any other action asks for 0x2D (byte +0xD) unless a message is
//!   showing (0x800E018C).
//! * If the bit is already set: 0x2E / 0x2F play 0x101E while player 1's
//!   motion is 0x52, and with motion 0x53 and the counter above 10 show
//!   hint 1 (placeholder 0x62E38) and restart the counter; 0x30 does
//!   nothing; any other action asks for 0x2E unless a message is showing.
//! * The counter counts up to 30. While its land object is registered, the
//!   player turns toward the shrine by a third of the gap,
//!   `yaw += trunc(0.33333334 * (s16)(0x4000 - a - yaw))` with
//!   `a = trunc(10430.38 * atan2f(dx, dz))`, also written to +0x1C of the
//!   player's 0x801E7748 entry.
//!
//! Messages (0x800E0118), sounds and requests are reported as events. The
//! tail's player-near flag (Status 0x100 within 32 + the model radius) is
//! not modelled.
use crate::msl_math::atan2f;
use crate::play_world::V3;
use crate::player_range::player_in_range;
use serde::Serialize;

const RANGE: f32 = 38.0; // sec4+0xD84
const BAMS_PER_RADIAN: f64 = 10430.38043493439; // sec4+0xD88
const THIRD: f32 = f32::from_bits(0x3eaa_aaab); // sec4+0xD90
pub const SHRINE_SOUND: u16 = 0x101e;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum HintText {
    /// Entry `index` (0 or 1) of the character's hint pair in the loaded
    /// message table.
    Loaded { character: i8, index: u8 },
    /// The module's placeholder string at this section-5 offset.
    Placeholder { offset: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum KnudaiEvent {
    Sound {
        sound: u16,
    },
    RequestAction {
        slot: u8,
        action: u8,
    },
    /// 0x800E0118(4, text, frames, ...).
    Message {
        text: HintText,
        frames: u32,
    },
    /// The player's new yaw (twp +0xC).
    TurnPlayer {
        slot: u8,
        yaw: i32,
    },
}

/// A player as the shrine reads it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KnudaiPlayer {
    pub centre: V3,
    pub position: V3,
    pub yaw: i32,
    pub action: u8,
    pub character: i8,
}

pub struct KnudaiInputs {
    pub players: [Option<KnudaiPlayer>; 2],
    /// Player 1's motion pair (+0x178, +0x17A of its 0x801E7728 entry).
    pub motion: (i16, i16),
    /// 0x800E018C: a message is showing.
    pub message_showing: bool,
    /// The message table has the hint entries (at least 0x72).
    pub hints_loaded: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Knudai {
    pub layer: usize,
    pub index: usize,
    pub position: V3,
    pub rotation: [i32; 3],
    /// twp +3.
    pub counter: u8,
    /// Its dynamic land object is registered (the work word at +0x38).
    pub registered: bool,
}

impl Knudai {
    pub fn new(layer: usize, index: usize, position: V3, rotation: [u16; 3]) -> Self {
        Self {
            layer,
            index,
            position,
            rotation: rotation.map(|r| i32::from(r as i16)),
            counter: 0,
            registered: true,
        }
    }

    /// The shrine's melody bit: 1 << (Rotation.x mod 8), C remainder.
    pub fn melody_bit(&self) -> u32 {
        let k = (self.rotation[0] & 0xf) % 8;
        1u32.checked_shl(k as u32).unwrap_or(0)
    }

    /// One frame; `melody` is the word at 0x803ADC14.
    pub fn step(&mut self, input: &KnudaiInputs, melody: &mut u32) -> Vec<KnudaiEvent> {
        let mut out = vec![];
        let centres = input.players.map(|p| p.map(|p| p.centre));
        let slot = player_in_range(&centres, self.position, RANGE);
        if slot != 0 {
            let s = (slot - 1) as usize;
            let Some(pl) = input.players[s] else {
                return out;
            };
            let hint = |index: u8, placeholder: u32| -> (HintText, u32) {
                if input.hints_loaded && pl.character >= 0 {
                    (
                        HintText::Loaded {
                            character: pl.character,
                            index,
                        },
                        600,
                    )
                } else if input.hints_loaded {
                    (
                        HintText::Loaded {
                            character: 0,
                            index,
                        },
                        600,
                    )
                } else {
                    (
                        HintText::Placeholder {
                            offset: placeholder,
                        },
                        200,
                    )
                }
            };
            let bit = self.melody_bit();
            let show = |out: &mut Vec<KnudaiEvent>, (text, frames): (HintText, u32)| {
                out.push(KnudaiEvent::Message { text, frames });
            };
            if *melody & bit == 0 {
                match pl.action {
                    0x2d => {
                        out.push(KnudaiEvent::Sound {
                            sound: SHRINE_SOUND,
                        });
                        *melody |= bit;
                        self.counter = 0;
                    }
                    0x2e | 0x2f => {
                        if pl.action == 0x2e {
                            out.push(KnudaiEvent::Sound {
                                sound: SHRINE_SOUND,
                            });
                        }
                        if self.counter > 10 && input.motion == (0x53, 0x52) {
                            show(&mut out, hint(0, 0x62e10));
                        }
                        self.counter = 0;
                    }
                    0x30 => {
                        if self.counter > 10 {
                            show(&mut out, hint(0, 0x62e10));
                        }
                        self.counter = 0;
                    }
                    _ => {
                        if !input.message_showing {
                            out.push(KnudaiEvent::RequestAction {
                                slot: s as u8,
                                action: 0x2d,
                            });
                        }
                    }
                }
            } else {
                match pl.action {
                    0x2e | 0x2f => {
                        if input.motion.0 == 0x52 {
                            out.push(KnudaiEvent::Sound {
                                sound: SHRINE_SOUND,
                            });
                        }
                        if input.motion.0 == 0x53 {
                            if self.counter > 10 {
                                show(&mut out, hint(1, 0x62e38));
                            }
                            self.counter = 0;
                        }
                    }
                    0x30 => {}
                    _ => {
                        if !input.message_showing {
                            out.push(KnudaiEvent::RequestAction {
                                slot: s as u8,
                                action: 0x2e,
                            });
                        }
                    }
                }
            }
            if self.counter < 30 {
                self.counter += 1;
            }
            if self.registered {
                let dx = self.position[0] - pl.position[0];
                let dz = self.position[2] - pl.position[2];
                let a = (BAMS_PER_RADIAN * f64::from(atan2f(dx, dz))) as i32;
                let d = (0x4000i32.wrapping_sub(a).wrapping_sub(pl.yaw)) as i16;
                let step = (f32::from(d) * THIRD) as i32 as i16;
                out.push(KnudaiEvent::TurnPlayer {
                    slot: s as u8,
                    yaw: pl.yaw.wrapping_add(i32::from(step)),
                });
            }
        }
        out
    }
}
