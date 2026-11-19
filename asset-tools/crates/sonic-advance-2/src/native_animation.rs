//! Executable A2NE sprite command VM and non-affine OBJ composition.
//!
//! Format/behavior reference: SAT-R/sa2 2dd3f17d, include/core.h,
//! include/constants/anim_commands.h and src/sprite.c:138-212,289-380,
//! 651-824,1058-1177. This is an independent bounded Rust implementation.
//! Native DMA is represented by private OBJ tile/palette buffers. Sound
//! commands are emitted as events; this module does not synthesize audio.

use crate::{Error, VerifiedRom, bounded_range, graphics::PaletteColor};
use serde::Serialize;

pub const SPRITE_TABLE_OFFSET: usize = 0xf40d4;
pub const ANIMATION_SLOTS: usize = 1133;
const ROM_BASE: u32 = 0x0800_0000;
const OBJ_BYTES: usize = 32 * 1024;
const COMMAND_BUDGET: usize = 256;
const MAX_CURSOR_WORDS: usize = 16_384;
const MAX_PIXELS: usize = 1024 * 1024;

fn err(message: impl Into<String>) -> Error {
    Error(message.into())
}

fn word(source: &[u8], offset: usize) -> Result<u32, Error> {
    let bytes = bounded_range(source, offset, 4)?;
    Ok(u32::from_le_bytes(bytes.try_into().expect("four bytes")))
}

fn address(source: &[u8], pointer: u32) -> Result<usize, Error> {
    let offset = pointer
        .checked_sub(ROM_BASE)
        .ok_or_else(|| err("Sprite pointer below ROM base"))? as usize;
    if offset >= source.len() {
        return Err(err("Sprite pointer outside ROM"));
    }
    Ok(offset)
}

