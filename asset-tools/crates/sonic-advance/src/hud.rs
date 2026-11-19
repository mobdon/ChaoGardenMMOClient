//! Native ASOE ring counter pixels and the selected SA1 HUD layout rules.
//!
//! This owns no ring balance or clock. The caller supplies its authoritative
//! count and an explicit zero-ring warning phase. Source-reference placement,
//! decimal saturation and warning cadence are represented without emulating
//! native OAM scheduling, draw priority, gameplay or the complete stage HUD.
use crate::{binary, graphics, tables, Error, Result};

pub const FONT_OFFSET: usize = 0x658078;
pub const PALETTE_OFFSET: usize = 0x65a078;
pub const NORMAL_DIGITS_OFFSET: usize = 0x658478;
pub const RED_DIGITS_OFFSET: usize = 0x659c78;
pub const RING_ICON_OFFSET: usize = 0x65a318;
pub const DIGIT_BANK_BYTES: usize = 22 * 32;
pub const RING_ICON_BYTES: usize = 16 * 32;
pub const MAX_FRAME_PIXELS: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CounterLayout {
    /// Hundreds, tens, ones. Only the trailing `digit_count` entries are drawn.
    pub digits: [u8; 3],
    pub digit_count: u8,
    pub origin_x: i32,
    pub origin_y: i32,
    pub warning_red: bool,
}

pub fn counter_layout(rings: u32, zero_phase: u8) -> Result<CounterLayout> {
    if zero_phase >= 32 {
        return Err(Error("zero-ring HUD phase must be below 32".into()));
    }
    let value = rings.min(999);
    let (digit_count, origin_x) = if value < 10 {
        (1, 9)
    } else if value < 100 {
        (2, 5)
    } else {
        (3, 1)
    };
    Ok(CounterLayout {
        digits: [
            (value / 100) as u8,
            (value / 10 % 10) as u8,
            (value % 10) as u8,
        ],
        digit_count,
        origin_x,
        origin_y: 5,
        warning_red: value == 0 && (8..24).contains(&zero_phase),
    })
}

/// Native warning state advances only on zero-ring HUD ticks. Call this after
/// displaying the current phase. Rendering twice must not advance simulation.
pub fn advance_zero_phase(phase: u8, rings: u32, ticks: u32) -> Result<u8> {
    if phase >= 32 {
        return Err(Error("zero-ring HUD phase must be below 32".into()));
    }
    Ok(if rings == 0 {
        ((u32::from(phase) + ticks % 32) % 32) as u8
    } else {
        phase
    })
}

#[derive(Debug, Clone)]
struct Image {
    width: usize,
    height: usize,
    rgba: Vec<[u8; 4]>,
}

#[derive(Debug, Clone)]
pub struct Hud {
    normal: Vec<Image>,
    red: Vec<Image>,
    ring: Image,
}

fn image(
    indices: &[u8],
    first_tile: usize,
    width: usize,
    height: usize,
    palette: &[[u8; 4]],
) -> Result<Image> {
    let mut rgba = Vec::new();
    rgba.try_reserve_exact(width * height)
        .map_err(|_| Error("HUD image allocation failed".into()))?;
    for y in 0..height {
        for x in 0..width {
            let tile = first_tile + (y / 8) * (width / 8) + x / 8;
            let index = *indices
                .get(tile * 64 + (y % 8) * 8 + x % 8)
                .ok_or_else(|| Error("HUD tile outside source bank".into()))?;
            let mut color = *palette
                .get(usize::from(index))
                .ok_or_else(|| Error("HUD pixel outside source palette".into()))?;
            if index == 0 {
                color[3] = 0;
            }
            rgba.push(color);
        }
    }
    Ok(Image {
        width,
        height,
        rgba,
    })
}

fn blit(output: &mut [u8], width: usize, height: usize, image: &Image, x: i32, y: i32) {
    for sy in 0..image.height {
        for sx in 0..image.width {
            let px = x + sx as i32;
            let py = y + sy as i32;
            if px < 0 || py < 0 || px as usize >= width || py as usize >= height {
                continue;
            }
            let color = image.rgba[sy * image.width + sx];
            if color[3] != 0 {
                let at = ((py as usize) * width + px as usize) * 4;
                output[at..at + 4].copy_from_slice(&color);
            }
        }
    }
}

