//! SA2 course checkpoint objects (visible 43 and silent 45).
use crate::{
    Error, VerifiedRom,
    native_animation::AnimationPlayer,
    native_entities::StageEntities,
    native_lifecycle::{Checkpoint, NativeLifecycle, checkpoint_position},
    native_player::PlayerState,
};
const ZONE_PALETTES: [(u16, u8); 7] = [
    (898, 2),
    (899, 7),
    (903, 3),
    (902, 0),
    (904, 6),
    (947, 11),
    (905, 6),
];
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    Hit,
    Used,
}
struct Visual<'a> {
    ball: AnimationPlayer<'a>,
    palette: AnimationPlayer<'a>,
    state: State,
    display: bool,
}
struct Task<'a> {
    index: usize,
    x: i32,
    y: i32,
    visual: Option<Visual<'a>>,
}
pub struct StageCheckpoints<'a> {
    rom: &'a VerifiedRom<'a>,
    destination: [i16; 2],
    palette_id: u16,
    bank: u8,
    restore: [u16; 16],
    background: Option<[u16; 16]>,
    tasks: Vec<Task<'a>>,
}
impl<'a> StageCheckpoints<'a> {
    pub fn new(rom: &'a VerifiedRom<'a>, level: u8) -> Result<Self, Error> {
        let destination = checkpoint_position(rom, level)?;
        let (palette_id, bank) = ZONE_PALETTES[usize::from(level / 4)];
        let mut first = AnimationPlayer::new(rom, palette_id, 0)?;
        first.advance()?;
        let restore = Self::colors(&first, bank);
        Ok(Self {
            rom,
            destination,
            palette_id,
            bank,
            restore,
            background: None,
            tasks: Vec::new(),
        })
    }
    fn colors(sprite: &AnimationPlayer<'_>, bank: u8) -> [u16; 16] {
        let off = usize::from(bank) * 16;
        sprite.palette_words()[off..off + 16]
            .try_into()
            .expect("fixed hardware bank")
    }
    /// Native stage palette bank override. Applies to background tiles, never the OBJ ball.
    pub fn background_palette(&self) -> Option<(u8, &[u16; 16])> {
        self.background.as_ref().map(|p| (self.bank, p))
    }
    pub fn sprites(&self) -> impl Iterator<Item = (i32, i32, &AnimationPlayer<'a>)> {
        self.tasks.iter().filter_map(|t| {
            t.visual
                .as_ref()
                .filter(|v| v.display)
                .map(|v| (t.x, t.y, &v.ball))
        })
    }
    /// Dispatch after the priority 0x2000 entity scan, before player motion. Return requested sound IDs.
    pub fn tick(
        &mut self,
        entities: &mut StageEntities,
        p: &PlayerState,
        cx: i32,
        cy: i32,
        life: &mut NativeLifecycle,
    ) -> Result<Vec<u16>, Error> {
        let previous_count = self.tasks.len();
        for index in entities.take_new_checkpoints() {
            let e = entities.placements()[index];
            if e.kind == 45 && life.checkpoint_reached {
                continue;
            }
            let visual = if e.kind == 43 {
                Some(Visual {
                    ball: AnimationPlayer::new(self.rom, 531, 0)?,
                    palette: AnimationPlayer::new(self.rom, self.palette_id, 0)?,
                    state: if life.checkpoint_reached {
                        State::Used
                    } else {
                        State::Idle
                    },
                    display: false,
                })
            } else {
                None
            };
            self.tasks.push(Task {
                index,
                x: e.x,
                y: e.y,
                visual,
            });
        }
        let palette_count = if entities.dispatch_new_priority_2000 {
            self.tasks.len()
        } else {
            previous_count
        };
        for t in &mut self.tasks[..palette_count] {
            if let Some(v) = &mut t.visual {
                v.palette.advance()?;
                self.background = Some(Self::colors(&v.palette, self.bank));
            }
        }
        let mut sounds = vec![];
        let mut i = 0;
        while i < self.tasks.len() {
            let t = &mut self.tasks[i];
            let sx = t.x.wrapping_sub(cx) as i16;
            let sy = t.y.wrapping_sub(cy) as i16;
            if (i32::from(sx) + 128) as u16 > 496 || !(-128..=288).contains(&sy) {
                entities.restore_checkpoint(t.index);
                if t.visual.is_some() {
                    self.background = Some(self.restore);
                }
                self.tasks.remove(i);
                continue;
            }
            let crosses = p.move_state & (0x00400000 | 0x80) == 0 && t.x <= p.q_world_x >> 8;
            let set = (t.visual.as_ref().is_none_or(|v| v.state == State::Idle)) && crosses;
            if set {
                life.checkpoint = Checkpoint {
                    x: self.destination[0],
                    y: self.destination[1],
                    time: life.checkpoint_time,
                };
                life.checkpoint_reached = true;
            }
            match &mut t.visual {
                None if set || life.checkpoint_reached => {
                    self.tasks.remove(i);
                    continue;
                }
                None => {}
                Some(v) => {
                    v.display = v.state != State::Used;
                    if set {
                        v.ball.set_animation(531, 1)?;
                        v.state = State::Hit;
                        sounds.push(152);
                    }
                    if v.display && v.ball.advance()?.ended && v.state == State::Hit {
                        v.state = State::Used;
                    }
                }
            }
            i += 1;
        }
        Ok(sounds)
    }
}
