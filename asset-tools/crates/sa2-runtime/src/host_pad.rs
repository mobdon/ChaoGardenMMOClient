//! Host input to the GameCube pad: what `PADRead` would hand the game.
//!
//! The browser's Gamepad API (W3C "standard" layout) and the keyboard are
//! turned into a raw [`PadStatus`] here; [`crate::PadSystem::read`] then runs
//! the native path on it (`PADClamp`, the button table, the stick records),
//! so dead zones, stick limits and trigger ranges stay the game's own.
//!
//! The PAD button bits are the GameCube SDK's `PAD_BUTTON_*` values, the
//! same twelve bits the button table 0x801D51A4 converts. Face buttons
//! follow their names, as the Steam version labels them on an Xbox-style
//! pad (A, B, X, Y); Z is the right bumper. Host-side choices, not game
//! data: an axis of ±1.0 is the raw stick value ±127, a trigger of 1.0 is
//! the raw analog value 255, and the digital L/R bit (the GameCube trigger's
//! end click) is set from [`TRIGGER_CLICK`] up.
use sa2_asset_tools::input::PadStatus;

pub const PAD_LEFT: u16 = 0x0001;
pub const PAD_RIGHT: u16 = 0x0002;
pub const PAD_DOWN: u16 = 0x0004;
pub const PAD_UP: u16 = 0x0008;
pub const PAD_Z: u16 = 0x0010;
pub const PAD_R: u16 = 0x0020;
pub const PAD_L: u16 = 0x0040;
pub const PAD_A: u16 = 0x0100;
pub const PAD_B: u16 = 0x0200;
pub const PAD_X: u16 = 0x0400;
pub const PAD_Y: u16 = 0x0800;
pub const PAD_START: u16 = 0x1000;

/// Trigger travel (0..1) from which the digital L/R bit is set.
pub const TRIGGER_CLICK: f32 = 0.9;
/// A digital button counts as held from this value (the API's own
/// `pressed` threshold varies by browser; this matches the common one).
const PRESSED: f32 = 0.5;

/// Standard-layout button index → PAD bit (triggers 6 and 7 are analog,
/// handled apart). Index 8 (Back/Select), 10/11 (stick clicks) and 16
/// (Home) have no GameCube button.
const STANDARD_BUTTONS: [(usize, u16); 10] = [
    (0, PAD_A),
    (1, PAD_B),
    (2, PAD_X),
    (3, PAD_Y),
    (5, PAD_Z),
    (9, PAD_START),
    (12, PAD_UP),
    (13, PAD_DOWN),
    (14, PAD_LEFT),
    (15, PAD_RIGHT),
];

/// An axis in -1..1 to a raw stick value; `flip` for the API's
/// down-positive Y (the GameCube's Y is up-positive).
fn axis(v: f32, flip: bool) -> i8 {
    let v = if v.is_finite() {
        v.clamp(-1.0, 1.0)
    } else {
        0.0
    };
    let v = if flip { -v } else { v };
    (v * 127.0).round() as i8
}

fn trigger(v: f32) -> u8 {
    let v = if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    };
    (v * 255.0).round() as u8
}

/// A "standard" Gamepad: `axes` (LX, LY, RX, RY) and each button's value.
/// Missing entries read as released / centred.
pub fn from_standard_gamepad(axes: &[f32], buttons: &[f32]) -> PadStatus {
    let ax = |i: usize| axes.get(i).copied().unwrap_or(0.0);
    let bt = |i: usize| buttons.get(i).copied().unwrap_or(0.0);
    let mut button = STANDARD_BUTTONS
        .iter()
        .filter(|(i, _)| bt(*i) >= PRESSED)
        .fold(0, |acc, (_, b)| acc | b);
    let (l, r) = (bt(6), bt(7));
    if l >= TRIGGER_CLICK {
        button |= PAD_L;
    }
    if r >= TRIGGER_CLICK {
        button |= PAD_R;
    }
    PadStatus {
        button,
        stick: [axis(ax(0), false), axis(ax(1), true)],
        substick: [axis(ax(2), false), axis(ax(3), true)],
        triggers: [trigger(l), trigger(r)],
        ..PadStatus::default()
    }
}

/// Keyboard and pad together: buttons and triggers combine; each stick is
/// the keyboard's while a direction key is held, else the pad's.
pub fn merge(keys: PadStatus, pad: PadStatus) -> PadStatus {
    let pick = |k: [i8; 2], p: [i8; 2]| if k != [0, 0] { k } else { p };
    PadStatus {
        button: keys.button | pad.button,
        stick: pick(keys.stick, pad.stick),
        substick: pick(keys.substick, pad.substick),
        triggers: [
            keys.triggers[0].max(pad.triggers[0]),
            keys.triggers[1].max(pad.triggers[1]),
        ],
        ..PadStatus::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::convert_buttons;

    #[test]
    fn idle_pad_is_neutral() {
        assert_eq!(from_standard_gamepad(&[], &[]), PadStatus::default());
        assert_eq!(
            from_standard_gamepad(&[0.0; 4], &[0.0; 17]),
            PadStatus::default()
        );
    }

    #[test]
    fn sticks_scale_and_flip_y() {
        let p = from_standard_gamepad(&[1.0, -1.0, -0.5, 0.5], &[]);
        assert_eq!(p.stick, [127, 127]);
        assert_eq!(p.substick, [-64, -64]);
        let p = from_standard_gamepad(&[f32::NAN, 2.0], &[]);
        assert_eq!(p.stick, [0, -127]);
    }

    #[test]
    fn face_buttons_reach_the_game_table() {
        let mut b = [0.0; 17];
        b[0] = 1.0; // A
        b[1] = 1.0; // B
        b[9] = 1.0; // Start
        let p = from_standard_gamepad(&[], &b);
        assert_eq!(p.button, PAD_A | PAD_B | PAD_START);
        // The native table: A 0x100 → 0x4, B 0x200 → 0x2, Start 0x1000 → 0x8.
        assert_eq!(convert_buttons(p.button), 0x4 | 0x2 | 0x8);
    }

    #[test]
    fn triggers_are_analog_with_end_click() {
        let mut b = [0.0; 17];
        b[6] = 0.5;
        b[7] = 1.0;
        let p = from_standard_gamepad(&[], &b);
        assert_eq!(p.triggers, [128, 255]);
        assert_eq!(p.button, PAD_R);
    }

    #[test]
    fn keyboard_stick_wins_while_held() {
        let pad = from_standard_gamepad(&[0.3, 0.0], &[1.0]);
        let keys = PadStatus {
            stick: [0, 127],
            button: PAD_B,
            ..PadStatus::default()
        };
        let m = merge(keys, pad);
        assert_eq!(m.stick, [0, 127]);
        assert_eq!(m.button, PAD_A | PAD_B);
        let m = merge(PadStatus::default(), pad);
        assert_eq!(m.stick, pad.stick);
    }

    #[test]
    fn full_tilt_reaches_the_native_stick_limit() {
        let mut sys = crate::PadSystem::default();
        let mut ev = Vec::new();
        let up = from_standard_gamepad(&[0.0, -1.0], &[]);
        sys.read(
            [
                up,
                PadStatus::default(),
                PadStatus::default(),
                PadStatus::default(),
            ],
            &mut ev,
        );
        // PADClamp limits the main stick to 72; fn_80119C4C scales it to 128.
        assert_eq!(sys.cur[0].stick, [0, 72]);
        assert_eq!(&sys.records[0][0x1C..0x20], &[0, 0, 0xFF, 0x80]);
    }
}
