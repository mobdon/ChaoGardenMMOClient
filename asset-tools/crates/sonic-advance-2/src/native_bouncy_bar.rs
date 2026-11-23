//! Native Leaf Forest bouncy-bar task and compression/launch arithmetic.
use crate::{
    Error, VerifiedRom,
    native_animation::{AnimationPlayer, Hitbox},
    native_entities::Interactable,
    native_player::{self as player, PlayerState},
};
use serde::Serialize;

pub const ENTITY_KIND: u8 = 31;
pub const ANIMATION: u16 = 538;
pub const SOUND: u16 = 279;
pub const IA_OVERRIDE: u32 = 0x0040_0000;
pub const UNCURL_TRANSITION: u8 = 5;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct BarState {
    pub launching: bool,
    pub landing_speed: u8,
    pub launch_frame: u8,
    pub spring_stiffness: i16,
    pub landing_position: i16,
}

impl BarState {
    /// Idle contact decision. Hitbox overlap is supplied by the native sprite
    /// service; the task adds its descending, alive and above-bar conditions.
    pub fn idle_tick(&mut self, p: &mut PlayerState, x: i32, y: i32, overlap: bool) -> bool {
        if p.move_state & crate::native_lifecycle::DEAD != 0
            || !overlap
            || p.q_speed_air_y <= 0
            || (p.q_world_y >> 8).wrapping_add(4) >= y
        {
            return false;
        }
        p.char_state = 50;
        self.landing_speed = (((i32::from(p.q_speed_air_y) >> 8) >> 2) as u8).min(2);
        self.launch_frame = self.landing_speed * 5 + 10;
        self.spring_stiffness = 9;
        self.landing_position = x.wrapping_sub(p.q_world_x >> 8).wrapping_abs() as i16;
        self.launching = true;
        p.move_state |= IA_OVERRIDE;
        true
    }

    pub fn active_variant(&self) -> u8 {
        3 - self.landing_speed
    }

    /// Task_BouncyBarLaunch runs before player main. The final compression
    /// update also releases the pending player callback in that same frame.
    pub fn launch_tick(&mut self, p: &mut PlayerState) {
        if self.launch_frame == 0 {
            return;
        }
        self.launch_frame -= 1;
        p.q_world_y = p
            .q_world_y
            .wrapping_add(i32::from(self.landing_position) * i32::from(self.spring_stiffness));
        p.q_speed_air_y = 0;
        if self.launch_frame == 0 {
            let factor =
                (((self.landing_position >> 1) + (self.landing_position >> 2)) as i8).min(24);
            let bonus = [-16, -18, -20][usize::from(self.landing_speed)];
            p.q_speed_air_y = (-384
                + ((i32::from(factor) * i32::from(self.spring_stiffness) * bonus) >> 1))
                as i16;
            p.move_state &= !(IA_OVERRIDE | 0x100);
        }
    }

    pub fn animation_finished(&mut self) {
        self.launching = false;
    }
}

/// Shared native Player_InitUncurl entry, deferred until IA_OVERRIDE clears.
/// `character_animation` is the character-relative native animation ID.
pub fn init_uncurl(p: &mut PlayerState, character_animation: u16, inverted: bool) {
    p.cancel_flying_and_boost();
    p.move_state |= player::IN_AIR;
    p.move_state &= !(0x0100_0000 | 0x20);
    if p.sprite_offset_x < 6 || p.sprite_offset_y < 9 {
        p.change_sprite_offsets(
            6,
            if matches!(character_animation, 4 | 10 | 11 | 70) {
                9
            } else {
                14
            },
            inverted,
        );
    }
    p.unk70 = true;
    p.unk6e = 0;
    p.sprite_frame_flags &= !0x4000;
    p.rotation = 0;
    p.callback = player::Callback::Uncurl;
}

/// Inclusive native HB_COLLISION of the two current sprite-body hitboxes.
pub fn body_overlap(x: i32, y: i32, entity: Hitbox, p: &PlayerState, body: Hitbox) -> bool {
    if entity.index == -1 || body.index == -1 || p.move_state & crate::native_lifecycle::DEAD != 0 {
        return false;
    }
    let [el, et, er, eb] = entity.rectangle.map(i32::from);
    let [pl, pt, pr, pb] = body.rectangle.map(i32::from);
    let px = p.q_world_x >> 8;
    let py = p.q_world_y >> 8;
    x + el <= px + pr && px + pl <= x + er && y + et <= py + pb && py + pt <= y + eb
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarEvent {
    Transition(u8),
    Sound(u16),
    Restore(usize),
}

struct Bar<'a> {
    index: usize,
    x: i32,
    y: i32,
    state: BarState,
    animation: AnimationPlayer<'a>,
}

