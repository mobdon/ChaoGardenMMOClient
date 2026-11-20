//! Native enemy placements, Mon's four callbacks, and ordinary SA2 enemy contact.
#[path = "native_buzzer.rs"]
mod buzzer;
use crate::{
    Error, VerifiedRom,
    native_animation::{AnimationPlayer, Hitbox},
    native_entities::Interactable,
    native_lifecycle::NativeLifecycle,
    native_placements::{RegionLayout, stage_stream},
    native_player::PlayerState,
    native_stage::NativeStage,
};
use buzzer::Buzzer;
use serde::Serialize;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum EnemyEvent {
    Transition(u8),
    Sound(u16),
    ScatterRings { x: i32, y: i32, count: u16 },
    Defeated { x: i32, y: i32, score_index: u8 },
    HomingTarget { x_q8: i32, y_q8: i32 },
    ExtraLifeMusic,
}
pub struct EnemyContext<'a> {
    pub player: &'a mut PlayerState,
    pub body: [Hitbox; 2],
    pub lifecycle: &'a mut NativeLifecycle,
    pub rings: &'a mut u16,
    pub score: &'a mut u32,
    pub lives: &'a mut u8,
    pub timer_invincibility: u16,
}
/// Original Coll_DamagePlayer entry for a single player in a regular course.
/// Hurt movement initializes on the next player transition, not here.
pub fn damage_player(c: &mut EnemyContext<'_>, events: &mut Vec<EnemyEvent>) -> bool {
    if c.timer_invincibility != 0 || c.lifecycle.timer_invulnerability != 0 {
        return false;
    }
    c.lifecycle.timer_invulnerability = 120;
    let p = &mut *c.player;
    if p.move_state & 0x1000000 != 0 {
        p.layer = 1;
        p.move_state &= !0x1000000;
        p.sprite_frame_flags = (p.sprite_frame_flags & !0x3000) | 0x2000;
    }
    c.lifecycle.item_effect &= !0x80;
    events.push(EnemyEvent::Transition(9));
    if c.lifecycle.item_effect & 9 != 0 {
        events.push(EnemyEvent::Sound(119));
        c.lifecycle.item_effect &= !9;
    } else if *c.rings != 0 {
        events.push(EnemyEvent::ScatterRings {
            x: p.q_world_x >> 8,
            y: p.q_world_y >> 8,
            count: *c.rings,
        });
        *c.rings = 0;
    } else if c.lifecycle.stage_flags & 0x40 == 0 {
        p.move_state |= 0x80;
    }
    true
}
fn overlap(x: i32, y: i32, a: Hitbox, px: i32, py: i32, b: Hitbox) -> bool {
    let [al, at, ar, ab] = a.rectangle.map(i32::from);
    let [bl, bt, br, bb] = b.rectangle.map(i32::from);
    a.index != -1
        && b.index != -1
        && x + al <= px + br
        && x + ar >= px + bl
        && y + at <= py + bb
        && y + ab >= py + bt
}
fn contact(
    x: i32,
    y: i32,
    enemy: Hitbox,
    c: &mut EnemyContext<'_>,
    events: &mut Vec<EnemyEvent>,
) -> bool {
    let p = &mut *c.player;
    if p.move_state & (0x80 | 0x40000000) != 0 {
        return false;
    }
    let (px, py) = (p.q_world_x >> 8, p.q_world_y >> 8);
    let attack = overlap(x, y, enemy, px, py, c.body[1]);
    if attack {
        if p.move_state & 0x2000000 != 0 {
            p.q_speed_air_x = 0;
            p.q_speed_air_y = 0;
            events.push(EnemyEvent::Transition(8));
        } else if p.q_speed_air_y > 0 {
            p.q_speed_air_y = -p.q_speed_air_y;
        }
        p.move_state |= 0x4000;
    } else if !overlap(x, y, enemy, px, py, c.body[0]) {
        return false;
    } else if c.lifecycle.item_effect & 2 == 0 {
        damage_player(c, events);
        return false;
    }
    let i = p.defeat_score_index.clamp(0, 4) as u8;
    let before = *c.score;
    *c.score = c
        .score
        .wrapping_add([100, 200, 400, 800, 1000][usize::from(i)]);
    events.push(EnemyEvent::Sound(138));
    if *c.score / 50000 != before / 50000 {
        *c.lives = c.lives.saturating_add(1);
        events.push(EnemyEvent::ExtraLifeMusic);
    }
    events.push(EnemyEvent::Defeated {
        x,
        y,
        score_index: i,
    });
    if i < 4 {
        p.defeat_score_index += 1;
    }
    true
}
#[derive(Clone, Copy)]
enum Phase {
    Idle,
    Prepare,
    Jump,
    Land,
}
struct Mon<'a> {
    idx: usize,
    x: i32,
    base_y: i32,
    y: i32,
    offset: i32,
    speed: i32,
    phase: Phase,
    sprite: AnimationPlayer<'a>,
}
pub struct StageEnemies<'a> {
    rom: &'a VerifiedRom<'a>,
    layout: RegionLayout<Interactable>,
    previous: Option<[i32; 2]>,
    spawned: Vec<bool>,
    mon: Vec<Mon<'a>>,
    buzzers: Vec<Buzzer<'a>>,
    easy: bool,
    level: u8,
}
impl<'a> StageEnemies<'a> {
    pub fn load(rom: &'a VerifiedRom<'a>, level: u8) -> Result<Self, Error> {
        let layout = RegionLayout::parse::<7>(&stage_stream(rom, 0xd4d94, level)?, |x, y, r| {
            Interactable {
                x,
                y,
                kind: r[2],
                data: [r[3], r[4], r[5], r[6]],
            }
        })?;
        let spawned = vec![false; layout.records.len()];
        Ok(Self {
            rom,
            layout,
            previous: None,
            spawned,
            mon: vec![],
            buzzers: vec![],
            easy: false,
            level,
        })
    }
    /// Configure campaign difficulty before initial entity admission.
    pub fn set_easy(&mut self, easy: bool) {
        self.easy = easy;
    }
    pub fn placements(&self) -> &[Interactable] {
        &self.layout.records
    }
    pub fn sprites(&self) -> impl Iterator<Item = (i32, i32, &AnimationPlayer<'a>)> {
        self.mon
            .iter()
            .map(|m| (m.x, m.y, &m.sprite))
            .chain(self.buzzers.iter().map(|b| (b.x, b.y, &b.sprite)))
    }
    pub fn admit(&mut self, stage: &NativeStage<'_>, x: i32, y: i32) -> Result<(), Error> {
        let full = [
            x.saturating_sub(128),
            y.saturating_sub(128),
            x.saturating_add(368),
            y.saturating_add(288),
        ];
        let mut rects = Vec::new();
        match self.previous {
            Some([px, py]) if x.abs_diff(px) <= 248 && y.abs_diff(py) <= 208 => {
                if x != px {
                    let mut a = full;
                    if x > px {
                        a[0] = px.saturating_add(368);
                    } else {
                        a[2] = px.saturating_sub(128);
                    }
                    rects.push((a, true));
                }
                if y != py {
                    let mut a = full;
                    if y > py {
                        a[1] = py.saturating_add(288);
                    } else {
                        a[3] = py.saturating_sub(128);
                    }
                    rects.push((a, true));
                }
            }
            _ => rects.push((full, false)),
        }
        self.previous = Some([x, y]);
        for (a, strip) in rects {
            let mx = (self.layout.columns * 256 - 1) as i32;
            let my = (self.layout.rows * 256 - 1) as i32;
            let (l, t, r, b) = (
                a[0].clamp(0, mx),
                a[1].clamp(0, my),
                a[2].clamp(0, mx),
                a[3].clamp(0, my),
            );
            if strip && (l == r || t == b) {
                continue;
            }
            for row in t / 256..(b + 255) / 256 {
                for col in l / 256..(r + 255) / 256 {
                    for idx in self.layout.regions
                        [row as usize * self.layout.columns + col as usize]
                        .clone()
                    {
                        let e = self.layout.records[idx];
                        if !matches!(e.kind, 0 | 2)
                            || self.spawned[idx]
                            || !(l..=r).contains(&e.x)
                            || !(t..=b).contains(&e.y)
                        {
                            continue;
                        }
                        if e.kind == 2 {
                            if !self.easy || self.level > 3 {
                                self.buzzers.push(Buzzer::new(self.rom, idx, e)?);
                                self.spawned[idx] = true;
                            }
                            continue;
                        }
                        let h = stage
                            .vertical_sample(e.x, e.y, u8::from(e.data[0] != 0))?
                            .extent;
                        let rem = e.y & 7;
                        let d = match h {
                            0 => 8 - rem,
                            8 => !rem,
                            n if n > 0 => n - 1 - rem,
                            n => n + rem,
                        };
                        let y = e.y + d;
                        self.mon.push(Mon {
                            idx,
                            x: e.x,
                            base_y: y,
                            y,
                            offset: 0,
                            speed: -1408,
                            phase: Phase::Idle,
                            sprite: AnimationPlayer::new(self.rom, 478, 0)?,
                        });
                        self.spawned[idx] = true;
                    }
                }
            }
        }
        Ok(())
    }
    pub fn tick(
        &mut self,
        c: &mut EnemyContext<'_>,
        cx: i32,
        cy: i32,
    ) -> Result<Vec<EnemyEvent>, Error> {
        let mut events = Vec::new();
        let mut i = 0;
        while i < self.mon.len() {
            let m = &mut self.mon[i];
            if matches!(m.phase, Phase::Jump) {
                m.speed += 52;
                m.offset += m.speed;
                m.y = m.base_y + (m.offset >> 8);
            } else {
                m.y = m.base_y;
            }
            if contact(m.x, m.y, m.sprite.hitboxes()[0], c, &mut events) {
                self.mon.remove(i);
                continue;
            }
            let sx = m.x.wrapping_sub(cx) as i16;
            let sy = m.y.wrapping_sub(cy) as i16;
            if (i32::from(sx) + 128) as u16 > 496 || !(-128..=288).contains(&sy) {
                self.spawned[m.idx] = false;
                self.mon.remove(i);
                continue;
            }
            let qx = m.x * 256;
            let qy = m.base_y * 256;
            let near = (qx - 120 * 256 < c.player.q_world_x && c.player.q_world_x < qx + 120 * 256)
                && (qy - 50 * 256 < c.player.q_world_y && c.player.q_world_y < qy + 50 * 256);
            let right = c.player.q_world_x >= qx;
            events.push(EnemyEvent::HomingTarget {
                x_q8: qx,
                y_q8: if matches!(m.phase, Phase::Jump) {
                    qy + m.offset
                } else {
                    qy
                },
            });
            match m.phase {
                Phase::Idle => {
                    if near {
                        m.phase = Phase::Prepare;
                        m.sprite.set_animation(478, 2)?;
                    }
                    m.sprite.set_flip(right, false);
                    m.sprite.advance()?;
                }
                Phase::Prepare => {
                    if m.sprite.advance()?.ended {
                        m.speed = -1408;
                        m.offset = 0;
                        m.phase = Phase::Jump;
                        m.sprite.set_animation(478, 1)?;
                    }
                }
                Phase::Jump => {
                    if m.offset >= 0 {
                        m.phase = Phase::Land;
                        m.sprite.set_animation(478, 3)?;
                    }
                    m.sprite.advance()?;
                }
                Phase::Land => {
                    if m.sprite.advance()?.ended {
                        if near {
                            m.sprite.set_flip(right, false);
                            m.phase = Phase::Prepare;
                            m.sprite.set_animation(478, 2)?;
                        } else {
                            m.phase = Phase::Idle;
                            m.sprite.set_animation(478, 0)?;
                        }
                    }
                }
            }
            i += 1;
        }
        let mut i = 0;
        while i < self.buzzers.len() {
            if let Some(restore) = self.buzzers[i].tick(c, cx, cy, &mut events)? {
                let b = self.buzzers.remove(i);
                if restore {
                    self.spawned[b.idx] = false;
                }
            } else {
                i += 1;
            }
        }
        Ok(events)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_damage_and_attack_speed_match_pinned_c() {
        let mut packed = vec![];
        for i in 0..4096 {
            let mut p = PlayerState::new_at(crate::native_player::Character::Sonic, 0, 0);
            p.sprite_frame_flags = 0x12345678;
            p.q_speed_air_x = -13;
            p.q_speed_air_y = if i & 1 != 0 { 111 } else { -222 };
            p.move_state =
                if i & 2 != 0 { 0x2000000 } else { 0 } | if i & 4 != 0 { 0x1000000 } else { 0 };
            p.layer = 7;
            let mut life = NativeLifecycle::new([0, 0], false);
            life.timer_invulnerability = if i & 16 != 0 { 1 } else { 0 };
            life.item_effect = if i & 32 != 0 { 1 } else { 0 }
                | if i & 64 != 0 { 8 } else { 0 }
                | if i & 128 != 0 { 128 } else { 0 };
            life.stage_flags = if i & 512 != 0 { 64 } else { 0 };
            let mut rings = if i & 256 != 0 { 137 } else { 0 };
            let (mut score, mut lives) = (0, 3);
            let hit = Hitbox {
                index: 0,
                rectangle: [-1, -1, 1, 1],
            };
            let mut c = EnemyContext {
                player: &mut p,
                body: [hit; 2],
                lifecycle: &mut life,
                rings: &mut rings,
                score: &mut score,
                lives: &mut lives,
                timer_invincibility: if i & 8 != 0 { 2 } else { 0 },
            };
            let mut ev = Vec::new();
            let rv = if i & 1024 != 0 {
                assert!(contact(0, 0, hit, &mut c, &mut ev));
                2
            } else {
                i32::from(damage_player(&mut c, &mut ev))
            };
            let (mut trans, mut lost, mut sound) = (0, 0, 0);
            for e in ev {
                match e {
                    EnemyEvent::Transition(t) => trans = i32::from(t),
                    EnemyEvent::ScatterRings { count, .. } => lost = i32::from(count),
                    EnemyEvent::Sound(s) if s == 119 => sound = i32::from(s),
                    _ => {}
                }
            }
            for v in [
                rv,
                i32::from(p.q_speed_air_x),
                i32::from(p.q_speed_air_y),
                p.move_state as i32,
                i32::from(p.layer),
                life.item_effect as i32,
                i32::from(life.timer_invulnerability),
                trans,
                p.sprite_frame_flags as i32,
                i32::from(rings),
                lost,
                sound,
            ] {
                packed.extend(v.to_le_bytes());
            }
        }
        assert_eq!(
            crate::gba::sha256(&packed),
            "fcb77ee2b0445af0eff98e98c30b160a79dd0c4d8f308166be38287207cf35cd"
        );
    }
}
