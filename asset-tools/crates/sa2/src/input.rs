//! Dolphin SDK PADClamp as reconstructed for SA2B GSNE8P.
use serde::{Deserialize, Serialize};

#[derive(Default, Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PadStatus {
    pub button: u16,
    pub stick: [i8; 2],
    pub substick: [i8; 2],
    pub triggers: [u8; 2],
    pub analog_a: u8,
    pub analog_b: u8,
    pub err: i8,
}

pub fn pad_clamp(status: &mut [PadStatus; 4]) {
    for pad in status.iter_mut().filter(|p| p.err == 0) {
        pad.stick = clamp_stick(pad.stick, 72, 40);
        pad.substick = clamp_stick(pad.substick, 59, 31);
        pad.triggers = pad.triggers.map(|v| v.min(180).saturating_sub(30));
    }
}

fn clamp_stick(raw: [i8; 2], max: i32, xy: i32) -> [i8; 2] {
    let [x, y] = raw.map(|v| (i32::from(v).abs() - 15).max(0));
    let d = xy * x.max(y) + (max - xy) * x.min(y);
    let mut corrected = [x, y];
    if d > xy * max {
        corrected = corrected.map(|v| xy * max * v / d);
    }
    std::array::from_fn(|i| (corrected[i] * i32::from(raw[i]).signum()) as i8)
}