impl Hud {
    pub fn load_asoe(rom: &[u8]) -> Result<Self> {
        binary::verify_asoe(rom)?;
        let rows = tables::ui_graphics(
            rom,
            tables::UI_TABLE_OFFSET,
            tables::UI_TABLE_COUNT,
            tables::UI_TABLE_COUNT,
        )?;
        if rows[0].tile_offset != FONT_OFFSET
            || rows[0].palette_offset != PALETTE_OFFSET
            || rows[5].tile_offset != RING_ICON_OFFSET
        {
            return Err(Error("ASOE HUD table roots differ".into()));
        }
        Self::decode(
            binary::range(rom, NORMAL_DIGITS_OFFSET, DIGIT_BANK_BYTES)?,
            binary::range(rom, RED_DIGITS_OFFSET, DIGIT_BANK_BYTES)?,
            binary::range(rom, RING_ICON_OFFSET, RING_ICON_BYTES)?,
            binary::range(rom, PALETTE_OFFSET, 32)?,
        )
    }

    /// Decode exactly eleven 8x16 normal/red glyphs and one 32x32 icon.
    /// Exposed separately for synthetic fixtures; no game identity is inferred.
    pub fn decode(
        normal_digits: &[u8],
        red_digits: &[u8],
        ring_icon: &[u8],
        palette: &[u8],
    ) -> Result<Self> {
        if normal_digits.len() != DIGIT_BANK_BYTES
            || red_digits.len() != DIGIT_BANK_BYTES
            || ring_icon.len() != RING_ICON_BYTES
            || palette.len() != 32
        {
            return Err(Error("HUD source bank lengths differ from layout".into()));
        }
        let palette = graphics::decode_palette(palette, 16)?;
        let normal_indices = graphics::decode_tile_indices(
            normal_digits,
            graphics::BitsPerPixel::Four,
            DIGIT_BANK_BYTES * 2,
        )?;
        let red_indices = graphics::decode_tile_indices(
            red_digits,
            graphics::BitsPerPixel::Four,
            DIGIT_BANK_BYTES * 2,
        )?;
        let ring_indices = graphics::decode_tile_indices(
            ring_icon,
            graphics::BitsPerPixel::Four,
            RING_ICON_BYTES * 2,
        )?;
        let mut normal = Vec::new();
        let mut red = Vec::new();
        normal
            .try_reserve_exact(11)
            .map_err(|_| Error("HUD glyph allocation failed".into()))?;
        red.try_reserve_exact(11)
            .map_err(|_| Error("HUD glyph allocation failed".into()))?;
        for glyph in 0..11 {
            normal.push(image(&normal_indices, glyph * 2, 8, 16, &palette)?);
            red.push(image(&red_indices, glyph * 2, 8, 16, &palette)?);
        }
        Ok(Self {
            normal,
            red,
            ring: image(&ring_indices, 0, 32, 32, &palette)?,
        })
    }

    /// Composite the reference ring-counter element at native screen coordinates.
    /// Count and warning phase are supplied by the caller; no state is awarded.
    /// Source-index-zero pixels retain the destination beneath the overlay.
    pub fn render_into(
        &self,
        output: &mut [u8],
        width: usize,
        height: usize,
        rings: u32,
        zero_phase: u8,
    ) -> Result<()> {
        let layout = counter_layout(rings, zero_phase)?;
        let pixels = width
            .checked_mul(height)
            .filter(|pixels| *pixels > 0 && *pixels <= MAX_FRAME_PIXELS)
            .ok_or_else(|| Error("HUD framebuffer exceeds pixel budget".into()))?;
        if pixels.checked_mul(4) != Some(output.len()) {
            return Err(Error(
                "HUD framebuffer length differs from dimensions".into(),
            ));
        }
        blit(output, width, height, &self.ring, -2, -1);
        let glyphs = if layout.warning_red {
            &self.red
        } else {
            &self.normal
        };
        let first_digit = 3 - usize::from(layout.digit_count);
        for (index, digit) in layout.digits[first_digit..].iter().enumerate() {
            blit(
                output,
                width,
                height,
                &glyphs[usize::from(*digit)],
                layout.origin_x + index as i32 * 8,
                layout.origin_y,
            );
        }
        Ok(())
    }
}