pub struct StageBouncyBars<'a> {
    rom: &'a VerifiedRom<'a>,
    active: Vec<Bar<'a>>,
}

impl<'a> StageBouncyBars<'a> {
    pub fn new(rom: &'a VerifiedRom<'a>) -> Self {
        Self {
            rom,
            active: Vec::new(),
        }
    }

    /// Native constructor: hitbox remains inactive until the first task tick.
    pub fn admit(&mut self, index: usize, entity: Interactable) -> Result<(), Error> {
        if entity.kind != ENTITY_KIND {
            return Err(Error("Non-bouncy-bar placement passed to bar task".into()));
        }
        if self.active.iter().any(|bar| bar.index == index) {
            return Err(Error("Bouncy-bar task admitted twice".into()));
        }
        let mut animation = AnimationPlayer::new(self.rom, ANIMATION, 0)?;
        animation.set_flip(entity.data[0] != 0, false);
        self.active.push(Bar {
            index,
            x: entity.x,
            y: entity.y,
            state: BarState::default(),
            animation,
        });
        Ok(())
    }

    /// Priority0x2010, before player main. Preserve same-priority insertion order.
    pub fn tick(
        &mut self,
        p: &mut PlayerState,
        body: Hitbox,
        camera: [i32; 2],
    ) -> Result<Vec<BarEvent>, Error> {
        let mut events = Vec::new();
        let mut cursor = 0;
        while cursor < self.active.len() {
            let bar = &mut self.active[cursor];
            let was_launching = bar.state.launching;
            let activated = if was_launching {
                bar.state.launch_tick(p);
                false
            } else {
                let contact = body_overlap(bar.x, bar.y, bar.animation.hitboxes()[0], p, body);
                bar.state.idle_tick(p, bar.x, bar.y, contact)
            };
            if activated {
                bar.animation
                    .set_animation(ANIMATION, bar.state.active_variant())?;
                events.push(BarEvent::Transition(UNCURL_TRANSITION));
                events.push(BarEvent::Sound(SOUND));
            }
            let sx = bar.x.wrapping_sub(camera[0]) as i16;
            let sy = bar.y.wrapping_sub(camera[1]) as i16;
            let outside = (i32::from(sx) + 128) as u16 > 496 || !(-128..=288).contains(&sy);
            // Idle contact wins over despawn; the launch task checks afterward.
            if outside && (was_launching || !activated) {
                events.push(BarEvent::Restore(bar.index));
                self.active.remove(cursor);
                continue;
            }
            let ended = bar.animation.advance()?.ended;
            if was_launching && ended {
                bar.state.animation_finished();
                bar.animation.set_animation(ANIMATION, 0)?;
            }
            cursor += 1;
        }
        Ok(events)
    }

    /// Native OAM order18/BG priority2. The caller preserves those attributes.
    pub fn sprites(&self) -> impl Iterator<Item = (i32, i32, &AnimationPlayer<'a>)> {
        self.active.iter().map(|bar| (bar.x, bar.y, &bar.animation))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compression_does_not_launch_early_and_center_has_no_bonus() {
        for (speed, frames) in [(1, 10), (1024, 15), (2048, 20), (32767, 20)] {
            let mut p = PlayerState::new_at(player::Character::Sonic, 100 * 256, 190 * 256);
            p.q_speed_air_y = speed;
            let mut bar = BarState::default();
            assert!(bar.idle_tick(&mut p, 100, 200, true));
            assert_eq!(bar.launch_frame, frames);
            for _ in 1..frames {
                bar.launch_tick(&mut p);
                assert_eq!(p.q_speed_air_y, 0);
                assert_ne!(p.move_state & IA_OVERRIDE, 0);
            }
            bar.launch_tick(&mut p);
            assert_eq!(p.q_speed_air_y, -384);
            assert_eq!(p.move_state & (IA_OVERRIDE | 0x100), 0);
        }
    }
    #[test]
    fn inactive_hitboxes_dead_players_and_touching_edges_are_explicit() {
        let mut p = PlayerState::new_at(player::Character::Sonic, 0, 0);
        let active = Hitbox {
            index: 0,
            rectangle: [-4, -4, 4, 4],
        };
        assert!(body_overlap(8, 8, active, &p, active));
        assert!(!body_overlap(9, 8, active, &p, active));
        p.move_state |= crate::native_lifecycle::DEAD;
        assert!(!body_overlap(8, 8, active, &p, active));
    }
}
