//! Normal-gravity single-player Sonic spring tasks, using native OBJ hitboxes.
use crate::{
    entities::{Placement, StageEntities},
    player::Player,
    sprite::{AnimationSource, Hitbox, SpritePlayer},
    Error, Result,
};

#[derive(Clone, Copy, Debug)]
pub struct SpringContext {
    pub camera: (i32, i32),
    pub min_y: i32,
    pub max_y: i32,
    pub ia_override: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpringLaunch {
    pub sound: u16,
    pub character_state: Option<u8>,
    pub record_offset: usize,
}

#[derive(Clone, Debug)]
struct Spring {
    p: Placement,
    sprite: Option<SpritePlayer>,
    animating: bool,
}

#[derive(Clone, Debug)]
pub struct StageSprings {
    springs: Vec<Spring>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Face {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Clone, Copy)]
struct Bounds {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl Bounds {
    fn from_hitbox(p: &Placement, h: Hitbox) -> Self {
        Self {
            left: p.x + i32::from(h.left),
            top: p.y + i32::from(h.top),
            right: p.x + i32::from(h.right),
            bottom: p.y + i32::from(h.bottom),
        }
    }
    fn touches(self, player: &Player, half_x: i32, half_y: i32) -> bool {
        let (x, y) = (player.x >> 8, player.y >> 8);
        self.left <= x + half_x
            && self.right >= x - half_x
            && self.top <= y + half_y
            && self.bottom >= y - half_y
    }
}

fn vertical_contact(bounds: Bounds, player: &mut Player, ctx: SpringContext) -> Option<Face> {
    let half = player.half_height;
    if !bounds.touches(player, 6, half) {
        return None;
    }
    let top = player.y >> 8 <= (bounds.top + bounds.bottom) >> 1;
    let target = if top {
        bounds.top - half
    } else {
        bounds.bottom + half
    };
    let min = ctx.min_y * 256;
    let max = ctx.max_y * 256;
    let anchored = target * 256 + (player.y & 255);
    player.y = anchored.clamp(min, max);
    if !(min..max).contains(&anchored) {
        return None;
    }
    if top {
        if player.vy < 0 {
            return None;
        }
        player.vy = 0;
        player.grounded = true;
        player.ground_speed = player.vx;
        player.angle = 0;
        Some(Face::Top)
    } else {
        if player.vy <= 0 {
            player.vy = 0;
        }
        // The native helper publishes flag 0x10000 even for positive air Y.
        Some(Face::Bottom)
    }
}

// Grounded normal-platform contact checks the wider, shorter side box first.
// Sideways spring tasks compare to the entity anchor, not the hitbox midpoint.
fn side_contact(
    bounds: Bounds,
    split_x: i32,
    player: &mut Player,
    airborne_snap: bool,
) -> Option<Face> {
    if !bounds.touches(player, 11, player.half_height - 1) {
        return None;
    }
    if player.x >> 8 <= split_x && player.vx >= 0 {
        if player.vx > 0 || airborne_snap {
            player.x = (bounds.left - 11) * 256;
        }
        Some(Face::Left)
    } else if player.x >> 8 > split_x && player.vx <= 0 {
        if player.vx < 0 || airborne_snap {
            player.x = (bounds.right + 12) * 256;
        }
        Some(Face::Right)
    } else {
        None
    }
}

fn platform_contact(bounds: Bounds, player: &mut Player, ctx: SpringContext) -> Option<Face> {
    if !player.grounded {
        if let Some(face) = vertical_contact(bounds, player, ctx) {
            return Some(face);
        }
    }
    if player.angle.wrapping_add(32) & 64 == 0 {
        if let Some(face) = side_contact(bounds, (bounds.left + bounds.right) >> 1, player, false) {
            player.ground_speed = 0;
            player.vx = 0;
            return Some(face);
        }
    }
    vertical_contact(bounds, player, ctx)
}

fn impulse(data: [u8; 4]) -> i32 {
    if data[0] as i8 >= 4 {
        i32::from(data[1]) << 4
    } else {
        [1920, 2304, 2688, 3072][usize::from(data[0] & 3)]
    }
}

fn contact(
    p: &Placement,
    sprite: &SpritePlayer,
    player: &mut Player,
    ctx: SpringContext,
) -> Option<SpringLaunch> {
    let speed = impulse(p.data);
    let state = if p.data[0] & 1 == 0 { 17 } else { 21 };
    let mut character_state = None;
    match p.kind {
        8 | 9 => {
            if p.kind == 8 && ctx.ia_override {
                return None;
            }
            let bounds = Bounds::from_hitbox(p, sprite.hitboxes[0]?);
            match (p.kind, platform_contact(bounds, player, ctx)?) {
                (8, Face::Top) => {
                    player.apply_spring_state(state);
                    player.vy = -speed;
                    character_state = Some(state);
                }
                (9, Face::Bottom) => {
                    player.apply_down_spring_state();
                    player.vy = speed;
                }
                _ => return None,
            }
        }
        10 | 11 => {
            let bounds = Bounds::from_hitbox(p, sprite.hitboxes[0]?);
            let airborne = !player.grounded;
            let body_contact = bounds.touches(player, 6, player.half_height);
            if airborne && body_contact && vertical_contact(bounds, player, ctx).is_some() {
                return None;
            }
            let side = side_contact(bounds, p.x, player, airborne && !body_contact)?;
            let direction = match (p.kind, side) {
                (10, Face::Left) => -1,
                (11, Face::Right) => 1,
                _ => {
                    if (side == Face::Left && player.vx > 0)
                        || (side == Face::Right && player.vx < 0)
                    {
                        player.vx = 0;
                        player.ground_speed = 0;
                    }
                    return None;
                }
            };
            player.vx = direction * speed;
            player.ground_speed = player.vx;
            player.facing_left = direction < 0;
            player.angle = 0;
            player.apply_side_spring_state();
        }
        12..=15 => {
            let direction = sprite.hitboxes[1].and_then(|h| {
                if !Bounds::from_hitbox(p, h).touches(player, 12, player.half_height + 1) {
                    return None;
                }
                let dx = (player.x >> 8) - p.x;
                if matches!(p.kind, 12 | 14) && dx < -20 {
                    Some(-1)
                } else if dx > 20 {
                    Some(1)
                } else {
                    None
                }
            });
            let Some(direction) = direction else {
                if let Some(h) = sprite.hitboxes[0] {
                    platform_contact(Bounds::from_hitbox(p, h), player, ctx);
                }
                return None;
            };
            player.apply_spring_state(state);
            player.vx = direction * speed;
            player.vy = -speed;
            character_state = Some(state);
        }
        _ => return None,
    }
    Some(SpringLaunch {
        sound: 170,
        character_state,
        record_offset: p.record_offset,
    })
}

impl StageSprings {
    pub fn new(entities: &StageEntities) -> Self {
        Self {
            springs: entities
                .interactables
                .records
                .iter()
                .filter(|p| (8..=15).contains(&p.kind))
                .cloned()
                .map(|p| Spring {
                    p,
                    sprite: None,
                    animating: false,
                })
                .collect(),
        }
    }
    pub fn visible_sprites(&self) -> impl Iterator<Item = (i32, i32, &SpritePlayer)> {
        self.springs
            .iter()
            .filter_map(|s| s.sprite.as_ref().map(|sprite| (s.p.x, s.p.y, sprite)))
    }
    pub fn tick(
        &mut self,
        source: &impl AnimationSource,
        player: &mut Player,
        ctx: SpringContext,
    ) -> Result<Vec<SpringLaunch>> {
        if ctx.min_y >= ctx.max_y
            || ctx.min_y < i32::MIN / 256
            || ctx.max_y > i32::MAX / 256
            || !(1..=127).contains(&player.half_height)
        {
            return Err(Error("invalid spring collision bounds".into()));
        }
        let mut events = Vec::new();
        for spring in &mut self.springs {
            let p = &spring.p;
            if !(0..=65535).contains(&p.x) || !(0..=65535).contains(&p.y) {
                return Err(Error("invalid spring world anchor".into()));
            }
            let in_range = (-128..=368).contains(&(i64::from(p.x) - i64::from(ctx.camera.0)))
                && (-128..=288).contains(&(i64::from(p.y) - i64::from(ctx.camera.1)));
            let idle = (p.kind - 8) & !1;
            if spring.sprite.is_none() {
                if !in_range {
                    continue;
                }
                let mut s = SpritePlayer::new(455, idle);
                s.flip_x = matches!(p.kind, 10 | 12 | 14);
                s.flip_y = p.kind == 9;
                s.priority = 2;
                s.oam_order = 18;
                s.tick(source)?;
                if p.kind == 8 && p.data[0] & 2 == 0 {
                    s.set_palette_override(source, 36, 8, 1)?;
                }
                spring.sprite = Some(s);
            }
            let sprite = spring.sprite.as_mut().expect("spring initialized above");
            let was_animating = spring.animating;
            if let Some(event) = contact(p, sprite, player, ctx) {
                if p.kind <= 11 {
                    sprite.restart_variant(idle + 1);
                } else {
                    sprite.variant = idle + 1;
                }
                spring.animating = true;
                events.push(event);
            }
            if !in_range {
                spring.sprite = None;
                spring.animating = false;
                continue;
            }
            if was_animating && sprite.tick(source)?.ended {
                sprite.variant = idle;
                sprite.tick(source)?;
                spring.animating = false;
            }
        }
        Ok(events)
    }
}
