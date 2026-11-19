//! ROM-backed ASOE English title slice, entered directly at CreateTitleScreen.
//! Native fade/zoom/blink/input gates stop at menu/demo request boundaries.
//! Sega/intro tasks, menus, music playback and native execution are not emulated.
pub mod menu;
mod objects;
use crate::{binary, graphics, tables, Error, Result};
use objects as sprite;

pub const WIDTH: usize = 240;
pub const HEIGHT: usize = 160;
pub const FRAME_BYTES: usize = WIDTH * HEIGHT * 4;
pub const START_BUTTON: u16 = 0x8;
pub const INPUT_MASK: u16 = 0x3ff;
pub const MAX_TICKS: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TitlePhase {
    FadeIn,
    LogoZoom,
    Waiting,
    MainMenuRequested,
    DemoRequested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleEvent {
    None,
    MainMenuRequested,
    DemoRequested,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TitleSnapshot {
    pub frame: u32,
    pub phase: TitlePhase,
    pub idle_ticks: u16,
    pub blend: u8,
    pub scale_fixed: u16,
    pub logo_visible: bool,
    pub press_start_visible: bool,
    pub copyright_visible: bool,
    pub start_enabled: bool,
    pub source_sha256: Option<&'static str>,
}

/// Title maps have fixed, source-evidenced dimensions: BG0 is 30x20 u16 text
/// entries; BG2 is 24x10 u8 affine entries. Both graphics banks are 8bpp.
#[derive(Clone, Copy)]
pub struct MapInput<'a> {
    pub tiles: &'a [u8],
    pub map: &'a [u8],
}

/// A single native 4bpp OBJ frame. Exactly one complete palette bank is loaded
/// at the specified destination; references to another bank are rejected.
#[derive(Clone, Copy)]
pub struct SpriteInput<'a> {
    pub tiles: &'a [u8],
    pub palette: &'a [u8],
    pub palette_bank: u8,
    pub dimensions: &'a [u8],
    pub oam: &'a [u8],
}

#[derive(Clone, Copy)]
pub struct TitleInput<'a> {
    pub background: MapInput<'a>,
    pub logo: MapInput<'a>,
    pub background_palette: &'a [u8],
    pub press_start: SpriteInput<'a>,
    pub copyright: SpriteInput<'a>,
}

pub struct Title {
    background: Vec<u8>,
    logo: Vec<u8>,
    palettes: [[[u8; 4]; 256]; 17],
    press_start: sprite::RenderedFrame,
    copyright: sprite::RenderedFrame,
    phase: TitlePhase,
    frame: u32,
    idle_ticks: u16,
    q_blend: i16,
    scale_fixed: u16,
    scale_delta: i16,
    held: u16,
    logo_visible: bool,
    press_start_visible: bool,
    copyright_visible: bool,
    source_sha256: Option<&'static str>,
}

impl Title {
    pub fn load_asoe(rom: &[u8]) -> Result<Self> {
        binary::verify_asoe(rom)?;
        let background = tables::tilemap(rom, 0xc8138, 600)?;
        let logo = tables::tilemap(rom, 0xbf72c, 240)?;
        for (map, dimensions) in [(&background, (30, 20)), (&logo, (24, 10))] {
            if (map.width_tiles, map.height_tiles) != dimensions
                || map.animation_tile_bytes != 0
                || map.animation_frames != 0
                || map.animation_delay != 0
                || map.palette_color_offset != 0
                || map.palette_color_count != 256
            {
                return Err(Error("unsupported native title map header".into()));
            }
        }
        if background.palette != logo.palette {
            return Err(Error("native title BG palette disagreement".into()));
        }
        let mut title = Self::decode(TitleInput {
            background: MapInput {
                tiles: background.tiles,
                map: binary::range(rom, background.map_offset, 1200)?,
            },
            logo: MapInput {
                tiles: logo.tiles,
                map: binary::range(rom, logo.map_offset, 240)?,
            },
            background_palette: logo
                .palette
                .ok_or_else(|| Error("missing native title palette".into()))?,
            // Anim779 variant0 and anim780 variant1 are static first frames,
            // initialized once by native CreateTitleScreen, not advanced there.
            press_start: SpriteInput {
                tiles: binary::range(rom, 0x410e14, 896)?,
                palette: binary::range(rom, 0x11b0d4, 32)?,
                palette_bank: 6,
                dimensions: binary::range(rom, 0x10985c, 12)?,
                oam: binary::range(rom, 0x117bd8, 24)?,
            },
            copyright: SpriteInput {
                tiles: binary::range(rom, 0x412c54, 2880)?,
                palette: binary::range(rom, 0x11beb4, 32)?,
                palette_bank: 3,
                dimensions: binary::range(rom, 0x1098b0, 12)?,
                // The dimension record selects OAM16..31 from this root.
                oam: binary::range(rom, 0x117c56, 192)?,
            },
        })?;
        title.source_sha256 = Some(binary::ROM_SHA256);
        Ok(title)
    }

