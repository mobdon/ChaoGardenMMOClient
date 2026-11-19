//! Local-space stepping for Wild Canyon type-zero paths (module 15, 1:0x21808).
use crate::{Error, Result};

/// Module 15, 1:0x217ac. The engine angle service takes horizontal X then Z.
pub fn segment_heading(from: [f32; 3], to: [f32; 3], angle: impl FnOnce(f32, f32) -> f64) -> i16 {
    // fsubs followed by fneg preserves the original signed zero at an axis.
    let radians = angle(-(from[0] - to[0]), -(from[2] - to[2]));
    // Native double from REL section 4 offset 0xb58, followed by fctiwz and extsh.
    (f64::from_bits(4666959776613632187) * radians) as i32 as i16
}

#[derive(Debug, PartialEq)]
pub struct PathStep {
    pub segment: usize,
    pub fraction: f32,
    pub position: [f32; 3],
    pub heading: i32,
    pub wrapped: bool,
}

/// The engine supplies segment lengths and signed binary-angle headings. Keeping those
/// services explicit avoids silently substituting host trig or distance approximations.
pub fn advance(
    points: &[[f32; 3]],
    mut segment: usize,
    mut fraction: f32,
    mut distance: f32,
    length: impl Fn(usize) -> f32,
    heading: impl Fn(usize) -> i16,
) -> Result<PathStep> {
    if points.len() < 2
        || segment >= points.len()
        || !(0.0..=1.0).contains(&fraction)
        || !distance.is_finite()
    {
        return Err(Error::invalid("invalid emerald path cursor"));
    }
    let mut wrapped = false;
    let forward = distance > 0.0;
    // Bound observation work even for degenerate paths or distances too large for f32
    // subtraction to make progress. Normal gameplay steps need only a few segments.
    for _ in 0..65536 {
        let len = length(segment);
        if !len.is_finite() || len < 0.0 {
            return Err(Error::invalid("invalid segment length"));
        }
        let next = (segment + 1) % points.len();
        let used = len * fraction;
        let remaining = (len - used).max(0.0);
        let (a, b) = if forward {
            if distance > remaining {
                distance -= remaining;
                wrapped |= next == 0;
                segment = next;
                fraction = 0.0;
                continue;
            }
            if len == 0.0 {
                return Err(Error::invalid("undefined native path division"));
            }
            let a = (remaining - distance) / len;
            if 1.0 - a < 0.0 {
                (1.0, 0.0)
            } else {
                (a, 1.0 - a)
            }
        } else {
            if -distance > used {
                distance += used;
                wrapped |= segment == 0;
                segment = if segment == 0 {
                    points.len() - 1
                } else {
                    segment - 1
                };
                fraction = 1.0;
                continue;
            }
            if len == 0.0 {
                return Err(Error::invalid("undefined native path division"));
            }
            let b = (used + distance) / len;
            if 1.0 - b < 0.0 {
                (0.0, 1.0)
            } else {
                (1.0 - b, b)
            }
        };
        let start = heading(segment);
        let delta = heading(next).wrapping_sub(start);
        return Ok(PathStep {
            segment,
            fraction: b,
            position: std::array::from_fn(|i| points[segment][i] * a + points[next][i] * b),
            heading: i32::from(start) + i32::from((f32::from(delta) * b) as i32 as i16),
            wrapped,
        });
    }
    Err(Error::invalid(
        "emerald path observation exceeded segment budget",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoints_and_signed_heading_seam() {
        let p = [[0.; 3], [10.; 3], [20.; 3]];
        let run = |s, f, d| {
            advance(
                &p,
                s,
                f,
                d,
                |i| [10., 10., 20.][i],
                |i| [32760, -32760, 0][i],
            )
            .unwrap()
        };
        let mid = run(0, 0., 5.);
        assert_eq!((mid.position, mid.heading), ([5.; 3], 32768));
        let end = run(2, 0., 20.);
        assert_eq!((end.segment, end.fraction, end.wrapped), (2, 1., false));
        let r = run(end.segment, end.fraction, 5.);
        assert_eq!((r.segment, r.fraction, r.wrapped), (0, 0.5, true));
        let back = run(0, 0., -5.);
        assert_eq!(
            (back.segment, back.fraction, back.position, back.wrapped),
            (2, 0.75, [5.; 3], true)
        );
    }
}
/// Task words at 0x38, 0x3c and 0x40. The allocation identity is opaque to Rust.
#[derive(Debug, Default, PartialEq)]
pub struct PathTaskWork {
    pub fraction: f32,
    pub segment: u32,
    pub allocation: Option<std::num::NonZeroU32>,
}

impl PathTaskWork {
    /// 1:0x220c8..0x22114. On success the caller can begin the first path step.
    /// With no SET entry the native initializer preserves the segment already in work.
    pub fn begin(
        &mut self,
        registration_word: i32,
        allocate: impl FnOnce(u32, u32) -> Option<std::num::NonZeroU32>,
        read_set_word: Option<impl FnOnce() -> u32>,
    ) -> bool {
        if registration_word < 0 {
            return false;
        }
        self.allocation = allocate(1, 24);
        if self.allocation.is_none() {
            return false;
        }
        if let Some(read) = read_set_word {
            self.segment = read() & 255;
        }
        true
    }

    /// 1:0x22298. Only the low segment byte is sent back to the SET service.
    pub fn unload(
        &mut self,
        write_set_byte: Option<impl FnOnce(u8)>,
        release: impl FnOnce(std::num::NonZeroU32),
    ) {
        if let Some(write) = write_set_byte {
            write(self.segment as u8);
        }
        if let Some(allocation) = self.allocation {
            release(allocation);
        }
        *self = Self::default();
    }
}
