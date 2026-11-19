//! Source-gated SA1 sprite command execution and normal, one-dimensional OBJ
//! composition. This is a host representation of sprite state, not GBA code
//! emulation. Affine objects, blend/window modes, mosaic and opcode -10 remain
//! explicit errors until their native behavior has independent evidence.
use crate::{binary, graphics, Error, Result};

const SPRITE_ROOT: usize = 0xd868c;
const ANIMATION_COUNT: usize = 906;
const SCRIPT_START: usize = 0xd86a4;
const SCRIPT_END: usize = 0xfe3a8;
const OBJECT_BYTES: usize = 0x8000;
const COMMAND_LIMIT: usize = 256;

#[derive(Debug, Clone, Copy)]
pub struct Program<'a> {
    pub bytes: &'a [u8],
    pub entry_words: usize,
}

/// Supplies bounded program regions and data to the command machine. Public
/// implementations permit synthetic tests without embedding proprietary data.
pub trait AnimationSource {
    fn program(&self, animation: u16, variant: u8) -> Result<Program<'_>>;
    fn tiles(&self, index: u32, count: usize, eight_bit: bool) -> Result<&[u8]>;
    fn palette(&self, index: u32, colors: usize) -> Result<&[u8]>;
}

#[derive(Debug)]
pub struct SpriteBank<'a> {
    rom: &'a [u8],
    roots: [usize; 6],
    animation_roots: Vec<Option<usize>>,
    dimension_roots: Vec<Option<usize>>,
    oam_roots: Vec<Option<usize>>,
}

fn non_null(table: &[Option<usize>], id: u16) -> Result<usize> {
    table
        .get(usize::from(id))
        .copied()
        .flatten()
        .ok_or_else(|| Error(format!("missing sprite animation slot {id}")))
}

impl<'a> SpriteBank<'a> {
    pub fn from_asoe(rom: &'a [u8]) -> Result<Self> {
        binary::verify_asoe(rom)?;
        let root_vec = binary::pointer_table(rom, SPRITE_ROOT, 6, 6)?;
        let roots: [usize; 6] = root_vec
            .try_into()
            .map_err(|_| Error("sprite root length mismatch".into()))?;
        if roots != [0xfe3a8, 0x10acb4, 0x119a2c, 0x11a854, 0x11e254, 0x465074] {
            return Err(Error("ASOE sprite root mismatch".into()));
        }
        Ok(Self {
            rom,
            animation_roots: binary::nullable_pointer_table(
                rom,
                roots[0],
                ANIMATION_COUNT,
                ANIMATION_COUNT,
            )?,
            dimension_roots: binary::nullable_pointer_table(
                rom,
                roots[1],
                ANIMATION_COUNT,
                ANIMATION_COUNT,
            )?,
            oam_roots: binary::nullable_pointer_table(
                rom,
                roots[2],
                ANIMATION_COUNT,
                ANIMATION_COUNT,
            )?,
            roots,
        })
    }

    /// Variant lists are consecutive bounded ROM pointers immediately following
    /// their script group. Never interpret an arbitrary variant byte as an
    /// unchecked dereference or continue into the following command stream.
    fn variants(&self, animation: u16) -> Result<(usize, Vec<usize>)> {
        let table = non_null(&self.animation_roots, animation)?;
        if !(SCRIPT_START..SCRIPT_END).contains(&table) || table % 4 != 0 {
            return Err(Error(
                "variant table is outside the ASOE script bank".into(),
            ));
        }
        let mut entries = Vec::new();
        for index in 0..=256 {
            let at = table
                .checked_add(index * 4)
                .ok_or_else(|| Error("variant cursor overflow".into()))?;
            if at >= SCRIPT_END {
                break;
            }
            let pointer = binary::u32_at(self.rom, at)?;
            let candidate = binary::rom_offset(pointer, self.rom.len(), 4).ok();
            let Some(offset) = candidate.filter(|offset| (SCRIPT_START..table).contains(offset))
            else {
                break;
            };
            if index == 256 {
                return Err(Error("variant list exceeds byte-index budget".into()));
            }
            entries.push(offset);
        }
        if entries.is_empty() {
            return Err(Error("empty or malformed variant list".into()));
        }
        Ok((table, entries))
    }