    /// Bounded representation decoder, also usable with synthetic fixtures.
    /// Synthetic inputs carry no cartridge fingerprint in the snapshot.
    pub fn decode(input: TitleInput<'_>) -> Result<Self> {
        if input.background_palette.len() != 512 {
            return Err(Error("title BG palette must contain 256 colors".into()));
        }
        let background = decode_map(input.background, false)?;
        let logo = decode_map(input.logo, true)?;
        let press_start = decode_sprite(input.press_start)?;
        let copyright = decode_sprite(input.copyright)?;
        let mut palettes = [[[0_u8; 4]; 256]; 17];
        for (coefficient, palette) in palettes.iter_mut().enumerate() {
            for (index, color) in palette.iter_mut().enumerate() {
                let word = binary::u16_at(input.background_palette, index * 2)?;
                for (channel, shift) in [0, 5, 10].into_iter().enumerate() {
                    let value = usize::from((word >> shift) & 31);
                    // BLDCNT0x20A5 selects brightness increase (effect2):
                    // title fades from white, with native5-bit truncation.
                    let shaded = (value + ((31 - value) * coefficient) / 16) as u8;
                    color[channel] = (shaded << 3) | (shaded >> 2);
                }
                color[3] = 255;
            }
        }
        Ok(Self {
            background,
            logo,
            palettes,
            press_start,
            copyright,
            phase: TitlePhase::FadeIn,
            frame: 0,
            idle_ticks: 0,
            q_blend: 0x1000,
            scale_fixed: 0x400,
            scale_delta: -16,
            held: 0,
            logo_visible: false,
            press_start_visible: false,
            copyright_visible: false,
            source_sha256: None,
        })
    }

    pub fn snapshot(&self) -> TitleSnapshot {
        TitleSnapshot {
            frame: self.frame,
            phase: self.phase,
            idle_ticks: self.idle_ticks,
            blend: self.blend(),
            scale_fixed: self.scale_fixed,
            logo_visible: self.logo_visible,
            press_start_visible: self.press_start_visible,
            copyright_visible: self.copyright_visible,
            start_enabled: self.phase == TitlePhase::Waiting && self.idle_ticks > 60,
            source_sha256: self.source_sha256,
        }
    }

    /// Input is held GBA keys; rising edges reproduce gPressedKeys. A held Start
    /// crossing the initial gate cannot request a menu. Multi-tick calls use the
    /// same held input, so only the first tick can contain a new edge.
    pub fn step(&mut self, held_buttons: u16, ticks: u32) -> Result<TitleEvent> {
        if held_buttons & !INPUT_MASK != 0 || ticks == 0 || ticks > MAX_TICKS {
            return Err(Error("invalid title input mask or tick budget".into()));
        }
        if matches!(
            self.phase,
            TitlePhase::MainMenuRequested | TitlePhase::DemoRequested
        ) {
            return Ok(TitleEvent::None);
        }
        for _ in 0..ticks {
            let pressed = held_buttons & !self.held;
            self.held = held_buttons;
            self.frame += 1;
            self.press_start_visible = false;
            self.copyright_visible = false;
            match self.phase {
                TitlePhase::FadeIn => {
                    self.q_blend -= 0x55;
                    if self.q_blend <= 0 {
                        self.phase = TitlePhase::LogoZoom;
                    }
                }
                TitlePhase::LogoZoom => {
                    self.logo_visible = true;
                    let scale = i32::from(self.scale_fixed) + i32::from(self.scale_delta);
                    self.scale_delta -= 8;
                    if scale < 0x100 {
                        self.scale_fixed = 0x100;
                        self.idle_ticks = 0;
                        self.phase = TitlePhase::Waiting;
                    } else {
                        self.scale_fixed = scale as u16;
                    }
                }
                TitlePhase::Waiting => {
                    self.idle_ticks += 1;
                    if self.idle_ticks > 60 {
                        if pressed & START_BUTTON != 0 {
                            self.phase = TitlePhase::MainMenuRequested;
                            return Ok(TitleEvent::MainMenuRequested);
                        }
                        self.press_start_visible = (self.idle_ticks - 60) & 0x20 == 0;
                    }
                    self.copyright_visible = true;
                    if self.idle_ticks > 900 {
                        self.phase = TitlePhase::DemoRequested;
                        return Ok(TitleEvent::DemoRequested);
                    }
                }
                TitlePhase::MainMenuRequested | TitlePhase::DemoRequested => unreachable!(),
            }
        }
        Ok(TitleEvent::None)
    }

