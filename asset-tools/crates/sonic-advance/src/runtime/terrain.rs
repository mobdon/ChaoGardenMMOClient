//! Bounded upright native tile-boundary sensors. Wall and ceiling sweeps remain
//! the runtime adapter's responsibility; this does not implement loop traversal.
use crate::{collision::CollisionStage, movement::ONE, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Floor {
    pub distance: i32,
    pub angle: u8,
}

// Native sub_801E4E4 with p3=+8. At most three boundary reads, including
// penetration recovery into the preceding two tiles. Pixel occupancy rays
// cannot replace this: positive profiles settle one pixel before the boundary.
fn down_distance(mut sample: impl FnMut(i32) -> Result<(i8, u8)>, y: i32) -> Result<Floor> {
    let remainder = y.rem_euclid(8);
    let (first, mut angle) = sample(y)?;
    let distance = if first == 0 {
        let mut result = 24 - remainder;
        for tile in 1..=2 {
            let (boundary, next_angle) = sample(y + tile * 8)?;
            if boundary != 0 {
                angle = next_angle;
                result = match boundary {
                    8 => -1 - remainder + tile * 8,
                    1..=7 => i32::from(boundary) - 1 - remainder + tile * 8,
                    _ => i32::from(boundary) + remainder + tile * 8,
                };
                break;
            }
        }
        result
    } else if first == 8 {
        let (previous, previous_angle) = sample(y - 8)?;
        if previous == 8 {
            angle = previous_angle;
            let (earlier, earlier_angle) = sample(y - 16)?;
            match earlier {
                8 => {
                    angle = earlier_angle;
                    -17 - remainder
                }
                0 => -9 - remainder,
                1..=7 => {
                    angle = earlier_angle;
                    i32::from(earlier) - 17 - remainder
                }
                _ => {
                    angle = earlier_angle;
                    i32::from(earlier) + remainder - 16
                }
            }
        } else {
            match previous {
                0 => -1 - remainder,
                1..=7 => {
                    angle = previous_angle;
                    i32::from(previous) - 9 - remainder
                }
                _ => {
                    angle = previous_angle;
                    i32::from(previous) + remainder - 8
                }
            }
        }
    } else if first > 0 {
        i32::from(first) - 1 - remainder
    } else {
        i32::from(first) + remainder
    };
    Ok(Floor { distance, angle })
}

pub(super) fn floor_pair(
    geometry: &CollisionStage<'_>,
    x: i32,
    y: i32,
    height: i32,
    layer: u8,
) -> Result<(Floor, Floor)> {
    let x = x.div_euclid(ONE);
    let y = y.div_euclid(ONE) + height;
    let mut sensors = [Floor {
        distance: 0,
        angle: 0,
    }; 2];
    for (sensor, sensor_x) in sensors.iter_mut().zip([x - 8, x + 8]) {
        *sensor = down_distance(
            |sensor_y| {
                // Native low-level readers clamp coordinates to the stage edges.
                let sx = sensor_x.clamp(0, i32::from(geometry.pixel_width()) - 1);
                let sy = sensor_y.clamp(0, i32::from(geometry.pixel_height()) - 1);
                let value = geometry
                    .sample(sx, sy, layer)?
                    .expect("clamped stage sample");
                Ok((value.vertical_boundary, value.vertical_angle))
            },
            y,
        )?;
    }
    // Native ties choose the right sensor, including its angle.
    Ok(if sensors[0].distance < sensors[1].distance {
        (sensors[0], sensors[1])
    } else {
        (sensors[1], sensors[0])
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_downward_boundary_branch_vectors() {
        // (current, next/previous, second next/previous, expected distance)
        for (tiles, expected) in [
            ([0, 0, 0], 21),
            ([0, 0, 8], 12),
            ([0, 0, 5], 17),
            ([0, 0, -2], 17),
            ([0, 8, 0], 4),
            ([0, 5, 0], 9),
            ([0, -2, 0], 9),
            ([8, 8, 8], -20),
            ([8, 8, 0], -12),
            ([8, 8, 5], -15),
            ([8, 8, -2], -15),
            ([8, 0, 0], -4),
            ([8, 5, 0], -7),
            ([8, -2, 0], -7),
            ([5, 0, 0], 1),
            ([-2, 0, 0], 1),
        ] {
            let mut reads = 0;
            let hit = down_distance(
                |_| {
                    let value = tiles[reads];
                    reads += 1;
                    Ok((value, 0))
                },
                19,
            )
            .unwrap();
            assert_eq!(hit.distance, expected, "{tiles:?}");
            assert!(reads <= 3);
        }
    }
}
