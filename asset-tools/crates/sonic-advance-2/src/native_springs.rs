//! Retail course spring objects, including their anchored OAM scripts and player transitions.
use crate::{
    Error, VerifiedRom,
    native_animation::{AnimationPlayer, Hitbox},
    native_entities::{Interactable, StageEntities},
    native_player::{IN_AIR, PlayerState, STOOD_ON_OBJECT},
};
use serde::Serialize;

const GRAPHICS_TABLE: usize = 0xd52e0;
const TRANSITIONS: usize = GRAPHICS_TABLE + 30 * 8;
// Constructor table (entity type 3..12) to the spring type used by the ROM.
const SPRING_TYPES: [usize; 10] = [0, 1, 3, 2, 4, 5, 7, 6, 9, 8];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SpringLaunch {
    pub transition: u8,
    pub speed_id: u8,
    pub sound_id: u16,
}

impl SpringLaunch {
    /// Run through the entry to Player_8029074. The host invalidates its
    /// previous animation state and executes that callback in the same frame.
    pub fn apply(&self, level_id: u8, p: &mut PlayerState) {
        let Some(dir @ 0..=7) = self.transition.checked_sub(14) else {
            return;
        };
        let strength = usize::from(self.speed_id & 3);
        p.unk6c = true;
        p.unk6e = (dir << 4) | strength as u8;
        let speed = [1920, 2304, 2688, 3072][strength];
        p.cancel_flying_and_boost();
        if matches!(dir, 2 | 3) && p.move_state & IN_AIR == 0 {
            p.move_state &= !(0x100 | IN_AIR);
        } else {
            p.move_state |= 0x100 | IN_AIR;
        }
        p.move_state &= !(0x1000000 | 0x20);
        p.change_sprite_offsets(6, 14, false);
        let music = matches!(level_id, 8 | 9);
        p.char_state = if music {
            37
        } else if i32::from(p.q_speed_air_x).abs() < 640 {
            38
        } else {
            39
        };
        if matches!(dir, 2 | 4 | 6) {
            p.q_speed_air_x = -speed;
        }
        if matches!(dir, 3 | 5 | 7) {
            p.q_speed_air_x = speed;
        }
        if matches!(dir, 0 | 4 | 5) {
            p.q_speed_air_y = -speed;
        }
        if matches!(dir, 1 | 6 | 7) {
            p.q_speed_air_y = speed;
        }
        if matches!(dir, 2 | 3) && p.move_state & IN_AIR == 0 && speed > 2304 {
            p.is_boosting = true;
        }
        p.disable_trick_timer = [4, 3, 2, 2][strength];
        if music {
            p.q_speed_air_x = (i32::from(p.q_speed_air_x) * 5 / 4) as i16;
            p.q_speed_air_y = (i32::from(p.q_speed_air_y) * 5 / 4) as i16;
        }
        if p.move_state & crate::native_player::IN_WATER != 0 {
            p.q_speed_air_y /= 2;
        }
        if p.q_speed_air_x > 320 {
            p.move_state &= !1;
        }
        if p.q_speed_air_x < -320 {
            p.move_state |= 1;
        }
    }
}

struct Spring<'a> {
    index: usize,
    x: i32,
    y: i32,
    animation: AnimationPlayer<'a>,
    idle_variant: u8,
    activated: bool,
    launch: SpringLaunch,
}

pub struct StageSprings<'a> {
    rom: &'a VerifiedRom<'a>,
    level_id: u8,
    active: Vec<Spring<'a>>,
    stood_on: Option<usize>,
}

impl<'a> StageSprings<'a> {
    pub fn new(rom: &'a VerifiedRom<'a>, level_id: u8) -> Self {
        Self {
            rom,
            level_id,
            active: Vec::new(),
            stood_on: None,
        }
    }

    fn spawn(&mut self, index: usize, entity: Interactable) -> Result<(), Error> {
        let spring_type = SPRING_TYPES[usize::from(entity.kind - 3)];
        let zone = self.level_id / 4;
        let family = match zone {
            2 => 1,
            5 => 2,
            _ => 0,
        };
        let raw = self
            .rom
            .range("rom", GRAPHICS_TABLE + (family * 10 + spring_type) * 8, 8)?;
        let anim = u16::from_le_bytes([raw[0], raw[1]]);
        let variant = raw[2];
        let flags = u16::from_le_bytes([raw[6], raw[7]]);
        let mut animation = AnimationPlayer::new(self.rom, anim, variant)?;
        animation.set_flip(flags & 0x400 != 0, flags & 0x800 != 0);
        animation.advance()?;
        let speed_id = entity.data[0] & 3;
        let transition = self.rom.range("rom", TRANSITIONS + spring_type, 1)?[0];
        let sound_id = if zone == 2 {
            let raw = self
                .rom
                .range("rom", TRANSITIONS + 10 + usize::from(speed_id) * 2, 2)?;
            u16::from_le_bytes([raw[0], raw[1]])
        } else {
            153
        };
        self.active.push(Spring {
            index,
            x: i32::from(entity.x as i16),
            y: i32::from(entity.y as i16),
            animation,
            idle_variant: variant,
            activated: false,
            launch: SpringLaunch {
                transition,
                speed_id,
                sound_id,
            },
        });
        Ok(())
    }