    pub fn render(&self) -> Result<Vec<u8>> {
        let mut output = vec![0; FRAME_BYTES];
        self.render_into(&mut output)?;
        Ok(output)
    }

    /// Fixed native viewport; invalid output sizes fail before any writes.
    pub fn render_into(&self, output: &mut [u8]) -> Result<()> {
        if output.len() != FRAME_BYTES {
            return Err(Error("title output must be exactly 240x160 RGBA".into()));
        }
        let palette = &self.palettes[usize::from(self.blend())];
        for (pixel, &index) in output
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(&self.background)
        {
            pixel.copy_from_slice(&palette[usize::from(index)]);
        }
        if self.logo_visible {
            // Native affine helper at angle0, Q8 scale, texture anchor(141,26)
            // and screen anchor(192,30). Outside the24x10 copied rectangle the
            // native map is255 and tile255 was cleared to transparent zero.
            let inverse_scale = 0x10000_i32 / i32::from(self.scale_fixed);
            for y in 0..HEIGHT {
                let sy = (((y as i32 - 30) * inverse_scale) + 26 * 256) >> 8;
                if !(0..80).contains(&sy) {
                    continue;
                }
                for x in 0..WIDTH {
                    let sx = (((x as i32 - 192) * inverse_scale) + 141 * 256) >> 8;
                    if !(0..192).contains(&sx) {
                        continue;
                    }
                    let index = self.logo[sy as usize * 192 + sx as usize];
                    if index != 0 {
                        let at = (y * WIDTH + x) * 4;
                        output[at..at + 4].copy_from_slice(&palette[usize::from(index)]);
                    }
                }
            }
        }
        if self.press_start_visible {
            sprite::composite_rgba(output, WIDTH, HEIGHT, &self.press_start, 120, 113, false)?;
        }
        if self.copyright_visible {
            sprite::composite_rgba(output, WIDTH, HEIGHT, &self.copyright, 0, 134, false)?;
        }
        Ok(())
    }

    fn blend(&self) -> u8 {
        if self.phase == TitlePhase::FadeIn {
            (self.q_blend >> 8).clamp(0, 16) as u8
        } else {
            0
        }
    }
}

fn decode_map(input: MapInput<'_>, affine: bool) -> Result<Vec<u8>> {
    let (width, height) = if affine { (24, 10) } else { (30, 20) };
    let tile_limit = if affine { 255 } else { 1024 };
    if input.tiles.is_empty()
        || !input.tiles.len().is_multiple_of(64)
        || input.tiles.len() > tile_limit * 64
        || input.map.len() != width * height * if affine { 1 } else { 2 }
    {
        return Err(Error("invalid title tile or map span".into()));
    }
    let mut output = vec![0; width * height * 64];
    for tile_y in 0..height {
        for tile_x in 0..width {
            let at = tile_y * width + tile_x;
            let entry = if affine {
                graphics::TextEntry::decode(u16::from(input.map[at]))
            } else {
                graphics::TextEntry::decode(binary::u16_at(input.map, at * 2)?)
            };
            let tile = usize::from(entry.tile);
            // Affine tile255 is the native cleared transparent border tile.
            if affine && tile == 255 {
                continue;
            }
            let pixels = binary::range(input.tiles, tile * 64, 64)?;
            for y in 0..8 {
                for x in 0..8 {
                    let sx = if entry.flip_x { 7 - x } else { x };
                    let sy = if entry.flip_y { 7 - y } else { y };
                    output[(tile_y * 8 + y) * width * 8 + tile_x * 8 + x] = pixels[sy * 8 + sx];
                }
            }
        }
    }
    Ok(output)
}

fn decode_sprite(input: SpriteInput<'_>) -> Result<sprite::RenderedFrame> {
    if input.palette.len() != 32 || input.palette_bank >= 16 || input.oam.len() > 128 * 6 {
        return Err(Error("invalid title OBJ palette or OAM span".into()));
    }
    let native = sprite::NativeFrame::parse(input.dimensions, input.oam, 32)?;
    if native
        .objects()
        .any(|object| object.palette_bank != usize::from(input.palette_bank))
    {
        return Err(Error("title OBJ depends on another palette bank".into()));
    }
    let mut palette = [[0_u8, 0, 0, 255]; 256];
    let decoded = graphics::decode_palette(input.palette, 16)?;
    let start = usize::from(input.palette_bank) * 16;
    palette[start..start + 16].copy_from_slice(&decoded);
    sprite::render_frame(&native, input.tiles, &palette, 240 * 32)
}
