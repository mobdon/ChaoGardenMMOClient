//! Ordinary single-player special ring tasks, in native task creation order.
use crate::{
    Error, VerifiedRom,
    native_animation::{AnimationPlayer, Hitbox},
    native_entities::StageEntities,
    native_player::{Character, PlayerState},
};

struct Ring<'a> {
    index: usize,
    x: i32,
    y: i32,
    anim: AnimationPlayer<'a>,
    collected: bool,
    ended: bool,
    submitted: bool,
}
pub struct StageSpecialRings<'a> {
    rom: &'a VerifiedRom<'a>,
    active: Vec<Ring<'a>>,
    pub count: u8,
    cream_targets: Vec<(i32, i32)>,
}
fn outside(x: i32, y: i32, cx: i32, cy: i32) -> bool {
    x.wrapping_add(128).wrapping_sub(cx) as u16 > 496
        || y.wrapping_add(128).wrapping_sub(cy) as u16 > 416
}
fn touching(x: i32, y: i32, h: Hitbox, p: &PlayerState, cheese_q: Option<[i32; 2]>) -> bool {
    if p.move_state & 0x80 != 0 {
        return false;
    }
    let [l, t, r, b] = h.rectangle.map(i32::from);
    let (px, py) = (p.q_world_x >> 8, p.q_world_y >> 8);
    let (w, v) = (i32::from(p.sprite_offset_x), i32::from(p.sprite_offset_y));
    let (sx, sy) = (x, y);
    if h.index != -1 && px + w >= sx + l && px - w <= sx + r && py + v >= sy + t && py - v <= sy + b
    {
        return true;
    }
    cheese_q.is_some_and(|[qx, qy]| {
        ((qx >> 8) + 16 - x) as u16 <= 32 && ((qy >> 8) + 32 - y) as u16 <= 32
    })
}
impl<'a> StageSpecialRings<'a> {
    pub fn new(rom: &'a VerifiedRom<'a>) -> Self {
        Self {
            rom,
            active: Vec::new(),
            count: 0,
            cream_targets: Vec::new(),
        }
    }
    pub fn tick(
        &mut self,
        entities: &mut StageEntities,
        p: &PlayerState,
        cx: i32,
        cy: i32,
        cheese_q: Option<[i32; 2]>,
    ) -> Result<Vec<u16>, Error> {
        for index in entities.take_new_special_rings() {
            let e = entities.placements()[index];
            self.active.push(Ring {
                index,
                x: e.x,
                y: e.y,
                anim: AnimationPlayer::new(self.rom, 730, 0)?,
                collected: false,
                ended: false,
                submitted: false,
            });
        }
        let mut sounds = Vec::new();
        self.cream_targets.clear();
        let mut i = 0;
        while i < self.active.len() {
            let ring = &mut self.active[i];
            ring.submitted = false;
            if !ring.collected {
                if p.character == Character::Cream {
                    self.cream_targets.push((ring.x, ring.y));
                }
                if touching(ring.x, ring.y, ring.anim.hitboxes()[0], p, cheese_q) {
                    self.count = self.count.wrapping_add(1);
                    ring.collected = true;
                    ring.anim.set_animation(730, 1)?;
                    sounds.push(298);
                    i += 1;
                    continue;
                }
            }
            if ring.ended || outside(ring.x, ring.y, cx, cy) {
                if !ring.collected {
                    entities.restore_special_ring(ring.index);
                }
                self.active.remove(i);
                continue;
            }
            ring.ended = ring.anim.advance()?.ended;
            ring.submitted = true;
            i += 1;
        }
        Ok(sounds)
    }
    pub fn sprites(&self) -> impl DoubleEndedIterator<Item = (i32, i32, &AnimationPlayer<'a>)> {
        self.active
            .iter()
            .filter(|r| r.submitted)
            .map(|r| (r.x, r.y, &r.anim))
    }
    /// World-space integer coordinates, before contact/cull, for Cream only.
    pub fn cream_targets(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        self.cream_targets.iter().copied()
    }
}
