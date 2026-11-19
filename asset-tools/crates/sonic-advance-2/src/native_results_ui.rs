//! Ordinary course results OAM and the screen brightness registers. Timing and
//! bonus ownership remain in native_goal::StageResults.
use crate::{
    Error, VerifiedRom,
    native_animation::{AnimationPlayer, SpriteFrame},
    native_goal::{GOAL_REACHED, ResultsContext, StageResults},
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Art {
    Separator,
    Title(u8),
    Label(u8),
    Digit(u8),
}
impl Art {
    fn order(self) -> u8 {
        match self {
            Self::Separator => 5,
            Self::Title(_) | Self::Label(_) => 4,
            Self::Digit(_) => 0,
        }
    }
}
#[derive(Debug, Clone, Copy)]
struct Draw {
    art: Art,
    x: i32,
    y: i32,
}
/// Native DisplaySprite submission order; back-to-front compositing is handled in draw().
fn layout(counter: u32, bonuses: [u32; 3]) -> Vec<Draw> {
    if counter == 0 {
        return vec![];
    }
    let mut calls = Vec::with_capacity(35);
    let mut put = |art, x, y| calls.push(Draw { art, x, y });
    let c = counter.min(i32::MAX as u32) as i32;
    let left = if c <= 16 { (16 - c) * 24 } else { 0 };
    let y = 100 - (c - 23).clamp(0, 5) * 7;
    for i in 0..8 {
        put(Art::Separator, left + i * 32, y);
    }
    if c > 28 {
        let x = if c < 45 { 240 - (c - 29) * 15 } else { 3 };
        for (i, offset) in [0, 69, 173].into_iter().enumerate() {
            put(
                Art::Title(i as u8),
                x + offset,
                if i == 0 { 41 } else { 49 },
            );
        }
    }
    for (i, mut value) in bonuses.into_iter().enumerate() {
        let start = 39 + 10 * i as i32;
        if c < start {
            continue;
        }
        let x = if c <= start + 16 {
            240 - (c - start) * 12
        } else {
            48
        };
        let y = 74 + 20 * i as i32;
        put(Art::Label(i as u8), x, y);
        let mut dx = x + 144;
        for _ in 0..7 {
            put(Art::Digit((value % 10) as u8), dx, y + 16);
            value /= 10;
            if value == 0 {
                break;
            }
            dx -= 8;
        }
    }
    calls
}
pub struct ResultsUi {
    separator: SpriteFrame,
    titles: Vec<SpriteFrame>,
    labels: Vec<SpriteFrame>,
    digits: Vec<SpriteFrame>,
    fade: Fade,
}
#[derive(Debug, Clone, Copy)]
struct Fade {
    brighten: bool,
    brightness: u16,
    coefficient: u8,
}
impl Fade {
    fn step(&mut self) -> bool {
        self.coefficient = (self.brightness >> 9).min(16) as u8;
        self.brightness = self
            .brightness
            .saturating_add(if self.brighten { 64 } else { 256 })
            .min(8192);
        let complete = self.brightness == 8192;
        if complete {
            self.coefficient = 16;
        } // Task_UpdateStageResults overrides gBldRegs.bldY on completion.
        complete
    }
    fn apply(self, rgba: &mut [u8]) {
        if self.coefficient == 0 {
            return;
        }
        let k = u16::from(self.coefficient);
        for pixel in rgba.as_chunks_mut::<4>().0 {
            for channel in &mut pixel[..3] {
                let v = u16::from(*channel >> 3);
                let c = if self.brighten {
                    v + (((31 - v) * k) >> 4)
                } else {
                    v - ((v * k) >> 4)
                } as u8;
                *channel = (c << 3) | (c >> 2);
            }
        }
    }
}
impl ResultsUi {
    pub fn new(rom: &VerifiedRom<'_>, context: &ResultsContext) -> Result<Self, Error> {
        if !crate::native_stage::is_course_act(context.level) || context.character >= 5 {
            return Err(Error(
                "Results presentation requires an ordinary course act and a valid character".into(),
            ));
        }
        let load = |anim, variant| -> Result<SpriteFrame, Error> {
            let mut s = AnimationPlayer::new(rom, anim, variant)?;
            s.advance()?;
            s.render()
        };
        Ok(Self {
            separator: load(1120, 0)?,
            titles: vec![
                load(1121, [0, 4, 1, 2, 3][usize::from(context.character)])?,
                load(1122, 0)?,
                load(1123, context.level & 1)?,
            ],
            labels: (0..3).map(|v| load(1124, v)).collect::<Result<_, _>>()?,
            digits: (16..26).map(|v| load(1119, v)).collect::<Result<_, _>>()?,
            fade: Fade {
                brighten: context.move_state & GOAL_REACHED != 0 && context.special_rings >= 7,
                brightness: 0,
                coefficient: 0,
            },
        })
    }
    /// Call once immediately before the scalar results update. Its increment/clamp
    /// reaches count_frames+310 precisely when the previous counter is >= +309.
    pub fn advance_fade_for_next_tick(&mut self, r: &StageResults) -> bool {
        !r.complete && r.counter >= r.count_frames + 309 && self.fade.step()
    }
    pub fn draw(&self, r: &StageResults, mut draw: impl FnMut(&SpriteFrame, i32, i32)) {
        if r.graphics_hidden || r.complete {
            return;
        }
        let calls = layout(r.counter, [r.time_bonus, r.ring_bonus, r.special_bonus]);
        for order in [5, 4, 0] {
            for d in calls.iter().rev().filter(|d| d.art.order() == order) {
                let s = match d.art {
                    Art::Separator => &self.separator,
                    Art::Title(i) => &self.titles[usize::from(i)],
                    Art::Label(i) => &self.labels[usize::from(i)],
                    Art::Digit(i) => &self.digits[usize::from(i)],
                };
                draw(s, d.x, d.y);
            }
        }
    }
    /// Composite after all BG/OBJ layers; normal course BLDCNT selects all first targets.
    pub fn apply_fade(&self, rgba: &mut [u8]) {
        self.fade.apply(rgba);
    }
    pub fn brightness_coefficient(&self) -> u8 {
        self.fade.coefficient
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_results_ui_c_submissions() {
        let mut bytes = Vec::new();
        for c in 1..=380 {
            for d in layout(
                c,
                [
                    if c < 250 { 80000 } else { 0 },
                    c * 300 % 10000,
                    if c < 150 { 10000 } else { 0 },
                ],
            ) {
                let id = match d.art {
                    Art::Separator => 0,
                    Art::Title(i) => 1 + i,
                    Art::Label(i) => 4 + i,
                    Art::Digit(i) => 7 + i,
                };
                for word in [i32::from(id), d.x, d.y] {
                    bytes.extend(word.to_le_bytes());
                }
            }
        }
        assert_eq!(bytes.len(), 8253 * 12);
        assert_eq!(
            crate::gba::sha256(&bytes),
            "7931794b4d4ac2dda3f8513076b1fd75cc1f87e23371ec57ff769dcaf531ee36"
        );
    }
    #[test]
    fn native_results_ui_c_fade() {
        let mut bytes = Vec::new();
        for brighten in [false, true] {
            let mut f = Fade {
                brighten,
                brightness: 0,
                coefficient: 0,
            };
            for _ in 0..129 {
                let done = f.step();
                for word in [i32::from(f.coefficient), i32::from(done)] {
                    bytes.extend(word.to_le_bytes());
                }
            }
            for (k, expected) in [
                (
                    8,
                    if brighten {
                        [123, 189, 255, 77]
                    } else {
                        [0, 66, 132, 77]
                    },
                ),
                (
                    16,
                    if brighten {
                        [255, 255, 255, 77]
                    } else {
                        [0, 0, 0, 77]
                    },
                ),
            ] {
                f.coefficient = k;
                let mut pixel = [0, 132, 255, 77];
                f.apply(&mut pixel);
                assert_eq!(pixel, expected);
            }
        }
        assert_eq!(
            crate::gba::sha256(&bytes),
            "834554dae26eee67065e245b40266c56735b4b56a7863150b55b868cd2637bdc"
        );
    }
    #[test]
    #[ignore = "requires local A2NE_ROM"]
    fn native_results_ui_rom_art() {
        let bytes = std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap();
        let rom = VerifiedRom::parse(&bytes).unwrap();
        let load = |id, var| {
            let mut anim = AnimationPlayer::new(&rom, id, var).unwrap();
            anim.advance().unwrap();
            anim.render().unwrap()
        };
        for level in [0, 1] {
            for (character, variant) in [0, 4, 1, 2, 3].into_iter().enumerate() {
                let ctx = ResultsContext {
                    level,
                    character: character as u8,
                    game_mode: 0,
                    move_state: 0,
                    special_rings: 0,
                };
                let ui = ResultsUi::new(&rom, &ctx).unwrap();
                let mut result = StageResults::begin(
                    ctx,
                    0,
                    0,
                    0,
                    &mut crate::native_campaign_save::CampaignProfile::new_usa(),
                )
                .unwrap();
                ui.draw(&result, |_, _, _| panic!("constructor frame draws nothing"));
                result.counter = 79;
                result.time_bonus = 1234567;
                result.ring_bonus = 890;
                result.special_bonus = 0;
                let mut points = Vec::new();
                ui.draw(&result, |frame, x, y| {
                    assert!(frame.rgba.as_chunks::<4>().0.iter().any(|p| p[3] != 0));
                    points.push((x, y, frame.rgba.clone()));
                });
                let at = |x, y| &points.iter().find(|p| p.0 == x && p.1 == y).unwrap().2;
                for (id, var, x, y) in [
                    (1120, 0, 0, 65),
                    (1121, variant, 3, 41),
                    (1122, 0, 72, 49),
                    (1123, level, 176, 49),
                    (1124, 0, 48, 74),
                    (1124, 1, 48, 94),
                    (1124, 2, 48, 114),
                ] {
                    assert_eq!(at(x, y), &load(id, var).rgba);
                }
                for (row, numbers) in [&[7u8, 6, 5, 4, 3, 2, 1][..], &[0, 9, 8], &[0]]
                    .into_iter()
                    .enumerate()
                {
                    for (i, n) in numbers.iter().enumerate() {
                        assert_eq!(
                            at(192 - 8 * i as i32, 90 + 20 * row as i32),
                            &load(1119, 16 + n).rgba
                        );
                    }
                }
                result.graphics_hidden = true;
                ui.draw(&result, |_, _, _| panic!("hidden"));
            }
        }
    }
}