fn indexed(base: usize, index: usize, stride: usize) -> Result<usize, Error> {
    base.checked_add(
        index
            .checked_mul(stride)
            .ok_or_else(|| err("Sprite index overflow"))?,
    )
    .ok_or_else(|| err("Sprite offset overflow"))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Command {
    Tiles {
        index: u32,
        eight_bpp: bool,
        count: u32,
    },
    Palette {
        bank: i32,
        colors: u16,
        insert: u16,
    },
    JumpBack {
        words: i32,
    },
    End,
    Sound {
        id: u16,
    },
    Hitbox {
        index: i32,
        rectangle: [i8; 4],
    },
    Translate {
        x: i16,
        y: i16,
    },
    NoOp8 {
        parameters: [i32; 2],
    },
    Change {
        animation: u16,
        variant: u16,
    },
    RetailUndefined10 {
        parameters: [i32; 3],
    },
    Priority {
        value: i32,
    },
    Order {
        value: i32,
    },
    Show {
        delay: i32,
        frame: i32,
    },
}

/// The returned length is in native 32-bit words. Unknown opcodes and
/// truncated operands are errors; no host pointer is ever executed.
pub fn read_command(source: &[u8], offset: usize) -> Result<(Command, usize), Error> {
    let id = word(source, offset)? as i32;
    let operand = |index| word(source, indexed(offset, index, 4)?);
    let result = match id {
        -1 => {
            let index = operand(1)?;
            (
                Command::Tiles {
                    index: index & 0x7fff_ffff,
                    eight_bpp: index & 0x8000_0000 != 0,
                    count: operand(2)?,
                },
                3,
            )
        }
        -2 => {
            let packed = operand(2)?;
            (
                Command::Palette {
                    bank: operand(1)? as i32,
                    colors: packed as u16,
                    insert: (packed >> 16) as u16,
                },
                3,
            )
        }
        -3 => (
            Command::JumpBack {
                words: operand(1)? as i32,
            },
            2,
        ),
        -4 => (Command::End, 1),
        -5 => (
            Command::Sound {
                id: operand(1)? as u16,
            },
            2,
        ),
        -6 => (
            Command::Hitbox {
                index: operand(1)? as i32,
                rectangle: operand(2)?.to_le_bytes().map(|b| b as i8),
            },
            3,
        ),
        -7 => {
            let packed = operand(1)?;
            (
                Command::Translate {
                    x: packed as i16,
                    y: (packed >> 16) as i16,
                },
                2,
            )
        }
        -8 => (
            Command::NoOp8 {
                parameters: [operand(1)? as i32, operand(2)? as i32],
            },
            3,
        ),
        -9 => {
            let packed = operand(1)?;
            (
                Command::Change {
                    animation: packed as u16,
                    variant: (packed >> 16) as u16,
                },
                2,
            )
        }
        -10 => (
            Command::RetailUndefined10 {
                parameters: [operand(1)? as i32, operand(2)? as i32, operand(3)? as i32],
            },
            4,
        ),
        -11 => (
            Command::Priority {
                value: operand(1)? as i32,
            },
            2,
        ),
        -12 => (
            Command::Order {
                value: operand(1)? as i32,
            },
            2,
        ),
        0.. => (
            Command::Show {
                delay: id,
                frame: operand(1)? as i32,
            },
            2,
        ),
        _ => return Err(err(format!("Unknown sprite opcode {id}"))),
    };
    Ok(result)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameDimensions {
    pub flip: u8,
    pub oam_index: u8,
    pub subframes: u16,
    pub width: u16,
    pub height: u16,
    pub origin_x: i16,
    pub origin_y: i16,
}

impl FrameDimensions {
    fn read(source: &[u8], offset: usize) -> Result<Self, Error> {
        let raw = bounded_range(source, offset, 12)?;
        let u16_at = |n| u16::from_le_bytes([raw[n], raw[n + 1]]);
        let result = Self {
            flip: raw[0],
            oam_index: raw[1],
            subframes: u16_at(2),
            width: u16_at(4),
            height: u16_at(6),
            origin_x: u16_at(8) as i16,
            origin_y: u16_at(10) as i16,
        };
        if result.subframes > 128
            || result.width > 1024
            || result.height > 1024
            || usize::from(result.width) * usize::from(result.height) > MAX_PIXELS
        {
            return Err(err("Sprite frame exceeds bounded OAM/pixel capacity"));
        }
        Ok(result)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hitbox {
    pub index: i32,
    pub rectangle: [i8; 4],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationStep {
    pub animation: u16,
    pub variant: u8,
    pub frame_index: Option<u16>,
    pub changed_frame: bool,
    pub ended: bool,
    pub sounds: Vec<u16>,
    pub commands_executed: usize,
    pub cursor_words: usize,
    pub delay_q8: i16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpriteFrame {
    pub width: usize,
    pub height: usize,
    /// The native logical sprite anchor in this frame's pixel rectangle.
    pub origin_x: i16,
    pub origin_y: i16,
    pub translation_x: i16,
    pub translation_y: i16,
    pub frame_index: Option<u16>,
    pub subframes: usize,
    pub rgba: Vec<u8>,
}

#[derive(Clone, Copy)]
struct Tables<'a> {
    rom: &'a [u8],
    animations: usize,
    dimensions: usize,
    oam: usize,
    palettes: usize,
    tiles4: usize,
    tiles8: usize,
}

impl<'a> Tables<'a> {
    fn load(rom: &'a [u8]) -> Result<Self, Error> {
        let mut offsets = [0; 6];
        for (index, offset) in offsets.iter_mut().enumerate() {
            *offset = address(rom, word(rom, indexed(SPRITE_TABLE_OFFSET, index, 4)?)?)?;
        }
        Ok(Self {
            rom,
            animations: offsets[0],
            dimensions: offsets[1],
            oam: offsets[2],
            palettes: offsets[3],
            tiles4: offsets[4],
            tiles8: offsets[5],
        })
    }

    fn table_entry(&self, table: usize, animation: u16) -> Result<usize, Error> {
        if usize::from(animation) >= ANIMATION_SLOTS {
            return Err(err("Animation ID exceeds native 1133 slots"));
        }
        address(
            self.rom,
            word(self.rom, indexed(table, usize::from(animation), 4)?)?,
        )
    }

    fn script(&self, animation: u16, variant: u8) -> Result<usize, Error> {
        let variants = self.table_entry(self.animations, animation)?;
        // A requested variant cannot cross a non-pointer word. This does not
        // manufacture a variant count from animation-name numbering gaps.
        let mut result = 0;
        for index in 0..=usize::from(variant) {
            result = address(self.rom, word(self.rom, indexed(variants, index, 4)?)?)?;
        }
        Ok(result)
    }
}

pub struct AnimationPlayer<'a> {
    tables: Tables<'a>,
    pub animation: u16,
    pub variant: u8,
    previous: Option<(u16, u8)>,
    cursor: usize,
    delay: i16,
    speed: u8,
    ended: bool,
    frame_index: Option<u16>,
    dimensions: Option<FrameDimensions>,
    tiles: Vec<u8>,
    palette: [u16; 256],
    flip_x: bool,
    flip_y: bool,
    priority: u8,
    order: u8,
    translation_x: i16,
    translation_y: i16,
    hitboxes: [Hitbox; 16],
}

impl<'a> AnimationPlayer<'a> {
    pub fn new(rom: &'a VerifiedRom<'_>, animation: u16, variant: u8) -> Result<Self, Error> {
        Self::from_tables(Tables::load(rom.rom_bytes())?, animation, variant)
    }

    fn from_tables(tables: Tables<'a>, animation: u16, variant: u8) -> Result<Self, Error> {
        tables.script(animation, variant)?;
        Ok(Self {
            tables,
            animation,
            variant,
            previous: None,
            cursor: 0,
            delay: 0,
            speed: 16,
            ended: false,
            frame_index: None,
            dimensions: None,
            tiles: vec![0; OBJ_BYTES],
            palette: [0; 256],
            flip_x: false,
            flip_y: false,
            priority: 0,
            order: 0,
            translation_x: 0,
            translation_y: 0,
            hitboxes: [Hitbox {
                index: -1,
                rectangle: [0; 4],
            }; 16],
        })
    }

    pub fn set_animation(&mut self, animation: u16, variant: u8) -> Result<(), Error> {
        self.tables.script(animation, variant)?;
        self.animation = animation;
        self.variant = variant;
        Ok(())
    }

    /// Native speed units: 0x10 is one source frame per 60 Hz update.
    pub fn set_speed(&mut self, speed: u8) {
        self.speed = speed;
    }

    pub fn set_flip(&mut self, horizontal: bool, vertical: bool) {
        self.flip_x = horizontal;
        self.flip_y = vertical;
    }

    pub fn hitboxes(&self) -> &[Hitbox; 16] {
        &self.hitboxes
    }
    pub fn palette_words(&self) -> &[u16; 256] {
        &self.palette
    }
    pub fn oam_order(&self) -> u8 {
        self.order
    }
    pub fn priority(&self) -> u8 {
        self.priority
    }

    fn step_report(&self, sounds: Vec<u16>, commands: usize, changed: bool) -> AnimationStep {
        AnimationStep {
            animation: self.animation,
            variant: self.variant,
            frame_index: self.frame_index,
            changed_frame: changed,
            ended: self.ended,
            sounds,
            commands_executed: commands,
            cursor_words: self.cursor,
            delay_q8: self.delay,
        }
    }

    /// Executes exactly one native UpdateSpriteAnimation call. A positive
    /// delay is decremented and processing resumes on the following call,
    /// preserving the original timing rather than a wall-clock approximation.
    pub fn advance(&mut self) -> Result<AnimationStep, Error> {
        if self.previous != Some((self.animation, self.variant)) {
            self.previous = Some((self.animation, self.variant));
            self.cursor = 0;
            self.delay = 0;
            self.ended = false;
        }
        if self.ended {
            return Ok(self.step_report(Vec::new(), 0, false));
        }
        let speed_q8 = i16::from(self.speed) * 16;
        if self.delay > 0 {
            self.delay = self.delay.wrapping_sub(speed_q8);
            return Ok(self.step_report(Vec::new(), 0, false));
        }
        let mut sounds = Vec::new();
        for executed in 1..=COMMAND_BUDGET {
            if self.cursor >= MAX_CURSOR_WORDS {
                return Err(err("Animation cursor exceeds host bound"));
            }
            let script = self.tables.script(self.animation, self.variant)?;
            let (command, words) = read_command(self.tables.rom, indexed(script, self.cursor, 4)?)?;
            match command {
                Command::Tiles {
                    index,
                    eight_bpp,
                    count,
                } => {
                    let stride = if eight_bpp { 64 } else { 32 };
                    let size = (count as usize)
                        .checked_mul(stride)
                        .ok_or_else(|| err("Tile transfer overflow"))?;
                    if size > OBJ_BYTES {
                        return Err(err("Tile transfer exceeds OBJ VRAM"));
                    }
                    let bank = if eight_bpp {
                        self.tables.tiles8
                    } else {
                        self.tables.tiles4
                    };
                    let offset = indexed(bank, index as usize, stride)?;
                    self.tiles[..size].copy_from_slice(bounded_range(
                        self.tables.rom,
                        offset,
                        size,
                    )?);
                }
                Command::Palette {
                    bank,
                    colors,
                    insert,
                } => {
                    if bank < 0 {
                        return Err(err("Negative palette source bank"));
                    }
                    let start = usize::from(insert);
                    let end = start
                        .checked_add(usize::from(colors))
                        .ok_or_else(|| err("Palette transfer overflow"))?;
                    if end > self.palette.len() {
                        return Err(err("Palette transfer exceeds OBJ palette"));
                    }
                    let source = indexed(self.tables.palettes, bank as usize, 32)?;
                    for (destination, bytes) in self.palette[start..end].iter_mut().zip(
                        bounded_range(self.tables.rom, source, usize::from(colors) * 2)?
                            .as_chunks::<2>()
                            .0
                            .iter(),
                    ) {
                        *destination = u16::from_le_bytes([bytes[0], bytes[1]]);
                    }
                }
                Command::JumpBack { words } => {
                    let next = (self.cursor as i64) - i64::from(words);
                    if !(0..MAX_CURSOR_WORDS as i64).contains(&next) {
                        return Err(err("Animation jump outside bounded script"));
                    }
                    self.cursor = next as usize;
                    continue;
                }
                Command::End => {
                    self.ended = true;
                    return Ok(self.step_report(sounds, executed, false));
                }
                Command::Sound { id } => sounds.push(id),
                Command::Hitbox {
                    index,
                    mut rectangle,
                } => {
                    let slot = (index as u32 & 15) as usize;
                    if rectangle == [0; 4] {
                        self.hitboxes[slot] = Hitbox {
                            index: -1,
                            rectangle,
                        };
                    } else {
                        if self.flip_y {
                            rectangle.swap(1, 3);
                            rectangle[1] = rectangle[1].wrapping_neg();
                            rectangle[3] = rectangle[3].wrapping_neg();
                        }
                        if self.flip_x {
                            rectangle.swap(0, 2);
                            rectangle[0] = rectangle[0].wrapping_neg();
                            rectangle[2] = rectangle[2].wrapping_neg();
                        }
                        self.hitboxes[slot] = Hitbox { index, rectangle };
                    }
                }
                Command::Translate { x, y } => {
                    self.translation_x = self.translation_x.wrapping_add(x);
                    self.translation_y = self.translation_y.wrapping_add(y);
                }
                Command::NoOp8 { .. } => {}
                Command::Change { animation, variant } => {
                    let variant = u8::try_from(variant)
                        .map_err(|_| err("Animation variant exceeds native byte"))?;
                    self.tables.script(animation, variant)?;
                    self.animation = animation;
                    self.variant = variant;
                    self.previous = None;
                    self.cursor = 0;
                    continue;
                }
                Command::RetailUndefined10 { .. } => {
                    return Err(err(
                        "Native opcode -10 returns a code pointer; behavior is unported",
                    ));
                }
                Command::Priority { value } => {
                    self.priority = u8::try_from(value)
                        .ok()
                        .filter(|v| *v <= 3)
                        .ok_or_else(|| err("Invalid sprite priority"))?;
                }
                Command::Order { value } => {
                    self.order = u8::try_from(value)
                        .ok()
                        .filter(|v| *v <= 31)
                        .ok_or_else(|| err("Invalid OAM order"))?;
                }
                Command::Show { delay, frame } => {
                    if !(-1..=4095).contains(&frame) {
                        return Err(err("Invalid sprite frame index"));
                    }
                    self.delay = self
                        .delay
                        .wrapping_add((delay as i16).wrapping_shl(8))
                        .wrapping_sub(speed_q8);
                    self.frame_index = if frame == -1 {
                        None
                    } else {
                        Some(frame as u16)
                    };
                    self.dimensions = if frame == -1 {
                        None
                    } else {
                        let base = self
                            .tables
                            .table_entry(self.tables.dimensions, self.animation)?;
                        Some(FrameDimensions::read(
                            self.tables.rom,
                            indexed(base, frame as usize, 12)?,
                        )?)
                    };
                    self.cursor += words;
                    return Ok(self.step_report(sounds, executed, true));
                }
            }
            self.cursor = self
                .cursor
                .checked_add(words)
                .ok_or_else(|| err("Animation cursor overflow"))?;
        }
        Err(err("Animation command budget exhausted before a frame/end"))
    }

    pub fn dimensions(&self) -> Option<FrameDimensions> {
        self.dimensions
    }

    /// Produces the native frame rectangle with its native anchor. OBJ index
    /// zero is transparent. The caller applies world position and camera.
    /// Affine, mosaic, semi-transparent and object-window modes are explicit
    /// errors, rather than silently converted to ordinary opaque sprites.
    pub fn render(&self) -> Result<SpriteFrame, Error> {
        let Some(dimensions) = self.dimensions else {
            return Ok(SpriteFrame {
                width: 0,
                height: 0,
                origin_x: 0,
                origin_y: 0,
                translation_x: self.translation_x,
                translation_y: self.translation_y,
                frame_index: self.frame_index,
                subframes: 0,
                rgba: Vec::new(),
            });
        };
        let width = usize::from(dimensions.width);
        let height = usize::from(dimensions.height);
        let mut rgba = vec![0; width * height * 4];
        let mut winning = vec![(u8::MAX, usize::MAX); width * height];
        let oam_base = self.tables.table_entry(self.tables.oam, self.animation)?;
        let flip_x = self.flip_x ^ (dimensions.flip & 1 != 0);
        let flip_y = self.flip_y ^ (dimensions.flip & 2 != 0);
        for part in 0..usize::from(dimensions.subframes) {
            let offset = indexed(oam_base, usize::from(dimensions.oam_index) + part, 6)?;
            let raw = bounded_range(self.tables.rom, offset, 6)?;
            let a0 = u16::from_le_bytes([raw[0], raw[1]]);
            let a1 = u16::from_le_bytes([raw[2], raw[3]]);
            let a2 = u16::from_le_bytes([raw[4], raw[5]]);
            // DisplaySprite clears raw attr0 bit8 with 0xFE00 before
            // choosing affine mode from the live Sprite flags. Several
            // original walk templates set it; it is not a live matrix here.
            if a0 & 0x1e00 != 0 {
                return Err(err(
                    "Affine/disabled/mosaic/blended/window OAM mode is unported",
                ));
            }
            let (part_width, part_height) = object_size((a0 >> 14) as u8, (a1 >> 14) as u8)?;
            let mut x = i32::from(a1 & 0x1ff);
            let mut y = i32::from(a0 & 0xff);
            if x >= 256 {
                x -= 512;
            }
            if y >= 128 {
                y -= 256;
            }
            if flip_x {
                x = width as i32 - part_width as i32 - x;
            }
            if flip_y {
                y = height as i32 - part_height as i32 - y;
            }
            let tile_base = usize::from(a2 & 0x3ff);
            let eight_bpp = a0 & 0x2000 != 0;
            let stride = if eight_bpp { 64 } else { 32 };
            let horizontal = flip_x ^ (a1 & 0x1000 != 0);
            let vertical = flip_y ^ (a1 & 0x2000 != 0);
            let palette_bank = usize::from(a2 >> 12);
            let priority = ((a2 >> 10) as u8 & 3) | self.priority;
            for local_y in 0..part_height {
                for local_x in 0..part_width {
                    let output_x = x + local_x as i32;
                    let output_y = y + local_y as i32;
                    if output_x < 0
                        || output_y < 0
                        || output_x >= width as i32
                        || output_y >= height as i32
                    {
                        continue;
                    }
                    let sx = if horizontal {
                        part_width - 1 - local_x
                    } else {
                        local_x
                    };
                    let sy = if vertical {
                        part_height - 1 - local_y
                    } else {
                        local_y
                    };
                    let tile = tile_base + (sy / 8) * (part_width / 8) + sx / 8;
                    let within = (sy % 8) * 8 + sx % 8;
                    let packed = tile
                        .checked_mul(stride)
                        .and_then(|n| n.checked_add(if eight_bpp { within } else { within / 2 }))
                        .ok_or_else(|| err("OBJ tile address overflow"))?;
                    let byte = *self
                        .tiles
                        .get(packed)
                        .ok_or_else(|| err("OAM tile index exceeds OBJ VRAM"))?;
                    let index = if eight_bpp {
                        byte
                    } else {
                        (byte >> ((within % 2) * 4)) & 15
                    };
                    if index == 0 {
                        continue;
                    }
                    let destination = output_y as usize * width + output_x as usize;
                    let rank = (priority, part);
                    if rank >= winning[destination] {
                        continue;
                    }
                    winning[destination] = rank;
                    let color_index =
                        usize::from(index) + if eight_bpp { 0 } else { palette_bank * 16 };
                    let color = PaletteColor::from_word(self.palette[color_index]).rgb8();
                    rgba[destination * 4..destination * 4 + 3].copy_from_slice(&color);
                    rgba[destination * 4 + 3] = 255;
                }
            }
        }
        Ok(SpriteFrame {
            width,
            height,
            origin_x: if self.flip_x {
                dimensions.width as i16 - dimensions.origin_x
            } else {
                dimensions.origin_x
            },
            origin_y: if self.flip_y {
                dimensions.height as i16 - dimensions.origin_y
            } else {
                dimensions.origin_y
            },
            translation_x: self.translation_x,
            translation_y: self.translation_y,
            frame_index: self.frame_index,
            subframes: usize::from(dimensions.subframes),
            rgba,
        })
    }
}

fn object_size(shape: u8, size: u8) -> Result<(usize, usize), Error> {
    const SIZES: [[(usize, usize); 4]; 3] = [
        [(8, 8), (16, 16), (32, 32), (64, 64)],
        [(16, 8), (32, 8), (32, 16), (64, 32)],
        [(8, 16), (8, 32), (16, 32), (32, 64)],
    ];
    SIZES
        .get(usize::from(shape))
        .and_then(|row| row.get(usize::from(size)))
        .copied()
        .ok_or_else(|| err("Invalid native OAM shape/size"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(source: &mut [u8], offset: usize, values: &[i32]) {
        for (i, value) in values.iter().enumerate() {
            source[offset + i * 4..offset + i * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
    }

    fn setup(script: &[i32]) -> (Vec<u8>, [usize; 6]) {
        let mut rom = vec![0; 4096];
        let offsets = [0, 16, 32, 64, 1024, 2048];
        put(&mut rom, 0, &[0x08000030]);
        put(&mut rom, 16, &[0x08000080]);
        put(&mut rom, 32, &[0x080000a0]);
        put(&mut rom, 48, &[0x08000100]);
        put(&mut rom, 256, script);
        rom[128..140].copy_from_slice(&[0, 0, 1, 0, 8, 0, 8, 0, 4, 0, 7, 0]);
        rom[66..68].copy_from_slice(&0x001fu16.to_le_bytes());
        rom[1024..1056].fill(0x11);
        (rom, offsets)
    }

    fn tables(rom: &[u8], offsets: [usize; 6]) -> Tables<'_> {
        Tables {
            rom,
            animations: offsets[0],
            dimensions: offsets[1],
            oam: offsets[2],
            palettes: offsets[3],
            tiles4: offsets[4],
            tiles8: offsets[5],
        }
    }

    #[test]
    fn every_command_rejects_truncated_operands_and_unknown_ids() {
        for script in [
            vec![-1, 0, 1],
            vec![-2, 0, 16],
            vec![-3, 0],
            vec![-4],
            vec![-5, 7],
            vec![-6, 0, 0],
            vec![-7, 0],
            vec![-8, 0, 0],
            vec![-9, 0],
            vec![-10, 0, 0, 0],
            vec![-11, 0],
            vec![-12, 0],
            vec![6, 0],
        ] {
            let bytes: Vec<_> = script
                .iter()
                .flat_map(|word: &i32| word.to_le_bytes())
                .collect();
            assert!(read_command(&bytes, 0).is_ok());
            for length in 0..bytes.len() {
                assert!(read_command(&bytes[..length], 0).is_err());
            }
        }
        assert!(read_command(&(-13_i32).to_le_bytes(), 0).is_err());
        assert!(read_command(&[0; 8], usize::MAX).is_err());
    }

    #[test]
    fn native_delay_has_separate_expiry_call_and_jump_is_bounded() {
        let (rom, offsets) = setup(&[2, 0, -3, 2]);
        let mut player = AnimationPlayer::from_tables(tables(&rom, offsets), 0, 0).unwrap();
        assert!(player.advance().unwrap().changed_frame);
        assert_eq!(player.delay, 256);
        assert!(!player.advance().unwrap().changed_frame);
        assert_eq!(player.delay, 0);
        assert!(player.advance().unwrap().changed_frame);
        let (rom, offsets) = setup(&[-3, 0]);
        assert!(
            AnimationPlayer::from_tables(tables(&rom, offsets), 0, 0)
                .unwrap()
                .advance()
                .is_err()
        );
        let (rom, offsets) = setup(&[-3, 1]);
        assert!(
            AnimationPlayer::from_tables(tables(&rom, offsets), 0, 0)
                .unwrap()
                .advance()
                .is_err()
        );
    }

    #[test]
    fn tile_palette_upload_and_transparent_pixel_composition_are_executable() {
        let (mut rom, offsets) = setup(&[-2, 0, 16, -1, 0, 1, 1, 0, -4]);
        rom[1024] = 0x10;
        let mut player = AnimationPlayer::from_tables(tables(&rom, offsets), 0, 0).unwrap();
        player.advance().unwrap();
        let image = player.render().unwrap();
        assert_eq!(
            (image.width, image.height, image.origin_x, image.origin_y),
            (8, 8, 4, 7)
        );
        assert_eq!(&image.rgba[0..4], &[0, 0, 0, 0]);
        assert_eq!(&image.rgba[4..8], &[255, 0, 0, 255]);
        player.set_flip(true, false);
        let image = player.render().unwrap();
        assert_eq!(&image.rgba[28..32], &[0, 0, 0, 0]);
        assert_eq!(&image.rgba[24..28], &[255, 0, 0, 255]);
        assert!(player.advance().unwrap().ended);
    }

    #[test]
    fn malformed_uploads_frames_and_retail_undefined_command_fail_explicitly() {
        for script in [
            vec![-1, 0, 1025],
            vec![-2, 0, (250 << 16) | 16],
            vec![1, -2],
            vec![-10, 0, 0, 0],
            vec![-11, 4],
            vec![-12, 32],
        ] {
            let (rom, offsets) = setup(&script);
            assert!(
                AnimationPlayer::from_tables(tables(&rom, offsets), 0, 0)
                    .unwrap()
                    .advance()
                    .is_err()
            );
        }
        let (rom, offsets) = setup(&[1, 0]);
        assert!(AnimationPlayer::from_tables(tables(&rom, offsets), 0, 1).is_err());
        assert!(
            AnimationPlayer::from_tables(tables(&rom, offsets), ANIMATION_SLOTS as u16, 0).is_err()
        );
    }

    #[test]
    fn animation_change_reloads_the_script_and_preserves_native_next_call_reset() {
        let (mut rom, offsets) = setup(&[-9, 1]);
        put(&mut rom, 4, &[0x08000034]);
        put(&mut rom, 20, &[0x08000080]);
        put(&mut rom, 36, &[0x080000a0]);
        put(&mut rom, 52, &[0x08000200]);
        put(&mut rom, 512, &[3, 0, -4]);
        let mut player = AnimationPlayer::from_tables(tables(&rom, offsets), 0, 0).unwrap();
        let first = player.advance().unwrap();
        assert_eq!(
            (first.animation, first.frame_index, first.delay_q8),
            (1, Some(0), 512)
        );
        // Native CHANGE_ANIM writes prevVariant=-1, so the next call's
        // SPRITE_INIT_ANIM_IF_CHANGED starts this script again once.
        let reset = player.advance().unwrap();
        assert!(reset.changed_frame);
        assert_eq!(reset.delay_q8, 512);
        assert!(!player.advance().unwrap().changed_frame);
        assert_eq!(player.delay, 256);
        player.advance().unwrap();
        assert!(player.advance().unwrap().ended);
    }

    #[test]
    fn raw_template_affine_bit_is_cleared_and_unsupported_modes_remain_explicit() {
        for (attributes, should_render) in [
            (0x0100_u16, true),
            (0x0400, false),
            (0x1000, false),
            (0xc000, false),
        ] {
            let (mut rom, offsets) = setup(&[-2, 0, 16, -1, 0, 1, 1, 0]);
            rom[160..162].copy_from_slice(&attributes.to_le_bytes());
            let mut player = AnimationPlayer::from_tables(tables(&rom, offsets), 0, 0).unwrap();
            player.advance().unwrap();
            assert_eq!(player.render().is_ok(), should_render);
        }
        let (mut rom, offsets) = setup(&[1, 0]);
        rom[132..134].copy_from_slice(&u16::MAX.to_le_bytes());
        let mut player = AnimationPlayer::from_tables(tables(&rom, offsets), 0, 0).unwrap();
        assert!(player.advance().is_err());
    }
}
