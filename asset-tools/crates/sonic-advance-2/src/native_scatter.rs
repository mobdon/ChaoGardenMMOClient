//! The 32 original single-player scatter slots and their shared animated ring sprite.
use crate::{
    Error, VerifiedRom,
    native_animation::{AnimationPlayer, Hitbox},
    native_player::PlayerState,
    native_rings::{RingPosition, stage_ring_palette},
    native_stage::NativeStage,
};
#[derive(Debug, Clone, Copy, Default)]
pub struct ScatterFrame {
    pub stage_time: u32,
    pub timer_invulnerability: u16,
    pub inverted: bool,
    pub world_speed: [i32; 2],
    pub ceiling_collision: bool,
}
#[derive(Debug, Clone, Copy, Default)]
struct Ring {
    x: i32,
    y: i32,
    vx: i16,
    vy: i16,
    life: u16,
    layer: u8,
    flags: u16,
}
pub struct StageScatter<'a> {
    rings: [Ring; 32],
    sprite: AnimationPlayer<'a>,
    palette: [u16; 16],
    sine: &'a [u8],
    visible: Vec<RingPosition>,
}
impl<'a> StageScatter<'a> {
    pub fn new(rom: &'a VerifiedRom<'_>) -> Result<Self, Error> {
        let mut sprite = AnimationPlayer::new(rom, 726, 0)?;
        sprite.set_speed(32);
        Ok(Self {
            rings: [Ring::default(); 32],
            sprite,
            palette: stage_ring_palette(rom)?,
            sine: rom.range("rom", 0x97aa4, 0xa00)?,
            visible: Vec::with_capacity(32),
        })
    }
    fn sine(&self, index: usize) -> i32 {
        i32::from(i16::from_le_bytes([
            self.sine[2 * index],
            self.sine[2 * index + 1],
        ]))
    }
    /// Consume the shared original engine RNG once per newly allocated slot only.
    pub fn emit(
        &mut self,
        x: i32,
        y: i32,
        count: u16,
        layer: u8,
        inverted: bool,
        rng: &mut u32,
    ) -> Option<u16> {
        if count == 0 {
            return None;
        }
        let mut n = 0u16;
        let mut phase = 0x488i32;
        let (mut vx, mut vy) = (0, 0);
        for i in 0..self.rings.len() {
            if self.rings[i].life != 0 {
                continue;
            }
            if phase >= 0 {
                let exponent = phase >> 8;
                let shift = if exponent > 5 { 9 - exponent } else { exponent };
                let index = ((phase as usize) & 255) * 4;
                vx = self.sine(index) >> shift;
                vy = self.sine(index + 256) >> shift;
                vx -= vx >> 2;
                vy -= vy >> 2;
                phase = (phase + 16) | 128;
            }
            *rng = rng.wrapping_mul(0x196225).wrapping_add(0x3c6ef35f);
            self.rings[i] = Ring {
                x: x.wrapping_mul(256),
                y: y.wrapping_mul(256),
                vx: vx as i16,
                vy: (if inverted { -vy } else { vy }) as i16,
                life: 180,
                layer,
                flags: (if *rng & 0x10000 == 0 { 4 } else { 0 }) | (n & 3),
            };
            n += 1;
            phase = -phase;
            vx = -vx;
            if n >= count.min(32) {
                break;
            }
        }
        Some(118)
    }
    /// Active slots remain alive offscreen; the visible positions are cached before the bounce correction.
    pub fn sprites(&self) -> impl Iterator<Item = (i32, i32, &AnimationPlayer<'a>)> {
        self.visible.iter().map(|r| (r.x, r.y, &self.sprite))
    }
    pub fn tick(
        &mut self,
        stage: &NativeStage<'_>,
        p: &PlayerState,
        body: Hitbox,
        camera: [i32; 2],
        frame: ScatterFrame,
    ) -> Result<Vec<RingPosition>, Error> {
        self.sprite.advance()?;
        self.sprite.set_palette_bank(6, &self.palette)?;
        self.step(p, body, camera, frame, |x, y, layer, step| {
            let h = stage.vertical_sample(x, y, layer)?.extent;
            let r = y & 7;
            Ok(match h {
                0 => {
                    if step > 0 {
                        8 - r
                    } else {
                        r + 1
                    }
                }
                8 => {
                    if step > 0 {
                        !r
                    } else {
                        -8 | y
                    }
                }
                n if n > 0 => n - 1 - r,
                n => n + r,
            })
        })
    }
    fn step(
        &mut self,
        p: &PlayerState,
        body: Hitbox,
        camera: [i32; 2],
        frame: ScatterFrame,
        mut probe: impl FnMut(i32, i32, u8, i32) -> Result<i32, Error>,
    ) -> Result<Vec<RingPosition>, Error> {
        self.visible.clear();
        let mut collected = Vec::new();
        let px = p.q_world_x >> 8;
        let py = p.q_world_y >> 8;
        let [l, t, r, b] = body.rectangle.map(i32::from);
        for ring in &mut self.rings {
            if ring.life == 0 {
                continue;
            }
            ring.x = ring.x.wrapping_add(i32::from(ring.vx));
            ring.y = ring.y.wrapping_add(i32::from(ring.vy));
            if !frame.inverted {
                ring.x = ring.x.wrapping_add(frame.world_speed[0]);
                ring.y = ring.y.wrapping_add(frame.world_speed[1]);
            }
            let (x, y) = (ring.x >> 8, ring.y >> 8);
            if ring.life <= 148
                && (p.char_state != 20 || frame.timer_invulnerability == 0)
                && p.move_state & 0x80 == 0
                && px + l <= x + 8
                && px + r >= x - 8
                && py + t <= y
                && py + b >= y - 16
            {
                collected.push(RingPosition { x, y });
                ring.life = 0;
                continue;
            }
            if ring.flags & 7 == 0 {
                let bounce = |ring: &mut Ring, delta: i32, up: bool| {
                    if delta <= 0 {
                        let q = delta.wrapping_mul(256);
                        ring.y = if up {
                            ring.y.wrapping_sub(q)
                        } else {
                            ring.y.wrapping_add(q)
                        };
                        ring.vy = (ring.vy >> 2).wrapping_sub(ring.vy);
                    }
                };
                if frame.inverted {
                    if ring.vy < 0 {
                        bounce(ring, probe(x, y - 16, ring.layer, -8)?, true);
                    }
                    if frame.ceiling_collision && ring.vy > 0 {
                        bounce(ring, probe(x, y, ring.layer, 8)?, false);
                    }
                } else {
                    if ring.vy > 0 {
                        bounce(ring, probe(x, y, ring.layer, 8)?, false);
                    }
                    if frame.ceiling_collision && ring.vy < 0 {
                        bounce(ring, probe(x, y - 16, ring.layer, -8)?, true);
                    }
                }
            }
            ring.vy = ring.vy.wrapping_add(if frame.inverted { -18 } else { 18 });
            let (sx, sy) = (x.wrapping_sub(camera[0]), y.wrapping_sub(camera[1]));
            if (-7..248).contains(&sx)
                && (-7..168).contains(&sy)
                && (ring.life >= 32 || frame.stage_time & 2 == 0)
            {
                self.visible.push(RingPosition { x, y });
            }
            ring.flags = (ring.flags & !3) | ((ring.flags + 1) & 3);
            ring.life -= 1;
        }
        Ok(collected)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires verified A2NE_ROM for its sine table"]
    fn original_c_scatter_state_and_display_match_on_both_gravity_paths() {
        let bytes = std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap();
        let rom = VerifiedRom::parse(&bytes).unwrap();
        let mut packed = Vec::new();
        for options in 0..4 {
            let mut scatter = StageScatter::new(&rom).unwrap();
            let mut rng = 0x87654321;
            let (mut count, mut sum_x, mut sum_y, mut sound) = (0, 0, 0, 0);
            let inverted = options & 1 != 0;
            let mut p = PlayerState::new_at(crate::native_player::Character::Sonic, 0, 0);
            let body = Hitbox {
                index: 0,
                rectangle: [-9, -14, 8, 12],
            };
            for t in 0..220 {
                p.q_world_x = if t < 42 { 220 } else { 102 } * 256;
                p.q_world_y = if t < 42 { 220 } else { 96 } * 256;
                p.char_state = if t < 60 { 20 } else { 0 };
                p.move_state = if t == 65 { 128 } else { 0 };
                let spawn = match t {
                    0 => Some((100, 100, 48)),
                    6 => Some((110, 110, 4)),
                    120 => Some((102, 90, 8)),
                    2 => Some((1, 1, 0)),
                    _ => None,
                };
                if let Some((x, y, n)) = spawn {
                    sound += i32::from(scatter.emit(x, y, n, 1, inverted, &mut rng).is_some());
                }
                let frame = ScatterFrame {
                    stage_time: t,
                    timer_invulnerability: 1,
                    inverted,
                    world_speed: [37, -26],
                    ceiling_collision: options & 2 != 0,
                };
                let got = scatter
                    .step(&p, body, [5, 7], frame, |_, y, _, dir| {
                        Ok(if dir > 0 { 140 - y } else { y - 25 })
                    })
                    .unwrap();
                for r in got {
                    count += 1;
                    sum_x += r.x;
                    sum_y += r.y;
                }
                let mut emit = |v: i32| packed.extend(v.to_le_bytes());
                for v in [
                    rng as i32,
                    count,
                    sum_x,
                    sum_y,
                    scatter.visible.len() as i32,
                    scatter.visible.iter().map(|p| p.x - 5).sum(),
                    scatter.visible.iter().map(|p| p.y - 7).sum(),
                    sound,
                ] {
                    emit(v);
                }
                for r in &scatter.rings {
                    for v in [
                        r.x,
                        r.y,
                        i32::from(r.vx),
                        i32::from(r.vy),
                        i32::from(r.life),
                        i32::from(r.layer),
                        i32::from(r.flags),
                    ] {
                        emit(v);
                    }
                }
            }
        }
        assert_eq!(
            crate::gba::sha256(&packed),
            "c13eb936ce9556b362f560dc1225d0b743fac84321b8250091f92a2ce7d8d9ab"
        );
    }
}
