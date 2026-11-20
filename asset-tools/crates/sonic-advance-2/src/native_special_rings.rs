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
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn special_ring_source_collision_and_cull() {
        let mut data = Vec::new();
        let (sx, sy) = (48000, 42000);
        for mode in 0..8 {
            for y in -48..=48 {
                for x in -32..=32 {
                    let mut p = PlayerState::new_at(
                        Character::Sonic,
                        (sx + x) * 256 + 127,
                        (sy + y) * 256 + 225,
                    );
                    p.move_state = if mode & 1 != 0 { 128 } else { 0 };
                    p.sprite_offset_x = 6;
                    p.sprite_offset_y = 14;
                    let h = Hitbox {
                        index: if mode & 2 != 0 { -1 } else { 0 },
                        rectangle: [-16, -28, 16, 4],
                    };
                    let cheese = [
                        (sx + if mode & 4 != 0 { x } else { 200 }) * 256,
                        (sy + y) * 256,
                    ];
                    data.extend([
                        u8::from(touching(sx, sy, h, &p, Some(cheese))),
                        u8::from(outside(sx, sy, sx - x * 16, sy - y * 12)),
                    ]);
                }
            }
        }
        assert_eq!(
            crate::gba::sha256(&data),
            "bdd417444275ceeaea2bf5667dd90c5dd7a9d7d9ae15c05fa135d7254a20d8bd"
        );
    }
    #[test]
    #[ignore = "requires local A2NE_ROM"]
    fn special_ring_course_lifecycle() {
        let bytes = std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap();
        let rom = VerifiedRom::parse(&bytes).unwrap();
        for id in (0..26).filter(|id| id % 4 < 2) {
            let mut e = StageEntities::load(&rom, id).unwrap();
            let places: Vec<_> = e
                .placements()
                .iter()
                .filter(|e| e.kind == 103)
                .copied()
                .collect();
            assert_eq!(places.len(), 7, "course {id}");
            let mut route = StageSpecialRings::new(&rom);
            for (n, pos) in places.iter().enumerate() {
                let mut p = PlayerState::new_at(Character::Sonic, pos.x * 256, pos.y * 256);
                let mut cues = Vec::new();
                for _ in 0..2 {
                    e.update_layers(&mut p, pos.x, pos.y);
                    cues.extend(route.tick(&mut e, &p, pos.x, pos.y, None).unwrap());
                }
                assert_eq!(cues, [298]);
                assert_eq!(usize::from(route.count), n + 1);
            }
            for pos in places {
                let (x, y) = (pos.x, pos.y);
                let mut rings = StageSpecialRings::new(&rom);
                let mut p = PlayerState::new_at(Character::Cream, x * 256, y * 256);
                // Fresh independent management state before each targeted approach.
                e = StageEntities::load(&rom, id).unwrap();
                let mut run = |rings: &mut StageSpecialRings, p: &mut PlayerState, cx, cy| {
                    e.update_layers(p, cx, cy);
                    rings.tick(&mut e, p, cx, cy, None).unwrap()
                };
                assert!(run(&mut rings, &mut p, x, y).is_empty());
                assert!(
                    rings.sprites().any(|(a, b, s)| (a, b) == (x, y)
                        && s.hitboxes()[0].rectangle == [-16, -28, 16, 4])
                );
                p.move_state |= 128;
                run(&mut rings, &mut p, x + 800, y);
                assert!(!rings.sprites().any(|(a, b, _)| (a, b) == (x, y)));
                run(&mut rings, &mut p, x, y);
                assert!(rings.sprites().any(|(a, b, _)| (a, b) == (x, y)));
                p.move_state &= !128;
                assert_eq!(run(&mut rings, &mut p, x, y), [298]);
                assert_eq!(rings.count, 1);
                assert!(rings.cream_targets().any(|v| v == (x, y)));
                assert!(!rings.sprites().any(|(a, b, _)| (a, b) == (x, y)));
                for i in 0..18 {
                    assert!(run(&mut rings, &mut p, x, y).is_empty());
                    let s = rings.sprites().find(|(a, b, _)| (*a, *b) == (x, y));
                    if i < 17 {
                        assert_eq!(
                            s.unwrap().2.render().unwrap().frame_index,
                            Some(8 + (i / 4).min(3))
                        );
                    } else {
                        assert!(s.is_none());
                    }
                }
                run(&mut rings, &mut p, x + 800, y);
                run(&mut rings, &mut p, x, y);
                assert!(!rings.sprites().any(|(a, b, _)| (a, b) == (x, y)));
                assert_eq!(rings.count, 1);
            }
        }
    }
    #[test]
    #[ignore = "requires local A2NE_ROM"]
    fn special_ring_hud_two_full_cycles() {
        let bytes = std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap();
        let rom = VerifiedRom::parse(&bytes).unwrap();
        let mut hud = crate::native_hud::StageHud::new(&rom).unwrap();
        let mut data = Vec::new();
        for t in 0..508 {
            hud.tick(0, 0, t);
            hud.set_special_count((t % 8) as u8);
            hud.draw(0, t, 1, 3, 0, |f, x, y| {
                if y - i32::from(f.origin_y) == 31 {
                    data.extend((x - i32::from(f.origin_x)).to_le_bytes());
                    data.extend(31_i32.to_le_bytes());
                    data.extend(&f.rgba);
                }
            });
        }
        assert_eq!(
            crate::gba::sha256(&data),
            "c1c8334e1005785b6e69ab6f96b5edcf28602af5721e24834d16f690a728c32a"
        );
    }
}
