use crate::{Error, Result};
use serde::{Deserialize, Serialize};

type Time = [i8; 3];
#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Clock {
    ticks: i32,
    display: Time,
    first: [Time; 3],
    second: [Time; 3],
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClockInput {
    clock: Clock,
    inhibit: bool,
    cumulative: bool,
    first_counter: i32,
    first_highest: i32,
    second_highest: Option<i32>,
}

/// Inputs observed at the shared timing pass, excluding the retained clock and
/// the primary kart counter already updated by the checkpoint callback.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TimingSignals {
    inhibit: bool,
    cumulative: bool,
    first_highest: i32,
    second_highest: Option<i32>,
}
impl TimingSignals {
    pub(crate) fn with_clock(self, clock: Clock, first_counter: i32) -> ClockInput {
        ClockInput {
            clock,
            first_counter,
            inhibit: self.inhibit,
            cumulative: self.cumulative,
            first_highest: self.first_highest,
            second_highest: self.second_highest,
        }
    }
}
fn subtract(a: Time, b: Time) -> Time {
    let value = |[m, s, c]: Time| i32::from(m) * 6000 + i32::from(s) * 100 + i32::from(c);
    let cs = value(a) - value(b);
    [(cs / 6000) as i8, (cs / 100 % 60) as i8, (cs % 100) as i8]
}

// Negative indices address bytes outside the native three-lap array.
fn lap(index: i32) -> Result<Option<usize>> {
    match index {
        0..=2 => Ok(Some(index as usize)),
        3.. => Ok(None),
        _ => Err(Error::invalid("negative kart lap index")),
    }
}

/// Shared timing pass, CartD 0x861c..0x8824 and split helper 0x81f0.
pub(super) fn advance(s: ClockInput, finished: bool) -> Result<Clock> {
    let mut c = s.clock;
    if s.inhibit || finished {
        return Ok(c);
    }
    c.ticks = c.ticks.wrapping_add(1).min(0x10000000);
    c.display = [
        (c.ticks / 3600) as i8,
        (c.ticks / 60 % 60) as i8,
        (c.ticks % 60 * 100 / 60) as i8,
    ];
    if s.cumulative {
        if let Some(i) = lap(s.first_counter)? {
            c.first[i] = c.display;
        }
    } else {
        if let Some(i) = lap(s.first_highest)? {
            c.first[i] = match i {
                0 => c.display,
                1 => subtract(c.display, c.first[0]),
                _ => subtract(subtract(c.display, c.first[0]), c.first[1]),
            };
        }
        if let Some(n) = s.second_highest {
            if let Some(i) = lap(n)? {
                c.second[i] = c.display;
            }
        }
    }
    Ok(c)
}