    /// Run after camera-based entity admission and before the player callback.
    /// Every touching spring emits its own native sound; the last transition
    /// written in task order is the one the player consumes.
    pub fn tick(
        &mut self,
        entities: &mut StageEntities,
        p: &mut PlayerState,
        camera_x: i32,
        camera_y: i32,
    ) -> Result<Vec<SpringLaunch>, Error> {
        for i in entities.take_new_springs() {
            self.spawn(i, entities.placements()[i])?;
        }
        if p.move_state & STOOD_ON_OBJECT == 0 {
            self.stood_on = None;
        }
        let mut launches = Vec::new();
        let mut cursor = 0;
        while cursor < self.active.len() {
            let spring = &mut self.active[cursor];
            let was_active = spring.activated;
            let hitbox = spring.animation.hitboxes()[0];
            if p.move_state & (0x400000 | 0x80) == 0 && hitbox.index != -1 {
                let stood =
                    self.stood_on == Some(spring.index) && p.move_state & STOOD_ON_OBJECT != 0;
                if stood {
                    p.move_state &= !STOOD_ON_OBJECT;
                }
                let mask = contact(spring.x, spring.y, hitbox, p);
                if mask & 0x10000 != 0 {
                    p.move_state |= STOOD_ON_OBJECT;
                    self.stood_on = Some(spring.index);
                } else if stood {
                    self.stood_on = None;
                }
                if mask != 0 {
                    p.unk6c = true;
                    p.unk6e = spring.launch.speed_id;
                    launches.push(spring.launch);
                    if !was_active {
                        spring.activated = true;
                        spring
                            .animation
                            .set_animation(spring.animation.animation, spring.idle_variant + 1)?;
                    }
                }
            }
            let sx = spring.x.wrapping_sub(camera_x) as i16;
            let sy = spring.y.wrapping_sub(camera_y) as i16;
            let outside = (i32::from(sx) + 128) as u16 > 496 || !(-128..=288).contains(&sy);
            if outside {
                entities.restore_spring(spring.index);
                self.active.remove(cursor);
                continue;
            }
            if was_active && spring.animation.advance()?.ended {
                spring.activated = false;
                spring
                    .animation
                    .set_animation(spring.animation.animation, spring.idle_variant)?;
                spring.animation.advance()?;
            }
            cursor += 1;
        }
        Ok(launches)
    }

    /// In native task/OAM insertion order. Use each frame's intrinsic origin.
    pub fn sprites(&self) -> impl Iterator<Item = (i32, i32, &AnimationPlayer<'a>)> {
        self.active.iter().map(|s| (s.x, s.y, &s.animation))
    }
}

/// Non-inverted single-player collision used by ordinary course springs.
fn contact(x: i32, y: i32, hitbox: Hitbox, p: &mut PlayerState) -> u32 {
    let [left, top, right, bottom] = hitbox.rectangle.map(i32::from);
    let px = p.q_world_x >> 8;
    let py = p.q_world_y >> 8;
    let w = i32::from(p.sprite_offset_x);
    let h = i32::from(p.sprite_offset_y);
    if px + w < x + left || px - w > x + right || py + h < y + top || py - h > y + bottom {
        return 0;
    }
    let from_right = px >= x + ((left + right) >> 1);
    let dx = if from_right {
        x + right - (px - w)
    } else {
        x + left - (px + w)
    };
    let from_above = py < y + ((top + bottom) >> 1);
    let dy = if from_above {
        y + top - (py + h)
    } else {
        y + bottom - (py - h)
    };
    let y_bias = if from_above {
        (dy + 5).min(0)
    } else {
        (dy + 2).max(0)
    };
    if dx.abs() < y_bias.abs() {
        let displacement = (dx as u32 & 255) << 8;
        if displacement == 0 {
            return 0;
        }
        return displacement | if from_right { 0x40000 } else { 0x80000 };
    }
    if from_above {
        if p.q_speed_air_y < 0 {
            return 0;
        }
        if p.move_state & IN_AIR == 0 && p.rotation.wrapping_add(32) & 64 != 0 {
            p.q_speed_ground = 0;
        }
    }
    (dy as u32 & 255) | if from_above { 0x10000 } else { 0x20000 }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn contact_floor_ceiling_sides_and_rotation_match_c() {
        let mut bytes = Vec::new();
        let hit = Hitbox {
            index: 0,
            rectangle: [-13, -7, 12, 1],
        };
        for k in 0..8 {
            for y in -24..=24 {
                for x in -24..=24 {
                    let mut p = PlayerState::new_at(
                        crate::native_player::Character::Sonic,
                        x * 256,
                        y * 256,
                    );
                    p.q_speed_air_y = if k & 1 != 0 { -1 } else { 1 };
                    p.q_speed_ground = 257;
                    p.rotation = if k & 2 != 0 { 64 } else { 0 };
                    p.move_state = if k & 4 != 0 { 2 } else { 0 };
                    let mask = contact(0, 0, hit, &mut p);
                    bytes.extend(mask.to_le_bytes());
                    bytes.extend(i32::from(p.q_speed_ground).to_le_bytes());
                }
            }
        }
        assert_eq!(
            crate::gba::sha256(&bytes),
            "124c771a5154f0fea8d9a625a05489aebc58e25a3ff613ee54205b9d48890b7e"
        );
    }
}
