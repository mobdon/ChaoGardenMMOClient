//! English main menu entered from the title. Inputs and elapsed time belong to Rust.
use super::{Title, TitlePhase, FRAME_BYTES, HEIGHT, WIDTH};
use crate::{
    sprite::{RenderedSprite, SpriteBank, SpritePlayer},
    Error, Result,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Destination {
    CharacterSelect,
    Multiplayer,
    TimeAttack,
    Options,
    TinyGarden,
}
const ITEMS: [Destination; 5] = [
    Destination::CharacterSelect,
    Destination::Multiplayer,
    Destination::TimeAttack,
    Destination::Options,
    Destination::TinyGarden,
];

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MenuState {
    pub frame: u32,
    pub selected: usize,
    pub fading: bool,
    pub blend: u8,
    pub destination: Option<Destination>,
}

pub struct MainMenu {
    state: MenuState,
    held: u16,
    repeat: [u8; 2],
    fade_q8: u16,
    background: Vec<u8>,
    labels: Vec<[RenderedSprite; 2]>,
}

impl MainMenu {
    /// Use the same title artwork that Start left behind. No framebuffer input
    /// from the browser or extraction to disk is needed.
    pub fn from_title(rom: &[u8], title: &Title, held: u16) -> Result<Self> {
        if title.snapshot().phase != TitlePhase::MainMenuRequested || held & !super::INPUT_MASK != 0
        {
            return Err(Error("main menu requires an accepted title Start".into()));
        }
        let bank = SpriteBank::from_asoe(rom)?;
        let mut labels = Vec::with_capacity(5);
        for variant in 1..=5 {
            let mut player = SpritePlayer::new(779, variant);
            player.tick(&bank)?;
            let mut label = |selected: bool| -> Result<RenderedSprite> {
                let dest = if selected { 4 } else { 3 };
                let original = if variant == 1 { 4 } else { 3 };
                player.set_palette_override(
                    &bank,
                    if selected { 178 } else { 179 },
                    dest,
                    (16 + dest - original) % 16,
                )?;
                bank.render(&player)?
                    .ok_or_else(|| Error("missing menu label frame".into()))
            };
            labels.push([label(false)?, label(true)?]);
        }
        Ok(Self {
            state: MenuState {
                frame: 0,
                selected: 0,
                fading: false,
                blend: 0,
                destination: None,
            },
            held,
            repeat: [20; 2],
            fade_q8: 0,
            background: title.render()?,
            labels,
        })
    }
    pub fn snapshot(&self) -> &MenuState {
        &self.state
    }
    pub fn step(&mut self, held: u16, ticks: u32) -> Result<()> {
        if held & !super::INPUT_MASK != 0 || !(1..=4).contains(&ticks) {
            return Err(Error("invalid menu input or batch".into()));
        }
        for _ in 0..ticks {
            if self.state.destination.is_some() {
                break;
            }
            let edges = held & !self.held;
            self.held = held;
            let mut repeated = edges;
            for (i, bit) in [0x40, 0x80].into_iter().enumerate() {
                if held & bit == 0 {
                    self.repeat[i] = 20
                } else if self.repeat[i] > 0 {
                    self.repeat[i] -= 1
                } else {
                    repeated |= bit;
                    self.repeat[i] = 8
                }
            }
            self.state.frame = self.state.frame.saturating_add(1);
            // Transition task calls CreateMainMenu, then init, then selection.
            if self.state.frame <= 2 {
                continue;
            }
            if self.state.fading {
                if self.fade_q8 == 4096 {
                    self.state.destination = Some(ITEMS[self.state.selected]);
                    break;
                }
                self.fade_q8 = (self.fade_q8 + 0x55).min(4096);
                self.state.blend = (self.fade_q8 >> 8) as u8;
            } else {
                if repeated & 0x40 != 0 {
                    self.state.selected = (self.state.selected + 4) % 5
                } else if repeated & 0x80 != 0 {
                    self.state.selected = (self.state.selected + 1) % 5
                }
                self.state.fading = edges & 1 != 0;
            }
        }
        Ok(())
    }
    pub fn render_into(&self, out: &mut [u8]) -> Result<()> {
        if out.len() != FRAME_BYTES {
            return Err(Error("main menu requires 240x160 RGBA".into()));
        }
        out.copy_from_slice(&self.background);
        if self.state.frame < 3 {
            return Ok(());
        }
        for (i, variants) in self.labels.iter().enumerate() {
            let s = &variants[usize::from(self.state.selected == i)];
            let (x0, y0) = (
                120 - i32::from(s.anchor_x),
                93 + 14 * i as i32 - i32::from(s.anchor_y),
            );
            for (n, pixel) in s.image.pixels.iter().enumerate() {
                let x = x0 + (n % s.image.width) as i32;
                let y = y0 + (n / s.image.width) as i32;
                if pixel[3] != 0
                    && (0..WIDTH as i32).contains(&x)
                    && (0..HEIGHT as i32).contains(&y)
                {
                    let off = (y as usize * WIDTH + x as usize) * 4;
                    out[off..off + 4].copy_from_slice(pixel);
                }
            }
        }
        if self.state.blend != 0 {
            let k = u16::from(self.state.blend);
            for pixel in out.as_chunks_mut::<4>().0 {
                for (i, channel) in pixel[..3].iter_mut().enumerate() {
                    let c = u16::from(*channel >> 3);
                    let v = if self.state.selected == 4 {
                        c + (31 - c) * k / 16
                    } else if i == 2 {
                        c - c * k / 16
                    } else {
                        c * (16 - k) / 16
                    };
                    *channel = ((v << 3) | (v >> 2)) as u8;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn logic_only() -> MainMenu {
        MainMenu {
            state: MenuState {
                frame: 0,
                selected: 0,
                fading: false,
                blend: 0,
                destination: None,
            },
            held: 0,
            repeat: [20; 2],
            fade_q8: 0,
            background: vec![],
            labels: vec![],
        }
    }

    #[test]
    fn all_destinations_require_fresh_confirm_and_freeze_after_exit() {
        for (i, destination) in ITEMS.iter().enumerate() {
            let mut m = logic_only();
            m.step(1, 4).unwrap(); // A held through init is not a fresh press
            assert!(!m.snapshot().fading);
            for _ in 0..i {
                m.step(0, 1).unwrap();
                m.step(128, 1).unwrap();
            }
            m.step(0, 1).unwrap();
            m.step(1, 1).unwrap();
            for _ in 0..16 {
                m.step(64, 4).unwrap();
            }
            assert_eq!(m.snapshot().destination, Some(*destination));
            let frame = m.snapshot().frame;
            m.step(128, 4).unwrap();
            assert_eq!(m.snapshot().frame, frame);
        }
    }

    #[test]
    fn up_wins_and_batched_repeat_matches_single_ticks() {
        let (mut batch, mut single) = (logic_only(), logic_only());
        for (held, ticks) in [(0, 3), (192, 1)]
            .into_iter()
            .chain(std::iter::repeat_n((192, 4), 12))
        {
            batch.step(held, ticks).unwrap();
            for _ in 0..ticks {
                single.step(held, 1).unwrap();
            }
            assert_eq!(batch.state.selected, single.state.selected);
            assert_eq!(batch.state.frame, single.state.frame);
            if batch.state.frame == 4 {
                assert_eq!(batch.state.selected, 4);
            }
        }
    }
}
