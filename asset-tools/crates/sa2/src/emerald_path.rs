//! Local-space stepping for Wild Canyon type-zero paths (module 15, 1:0x21808).
use crate::{Error, Result};

/// The eight mounted alternate placements use two zero rotation words. Compose
/// their translations first, as the stage does before submitting a local point.
pub fn unrotated_path_translation(
    origin: [f32; 3],
    rotations: [u16; 2],
    center: [f32; 3],
) -> Option<[f32; 3]> {
    (rotations == [0, 0]).then(|| std::array::from_fn(|i| origin[i] - center[i]))
}
pub fn translate_path_point(translation: [f32; 3], point: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| point[i] + translation[i])
}

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
    /// Apply one step to a live unrotated placement. Keep the restored segment
    /// byte until stepping, when the native routine reduces it by the path count.
    pub fn step_placed(
        &mut self,
        points: &[[f32; 3]],
        translation: [f32; 3],
        distance: f32,
        length: impl Fn(usize) -> f32,
        angle: impl Fn(usize, f32, f32) -> f64,
    ) -> Result<(PathStep, [f32; 3])> {
        if self.allocation.is_none() || points.len() < 2 || self.segment > i32::MAX as u32 {
            return Err(Error::invalid("invalid live emerald path work"));
        }
        let step = advance(
            points,
            self.segment as usize % points.len(),
            self.fraction,
            distance,
            length,
            |i| {
                segment_heading(points[i], points[(i + 1) % points.len()], |x, z| {
                    angle(i, x, z)
                })
            },
        )?;
        let world = translate_path_point(translation, step.position);
        self.segment = step.segment as u32;
        self.fraction = step.fraction;
        Ok((step, world))
    }

    /// Step using the same host njDistanceP2P evaluation as City Escape's board
    /// trigger. Angle calculation remains an explicit engine service.
    pub fn step_placed_geometry(
        &mut self,
        points: &[[f32; 3]],
        translation: [f32; 3],
        distance: f32,
        angle: impl Fn(usize, f32, f32) -> f64,
    ) -> Result<(PathStep, [f32; 3])> {
        self.step_placed(
            points,
            translation,
            distance,
            |i| crate::board::point_distance(points[i], points[(i + 1) % points.len()]),
            angle,
        )
    }

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
/// Module 15, 1:0x1fac4: only these piece types inspect the player task.
pub(crate) fn checks_player_action(kind: u64) -> bool {
    matches!(kind, 4..=6)
}
pub(crate) fn needs_mode_query(action: Option<i8>) -> bool {
    action.is_some_and(|v| !matches!(v, 0x3e | 0x42))
}
pub(crate) fn mode_allows_pickup(mode: i32) -> bool {
    matches!(mode, 8 | 16 | 18 | 41 | 46)
}

/// Stage-owned decisions in module 15, 1:0x22300. Services operate on the same
/// task and may update its cursor, transformed position, or engine collision data.
pub trait MovingEmeraldServices {
    fn mode_zero_gate(&mut self) -> i32;
    fn advance_and_transform(&mut self);
    fn selection(&mut self) -> i32;
    fn collision_flags(&mut self) -> u16;
    fn hit_count(&mut self) -> i16;
    fn game_mode(&mut self) -> i32;
    fn engine_byte(&mut self) -> i8;
    fn owner_id(&mut self) -> u16;
    fn collector(&mut self) -> i32;
    fn player_action(&mut self, collector: i32) -> Option<i8>;
    fn pickup_game_mode(&mut self) -> i32;
    fn pickup(&mut self, collector: i32);
    fn remove(&mut self, has_set_entry: bool);
    fn refresh_shape(&mut self);
    fn report_position(&mut self);
    fn enter_collision(&mut self);
}

pub struct MovingEmeraldFrame {
    pub mode: i8,
    pub submode: i8,
    pub has_allocation: bool,
    pub has_set_entry: bool,
    pub type_word: u32,
    pub spin: u32,
}

impl MovingEmeraldFrame {
    /// Returns true if the native callback dispatched removal.
    pub fn update(
        &mut self,
        services: &mut impl MovingEmeraldServices,
        spin_increment: u32,
    ) -> bool {
        if (self.mode == 0 && services.mode_zero_gate() != 0) || !self.has_allocation {
            return false;
        }
        if self.submode != 3 {
            services.advance_and_transform();
        }
        match self.submode {
            1 => match services.selection() {
                0 => self.submode = 3,
                1..=3 => self.submode = 2,
                _ => {}
            },
            s if s < 1 => self.submode = 1,
            _ => {}
        }
        if self.submode != 2 || self.mode == 5 {
            return false;
        }
        if services.collision_flags() & 1 != 0
            && !(services.hit_count() != 0
                && services.game_mode() == 0x1a
                && services.engine_byte() == 0
                && self.type_word == 9)
            && services.owner_id() <= 1
        {
            let candidate = services.collector();
            if !checks_player_action(u64::from(self.type_word))
                || !needs_mode_query(services.player_action(candidate))
                || mode_allows_pickup(services.pickup_game_mode())
            {
                let collector = services.collector();
                services.pickup(collector);
                services.remove(self.has_set_entry);
                return true;
            }
        }
        services.refresh_shape();
        services.report_position();
        services.enter_collision();
        self.spin = self.spin.wrapping_add(spin_increment);
        false
    }
}