    pub fn variant_count(&self, animation: u16) -> Result<usize> {
        self.variants(animation).map(|(_, entries)| entries.len())
    }

    pub fn dimensions(&self, animation: u16, frame: u32) -> Result<FrameDimensions> {
        let start = non_null(&self.dimension_roots, animation)?;
        let limit = self
            .dimension_roots
            .iter()
            .flatten()
            .copied()
            .filter(|&at| at > start)
            .min()
            .unwrap_or(self.roots[1]);
        let frame_index =
            usize::try_from(frame).map_err(|_| Error("sprite frame index overflow".into()))?;
        let offset = start
            .checked_add(
                frame_index
                    .checked_mul(12)
                    .ok_or_else(|| Error("dimension index overflow".into()))?,
            )
            .ok_or_else(|| Error("dimension cursor overflow".into()))?;
        if offset.checked_add(12).is_none_or(|end| end > limit) {
            return Err(Error("frame exceeds its bounded dimension records".into()));
        }
        let raw = binary::range(self.rom, offset, 12)?;
        let dimensions = FrameDimensions {
            source_offset: offset,
            flip: raw[0],
            oam_index: raw[1],
            subframes: u16::from_le_bytes([raw[2], raw[3]]),
            width: u16::from_le_bytes([raw[4], raw[5]]),
            height: u16::from_le_bytes([raw[6], raw[7]]),
            anchor_x: i16::from_le_bytes([raw[8], raw[9]]),
            anchor_y: i16::from_le_bytes([raw[10], raw[11]]),
        };
        if dimensions.flip > 3
            || dimensions.subframes > 128
            || dimensions.width == 0
            || dimensions.height == 0
            || dimensions.width > 512
            || dimensions.height > 512
        {
            return Err(Error(
                "unsupported sprite dimensions or subframe count".into(),
            ));
        }
        Ok(dimensions)
    }

    pub fn render(&self, player: &SpritePlayer) -> Result<Option<RenderedSprite>> {
        let Some(frame) = player.current_frame else {
            return Ok(None);
        };
        let dimensions = self.dimensions(player.animation, frame)?;
        let oam_start = non_null(&self.oam_roots, player.animation)?;
        let oam_limit = self
            .oam_roots
            .iter()
            .flatten()
            .copied()
            .filter(|&at| at > oam_start)
            .min()
            .unwrap_or(self.roots[2]);
        let record_start = oam_start
            .checked_add(usize::from(dimensions.oam_index) * 6)
            .ok_or_else(|| Error("OAM offset overflow".into()))?;
        let count = usize::from(dimensions.subframes);
        if record_start
            .checked_add(count * 6)
            .is_none_or(|end| end > oam_limit)
        {
            return Err(Error("sprite subframes exceed bounded OAM records".into()));
        }
        let oam = binary::range(self.rom, record_start, count * 6)?;
        let objects = oam
            .as_chunks::<6>()
            .0
            .iter()
            .map(|raw| OamObject::decode(raw))
            .collect::<Result<Vec<_>>>()?;
        compose(&dimensions, &objects, player)
    }
}

