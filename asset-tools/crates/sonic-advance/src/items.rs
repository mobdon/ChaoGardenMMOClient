//! Single-player SA1 item monitors. Only the icon rises after the case breaks.
use crate::{
    binary,
    entities::StageEntities,
    player::{PlatformContact, Player, HALF_WIDTH, ONE},
    sprite::{Hitbox, SpriteBank, SpritePlayer},
    stage::Stage,
    Error, Result,
};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemGrant {
    Life,
    Shield,
    MagneticShield,
    Invincibility,
    SpeedUp,
    RandomRings,
    Rings(u8),
}
impl ItemGrant {
    pub fn random_ring_count(draw: u32) -> u8 {
        [1, 5, 10, 20, 30, 40][(draw % 6) as usize]
    }
    fn from_kind(kind: u8) -> Option<Self> {
        Some(match kind {
            0 => Self::Life,
            1 => Self::Shield,
            2 => Self::MagneticShield,
            3 => Self::Invincibility,
            4 => Self::SpeedUp,
            5 => Self::RandomRings,
            6 => Self::Rings(5),
            7 => Self::Rings(10),
            _ => return None,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemEvent {
    Break,
    Grant(ItemGrant),
}
#[derive(Clone, Copy, Debug)]
enum Phase {
    Intact,
    Rising(u8),
    Awarded(u8),
    Gone,
}
#[derive(Clone, Debug)]
struct Monitor {
    x: i32,
    y: i32,
    offset: i32,
    bump: Option<i32>,
    phase: Phase,
    kind: u8,
}
#[derive(Clone, Debug)]
pub struct ItemBoxes {
    boxes: Vec<Monitor>,
    body: Arc<SpritePlayer>,
    icons: Arc<Vec<SpritePlayer>>,
    bounds: Hitbox,
    standing: Option<usize>,
    dust: Vec<(i32, i32, SpritePlayer)>,
}
impl ItemBoxes {
    pub fn new(rom: &[u8], entities: &StageEntities, bank: &SpriteBank<'_>) -> Result<Self> {
        binary::verify_asoe(rom)?;
        let mut body = SpritePlayer::new(705, 0);
        body.tick(bank)?;
        body.oam_order = 18;
        let bounds = body.hitboxes[0]
            .ok_or_else(|| Error("item monitor is missing its native bounds".into()))?;
        let mut icons = Vec::new();
        for &variant in binary::range(rom, 0xb84e0, 11)? {
            let mut icon = SpritePlayer::new(706, variant);
            icon.tick(bank)?;
            icon.oam_order = 19;
            icons.push(icon);
        }
        let mut boxes = Vec::new();
        for p in &entities.item_boxes.records {
            if usize::from(p.kind) >= icons.len()
                || !(0..=0xffff).contains(&p.x)
                || !(0..=0xffff).contains(&p.y)
            {
                return Err(Error("invalid item box placement".into()));
            }
            boxes.push(Monitor {
                x: p.x,
                y: p.y,
                offset: 0,
                bump: None,
                kind: p.kind,
                phase: Phase::Intact,
            });
        }
        Ok(Self {
            bounds,
            boxes,
            body: Arc::new(body),
            icons: Arc::new(icons),
            standing: None,
            dust: Vec::new(),
        })
    }
    pub fn visible_sprites(&self) -> impl Iterator<Item = (i32, i32, &SpritePlayer)> {
        self.boxes
            .iter()
            .flat_map(|m| {
                let y = m.y + m.offset;
                let icon = (!matches!(m.phase, Phase::Gone | Phase::Rising(0))).then_some((
                    m.x,
                    y,
                    &self.icons[m.kind as usize],
                ));
                let body = matches!(m.phase, Phase::Intact).then_some((m.x, y, self.body.as_ref()));
                [icon, body].into_iter().flatten()
            })
            .chain(self.dust.iter().map(|(x, y, s)| (*x, *y, s)))
    }
    pub fn tick(
        &mut self,
        p: &mut Player,
        actor: &SpritePlayer,
        stage: &Stage<'_>,
        bank: &SpriteBank<'_>,
    ) -> Result<Vec<ItemEvent>> {
        if ![
            crate::player::SPIN_HALF_HEIGHT,
            crate::player::STANDING_HALF_HEIGHT,
        ]
        .contains(&p.half_height)
            || p.collision_layer > 1
        {
            return Err(Error("invalid player geometry for item box contact".into()));
        }
        let mut events = Vec::new();
        for (_, _, sprite) in &mut self.dust {
            sprite.tick(bank)?;
        }
        self.dust.retain(|(_, _, s)| !s.ended);
        for (index, m) in self.boxes.iter_mut().enumerate() {
            match m.phase {
                Phase::Gone => continue,
                Phase::Awarded(t) => {
                    m.phase = if t == 30 {
                        Phase::Gone
                    } else {
                        Phase::Awarded(t + 1)
                    };
                    continue;
                }
                Phase::Rising(t) => {
                    m.offset -= 1;
                    if t == 59 {
                        if let Some(grant) = ItemGrant::from_kind(m.kind) {
                            events.push(ItemEvent::Grant(grant));
                        }
                        m.phase = Phase::Awarded(0);
                    } else {
                        m.phase = Phase::Rising(t + 1);
                    }
                    continue;
                }
                Phase::Intact => {}
            }
            if let Some(v) = m.bump {
                m.offset += v >> 8;
                m.bump = Some(v + 40);
                let y = m.y + m.offset;
                let sample = stage
                    .collision_at(
                        m.x.clamp(0, stage.width_px() as i32 - 1),
                        y.clamp(0, stage.height_px() as i32 - 1),
                        1,
                    )?
                    .ok_or_else(|| Error("missing box bump terrain".into()))?;
                let local = y & 7;
                let d = match sample.vertical_height {
                    0 => 8 - local,
                    8 => -1 - local,
                    h if h > 0 => i32::from(h) - 1 - local,
                    h => i32::from(h) + local,
                };
                if d < 0 {
                    m.offset += d;
                    m.bump = None;
                }
            }
            let y = m.y + m.offset;
            let stood = self.standing == Some(index) && p.grounded;
            if self.standing == Some(index) {
                self.standing = None;
            }
            let attack = actor.hitboxes[1];
            if stood || attack.is_none() {
                match p.platform_contact(stage, m.x, y, self.bounds, stood)? {
                    PlatformContact::Top => {
                        self.standing = Some(index);
                        p.object_support = true;
                    }
                    PlatformContact::Underside => {
                        m.bump = Some(-ONE);
                    }
                    _ if stood => p.grounded = false,
                    _ => {}
                }
                continue;
            }
            let attack = attack.unwrap();
            let px = p.x >> 8;
            let py = p.y >> 8;
            let l = m.x + i32::from(self.bounds.left);
            let r = m.x + i32::from(self.bounds.right);
            let t = y + i32::from(self.bounds.top);
            let b = y + i32::from(self.bounds.bottom);
            let overlap = |left, right, top, bottom| {
                px + right >= l && px + left <= r && py + bottom >= t && py + top <= b
            };
            let mut destroy = overlap(
                i32::from(attack.left),
                i32::from(attack.right),
                i32::from(attack.top),
                i32::from(attack.bottom),
            );
            if destroy && !p.grounded && p.vy > 0 {
                p.vy = -p.vy;
            }
            if !p.grounded {
                if overlap(-HALF_WIDTH, HALF_WIDTH, -p.half_height, p.half_height) {
                    if py <= (t + b) >> 1 {
                        if p.vy >= 0 {
                            destroy = true;
                            p.vy = -p.vy;
                        }
                    } else if p.vy < 0 {
                        p.vy = 0;
                        p.y = (b + p.half_height) * ONE + (p.y & 255);
                        m.bump = Some(-ONE);
                        continue;
                    }
                } else if overlap(
                    -HALF_WIDTH - 5,
                    HALF_WIDTH + 5,
                    1 - p.half_height,
                    p.half_height - 1,
                ) {
                    if px <= (l + r) >> 1 && p.vx > 0 {
                        p.vx = 0;
                        p.x = (l - HALF_WIDTH - 5) * ONE;
                    } else if px > (l + r) >> 1 && p.vx < 0 {
                        p.vx = 0;
                        p.x = (r + HALF_WIDTH + 6) * ONE;
                    }
                }
            }
            if destroy {
                events.push(ItemEvent::Break);
                m.phase = Phase::Rising(0);
                let mut dust = SpritePlayer::new(407, 0);
                dust.oam_order = 15;
                dust.tick(bank)?;
                self.dust.push((m.x, y, dust));
            }
        }
        Ok(events)
    }
}
