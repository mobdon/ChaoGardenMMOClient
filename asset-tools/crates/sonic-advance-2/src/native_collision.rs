//! Normal-gravity native player sensors and correction/landing decisions.
//!
//! Evidence: pinned SAT-R/sa2 player.c sub_802302C/sub_8023128,
//! sub_80228C0/sub_8022D6C, sub_8022F58, sub_8022190 and its four air
//! collision branches. Host errors explicitly reject unported object/water,
//! wall-running and inverted-gravity state; corrections are transactional.

use crate::{
    Error,
    native_player::{
        Callback, FLIP_WITH_MOVE_DIR, IN_AIR, IN_WATER, PlayerState, SPIN_ATTACK, STOOD_ON_OBJECT,
    },
    native_stage::{NativeStage, TerrainDistance},
};
use serde::Serialize;

pub trait TerrainProbes {
    fn terrain_probe(
        &self,
        x: i32,
        y: i32,
        layer: u8,
        step: i32,
        vertical: bool,
    ) -> Result<TerrainDistance, Error>;
}

impl TerrainProbes for NativeStage<'_> {
    fn terrain_probe(
        &self,
        x: i32,
        y: i32,
        layer: u8,
        step: i32,
        vertical: bool,
    ) -> Result<TerrainDistance, Error> {
        self.probe(x, y, layer, step, vertical)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollisionReport {
    pub landed: bool,
    pub left_ground: bool,
    pub blocked_left: bool,
    pub blocked_right: bool,
    pub hit_ceiling: bool,
    pub q_correction_x: i32,
    pub q_correction_y: i32,
}

fn validate(player: &PlayerState) -> Result<(), Error> {
    if player.layer > 1 {
        return Err(Error("Native player collision layer must be 0 or 1".into()));
    }
    if player.move_state & (IN_WATER | STOOD_ON_OBJECT | 0x200 | 0x800000) != 0 {
        return Err(Error(
            "Object/water/special native collision state is not ported".into(),
        ));
    }
    Ok(())
}

fn floor_quadrant(rotation: u8) -> bool {
    let signed = i32::from(rotation as i8);
    let rounded = signed
        + if signed <= 0 && signed + 32 > 0 {
            32
        } else {
            31
        };
    (rounded as u8) >> 6 == 0
}

fn require_floor(player: &PlayerState) -> Result<(), Error> {
    validate(player)?;
    if !floor_quadrant(player.rotation) {
        return Err(Error("Native wall/ceiling running is not ported".into()));
    }
    Ok(())
}

fn transactional(
    player: &mut PlayerState,
    operation: impl FnOnce(&mut PlayerState, &mut CollisionReport) -> Result<(), Error>,
) -> Result<CollisionReport, Error> {
    validate(player)?;
    let mut next = *player;
    let mut report = CollisionReport::default();
    operation(&mut next, &mut report)?;
    report.q_correction_x = next.q_world_x.wrapping_sub(player.q_world_x);
    report.q_correction_y = next.q_world_y.wrapping_sub(player.q_world_y);
    report.left_ground = player.move_state & IN_AIR == 0 && next.move_state & IN_AIR != 0;
    *player = next;
    Ok(report)
}

#[derive(Debug, Clone, Copy)]
struct Pair {
    distance: i32,
    other: i32,
    rotation: u8,
}

fn sensor_pair(
    terrain: &impl TerrainProbes,
    player: &PlayerState,
    direction: u8,
) -> Result<Pair, Error> {
    let x = player.q_world_x >> 8;
    let y = player.q_world_y >> 8;
    let dx = 2 + i32::from(player.sprite_offset_x);
    let dy = i32::from(player.sprite_offset_y);
    let mask = player.layer
        | if (direction == 0 && player.q_speed_air_y < 0)
            || (direction != 0 && player.q_speed_air_y < 768)
        {
            128
        } else {
            0
        };
    let (first, second) = match direction {
        0 => (
            terrain.terrain_probe(x - dx, y + dy, mask, 8, true)?,
            terrain.terrain_probe(x + dx, y + dy, mask, 8, true)?,
        ),
        1 => (
            terrain.terrain_probe(x - dx, y - dy, mask, -8, false)?,
            terrain.terrain_probe(x - dx, y + dy, mask, -8, false)?,
        ),
        2 => (
            terrain.terrain_probe(x - dx, y - dy, mask, -8, true)?,
            terrain.terrain_probe(x + dx, y - dy, mask, -8, true)?,
        ),
        3 => (
            terrain.terrain_probe(x + dx, y - dy, mask, 8, false)?,
            terrain.terrain_probe(x + dx, y + dy, mask, 8, false)?,
        ),
        _ => return Err(Error("Invalid native sensor direction".into())),
    };
    let (selected, other) = if first.distance < second.distance {
        (first, second)
    } else {
        (second, first)
    };
    let raw_rotation = selected.rotation.unwrap_or(0);
    Ok(Pair {
        distance: selected.distance,
        other: other.distance,
        rotation: if raw_rotation & 1 != 0 {
            0
        } else {
            raw_rotation
        },
    })
}

/// Native sub_8022F58(rotation+128). Jump tests its return against 4 pixels.
/// The native routine stores that angle in unk28/unk29 before probing.
pub fn jump_clearance(
    terrain: &impl TerrainProbes,
    player: &mut PlayerState,
) -> Result<i32, Error> {
    require_floor(player)?;
    let angle = player.rotation.wrapping_add(128);
    let result = sensor_pair(terrain, player, angle.wrapping_add(32) >> 6)?.distance;
    player.unk28 = angle;
    player.unk29 = angle;
    Ok(result)
}

fn rounded_probe_angle(angle: u8) -> u8 {
    let positive = angle.wrapping_add(32) as i8 > 0;
    let nonpositive = angle as i8 <= 0;
    angle.wrapping_add(if positive == nonpositive { 32 } else { 31 })
}

/// Native sub_8023128 called at the end of ground input, before conversion
/// of ground speed to air speed and before position advancement.
pub fn ground_wall(
    terrain: &impl TerrainProbes,
    player: &mut PlayerState,
) -> Result<CollisionReport, Error> {
    require_floor(player)?;
    transactional(player, |p, report| {
        if p.q_speed_ground == 0 {
            return Ok(());
        }
        let angle = p
            .rotation
            .wrapping_add(if p.q_speed_ground >= 0 { 192 } else { 64 });
        let direction = rounded_probe_angle(angle) >> 6;
        let x = p.q_world_x >> 8;
        let y = p.q_world_y >> 8;
        let reach = 2 + i32::from(p.sprite_offset_x);
        let sample = match direction {
            0 => terrain.terrain_probe(x, y + reach, p.layer, 8, true)?,
            1 => terrain.terrain_probe(x - reach, y, p.layer, -8, false)?,
            2 => terrain.terrain_probe(x, y - reach, p.layer, -8, true)?,
            _ => terrain.terrain_probe(x + reach, y, p.layer, 8, false)?,
        };
        p.unk28 = angle;
        p.unk29 = angle;
        if sample.distance <= 0 {
            let delta = sample.distance.wrapping_mul(256);
            match angle.wrapping_add(32) >> 6 {
                0 => {
                    p.q_world_y = p.q_world_y.wrapping_add(delta);
                    p.q_speed_air_y = 0;
                }
                1 => {
                    p.q_world_x = p.q_world_x.wrapping_sub(delta);
                    p.q_speed_air_x = 0;
                    p.q_speed_ground = 0;
                    p.move_state |= 0x20;
                    report.blocked_left = true;
                }
                2 => {
                    p.q_world_y = p.q_world_y.wrapping_sub(delta);
                    p.q_speed_air_y = 0;
                    p.move_state |= IN_AIR;
                    report.hit_ceiling = true;
                }
                _ => {
                    p.q_world_x = p.q_world_x.wrapping_add(delta);
                    p.q_speed_air_x = 0;
                    p.q_speed_ground = 0;
                    p.move_state |= 0x20;
                    report.blocked_right = true;
                }
            }
            p.unk62 = 0;
            p.unk63 = 0;
        }
        Ok(())
    })
}

/// Native floor branch sub_80228C0. Called after position advancement.
pub fn ground_follow(
    terrain: &impl TerrainProbes,
    player: &mut PlayerState,
) -> Result<CollisionReport, Error> {
    require_floor(player)?;
    transactional(player, |p, _| {
        let x = p.q_world_x >> 8;
        let y = (p.q_world_y >> 8) + i32::from(p.sprite_offset_y);
        let reach = 2 + i32::from(p.sprite_offset_x);
        let a = terrain.terrain_probe(x - reach, y, p.layer, 8, true)?;
        let b = terrain.terrain_probe(x + reach, y, p.layer, 8, true)?;
        if let Some(rotation) = a.rotation {
            p.unk28 = rotation;
        }
        if let Some(rotation) = b.rotation {
            p.unk29 = rotation;
        }
        let (distance, rotation) = if a.distance < b.distance {
            (a.distance, p.unk28)
        } else {
            (b.distance, p.unk29)
        };
        if distance < -11 {
            return Ok(());
        }
        if distance > 0 {
            let following_limit = ((i32::from(p.q_speed_air_x).abs() >> 8) + 3).min(11);
            if distance > following_limit {
                p.move_state |= IN_AIR;
                p.move_state &= !0x20;
                return Ok(());
            }
        }
        p.q_world_y = p.q_world_y.wrapping_add(distance.wrapping_mul(256));
        if rotation & 1 == 0 {
            if !floor_quadrant(rotation) {
                return Err(Error(
                    "Native floor continuation entered unported wall-running quadrant".into(),
                ));
            }
            p.rotation = rotation;
        }
        Ok(())
    })
}

fn land(p: &mut PlayerState, rotation: u8, report: &mut CollisionReport) -> Result<(), Error> {
    if !floor_quadrant(rotation) {
        return Err(Error(
            "Native landing would attach to unported wall/ceiling".into(),
        ));
    }
    p.rotation = rotation;
    p.callback = Callback::Ground;
    p.move_state &= !(FLIP_WITH_MOVE_DIR | IN_AIR);
    if p.move_state & SPIN_ATTACK != 0 {
        p.move_state &= !SPIN_ATTACK;
        p.change_sprite_offsets(6, 14, false);
    } else {
        p.sprite_offset_x = 6;
        p.sprite_offset_y = 14;
    }
    p.defeat_score_index = 0;
    report.landed = true;
    Ok(())
}

fn side(
    terrain: &impl TerrainProbes,
    p: &mut PlayerState,
    right: bool,
    report: &mut CollisionReport,
) -> Result<(), Error> {
    let reach = 3 + i32::from(p.sprite_offset_x);
    let x = (p.q_world_x >> 8) + if right { reach } else { -reach };
    let mask = p.layer | if p.q_speed_air_y < 768 { 128 } else { 0 };
    let sample =
        terrain.terrain_probe(x, p.q_world_y >> 8, mask, if right { 8 } else { -8 }, false)?;
    if sample.distance <= 0 {
        let delta = sample.distance.wrapping_mul(256);
        p.q_world_x = if right {
            p.q_world_x.wrapping_add(delta)
        } else {
            p.q_world_x.wrapping_sub(delta)
        };
        p.q_speed_air_x = 0;
        if right {
            report.blocked_right = true;
        } else {
            report.blocked_left = true;
        }
    }
    Ok(())
}

fn descending(
    terrain: &impl TerrainProbes,
    p: &mut PlayerState,
    report: &mut CollisionReport,
) -> Result<(), Error> {
    side(terrain, p, false, report)?;
    side(terrain, p, true, report)?;
    let floor = sensor_pair(terrain, p, 0)?;
    if floor.distance < 0 {
        let limit = -((i32::from(p.q_speed_air_y) >> 8) + 6);
        if floor.distance >= limit || floor.other >= limit {
            p.q_world_y = p.q_world_y.wrapping_add(floor.distance.wrapping_mul(256));
            land(p, floor.rotation, report)?;
            if (u16::from(floor.rotation) + 32) & 0x40 != 0 {
                p.q_speed_air_x = 0;
                p.q_speed_air_y = p.q_speed_air_y.min(3024);
            } else if (u16::from(floor.rotation) + 16) & 0x20 == 0 {
                p.q_speed_air_y = 0;
                p.q_speed_ground = p.q_speed_air_x;
                return Ok(());
            } else {
                p.q_speed_air_y >>= 1;
            }
            let speed = i32::from(p.q_speed_air_y).abs();
            p.q_speed_ground = (if (floor.rotation as i8) < 0 {
                -speed
            } else {
                speed
            }) as i16;
        }
    }
    Ok(())
}

fn ascending(
    terrain: &impl TerrainProbes,
    p: &mut PlayerState,
    report: &mut CollisionReport,
) -> Result<(), Error> {
    side(terrain, p, false, report)?;
    side(terrain, p, true, report)?;
    let ceiling = sensor_pair(terrain, p, 2)?;
    if ceiling.distance <= 0 {
        p.q_world_y = p.q_world_y.wrapping_sub(ceiling.distance.wrapping_mul(256));
        report.hit_ceiling = true;
        if (u16::from(ceiling.rotation) + 32) & 0x40 != 0
            && i32::from(ceiling.rotation as i8) - 64 > 0
        {
            return Err(Error(
                "Native ceiling contact would attach to unported wall-running state".into(),
            ));
        }
        p.q_speed_air_y = 0;
    }
    Ok(())
}

fn sideways(
    terrain: &impl TerrainProbes,
    p: &mut PlayerState,
    right: bool,
    report: &mut CollisionReport,
) -> Result<(), Error> {
    side(terrain, p, right, report)?;
    if report.blocked_left || report.blocked_right {
        p.q_speed_ground = p.q_speed_air_y;
    }
    let ceiling = sensor_pair(terrain, p, 2)?;
    if ceiling.distance <= 0 {
        p.q_world_y = p.q_world_y.wrapping_sub(ceiling.distance.wrapping_mul(256));
        report.hit_ceiling = true;
        if p.q_speed_air_y < 0 {
            p.q_speed_air_y = 0;
        }
    } else if p.q_speed_air_y >= 0 {
        let floor = sensor_pair(terrain, p, 0)?;
        if floor.distance <= 0 {
            p.q_world_y = p.q_world_y.wrapping_add(floor.distance.wrapping_mul(256));
            land(p, floor.rotation, report)?;
            p.q_speed_air_y = 0;
            p.q_speed_ground = p.q_speed_air_x;
        }
    }
    Ok(())
}

/// Integer angle math matching the pinned portable BIOS-math reference.
/// This ports a math service; it does not interpret GBA instructions.
pub fn native_arctan2(x: i16, y: i16) -> u16 {
    fn atan(value: i32) -> u16 {
        let argument = -(value.wrapping_mul(value) >> 14);
        let mut coefficient = 0xa9i32;
        for next in [0x390, 0x91c, 0xfb6, 0x16aa, 0x2081, 0x3651, 0xa2f9] {
            coefficient = (coefficient.wrapping_mul(argument) >> 14).wrapping_add(next);
        }
        (value.wrapping_mul(coefficient) >> 16) as u16
    }
    let x = i32::from(x);
    let y = i32::from(y);
    if y == 0 {
        return if x >= 0 { 0 } else { 0x8000 };
    }
    if x == 0 {
        return if y >= 0 { 0x4000 } else { 0xc000 };
    }
    if y >= 0 {
        if x >= y {
            return atan((y << 14) / x);
        }
        if x < 0 && -x >= y {
            return atan((y << 14) / x).wrapping_add(0x8000);
        }
        0x4000u16.wrapping_sub(atan((x << 14) / y))
    } else {
        if x <= 0 && -x > -y {
            return atan((y << 14) / x).wrapping_add(0x8000);
        }
        if x > 0 && x >= -y {
            return atan((y << 14) / x);
        }
        0xc000u16.wrapping_sub(atan((x << 14) / y))
    }
}

/// Native sub_8022190 direction dispatch and supported normal-gravity
/// branches, called after gravity, position and visual-rotation updates.
pub fn air_collision(
    terrain: &impl TerrainProbes,
    player: &mut PlayerState,
) -> Result<CollisionReport, Error> {
    transactional(player, |p, report| {
        let angle = (native_arctan2(p.q_speed_air_x, p.q_speed_air_y) >> 8) as u8;
        match angle.wrapping_sub(32) >> 6 {
            0 => descending(terrain, p, report),
            1 => sideways(terrain, p, false, report),
            2 => ascending(terrain, p, report),
            _ => sideways(terrain, p, true, report),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_player::Character;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    struct Replies(RefCell<VecDeque<TerrainDistance>>);
    impl TerrainProbes for Replies {
        fn terrain_probe(
            &self,
            _: i32,
            _: i32,
            _: u8,
            _: i32,
            _: bool,
        ) -> Result<TerrainDistance, Error> {
            self.0
                .borrow_mut()
                .pop_front()
                .ok_or_else(|| Error("Unexpected sensor request".into()))
        }
    }
    fn replies(values: &[(i32, u8)]) -> Replies {
        Replies(RefCell::new(
            values
                .iter()
                .map(|&(distance, rotation)| TerrainDistance {
                    distance,
                    rotation: Some(rotation),
                })
                .collect(),
        ))
    }
    fn player() -> PlayerState {
        let mut p = PlayerState::new_at(Character::Sonic, 100 * 256, 100 * 256);
        p.move_state = 0;
        p
    }

    #[test]
    fn ground_native_penetration_and_follow_limits_are_preserved() {
        let mut p = player();
        p.q_speed_air_x = 0;
        let before = p;
        ground_follow(&replies(&[(-12, 0), (-13, 0)]), &mut p).unwrap();
        assert_eq!(p.q_world_y, before.q_world_y);
        ground_follow(&replies(&[(3, 0), (4, 0)]), &mut p).unwrap();
        assert_eq!(p.q_world_y, before.q_world_y + 3 * 256);
        let report = ground_follow(&replies(&[(4, 0), (5, 0)]), &mut p).unwrap();
        assert!(report.left_ground);
        let before = p;
        assert!(ground_follow(&replies(&[(0, 64), (0, 64)]), &mut p).is_err());
        assert_eq!(p, before, "unsupported mode cannot partly mutate state");
    }

    #[test]
    fn flat_downward_landing_resets_native_shape_callback_and_speed() {
        let mut p = player();
        p.move_state = IN_AIR | SPIN_ATTACK | FLIP_WITH_MOVE_DIR;
        p.sprite_offset_y = 9;
        p.q_speed_air_y = 768;
        p.q_speed_air_x = 128;
        let report = air_collision(&replies(&[(5, 0), (5, 0), (-1, 0), (-2, 0)]), &mut p).unwrap();
        assert!(report.landed);
        assert_eq!(p.callback, Callback::Ground);
        assert_eq!(
            p.move_state & (IN_AIR | SPIN_ATTACK | FLIP_WITH_MOVE_DIR),
            0
        );
        assert_eq!((p.sprite_offset_x, p.sprite_offset_y), (6, 14));
        assert_eq!(p.q_speed_air_y, 0);
        assert_eq!(p.q_speed_ground, 128);
        assert_eq!(
            p.q_world_y,
            93 * 256,
            "-2 terrain correction followed by native -5 shape restoration"
        );
    }

    #[test]
    fn direction_axes_and_unsupported_modes_are_explicit() {
        for (x, y, angle) in [(1, 0, 0), (0, 1, 0x4000), (-1, 0, 0x8000), (0, -1, 0xc000)] {
            assert_eq!(native_arctan2(x, y), angle);
        }
        let mut p = player();
        p.move_state = IN_WATER;
        assert!(jump_clearance(&replies(&[]), &mut p).is_err());
        p.move_state = 0;
        p.rotation = 64;
        assert!(ground_wall(&replies(&[]), &mut p).is_err());
    }
}