impl AnimationSource for SpriteBank<'_> {
    fn program(&self, animation: u16, variant: u8) -> Result<Program<'_>> {
        let (end, variants) = self.variants(animation)?;
        let entry = *variants
            .get(usize::from(variant))
            .ok_or_else(|| Error("variant exceeds native list".into()))?;
        let start = *variants
            .iter()
            .min()
            .ok_or_else(|| Error("empty variant list".into()))?;
        if end - start > 65536 {
            return Err(Error("sprite program group exceeds 64 KiB".into()));
        }
        Ok(Program {
            bytes: binary::range(self.rom, start, end - start)?,
            entry_words: (entry - start) / 4,
        })
    }

    fn tiles(&self, index: u32, count: usize, eight_bit: bool) -> Result<&[u8]> {
        let bytes_per_tile = if eight_bit { 64 } else { 32 };
        let length = count
            .checked_mul(bytes_per_tile)
            .ok_or_else(|| Error("tile byte count overflow".into()))?;
        if count == 0 || length > OBJECT_BYTES {
            return Err(Error("tile command exceeds OBJ memory budget".into()));
        }
        let tile = usize::try_from(index).map_err(|_| Error("tile index overflow".into()))?;
        let base = self.roots[if eight_bit { 5 } else { 4 }];
        let offset = base
            .checked_add(
                tile.checked_mul(bytes_per_tile)
                    .ok_or_else(|| Error("tile index overflow".into()))?,
            )
            .ok_or_else(|| Error("tile offset overflow".into()))?;
        if !eight_bit
            && offset
                .checked_add(length)
                .is_none_or(|end| end > self.roots[5])
        {
            return Err(Error("4bpp sprite tiles cross the next bank".into()));
        }
        binary::range(self.rom, offset, length)
    }

    fn palette(&self, index: u32, colors: usize) -> Result<&[u8]> {
        let offset = self.roots[3]
            .checked_add(
                usize::try_from(index)
                    .map_err(|_| Error("palette index overflow".into()))?
                    .checked_mul(32)
                    .ok_or_else(|| Error("palette index overflow".into()))?,
            )
            .ok_or_else(|| Error("palette offset overflow".into()))?;
        let length = colors
            .checked_mul(2)
            .ok_or_else(|| Error("palette size overflow".into()))?;
        if colors == 0
            || colors > 256
            || offset
                .checked_add(length)
                .is_none_or(|end| end > self.roots[4])
        {
            return Err(Error("palette command crosses bounded bank".into()));
        }
        binary::range(self.rom, offset, length)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hitbox {
    pub index: i32,
    pub left: i8,
    pub top: i8,
    pub right: i8,
    pub bottom: i8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpriteEvent {
    Frame(Option<u32>),
    Sound(u16),
    Tiles {
        index: u32,
        count: usize,
        eight_bit: bool,
    },
    Palette {
        index: u32,
        count: usize,
        destination: usize,
    },
    Hitbox {
        slot: usize,
        value: Option<Hitbox>,
    },
    Translate {
        x: i16,
        y: i16,
    },
    Noop8 {
        operands: [u32; 2],
    },
    AnimationChanged {
        animation: u16,
        variant: u8,
    },
    Priority(u8),
    OamOrder(u8),
    Ended,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TickResult {
    pub events: Vec<SpriteEvent>,
    pub ended: bool,
}

#[derive(Debug, Clone)]
pub struct SpritePlayer {
    pub animation: u16,
    pub variant: u8,
    /// Native Q4.4 speed: 0x10 is one frame per call.
    pub speed_q4_4: u8,
    pub flip_x: bool,
    pub flip_y: bool,
    pub current_frame: Option<u32>,
    pub translation_x: i16,
    pub translation_y: i16,
    pub hitboxes: [Option<Hitbox>; 16],
    pub priority: u8,
    pub oam_order: u8,
    pub ended: bool,
    previous: Option<(u16, u8)>,
    cursor_words: usize,
    delay_q8: i16,
    object_tiles: Vec<u8>,
    palette: [[u8; 4]; 256],
    palette_bank_offset: u8,
}

impl SpritePlayer {
    pub fn new(animation: u16, variant: u8) -> Self {
        Self {
            animation,
            variant,
            speed_q4_4: 16,
            flip_x: false,
            flip_y: false,
            current_frame: None,
            translation_x: 0,
            translation_y: 0,
            hitboxes: [None; 16],
            priority: 0,
            oam_order: 0,
            ended: false,
            previous: None,
            cursor_words: 0,
            delay_q8: 0,
            object_tiles: vec![0; OBJECT_BYTES],
            palette: [[0, 0, 0, 255]; 256],
            palette_bank_offset: 0,
        }
    }

    pub fn delay_q8(&self) -> i16 {
        self.delay_q8
    }

    /// Install a full native palette bank and the sprite's 4bpp OAM palette offset.
    pub fn set_palette_override(
        &mut self,
        source: &impl AnimationSource,
        source_bank: u32,
        destination_bank: u8,
        add_bank: u8,
    ) -> Result<()> {
        if destination_bank >= 16 || add_bank >= 16 {
            return Err(Error("OBJ palette bank outside 4-bit field".into()));
        }
        let colors = graphics::decode_palette(source.palette(source_bank, 16)?, 16)?;
        if colors.len() != 16 {
            return Err(Error("truncated OBJ palette bank".into()));
        }
        let start = usize::from(destination_bank) * 16;
        self.palette[start..start + 16].copy_from_slice(&colors);
        self.palette_bank_offset = add_bank;
        Ok(())
    }

    pub fn restart_variant(&mut self, variant: u8) {
        self.variant = variant;
        self.previous = None;
    }

    /// A failed step does not publish partial tile/palette/state mutations.
    /// Native unbounded loops become an explicit error after 256 commands.
    pub fn tick(&mut self, source: &impl AnimationSource) -> Result<TickResult> {
        let mut next = self.clone();
        let result = next.step(source)?;
        *self = next;
        Ok(result)
    }

    fn step(&mut self, source: &impl AnimationSource) -> Result<TickResult> {
        let mut program = source.program(self.animation, self.variant)?;
        validate_program(program)?;
        if self.previous != Some((self.animation, self.variant)) {
            self.previous = Some((self.animation, self.variant));
            self.cursor_words = program.entry_words;
            self.delay_q8 = 0;
            self.ended = false;
        }
        let mut events = Vec::new();
        if self.ended {
            return Ok(TickResult {
                events,
                ended: true,
            });
        }
        let advance = i16::from(self.speed_q4_4) * 16;
        if self.delay_q8 > 0 {
            self.delay_q8 = self.delay_q8.wrapping_sub(advance);
            return Ok(TickResult {
                events,
                ended: false,
            });
        }
        for _ in 0..COMMAND_LIMIT {
            let at = self.cursor_words;
            let opcode = word(program.bytes, at)? as i32;
            let operand = |index| word(program.bytes, at + index);
            match opcode {
                0..=127 => {
                    let frame = operand(1)? as i32;
                    if frame < -1 {
                        return Err(Error("negative sprite frame index".into()));
                    }
                    self.current_frame = if frame == -1 {
                        None
                    } else {
                        Some(frame as u32)
                    };
                    self.delay_q8 = self
                        .delay_q8
                        .wrapping_add((opcode as i16).wrapping_mul(256))
                        .wrapping_sub(advance);
                    self.cursor_words += 2;
                    events.push(SpriteEvent::Frame(self.current_frame));
                    return Ok(TickResult {
                        events,
                        ended: false,
                    });
                }
                -1 => {
                    let raw_index = operand(1)?;
                    let count = usize::try_from(operand(2)?)
                        .map_err(|_| Error("tile count overflow".into()))?;
                    let eight_bit = raw_index & 0x8000_0000 != 0;
                    let index = raw_index & 0x7fff_ffff;
                    let tiles = source.tiles(index, count, eight_bit)?;
                    let expected = count
                        .checked_mul(if eight_bit { 64 } else { 32 })
                        .ok_or_else(|| Error("tile command overflow".into()))?;
                    if expected == 0 || expected > OBJECT_BYTES || tiles.len() != expected {
                        return Err(Error("invalid OBJ tile command extent".into()));
                    }
                    self.object_tiles[..expected].copy_from_slice(tiles);
                    self.cursor_words += 3;
                    events.push(SpriteEvent::Tiles {
                        index,
                        count,
                        eight_bit,
                    });
                }
                -2 => {
                    let index = operand(1)?;
                    let packed = operand(2)?;
                    let count = (packed & 0xffff) as usize;
                    let destination = (packed >> 16) as usize;
                    if count == 0 || destination.checked_add(count).is_none_or(|end| end > 256) {
                        return Err(Error("OBJ palette command exceeds 256 colors".into()));
                    }
                    let colors = graphics::decode_palette(source.palette(index, count)?, 256)?;
                    if colors.len() != count {
                        return Err(Error("palette source length mismatch".into()));
                    }
                    self.palette[destination..destination + count].copy_from_slice(&colors);
                    self.cursor_words += 3;
                    events.push(SpriteEvent::Palette {
                        index,
                        count,
                        destination,
                    });
                }
                -3 => {
                    let distance = operand(1)? as i32;
                    let target = i64::try_from(at)
                        .map_err(|_| Error("animation cursor overflow".into()))?
                        - i64::from(distance);
                    if target < 0 || target as u64 >= (program.bytes.len() / 4) as u64 {
                        return Err(Error("animation branch escapes bounded program".into()));
                    }
                    if !is_instruction_boundary(program.bytes, target as usize)? {
                        return Err(Error("animation branch targets an operand".into()));
                    }
                    self.cursor_words = target as usize;
                }
                -4 => {
                    self.ended = true;
                    events.push(SpriteEvent::Ended);
                    return Ok(TickResult {
                        events,
                        ended: true,
                    });
                }
                -5 => {
                    events.push(SpriteEvent::Sound(operand(1)? as u16));
                    self.cursor_words += 2;
                }
                -6 => {
                    let index = operand(1)? as i32;
                    let slot = (index as u32 % 16) as usize;
                    let packed = operand(2)?.to_le_bytes();
                    let mut value = Hitbox {
                        index,
                        left: packed[0] as i8,
                        top: packed[1] as i8,
                        right: packed[2] as i8,
                        bottom: packed[3] as i8,
                    };
                    let active = packed != [0; 4];
                    if self.flip_x {
                        (value.left, value.right) =
                            (value.right.wrapping_neg(), value.left.wrapping_neg());
                    }
                    if self.flip_y {
                        (value.top, value.bottom) =
                            (value.bottom.wrapping_neg(), value.top.wrapping_neg());
                    }
                    self.hitboxes[slot] = active.then_some(value);
                    events.push(SpriteEvent::Hitbox {
                        slot,
                        value: self.hitboxes[slot],
                    });
                    self.cursor_words += 3;
                }
                -7 => {
                    let packed = operand(1)?;
                    let x = packed as u16 as i16;
                    let y = (packed >> 16) as u16 as i16;
                    self.translation_x = self.translation_x.wrapping_add(x);
                    self.translation_y = self.translation_y.wrapping_add(y);
                    events.push(SpriteEvent::Translate { x, y });
                    self.cursor_words += 2;
                }
                -8 => {
                    events.push(SpriteEvent::Noop8 {
                        operands: [operand(1)?, operand(2)?],
                    });
                    self.cursor_words += 3;
                }
                -9 => {
                    let packed = operand(1)?;
                    let variant = packed >> 16;
                    if variant > 255 {
                        return Err(Error("animation variant does not fit native byte".into()));
                    }
                    self.animation = packed as u16;
                    self.variant = variant as u8;
                    // The native handler invalidates prevVariant, then enters
                    // the new program immediately; next tick initializes it.
                    self.previous = None;
                    program = source.program(self.animation, self.variant)?;
                    validate_program(program)?;
                    self.cursor_words = program.entry_words;
                    events.push(SpriteEvent::AnimationChanged {
                        animation: self.animation,
                        variant: self.variant,
                    });
                }
                -11 => {
                    let priority = operand(1)?;
                    if priority > 3 {
                        return Err(Error("unsupported OBJ priority".into()));
                    }
                    self.priority = priority as u8;
                    self.cursor_words += 2;
                    events.push(SpriteEvent::Priority(self.priority));
                }
                -12 => {
                    let order = operand(1)?;
                    if order > 31 {
                        return Err(Error("unsupported OAM order".into()));
                    }
                    self.oam_order = order as u8;
                    self.cursor_words += 2;
                    events.push(SpriteEvent::OamOrder(self.oam_order));
                }
                _ => return Err(Error(format!("unsupported sprite command {opcode}"))),
            }
        }
        Err(Error("sprite command execution budget exhausted".into()))
    }
}

fn word(bytes: &[u8], index: usize) -> Result<u32> {
    let offset = index
        .checked_mul(4)
        .ok_or_else(|| Error("animation word offset overflow".into()))?;
    binary::u32_at(bytes, offset)
}

fn validate_program(program: Program<'_>) -> Result<()> {
    if program.bytes.is_empty()
        || program.bytes.len() > 65536
        || !program.bytes.len().is_multiple_of(4)
        || !is_instruction_boundary(program.bytes, program.entry_words)?
    {
        return Err(Error(
            "invalid bounded animation program or entry point".into(),
        ));
    }
    Ok(())
}

fn is_instruction_boundary(bytes: &[u8], target: usize) -> Result<bool> {
    let mut cursor = 0;
    while cursor < bytes.len() / 4 {
        if cursor == target {
            return Ok(true);
        }
        let length = match word(bytes, cursor)? as i32 {
            0..=i32::MAX | -3 | -5 | -7 | -9 | -11 | -12 => 2,
            -1 | -2 | -6 | -8 => 3,
            -4 => 1,
            -10 => 4,
            _ => {
                return Err(Error(
                    "unknown command before animation branch target".into(),
                ))
            }
        };
        cursor += length;
    }
    Ok(false)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameDimensions {
    pub source_offset: usize,
    pub flip: u8,
    pub oam_index: u8,
    pub subframes: u16,
    pub width: u16,
    pub height: u16,
    pub anchor_x: i16,
    pub anchor_y: i16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OamObject {
    pub x: i16,
    pub y: i16,
    pub width: u16,
    pub height: u16,
    pub tile: u16,
    pub palette_bank: u8,
    pub priority: u8,
    pub flip_x: bool,
    pub flip_y: bool,
    pub eight_bit: bool,
}

impl OamObject {
    /// Decode a native OAM *template*. DisplaySprite clears attr0 bit 8 before
    /// applying the caller's transform flags; walking/running templates have
    /// that bit set even when the caller requests normal unscaled rendering.
    pub fn decode(raw: &[u8]) -> Result<Self> {
        let data = binary::range(raw, 0, 6)?;
        let attr0 = u16::from_le_bytes([data[0], data[1]]);
        let attr1 = u16::from_le_bytes([data[2], data[3]]);
        let attr2 = u16::from_le_bytes([data[4], data[5]]);
        if attr0 & 0x1e00 != 0 {
            return Err(Error(
                "double-size/hide, blend/window or mosaic OBJ is unsupported".into(),
            ));
        }
        let shape = (attr0 >> 14) as usize;
        let size = (attr1 >> 14) as usize;
        let sizes = [
            [(8, 8), (16, 16), (32, 32), (64, 64)],
            [(16, 8), (32, 8), (32, 16), (64, 32)],
            [(8, 16), (8, 32), (16, 32), (32, 64)],
        ];
        let (width, height) = *sizes
            .get(shape)
            .and_then(|row| row.get(size))
            .ok_or_else(|| Error("prohibited OAM shape".into()))?;
        let raw_x = (attr1 & 0x1ff) as i16;
        let raw_y = (attr0 & 0xff) as i16;
        Ok(Self {
            x: if raw_x >= 256 { raw_x - 512 } else { raw_x },
            y: if raw_y >= 128 { raw_y - 256 } else { raw_y },
            width,
            height,
            tile: attr2 & 0x3ff,
            palette_bank: (attr2 >> 12) as u8,
            priority: ((attr2 >> 10) & 3) as u8,
            flip_x: attr1 & 0x1000 != 0,
            flip_y: attr1 & 0x2000 != 0,
            eight_bit: attr0 & 0x2000 != 0,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedSprite {
    pub image: graphics::RgbaImage,
    /// Native image-local attachment coordinate, after caller-facing flips.
    pub anchor_x: i16,
    pub anchor_y: i16,
}

fn compose(
    dimensions: &FrameDimensions,
    objects: &[OamObject],
    player: &SpritePlayer,
) -> Result<Option<RenderedSprite>> {
    let width = usize::from(dimensions.width);
    let height = usize::from(dimensions.height);
    let mut pixels = vec![[0; 4]; width * height];
    let global_x = player.flip_x ^ (dimensions.flip & 1 != 0);
    let global_y = player.flip_y ^ (dimensions.flip & 2 != 0);
    // Smaller OBJ priority wins; lower OAM index wins a same-priority overlap.
    // DisplaySprite ORs the caller priority into each template's priority bits.
    let mut order: Vec<usize> = (0..objects.len()).collect();
    order.sort_by_key(|&index| {
        (
            std::cmp::Reverse(objects[index].priority | player.priority),
            std::cmp::Reverse(index),
        )
    });
    for index in order {
        let object = &objects[index];
        let x = if global_x {
            i32::from(dimensions.width) - i32::from(object.width) - i32::from(object.x)
        } else {
            i32::from(object.x)
        };
        let y = if global_y {
            i32::from(dimensions.height) - i32::from(object.height) - i32::from(object.y)
        } else {
            i32::from(object.y)
        };
        let flip_x = object.flip_x ^ global_x;
        let flip_y = object.flip_y ^ global_y;
        let tile_bytes = if object.eight_bit { 64 } else { 32 };
        // DisplaySprite doubles the source OAM tile number in 8bpp mode before
        // adding the VRAM destination, so this source index is in whole tiles.
        let base = usize::from(object.tile) * tile_bytes;
        let count = usize::from(object.width / 8) * usize::from(object.height / 8);
        let tiles = binary::range(&player.object_tiles, base, count * tile_bytes)?;
        let bank = if object.eight_bit {
            0
        } else {
            usize::from((object.palette_bank + player.palette_bank_offset) & 15) * 16
        };
        for local_y in 0..usize::from(object.height) {
            for local_x in 0..usize::from(object.width) {
                let output_x = x + local_x as i32;
                let output_y = y + local_y as i32;
                if output_x < 0
                    || output_y < 0
                    || output_x >= width as i32
                    || output_y >= height as i32
                {
                    continue;
                }
                let source_x = if flip_x {
                    usize::from(object.width) - 1 - local_x
                } else {
                    local_x
                };
                let source_y = if flip_y {
                    usize::from(object.height) - 1 - local_y
                } else {
                    local_y
                };
                let tile = (source_y / 8) * usize::from(object.width / 8) + source_x / 8;
                let in_tile = (source_y % 8) * 8 + source_x % 8;
                let color = if object.eight_bit {
                    tiles[tile * 64 + in_tile]
                } else {
                    let packed = tiles[tile * 32 + in_tile / 2];
                    if in_tile % 2 == 0 {
                        packed & 15
                    } else {
                        packed >> 4
                    }
                };
                if color != 0 {
                    pixels[output_y as usize * width + output_x as usize] =
                        player.palette[bank + usize::from(color)];
                }
            }
        }
    }
    let anchor_x = if player.flip_x {
        i32::from(dimensions.width) - i32::from(dimensions.anchor_x)
    } else {
        i32::from(dimensions.anchor_x)
    };
    let anchor_y = if player.flip_y {
        i32::from(dimensions.height) - i32::from(dimensions.anchor_y)
    } else {
        i32::from(dimensions.anchor_y)
    };
    Ok(Some(RenderedSprite {
        image: graphics::RgbaImage {
            width,
            height,
            pixels,
        },
        anchor_x: i16::try_from(anchor_x).map_err(|_| Error("sprite anchor overflow".into()))?,
        anchor_y: i16::try_from(anchor_y).map_err(|_| Error("sprite anchor overflow".into()))?,
    }))
}
