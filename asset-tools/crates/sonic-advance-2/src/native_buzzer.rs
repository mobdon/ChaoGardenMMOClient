//! Buzzer's normal course patrol, turn, dive and return (task priority 0x4030).
use super::*;
#[derive(Clone, Copy)]
enum Phase {
    Patrol,
    Turn,
    Dive,
    Return,
}
pub(super) struct Buzzer<'a> {
    pub(super) idx: usize,
    spawn: [i32; 2],
    pub(super) x: i32,
    pub(super) y: i32,
    offset: i32,
    attack: [i32; 2],
    velocity: [i16; 2],
    target: i32,
    left: i32,
    right: i32,
    face_right: bool,
    cooldown: u8,
    phase: Phase,
    pub(super) sprite: AnimationPlayer<'a>,
}
impl<'a> Buzzer<'a> {
    pub(super) fn new(
        rom: &'a VerifiedRom<'_>,
        idx: usize,
        e: Interactable,
    ) -> Result<Self, Error> {
        let left = i32::from(e.data[0] as i8) * 8;
        Ok(Self {
            idx,
            spawn: [e.x, e.y],
            x: e.x,
            y: e.y,
            offset: 0,
            attack: [0; 2],
            velocity: [0; 2],
            target: 0,
            left,
            right: left + i32::from(e.data[2]) * 8,
            face_right: false,
            cooldown: 0,
            phase: Phase::Patrol,
            sprite: AnimationPlayer::new(rom, 482, 0)?,
        })
    }
    /// Some(true) restores the map marker for offscreen removal; defeats stay removed.
    pub(super) fn tick(
        &mut self,
        c: &mut EnemyContext<'_>,
        cx: i32,
        cy: i32,
        ev: &mut Vec<EnemyEvent>,
    ) -> Result<Option<bool>, Error> {
        let f = self.phase;
        match f {
            Phase::Patrol => self.offset += if self.face_right { 192 } else { -192 },
            Phase::Dive | Phase::Return => {
                let sign = if matches!(f, Phase::Dive) { 1 } else { -1 };
                for i in 0..2 {
                    self.attack[i] += sign * i32::from(self.velocity[i]);
                }
            }
            Phase::Turn => {}
        }
        self.x = self.spawn[0] + ((self.offset + self.attack[0]) >> 8);
        self.y = self.spawn[1] + (self.attack[1] >> 8);
        if contact(self.x, self.y, self.sprite.hitboxes()[0], c, ev) {
            return Ok(Some(false));
        }
        let anchor_off = !(i64::from(cx) - 128..=i64::from(cx) + 368)
            .contains(&i64::from(self.spawn[0]))
            || !(i64::from(cy) - 128..=i64::from(cy) + 288).contains(&i64::from(self.spawn[1]));
        let sx = self.x.wrapping_sub(cx) as i16;
        let sy = self.y.wrapping_sub(cy) as i16;
        if anchor_off && ((i32::from(sx) + 128) as u16 > 496 || !(-128..=288).contains(&sy)) {
            return Ok(Some(true));
        }
        ev.push(EnemyEvent::HomingTarget {
            x_q8: self.x * 256,
            y_q8: self.y * 256,
        });
        match f {
            Phase::Patrol => {
                self.cooldown = self.cooldown.saturating_sub(1);
                let dx = (c.player.q_world_x >> 8) - self.x;
                let dy = (c.player.q_world_y >> 8) - self.y;
                let ahead = if self.face_right { dx } else { -dx };
                if self.cooldown == 0 && (1..60).contains(&ahead) && (1..80).contains(&dy) {
                    self.phase = Phase::Dive;
                    self.sprite.set_animation(482, 1)?;
                    self.target = c.player.q_world_y;
                    self.velocity = [
                        ((c.player.q_world_x - self.x * 256) / 32) as i16,
                        ((c.player.q_world_y - self.y * 256) / 32) as i16,
                    ];
                } else if (!self.face_right && self.offset >> 8 <= self.left)
                    || (self.face_right && self.offset >> 8 >= self.right)
                {
                    self.phase = Phase::Turn;
                    self.sprite.set_animation(482, 2)?;
                }
                self.sprite.advance()?;
            }
            Phase::Turn => {
                if self.sprite.advance()?.ended {
                    self.face_right = !self.face_right;
                    self.sprite.set_flip(self.face_right, false);
                    self.phase = Phase::Patrol;
                    self.sprite.set_animation(482, 0)?;
                }
            }
            Phase::Dive => {
                if self.y * 256 > self.target {
                    self.phase = Phase::Return;
                }
                self.sprite.advance()?;
            }
            Phase::Return => {
                if self.attack[1] < 1 {
                    self.attack = [0; 2];
                    self.phase = Phase::Patrol;
                    self.cooldown = 60;
                    self.sprite.set_animation(482, 0)?;
                }
                self.sprite.advance()?;
            }
        }
        Ok(None)
    }
}
