//! The shared enemy library: main.dol's enemy helpers and the enemy logic
//! compiled into the stage RELs (GameCube SA2B, GSNE8P), ported from the
//! object code on the user's own disc.
//!
//! * `engine`: main.dol's helpers (work set-up, switch groups, noticing the
//!   player, turning, the leash back to home, `rand`, Ninja math the
//!   enemies call).
//! * `motion`: the motion sequencer at enemy work +0x1E0 (fn_80015C40).
//!
//! Field names follow what the code does with them; each field notes its
//! offset in the task's work (`twp`, task +0x34) or the enemy work (task
//! +0x38, 0x210 bytes from fn_80018D28).
pub mod e_ai;
pub mod e_gold;
pub mod engine;
pub mod host;
pub mod motion;

pub type V3 = [f32; 3];

/// MSL `rand`'s state (`next`, 0x803AD6A0). The game shares it between all
/// callers, so the host owns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rand(pub u32);

/// What an enemy reads of a player task.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerView {
    /// The player task's position (twp +0x14).
    pub pos: V3,
    /// The centre of the player's first collision entry (cwp +0xC -> +8).
    pub center: V3,
}

/// The parts of an enemy's task work (twp) the enemy code uses.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EnemyTask {
    /// +0: the state.
    pub mode: i8,
    /// +1: the kind (from Scale.x).
    pub kind: i8,
    /// +2.
    pub b2: u8,
    /// +3: shots fired in the current burst.
    pub shots: u8,
    /// +4: flags.
    pub flags: i16,
    /// +6: the state timer.
    pub timer: u16,
    /// +8, +0xC, +0x10: Rotation (BAMS).
    pub ang: [i32; 3],
    /// +0x14: Position.
    pub pos: V3,
    /// +0x20: Scale (the SET parameters).
    pub scl: V3,
}

/// The enemy work (task +0x38).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EnemyWork {
    /// +0, +4, +8: speed (forward, vertical, sideways in the E EMI family).
    pub spd: V3,
    /// +0xC: forward acceleration.
    pub accel: f32,
    /// +0x14: sideways acceleration.
    pub side_accel: f32,
    /// +0x18: frames until the next wander target.
    pub wander_timer: i32,
    /// +0x40: non-null (a pointer the game sets; kind 5 counts as noticing
    /// while it is set).
    pub link_40: bool,
    /// +0x44: passed as the player to fn_80015730 when the enemy becomes a
    /// carried object.
    pub b_44: i8,
    /// +0x45: the switch group (-1 for none).
    pub group: i8,
    /// +0x47: shots per burst (Rotation.x low byte for most kinds).
    pub burst: i8,
    /// +0x48: frames between bursts (Rotation.z, at least 60, for most kinds).
    pub interval: i16,
    /// +0x4C: flags.
    pub flags: i16,
    /// +0x54.
    pub f_54: f32,
    /// +0x58: home.
    pub home: V3,
    /// +0x64: the wander target.
    pub wander_target: V3,
    /// +0x70: offset from home of the point the notice range is centred on.
    pub home_offset: V3,
    /// +0x7C: the position the frame started from.
    pub last_pos: V3,
    /// +0x94: Scale.y for most kinds.
    pub f_94: f32,
    /// +0x98, +0xAC, +0xB0..+0xB8 (E GOLD: glow, spin and scale).
    pub f_98: f32,
    pub f_ac: f32,
    pub scale: V3,
    /// +0xC0, +0xC8, +0xCC, +0xD4..+0xE8, +0xEC, +0xF0.
    pub f_c0: f32,
    pub f_c8: f32,
    pub f_cc: f32,
    /// +0xD0: height above the ground probe the enemy keeps.
    pub hover: f32,
    pub f_d4: f32,
    pub f_d8: f32,
    pub f_dc: f32,
    pub f_e0: f32,
    pub f_e4: f32,
    pub i_e8: i32,
    pub f_ec: f32,
    pub f_f0: f32,
    /// +0xF4: the ground probe position (fn_800186B0).
    pub probe: V3,
    /// +0x150: the ground height under the probe (GetShadowPosXYZ's
    /// result at +0x140).
    pub shadow_y: f32,
    /// +0x1C0.
    pub f_1c0: f32,
    /// +0x1C4: the bobbing angle.
    pub bob: i32,
    /// +0x1CC: E GOLD's bobbing step count.
    pub i_1cc: i32,
    /// +0x1C8: turn rate (BAMS per frame).
    pub turn_rate: i32,
    /// +0x1D0: the yaw being turned toward.
    pub target_yaw: i32,
    /// +0x1D4, +0x1D8.
    pub i_1d4: i32,
    pub f_1d8: f32,
    /// +0x1DC: the notice range, squared.
    pub notice_range_sq: f32,
    /// +0x1E0: the motion sequencer.
    pub motion: motion::Motion,
}
