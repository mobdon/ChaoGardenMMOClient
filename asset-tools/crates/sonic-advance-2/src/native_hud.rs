//! Single-player A2NE course HUD. Layout and OAM order follow Task_StageUIMain.
use crate::{
    Error, VerifiedRom,
    native_animation::{AnimationPlayer, SpriteFrame},
};

pub(crate) struct StageHud {
    digits: Vec<[SpriteFrame; 2]>,
    container: SpriteFrame,
    icon: SpriteFrame,
    ring: Vec<SpriteFrame>,
    ring_phase: u16,
    special_frames: Vec<SpriteFrame>,
    special_cycle: Vec<usize>,
    special_tick: usize,
    special_count: u8,
    milliseconds: [u8; 120],
    seconds: [u8; 120],
    previous_digit_palette: [bool; 10],
    overflow_palette: bool,
}

impl StageHud {
    pub fn new(rom: &VerifiedRom<'_>) -> Result<Self, Error> {
        let load = |animation, variant| {
            let mut sprite = AnimationPlayer::new(rom, animation, variant)?;
            sprite.advance()?;
            Ok::<_, Error>(sprite)
        };
        // sPalette1UpIcons follows sAnims1UpIcons in stage_ui.c. This
        // palette is also used when the zero ring count flashes red.
        let raw = rom.range("rom", 0xd6ace, 32)?;
        let alternate = std::array::from_fn(|i| u16::from_le_bytes([raw[2 * i], raw[2 * i + 1]]));
        let mut digits = Vec::with_capacity(11);
        for variant in 16..=26 {
            let mut sprite = load(1119, variant)?;
            let regular = sprite.render()?;
            sprite.set_palette_bank(6, &alternate)?;
            digits.push([regular, sprite.render()?]);
        }
        let ring = (0..8)
            .map(|v| load(1127, v)?.render())
            .collect::<Result<_, Error>>()?;
        // Cache the native script until its first display state repeats.
        let mut anim = AnimationPlayer::new(rom, 1128, 0)?;
        let mut frames = Vec::new();
        let mut cycle = Vec::new();
        let mut first = None;
        loop {
            let step = anim.advance()?;
            let key = (step.cursor_words, step.delay_q8, step.frame_index);
            if first == Some(key) {
                break;
            }
            first.get_or_insert(key);
            let index = usize::from(
                step.frame_index
                    .ok_or_else(|| Error("Empty special ring HUD frame".into()))?,
            );
            if index == frames.len() {
                frames.push(anim.render()?);
            }
            if index >= frames.len() || cycle.len() >= 1024 {
                return Err(Error("Unexpected special ring HUD animation".into()));
            }
            cycle.push(index);
        }
        Ok(Self {
            special_tick: cycle.len() - 1,
            special_frames: frames,
            special_cycle: cycle,
            special_count: 0,
            digits,
            container: load(1126, 0)?.render()?,
            icon: load(1125, 0)?.render()?,
            ring,
            ring_phase: 0,
            seconds: rom.range("rom", 0xd6b08, 120)?.try_into().unwrap(),
            milliseconds: rom.range("rom", 0xd6b80, 120)?.try_into().unwrap(),
            previous_digit_palette: [false; 10],
            overflow_palette: false,
        })
    }

    pub fn set_special_count(&mut self, count: u8) {
        self.special_count = count;
    }
    pub fn tick(&mut self, speed_air_x: i16, time: u32, stage_frame: u64) {
        self.special_tick = (self.special_tick + 1) % self.special_cycle.len();
        self.ring_phase = self
            .ring_phase
            .wrapping_add(((speed_air_x >> 3) + 64) as u16)
            & 2047;
        // The >999 path uses DisplaySprite on the existing 9 sprite before
        // this frame's timer updates its palId.
        self.overflow_palette = self.previous_digit_palette[9];
        let warning = time > 32400 && stage_frame & 16 != 0;
        for digit in self.clock_digits(time) {
            self.previous_digit_palette[digit as usize] = warning;
        }
    }

    fn clock_digits(&self, time: u32) -> [u8; 5] {
        let time = time.min(35999);
        let sec = (time / 60 % 60) as usize * 2;
        let fraction = (time % 60) as usize * 2;
        [
            (time / 3600) as u8,
            self.seconds[sec],
            self.seconds[sec + 1],
            self.milliseconds[fraction],
            self.milliseconds[fraction + 1],
        ]
    }

    pub fn draw(
        &self,
        time: u32,
        stage_frame: u64,
        rings: u16,
        lives: u8,
        score: u32,
        mut draw: impl FnMut(&SpriteFrame, i32, i32),
    ) {
        // All positions below are top-left for direct 4bpp OAM writes.
        // DisplaySprite calls use the original animation's anchor instead.
        let mut direct = |sprite: &SpriteFrame, x, y| {
            draw(
                sprite,
                x + i32::from(sprite.origin_x),
                y + i32::from(sprite.origin_y),
            )
        };
        direct(&self.container, -3, 0);
        let special = &self.special_frames[self.special_cycle[self.special_tick]];
        for i in 0..self.special_count {
            direct(special, 4 + 8 * i32::from(i), 31);
        }
        direct(&self.ring[usize::from(self.ring_phase >> 8)], 7, 8);
        direct(&self.icon, 6, 142);
        direct(
            &self.digits[usize::from(lives.saturating_sub(1).min(9))][0],
            30,
            140,
        );
        let mut value = score.min(999999);
        for i in (0..6).rev() {
            direct(&self.digits[(value % 10) as usize][0], 28 + i * 8, 14);
            value /= 10;
        }
        direct(&self.digits[10][0], 99, 0);
        direct(&self.digits[10][0], 123, 0);
        if rings <= 999 {
            let alternate = usize::from(rings == 0 && stage_frame & 16 != 0);
            for (x, digit) in [(28, rings / 100), (36, rings / 10 % 10), (44, rings % 10)] {
                direct(&self.digits[usize::from(digit)][alternate], x, 0);
            }
        } else {
            for x in [32, 40, 48] {
                draw(&self.digits[9][usize::from(self.overflow_palette)], x, 16);
            }
        }
        let alternate = usize::from(time > 32400 && stage_frame & 16 != 0);
        for (x, digit) in [96, 112, 120, 136, 144]
            .into_iter()
            .zip(self.clock_digits(time))
        {
            draw(&self.digits[usize::from(digit)][alternate], x, 16);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires local verified A2NE_ROM"]
    fn native_single_player_hud_matches_independent_direct_oam_pixels() {
        let bytes = std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap();
        let rom = VerifiedRom::parse(&bytes).unwrap();
        let mut hud = StageHud::new(&rom).unwrap();
        type Row = (u32, u64, u16, u8, u32, i16, String);
        #[derive(serde::Deserialize)]
        struct Report {
            rows: Vec<Row>,
        }
        let report: Report =
            serde_json::from_str(include_str!("../catalog/native-hud-parity.json")).unwrap();
        for (time, tick, rings, lives, score, speed, hash) in report.rows {
            hud.tick(speed, time, tick);
            let mut rgba = vec![0; 240 * 160 * 4];
            hud.draw(time, tick, rings, lives, score, |img, x, y| {
                crate::native_game::blit(&mut rgba, img, x, y)
            });
            assert_eq!(
                crate::gba::sha256(&rgba),
                hash,
                "HUD clock {time}, rings {rings}"
            );
        }
    }
}
