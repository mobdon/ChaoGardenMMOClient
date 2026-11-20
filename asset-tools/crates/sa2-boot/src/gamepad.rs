//! Gamepads (gilrs: XInput, PlayStation and others) as a GameCube controller.
//! A thread of its own polls the first connected pad; [`state`] reads the
//! latest. Mapping: left stick = stick, right stick = C-stick, South (A/Cross)
//! = A, West (X/Square) = B, East (B/Circle) = X, North (Y/Triangle) = Y,
//! Start, LT/RT = L/R (analog), RB = Z, D-pad = D-pad.
use sa2_asset_tools::input::PadStatus;
use std::sync::Mutex;

static STATE: Mutex<Option<PadStatus>> = Mutex::new(None);
static STARTED: std::sync::Once = std::sync::Once::new();

/// The connected gamepad as a controller (None when there is none).
pub fn state() -> Option<PadStatus> {
    STARTED.call_once(|| {
        let _ = std::thread::Builder::new().name("gamepad".into()).spawn(poll);
    });
    *STATE.lock().unwrap_or_else(|e| e.into_inner())
}

fn poll() {
    use gilrs::{Axis, Button, Gilrs};
    let Ok(mut g) = Gilrs::new() else {
        eprintln!("gamepad: no gamepad support on this system");
        return;
    };
    loop {
        while g.next_event().is_some() {}
        let pad = g.gamepads().find(|(_, p)| p.is_connected()).map(|(_, p)| {
            let mut s = PadStatus::default();
            let stick = |a: Axis| (p.value(a).clamp(-1.0, 1.0) * 100.0).round() as i8;
            s.stick = [stick(Axis::LeftStickX), stick(Axis::LeftStickY)];
            s.substick = [stick(Axis::RightStickX), stick(Axis::RightStickY)];
            let trig = |b: Button| p.button_data(b).map_or(0.0, |d| d.value()).clamp(0.0, 1.0);
            s.triggers = [(trig(Button::LeftTrigger2) * 255.0) as u8, (trig(Button::RightTrigger2) * 255.0) as u8];
            for (b, bit) in [
                (Button::South, 0x0100),
                (Button::West, 0x0200),
                (Button::East, 0x0400),
                (Button::North, 0x0800),
                (Button::Start, 0x1000),
                (Button::RightTrigger, 0x0010),
                (Button::DPadLeft, 0x0001),
                (Button::DPadRight, 0x0002),
                (Button::DPadDown, 0x0004),
                (Button::DPadUp, 0x0008),
            ] {
                if p.is_pressed(b) {
                    s.button |= bit;
                }
            }
            if s.triggers[0] > 200 {
                s.button |= 0x0040; // L (fully pressed)
            }
            if s.triggers[1] > 200 {
                s.button |= 0x0020; // R
            }
            s
        });
        *STATE.lock().unwrap_or_else(|e| e.into_inner()) = pad;
        std::thread::sleep(std::time::Duration::from_millis(4));
    }
}
