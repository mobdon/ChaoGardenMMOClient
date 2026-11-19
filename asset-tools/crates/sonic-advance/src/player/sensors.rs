//! ASOE terrain distance is measured from the last free pixel, including at
//! tile seams. Positive distances are gaps and negative ones are penetration.
use super::*;
use crate::stage::Stage;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContactMode {
    Floor,
    Left,
    Ceiling,
    Right,
}

impl ContactMode {
    pub fn for_angle(angle: u8) -> Self {
        let signed = i32::from(angle as i8);
        let bias = if (-31..=0).contains(&signed) { 32 } else { 31 };
        Self::quadrant(angle.wrapping_add(bias))
    }
    pub(super) fn quadrant(biased: u8) -> Self {
        match biased >> 6 {
            0 => Self::Floor,
            1 => Self::Left,
            2 => Self::Ceiling,
            _ => Self::Right,
        }
    }
    fn normal(self) -> (i32, i32) {
        match self {
            Self::Floor => (0, 1),
            Self::Left => (-1, 0),
            Self::Ceiling => (0, -1),
            Self::Right => (1, 0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SensorHit {
    pub distance: i32,
    pub angle: Option<u8>,
}

/// The native sampler clamps the tile lookup, while distance arithmetic retains
/// the low bits of the original, unclamped world coordinate.
impl Stage<'_> {
    pub fn terrain_sensor(
        &self,
        x: i32,
        y: i32,
        mode: ContactMode,
        layer: usize,
        skip_one_way: bool,
    ) -> Result<SensorHit> {
        if layer > 1 {
            return Err(Error("invalid terrain sensor layer".into()));
        }
        let (dx, dy) = mode.normal();
        let axis = if dx == 0 { y } else { x };
        let positive = dx + dy > 0;
        tile_distance(axis, positive, |step| {
            let x = (i64::from(x) + i64::from(dx * step * 8)).clamp(0, self.width_px() as i64 - 1)
                as i32;
            let y = (i64::from(y) + i64::from(dy * step * 8)).clamp(0, self.height_px() as i64 - 1)
                as i32;
            let tile = self
                .collision_at(x, y, layer)?
                .ok_or_else(|| Error("missing clamped collision tile".into()))?;
            let (h, a) = if dx == 0 {
                (tile.vertical_height, tile.angle)
            } else {
                (tile.horizontal_width, tile.horizontal_angle)
            };
            Ok((
                if skip_one_way && tile.flags & 1 != 0 {
                    0
                } else {
                    h
                },
                a,
            ))
        })
    }
}

fn tile_distance(
    axis: i32,
    positive: bool,
    mut sample: impl FnMut(i32) -> Result<(i8, u8)>,
) -> Result<SensorHit> {
    let local = axis & 7;
    let full_edge = if positive { -1 - local } else { local - 8 };
    let (h, a) = sample(0)?;
    let stride = if h == 8 { -1 } else { 1 };
    let mut angle = None;
    let mut current = (h, a);
    for depth in 0..=2 {
        let offset = depth * stride * 8;
        let (h, a) = current;
        if !(-7..=8).contains(&h) {
            return Err(Error("invalid collision profile height".into()));
        }
        if h != 0 {
            angle = Some(a);
        }
        let distance = match h {
            0 if stride < 0 => full_edge + offset + 8,
            0 if depth == 2 => {
                if positive {
                    24 - local
                } else {
                    local + 17
                }
            }
            8 if stride > 0 || depth == 2 => full_edge + offset,
            0 | 8 => {
                current = sample((depth + 1) * stride)?;
                continue;
            }
            h if h > 0 => offset + i32::from(h) - 1 - local,
            h => offset + i32::from(h) + local,
        };
        return Ok(SensorHit { distance, angle });
    }
    unreachable!()
}

impl Player {
    pub(super) fn shift_radius(&mut self, new_height: i32) {
        let mode =
            ContactMode::quadrant(
                self.angle
                    .wrapping_add(if self.angle == 0 { 32 } else { 31 }),
            );
        self.correct(mode, self.half_height - new_height);
        self.half_height = new_height;
    }
    fn correct(&mut self, mode: ContactMode, distance: i32) {
        let (dx, dy) = mode.normal();
        self.x += dx * distance * ONE;
        self.y += dy * distance * ONE;
    }
    pub(super) fn probe_pair(
        &self,
        stage: &Stage<'_>,
        mode: ContactMode,
        skip: bool,
    ) -> Result<(SensorHit, SensorHit)> {
        let (dx, dy) = mode.normal();
        let x = (self.x >> 8) + dx * self.half_height;
        let y = (self.y >> 8) + dy * self.half_height;
        let side = HALF_WIDTH + 2;
        // Left-wall A is above center, right-wall A below center.
        let a = stage.terrain_sensor(
            x - dy * side,
            y + dx * side,
            mode,
            self.collision_layer,
            skip,
        )?;
        let b = stage.terrain_sensor(
            x + dy * side,
            y - dx * side,
            mode,
            self.collision_layer,
            skip,
        )?;
        Ok(if a.distance < b.distance {
            (a, b)
        } else {
            (b, a)
        })
    }
    pub(super) fn jump_clearance(&self, stage: &Stage<'_>) -> Result<i32> {
        let mode = ContactMode::quadrant(self.angle.wrapping_add(160));
        let (dx, dy) = mode.normal();
        let x = self.x >> 8;
        let y = self.y >> 8;
        let (normal, lateral) = if dx == 0 {
            (self.half_height, HALF_WIDTH + 2)
        } else {
            (HALF_WIDTH + 2, self.half_height)
        };
        let skip = if mode == ContactMode::Floor {
            self.vy < 0
        } else {
            self.vy < 3 * ONE
        };
        let a = stage.terrain_sensor(
            x + dx * normal - dy * lateral,
            y + dy * normal + dx * lateral,
            mode,
            self.collision_layer,
            skip,
        )?;
        let b = stage.terrain_sensor(
            x + dx * normal + dy * lateral,
            y + dy * normal - dx * lateral,
            mode,
            self.collision_layer,
            skip,
        )?;
        Ok(a.distance.min(b.distance))
    }
    pub(super) fn move_native(
        &mut self,
        stage: &Stage<'_>,
        ground: bool,
        out: &mut FrameOutcome,
    ) -> Result<()> {
        if ground {
            if self.ground_speed != 0 {
                let ahead =
                    ContactMode::for_angle(self.angle.wrapping_add(if self.ground_speed > 0 {
                        192
                    } else {
                        64
                    }));
                let (dx, dy) = ahead.normal();
                let hit = stage.terrain_sensor(
                    (self.x >> 8) + dx * (HALF_WIDTH + 2),
                    (self.y >> 8) + dy * (HALF_WIDTH + 2),
                    ahead,
                    self.collision_layer,
                    false,
                )?;
                if hit.distance <= 0 {
                    self.correct(ahead, hit.distance);
                    if dx != 0 {
                        self.vx = 0;
                        self.ground_speed = 0;
                        out.blocked_x = true;
                    } else {
                        self.vy = 0;
                        out.blocked_y = true;
                        if ahead == ContactMode::Ceiling {
                            self.grounded = false;
                            out.left_ground = true;
                            return Ok(());
                        }
                    }
                }
            }
            self.integrate(stage);
            self.follow_support(stage, out)?;
            if self.adhesion_timer != 0 {
                self.adhesion_timer -= 1;
            } else if self.angle.wrapping_add(32) & 0xc0 != 0 && self.ground_speed.abs() < 480 {
                self.ground_speed = 0;
                self.grounded = false;
                out.left_ground = true;
                self.adhesion_timer = 30;
            }
        } else {
            self.integrate(stage);
            self.vy += GRAVITY;
            self.angle = toward_zero(i32::from(self.angle as i8), 2) as u8;
            self.air_contact(stage, out)?;
        }
        Ok(())
    }
    pub(super) fn follow_support(
        &mut self,
        stage: &Stage<'_>,
        out: &mut FrameOutcome,
    ) -> Result<()> {
        if self.object_support {
            return Ok(());
        }
        let mode = ContactMode::for_angle(self.angle);
        let (hit, _) = self.probe_pair(stage, mode, false)?;
        let tangent = if mode.normal().0 == 0 {
            self.vx
        } else {
            self.vy
        };
        if hit.distance > ((tangent.abs() >> 8) + 3).min(11) {
            self.grounded = false;
            out.left_ground = true;
        } else if hit.distance >= -11 {
            self.correct(mode, hit.distance);
            if let Some(a) = hit.angle.filter(|a| a & 1 == 0) {
                self.angle = a;
            }
        }
        Ok(())
    }
    fn integrate(&mut self, stage: &Stage<'_>) {
        self.x = (self.x + self.vx).clamp(
            HALF_WIDTH * ONE,
            (stage.width_px() as i32 - HALF_WIDTH - 1) * ONE,
        );
        self.y += self.vy;
    }

    fn air_contact(&mut self, stage: &Stage<'_>, out: &mut FrameOutcome) -> Result<()> {
        use ContactMode::*;
        let direction = match (self.vx, self.vy) {
            (0, 0) => Right,
            (x, y) if y >= x && y > -x => Floor,
            (x, y) if -x >= y && -x > -y => Left,
            (x, y) if -y >= -x && -y > x => Ceiling,
            _ => Right,
        };
        for side in [Left, Right] {
            if matches!(direction, Left | Right) && direction != side {
                continue;
            }
            let (dx, _) = side.normal();
            let hit = stage.terrain_sensor(
                (self.x >> 8) + dx * (HALF_WIDTH + 3),
                self.y >> 8,
                side,
                self.collision_layer,
                self.vy < 3 * ONE,
            )?;
            if hit.distance <= 0 {
                self.correct(side, hit.distance);
                self.vx = 0;
                if matches!(direction, Left | Right) {
                    self.ground_speed = self.vy;
                }
                out.blocked_x = true;
            }
        }
        if direction != Floor {
            // The air-ceiling pair visits left before right.
            let x = self.x >> 8;
            let y = (self.y >> 8) - self.half_height;
            let left = stage.terrain_sensor(
                x - HALF_WIDTH - 2,
                y,
                Ceiling,
                self.collision_layer,
                self.vy < 3 * ONE,
            )?;
            let right = stage.terrain_sensor(
                x + HALF_WIDTH + 2,
                y,
                Ceiling,
                self.collision_layer,
                self.vy < 3 * ONE,
            )?;
            let hit = if left.distance < right.distance {
                left
            } else {
                right
            };
            if hit.distance <= 0 {
                self.correct(Ceiling, hit.distance);
                out.blocked_y = true;
                let angle = hit.angle.filter(|a| a & 1 == 0).unwrap_or(0);
                if direction == Ceiling
                    && angle.wrapping_add(32) & 64 != 0
                    && i32::from(angle as i8) > 64
                {
                    self.land(angle, out);
                    self.ground_speed = self.vy.abs();
                } else if self.vy < 0 {
                    self.vy = 0;
                }
                return Ok(());
            }
            if direction == Ceiling || self.vy < 0 {
                return Ok(());
            }
        }
        let (hit, other) = self.probe_pair(stage, Floor, self.vy < 0)?;
        if direction == Floor {
            if hit.distance >= 0 || other.distance < -(6 + (self.vy >> 8)) {
                return Ok(());
            }
        } else if hit.distance > 0 {
            return Ok(());
        }
        self.correct(Floor, hit.distance);
        self.land(hit.angle.filter(|a| a & 1 == 0).unwrap_or(0), out);
        if direction != Floor
            || (self.angle.wrapping_add(32) & 64 == 0 && self.angle.wrapping_add(16) & 32 == 0)
        {
            self.vy = 0;
            self.ground_speed = self.vx;
        } else {
            if self.angle.wrapping_add(32) & 64 != 0 {
                self.vx = 0;
                self.vy = self.vy.min(3024);
            } else {
                self.vy >>= 1;
            }
            self.ground_speed = if (self.angle as i8) < 0 {
                -self.vy.abs()
            } else {
                self.vy.abs()
            };
        }
        Ok(())
    }
    fn land(&mut self, angle: u8, out: &mut FrameOutcome) {
        self.angle = angle;
        self.shift_radius(STANDING_HALF_HEIGHT);
        self.grounded = true;
        self.jumping = false;
        out.landed = true;
    }
}
