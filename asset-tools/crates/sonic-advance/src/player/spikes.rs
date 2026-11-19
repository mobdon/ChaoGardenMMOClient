use super::*;
use crate::{
    sprite::{Hitbox, RenderedSprite, SpriteBank, SpritePlayer},
    stage::Stage,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpikeContact {
    None,
    Side,
    Underside,
    Damage,
}

#[derive(Clone, Debug)]
pub struct UprightSpikes {
    bounds: Hitbox,
    image: RenderedSprite,
}

impl UprightSpikes {
    /// The normal-up sprite installs hitbox 0 on the first animation tick.
    pub fn from_bank(bank: &SpriteBank<'_>) -> Result<Self> {
        let mut sprite = SpritePlayer::new(454, 2);
        sprite.tick(bank)?;
        let bounds =
            sprite.hitboxes[0].ok_or_else(|| Error("missing upright spike hitbox".into()))?;
        let image = bank
            .render(&sprite)?
            .ok_or_else(|| Error("missing upright spike frame".into()))?;
        Ok(Self { bounds, image })
    }
    pub fn rendered(&self) -> &RenderedSprite {
        &self.image
    }

    pub fn contact(
        &self,
        p: &mut Player,
        stage: &Stage<'_>,
        x: i32,
        y: i32,
    ) -> Result<SpikeContact> {
        if !(0..=stage.width_px() as i32).contains(&x)
            || !(0..=stage.height_px() as i32).contains(&y)
            || ![SPIN_HALF_HEIGHT, STANDING_HALF_HEIGHT].contains(&p.half_height)
            || p.collision_layer > 1
        {
            return Err(Error("invalid spike placement or collision layer".into()));
        }
        match p.platform_contact(stage, x, y, self.bounds, false)? {
            PlatformContact::Top => {}
            PlatformContact::None => return Ok(SpikeContact::None),
            PlatformContact::Side => return Ok(SpikeContact::Side),
            PlatformContact::Underside => return Ok(SpikeContact::Underside),
        }
        if p.invulnerable_to_damage() {
            return Ok(SpikeContact::None);
        }
        p.invulnerability_frames = 120;
        p.vx = if p.vx < 0 || (p.vx == 0 && p.facing_left) {
            384
        } else {
            -384
        };
        p.vy = -768;
        p.grounded = false;
        p.hurt = true;
        p.spin.mode = SpinMode::Standing;
        p.spin.roll_jump = false;
        p.spring_state = None;
        p.jump_cut = false;
        Ok(SpikeContact::Damage)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlatformContact {
    None,
    Side,
    Underside,
    Top,
}
impl Player {
    pub(crate) fn platform_contact(
        &mut self,
        stage: &Stage<'_>,
        x: i32,
        y: i32,
        bounds: Hitbox,
        already_standing: bool,
    ) -> Result<PlatformContact> {
        let p = self;
        let l = x + i32::from(bounds.left);
        let r = x + i32::from(bounds.right);
        let t = y + i32::from(bounds.top);
        let b = y + i32::from(bounds.bottom);
        let px = p.x >> 8;
        let py = p.y >> 8;
        let overlap =
            |rx: i32, ry: i32| px + rx >= l && px - rx <= r && py + ry >= t && py - ry <= b;
        let body = overlap(HALF_WIDTH, p.half_height);
        if (!body || (p.grounded && !already_standing))
            && p.angle.wrapping_add(32) & 64 == 0
            && overlap(HALF_WIDTH + 5, p.half_height - 1)
        {
            let left = px <= (l + r) >> 1;
            let near = l - HALF_WIDTH - 5;
            let far = r + HALF_WIDTH + 6;
            let (target, alternate, dir) = if left {
                (near, far, ContactMode::Left)
            } else {
                (far, near, ContactMode::Right)
            };
            let hit = stage.terrain_sensor(target, py, dir, p.collision_layer, false)?;
            p.x = if hit.distance < 0 { alternate } else { target } * ONE;
            if (left && p.vx >= 0) || (!left && p.vx <= 0) {
                if p.grounded && p.vx != 0 {
                    p.ground_speed = 0;
                    p.spin.mode = SpinMode::Standing;
                    p.facing_left = !left;
                    p.shift_radius(STANDING_HALF_HEIGHT);
                }
                p.vx = 0;
            }
            return Ok(PlatformContact::Side);
        }
        if !body {
            return Ok(PlatformContact::None);
        }
        if py > (t + b) >> 1 {
            p.y = (b + p.half_height) * ONE + (p.y & 255);
            if p.vy <= 0 {
                p.vy = 0;
            }
            return Ok(PlatformContact::Underside);
        }
        p.y = (t - p.half_height) * ONE + (p.y & 255);
        if p.vy < 0 {
            return Ok(PlatformContact::None);
        }
        p.vy = 0;
        p.grounded = true;
        if !already_standing {
            p.angle = 0;
            p.jumping = false;
            p.jump_cut = false;
            p.spring_state = None;
            p.spin.mode = SpinMode::Standing;
            p.spin.roll_jump = false;
            p.shift_radius(STANDING_HALF_HEIGHT);
            p.ground_speed = p.vx;
        }
        Ok(PlatformContact::Top)
    }
}
